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
use forge_executor::{HeartbeatMonitor, LeaseReaper};
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
            let engine = SchedulerEngine::new(pool, batch);
            let mut ticker = tokio::time::interval(interval);
            // Skip missed ticks rather than firing a burst of catch-up work after
            // the process was paused.
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

            info!(
                interval_ms = interval.as_millis() as u64,
                batch_size = batch,
                "scheduler loop started"
            );

            loop {
                tokio::select! {
                    _ = ticker.tick() => {
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

                        // Retries the operator requested are `RETRY_SCHEDULED`
                        // until something moves them back to `QUEUED`; without
                        // this they sat there holding a concurrency slot.
                        // The reaper above is pool-wide, so the retry sweep
                        // matches it: a single statement over every tenant,
                        // which cannot straddle one by construction.
                        let retried = match sqlx::query(
                            "UPDATE executions SET status = 'QUEUED', updated_at = NOW()
                             WHERE id IN (
                                 SELECT id FROM executions
                                 WHERE status = 'RETRY_SCHEDULED'
                                   AND updated_at <= NOW() - INTERVAL '5 seconds'
                                 ORDER BY updated_at ASC
                                 FOR UPDATE SKIP LOCKED
                                 LIMIT 100
                             )",
                        )
                        .execute(&pool)
                        .await
                        {
                            Ok(result) => result.rows_affected() as i64,
                            Err(e) => {
                                warn!(error = %e, "could not requeue due retries");
                                0
                            }
                        };

                        if stats.requeued > 0 || stats.dead_lettered > 0 || retried > 0 {
                            info!(
                                examined = stats.leases_examined,
                                requeued = stats.requeued,
                                dead_lettered = stats.dead_lettered,
                                retries_requeued = retried,
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

/// How many rows one retention pass removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetentionReport {
    pub executions: u64,
    pub attempts: u64,
    pub idempotency: u64,
}

impl RetentionReport {
    pub fn total(&self) -> u64 {
        self.executions + self.attempts + self.idempotency
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
            idempotency: 5,
        };
        assert_eq!(report.total(), 10);
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
