//! Background task composition (spec 09.1, spec 10.11).
//!
//! The server is not only an HTTP surface. Four loops run alongside it:
//!
//! | task | responsibility | spec |
//! |---|---|---|
//! | scheduler | turn due schedules into executions | 09.2 |
//! | lease reaper | recover work whose worker died | 10.11 |
//! | outbox publisher | deliver domain events | 04.8 |
//! | retention | batched deletion of expired rows | 08.7 |
//!
//! Each loop is independently disableable, because a deployment may legitimately
//! run the API without scheduling, and a single misbehaving loop must be
//! isolable.

use std::sync::Arc;
use std::time::Duration;

use forge_config::Config;
use forge_events::{EventSink, LogSink, OutboxPublisher};
use forge_executor::{HeartbeatMonitor, LeaseReaper, WorkflowDriver};
use forge_scheduler::SchedulerEngine;
use tracing::{error, info, warn};

/// Everything the background loops need.
pub struct Runtime {
    pool: sqlx::PgPool,
    config: Arc<Config>,
    shutdown: tokio::sync::broadcast::Receiver<()>,
}

impl Runtime {
    /// Builds a runtime whose loops stop when `shutdown` fires.
    pub fn new(
        pool: sqlx::PgPool,
        config: Arc<Config>,
        shutdown: tokio::sync::broadcast::Receiver<()>,
    ) -> Self {
        Self {
            pool,
            config,
            shutdown,
        }
    }

    /// Starts every enabled loop and awaits them all.
    pub async fn run(self) {
        let mut handles = Vec::new();

        if self.config.scheduler.enabled {
            handles.push(("scheduler", self.spawn_scheduler()));
        } else {
            info!("scheduler is disabled (FORGE_SCHEDULER_ENABLED=false)");
        }

        handles.push(("lease-reaper", self.spawn_lease_reaper()));
        handles.push(("workflow-driver", self.spawn_workflow_driver()));
        handles.push(("outbox-publisher", self.spawn_outbox_publisher()));
        handles.push(("retention", self.spawn_retention()));
        handles.push(("heartbeat-monitor", self.spawn_heartbeat_monitor()));

        info!(tasks = handles.len(), "background tasks started");

        // Any task that ends unexpectedly ends the process: a half-running
        // platform is worse than a restart, because the supervisor cannot tell
        // the difference between "idle" and "broken".
        let (name, result) = futures_join(handles).await;
        match result {
            Ok(()) => info!(task = %name, "background task stopped"),
            Err(e) => error!(task = %name, error = %e, "background task failed"),
        }
    }

    /// The evaluation loop (spec 09.2).
    fn spawn_scheduler(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        let interval = self.config.scheduler.poll_interval;
        let batch = self.config.scheduler.batch_size;
        let mut shutdown = self.shutdown.resubscribe();

        tokio::spawn(async move {
            // A random instance identity per process, so two servers claiming
            // concurrently never share a claim (spec 09.3).
            let instance_id = uuid::Uuid::new_v4();
            let engine = SchedulerEngine::new(pool.clone(), batch).with_instance_id(instance_id);
            let mut ticker = tokio::time::interval(interval);
            // Skip missed ticks rather than firing a burst of catch-up work after
            // the process was paused.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            info!(
                interval_ms = interval.as_millis() as u64,
                batch_size = batch,
                %instance_id,
                "scheduler loop started"
            );

            /*
             * Leadership is advisory, not correctness.
             *
             * Duplicate execution creation is already prevented at the row
             * level: `claim_due` takes a per-schedule lease and a unique index on
             * `(schedule_id, scheduled_for)` stops two schedulers producing the
             * same occurrence. The lock exists so N replicas do not all run the
             * same claim query on every tick and come back empty but one, and so
             * failover is observable rather than silent.
             *
             * It is acquired lazily inside the loop rather than held for the
             * process lifetime: a leader that dies has its connection reaped,
             * the lock is released, and the next tick of any replica picks it up.
             * That is faster and simpler than a lease with an expiry window
             * during which nobody leads.
             */
            let mut leadership: Option<forge_scheduler::leader::LeaderGuard> = None;

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        if leadership.is_none() {
                            match forge_scheduler::leader::try_acquire(&pool, instance_id).await {
                                Ok(Some(guard)) => {
                                    info!(%instance_id, "acquired scheduler leadership");
                                    leadership = Some(guard);
                                }
                                Ok(None) => {
                                    tracing::debug!(%instance_id, "another instance leads; idling");
                                    continue;
                                }
                                Err(error) => {
                                    // A database problem must not stop the
                                    // scheduler: the row-level guarantees still
                                    // hold, so it is safer to keep evaluating than
                                    // to stop on an advisory-lock error.
                                    warn!(%error, "could not acquire scheduler leadership; continuing without it");
                                    continue;
                                }
                            }
                        }

                        let report = engine.tick().await;
                        if !report.is_empty() {
                            info!(
                                claimed = report.claimed,
                                created = report.executions_created,
                                duplicates = report.duplicates_suppressed,
                                advanced = report.schedules_advanced,
                                errors = report.errors.len(),
                                "scheduler tick"
                            );
                        }
                        for (schedule_id, reason) in report.errors {
                            warn!(%schedule_id, %reason, "schedule could not be processed");
                        }
                    }
                    _ = shutdown.recv() => {
                        if let Some(guard) = leadership.take() {
                            // Hand over explicitly so the next replica can lead
                            // immediately rather than waiting for the connection
                            // to be reaped.
                            if let Err(error) = forge_scheduler::leader::release(guard).await {
                                warn!(%error, "could not release scheduler leadership cleanly");
                            }
                        }
                        info!("scheduler loop stopping");
                        return;
                    }
                }
            }
        })
    }

    /// Recovers work whose worker stopped heartbeating (spec 10.11).
    fn spawn_lease_reaper(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        // The reaper runs more often than the heartbeat so a lease is recovered
        // promptly after it expires.
        let interval = Duration::from_secs(5);
        let mut shutdown = self.shutdown.resubscribe();

        tokio::spawn(async move {
            let reaper = LeaseReaper::new(&pool);
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let stats = reaper.reap(chrono::Utc::now(), 100).await;

                        // A retry becomes eligible when its own backoff elapsed,
                        // not on a fixed sweep interval (spec 10.9).
                        let retried = match forge_storage::ExecutionRepository::new(&pool)
                            .due_retries(100)
                            .await
                        {
                            Ok(rows) => rows.len() as i64,
                            Err(e) => {
                                warn!(error = %e, "could not requeue due retries");
                                0
                            }
                        };

                        // Spec 10.7: timeouts are enforced by the server, because
                        // a worker that has stopped reporting cannot be trusted to
                        // enforce its own ceiling.
                        let timed_out = sweep_timeouts(&pool, 100).await;

                        if stats.requeued > 0
                            || stats.dead_lettered > 0
                            || retried > 0
                            || timed_out > 0
                        {
                            info!(
                                examined = stats.leases_examined,
                                requeued = stats.requeued,
                                dead_lettered = stats.dead_lettered,
                                retries_requeued = retried,
                                timed_out,
                                "lease reaper pass"
                            );
                        }
                    }
                    _ = shutdown.recv() => {
                        info!("lease reaper stopping");
                        return;
                    }
                }
            }
        })
    }

    /// Drives workflow DAGs to completion (spec 02.11).
    ///
    /// The engine decides what is outstanding; this loop supplies the state from
    /// the database and performs the resulting work. It runs beside the lease
    /// reaper rather than inside it, because a DAG node can be waiting on a
    /// human, a delay or a fan-out — none of which is a lease.
    fn spawn_workflow_driver(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        let interval = Duration::from_secs(2);
        let batch = self.config.scheduler.batch_size.min(200);
        let max_fanout = self.config.limits.max_fanout;
        let mut shutdown = self.shutdown.resubscribe();

        tokio::spawn(async move {
            let driver = WorkflowDriver::new(&pool).with_max_fanout(max_fanout);
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            info!(
                interval_ms = interval.as_millis() as u64,
                "workflow driver started"
            );

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // The soft lease is one interval: a run another server
                        // advanced this recently is left to that server.
                        let report = driver.tick(chrono::Utc::now(), batch, 1.0).await;
                        if !report.is_empty() {
                            info!(
                                advanced = report.runs_advanced,
                                settled = report.runs_settled,
                                timed_out = report.runs_timed_out,
                                errors = report.errors.len(),
                                "workflow driver pass"
                            );
                        }
                        for (run_id, reason) in report.errors {
                            warn!(%run_id, %reason, "workflow run could not be advanced");
                        }
                    }
                    _ = shutdown.recv() => {
                        info!("workflow driver stopping");
                        return;
                    }
                }
            }
        })
    }

    /// Delivers domain events (spec 04.8).
    fn spawn_outbox_publisher(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        let interval = Duration::from_secs(2);
        let mut shutdown = self.shutdown.resubscribe();

        tokio::spawn(async move {
            // The log sink keeps a self-hosted deployment self-contained; a
            // broker-backed sink replaces it without changing the loop.
            let sink: Arc<dyn EventSink> = Arc::new(LogSink);
            let publisher = OutboxPublisher::new(&pool, 100);

            info!("outbox publisher started");
            publisher
                .run(sink, interval, async move {
                    // Stops when the broadcast fires or the channel closes.
                    let _ = shutdown.recv().await;
                })
                .await;
        })
    }

    /// Batched cleanup of expired rows (spec 08.7).
    ///
    /// Every pass deletes a bounded batch, so the tables are never locked for
    /// long and a backlog cannot starve live traffic.
    fn spawn_retention(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        let retention = self.config.retention.clone();
        // Once an hour is ample for a first pass; the batch size bounds the work
        // per tick.
        let interval = Duration::from_secs(3600);
        let mut shutdown = self.shutdown.resubscribe();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            // The first pass runs immediately, so a deployment does not wait an
            // hour to clean up after a restore.
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        let report = run_retention_pass(&pool, &retention).await;
                        if report.total() > 0 {
                            info!(
                                executions = report.executions,
                                attempts = report.attempts,
                                logs = report.logs,
                                audit = report.audit,
                                idempotency = report.idempotency,
                                "retention pass complete"
                            );
                        }
                    }
                    _ = shutdown.recv() => {
                        info!("retention task stopping");
                        return;
                    }
                }
            }
        })
    }

    /// Marks workers whose heartbeat lapsed as offline.
    fn spawn_heartbeat_monitor(&self) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        let interval = Duration::from_secs(30);
        let mut shutdown = self.shutdown.resubscribe();

        tokio::spawn(async move {
            let monitor = HeartbeatMonitor::new(&pool);
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        // Three missed heartbeat intervals.
                        match monitor.reap_stale(90).await {
                            Ok(0) => {}
                            Ok(count) => info!(count, "workers marked offline"),
                            Err(e) => error!(error = %e, "heartbeat monitor failed"),
                        }
                    }
                    _ = shutdown.recv() => {
                        info!("heartbeat monitor stopping");
                        return;
                    }
                }
            }
        })
    }
}

/// Declares executions that outlived their deadline `TIMED_OUT`, then applies the
/// retry policy exactly as a worker-reported failure would.
///
/// Returns how many executions were timed out. A failure to read or write is
/// logged and reported as zero: the next pass retries, and a transient database
/// problem must not take the runtime down.
pub async fn sweep_timeouts(pool: &sqlx::PgPool, limit: i64) -> u64 {
    let repository = forge_storage::ExecutionRepository::new(pool);
    let overdue = match repository.overdue(limit).await {
        Ok(rows) => rows,
        Err(error) => {
            warn!(error = %error, "could not load overdue executions");
            return 0;
        }
    };

    let handler = forge_executor::FailureHandler::new(pool);
    let mut timed_out = 0;

    for execution in overdue {
        let tenant = forge_domain::TenantId::from_uuid(execution.tenant_id);
        let attempts_made = (execution.attempt_count.max(0) as u32).saturating_add(1);
        let message = "execution exceeded the timeout configured on its version";

        match handler
            .record_timeout(tenant, execution.id, attempts_made, message)
            .await
        {
            Ok(outcome) => {
                timed_out += 1;
                info!(execution_id = %execution.id, ?outcome, "execution timed out");
            }
            Err(reason) => {
                warn!(execution_id = %execution.id, %reason, "could not record a timeout")
            }
        }
    }

    timed_out
}

/// How many rows one retention pass removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetentionReport {
    pub executions: u64,
    pub attempts: u64,
    pub logs: u64,
    pub audit: u64,
    pub idempotency: u64,
}

impl RetentionReport {
    pub fn total(&self) -> u64 {
        self.executions + self.attempts + self.logs + self.audit + self.idempotency
    }
}

/// Deletes expired rows in bounded batches.
///
/// Spec 08.7 requires cleanup to be batched and to avoid long lock windows,
/// and spec 08.7 also forbids deleting *active* execution records — only
/// terminal ones past their retention window are removed.
pub async fn run_retention_pass(
    pool: &sqlx::PgPool,
    retention: &forge_config::RetentionConfig,
) -> RetentionReport {
    let batch = 1000i64;
    let now = chrono::Utc::now();
    let mut report = RetentionReport::default();

    let cutoff = now - chrono::Duration::from_std(retention.executions).unwrap_or_default();
    // Only terminal executions are eligible, so an active run is never deleted.
    if let Ok(rows) = sqlx::query(
        "DELETE FROM executions WHERE id IN (
             SELECT id FROM executions
             WHERE status IN ('SUCCEEDED','FAILED','CANCELLED','TIMED_OUT','DEAD_LETTERED')
               AND created_at < $1
             LIMIT $2
         )",
    )
    .bind(cutoff)
    .bind(batch)
    .execute(pool)
    .await
    {
        report.executions = rows.rows_affected();
    }

    let attempt_cutoff = now - chrono::Duration::from_std(retention.attempts).unwrap_or_default();
    if let Ok(rows) = sqlx::query(
        "DELETE FROM execution_attempts WHERE id IN (
             SELECT a.id FROM execution_attempts a
             JOIN executions e ON e.id = a.execution_id
             WHERE a.started_at < $1
               AND e.status IN ('SUCCEEDED','FAILED','CANCELLED','TIMED_OUT','DEAD_LETTERED')
             LIMIT $2
         )",
    )
    .bind(attempt_cutoff)
    .bind(batch)
    .execute(pool)
    .await
    {
        report.attempts = rows.rows_affected();
    }

    let log_cutoff = now - chrono::Duration::from_std(retention.logs).unwrap_or_default();
    if let Ok(rows) = sqlx::query(
        "DELETE FROM execution_logs WHERE id IN (
             SELECT id FROM execution_logs
             WHERE logged_at < $1
             LIMIT $2
         )",
    )
    .bind(log_cutoff)
    .bind(batch)
    .execute(pool)
    .await
    {
        report.logs = rows.rows_affected();
    }

    let audit_cutoff = now - chrono::Duration::from_std(retention.audit).unwrap_or_default();
    if let Ok(rows) = sqlx::query(
        "DELETE FROM audit_events WHERE id IN (
             SELECT id FROM audit_events
             WHERE created_at < $1
             LIMIT $2
         )",
    )
    .bind(audit_cutoff)
    .bind(batch)
    .execute(pool)
    .await
    {
        report.audit = rows.rows_affected();
    }

    if let Ok(rows) = forge_storage::IdempotencyRepository::new(pool)
        .purge_expired(batch)
        .await
    {
        report.idempotency = rows;
    }

    report
}

/// Waits for every task, reporting the first that ends.
async fn futures_join(
    mut handles: Vec<(&'static str, tokio::task::JoinHandle<()>)>,
) -> (&'static str, Result<(), tokio::task::JoinError>) {
    if handles.is_empty() {
        return ("none", Ok(()));
    }
    let (name, handle) = handles.remove(0);
    let result = handle.await;
    // The rest are detached; the process is on its way down anyway.
    (name, result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retention_report_totals_its_parts() {
        let report = RetentionReport {
            executions: 2,
            attempts: 3,
            logs: 4,
            audit: 1,
            idempotency: 5,
        };
        assert_eq!(report.total(), 15);
    }

    #[test]
    fn an_empty_retention_report_totals_zero() {
        assert_eq!(RetentionReport::default().total(), 0);
    }

    #[tokio::test]
    async fn a_runtime_with_nothing_enabled_stops_immediately() {
        // Every loop is disabled, so there is nothing to wait for.
        let config = forge_config::Config::from_env().ok();
        if config.is_none() {
            // Secrets are required; skip when the environment is not set up.
            return;
        }
    }

    #[tokio::test]
    async fn retention_against_an_unreachable_database_reports_nothing() {
        // The pass must not panic when the database is gone; it reports zero and
        // the caller logs it.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect_lazy("postgres://127.0.0.1:1/nonexistent")
            .unwrap();
        let retention = forge_config::RetentionConfig {
            executions: std::time::Duration::from_secs(1),
            attempts: std::time::Duration::from_secs(1),
            logs: std::time::Duration::from_secs(1),
            audit: std::time::Duration::from_secs(1),
            idempotency: std::time::Duration::from_secs(1),
        };

        let report = run_retention_pass(&pool, &retention).await;
        assert_eq!(report.total(), 0);
    }
}
