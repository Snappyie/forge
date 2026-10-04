//! Occurrence calculation for recurring schedules.
//!
//! This is the *single* calculation engine the specification requires
//! (09.13): the UI preview, the API preview, and the scheduler loop all call
//! [`CronSchedule`], so a preview can never disagree with what actually fires.

use chrono::{DateTime, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use forge_domain::ScheduleType;
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

    /// A cron schedule with no expression cannot be evaluated. Stored rows can
    /// hold this state, so it is a handled configuration error rather than an
    /// impossibility.
    #[error("a cron schedule requires an expression")]
    MissingCronExpression,

    /// Spec 01.5: a fixed interval schedule needs a positive period. A
    /// non-positive stored value is disabled rather than looped on.
    #[error("an interval schedule requires a positive period in seconds, got {0}")]
    InvalidInterval(i64),

    #[error("an interval schedule requires interval_seconds")]
    MissingInterval,

    /// Spec 01.5: a one-time schedule fires at exactly one instant.
    #[error("a one-time schedule requires one_time_at")]
    MissingOneTimeAt,
}

/// Validates that `name` is a known IANA timezone.
///
/// Exposed for schedule kinds whose occurrence arithmetic is timezone-free but
/// which are still recurring, so spec 09.5's requirement that every recurring
/// schedule names a real zone can be enforced at creation.
pub fn validate_timezone(name: &str) -> Result<(), SchedulerError> {
    name.parse::<Tz>()
        .map(|_| ())
        .map_err(|_| SchedulerError::UnknownTimezone(name.to_string()))
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
            // `cron` 0.12 renders every parse failure as
            // "Invalid expression: Invalid cron expression." — no detail about
            // which field failed. Forwarding that produced
            // "invalid cron expression: Invalid expression: Invalid cron
            // expression.", which names the same problem three times and tells
            // the reader nothing. Anything that is only the crate's own
            // boilerplate is dropped in favour of what Forge expected.
            let raw = e.to_string();
            const BOILERPLATE: [&str; 3] = [
                "Invalid expression: Invalid cron expression.",
                "Invalid cron expression.",
                "Invalid expression.",
            ];
            let is_boilerplate = BOILERPLATE
                .iter()
                .any(|b| raw.trim().eq_ignore_ascii_case(b));
            let detail = if is_boilerplate {
                String::new()
            } else {
                raw.trim().trim_end_matches('.').to_string()
            };
            SchedulerError::InvalidCronExpression(if detail.is_empty() {
                "a field is out of range (expected 5 fields: minute hour day-of-month month day-of-week)".to_string()
            } else {
                format!("{detail} (expected 5 fields: minute hour day-of-month month day-of-week)")
            })
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

/// A source of occurrence instants for one schedule kind.
///
/// Spec 09.13 requires one calculation engine for the preview, the API and the
/// scheduler loop; this trait is the seam that lets all three kinds flow
/// through the same misfire/catch-up planning without each caller matching on
/// `schedule_type`.
pub trait OccurrenceSource {
    /// The first occurrence strictly after `after`.
    fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>>;

    /// Every occurrence in `(from, to]`, bounded by `max`.
    fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>>;

    /// The next `count` occurrences after `after`, oldest first.
    fn next_n_after(&self, after: DateTime<Utc>, count: usize) -> Vec<DateTime<Utc>> {
        let mut occurrences = Vec::new();
        let mut cursor = after;
        while occurrences.len() < count {
            match self.next_after(cursor) {
                Some(next) => {
                    occurrences.push(next);
                    cursor = next;
                }
                None => break,
            }
        }
        occurrences
    }
}

impl OccurrenceSource for CronSchedule {
    fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        CronSchedule::next_after(self, after)
    }

    fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>> {
        CronSchedule::occurrences_between(self, from, to, max)
    }

    fn next_n_after(&self, after: DateTime<Utc>, count: usize) -> Vec<DateTime<Utc>> {
        CronSchedule::next_n_after(self, after, count)
    }
}

/// A fixed-interval schedule (spec 01.5): one occurrence every `interval`,
/// counted from `anchor`.
///
/// `anchor` is the schedule's stored `next_run_at`, so the series survives
/// restarts and never re-anchors on a process start. Arithmetic steps over the
/// period rather than iterating: a schedule that is a year behind must not
/// walk a year of missed occurrences to answer "when is the next one".
#[derive(Debug, Clone)]
pub struct IntervalSchedule {
    interval: Duration,
    anchor: DateTime<Utc>,
}

impl IntervalSchedule {
    /// Builds an interval series. A non-positive period is rejected here so
    /// the engine disables such a row deterministically instead of looping.
    pub fn new(interval_seconds: i64, anchor: DateTime<Utc>) -> Result<Self, SchedulerError> {
        if interval_seconds <= 0 {
            return Err(SchedulerError::InvalidInterval(interval_seconds));
        }
        Ok(Self {
            interval: Duration::seconds(interval_seconds),
            anchor,
        })
    }

    pub fn interval_seconds(&self) -> i64 {
        self.interval.num_seconds()
    }

    pub fn anchor(&self) -> DateTime<Utc> {
        self.anchor
    }

    /// The index of the first occurrence strictly after `after`.
    fn first_index_after(&self, after: DateTime<Utc>) -> i64 {
        let delta = after - self.anchor;
        if delta < Duration::zero() {
            return 0;
        }
        let period = self.interval.num_microseconds().unwrap_or(1).max(1);
        let elapsed = delta.num_microseconds().unwrap_or(i64::MAX);
        elapsed / period + 1
    }

    fn occurrence_at(&self, index: i64) -> Option<DateTime<Utc>> {
        let period = self.interval.num_microseconds()?;
        let offset = period.checked_mul(index)?;
        self.anchor
            .checked_add_signed(Duration::microseconds(offset))
    }
}

impl OccurrenceSource for IntervalSchedule {
    fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.occurrence_at(self.first_index_after(after))
    }

    fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>> {
        let mut occurrences = Vec::new();
        let mut index = self.first_index_after(from);
        while occurrences.len() < max {
            // A `None` here means the instant left `DateTime`'s range; stop
            // rather than producing a wrong occurrence.
            let Some(at) = self.occurrence_at(index) else {
                break;
            };
            if at > to {
                break;
            }
            occurrences.push(at);
            index += 1;
        }
        occurrences
    }
}

/// A one-time schedule (spec 01.5): exactly one occurrence, at a fixed UTC
/// instant.
#[derive(Debug, Clone)]
pub struct OneTimeSchedule {
    at: DateTime<Utc>,
}

impl OneTimeSchedule {
    pub fn new(at: DateTime<Utc>) -> Self {
        Self { at }
    }

    pub fn at(&self) -> DateTime<Utc> {
        self.at
    }
}

impl OccurrenceSource for OneTimeSchedule {
    fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        // Only the future fires; once the instant has passed there is no next
        // occurrence, which is what makes completion terminal.
        (self.at > after).then_some(self.at)
    }

    fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>> {
        if max > 0 && self.at > from && self.at <= to {
            vec![self.at]
        } else {
            Vec::new()
        }
    }
}

/// The occurrence calculator for a schedule, selected by its kind.
///
/// The cron variant is boxed because a parsed `cron::Schedule` is an order of
/// magnitude larger than the other two, and this value is carried through
/// every step of the evaluation loop.
#[derive(Debug, Clone)]
pub enum OccurrenceCalculator {
    Cron(Box<CronSchedule>),
    Interval(IntervalSchedule),
    OneTime(OneTimeSchedule),
}

impl OccurrenceCalculator {
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Self::Cron(cron) => cron.next_after(after),
            Self::Interval(interval) => interval.next_after(after),
            Self::OneTime(one_time) => one_time.next_after(after),
        }
    }

    pub fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>> {
        match self {
            Self::Cron(cron) => cron.occurrences_between(from, to, max),
            Self::Interval(interval) => interval.occurrences_between(from, to, max),
            Self::OneTime(one_time) => one_time.occurrences_between(from, to, max),
        }
    }

    pub fn next_n_after(&self, after: DateTime<Utc>, count: usize) -> Vec<DateTime<Utc>> {
        // Walked through `OccurrenceSource::next_after`, so every kind uses the
        // one series definition. Deliberately not routed back through an
        // `OccurrenceSource::next_n_after` override: that override would call
        // this method, and this method that override.
        let mut occurrences = Vec::new();
        let mut cursor = after;
        while occurrences.len() < count {
            match self.next_after(cursor) {
                Some(next) => {
                    occurrences.push(next);
                    cursor = next;
                }
                None => break,
            }
        }
        occurrences
    }
}

impl OccurrenceSource for OccurrenceCalculator {
    fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        OccurrenceCalculator::next_after(self, after)
    }

    fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        max: usize,
    ) -> Vec<DateTime<Utc>> {
        OccurrenceCalculator::occurrences_between(self, from, to, max)
    }
}

/// A schedule's recurrence configuration, independent of how it is stored.
///
/// The API and the engine both build one of these and call
/// [`RecurrenceSpec::calculator`], which is what keeps a preview and the real
/// loop on the same engine (spec 09.13).
#[derive(Debug, Clone)]
pub struct RecurrenceSpec {
    pub schedule_type: ScheduleType,
    pub expression: Option<String>,
    pub timezone: String,
    pub interval_seconds: Option<i64>,
    pub one_time_at: Option<DateTime<Utc>>,
    pub blackout_dates: Vec<String>,
    pub time_window: Option<forge_domain::TimeWindow>,
}

impl RecurrenceSpec {
    pub fn cron(expression: &str, timezone: &str) -> Self {
        Self {
            schedule_type: ScheduleType::Cron,
            expression: Some(expression.to_string()),
            timezone: timezone.to_string(),
            interval_seconds: None,
            one_time_at: None,
            blackout_dates: Vec::new(),
            time_window: None,
        }
    }

    pub fn interval(interval_seconds: i64, timezone: &str) -> Self {
        Self {
            schedule_type: ScheduleType::Interval,
            expression: None,
            timezone: timezone.to_string(),
            interval_seconds: Some(interval_seconds),
            one_time_at: None,
            blackout_dates: Vec::new(),
            time_window: None,
        }
    }

    pub fn one_time(at: DateTime<Utc>, timezone: &str) -> Self {
        Self {
            schedule_type: ScheduleType::OneTime,
            expression: None,
            timezone: timezone.to_string(),
            interval_seconds: None,
            one_time_at: Some(at),
            blackout_dates: Vec::new(),
            time_window: None,
        }
    }

    pub fn with_blackout_dates(mut self, dates: Vec<String>) -> Self {
        self.blackout_dates = dates;
        self
    }

    pub fn with_time_window(mut self, window: forge_domain::TimeWindow) -> Self {
        self.time_window = Some(window);
        self
    }

    /// Checks if a proposed run instant falls on a blackout date or outside the daily time window.
    pub fn exclusion_reason(&self, instant: DateTime<Utc>) -> Option<crate::misfire::SkipReason> {
        use chrono::Timelike;
        let tz: Tz = self.timezone.parse().ok()?;
        let local = instant.with_timezone(&tz);

        let date_str = local.format("%Y-%m-%d").to_string();
        if self.blackout_dates.iter().any(|d| d.trim() == date_str) {
            return Some(crate::misfire::SkipReason::BlackoutOrHoliday);
        }

        if let Some(window) = self.time_window {
            let hour = local.hour() as u8;
            let within = if window.start_hour <= window.end_hour {
                hour >= window.start_hour && hour < window.end_hour
            } else {
                hour >= window.start_hour || hour < window.end_hour
            };
            if !within {
                return Some(crate::misfire::SkipReason::OutsideTimeWindow);
            }
        }

        None
    }

    /// Builds the calculator for this configuration.
    ///
    /// `anchor` is the instant an interval series counts from — the schedule's
    /// stored `next_run_at` — and is ignored by absolute kinds (cron, one-time).
    /// A configuration that cannot be evaluated is an explicit error, which
    /// the engine turns into a disabled schedule with a logged reason.
    pub fn calculator(
        &self,
        anchor: DateTime<Utc>,
    ) -> Result<OccurrenceCalculator, SchedulerError> {
        match self.schedule_type {
            ScheduleType::Cron => {
                let expression = self
                    .expression
                    .as_deref()
                    .ok_or(SchedulerError::MissingCronExpression)?;
                Ok(OccurrenceCalculator::Cron(Box::new(CronSchedule::parse(
                    expression,
                    &self.timezone,
                )?)))
            }
            ScheduleType::Interval => {
                let seconds = self
                    .interval_seconds
                    .ok_or(SchedulerError::MissingInterval)?;
                Ok(OccurrenceCalculator::Interval(IntervalSchedule::new(
                    seconds, anchor,
                )?))
            }
            ScheduleType::OneTime => {
                let at = self.one_time_at.ok_or(SchedulerError::MissingOneTimeAt)?;
                Ok(OccurrenceCalculator::OneTime(OneTimeSchedule::new(at)))
            }
        }
    }

    /// Human-readable form used by the preview and explain responses, where a
    /// non-cron schedule has no expression to echo.
    pub fn describe(&self) -> String {
        match self.schedule_type {
            ScheduleType::Cron => self.expression.clone().unwrap_or_default(),
            ScheduleType::Interval => match self.interval_seconds {
                Some(seconds) => format!("every {seconds} seconds"),
                None => "interval with no period".to_string(),
            },
            ScheduleType::OneTime => match self.one_time_at {
                Some(at) => format!("one-time at {}", at.to_rfc3339()),
                None => "one-time with no instant".to_string(),
            },
        }
    }
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

    // --- ONE_TIME and INTERVAL (spec 01.5) ---

    fn utc(rfc3339: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .with_timezone(&Utc)
    }

    /// AT-SCH-001 for a one-shot: one instant, then nothing.
    #[test]
    fn a_one_time_schedule_fires_once_and_then_has_no_next_run() {
        let instant = utc("2026-10-03T09:00:00Z");
        let one_time = OneTimeSchedule::new(instant);

        assert_eq!(
            one_time.next_after(utc("2026-10-03T08:59:59Z")),
            Some(instant)
        );
        assert_eq!(
            one_time.next_after(instant),
            None,
            "the instant itself is not a future occurrence"
        );
        assert_eq!(
            one_time.next_after(utc("2026-10-04T00:00:00Z")),
            None,
            "a fired one-shot never comes round again"
        );
        assert_eq!(
            OccurrenceSource::next_n_after(&one_time, utc("2026-10-03T00:00:00Z"), 10),
            vec![instant]
        );
    }

    #[test]
    fn a_one_time_schedule_occurrence_window_is_the_single_instant() {
        let instant = utc("2026-10-03T09:00:00Z");
        let one_time = OneTimeSchedule::new(instant);
        assert_eq!(
            one_time.occurrences_between(utc("2026-10-03T00:00:00Z"), instant, 10),
            vec![instant]
        );
        assert!(one_time
            .occurrences_between(instant, utc("2026-10-04T00:00:00Z"), 10)
            .is_empty());
    }

    /// The interval series is anchored at the stored occurrence, so successive
    /// runs are exact multiples of the period and survive a restart.
    #[test]
    fn an_interval_schedule_computes_successive_occurrences() {
        let anchor = utc("2026-10-03T00:00:00Z");
        let interval = IntervalSchedule::new(3600, anchor).unwrap();

        assert_eq!(
            interval.next_after(anchor),
            Some(utc("2026-10-03T01:00:00Z"))
        );
        assert_eq!(
            interval.next_after(utc("2026-10-03T01:00:00Z")),
            Some(utc("2026-10-03T02:00:00Z")),
            "an occurrence exactly on the boundary is not repeated"
        );
        // Mid-period, the next run is still the following anchor point.
        assert_eq!(
            interval.next_after(utc("2026-10-03T01:30:00Z")),
            Some(utc("2026-10-03T02:00:00Z"))
        );
        assert_eq!(
            interval.occurrences_between(
                utc("2026-10-03T00:00:00Z"),
                utc("2026-10-03T03:00:00Z"),
                10
            ),
            vec![
                utc("2026-10-03T01:00:00Z"),
                utc("2026-10-03T02:00:00Z"),
                utc("2026-10-03T03:00:00Z"),
            ]
        );
    }

    /// A schedule a long way behind must not walk every missed occurrence.
    #[test]
    fn a_far_behind_interval_schedule_still_answers_directly() {
        let anchor = utc("2020-01-01T00:00:00Z");
        let interval = IntervalSchedule::new(60, anchor).unwrap();
        let next = interval.next_after(utc("2026-10-03T00:00:00Z")).unwrap();
        assert_eq!(next, utc("2026-10-03T00:01:00Z"));
    }

    #[test]
    fn an_interval_schedule_rejects_a_non_positive_period() {
        let anchor = utc("2026-10-03T00:00:00Z");
        assert!(matches!(
            IntervalSchedule::new(0, anchor),
            Err(SchedulerError::InvalidInterval(0))
        ));
        assert!(matches!(
            IntervalSchedule::new(-30, anchor),
            Err(SchedulerError::InvalidInterval(-30))
        ));
    }

    #[test]
    fn a_recurrence_spec_selects_the_calculator_for_each_kind() {
        let now = utc("2026-10-03T00:00:00Z");
        assert!(matches!(
            RecurrenceSpec::cron("0 2 * * *", "UTC").calculator(now),
            Ok(OccurrenceCalculator::Cron(_))
        ));
        assert!(matches!(
            RecurrenceSpec::interval(60, "UTC").calculator(now),
            Ok(OccurrenceCalculator::Interval(_))
        ));
        assert!(matches!(
            RecurrenceSpec::one_time(now, "UTC").calculator(now),
            Ok(OccurrenceCalculator::OneTime(_))
        ));
    }

    /// A stored row can be genuinely unusable; every such case is an explicit
    /// error so the engine can disable it with a reason.
    #[test]
    fn an_unusable_recurrence_spec_is_an_explicit_error() {
        let now = utc("2026-10-03T00:00:00Z");

        let missing_expression = RecurrenceSpec {
            schedule_type: ScheduleType::Cron,
            expression: None,
            timezone: "UTC".to_string(),
            interval_seconds: None,
            one_time_at: None,
            blackout_dates: Vec::new(),
            time_window: None,
        };
        assert!(matches!(
            missing_expression.calculator(now),
            Err(SchedulerError::MissingCronExpression)
        ));

        let missing_period = RecurrenceSpec {
            schedule_type: ScheduleType::Interval,
            expression: None,
            timezone: "UTC".to_string(),
            interval_seconds: None,
            one_time_at: None,
            blackout_dates: Vec::new(),
            time_window: None,
        };
        assert!(matches!(
            missing_period.calculator(now),
            Err(SchedulerError::MissingInterval)
        ));

        let missing_instant = RecurrenceSpec {
            schedule_type: ScheduleType::OneTime,
            expression: None,
            timezone: "UTC".to_string(),
            interval_seconds: None,
            one_time_at: None,
            blackout_dates: Vec::new(),
            time_window: None,
        };
        assert!(matches!(
            missing_instant.calculator(now),
            Err(SchedulerError::MissingOneTimeAt)
        ));
    }

    #[test]
    fn blackout_date_and_time_window_exclusions_work() {
        let spec = RecurrenceSpec::cron("0 10 * * *", "UTC")
            .with_blackout_dates(vec!["2026-12-25".to_string(), "2027-01-01".to_string()])
            .with_time_window(forge_domain::TimeWindow {
                start_hour: 9,
                end_hour: 17,
            });

        // 2026-12-24 at 10:00 is allowed.
        assert_eq!(spec.exclusion_reason(utc("2026-12-24T10:00:00Z")), None);

        // 2026-12-25 at 10:00 is on a blackout date.
        assert_eq!(
            spec.exclusion_reason(utc("2026-12-25T10:00:00Z")),
            Some(crate::misfire::SkipReason::BlackoutOrHoliday)
        );

        // 2026-12-26 at 20:00 is outside the daily time window (9-17).
        assert_eq!(
            spec.exclusion_reason(utc("2026-12-26T20:00:00Z")),
            Some(crate::misfire::SkipReason::OutsideTimeWindow)
        );
    }

    #[test]
    fn interval_specs_are_described_without_a_cron_expression() {
        assert_eq!(
            RecurrenceSpec::interval(300, "UTC").describe(),
            "every 300 seconds"
        );
        let instant = utc("2026-10-03T09:00:00Z");
        assert_eq!(
            RecurrenceSpec::one_time(instant, "UTC").describe(),
            "one-time at 2026-10-03T09:00:00+00:00"
        );
        assert_eq!(
            RecurrenceSpec::cron("0 2 * * *", "UTC").describe(),
            "0 2 * * *"
        );
    }

    #[test]
    fn timezone_validation_rejects_unknown_zones() {
        assert!(validate_timezone("Asia/Kolkata").is_ok());
        assert!(validate_timezone("").is_err());
        assert!(validate_timezone("Mars/Olympus_Mons").is_err());
    }
}
