//! [`PublishOutboxUseCase`] — one round of turning committed intent
//! into delivered effect.
//!
//! The retry, exhaustion and ordering rules live here rather than in
//! each adapter: a host supplies a store and a transport, not a
//! delivery policy it has to get right on its own.

use std::sync::Arc;

use choreo_core::error::DomainError;
use choreo_core::ports::{ClockPort, OutboxPort, OutboxTransportPort};
use choreo_core::value_objects::{ClaimedOutboxMessage, EventId, OutboxQuarantineReason};

use super::{PublishOutboxInput, PublishOutboxRound};

pub struct PublishOutboxUseCase {
    outbox: Arc<dyn OutboxPort>,
    transport: Arc<dyn OutboxTransportPort>,
    clock: Arc<dyn ClockPort>,
}

impl std::fmt::Debug for PublishOutboxUseCase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("PublishOutboxUseCase").finish()
    }
}

impl PublishOutboxUseCase {
    #[must_use]
    pub fn new(
        outbox: Arc<dyn OutboxPort>,
        transport: Arc<dyn OutboxTransportPort>,
        clock: Arc<dyn ClockPort>,
    ) -> Self {
        Self {
            outbox,
            transport,
            clock,
        }
    }

    /// Claim a batch, deliver what it can, and account for the rest.
    ///
    /// Messages are marked delivered only after the transport confirms.
    /// A failure between the two produces a redelivery rather than a
    /// loss, which is the trade the at-least-once contract makes.
    pub async fn execute(
        &self,
        input: PublishOutboxInput,
    ) -> Result<PublishOutboxRound, DomainError> {
        let claimed = self
            .outbox
            .claim(input.batch_size(), self.clock.now(), input.lease())
            .await?;

        let mut round = PublishOutboxRound::default();
        let mut delivered = Vec::with_capacity(claimed.len());

        for message in &claimed {
            if message.attempt().is_exhausted(input.max_attempts()) {
                self.quarantine(message, input.max_attempts()).await?;
                round.record_quarantined();
                continue;
            }

            if self.transport.deliver(message.message()).await.is_ok() {
                delivered.push(message.message().event_id().clone());
                round.record_delivered();
            } else {
                self.outbox
                    .mark_failed(message.message().event_id())
                    .await?;
                round.record_failed();
            }
        }

        self.mark_delivered(&delivered).await?;
        Ok(round)
    }

    /// Exhaustion is recorded with its cause, never as a quiet removal.
    async fn quarantine(
        &self,
        message: &ClaimedOutboxMessage,
        max_attempts: u32,
    ) -> Result<(), DomainError> {
        let reason = OutboxQuarantineReason::new(format!(
            "delivery failed {} times, at or above the limit of {max_attempts}",
            message.attempt().value()
        ))?;
        self.outbox
            .quarantine(message.message().event_id(), reason)
            .await
    }

    async fn mark_delivered(&self, delivered: &[EventId]) -> Result<(), DomainError> {
        if delivered.is_empty() {
            return Ok(());
        }
        self.outbox.mark_delivered(delivered).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use choreo_core::value_objects::{
        ClaimedOutboxMessage, DurationMs, OutboxAttempt, OutboxMessage, OutboxSubject,
    };
    use serde_json::json;
    use std::sync::Mutex;
    use time::OffsetDateTime;

    const MAX_ATTEMPTS: u32 = 3;

    fn input() -> PublishOutboxInput {
        PublishOutboxInput::new(8, MAX_ATTEMPTS, DurationMs::from_millis(30_000)).unwrap()
    }

    fn message(event: &str) -> OutboxMessage {
        OutboxMessage::new(
            EventId::new(event).unwrap(),
            OutboxSubject::new("test.subject").unwrap(),
            json!({ "event": event }),
            OffsetDateTime::UNIX_EPOCH,
        )
        .unwrap()
    }

    #[derive(Debug, Default)]
    struct RecordingOutbox {
        claimable: Mutex<Vec<ClaimedOutboxMessage>>,
        delivered: Mutex<Vec<EventId>>,
        failed: Mutex<Vec<EventId>>,
        quarantined: Mutex<Vec<(EventId, OutboxQuarantineReason)>>,
    }

    impl RecordingOutbox {
        fn holding(messages: Vec<ClaimedOutboxMessage>) -> Arc<Self> {
            Arc::new(Self {
                claimable: Mutex::new(messages),
                ..Self::default()
            })
        }
    }

    #[async_trait::async_trait]
    impl OutboxPort for RecordingOutbox {
        async fn claim(
            &self,
            limit: usize,
            _now: OffsetDateTime,
            _lease: DurationMs,
        ) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
            let mut claimable = self.claimable.lock().unwrap();
            let taken = claimable.len().min(limit);
            Ok(claimable.drain(..taken).collect())
        }

        async fn mark_delivered(&self, event_ids: &[EventId]) -> Result<(), DomainError> {
            self.delivered.lock().unwrap().extend(event_ids.to_vec());
            Ok(())
        }

        async fn mark_failed(&self, event_id: &EventId) -> Result<(), DomainError> {
            self.failed.lock().unwrap().push(event_id.clone());
            Ok(())
        }

        async fn quarantine(
            &self,
            event_id: &EventId,
            reason: OutboxQuarantineReason,
        ) -> Result<(), DomainError> {
            self.quarantined
                .lock()
                .unwrap()
                .push((event_id.clone(), reason));
            Ok(())
        }

        async fn quarantined(&self) -> Result<Vec<ClaimedOutboxMessage>, DomainError> {
            Ok(Vec::new())
        }
    }

    #[derive(Debug)]
    struct Transport {
        accepts: bool,
        delivered: Mutex<Vec<EventId>>,
    }

    impl Transport {
        fn accepting() -> Arc<Self> {
            Arc::new(Self {
                accepts: true,
                delivered: Mutex::new(Vec::new()),
            })
        }

        fn refusing() -> Arc<Self> {
            Arc::new(Self {
                accepts: false,
                delivered: Mutex::new(Vec::new()),
            })
        }
    }

    #[async_trait::async_trait]
    impl OutboxTransportPort for Transport {
        async fn deliver(&self, message: &OutboxMessage) -> Result<(), DomainError> {
            if !self.accepts {
                return Err(DomainError::InvariantViolated {
                    reason: "transport refused",
                });
            }
            self.delivered
                .lock()
                .unwrap()
                .push(message.event_id().clone());
            Ok(())
        }
    }

    #[derive(Debug)]
    struct FrozenClock;

    impl ClockPort for FrozenClock {
        fn now(&self) -> OffsetDateTime {
            OffsetDateTime::UNIX_EPOCH
        }
    }

    fn publisher(outbox: Arc<RecordingOutbox>, transport: Arc<Transport>) -> PublishOutboxUseCase {
        PublishOutboxUseCase::new(outbox, transport, Arc::new(FrozenClock))
    }

    #[tokio::test]
    async fn an_empty_outbox_produces_an_idle_round() {
        let outbox = RecordingOutbox::holding(Vec::new());

        let round = publisher(Arc::clone(&outbox), Transport::accepting())
            .execute(input())
            .await
            .unwrap();

        assert!(round.is_idle());
        assert!(outbox.delivered.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn accepted_messages_are_marked_only_after_the_transport_confirms() {
        let outbox = RecordingOutbox::holding(vec![
            ClaimedOutboxMessage::new(message("a"), OutboxAttempt::NONE),
            ClaimedOutboxMessage::new(message("b"), OutboxAttempt::NONE),
        ]);
        let transport = Transport::accepting();

        let round = publisher(Arc::clone(&outbox), Arc::clone(&transport))
            .execute(input())
            .await
            .unwrap();

        assert_eq!(round.delivered(), 2);
        assert_eq!(transport.delivered.lock().unwrap().len(), 2);
        assert_eq!(outbox.delivered.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_refused_message_is_counted_as_failed_and_never_marked_delivered() {
        let outbox = RecordingOutbox::holding(vec![ClaimedOutboxMessage::new(
            message("a"),
            OutboxAttempt::NONE,
        )]);

        let round = publisher(Arc::clone(&outbox), Transport::refusing())
            .execute(input())
            .await
            .unwrap();

        assert_eq!(round.failed(), 1);
        assert_eq!(round.delivered(), 0);
        assert_eq!(outbox.failed.lock().unwrap().len(), 1);
        assert!(
            outbox.delivered.lock().unwrap().is_empty(),
            "a refused message was marked delivered — that is a lost message"
        );
    }

    #[tokio::test]
    async fn an_exhausted_message_is_quarantined_with_its_reason_and_not_retried() {
        let outbox = RecordingOutbox::holding(vec![ClaimedOutboxMessage::new(
            message("poison"),
            OutboxAttempt::new(MAX_ATTEMPTS),
        )]);
        let transport = Transport::accepting();

        let round = publisher(Arc::clone(&outbox), Arc::clone(&transport))
            .execute(input())
            .await
            .unwrap();

        assert_eq!(round.quarantined(), 1);
        assert!(
            transport.delivered.lock().unwrap().is_empty(),
            "an exhausted message was still handed to the transport"
        );

        let quarantined = outbox.quarantined.lock().unwrap();
        assert_eq!(quarantined.len(), 1);
        assert!(
            quarantined[0].1.as_str().contains('3'),
            "the quarantine reason does not say why: {}",
            quarantined[0].1
        );
    }

    #[tokio::test]
    async fn every_claimed_message_is_accounted_for() {
        let outbox = RecordingOutbox::holding(vec![
            ClaimedOutboxMessage::new(message("ok"), OutboxAttempt::NONE),
            ClaimedOutboxMessage::new(message("poison"), OutboxAttempt::new(MAX_ATTEMPTS)),
        ]);

        let round = publisher(Arc::clone(&outbox), Transport::accepting())
            .execute(input())
            .await
            .unwrap();

        assert_eq!(round.claimed(), 2);
        assert_eq!(round.delivered() + round.failed() + round.quarantined(), 2);
    }
}
