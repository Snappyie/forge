//! Deciding what happens after a failure (spec 10.8, spec 10.9, spec 10.7).
//!
//! The retry policy, the backoff arithmetic and the jitter bound all live in
//! `forge-domain::policy`, with unit tests. Nothing consulted them: a failure
//! went straight to `FAILED`, and `RETRY_SCHEDULED` was requeued on a fixed
//! five-second timer that ignored the policy entirely. This module is the
//! missing caller.
//!
//! It is a deliberate seam: the worker protocol, the timeout sweeper and the
//! lease reaper all reach the same decision through [`FailureHandler`], so a
//! retry cannot behave one way when a worker reports a failure and another way
//! when the server times the work out.

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use forge_domain::policy::{JitterSource, RetryPolicy};
use forge_domain::{ErrorClass, ExecutionStatus, TenantId};
use sqlx::PgPool;
use uuid::Uuid;

use forge_storage::ExecutionRepository;

/// What happened to a failed execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureOutcome {
    /// A retry was scheduled; the execution will become eligible at `retry_at`.
    RetryScheduled {
        attempt: u32,
        retry_at: DateTime<Utc>,
    },
    /// The policy refused a retry, so the execution is terminally failed.
    Failed { attempt: u32 },
    /// No policy was found (the version was deleted), so the failure stands.
    FailedWithoutPolicy,
}

/// Deterministic jitter derived from the execution's own identity.
///
/// Spec 10.9 requires *bounded* jitter, and spec 04.4 keeps infrastructure out
/// of the domain. Deriving the fraction from the execution id means two
/// executions that failed at the same moment do not retry in lockstep, while a
/// single execution's delay is reproducible — which is what makes the behaviour
/// testable at all.
struct IdJitter {
    state: u64,
}

impl IdJitter {
    fn new(execution_id: Uuid, attempt: u32) -> Self {
        let (high, low) = execution_id.as_u64_pair();
        Self {
            state: high
                ^ low.rotate_left(17)
                ^ u64::from(attempt).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        }
    }
}

impl JitterSource for IdJitter {
    fn next_fraction(&mut self) -> f64 {
        // splitmix64: cheap, well-distributed, and dependency-free.
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // 53 significant bits is exactly what an f64 can hold.
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Applies a version's retry policy to a failed execution.
pub struct FailureHandler<'a> {
    pool: &'a PgPool,
}

impl<'a> FailureHandler<'a> {
    pub fn new(pool: &'a PgPool) -> Self {
        Self { pool }
    }

    /// Loads the retry policy attached to the version an execution ran.
    pub async fn policy_for(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
    ) -> Result<Option<RetryPolicy>, sqlx::Error> {
        let raw: Option<(Option<serde_json::Value>,)> = sqlx::query_as(
            "SELECT v.retry_policy
             FROM executions e
             LEFT JOIN job_versions v ON v.id = e.job_version_id
             WHERE e.id = $1 AND e.tenant_id = $2",
        )
        .bind(execution_id)
        .bind(tenant_id.into_uuid())
        .fetch_optional(self.pool)
        .await?;

        let Some((raw,)) = raw else {
            return Ok(None);
        };

        // A version row with a malformed policy, or a workflow run with no
        // version at all, both mean "no policy": the failure stands rather than
        // being retried under semantics nobody configured.
        Ok(raw.and_then(|value| serde_json::from_value::<RetryPolicy>(value).ok()))
    }

    /// Records a failure against the execution's retry policy.
    ///
    /// `attempts_made` is the number of attempts already completed, including
    /// the one that just failed.
    ///
    /// The execution is always moved to `FAILED` first, because that is the only
    /// state the machine permits a retry to leave (spec 02.6). A crash between
    /// the two writes therefore leaves the execution terminally failed, which is
    /// safe: the alternative — scheduling a retry that was never justified —
    /// could duplicate work.
    pub async fn record_failure(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        class: ErrorClass,
        error_message: &str,
        attempts_made: u32,
    ) -> Result<FailureOutcome, String> {
        let policy = match self.policy_for(tenant_id, execution_id).await {
            Ok(policy) => policy,
            Err(error) => return Err(error.to_string()),
        };

        self.fail(tenant_id, execution_id, class, error_message)
            .await?;

        let Some(policy) = policy else {
            return Ok(FailureOutcome::FailedWithoutPolicy);
        };

        if !policy.should_retry(class, attempts_made) {
            return Ok(FailureOutcome::Failed {
                attempt: attempts_made,
            });
        }

        let mut jitter = IdJitter::new(execution_id, attempts_made);
        let delay = policy.delay_for(attempts_made, &mut jitter);
        let retry_at = Utc::now()
            + ChronoDuration::from_std(delay).unwrap_or_else(|_| ChronoDuration::seconds(1));

        ExecutionRepository::new(self.pool)
            .schedule_retry(tenant_id, execution_id, retry_at)
            .await
            .map_err(|error| error.to_string())?;

        Ok(FailureOutcome::RetryScheduled {
            attempt: attempts_made,
            retry_at,
        })
    }

    /// Records a timeout and applies the same retry policy a reported failure
    /// would get (spec 10.7, spec 10.8).
    ///
    /// The attempt is marked `TIMED_OUT` rather than `FAILED`: that is what the
    /// state machine permits out of `RUNNING`/`DISPATCHED`, and it is the
    /// distinction operators need — "the work overshot its ceiling" is not the
    /// same fact as "the work reported an error".
    pub async fn record_timeout(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        attempts_made: u32,
        error_message: &str,
    ) -> Result<FailureOutcome, String> {
        let policy = match self.policy_for(tenant_id, execution_id).await {
            Ok(policy) => policy,
            Err(error) => return Err(error.to_string()),
        };

        ExecutionRepository::new(self.pool)
            .transition(
                tenant_id,
                execution_id,
                ExecutionStatus::TimedOut,
                Some(ErrorClass::Timeout),
                Some(error_message),
            )
            .await
            .map_err(|error| error.to_string())?;

        let Some(policy) = policy else {
            return Ok(FailureOutcome::FailedWithoutPolicy);
        };

        if !policy.should_retry(ErrorClass::Timeout, attempts_made) {
            return Ok(FailureOutcome::Failed {
                attempt: attempts_made,
            });
        }

        let mut jitter = IdJitter::new(execution_id, attempts_made);
        let delay = policy.delay_for(attempts_made, &mut jitter);
        let retry_at = Utc::now()
            + ChronoDuration::from_std(delay).unwrap_or_else(|_| ChronoDuration::seconds(1));

        ExecutionRepository::new(self.pool)
            .schedule_retry(tenant_id, execution_id, retry_at)
            .await
            .map_err(|error| error.to_string())?;

        Ok(FailureOutcome::RetryScheduled {
            attempt: attempts_made,
            retry_at,
        })
    }

    /// Marks the execution terminally failed with its classified reason.
    async fn fail(
        &self,
        tenant_id: TenantId,
        execution_id: Uuid,
        class: ErrorClass,
        error_message: &str,
    ) -> Result<(), String> {
        ExecutionRepository::new(self.pool)
            .transition(
                tenant_id,
                execution_id,
                ExecutionStatus::Failed,
                Some(class),
                Some(error_message),
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::policy::{BackoffStrategy, RetryPolicy};
    use std::time::Duration;

    fn policy(max_attempts: u32, jitter_ratio: f64) -> RetryPolicy {
        RetryPolicy {
            max_attempts,
            backoff: BackoffStrategy::Exponential {
                initial_delay: Duration::from_secs(10),
                multiplier: 2,
                max_delay: Duration::from_secs(600),
            },
            retryable_error_classes: vec![],
            non_retryable_error_classes: vec![],
            jitter_ratio,
        }
    }

    /// The whole point of the seam: the domain policy now has a caller, and the
    /// delay it produces is the documented exponential series.
    #[test]
    fn the_delay_grows_exponentially() {
        let policy = policy(5, 0.0);
        let mut jitter = IdJitter::new(Uuid::nil(), 1);

        assert_eq!(policy.delay_for(1, &mut jitter), Duration::from_secs(10));
        assert_eq!(policy.delay_for(2, &mut jitter), Duration::from_secs(20));
        assert_eq!(policy.delay_for(3, &mut jitter), Duration::from_secs(40));
    }

    #[test]
    fn the_delay_is_capped_at_the_policy_maximum() {
        let policy = policy(50, 0.0);
        let mut jitter = IdJitter::new(Uuid::nil(), 1);
        assert_eq!(policy.delay_for(20, &mut jitter), Duration::from_secs(600));
    }

    #[test]
    fn jitter_stays_inside_its_documented_bound() {
        let policy = policy(5, 0.5);
        for attempt in 1..=5 {
            let mut jitter = IdJitter::new(Uuid::new_v4(), attempt);
            let delay = policy.delay_for(attempt, &mut jitter);
            let base = policy.base_delay_for(attempt);
            let ceiling = base + base.mul_f64(0.5);
            assert!(
                delay <= ceiling,
                "attempt {attempt}: {delay:?} exceeds the {ceiling:?} ceiling"
            );
            assert!(delay >= base, "jitter must never shorten a delay");
        }
    }

    #[test]
    fn the_same_execution_always_gets_the_same_jitter() {
        let id = Uuid::new_v4();
        let policy = policy(5, 0.4);
        let first = policy.delay_for(2, &mut IdJitter::new(id, 2));
        let second = policy.delay_for(2, &mut IdJitter::new(id, 2));
        assert_eq!(first, second);
    }

    #[test]
    fn different_executions_get_different_jitter() {
        let policy = policy(5, 0.4);
        let a = policy.delay_for(2, &mut IdJitter::new(Uuid::from_u128(1), 2));
        let b = policy.delay_for(2, &mut IdJitter::new(Uuid::from_u128(2), 2));
        assert_ne!(a, b, "jitter must decorrelate independent executions");
    }

    #[test]
    fn the_jitter_fraction_is_a_probability() {
        let mut jitter = IdJitter::new(Uuid::new_v4(), 3);
        for _ in 0..1000 {
            let fraction = jitter.next_fraction();
            assert!((0.0..1.0).contains(&fraction), "got {fraction}");
        }
    }

    /// A cancellation is never a retry, whatever the policy says (AT-RETRY-007).
    #[test]
    fn a_cancellation_is_never_retried() {
        let policy = policy(10, 0.0);
        assert!(!policy.should_retry(ErrorClass::Cancellation, 1));
    }

    #[test]
    fn attempts_beyond_the_maximum_are_not_retried() {
        let policy = policy(3, 0.0);
        assert!(policy.should_retry(ErrorClass::Transient, 2));
        assert!(!policy.should_retry(ErrorClass::Transient, 3));
    }

    #[test]
    fn a_permanent_error_is_not_retried_by_default() {
        let policy = policy(5, 0.0);
        assert!(!policy.should_retry(ErrorClass::Permanent, 1));
    }

    #[test]
    fn a_non_retryable_class_overrides_an_allow_list() {
        let mut policy = policy(5, 0.0);
        policy.retryable_error_classes = vec!["*".to_string()];
        policy.non_retryable_error_classes = vec!["VALIDATION".to_string()];
        assert!(policy.should_retry(ErrorClass::Transient, 1));
        assert!(!policy.should_retry(ErrorClass::Validation, 1));
    }
}
