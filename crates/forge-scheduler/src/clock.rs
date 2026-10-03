//! Injectable time source.
//!
//! Spec 04.9 requires the scheduler to use an injectable clock so tests are
//! deterministic; DST and misfire behaviour is otherwise impossible to pin
//! down. Production uses [`SystemClock`], which is simply server time.

use chrono::{DateTime, Utc};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;

    /// Moves the clock forward. A no-op for wall-clock sources, so a test can
    /// hold a plain `SharedClock` and still drive time.
    fn advance(&self, _by: chrono::Duration) {}
}

/// Server time. Spec 09.9 requires the scheduler to use server time rather than
/// any per-node clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// A clock frozen at a fixed instant, and optionally advanceable.
#[derive(Debug)]
pub struct FixedClock {
    /// Milliseconds since the Unix epoch. Stored as an integer so a clock can
    /// be advanced atomically from another thread without `Mutex`.
    millis: AtomicI64,
}

impl FixedClock {
    pub fn at(rfc3339: &str) -> Self {
        let parsed = DateTime::parse_from_rfc3339(rfc3339)
            .unwrap_or_else(|e| panic!("FixedClock::at({rfc3339:?}): {e}"))
            .with_timezone(&Utc);
        Self::from(parsed)
    }

    pub fn from(instant: DateTime<Utc>) -> Self {
        Self {
            millis: AtomicI64::new(instant.timestamp_millis()),
        }
    }

    /// Moves the clock forward.
    pub fn advance(&self, by: chrono::Duration) {
        self.millis
            .fetch_add(by.num_milliseconds(), Ordering::SeqCst);
    }

    /// Shared handle, so a background task can advance the same clock the test
    /// is driving.
    pub fn shared(self) -> Arc<Self> {
        Arc::new(self)
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        let millis = self.millis.load(Ordering::SeqCst);
        DateTime::from_timestamp_millis(millis).unwrap_or_else(|| {
            panic!("FixedClock holds an out-of-range instant: {millis} ms since epoch")
        })
    }

    fn advance(&self, by: chrono::Duration) {
        self.millis
            .fetch_add(by.num_milliseconds(), Ordering::SeqCst);
    }
}

/// A clock that advances only when told to.
pub type SharedClock = Arc<dyn Clock>;

/// The default `SharedClock`: server time.
pub fn system_clock() -> SharedClock {
    Arc::new(SystemClock)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_is_frozen() {
        let clock = FixedClock::at("2026-10-03T12:00:00Z");
        let first = clock.now();
        // Time does not move on its own.
        assert_eq!(clock.now(), first);
    }

    #[test]
    fn fixed_clock_advances_only_when_told() {
        let clock = FixedClock::at("2026-10-03T12:00:00Z");
        clock.advance(chrono::Duration::minutes(90));
        assert_eq!(
            clock.now().to_rfc3339(),
            "2026-10-03T13:30:00+00:00"
        );
    }

    #[test]
    fn a_shared_fixed_clock_can_be_advanced_from_another_thread() {
        let handle = FixedClock::at("2026-10-03T00:00:00Z").shared();
        assert_eq!(handle.now().to_rfc3339(), "2026-10-03T00:00:00+00:00");

        // Join the writer before reading: a detached thread would race the
        // assertion and the read could observe the pre-advance value.
        let writer = handle.clone();
        std::thread::spawn(move || {
            writer.advance(chrono::Duration::hours(1));
        })
        .join()
        .unwrap();

        // Every handle sees the advance, because they share one atomic counter.
        assert_eq!(handle.now().to_rfc3339(), "2026-10-03T01:00:00+00:00");
    }

    #[test]
    fn advancing_a_shared_clock_is_visible_to_the_engine() {
        // The pattern the scheduler uses in tests: build with a shared clock,
        // then drive time forward.
        let clock = FixedClock::at("2026-10-03T00:00:00Z").shared();
        assert_eq!(clock.now().to_rfc3339(), "2026-10-03T00:00:00+00:00");

        clock.advance(chrono::Duration::hours(2));
        assert_eq!(clock.now().to_rfc3339(), "2026-10-03T02:00:00+00:00");
    }

    #[test]
    fn system_clock_is_close_to_now() {
        let now = system_clock().now();
        let delta = (Utc::now() - now).num_seconds().abs();
        assert!(delta < 5, "system clock drifted by {delta}s");
    }
}