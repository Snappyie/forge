//! The schedule evaluation loop (spec 09.2).
//!
//! ```text
//! load due schedules
//!   -> claim
//!   -> calculate intended occurrence
//!   -> create execution
//!   -> enqueue
//!   -> calculate next occurrence
//!   -> persist
//! ```
//!
//! Every step is idempotent. Duplicate execution creation is prevented at the
//! database level by the unique index on `(schedule_id, scheduled_for)`
//! (spec 09.3), so two schedulers racing the same occurrence cannot both win.

use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use forge_domain::{JobId, JobVersionId, TenantId};
use forge_storage::{
    ClaimedSchedule, ExecutionRepository, JobRepository, JobVersionRepository, NewExecution,
    OutboxRepository, ScheduleRepository,
};

use crate::clock::{Clock, SystemClock};
use crate::misfire::{default_catch_up_limit, plan_occurrences, MisfirePolicy, SkipReason};
use crate::schedule::CronSchedule;

/// What one tick of the loop did. Returned so callers (and tests) can assert on
/// it without reaching into the database.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct TickReport {
    pub claimed: usize,
    pub executions_created: usize,
    pub duplicates_suppressed: usize,
    pub schedules_advanced: usize,
    pub skipped: Vec<(Uuid, DateTime<Utc>, SkipReason)>,
    pub errors: Vec<(Uuid, String)>,
}

impl TickReport {
    pub fn is_empty(&self) -> bool {
        self.claimed == 0 && self.executions_created == 0
    }
}

/// Drives schedules to executions.
pub struct SchedulerEngine {
    pool: sqlx::PgPool,
    clock: Arc<dyn Clock>,
    batch_size: i64,
    /// Identifies this scheduler instance when it claims schedules.
    instance_id: Uuid,
    /// How long a claim is held before another scheduler may take the schedule.
    claim_lease: Duration,
}

impl SchedulerEngine {
    pub fn new(pool: sqlx::PgPool, batch_size: i64) -> Self {
        Self {
            pool,
            clock: Arc::new(SystemClock),
            batch_size,
            // A random per-process identity: two engines in one process (as in
            // tests) must not share a claim identity.
            instance_id: Uuid::new_v4(),
            claim_lease: Duration::seconds(30),
        }
    }

    /// Replaces the clock. Tests use this to pin DST and misfire behaviour
    /// (spec 04.9).
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Overrides the claim identity. Production uses the random default.
    pub fn with_instance_id(mut self, id: Uuid) -> Self {
        self.instance_id = id;
        self
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Runs one tick of the evaluation loop.
    pub async fn tick(&self) -> TickReport {
        let now = self.now();
        let schedules = ScheduleRepository::new(&self.pool);
        let executions = ExecutionRepository::new(&self.pool);

        // `claim_due` writes a lease marker inside its transaction, so only one
        // scheduler instance can hold a given schedule (spec 09.3).
        let claimed = match schedules
            .claim_due(now, self.batch_size, self.instance_id, self.claim_lease.num_seconds())
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                warn!(error = %e, "scheduler failed to claim due schedules");
                return TickReport::default();
            }
        };

        let mut report = TickReport {
            claimed: claimed.len(),
            ..Default::default()
        };

        for row in claimed {
            match self
                .process(&row, now, &schedules, &executions)
                .await
            {
                Ok(outcome) => {
                    report.executions_created += outcome.created;
                    report.duplicates_suppressed += outcome.duplicates;
                    report.schedules_advanced += usize::from(outcome.advanced);
                    report.skipped.extend(outcome.skipped);
                }
                Err(e) => {
                    warn!(
                        schedule_id = %row.id,
                        error = %e,
                        "scheduler failed to process schedule"
                    );
                    report.errors.push((row.id, e.to_string()));
                    // Release the claim so another instance can retry promptly
                    // rather than waiting for the lease to expire.
                    let _ = schedules.release_claim(row.id, self.instance_id).await;
                }
            }
        }

        if !report.is_empty() {
            info!(
                claimed = report.claimed,
                created = report.executions_created,
                duplicates = report.duplicates_suppressed,
                errors = report.errors.len(),
                "scheduler tick complete"
            );
        }

        report
    }

    /// Handles one claimed schedule.
    async fn process(
        &self,
        row: &ClaimedSchedule,
        now: DateTime<Utc>,
        schedules: &ScheduleRepository<'_>,
        executions: &ExecutionRepository<'_>,
    ) -> Result<ProcessOutcome, forge_storage::StorageError> {
        let tenant = TenantId::from_uuid(row.tenant_id);
        let due = row.next_run_at;
        let mut outcome = ProcessOutcome::default();

        // Resolve the cron before doing anything else; a schedule with an
        // invalid expression must not block the rest of the batch.
        let cron = match CronSchedule::parse(
            row.cron_expression.as_deref().unwrap_or_default(),
            &row.timezone,
        ) {
            Ok(c) => c,
            Err(e) => {
                warn!(schedule_id = %row.id, error = %e, "skipping schedule with invalid cron");
                // Still advance so a broken schedule is not retried forever.
                self.advance(row, tenant, due, now, schedules).await?;
                outcome.advanced = true;
                return Ok(outcome);
            }
        };

        let policy = MisfirePolicy::parse(&row.misfire_policy);
        let catch_up_limit = row
            .catch_up_policy
            .get("max_occurrences")
            .and_then(|v| v.as_u64())
            .unwrap_or(default_catch_up_limit() as u64) as u32;

        // Which occurrences already exist makes the whole tick idempotent.
        let existing = schedules.materialised_occurrences(row.id, due).await?;
        let plan = plan_occurrences(
            &cron,
            due,
            now,
            policy,
            catch_up_limit,
            &existing,
        );

        for (occurrence, reason) in &plan.skipped {
            if *reason == SkipReason::AlreadyMaterialised {
                outcome.duplicates += 1;
            }
            outcome.skipped.push((row.id, *occurrence, *reason));
        }

        // Create one execution per planned occurrence.
        for occurrence in &plan.fire {
            match self
                .create_execution(row, tenant, *occurrence, executions)
                .await
            {
                Ok(true) => outcome.created += 1,
                // The unique index rejected it: another scheduler won the race.
                Ok(false) => outcome.duplicates += 1,
                Err(e) => {
                    warn!(
                        schedule_id = %row.id,
                        error = %e,
                        "failed to create execution for occurrence"
                    );
                    return Err(e);
                }
            }
        }

        // Advance the schedule regardless of how many executions were made, so
        // a broken occurrence does not wedge the schedule forever.
        self.advance(row, tenant, due, now, schedules).await?;
        outcome.advanced = true;

        Ok(outcome)
    }

    /// Advances `next_run_at` to the following occurrence.
    ///
    /// The UPDATE is conditional on the occurrence that was acted on, so a
    /// scheduler acting on a stale read cannot overwrite a newer one.
    async fn advance(
        &self,
        row: &ClaimedSchedule,
        tenant: TenantId,
        due: DateTime<Utc>,
        now: DateTime<Utc>,
        schedules: &ScheduleRepository<'_>,
    ) -> Result<(), forge_storage::StorageError> {
        let next = match CronSchedule::parse(
            row.cron_expression.as_deref().unwrap_or_default(),
            &row.timezone,
        ) {
            Ok(c) => c.next_after(due).or_else(|| c.next_after(now)),
            Err(_) => None,
        };

        match next {
            Some(next_run) => {
                schedules
                    .advance_next_run(row.id, tenant, due, next_run)
                    .await?;
                debug!(
                    schedule_id = %row.id,
                    %due,
                    next = %next_run,
                    "advanced schedule"
                );
            }
            None => {
                // An expression with no future occurrence (e.g. February 30th)
                // disables the schedule rather than spinning on it.
                warn!(schedule_id = %row.id, "cron has no future occurrence; disabling");
                schedules.pause(tenant, row.id).await?;
            }
        }
        Ok(())
    }

    /// Creates an execution for one occurrence.
    ///
    /// Returns `false` when the database rejected a duplicate, which is how a
    /// losing scheduler in a race learns it did not win.
    async fn create_execution(
        &self,
        row: &ClaimedSchedule,
        tenant: TenantId,
        occurrence: DateTime<Utc>,
        executions: &ExecutionRepository<'_>,
    ) -> Result<bool, forge_storage::StorageError> {
        // Resolve the effective version (spec invariant 6: the version an
        // execution ran is recorded on the execution).
        let version_id = match self.resolve_version(tenant, row, executions).await? {
            Some(id) => id,
            None => {
                warn!(
                    schedule_id = %row.id,
                    "no published version available; occurrence skipped"
                );
                return Ok(false);
            }
        };

        let job_id = JobId::from_uuid(row.target_id);
        // The correlation id must include the occurrence: catch-up creates
        // several executions for one schedule, and a schedule-only id would
        // collide with the (tenant, correlation_id) uniqueness index on the
        // second one.
        let correlation_id = format!("schedule:{}:{}", row.id, occurrence.timestamp());
        match executions
            .create(NewExecution {
                tenant_id: tenant,
                job_id,
                job_version_id: version_id,
                queue_id: None,
                schedule_id: Some(row.id),
                trigger_source: forge_domain::TriggerSource::Schedule,
                priority: forge_domain::Priority::Normal,
                scheduled_for: Some(occurrence),
                correlation_id: Some(correlation_id),
                input: serde_json::json!({}),
            })
            .await
        {
            Ok(_) => Ok(true),
            // A unique violation on (schedule_id, scheduled_for) means this
            // occurrence already exists.
            Err(forge_storage::StorageError::Conflict(_)) => {
                debug!(schedule_id = %row.id, %occurrence, "occurrence already materialised");
                Ok(false)
            }
            Err(e) => Err(e),
        }
    }

    /// Resolves the job version an occurrence should run.
    async fn resolve_version(
        &self,
        tenant: TenantId,
        row: &ClaimedSchedule,
        _executions: &ExecutionRepository<'_>,
    ) -> Result<Option<JobVersionId>, forge_storage::StorageError> {
        let jobs = JobRepository::new(&self.pool);
        let versions = JobVersionRepository::new(&self.pool);
        let job_id = JobId::from_uuid(row.target_id);

        // An explicit current version wins.
        if let Ok(job) = jobs.get(tenant, job_id).await {
            if let Some(current) = job.current_version_id {
                return Ok(Some(JobVersionId::from_uuid(current)));
            }
        }

        // Otherwise fall back to the latest published version.
        Ok(versions
            .latest_published(tenant, job_id)
            .await?
            .map(|v| JobVersionId::from_uuid(v.id)))
    }

    /// Writes an audit/outbox record for an execution created by the scheduler.
    pub async fn record_created(
        &self,
        tenant: TenantId,
        execution_id: Uuid,
        occurrence: DateTime<Utc>,
    ) -> Result<(), forge_storage::StorageError> {
        let outbox = OutboxRepository::new(&self.pool);
        outbox
            .enqueue(
                Some(tenant),
                "execution.created",
                "execution",
                execution_id,
                serde_json::json!({
                    "scheduled_for": occurrence.to_rfc3339(),
                    "source": "scheduler",
                }),
            )
            .await?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct ProcessOutcome {
    created: usize,
    duplicates: usize,
    advanced: bool,
    skipped: Vec<(Uuid, DateTime<Utc>, SkipReason)>,
}

/// Previews the next `count` occurrences for a schedule expression.
///
/// Spec 9.13 requires the preview and the real loop to share one engine, so
/// this is the same [`CronSchedule`] the loop uses — a preview cannot drift
/// from actual behaviour.
pub fn preview_occurrences(
    expression: &str,
    timezone: &str,
    after: DateTime<Utc>,
    count: usize,
) -> Result<Vec<DateTime<Utc>>, crate::schedule::SchedulerError> {
    let cron = CronSchedule::parse(expression, timezone)?;
    Ok(cron.next_n_after(after, count))
}

/// Human-readable explanation of what a schedule will do next, used by the
/// dispatch explanation required by spec 12 (AT-OBS-005).
pub fn explain_schedule(
    expression: &str,
    timezone: &str,
    after: DateTime<Utc>,
) -> Result<ScheduleExplanation, crate::schedule::SchedulerError> {
    let cron = CronSchedule::parse(expression, timezone)?;
    let upcoming = cron.next_n_after(after, 3);
    Ok(ScheduleExplanation {
        expression: expression.to_string(),
        timezone: timezone.to_string(),
        next_run_at: upcoming.first().copied(),
        upcoming,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleExplanation {
    pub expression: String,
    pub timezone: String,
    pub next_run_at: Option<DateTime<Utc>>,
    pub upcoming: Vec<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_and_scheduler_share_one_engine() {
        // The preview helper delegates to CronSchedule, so both paths agree by
        // construction rather than by convention (spec 9.13).
        let after = DateTime::parse_from_rfc3339("2026-10-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let preview = preview_occurrences("0 2 * * *", "UTC", after, 5).unwrap();
        let cron = CronSchedule::parse("0 2 * * *", "UTC").unwrap();
        assert_eq!(preview, cron.next_n_after(after, 5));
    }

    #[test]
    fn preview_rejects_a_six_field_expression() {
        let after = Utc::now();
        assert!(preview_occurrences("0 0 2 * * *", "UTC", after, 5).is_err());
    }

    #[test]
    fn preview_rejects_an_unknown_timezone() {
        let after = Utc::now();
        assert!(preview_occurrences("0 2 * * *", "Nowhere/Land", after, 5).is_err());
    }

    #[test]
    fn explanation_reports_the_next_run() {
        let after = DateTime::parse_from_rfc3339("2026-10-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let explanation = explain_schedule("0 2 * * *", "UTC", after).unwrap();
        assert_eq!(
            explanation.next_run_at.unwrap().to_rfc3339(),
            "2026-10-03T02:00:00+00:00"
        );
        assert_eq!(expression_upcoming_len(&explanation), 3);
        assert_eq!(explanation.timezone, "UTC");
    }

    fn expression_upcoming_len(e: &ScheduleExplanation) -> usize {
        e.upcoming.len()
    }

    #[test]
    fn explanation_respects_the_schedule_timezone() {
        use chrono::Timelike;
        use chrono_tz::Tz;

        let after = DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let explanation = explain_schedule("0 2 * * *", "Asia/Kolkata", after).unwrap();
        let next = explanation.next_run_at.unwrap();

        // The instant is 02:00 in the schedule's own zone, whatever the UTC
        // offset happens to be. Asserting the local reading rather than a
        // hardcoded UTC string keeps the test honest about what it proves.
        let kolkata: Tz = "Asia/Kolkata".parse().unwrap();
        let local = next.with_timezone(&kolkata);
        assert_eq!(local.hour(), 2);
        assert_eq!(local.minute(), 0);
        assert_eq!(explanation.timezone, "Asia/Kolkata");
    }
}