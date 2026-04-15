//! [`ClockPort`] — source of wall-clock time.
//!
//! The domain never calls `OffsetDateTime::now_utc` directly so that
//! deliberations stay reproducible under test and deterministic
//! clocks can be injected (frozen clocks for replay, accelerated
//! clocks for load tests, etc.).

use time::OffsetDateTime;

pub trait ClockPort: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}
