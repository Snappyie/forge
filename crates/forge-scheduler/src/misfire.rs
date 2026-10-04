//! Misfire handling (spec 09.7, 09.8).
//!
//! A schedule that was due while the system was down, or that fell behind under
//! load, must still resolve to a definite set of executions. This module turns
//! "due at T, it is now N" into the occurrences that should actually fire, and
//! records why anything was dropped so an operator can explain the gap.

use chrono::{DateTime, Duration, Utc};

use crate::schedule::OccurrenceSource;

/// Why a planned occurrence was not created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Already materialised by an earlier tick; the loop is idempotent.
    AlreadyMaterialised,
    /// Beyond the configured catch-up ceiling (spec 09.8).
    CatchUpLimit,
    /// The schedule was disabled between claiming and planning.
    ScheduleDisabled,
    /// Skipped because the occurrence falls on a holiday or blackout date.
    BlackoutOrHoliday,
    /// Skipped because the occurrence falls outside the permitted daily time window.
    OutsideTimeWindow,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            SkipReason::AlreadyMaterialised => "already materialised",
            SkipReason::CatchUpLimit => "beyond the catch-up limit",
            SkipReason::ScheduleDisabled => "schedule is disabled",
            SkipReason::BlackoutOrHoliday => "occurrence falls on a blackout date or holiday",
            SkipReason::OutsideTimeWindow => "occurrence falls outside permitted time window",
        })
    }
}

/// The occurrences to create, plus what was deliberately not created.
#[derive(Debug, Clone, PartialEq)]
pub struct OccurrencePlan {
    pub fire: Vec<DateTime<Utc>>,
    pub skipped: Vec<(DateTime<Utc>, SkipReason)>,
}

impl OccurrencePlan {
    pub fn is_empty(&self) -> bool {
        self.fire.is_empty()
    }
}

/// How missed occurrences are resolved (spec 09.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisfirePolicy {
    /// Discard missed occurrences entirely.
    Skip,
    /// Create exactly one execution for the missed period.
    FireOnce,
    /// Create one per missed occurrence, subject to the ceiling.
    CatchUp,
}

impl MisfirePolicy {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "SKIP" => MisfirePolicy::Skip,
            "CATCH_UP" => MisfirePolicy::CatchUp,
            _ => MisfirePolicy::FireOnce,
        }
    }
}

/// Safety ceiling on catch-up (spec 09.8): default 100, configurable.
pub fn default_catch_up_limit() -> u32 {
    100
}

/// Plans occurrences for a due schedule.
///
/// `already_materialised` guards idempotency: a scheduler that retries a tick,
/// or a second scheduler that somehow claimed the same row, produces the same
/// plan minus the occurrences that already exist.
///
/// `schedule` is any [`OccurrenceSource`], so cron, interval and one-time
/// schedules share one misfire/catch-up path (spec 09.7, 09.8).
pub fn plan_occurrences(
    schedule: &dyn OccurrenceSource,
    due: DateTime<Utc>,
    now: DateTime<Utc>,
    policy: MisfirePolicy,
    catch_up_limit: u32,
    already_materialised: &[DateTime<Utc>],
) -> OccurrencePlan {
    let mut plan = OccurrencePlan {
        fire: Vec::new(),
        skipped: Vec::new(),
    };

    // Idempotency first: never plan an occurrence that already exists.
    if already_materialised.contains(&due) {
        plan.skipped.push((due, SkipReason::AlreadyMaterialised));
        return plan;
    }

    // Not yet due. The scheduler only calls this with a due value, but a clock
    // that moved backwards should not produce a future execution.
    if due > now {
        plan.skipped.push((due, SkipReason::AlreadyMaterialised));
        return plan;
    }

    match policy {
        MisfirePolicy::FireOnce => {
            // One execution stands in for the whole missed period.
            plan.fire.push(due);
        }

        MisfirePolicy::Skip => {
            // Only the intended occurrence; anything missed before it is
            // discarded rather than backfilled.
            plan.fire.push(due);
        }

        MisfirePolicy::CatchUp => {
            let ceiling = catch_up_limit.max(1) as usize;
            // Reconstruct the missed window from the due occurrence up to now.
            let candidates =
                schedule.occurrences_between(due - Duration::seconds(1), now, ceiling + 1);

            for candidate in candidates.iter().take(ceiling) {
                if already_materialised.contains(candidate) {
                    plan.skipped
                        .push((*candidate, SkipReason::AlreadyMaterialised));
                } else {
                    plan.fire.push(*candidate);
                }
            }

            // Anything the ceiling excluded is reported rather than silently
            // dropped.
            for extra in candidates.iter().skip(ceiling) {
                plan.skipped.push((*extra, SkipReason::CatchUpLimit));
            }
        }
    }

    // Chronological order makes execution creation deterministic and keeps the
    // catch-up series readable in the UI.
    plan.fire.sort();
    plan.fire.dedup();
    plan.skipped.sort_by_key(|(t, _)| *t);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::{CronSchedule, IntervalSchedule, OccurrenceSource, OneTimeSchedule};

    fn cron() -> CronSchedule {
        CronSchedule::parse("0 * * * *", "UTC").unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    // AT-SCH-001: a due occurrence fires exactly once.
    #[test]
    fn due_occurrence_fires_once() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T02:00:00Z"),
            at("2026-10-03T02:00:01Z"),
            MisfirePolicy::FireOnce,
            default_catch_up_limit(),
            &[],
        );
        assert_eq!(plan.fire, vec![at("2026-10-03T02:00:00Z")]);
    }

    /// Spec 09.2 requires the loop to be idempotent.
    #[test]
    fn already_materialised_occurrence_is_not_replanned() {
        let due = at("2026-10-03T02:00:00Z");
        let plan = plan_occurrences(
            &cron(),
            due,
            at("2026-10-03T02:00:01Z"),
            MisfirePolicy::FireOnce,
            default_catch_up_limit(),
            &[due],
        );
        assert!(plan.is_empty());
        assert_eq!(plan.skipped[0].1, SkipReason::AlreadyMaterialised);
    }

    #[test]
    fn future_occurrence_does_not_fire() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T05:00:00Z"),
            at("2026-10-03T02:00:00Z"),
            MisfirePolicy::FireOnce,
            default_catch_up_limit(),
            &[],
        );
        assert!(
            plan.is_empty(),
            "a clock that moved back must not fire future work"
        );
    }

    // AT-SCH-009: catch-up obeys the maximum limit.
    #[test]
    fn catch_up_respects_the_ceiling() {
        // Hourly schedule, five hours missed, ceiling of 3.
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T00:00:00Z"),
            at("2026-10-03T05:00:00Z"),
            MisfirePolicy::CatchUp,
            3,
            &[],
        );
        assert_eq!(plan.fire.len(), 3);
        assert_eq!(plan.fire[0], at("2026-10-03T00:00:00Z"));
        assert!(
            plan.skipped
                .iter()
                .any(|(_, r)| *r == SkipReason::CatchUpLimit),
            "occurrences beyond the ceiling must be reported"
        );
    }

    #[test]
    fn catch_up_within_the_ceiling_creates_every_missed_run() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T00:00:00Z"),
            at("2026-10-03T04:30:00Z"),
            MisfirePolicy::CatchUp,
            100,
            &[],
        );
        // 00:00, 01:00, 02:00, 03:00, 04:00.
        assert_eq!(plan.fire.len(), 5);
        assert_eq!(plan.fire[4], at("2026-10-03T04:00:00Z"));
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn catch_up_skips_occurrences_already_materialised() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T00:00:00Z"),
            at("2026-10-03T02:30:00Z"),
            MisfirePolicy::CatchUp,
            100,
            &[at("2026-10-03T01:00:00Z")],
        );
        // 00:00, 01:00 (existing), 02:00.
        assert_eq!(
            plan.fire,
            vec![at("2026-10-03T00:00:00Z"), at("2026-10-03T02:00:00Z")]
        );
        assert!(plan.skipped.iter().any(
            |(t, r)| *t == at("2026-10-03T01:00:00Z") && *r == SkipReason::AlreadyMaterialised
        ));
    }

    #[test]
    fn catch_up_ceiling_of_zero_still_fires_one() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T00:00:00Z"),
            at("2026-10-03T05:00:00Z"),
            MisfirePolicy::CatchUp,
            0,
            &[],
        );
        assert_eq!(plan.fire.len(), 1, "a zero ceiling clamps to one, not zero");
    }

    /// FIRE_ONCE collapses a long outage into a single execution.
    #[test]
    fn fire_once_collapses_a_long_outage() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-01T00:00:00Z"),
            at("2026-10-03T00:00:00Z"),
            MisfirePolicy::FireOnce,
            default_catch_up_limit(),
            &[],
        );
        assert_eq!(plan.fire, vec![at("2026-10-01T00:00:00Z")]);
    }

    /// SKIP discards the backlog: only the intended occurrence fires.
    #[test]
    fn skip_policy_does_not_backfill() {
        let plan = plan_occurrences(
            &cron(),
            at("2026-10-03T00:00:00Z"),
            at("2026-10-03T05:00:00Z"),
            MisfirePolicy::Skip,
            default_catch_up_limit(),
            &[],
        );
        assert_eq!(plan.fire, vec![at("2026-10-03T00:00:00Z")]);
    }

    #[test]
    fn policy_parsing_defaults_to_fire_once() {
        assert_eq!(MisfirePolicy::parse("SKIP"), MisfirePolicy::Skip);
        assert_eq!(MisfirePolicy::parse("CATCH_UP"), MisfirePolicy::CatchUp);
        assert_eq!(MisfirePolicy::parse("FIRE_ONCE"), MisfirePolicy::FireOnce);
        assert_eq!(MisfirePolicy::parse("nonsense"), MisfirePolicy::FireOnce);
    }

    /// Spec 09.7/09.8: interval catch-up walks the series by its own period,
    /// not by cron density, and honours the same ceiling.
    #[test]
    fn interval_catch_up_uses_the_interval_period() {
        let anchor = at("2026-10-03T00:00:00Z");
        let interval = IntervalSchedule::new(600, anchor).unwrap();

        let plan = plan_occurrences(
            &interval,
            anchor,
            at("2026-10-03T00:25:00Z"),
            MisfirePolicy::CatchUp,
            100,
            &[],
        );
        assert_eq!(
            plan.fire,
            vec![
                anchor,
                at("2026-10-03T00:10:00Z"),
                at("2026-10-03T00:20:00Z"),
            ]
        );
        assert!(plan.skipped.is_empty());
    }

    #[test]
    fn interval_catch_up_respects_the_ceiling() {
        let anchor = at("2026-10-03T00:00:00Z");
        let interval = IntervalSchedule::new(60, anchor).unwrap();

        let plan = plan_occurrences(
            &interval,
            anchor,
            at("2026-10-03T00:10:00Z"),
            MisfirePolicy::CatchUp,
            3,
            &[],
        );
        assert_eq!(plan.fire.len(), 3);
        assert!(plan
            .skipped
            .iter()
            .any(|(_, reason)| *reason == SkipReason::CatchUpLimit));
    }

    /// Spec 01.5: a one-time occurrence fires once and then has no successor,
    /// which is what lets the engine complete the schedule.
    #[test]
    fn a_one_time_occurrence_fires_once_and_has_no_successor() {
        let instant = at("2026-10-03T05:00:00Z");
        let one_time = OneTimeSchedule::new(instant);

        let plan = plan_occurrences(
            &one_time,
            instant,
            at("2026-10-03T06:00:00Z"),
            MisfirePolicy::FireOnce,
            default_catch_up_limit(),
            &[],
        );
        assert_eq!(plan.fire, vec![instant]);

        assert_eq!(
            one_time.next_after(instant),
            None,
            "a fired one-time schedule has no next run"
        );
        assert!(one_time
            .occurrences_between(instant, at("2026-10-04T00:00:00Z"), 10)
            .is_empty());
    }

    #[test]
    fn skip_reasons_render_for_the_operator() {
        assert_eq!(
            SkipReason::CatchUpLimit.to_string(),
            "beyond the catch-up limit"
        );
        assert_eq!(
            SkipReason::AlreadyMaterialised.to_string(),
            "already materialised"
        );
    }
}
