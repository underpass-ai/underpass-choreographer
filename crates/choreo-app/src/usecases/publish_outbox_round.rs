//! [`PublishOutboxRound`] — what one publishing round did.

/// Every message a round touched is accounted for in exactly one of
/// these. A round that reports fewer than it claimed has lost track of
/// something, which for a delivery guarantee is the whole problem.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PublishOutboxRound {
    delivered: usize,
    failed: usize,
    quarantined: usize,
}

impl PublishOutboxRound {
    #[must_use]
    pub fn delivered(self) -> usize {
        self.delivered
    }

    #[must_use]
    pub fn failed(self) -> usize {
        self.failed
    }

    #[must_use]
    pub fn quarantined(self) -> usize {
        self.quarantined
    }

    #[must_use]
    pub fn claimed(self) -> usize {
        self.delivered + self.failed + self.quarantined
    }

    /// Whether the round found nothing to do — the signal a caller uses
    /// to back off instead of spinning.
    #[must_use]
    pub fn is_idle(self) -> bool {
        self.claimed() == 0
    }

    pub(super) fn record_delivered(&mut self) {
        self.delivered += 1;
    }

    pub(super) fn record_failed(&mut self) {
        self.failed += 1;
    }

    pub(super) fn record_quarantined(&mut self) {
        self.quarantined += 1;
    }
}
