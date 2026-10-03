//! Lease lifecycle and recovery (spec 10.11, 10.5).
//!
//! Two guarantees live here:
//!
//! * A lease that expires is recovered: the attempt is marked abandoned, its
//!   history is preserved, and the execution is re-queued so the work is not
//!   lost.
//! * A completion carrying a stale lease is rejected rather than allowed to
//!   overwrite state that has since been recovered by somebody else.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use forge_domain::{ErrorClass, ExecutionStatus, TenantId};
use forge_storage::{ExecutionRepository, LeaseRepository, WorkerRepository};

/// What the reaper decided to do with an expired lease.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryOutcome {
    pub execution_id: Uuid,
    /// The attempt that was abandoned.
    pub attempt_id: Option<Uuid>,
    /// The lease that expired.
    pub lease_id: Uuid,
    /// What happened to the execution afterwards.
    pub action: RecoveryAction,
}

/// How a recovered execution was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryAction {
    /// Re-queued for another attempt.
    Requeued,
    /// Moved to the dead-letter queue; retry is no longer possible.
    DeadLettered,
    /// The execution had already finished, so the stale lease was simply
    /// released.
    AlreadyTerminal,
}

/// Why a completion was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionRejection {
    /// The lease id does not match the execution's active lease.
    StaleLease,
    /// The lease belongs to a different worker.
    NotLeaseHolder,
    /// The lease has expired, so this result lost the race with recovery.
    LeaseExpired,
    /// The execution is already in a terminal state.
    ExecutionTerminal,
}

/// Statistics for observability (spec 12 / AT-OBS-002).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryStats {
    pub leases_examined: u64,
    pub requeued: u64,
    pub dead_lettered: u64,
    pub already_terminal: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RecoveryPolicy {
    /// Whether a recovered execution is retried or dead-lettered.
    pub retry_on_recovery: bool,
    /// How many recovery attempts an execution tolerates before it is
    /// dead-lettered, so a worker that always dies cannot loop forever.
    pub max_recovery_attempts: u32,
}

impl Default for RecoveryPolicy {
    fn default() -> Self {
        Self {
            retry_on_recovery: true,
            max_recovery_attempts: 3,
        }
    }
}

/// Recovers executions whose lease has expired.
pub struct LeaseReaper<'a> {
    pool: &'a sqlx::PgPool,
}

impl<'a> LeaseReaper<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Scans for expired leases and recovers the executions behind them.
    ///
    /// `batch` bounds how many leases are examined per pass, so the reaper never
    /// holds locks for long (spec 08.7).
    pub async fn reap(&self, _now: DateTime<Utc>, batch: i64) -> RecoveryStats {
        let leases = LeaseRepository::new(self.pool);
        let policy = RecoveryPolicy::default();
        let mut stats = RecoveryStats::default();

        let expired = match leases.claim_expired(batch).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::warn!(error = %e, "lease reaper failed to list expired leases");
                return stats;
            }
        };
        stats.leases_examined = expired.len() as u64;

        for lease in expired {
            match self.recover_one(&lease, policy).await {
                Ok(outcome) => {
                    // Release the lease so the row stops being re-examined.
                    let _ = leases.release(lease.id, lease.worker_id).await;
                    match outcome.action {
                        RecoveryAction::Requeued => stats.requeued += 1,
                        RecoveryAction::DeadLettered => stats.dead_lettered += 1,
                        RecoveryAction::AlreadyTerminal => stats.already_terminal += 1,
                    }
                    tracing::info!(
                        execution_id = %outcome.execution_id,
                        action = ?outcome.action,
                        "recovered an expired lease"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        execution_id = %lease.execution_id,
                        error = %e,
                        "lease reaper failed to recover an execution"
                    );
                }
            }
        }

        stats
    }

    async fn recover_one(
        &self,
        lease: &forge_storage::LeaseRow,
        policy: RecoveryPolicy,
    ) -> Result<RecoveryOutcome, forge_storage::StorageError> {
        let executions = ExecutionRepository::new(self.pool);
        let current = executions
            .get_unchecked(lease.execution_id)
            .await?
            .ok_or_else(|| forge_storage::StorageError::not_found("execution"))?;

        let tenant = TenantId::from_uuid(current.tenant_id);
        let mut attempt_id = None;
        let is_terminal = current
            .status
            .parse::<ExecutionStatus>()
            .map(|s| s.is_terminal())
            .unwrap_or(false);

        let action = if is_terminal {
            // The worker finished after the lease expired, or the execution was
            // cancelled meanwhile. Its state is authoritative; the lease is
            // simply stale.
            RecoveryAction::AlreadyTerminal
        } else {
            // Spec 10.11: mark the attempt abandoned, preserve its record, then
            // decide whether to retry.
            attempt_id = self.record_abandoned_attempt(&current, lease).await?;

            let recoverable = policy.retry_on_recovery
                && (current.attempt_count.max(0) as u32) < policy.max_recovery_attempts;

            if recoverable {
                executions
                    .transition(
                        tenant,
                        current.id,
                        ExecutionStatus::Abandoned,
                        Some(ErrorClass::Transient),
                        Some("worker lease expired"),
                    )
                    .await?;
                // Recovery counts as a new attempt. `ABANDONED -> QUEUED` does
                // not increment the counter itself, so without this a worker
                // that always dies would be retried forever and the recovery
                // budget would never deplete.
                executions
                    .increment_attempt(tenant, current.id)
                    .await?;
                // Spec 02.6: recovery re-queues the abandoned work.
                executions
                    .transition(tenant, current.id, ExecutionStatus::Queued, None, None)
                    .await?;
                RecoveryAction::Requeued
            } else {
                executions
                    .transition(
                        tenant,
                        current.id,
                        ExecutionStatus::Abandoned,
                        Some(ErrorClass::Transient),
                        Some("worker lease expired; recovery budget exhausted"),
                    )
                    .await?;
                executions
                    .transition(tenant, current.id, ExecutionStatus::DeadLettered, None, None)
                    .await?;
                RecoveryAction::DeadLettered
            }
        };

        Ok(RecoveryOutcome {
            execution_id: lease.execution_id,
            attempt_id,
            lease_id: lease.id,
            action,
        })
    }

    /// Preserves the abandoned attempt's record (spec 10.11 step 2).
    ///
    /// Execution history is never deleted merely because a worker disappeared,
    /// so the attempt is written with its error classification and left in
    /// place.
    async fn record_abandoned_attempt(
        &self,
        execution: &forge_storage::ExecutionRow,
        lease: &forge_storage::LeaseRow,
    ) -> Result<Option<Uuid>, forge_storage::StorageError> {
        let attempt_id = Uuid::new_v4();
        let attempt_number = execution.attempt_count.max(0) + 1;

        sqlx::query(
            "INSERT INTO execution_attempts
                 (id, tenant_id, execution_id, attempt_number, worker_id, lease_id, status,
                  started_at, ended_at, error_class, error_message)
             VALUES ($1, $2, $3, $4, $5, $6, 'ABANDONED', NOW(), NOW(), 'TRANSIENT', $7)
             ON CONFLICT (execution_id, attempt_number) DO NOTHING",
        )
        .bind(attempt_id)
        .bind(execution.tenant_id)
        .bind(execution.id)
        .bind(attempt_number)
        .bind(lease.worker_id)
        .bind(lease.id)
        .bind("worker lease expired")
        .execute(self.pool)
        .await
        .map_err(forge_storage::StorageError::from_sqlx)?;

        Ok(Some(attempt_id))
    }
}

/// Accepts a worker's completion, or explains why it is refused.
///
/// Spec 10.5: a worker that finishes after its lease expired must not be able
/// to overwrite state that recovery has since moved on.
pub struct CompletionGate<'a> {
    pool: &'a sqlx::PgPool,
}

impl<'a> CompletionGate<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Validates a completion report against the current lease and execution.
    pub async fn check(
        &self,
        execution_id: Uuid,
        lease_id: Uuid,
        worker_id: Uuid,
    ) -> Result<(), CompletionRejection> {
        let leases = LeaseRepository::new(self.pool);
        let executions = ExecutionRepository::new(self.pool);

        let execution = executions
            .get_unchecked(execution_id)
            .await
            .map_err(|_| CompletionRejection::StaleLease)?
            .ok_or(CompletionRejection::StaleLease)?;

        if execution
            .status
            .parse::<ExecutionStatus>()
            .map(|s| s.is_terminal())
            .unwrap_or(false)
        {
            return Err(CompletionRejection::ExecutionTerminal);
        }

        let lease = leases
            .active_for_execution(execution_id)
            .await
            .map_err(|_| CompletionRejection::StaleLease)?
            .ok_or(CompletionRejection::StaleLease)?;

        // The reported lease must be the active one.
        if lease.id != lease_id {
            return Err(CompletionRejection::StaleLease);
        }
        if lease.worker_id != worker_id {
            return Err(CompletionRejection::NotLeaseHolder);
        }
        if lease.expires_at <= Utc::now() {
            return Err(CompletionRejection::LeaseExpired);
        }

        Ok(())
    }
}

/// Counts workers whose heartbeat has lapsed.
pub struct HeartbeatMonitor<'a> {
    pool: &'a sqlx::PgPool,
}

impl<'a> HeartbeatMonitor<'a> {
    pub fn new(pool: &'a sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Marks workers offline once their heartbeat is older than
    /// `stale_after_secs`.
    pub async fn reap_stale(&self, stale_after_secs: i64) -> Result<u64, forge_storage::StorageError> {
        let workers = WorkerRepository::new(self.pool);
        workers.reap_stale(stale_after_secs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_policy_defaults_are_bounded() {
        let policy = RecoveryPolicy::default();
        assert!(policy.retry_on_recovery);
        assert_eq!(
            policy.max_recovery_attempts, 3,
            "recovery must not retry forever"
        );
    }

    #[test]
    fn rejection_variants_distinguish_causes() {
        // Spec 10.5 wants each failure mode distinguishable for metrics and
        // for the worker's own logging.
        assert_ne!(
            CompletionRejection::StaleLease,
            CompletionRejection::NotLeaseHolder
        );
        assert_ne!(
            CompletionRejection::LeaseExpired,
            CompletionRejection::ExecutionTerminal
        );
    }

    #[test]
    fn recovery_actions_are_distinct() {
        assert_ne!(RecoveryAction::Requeued, RecoveryAction::DeadLettered);
        assert_ne!(RecoveryAction::Requeued, RecoveryAction::AlreadyTerminal);
    }

    #[test]
    fn recovery_stats_start_at_zero() {
        let stats = RecoveryStats::default();
        assert_eq!(stats.requeued, 0);
        assert_eq!(stats.dead_lettered, 0);
        assert_eq!(stats.leases_examined, 0);
    }
}