use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A point to read memory as of.
///
/// The question "what did we know at 03:20" is not the same as "what
/// do we know now, filtered to before 03:20": the first excludes what
/// was learned later even about earlier events, which is exactly what
/// makes it useful for judging a decision by what was available when
/// it was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryMoment(#[serde(with = "time::serde::rfc3339")] OffsetDateTime);

impl MemoryMoment {
    #[must_use]
    pub const fn at(instant: OffsetDateTime) -> Self {
        Self(instant)
    }

    #[must_use]
    pub const fn instant(self) -> OffsetDateTime {
        self.0
    }
}
