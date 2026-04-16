//! Clock adapter.
//!
//! The domain never reads the wall clock directly. Aggregates receive
//! an `OffsetDateTime` through [`ClockPort`] so deliberations stay
//! reproducible under test.

use choreo_core::ports::ClockPort;
use time::OffsetDateTime;

/// Wall-clock implementation of [`ClockPort`] that returns UTC time
/// from the host's monotonic source as known to `time`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl SystemClock {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl ClockPort for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn now_returns_utc() {
        let now = SystemClock::new().now();
        assert_eq!(now.offset(), time::UtcOffset::UTC);
    }

    #[test]
    fn subsequent_reads_are_monotonically_non_decreasing() {
        let clock = SystemClock::new();
        let a = clock.now();
        sleep(Duration::from_millis(1));
        let b = clock.now();
        assert!(b >= a, "wall clock went backwards: {a:?} -> {b:?}");
    }
}
