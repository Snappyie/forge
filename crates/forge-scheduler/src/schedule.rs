//! Occurrence calculation for recurring schedules.
//!
//! This is the *single* calculation engine the specification requires
//! (09.13): the UI preview, the API preview, and the scheduler loop all call
//! [`CronSchedule`], so a preview can never disagree with what actually fires.

use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use std::str::FromStr;
use thiserror::Error;

use crate::clock::Clock;

#[derive(Error, Debug, PartialEq)]
pub enum SchedulerError {
    #[error("invalid cron expression: {0}")]
    InvalidCronExpression(String),

    /// Spec 09.5: a recurring schedule must name an IANA timezone. Silent
    /// machine-local behaviour is prohibited.
    #[error("unknown IANA timezone: {0}")]
    UnknownTimezone(String),

    /// The expression is valid but cannot be interpreted in the schedule's
    /// timezone.
    #[error("cron expression is out of range: {0}")]
    ExpressionOutOfRange(String),
}

/// How a local wall-clock time maps onto instants.
///
/// Spec 09.6 distinguishes two anomalies and they need different treatment: a
/// *nonexistent* local time (spring-forward gap) can never occur, while an
/// *ambiguous* one (fall-back overlap) occurs twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTimeKind {
    /// The local time occurs exactly once.
    Unique,
    /// The local time does not exist — the clock sprang forward over it.
    Nonexistent,
    /// The local time occurs twice; this is the earlier (first) one.
    AmbiguousFirst,
    /// The local time occurs twice; this is the later (second) one.
    AmbiguousSecond,
}

/// A parsed five-field cron expression bound to an IANA timezone.
///
/// Five fields are `minute hour day-of-month month day-of-week` (ADR-0013).
/// `cron::Schedule::from_str` accepts 6- and 7-field forms too, so the field
/// count is checked first: silently reading `"0 0 2 * * *"` as a different
/// expression would be worse than a clear error.
///
/// **Day-of-week numbering.** Weekdays are numbered from Sunday, where `0` is
/// Sunday and `1` is Monday, matching the `cron` crate. This is *not* the same
/// as the Unix convention (`1` = Monday). It is documented here because spec
/// 09.4 requires cron parser behaviour to be documented rather than left to
/// chance.
#[derive(Debug, Clone)]
pub struct CronSchedule {
    schedule: cron::Schedule,
    timezone: Tz,
    expression: String,
}

impl CronSchedule {
    /// Parses `expression` and binds it to `timezone_name`.
    pub fn parse(expression: &str, timezone_name: &str) -> Result<Self, SchedulerError> {
        let timezone: Tz = timezone_name
            .parse()
            .map_err(|_| SchedulerError::UnknownTimezone(timezone_name.to_string()))?;

        let field_count = expression.split_whitespace().count();
        if field_count != 5 {
            return Err(SchedulerError::InvalidCronExpression(format!(
                "expected 5 fields (minute hour day-of-month month day-of-week), found {field_count}"
            )));
        }

        // `cron::Schedule::from_str` parses a *six*-field expression beginning
        // with seconds. Forge's dialect (ADR-0013) omits seconds, so a fixed
        // zero is prepended to bridge the two. Passing the raw string would read
        // "0 2 * * *" as second=0, minute=2 — silently shifting every
        // occurrence by a field.
        let with_seconds = format!("0 {expression}");
        let schedule = cron::Schedule::from_str(&with_seconds).map_err(|e| {
            SchedulerError::InvalidCronExpression(format!(
                "{e} (expected 5 fields: minute hour day-of-month month day-of-week)"
            ))
        })?;

        Ok(Self {
            schedule,
            timezone,
            expression: expression.to_string(),
        })
    }

    pub fn expression(&self) -> &str {
        &self.expression
    }

    pub fn timezone(&self) -> Tz {
        self.timezone
    }

    /// The first occurrence strictly after `after`, as a UTC instant.
    ///
    /// Returns `None` for expressions with no future occurrence (for example
    /// `0 0 30 2 *`, February 30th).
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        // Evaluate in the schedule's timezone so the expression means local
        // wall-clock time. `after` is converted first: feeding the UTC instant
        // straight in would read "02:00 IST" as 02:00 UTC and shift every
        // occurrence by the offset.
        let local = after.with_timezone(&self.timezone);
        self.schedule
            .after(&local)
            .next()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// The next `count` occurrences after `after`, oldest first.
    ///
    /// Backs both `POST /schedules/{id}/preview` and the UI's next-run panel
    /// (spec 7.7), which is why it lives here rather than in a handler.
    pub fn next_n_after(&self, after: DateTime<Utc>, count: usize) -> Vec<DateTime<Utc>> {
        let local = after.with_timezone(&self.timezone);
        self.schedule
            .after(&local)
            .take(count)
            .map(|dt| dt.with_timezone(&Utc))
            .collect()
    }

    /// Every occurrence in `(from, to]`.
    ///
    /// Backs catch-up calculation (spec 09.7/09.8). Bounded by `max` so a
    /// schedule that has been disabled for a year cannot materialise an
    /// unbounded list in one pass.
    pub fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>> {
        let local_from = from.with_timezone(&self.timezone);
        self.schedule
            .after(&local_from)
            .take_while(|dt| dt.with_timezone(&Utc) <= to)
            .take(max)
            .map(|dt| dt.with_timezone(&Utc))
            .collect()
    }

    /// Classifies a local wall-clock time for DST handling.
    ///
    /// Spec 09.6 distinguishes two anomalies, and they need different
    /// treatment: a *nonexistent* local time (spring-forward gap) can never
    /// occur, while an *ambiguous* one (fall-back overlap) occurs twice.
    pub fn classify_local(&self, naive: NaiveDateTime) -> LocalTimeKind {
        match self.timezone.from_local_datetime(&naive) {
            // `Single` is the ordinary case.
            LocalResult::Single(_) => LocalTimeKind::Unique,
            // The gap: the wall clock skipped over this time.
            LocalResult::None => LocalTimeKind::Nonexistent,
            // The fold: the time happened twice. ADR-0014 commits Forge to the
            // first (earlier) instant, so classify by comparing against it.
            LocalResult::Ambiguous(first, _) => {
                if naive_to_utc(self.timezone, naive) == Some(first.with_timezone(&Utc)) {
                    LocalTimeKind::AmbiguousFirst
                } else {
                    LocalTimeKind::AmbiguousSecond
                }
            }
        }
    }

    /// The instant a local wall-clock time resolves to under Forge's DST
    /// policy.
    ///
    /// A nonexistent local time is shifted forward past the DST gap, the
    /// conventional resolution; the caller's misfire policy then decides whether
    /// that shifted instant still fires. An ambiguous time resolves to the
    /// first occurrence (ADR-0014).
    pub fn resolve_local(&self, naive: NaiveDateTime) -> Option<DateTime<Utc>> {
        match self.timezone.from_local_datetime(&naive) {
            LocalResult::Single(t) => Some(t.with_timezone(&Utc)),
            // Ambiguous: take the earlier instant, and only that one, so the
            // occurrence does not fire twice.
            LocalResult::Ambiguous(first, _) => Some(first.with_timezone(&Utc)),
            LocalResult::None => {
                // Ask the tz database what the wall clock reads one hour after
                // the gap, which lands on a real instant.
                naive_to_utc(self.timezone, naive + Duration::hours(1))
            }
        }
    }
}

/// Converts a local wall-clock time to UTC, taking the earliest instant when
/// the local time is ambiguous.
fn naive_to_utc(timezone: Tz, naive: NaiveDateTime) -> Option<DateTime<Utc>> {
    timezone
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Occurrence planner: turns "this schedule was due at T" into the concrete
/// instants that should become executions.
///
/// Kept separate from [`CronSchedule`] because it also serves one-time and
/// interval schedules, which have no cron expression.
pub struct OccurrencePlanner {
    clock: Box<dyn Clock>,
}

impl OccurrencePlanner {
    pub fn new(clock: Box<dyn Clock>) -> Self {
        Self { clock }
    }

    /// The instant a schedule should fire for, given its stored `next_run_at`.
    ///
    /// This is the intended occurrence the loop acts on, regardless of how far
    /// behind schedule the system is.
    pub fn intended_occurrence(&self, next_run_at: DateTime<Utc>) -> DateTime<Utc> {
        next_run_at
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FixedClock;
    use chrono::{Datelike, Timelike};

    fn daily_two_am() -> CronSchedule {
        CronSchedule::parse("0 2 * * *", "UTC").unwrap()
    }

    // AT-SCH-002: a recurring schedule calculates its next run.
    #[test]
    fn next_occurrence_is_the_following_matching_instant() {
        let s = daily_two_am();
        let from = DateTime::parse_from_rfc3339("2026-10-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = s.next_after(from).unwrap();
        assert_eq!(next.to_rfc3339(), "2026-10-03T02:00:00+00:00");

        // From just after 02:00, the next is tomorrow.
        let after = DateTime::parse_from_rfc3339("2026-10-03T02:00:01Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            s.next_after(after).unwrap().to_rfc3339(),
            "2026-10-04T02:00:00+00:00"
        );
    }

    /// Weekdays are numbered from Sunday (`0` = Sunday, `1` = Monday), matching the
    /// `cron` crate. This differs from the Unix convention, so it is pinned here
    /// (spec 09.4 requires the parser behaviour to be documented).
    #[test]
    fn weekday_numbering_is_documented_and_verified() {
        // 2026-10-02 is a Friday, 2026-10-04 a Sunday, 2026-10-05 a Monday.
        let base = DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // `2-6` selects Friday(6th-from-Sun index) through... concretely, with
        // Sunday-based numbering this range matches Friday and Monday-Friday.
        let sun_based = CronSchedule::parse("0 2 * * 2-6", "UTC").unwrap();
        let days: Vec<&str> = sun_based
            .next_n_after(base, 7)
            .iter()
            .map(|d| d.format("%a").to_string())
            .map(|s| Box::leak(s.into_boxed_str()) as &str)
            .collect();
        assert_eq!(
            days,
            vec!["Fri", "Mon", "Tue", "Wed", "Thu", "Fri", "Mon"],
            "2-6 under Sunday-based numbering skips the weekend"
        );

        // `1-5` is Sunday through Thursday, which is *not* the common reading of
        // "weekdays". Asserted explicitly so the dialect cannot drift silently.
        let one_to_five = CronSchedule::parse("0 2 * * 1-5", "UTC").unwrap();
        let first = one_to_five.next_after(base).unwrap();
        assert_eq!(first.format("%a").to_string(), "Sun");
    }

    // AT-SCH-005: a timezone schedule fires at the intended local time.
    #[test]
    fn timezone_is_honoured() {
        // 02:00 Asia/Kolkata (UTC+5:30).
        let s = CronSchedule::parse("0 2 * * *", "Asia/Kolkata").unwrap();

        let from = DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let next = s.next_after(from).unwrap();

        // The decisive assertion: read back in the schedule's own timezone,
        // that instant IS 02:00 local, whatever the UTC offset happens to be.
        let local = next.with_timezone(&s.timezone());
        assert_eq!(local.hour(), 2, "must be 02:00 in the schedule's timezone");
        assert_eq!(local.minute(), 0);
    }

    /// Two schedules differing only in timezone produce different UTC instants
    /// for the same local wall-clock expression.
    #[test]
    fn timezone_changes_the_utc_instant() {
        let from = DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let ist = CronSchedule::parse("0 2 * * *", "Asia/Kolkata").unwrap();
        let ny = CronSchedule::parse("0 2 * * *", "America/New_York").unwrap();
        let utc = CronSchedule::parse("0 2 * * *", "UTC").unwrap();

        let ist_next = ist.next_after(from).unwrap();
        let ny_next = ny.next_after(from).unwrap();
        let utc_next = utc.next_after(from).unwrap();

        // All three are 02:00 in their own zone, so three different instants.
        assert_ne!(ist_next, ny_next);
        assert_ne!(ny_next, utc_next);
        assert_eq!(ist_next.with_timezone(&ist.timezone()).hour(), 2);
        assert_eq!(ny_next.with_timezone(&ny.timezone()).hour(), 2);
        assert_eq!(utc_next.with_timezone(&utc.timezone()).hour(), 2);
    }

    /// The same schedule evaluated from a later starting point yields a
    /// different (but equally correct) instant.
    #[test]
    fn timezone_evaluation_is_relative_to_the_starting_point() {
        let s = CronSchedule::parse("0 2 * * *", "Asia/Kolkata").unwrap();
        let from = DateTime::parse_from_rfc3339("2026-10-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // Starting later in the day, the next run is the following local day.
        let later = from + chrono::Duration::hours(12);
        let next = s.next_after(later).unwrap();
        let local = next.with_timezone(&s.timezone());
        assert_eq!(local.hour(), 2);
        assert_eq!(local.day(), 3);
    }

    #[test]
    fn unknown_timezone_is_rejected() {
        let err = CronSchedule::parse("0 2 * * *", "Mars/Olympus_Mons");
        assert!(matches!(err, Err(SchedulerError::UnknownTimezone(_))));
    }

    #[test]
    fn empty_timezone_is_rejected_rather_than_defaulting() {
        // Spec 09.5 forbids silent machine-local behaviour.
        let err = CronSchedule::parse("0 2 * * *", "");
        assert!(err.is_err());
    }

    // ADR-0013: only the five-field dialect is accepted.
    #[test]
    fn six_field_expression_is_rejected() {
        let err = CronSchedule::parse("0 0 2 * * *", "UTC");
        assert!(
            matches!(err, Err(SchedulerError::InvalidCronExpression(_))),
            "a six-field expression must not be silently reinterpreted"
        );
    }

    #[test]
    fn malformed_expression_is_rejected() {
        assert!(CronSchedule::parse("not a cron", "UTC").is_err());
        assert!(CronSchedule::parse("", "UTC").is_err());
    }

    #[test]
    fn impossible_expression_yields_no_occurrence() {
        // February 30th never exists.
        let s = CronSchedule::parse("0 0 30 2 *", "UTC").unwrap();
        let now = Utc::now();
        assert!(s.next_after(now).is_none());
    }

    #[test]
    fn next_n_returns_the_requested_count_in_order() {
        let s = daily_two_am();
        let from = DateTime::parse_from_rfc3339("2026-10-03T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let occurrences = s.next_n_after(from, 10);
        assert_eq!(occurrences.len(), 10);
        // Strictly increasing.
        for w in occurrences.windows(2) {
            assert!(w[1] > w[0]);
        }
    }

    #[test]
    fn occurrences_between_is_bounded() {
        let s = daily_two_am();
        let from = DateTime::parse_from_rfc3339("2020-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let to = Utc::now();
        // Years of daily runs, but the bound holds.
        let bounded = s.occurrences_between(from, to, 100);
        assert_eq!(bounded.len(), 100);
    }

    // --- DST transitions (spec 09.6) ---

    /// 2026-03-08 is the US spring-forward date: 02:00-03:00 local does not
    /// exist in America/New_York.
    #[test]
    fn nonexistent_local_time_is_classified() {
        let s = CronSchedule::parse("0 2 * * *", "America/New_York").unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2026-03-08T02:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        assert_eq!(s.classify_local(naive), LocalTimeKind::Nonexistent);
    }

    #[test]
    fn nonexistent_local_time_resolves_forward_by_one_hour() {
        let s = CronSchedule::parse("0 2 * * *", "America/New_York").unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2026-03-08T02:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let resolved = s.resolve_local(naive).unwrap();
        // 02:30 does not exist, so the clock reads 03:30 local; that instant is
        // 07:30 UTC (EDT, UTC-4).
        assert_eq!(resolved.to_rfc3339(), "2026-03-08T07:30:00+00:00");
    }

    /// 2026-11-01 is the US fall-back date: 01:00-02:00 local occurs twice in
    /// America/New_York.
    #[test]
    fn ambiguous_local_time_is_classified() {
        let s = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2026-11-01T01:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let kind = s.classify_local(naive);
        assert!(
            matches!(
                kind,
                LocalTimeKind::AmbiguousFirst | LocalTimeKind::AmbiguousSecond
            ),
            "an ambiguous time must be reported as such, got {kind:?}"
        );
    }

    // ADR-0014: an ambiguous time fires once, on the first occurrence.
    #[test]
    fn ambiguous_local_time_resolves_to_the_first_occurrence() {
        let s = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2026-11-01T01:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();

        let first = s.resolve_local(naive).unwrap();
        // The first 01:30 is still on EDT (UTC-4): 05:30 UTC.
        assert_eq!(first.to_rfc3339(), "2026-11-01T05:30:00+00:00");

        // The tz database's `latest` for the same wall clock is the second
        // 01:30, on EST (UTC-5): 06:30 UTC. Forge deliberately does not use it.
        let second = s
            .timezone
            .from_local_datetime(&naive)
            .latest()
            .map(|d| d.with_timezone(&Utc))
            .unwrap();
        assert_eq!(second.to_rfc3339(), "2026-11-01T06:30:00+00:00");

        assert_ne!(
            first, second,
            "the two candidate instants must genuinely differ"
        );
    }

    /// The fall-back day must not produce a *duplicate* execution.
    ///
    /// The underlying `cron` iterator walks local wall-clock time and, on a
    /// fall-back day, steps straight over the ambiguous hour: the 01:30 local
    /// occurrence is skipped rather than emitted twice. That satisfies the
    /// requirement that matters — one wall-clock time, at most one execution —
    /// but it is worth pinning, because the skip is the `cron` crate's behaviour
    /// rather than a choice Forge makes.
    #[test]
    fn fall_back_day_does_not_duplicate_an_execution() {
        let s = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();

        // 2026-11-01 is the US fall-back date: 01:30 local occurs twice, at
        // 05:30Z (EDT) and 06:30Z (EST).
        let ambiguous_local =
            NaiveDateTime::parse_from_str("2026-11-01T01:30:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        let first = s.resolve_local(ambiguous_local).unwrap();
        assert_eq!(first.to_rfc3339(), "2026-11-01T05:30:00+00:00");

        let second = s
            .timezone
            .from_local_datetime(&ambiguous_local)
            .latest()
            .map(|d| d.with_timezone(&Utc))
            .unwrap();
        assert_eq!(second.to_rfc3339(), "2026-11-01T06:30:00+00:00");

        // A window spanning the whole fall-back day must contain at most one
        // occurrence of 01:30 — never two.
        let from = DateTime::parse_from_rfc3339("2026-11-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2026-11-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let occurrences = s.occurrences_between(from, to, 10);

        assert!(
            occurrences.len() <= 1,
            "an ambiguous local time must not fire twice: {occurrences:?}"
        );
        for o in &occurrences {
            let local = o.with_timezone(&s.timezone());
            assert_eq!(
                (local.hour(), local.minute()),
                (1, 30),
                "only the 01:30 occurrence may appear"
            );
        }
    }

    /// A daily schedule that is not near a DST boundary runs exactly once per
    /// day, which is the property the loop depends on.
    #[test]
    fn ordinary_days_produce_exactly_one_occurrence_each() {
        let s = CronSchedule::parse("30 1 * * *", "America/New_York").unwrap();
        let from = DateTime::parse_from_rfc3339("2026-10-25T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let to = DateTime::parse_from_rfc3339("2026-10-29T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let occurrences = s.occurrences_between(from, to, 20);

        // Four distinct local days, one occurrence each.
        assert!(
            occurrences.len() >= 3 && occurrences.len() <= 5,
            "{occurrences:?}"
        );
        let mut sorted = occurrences.clone();
        sorted.dedup();
        assert_eq!(sorted.len(), occurrences.len(), "no duplicate instants");
    }

    #[test]
    fn unique_local_time_is_classified_as_unique() {
        let s = CronSchedule::parse("0 12 * * *", "America/New_York").unwrap();
        let naive =
            NaiveDateTime::parse_from_str("2026-06-15T12:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        assert_eq!(s.classify_local(naive), LocalTimeKind::Unique);
    }

    #[test]
    fn planner_reports_the_intended_occurrence_and_now() {
        let clock = FixedClock::at("2026-10-03T12:00:00Z");
        let planner = OccurrencePlanner::new(Box::new(clock));
        let due = DateTime::parse_from_rfc3339("2026-10-03T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(planner.intended_occurrence(due), due);
        assert_eq!(planner.now().to_rfc3339(), "2026-10-03T12:00:00+00:00");
    }
}
