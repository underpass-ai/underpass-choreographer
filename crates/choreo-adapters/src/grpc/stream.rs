//! Channel-backed [`DeliberationObserverPort`] + helpers for the
//! `StreamDeliberation` RPC.
//!
//! One `mpsc` channel per in-flight stream: the observer pushes phase
//! updates into it, the handler forwards the final
//! [`DeliberationResult`] on success, and the receiver is handed back
//! as the gRPC stream. If the client drops the stream the channel
//! closes — sends become no-ops and the spawned deliberation finishes
//! anyway so downstream state (repository, messaging, statistics)
//! stays consistent.

use async_trait::async_trait;
use choreo_core::entities::DeliberationPhase;
use choreo_core::ports::DeliberationObserverPort;
use choreo_core::value_objects::TaskId;
use choreo_proto::v1 as pb;
use time::OffsetDateTime;
use tokio::sync::mpsc;
use tonic::Status;

use super::mappers::{offset_to_timestamp, proto_phase_from_domain};

/// Forwards every `on_phase_changed` callback as a
/// `StreamDeliberationResponse` onto an `mpsc` sink.
pub struct ChannelObserver {
    sink: mpsc::Sender<Result<pb::StreamDeliberationResponse, Status>>,
}

impl ChannelObserver {
    #[must_use]
    pub fn new(sink: mpsc::Sender<Result<pb::StreamDeliberationResponse, Status>>) -> Self {
        Self { sink }
    }
}

impl std::fmt::Debug for ChannelObserver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelObserver").finish()
    }
}

#[async_trait]
impl DeliberationObserverPort for ChannelObserver {
    async fn on_phase_changed(
        &self,
        task_id: &TaskId,
        phase: DeliberationPhase,
        emitted_at: OffsetDateTime,
    ) {
        let frame = pb::StreamDeliberationResponse {
            update: Some(pb::DeliberationUpdate {
                task_id: task_id.as_str().to_owned(),
                phase: proto_phase_from_domain(phase) as i32,
                emitted_at: Some(offset_to_timestamp(emitted_at)),
                payload: None,
            }),
        };
        // Intentional: a closed sink means the client went away.
        // Dropping silently honours the port contract.
        let _ = self.sink.send(Ok(frame)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::wrappers::ReceiverStream;
    use tokio_stream::StreamExt;

    fn task_id() -> TaskId {
        TaskId::new("t-stream").unwrap()
    }

    #[tokio::test]
    async fn phase_change_flows_into_the_channel_as_an_update_frame() {
        let (tx, rx) = mpsc::channel(4);
        let observer = ChannelObserver::new(tx);

        let emitted = time::macros::datetime!(2026-04-15 12:00:00 UTC);
        observer
            .on_phase_changed(&task_id(), DeliberationPhase::Revising, emitted)
            .await;
        drop(observer);

        let frames: Vec<_> = ReceiverStream::new(rx).collect().await;
        assert_eq!(frames.len(), 1);
        let frame = frames.into_iter().next().unwrap().unwrap();
        let update = frame.update.unwrap();
        assert_eq!(update.task_id, "t-stream");
        assert_eq!(update.phase, pb::DeliberationPhase::Revising as i32);
        assert!(update.emitted_at.is_some());
        assert!(update.payload.is_none());
    }

    #[tokio::test]
    async fn closed_sink_is_silently_tolerated() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx); // client vanished
        let observer = ChannelObserver::new(tx);
        // Must not panic, must not deadlock.
        observer
            .on_phase_changed(
                &task_id(),
                DeliberationPhase::Completed,
                OffsetDateTime::UNIX_EPOCH,
            )
            .await;
    }

    #[tokio::test]
    async fn every_phase_maps_to_a_distinct_proto_enum_on_the_wire() {
        let (tx, rx) = mpsc::channel(8);
        let observer = ChannelObserver::new(tx);
        for phase in [
            DeliberationPhase::Proposing,
            DeliberationPhase::Revising,
            DeliberationPhase::Validating,
            DeliberationPhase::Scoring,
            DeliberationPhase::Completed,
        ] {
            observer
                .on_phase_changed(&task_id(), phase, OffsetDateTime::UNIX_EPOCH)
                .await;
        }
        drop(observer);

        let frames: Vec<_> = ReceiverStream::new(rx).collect().await;
        let phases: Vec<i32> = frames
            .into_iter()
            .map(|f| f.unwrap().update.unwrap().phase)
            .collect();
        assert_eq!(
            phases,
            vec![
                pb::DeliberationPhase::Proposing as i32,
                pb::DeliberationPhase::Revising as i32,
                pb::DeliberationPhase::Validating as i32,
                pb::DeliberationPhase::Scoring as i32,
                pb::DeliberationPhase::Completed as i32,
            ]
        );
    }
}
