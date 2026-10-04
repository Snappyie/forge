use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::error::{DomainError, ErrorClass};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BackoffStrategy {
    Fixed {
        delay: Duration,
    },
    Linear {
        initial_delay: Duration,
        increment: Duration,
    },
    Exponential {
        initial_delay: Duration,
        multiplier: u32,
        max_delay: Duration,
    },
}

/// Injected randomness, so jitter is deterministic under test (spec 04.10).
pub trait JitterSource {
    /// Returns a value in `[0.0, 1.0)`.
    fn next_fraction(&mut self) -> f64;
}

/// Production jitter. `rand` is not a dependency of the domain crate, which
/// spec 04.4 requires to stay free of infrastructure; the executor supplies a
/// crypto-quality implementation instead.
#[derive(Debug, Default)]
pub struct NoJitter;

impl JitterSource for NoJitter {
    fn next_fraction(&mut self) -> f64 {
        0.0
    }
}

/// `Eq` is intentionally not derived: `jitter_ratio` is an `f64`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: BackoffStrategy,
    /// Error classes eligible for retry. Empty means "use the class defaults".
    pub retryable_error_classes: Vec<String>,
    /// Classes explicitly excluded from retry, overriding the allow-list.
    pub non_retryable_error_classes: Vec<String>,
    /// Fraction of the computed delay that jitter may add, in `0.0..=1.0`.
    pub jitter_ratio: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: BackoffStrategy::Exponential {
                initial_delay: Duration::from_secs(1),
                multiplier: 2,
                max_delay: Duration::from_secs(3600),
            },
            retryable_error_classes: vec![],
            non_retryable_error_classes: vec![],
            jitter_ratio: 0.0,
        }
    }
}

impl RetryPolicy {
    /// Delay before attempt number `attempt + 1`, where `attempt` is the
    /// 1-based number of the attempt that just failed.
    ///
    /// Spec 10.9: exponential backoff is
    /// `min(max_delay, initial_delay * multiplier^(attempt-1))`. The
    /// exponentiation is saturating so a large attempt number or multiplier
    /// clamps at `max_delay` instead of overflowing.
    pub fn delay_for(&self, attempt: u32, jitter: &mut dyn JitterSource) -> Duration {
        let base = self.base_delay_for(attempt);
        self.apply_jitter(base, jitter)
    }

    /// The un-jittered delay for the next attempt.
    pub fn base_delay_for(&self, attempt: u32) -> Duration {
        match &self.backoff {
            BackoffStrategy::Fixed { delay } => *delay,
            BackoffStrategy::Linear {
                initial_delay,
                increment,
            } => {
                // `initial_delay + increment * (attempt - 1)`: the first
                // retry waits exactly `initial_delay`, and each subsequent
                // retry adds one more increment.
                let steps = attempt.max(1) - 1;
                initial_delay
                    .checked_add(increment.checked_mul(steps).unwrap_or(Duration::MAX))
                    .unwrap_or(Duration::MAX)
            }
            BackoffStrategy::Exponential {
                initial_delay,
                multiplier,
                max_delay,
            } => {
                // Saturating exponentiation. `pow` on u64 overflows long
                // before `Duration` does, so clamp the exponent to keep the
                // intermediate in range.
                let exp = attempt.saturating_sub(1).min(32);
                let factor = (*multiplier as u64).saturating_pow(exp);
                let millis = (initial_delay.as_millis() as u64).saturating_mul(factor);
                Duration::from_millis(millis).min(*max_delay)
            }
        }
    }

    /// Applies bounded jitter: the returned delay never exceeds
    /// `base * (1 + jitter_ratio)`.
    fn apply_jitter(&self, base: Duration, jitter: &mut dyn JitterSource) -> Duration {
        if self.jitter_ratio <= 0.0 {
            return base;
        }
        let fraction = jitter.next_fraction().clamp(0.0, 1.0);
        // Work in whole milliseconds. Going through `from_secs_f64` makes
        // exact values like 50.0s drift by a few hundred nanoseconds, so a
        // bounded-jitter result could exceed its own documented ceiling by a
        // rounding artefact. Truncate to whole millis so the documented
        // bound `base * (1 + jitter_ratio)` holds exactly.
        let extra_ms = (base.as_millis() as f64) * self.jitter_ratio * fraction;
        if !extra_ms.is_finite() || extra_ms >= Duration::MAX.as_millis() as f64 {
            return Duration::MAX;
        }
        match base.checked_add(Duration::from_millis(extra_ms as u64)) {
            Some(total) => total,
            None => Duration::MAX,
        }
    }

    /// Whether a failure in `class` may be retried after `attempts_made`
    /// attempts.
    ///
    /// Cancellation is never retryable (spec 10.8 / AT-RETRY-007): a
    /// cancellation must not silently turn into another execution.
    pub fn should_retry(&self, class: ErrorClass, attempts_made: u32) -> bool {
        if class == ErrorClass::Cancellation {
            return false;
        }
        if attempts_made >= self.max_attempts {
            return false;
        }
        if self
            .non_retryable_error_classes
            .iter()
            .any(|c| c.eq_ignore_ascii_case(class.as_str()))
        {
            return false;
        }
        if self
            .retryable_error_classes
            .iter()
            .any(|c| c == "*" || c.eq_ignore_ascii_case(class.as_str()))
        {
            return true;
        }
        if !self.retryable_error_classes.is_empty() {
            // A non-empty allow-list that does not name this class is a
            // deliberate exclusion.
            return false;
        }
        class.is_retryable_by_default()
    }

    /// Validates the policy itself.
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.max_attempts == 0 {
            return Err(DomainError::ValidationError(
                "retry policy must allow at least one attempt".to_string(),
            ));
        }
        if !(0.0..=1.0).contains(&self.jitter_ratio) {
            return Err(DomainError::ValidationError(
                "jitter_ratio must be between 0.0 and 1.0".to_string(),
            ));
        }
        if let BackoffStrategy::Exponential { multiplier, .. } = &self.backoff {
            if *multiplier == 0 {
                return Err(DomainError::ValidationError(
                    "exponential backoff multiplier must be at least 1".to_string(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConcurrencyLimit {
    Unlimited,
    Bounded(u32),
}

impl ConcurrencyLimit {
    pub fn limit(&self) -> Option<u32> {
        match self {
            ConcurrencyLimit::Unlimited => None,
            ConcurrencyLimit::Bounded(n) => Some(*n),
        }
    }

    /// Whether `active` already-scheduled executions leave room for one more.
    pub fn admits(&self, active: u32) -> bool {
        match self.limit() {
            None => true,
            Some(max) => active < max,
        }
    }
}

/// Scope a concurrency limit applies to (spec 01.12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConcurrencyScope {
    Job,
    Workflow,
    Queue,
    Tenant,
    Global,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ConcurrencyPolicy {
    pub max_concurrent_executions: ConcurrencyLimit,
    pub scope: ConcurrencyScope,
    pub queue_id: Option<String>,
}

impl Default for ConcurrencyPolicy {
    fn default() -> Self {
        Self {
            max_concurrent_executions: ConcurrencyLimit::Unlimited,
            scope: ConcurrencyScope::Job,
            queue_id: None,
        }
    }
}

impl ConcurrencyPolicy {
    /// Whether a new execution may start given the currently-active count for
    /// the configured scope.
    pub fn admits(&self, active_count: u32) -> bool {
        self.max_concurrent_executions.admits(active_count)
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        if let ConcurrencyLimit::Bounded(0) = self.max_concurrent_executions {
            return Err(DomainError::ValidationError(
                "a bounded concurrency limit must be at least 1".to_string(),
            ));
        }
        Ok(())
    }
}

/// Named priority levels mapped to integers per spec 02.13.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Priority {
    Critical,
    High,
    #[default]
    Normal,
    Low,
    Background,
}

impl Priority {
    pub fn weight(&self) -> i64 {
        match self {
            Priority::Critical => 1000,
            Priority::High => 750,
            Priority::Normal => 500,
            Priority::Low => 250,
            Priority::Background => 100,
        }
    }

    /// Effective ordering score, combining priority with an aging term.
    ///
    /// Spec 02.13 requires that priority MUST NOT create unbounded starvation
    /// and that an aging mechanism SHOULD be available. A queued execution
    /// gains one point per second waited, so a long-waiting low-priority item
    /// eventually outranks a freshly submitted high-priority one.
    pub fn effective_score(priority: Priority, waited: Duration) -> i64 {
        let aging = waited.as_secs() as i64;
        priority.weight() + aging
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Critical => "CRITICAL",
            Priority::High => "HIGH",
            Priority::Normal => "NORMAL",
            Priority::Low => "LOW",
            Priority::Background => "BACKGROUND",
        }
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exp(initial_secs: u64, multiplier: u32, max_secs: u64) -> BackoffStrategy {
        BackoffStrategy::Exponential {
            initial_delay: Duration::from_secs(initial_secs),
            multiplier,
            max_delay: Duration::from_secs(max_secs),
        }
    }

    // AT-RETRY-001: fixed delay
    #[test]
    fn fixed_backoff_is_constant() {
        let policy = RetryPolicy {
            backoff: BackoffStrategy::Fixed {
                delay: Duration::from_secs(30),
            },
            ..Default::default()
        };
        for attempt in 1..=5 {
            assert_eq!(policy.base_delay_for(attempt), Duration::from_secs(30));
        }
    }

    // AT-RETRY-002: exponential delay
    #[test]
    fn exponential_backoff_doubles_per_attempt() {
        let policy = RetryPolicy {
            backoff: exp(1, 2, 3600),
            ..Default::default()
        };
        assert_eq!(policy.base_delay_for(1), Duration::from_secs(1));
        assert_eq!(policy.base_delay_for(2), Duration::from_secs(2));
        assert_eq!(policy.base_delay_for(3), Duration::from_secs(4));
        assert_eq!(policy.base_delay_for(4), Duration::from_secs(8));
    }

    #[test]
    fn linear_backoff_increments() {
        let policy = RetryPolicy {
            backoff: BackoffStrategy::Linear {
                initial_delay: Duration::from_secs(5),
                increment: Duration::from_secs(5),
            },
            ..Default::default()
        };
        assert_eq!(policy.base_delay_for(1), Duration::from_secs(5));
        assert_eq!(policy.base_delay_for(3), Duration::from_secs(15));
    }

    // AT-RETRY-003: max delay is respected
    #[test]
    fn exponential_backoff_clamps_to_max_delay() {
        let policy = RetryPolicy {
            backoff: exp(1, 10, 60),
            ..Default::default()
        };
        assert_eq!(policy.base_delay_for(1), Duration::from_secs(1));
        assert_eq!(policy.base_delay_for(2), Duration::from_secs(10));
        // 1 * 10^3 = 1000s, clamped to the 60s ceiling.
        assert_eq!(policy.base_delay_for(4), Duration::from_secs(60));
        assert_eq!(policy.base_delay_for(30), Duration::from_secs(60));
    }

    /// Spec 10.9: overflow MUST be prevented.
    #[test]
    fn exponential_backoff_does_not_overflow() {
        let policy = RetryPolicy {
            backoff: exp(1, u32::MAX, u64::MAX),
            ..Default::default()
        };
        for attempt in [1_u32, 2, 10, 100, 10_000, u32::MAX] {
            let delay = policy.base_delay_for(attempt);
            // A sane bound: it never exceeds max_delay.
            assert!(delay <= Duration::from_secs(u64::MAX), "attempt {attempt}");
        }
    }

    // AT-RETRY-004: jitter is bounded
    #[test]
    fn jitter_is_bounded_above_and_below() {
        struct Fixed(f64);
        impl JitterSource for Fixed {
            fn next_fraction(&mut self) -> f64 {
                self.0
            }
        }

        let policy = RetryPolicy {
            backoff: BackoffStrategy::Fixed {
                delay: Duration::from_secs(100),
            },
            jitter_ratio: 0.5,
            ..Default::default()
        };

        // No jitter added at fraction 0.
        assert_eq!(
            policy.delay_for(1, &mut Fixed(0.0)),
            Duration::from_secs(100)
        );
        // Full jitter added at fraction 1.
        assert_eq!(
            policy.delay_for(1, &mut Fixed(1.0)),
            Duration::from_secs(150)
        );
        // Never negative even if the source misbehaves.
        assert_eq!(
            policy.delay_for(1, &mut Fixed(-5.0)),
            Duration::from_secs(100)
        );
    }

    #[test]
    fn no_jitter_is_deterministic() {
        let policy = RetryPolicy {
            jitter_ratio: 1.0,
            ..Default::default()
        };
        assert_eq!(policy.delay_for(1, &mut NoJitter), policy.base_delay_for(1));
    }

    // AT-RETRY-005: a non-retryable error is not retried
    #[test]
    fn non_retryable_class_is_not_retried() {
        let policy = RetryPolicy {
            max_attempts: 5,
            retryable_error_classes: vec!["*".to_string()],
            ..Default::default()
        };
        // An explicit deny-list wins even against a wildcard allow-list.
        let policy = RetryPolicy {
            non_retryable_error_classes: vec!["VALIDATION".to_string()],
            ..policy
        };
        assert!(!policy.should_retry(ErrorClass::Validation, 1));
        assert!(policy.should_retry(ErrorClass::Transient, 1));
    }

    #[test]
    fn allow_list_excludes_unlisted_classes() {
        let policy = RetryPolicy {
            retryable_error_classes: vec!["TIMEOUT".to_string()],
            ..Default::default()
        };
        assert!(policy.should_retry(ErrorClass::Timeout, 1));
        assert!(!policy.should_retry(ErrorClass::Transient, 1));
    }

    #[test]
    fn empty_policy_falls_back_to_class_defaults() {
        let policy = RetryPolicy::default();
        assert!(policy.should_retry(ErrorClass::Transient, 1));
        assert!(policy.should_retry(ErrorClass::Timeout, 1));
        assert!(!policy.should_retry(ErrorClass::Permanent, 1));
        assert!(!policy.should_retry(ErrorClass::Validation, 1));
    }

    // AT-RETRY-006: max attempts is enforced
    #[test]
    fn max_attempts_is_enforced() {
        let policy = RetryPolicy {
            max_attempts: 3,
            ..Default::default()
        };
        assert!(policy.should_retry(ErrorClass::Transient, 1));
        assert!(policy.should_retry(ErrorClass::Transient, 2));
        // The third failure exhausts the budget.
        assert!(!policy.should_retry(ErrorClass::Transient, 3));
        assert!(!policy.should_retry(ErrorClass::Transient, 4));
    }

    // AT-RETRY-007: cancellation does not become a retry
    #[test]
    fn cancellation_is_never_retried() {
        let policy = RetryPolicy {
            max_attempts: 10,
            retryable_error_classes: vec!["*".to_string()],
            jitter_ratio: 1.0,
            ..Default::default()
        };
        assert!(!policy.should_retry(ErrorClass::Cancellation, 1));
    }

    #[test]
    fn invalid_policies_are_rejected() {
        assert!(RetryPolicy {
            max_attempts: 0,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(RetryPolicy {
            jitter_ratio: 2.0,
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(RetryPolicy {
            backoff: exp(1, 0, 60),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(RetryPolicy::default().validate().is_ok());
    }

    #[test]
    fn concurrency_limits_admit_correctly() {
        assert!(ConcurrencyLimit::Unlimited.admits(u32::MAX));
        assert!(ConcurrencyLimit::Bounded(2).admits(0));
        assert!(ConcurrencyLimit::Bounded(2).admits(1));
        assert!(!ConcurrencyLimit::Bounded(2).admits(2));
        assert!(!ConcurrencyLimit::Bounded(2).admits(3));
    }

    #[test]
    fn zero_bounded_concurrency_is_invalid() {
        assert!(ConcurrencyPolicy {
            max_concurrent_executions: ConcurrencyLimit::Bounded(0),
            ..Default::default()
        }
        .validate()
        .is_err());
    }

    /// Spec 02.13: priority must not starve lower-priority work.
    #[test]
    fn priority_aging_prevents_starvation() {
        // Priority dominates ordering between equally-aged items.
        let critical = Priority::effective_score(Priority::Critical, Duration::from_secs(0));
        let high = Priority::effective_score(Priority::High, Duration::from_secs(0));
        let normal = Priority::effective_score(Priority::Normal, Duration::from_secs(0));
        let low = Priority::effective_score(Priority::Low, Duration::from_secs(0));
        assert!(critical > high && high > normal && normal > low);

        // A BACKGROUND item queued long enough outranks a fresh CRITICAL.
        // This is the anti-starvation property: nothing waits forever.
        let aged_background =
            Priority::effective_score(Priority::Background, Duration::from_secs(1_000));
        let fresh_critical = Priority::effective_score(Priority::Critical, Duration::from_secs(0));
        assert!(
            aged_background > fresh_critical,
            "aging ({aged_background}) must eventually overtake a fresh CRITICAL ({fresh_critical})"
        );

        // And ordering is monotonic in wait time for every level.
        let earlier = Priority::effective_score(Priority::Low, Duration::from_secs(10));
        let later = Priority::effective_score(Priority::Low, Duration::from_secs(20));
        assert!(later > earlier);
    }

    #[test]
    fn priority_weights_match_spec() {
        assert_eq!(Priority::Critical.weight(), 1000);
        assert_eq!(Priority::High.weight(), 750);
        assert_eq!(Priority::Normal.weight(), 500);
        assert_eq!(Priority::Low.weight(), 250);
        assert_eq!(Priority::Background.weight(), 100);
    }

    #[test]
    fn error_class_defaults_match_taxonomy() {
        assert!(ErrorClass::Transient.is_retryable_by_default());
        assert!(ErrorClass::Timeout.is_retryable_by_default());
        assert!(!ErrorClass::Permanent.is_retryable_by_default());
        assert!(!ErrorClass::Authorization.is_retryable_by_default());
        assert_eq!(ErrorClass::ResourceExhausted.as_str(), "RESOURCE_EXHAUSTED");
    }
}
