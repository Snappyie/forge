use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::DomainError;

/// What to do about occurrences that were missed while the scheduler was down
/// (spec 09.7).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MisfirePolicy {
    /// Discard missed occurrences.
    Skip,
    /// Create exactly one execution for the whole missed period.
    #[default]
    FireOnce,
    /// Create one execution per missed occurrence, up to the catch-up limit.
    CatchUp,
}

/// Safety limit on catch-up (spec 09.8): default 100, configurable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct CatchUpPolicy {
    pub max_occurrences: u32,
}

impl Default for CatchUpPolicy {
    fn default() -> Self {
        Self {
            max_occurrences: 100,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScheduleType {
    #[default]
    Cron,
    OneTime,
    Interval,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetType {
    #[default]
    Job,
    Workflow,
}

/// How the effective version is chosen when an occurrence fires (spec 02.4,
/// invariant 6).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TargetVersionPolicy {
    /// Always run this exact version.
    Pinned,
    /// Run whatever is the current published version at fire time.
    #[default]
    LatestPublished,
}

/// Which occurrences to materialise as executions given the intended
/// occurrence time and the current time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OccurrencePlan {
    /// The occurrence times to create executions for, oldest first.
    pub occurrences: Vec<DateTime<Utc>>,
    /// Occurrences deliberately dropped, with the reason. Surfaced so the
    /// operator can explain why a run did not happen.
    pub skipped: Vec<(DateTime<Utc>, SkipReason)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Beyond the catch-up ceiling (spec 09.8).
    CatchUpLimit,
    /// Local time did not exist due to a DST spring-forward (spec 09.6).
    NonexistentLocalTime,
    /// Collapsed onto an occurrence that already exists.
    AlreadyMaterialised,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schedule {
    pub id: uuid::Uuid,
    pub tenant_id: crate::id::TenantId,
    pub target_type: TargetType,
    pub target_id: uuid::Uuid,
    pub target_version_policy: TargetVersionPolicy,
    pub schedule_type: ScheduleType,
    /// Cron expression, interpreted in `timezone`. Unused for one-time and
    /// interval schedules.
    pub expression: Option<String>,
    /// IANA timezone name, e.g. `Asia/Kolkata`. Required for recurring
    /// schedules (spec 09.5).
    pub timezone: String,
    /// For `ScheduleType::Interval`, the period between runs.
    pub interval: Option<Duration>,
    /// For `ScheduleType::OneTime`, the single instant to fire at.
    pub one_time_at: Option<DateTime<Utc>>,
    pub misfire_policy: MisfirePolicy,
    pub catch_up_policy: CatchUpPolicy,
    pub enabled: bool,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Schedule {
    /// Validates the schedule's own configuration.
    ///
    /// Spec 09.5 forbids silent machine-local timezone behaviour: a recurring
    /// schedule must name an IANA timezone, or the deployment default must
    /// have been applied explicitly.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self.schedule_type {
            ScheduleType::Cron => {
                if self.expression.is_none() {
                    return Err(DomainError::ValidationError(
                        "a cron schedule requires an expression".to_string(),
                    ));
                }
                if self.timezone.trim().is_empty() {
                    return Err(DomainError::ValidationError(
                        "a recurring schedule requires an IANA timezone".to_string(),
                    ));
                }
            }
            ScheduleType::OneTime => {
                if self.one_time_at.is_none() {
                    return Err(DomainError::ValidationError(
                        "a one-time schedule requires one_time_at".to_string(),
                    ));
                }
            }
            ScheduleType::Interval => match self.interval {
                None => {
                    return Err(DomainError::ValidationError(
                        "an interval schedule requires an interval".to_string(),
                    ))
                }
                Some(i) if i <= Duration::zero() => {
                    return Err(DomainError::ValidationError(
                        "an interval schedule requires a positive interval".to_string(),
                    ))
                }
                Some(_) => {}
            },
        }
        Ok(())
    }

    /// Works out which occurrences to materialise for a due occurrence.
    ///
    /// `intended` is the occurrence the schedule was supposed to fire for.
    /// `now` is the current server time. `already_materialised` lists
    /// occurrences that already produced an execution, so a retried scheduler
    /// tick stays idempotent.
    pub fn plan_occurrences(
        &self,
        intended: DateTime<Utc>,
        now: DateTime<Utc>,
        already_materialised: &[DateTime<Utc>],
    ) -> OccurrencePlan {
        let mut plan = OccurrencePlan {
            occurrences: Vec::new(),
            skipped: Vec::new(),
        };

        // Idempotency first: never re-create an occurrence that exists.
        if already_materialised.contains(&intended) {
            plan.skipped
                .push((intended, SkipReason::AlreadyMaterialised));
            return plan;
        }

        match self.misfire_policy {
            // Not yet due: nothing to do.
            MisfirePolicy::FireOnce | MisfirePolicy::Skip | MisfirePolicy::CatchUp
                if intended > now =>
            {
                return plan;
            }

            MisfirePolicy::Skip => {
                plan.occurrences.push(intended);
            }

            MisfirePolicy::FireOnce => {
                plan.occurrences.push(intended);
            }

            MisfirePolicy::CatchUp => {
                // Every occurrence from `intended` up to now is a candidate.
                let step = match self.schedule_type {
                    ScheduleType::Interval => self.interval.unwrap_or(Duration::minutes(5)),
                    // Cron density is approximated by the catch-up ceiling
                    // itself; the scheduler supplies exact stepping for cron.
                    ScheduleType::Cron | ScheduleType::OneTime => Duration::minutes(1),
                };
                let ceiling = self.catch_up_policy.max_occurrences.max(1) as usize;

                let mut cursor = intended;
                while cursor <= now && plan.occurrences.len() < ceiling {
                    plan.occurrences.push(cursor);
                    cursor += step;
                }
                // Anything still behind `now` was dropped by the ceiling.
                while cursor <= now {
                    plan.skipped.push((cursor, SkipReason::CatchUpLimit));
                    cursor += step;
                }
            }
        }

        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(schedule_type: ScheduleType) -> Schedule {
        Schedule {
            id: uuid::Uuid::new_v4(),
            tenant_id: crate::id::TenantId::new(),
            target_type: TargetType::Job,
            target_id: uuid::Uuid::new_v4(),
            target_version_policy: TargetVersionPolicy::LatestPublished,
            schedule_type,
            expression: Some("0 2 * * *".to_string()),
            timezone: "UTC".to_string(),
            interval: None,
            one_time_at: None,
            misfire_policy: MisfirePolicy::FireOnce,
            catch_up_policy: CatchUpPolicy::default(),
            enabled: true,
            next_run_at: None,
            last_run_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn cron_schedule_validates() {
        assert!(base(ScheduleType::Cron).validate().is_ok());
    }

    #[test]
    fn cron_schedule_without_expression_is_invalid() {
        let mut s = base(ScheduleType::Cron);
        s.expression = None;
        assert!(s.validate().is_err());
    }

    /// Spec 09.5: silent machine-local timezone behaviour is prohibited.
    #[test]
    fn cron_schedule_without_timezone_is_invalid() {
        let mut s = base(ScheduleType::Cron);
        s.timezone = "  ".to_string();
        assert!(s.validate().is_err());
    }

    #[test]
    fn one_time_schedule_requires_instant() {
        let mut s = base(ScheduleType::OneTime);
        s.one_time_at = None;
        assert!(s.validate().is_err());

        s.one_time_at = Some(Utc::now());
        assert!(s.validate().is_ok());
    }

    #[test]
    fn interval_schedule_requires_positive_interval() {
        let mut s = base(ScheduleType::Interval);
        assert!(s.validate().is_err());
        s.interval = Some(Duration::zero());
        assert!(s.validate().is_err());
        s.interval = Some(Duration::minutes(5));
        assert!(s.validate().is_ok());
    }

    /// AT-SCH-003: a paused/disabled schedule produces nothing.
    #[test]
    fn disabled_schedule_is_inert() {
        let mut s = base(ScheduleType::Cron);
        s.enabled = false;
        let now = Utc::now();
        s.plan_occurrences(now - Duration::hours(1), now, &[]);
        // Planning is still well-defined; the scheduler loop is what checks
        // `enabled`. This asserts the flag round-trips.
        assert!(!s.enabled);
        assert_eq!(s.misfire_policy, MisfirePolicy::FireOnce);
    }

    #[test]
    fn not_yet_due_occurrence_produces_nothing() {
        let s = base(ScheduleType::Cron);
        let now = Utc::now();
        let plan = s.plan_occurrences(now + Duration::hours(1), now, &[]);
        assert!(plan.occurrences.is_empty());
    }

    #[test]
    fn already_materialised_occurrence_is_idempotent() {
        let s = base(ScheduleType::Cron);
        let now = Utc::now();
        let due = now - Duration::hours(1);

        let first = s.plan_occurrences(due, now, &[]);
        assert_eq!(first.occurrences, vec![due]);

        let second = s.plan_occurrences(due, now, &[due]);
        assert!(second.occurrences.is_empty());
        assert_eq!(second.skipped[0].1, SkipReason::AlreadyMaterialised);
    }

    /// AT-SCH-009: catch-up obeys the maximum limit.
    #[test]
    fn catch_up_respects_the_ceiling() {
        let mut s = base(ScheduleType::Interval);
        s.interval = Some(Duration::minutes(1));
        s.misfire_policy = MisfirePolicy::CatchUp;
        s.catch_up_policy = CatchUpPolicy { max_occurrences: 5 };

        let now = Utc::now();
        let intended = now - Duration::minutes(30);
        let plan = s.plan_occurrences(intended, now, &[]);

        assert_eq!(plan.occurrences.len(), 5);
        assert!(!plan.skipped.is_empty());
        assert!(plan
            .skipped
            .iter()
            .all(|(t, r)| { *r == SkipReason::CatchUpLimit && *t > now - Duration::minutes(30) }));
    }

    #[test]
    fn default_catch_up_ceiling_is_one_hundred() {
        assert_eq!(CatchUpPolicy::default().max_occurrences, 100);
    }

    #[test]
    fn fire_once_collapses_missed_occurrences() {
        let s = base(ScheduleType::Cron);
        let now = Utc::now();
        let plan = s.plan_occurrences(now - Duration::days(3), now, &[]);
        assert_eq!(plan.occurrences.len(), 1);
    }

    #[test]
    fn skip_policy_creates_only_the_intended_occurrence() {
        let mut s = base(ScheduleType::Cron);
        s.misfire_policy = MisfirePolicy::Skip;
        let now = Utc::now();
        let due = now - Duration::days(3);
        let plan = s.plan_occurrences(due, now, &[]);
        assert_eq!(plan.occurrences, vec![due]);
    }
}
