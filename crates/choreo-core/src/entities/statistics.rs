//! [`Statistics`] entity — operational counters for the service.
//!
//! Neutral translation of `OrchestratorStatistics` from the Python
//! reference. All counters are specialty-indexed instead of role-indexed;
//! no SWE-specific vocabulary remains.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::value_objects::{DurationMs, Specialty};

/// Operational statistics tracked by the Choreographer.
///
/// Mutable entity: methods advance the counters, they are never
/// mutated from outside. Saturating arithmetic prevents silent
/// overflow under very long uptimes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statistics {
    total_deliberations: u64,
    total_orchestrations: u64,
    total_duration: DurationMs,
    per_specialty: BTreeMap<Specialty, u64>,
}

impl Statistics {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rehydrate a [`Statistics`] from already-aggregated counters.
    ///
    /// Used by persistent adapters whose storage shape keeps running
    /// sums rather than the stream of individual records. The entity's
    /// mutators stay the single source of truth for increments; this
    /// is the matching read path.
    #[must_use]
    pub fn from_counters(
        total_deliberations: u64,
        total_orchestrations: u64,
        total_duration: DurationMs,
        per_specialty: BTreeMap<Specialty, u64>,
    ) -> Self {
        Self {
            total_deliberations,
            total_orchestrations,
            total_duration,
            per_specialty,
        }
    }

    /// Record that a deliberation for `specialty` completed in `duration`.
    pub fn record_deliberation(&mut self, specialty: &Specialty, duration: DurationMs) {
        self.total_deliberations = self.total_deliberations.saturating_add(1);
        self.total_duration = self.total_duration.saturating_add(duration);
        let entry = self.per_specialty.entry(specialty.clone()).or_insert(0);
        *entry = entry.saturating_add(1);
    }

    /// Record that an orchestration (deliberate + execute) completed.
    pub fn record_orchestration(&mut self, duration: DurationMs) {
        self.total_orchestrations = self.total_orchestrations.saturating_add(1);
        self.total_duration = self.total_duration.saturating_add(duration);
    }

    /// Reset all counters. Used mostly in tests and admin endpoints.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    #[must_use]
    pub fn total_deliberations(&self) -> u64 {
        self.total_deliberations
    }

    #[must_use]
    pub fn total_orchestrations(&self) -> u64 {
        self.total_orchestrations
    }

    #[must_use]
    pub fn total_duration(&self) -> DurationMs {
        self.total_duration
    }

    #[must_use]
    pub fn per_specialty(&self) -> &BTreeMap<Specialty, u64> {
        &self.per_specialty
    }

    /// Average duration per operation across all deliberations and
    /// orchestrations. Returns zero when no operation has been recorded.
    #[must_use]
    pub fn average_duration_ms(&self) -> f64 {
        let ops = self.total_deliberations + self.total_orchestrations;
        if ops == 0 {
            0.0
        } else {
            self.total_duration.get() as f64 / ops as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(s: &str) -> Specialty {
        Specialty::new(s).unwrap()
    }

    #[test]
    fn fresh_stats_are_zero() {
        let s = Statistics::new();
        assert_eq!(s.total_deliberations(), 0);
        assert_eq!(s.total_orchestrations(), 0);
        assert_eq!(s.total_duration(), DurationMs::ZERO);
        assert_eq!(s.average_duration_ms(), 0.0);
        assert!(s.per_specialty().is_empty());
    }

    #[test]
    fn record_deliberation_advances_counters() {
        let mut s = Statistics::new();
        s.record_deliberation(&sp("triage"), DurationMs::from_millis(100));
        s.record_deliberation(&sp("triage"), DurationMs::from_millis(50));
        s.record_deliberation(&sp("reviewer"), DurationMs::from_millis(200));

        assert_eq!(s.total_deliberations(), 3);
        assert_eq!(s.total_duration(), DurationMs::from_millis(350));
        assert_eq!(s.per_specialty().get(&sp("triage")).copied(), Some(2));
        assert_eq!(s.per_specialty().get(&sp("reviewer")).copied(), Some(1));
    }

    #[test]
    fn record_orchestration_advances_counters_but_not_specialty_map() {
        let mut s = Statistics::new();
        s.record_orchestration(DurationMs::from_millis(300));
        assert_eq!(s.total_orchestrations(), 1);
        assert_eq!(s.total_duration(), DurationMs::from_millis(300));
        assert!(s.per_specialty().is_empty());
    }

    #[test]
    fn average_duration_divides_over_all_ops() {
        let mut s = Statistics::new();
        s.record_deliberation(&sp("a"), DurationMs::from_millis(100));
        s.record_orchestration(DurationMs::from_millis(200));
        // (100 + 200) / 2 = 150.0
        assert_eq!(s.average_duration_ms(), 150.0);
    }

    #[test]
    fn reset_clears_state() {
        let mut s = Statistics::new();
        s.record_deliberation(&sp("x"), DurationMs::from_millis(10));
        s.reset();
        assert_eq!(s, Statistics::default());
    }

    #[test]
    fn saturating_counters_do_not_overflow() {
        let mut s = Statistics::new();
        s.total_deliberations = u64::MAX;
        s.total_duration = DurationMs::from_millis(u64::MAX);
        s.record_deliberation(&sp("x"), DurationMs::from_millis(42));
        assert_eq!(s.total_deliberations(), u64::MAX);
        assert_eq!(s.total_duration().get(), u64::MAX);
    }
}
