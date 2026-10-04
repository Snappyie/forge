use crate::error::{DomainError, ErrorClass};
use crate::id::{ExecutionId, JobId, JobVersionId, TenantId, WorkerId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Execution lifecycle states from spec 02.5.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionStatus {
    /// Created by the scheduler for a future occurrence, not yet enqueued.
    Scheduled,
    Queued,
    Dispatched,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    CancelRequested,
    Cancelled,
    RetryScheduled,
    DeadLettered,
    /// Lease expired and recovery began. Intermediate, NOT terminal: spec 02.6
    /// requires `ABANDONED -> QUEUED` so the work can be re-executed.
    Abandoned,
}

impl ExecutionStatus {
    /// The five terminal states from spec 02.5.
    ///
    /// `Abandoned` is deliberately excluded. Treating it as terminal made the
    /// recovery transition unreachable, so a lost worker permanently stranded
    /// its execution.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Succeeded
                | ExecutionStatus::Failed
                | ExecutionStatus::Cancelled
                | ExecutionStatus::TimedOut
                | ExecutionStatus::DeadLettered
        )
    }

    /// Whether the execution still occupies a concurrency slot (spec 01.12).
    pub fn holds_concurrency_slot(&self) -> bool {
        !self.is_terminal()
    }

    /// Whether a worker could be running this execution right now.
    pub fn is_in_flight(&self) -> bool {
        matches!(
            self,
            ExecutionStatus::Dispatched | ExecutionStatus::Running | ExecutionStatus::Abandoned
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStatus::Scheduled => "SCHEDULED",
            ExecutionStatus::Queued => "QUEUED",
            ExecutionStatus::Dispatched => "DISPATCHED",
            ExecutionStatus::Running => "RUNNING",
            ExecutionStatus::Succeeded => "SUCCEEDED",
            ExecutionStatus::Failed => "FAILED",
            ExecutionStatus::TimedOut => "TIMED_OUT",
            ExecutionStatus::CancelRequested => "CANCEL_REQUESTED",
            ExecutionStatus::Cancelled => "CANCELLED",
            ExecutionStatus::RetryScheduled => "RETRY_SCHEDULED",
            ExecutionStatus::DeadLettered => "DEAD_LETTERED",
            ExecutionStatus::Abandoned => "ABANDONED",
        }
    }
}

impl std::fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for ExecutionStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "SCHEDULED" => Ok(ExecutionStatus::Scheduled),
            "QUEUED" => Ok(ExecutionStatus::Queued),
            "DISPATCHED" => Ok(ExecutionStatus::Dispatched),
            "RUNNING" => Ok(ExecutionStatus::Running),
            "SUCCEEDED" => Ok(ExecutionStatus::Succeeded),
            "FAILED" => Ok(ExecutionStatus::Failed),
            "TIMED_OUT" => Ok(ExecutionStatus::TimedOut),
            "CANCEL_REQUESTED" => Ok(ExecutionStatus::CancelRequested),
            "CANCELLED" => Ok(ExecutionStatus::Cancelled),
            "RETRY_SCHEDULED" => Ok(ExecutionStatus::RetryScheduled),
            "DEAD_LETTERED" => Ok(ExecutionStatus::DeadLettered),
            "ABANDONED" => Ok(ExecutionStatus::Abandoned),
            other => Err(DomainError::ValidationError(format!(
                "unknown execution status: {other}"
            ))),
        }
    }
}

/// What caused the execution to enter its current state. Useful for the
/// dispatch explanation required by spec 12 / AT-OBS-005.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TriggerSource {
    Schedule,
    #[default]
    Manual,
    Api,
    Workflow,
    Retry,
    Recovery,
}

impl TriggerSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerSource::Schedule => "SCHEDULE",
            TriggerSource::Manual => "MANUAL",
            TriggerSource::Api => "API",
            TriggerSource::Workflow => "WORKFLOW",
            TriggerSource::Retry => "RETRY",
            TriggerSource::Recovery => "RECOVERY",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub id: ExecutionId,
    pub tenant_id: TenantId,
    pub job_id: JobId,
    pub job_version_id: JobVersionId,
    pub status: ExecutionStatus,
    pub worker_id: Option<WorkerId>,
    pub attempt_count: u32,
    pub trigger_source: TriggerSource,
    pub correlation_id: Option<String>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub enqueued_at: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub error_class: Option<ErrorClass>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub metrics: BTreeMap<String, i64>,
}

impl Execution {
    pub fn new(tenant_id: TenantId, job_id: JobId, job_version_id: JobVersionId) -> Self {
        Self {
            id: ExecutionId::new(),
            tenant_id,
            job_id,
            job_version_id,
            status: ExecutionStatus::Queued,
            worker_id: None,
            attempt_count: 0,
            trigger_source: TriggerSource::default(),
            correlation_id: None,
            scheduled_for: None,
            enqueued_at: None,
            retry_at: None,
            error_class: None,
            error_message: None,
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            metrics: BTreeMap::new(),
        }
    }

    /// Applies a state transition, rejecting anything the spec 02.6 table does
    /// not allow.
    pub fn transition_to(&mut self, new_status: ExecutionStatus) -> Result<(), DomainError> {
        self.transition_to_at(new_status, Utc::now())
    }

    /// Transition with an explicit clock, so tests are deterministic.
    pub fn transition_to_at(
        &mut self,
        new_status: ExecutionStatus,
        now: DateTime<Utc>,
    ) -> Result<(), DomainError> {
        // Invariant 3 (spec 01.22): a terminal execution cannot transition back
        // to an active state.
        //
        // `FAILED` is terminal in the sense that the attempt is over, but spec
        // 02.6 still routes it to RETRY_SCHEDULED or DEAD_LETTERED. Those are
        // the only exits permitted out of a terminal state; everything else
        // (a SUCCEEDED run going back to RUNNING, say) stays forbidden.
        if self.status.is_terminal() {
            let permitted_exit = matches!(
                (self.status, new_status),
                (ExecutionStatus::Failed, ExecutionStatus::RetryScheduled)
                    | (ExecutionStatus::Failed, ExecutionStatus::DeadLettered)
                    | (ExecutionStatus::TimedOut, ExecutionStatus::RetryScheduled)
                    | (ExecutionStatus::TimedOut, ExecutionStatus::DeadLettered)
            );
            if !permitted_exit {
                return Err(DomainError::InvalidStateTransition {
                    from: self.status.to_string(),
                    to: new_status.to_string(),
                });
            }
        }

        // Order matters: the catch-all `(_, CancelRequested)` arm must come
        // LAST so the specific out-of-a-failure arms are reachable. Placed
        // earlier it shadowed them and made FAILED unreachable.
        let valid = matches!(
            (self.status, new_status),
            (ExecutionStatus::Scheduled, ExecutionStatus::Queued)
                | (ExecutionStatus::Queued, ExecutionStatus::Dispatched)
                | (ExecutionStatus::Dispatched, ExecutionStatus::Running)
                // A worker may report the outcome without an explicit "started"
                // step: the protocol hands out work as DISPATCHED and the SDKs go
                // straight to complete/fail. Requiring a heartbeat first would
                // strand every fast job in DISPATCHED forever.
                | (ExecutionStatus::Dispatched, ExecutionStatus::Succeeded)
                | (ExecutionStatus::Dispatched, ExecutionStatus::Failed)
                | (ExecutionStatus::Running, ExecutionStatus::Succeeded)
                | (ExecutionStatus::Running, ExecutionStatus::Failed)
                | (ExecutionStatus::Running, ExecutionStatus::TimedOut)
                // A dispatched attempt whose deadline passes has also run out of
                // time, whether or not the worker ever reported that it started.
                | (ExecutionStatus::Dispatched, ExecutionStatus::TimedOut)
                | (ExecutionStatus::Running, ExecutionStatus::Abandoned)
                | (ExecutionStatus::Dispatched, ExecutionStatus::Abandoned)
                | (ExecutionStatus::CancelRequested, ExecutionStatus::Cancelled)
                | (ExecutionStatus::CancelRequested, ExecutionStatus::Failed)
                | (ExecutionStatus::Failed, ExecutionStatus::RetryScheduled)
                | (ExecutionStatus::Failed, ExecutionStatus::DeadLettered)
                | (ExecutionStatus::TimedOut, ExecutionStatus::RetryScheduled)
                | (ExecutionStatus::TimedOut, ExecutionStatus::DeadLettered)
                | (ExecutionStatus::RetryScheduled, ExecutionStatus::Queued)
                // Spec 02.6: recovery policy permits re-execution.
                | (ExecutionStatus::Abandoned, ExecutionStatus::Queued)
                | (ExecutionStatus::Abandoned, ExecutionStatus::DeadLettered)
                // Any non-terminal state may have cancellation requested of it.
                | (_, ExecutionStatus::CancelRequested)
        );

        if !valid {
            return Err(DomainError::InvalidStateTransition {
                from: self.status.to_string(),
                to: new_status.to_string(),
            });
        }

        self.status = new_status;

        match new_status {
            ExecutionStatus::Dispatched => {
                self.enqueued_at.get_or_insert(now);
            }
            ExecutionStatus::Running => {
                self.started_at.get_or_insert(now);
            }
            ExecutionStatus::RetryScheduled => {
                // A requeue is a new attempt of the same logical execution.
                self.attempt_count = self.attempt_count.saturating_add(1);
                // The previous attempt ended; the execution itself has not.
                self.ended_at = None;
                self.started_at = None;
            }
            ExecutionStatus::Queued => {
                self.retry_at = None;
            }
            ExecutionStatus::Abandoned => {
                // The worker is gone; the attempt is not complete.
                self.worker_id = None;
            }
            _ => {}
        }

        if new_status.is_terminal() {
            self.ended_at = Some(now);
        }

        Ok(())
    }

    /// Records the worker that acquired the execution (spec 10.1).
    pub fn assign_worker(&mut self, worker_id: WorkerId) {
        self.worker_id = Some(worker_id);
    }

    /// Records a classified failure so retry decisions never depend on
    /// matching error text (spec 10.8).
    pub fn record_failure(&mut self, class: ErrorClass, message: Option<String>) {
        self.error_class = Some(class);
        self.error_message = message;
    }

    /// Whether the execution may be dispatched to a worker right now.
    pub fn is_dispatchable(&self) -> bool {
        matches!(self.status, ExecutionStatus::Queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Execution {
        Execution::new(TenantId::new(), JobId::new(), JobVersionId::new())
    }

    // AT-STATE-001: valid transitions succeed
    #[test]
    fn happy_path_transitions() {
        let mut exec = sample();
        assert_eq!(exec.status, ExecutionStatus::Queued);

        exec.transition_to(ExecutionStatus::Dispatched).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Dispatched);

        exec.transition_to(ExecutionStatus::Running).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Running);
        assert!(
            exec.started_at.is_some(),
            "started_at must be stamped on Running"
        );

        exec.transition_to(ExecutionStatus::Succeeded).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Succeeded);
        assert!(
            exec.ended_at.is_some(),
            "ended_at must be stamped when terminal"
        );
    }

    #[test]
    fn scheduled_to_queued() {
        let mut exec = sample();
        exec.status = ExecutionStatus::Scheduled;
        exec.transition_to(ExecutionStatus::Queued).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Queued);
    }

    // AT-STATE-002: invalid transitions fail
    #[test]
    fn invalid_transitions_are_rejected() {
        let mut exec = sample();
        // Cannot succeed before running.
        assert!(exec.transition_to(ExecutionStatus::Succeeded).is_err());
        // Cannot skip straight to running.
        assert!(exec.transition_to(ExecutionStatus::Running).is_err());
        assert_eq!(
            exec.status,
            ExecutionStatus::Queued,
            "state must not change"
        );
    }

    #[test]
    fn cannot_skip_from_queued_to_succeeded() {
        let mut exec = sample();
        // QUEUED -> SUCCEEDED is not in the spec table: work must at least be
        // handed to a worker before it can be reported done.
        assert!(exec.transition_to(ExecutionStatus::Succeeded).is_err());
    }

    /// A worker reports the outcome of dispatched work without a separate
    /// "started" step, so completion must be legal from DISPATCHED as well as
    /// from RUNNING. Without this every fast job was refused at completion time
    /// with `Invalid state transition: DISPATCHED -> SUCCEEDED`.
    #[test]
    fn dispatched_work_can_be_completed_without_a_started_step() {
        for outcome in [
            ExecutionStatus::Succeeded,
            ExecutionStatus::Failed,
            ExecutionStatus::TimedOut,
        ] {
            let mut exec = sample();
            exec.transition_to(ExecutionStatus::Dispatched).unwrap();
            assert!(
                exec.transition_to(outcome).is_ok(),
                "DISPATCHED -> {outcome} must be permitted"
            );
        }
    }

    // AT-STATE-003: a terminal execution cannot return to an active state
    #[test]
    fn terminal_execution_cannot_restart() {
        for terminal in [
            ExecutionStatus::Succeeded,
            ExecutionStatus::Cancelled,
            ExecutionStatus::DeadLettered,
        ] {
            let mut exec = sample();
            exec.status = terminal;

            for target in [
                ExecutionStatus::Queued,
                ExecutionStatus::Dispatched,
                ExecutionStatus::Running,
                ExecutionStatus::RetryScheduled,
            ] {
                assert!(
                    exec.transition_to(target).is_err(),
                    "{terminal} must not transition to {target}"
                );
            }
            assert_eq!(exec.status, terminal);
        }
    }

    /// A FAILED attempt is finished, but spec 02.6 still routes it to a retry
    /// or to the dead-letter queue.
    #[test]
    fn failed_may_retry_or_dead_letter_but_never_restart() {
        // Permitted exit 1: schedule a retry.
        let mut failed = sample();
        failed.status = ExecutionStatus::Failed;
        assert!(failed
            .transition_to(ExecutionStatus::RetryScheduled)
            .is_ok());

        // Permitted exit 2: dead-letter directly (a separate execution, since
        // the first has already moved on).
        let mut failed = sample();
        failed.status = ExecutionStatus::Failed;
        assert!(failed.transition_to(ExecutionStatus::DeadLettered).is_ok());

        // Never back to an active state.
        let mut failed = sample();
        failed.status = ExecutionStatus::Failed;
        for target in [
            ExecutionStatus::Queued,
            ExecutionStatus::Dispatched,
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
        ] {
            assert!(
                failed.transition_to(target).is_err(),
                "FAILED must not transition to {target}"
            );
        }
    }

    #[test]
    fn timed_out_may_retry_or_dead_letter() {
        let mut exec = sample();
        exec.status = ExecutionStatus::TimedOut;
        assert!(exec.transition_to(ExecutionStatus::RetryScheduled).is_ok());

        let mut exec = sample();
        exec.status = ExecutionStatus::TimedOut;
        assert!(exec.transition_to(ExecutionStatus::DeadLettered).is_ok());

        let mut exec = sample();
        exec.status = ExecutionStatus::TimedOut;
        assert!(exec.transition_to(ExecutionStatus::Running).is_err());
    }

    /// The regression this state machine previously had.
    #[test]
    fn abandoned_is_not_terminal_and_allows_recovery() {
        assert!(
            !ExecutionStatus::Abandoned.is_terminal(),
            "ABANDONED must be recoverable, not terminal"
        );

        let mut exec = sample();
        exec.transition_to(ExecutionStatus::Dispatched).unwrap();
        exec.transition_to(ExecutionStatus::Running).unwrap();
        exec.attempt_count = 1;

        exec.transition_to(ExecutionStatus::Abandoned).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Abandoned);
        assert!(
            exec.worker_id.is_none(),
            "an abandoned execution has no worker"
        );

        // Spec 02.6: recovery re-queues the work.
        exec.transition_to(ExecutionStatus::Queued).unwrap();
        assert_eq!(exec.status, ExecutionStatus::Queued);
        assert_eq!(
            exec.attempt_count, 1,
            "recovery re-queues; the next dispatch opens attempt 2"
        );
    }

    #[test]
    fn retry_schedule_increments_attempt_count() {
        let mut exec = sample();
        exec.transition_to(ExecutionStatus::Dispatched).unwrap();
        exec.transition_to(ExecutionStatus::Running).unwrap();
        exec.transition_to(ExecutionStatus::Failed).unwrap();
        assert_eq!(exec.attempt_count, 0);

        exec.transition_to(ExecutionStatus::RetryScheduled).unwrap();
        assert_eq!(exec.attempt_count, 1);

        exec.transition_to(ExecutionStatus::Queued).unwrap();
        assert!(exec.ended_at.is_none(), "a retry is not an ending");
    }

    #[test]
    fn cancellation_is_requestable_from_any_active_state() {
        for from in [
            ExecutionStatus::Queued,
            ExecutionStatus::Dispatched,
            ExecutionStatus::Running,
        ] {
            let mut exec = sample();
            exec.status = from;
            exec.transition_to(ExecutionStatus::CancelRequested)
                .unwrap();
            exec.transition_to(ExecutionStatus::Cancelled).unwrap();
            assert_eq!(exec.status, ExecutionStatus::Cancelled);
        }
    }

    /// AT-RETRY-007: a cancelled execution must not become a retry.
    #[test]
    fn cancelled_execution_cannot_be_retried() {
        let mut exec = sample();
        exec.transition_to(ExecutionStatus::CancelRequested)
            .unwrap();
        exec.transition_to(ExecutionStatus::Cancelled).unwrap();

        assert!(exec.transition_to(ExecutionStatus::RetryScheduled).is_err());
        assert!(exec.transition_to(ExecutionStatus::Queued).is_err());
    }

    #[test]
    fn dead_lettering_is_reachable_from_failed_and_abandoned() {
        let mut exec = sample();
        exec.transition_to(ExecutionStatus::Dispatched).unwrap();
        exec.transition_to(ExecutionStatus::Running).unwrap();
        exec.transition_to(ExecutionStatus::Failed).unwrap();
        exec.transition_to(ExecutionStatus::DeadLettered).unwrap();
        assert!(exec.status.is_terminal());

        let mut exec = sample();
        exec.transition_to(ExecutionStatus::Dispatched).unwrap();
        exec.transition_to(ExecutionStatus::Abandoned).unwrap();
        exec.transition_to(ExecutionStatus::DeadLettered).unwrap();
        assert!(exec.status.is_terminal());
    }

    #[test]
    fn concurrency_slot_released_only_when_terminal() {
        assert!(ExecutionStatus::Queued.holds_concurrency_slot());
        assert!(ExecutionStatus::Running.holds_concurrency_slot());
        assert!(ExecutionStatus::Abandoned.holds_concurrency_slot());
        assert!(!ExecutionStatus::Succeeded.holds_concurrency_slot());
        assert!(!ExecutionStatus::Failed.holds_concurrency_slot());
        assert!(!ExecutionStatus::DeadLettered.holds_concurrency_slot());
    }

    #[test]
    fn status_round_trips_through_its_string_form() {
        for status in [
            ExecutionStatus::Scheduled,
            ExecutionStatus::Queued,
            ExecutionStatus::Dispatched,
            ExecutionStatus::Running,
            ExecutionStatus::Succeeded,
            ExecutionStatus::Failed,
            ExecutionStatus::TimedOut,
            ExecutionStatus::CancelRequested,
            ExecutionStatus::Cancelled,
            ExecutionStatus::RetryScheduled,
            ExecutionStatus::DeadLettered,
            ExecutionStatus::Abandoned,
        ] {
            let parsed: ExecutionStatus = status.as_str().parse().unwrap();
            assert_eq!(parsed, status);
        }
        assert!("NOPE".parse::<ExecutionStatus>().is_err());
    }

    #[test]
    fn failure_records_class_not_just_text() {
        let mut exec = sample();
        exec.record_failure(ErrorClass::Timeout, Some("upstream slow".into()));
        assert_eq!(exec.error_class, Some(ErrorClass::Timeout));
    }
}
