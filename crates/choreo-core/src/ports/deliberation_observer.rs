//! [`DeliberationObserverPort`] — per-call hook for observing a
//! deliberation's lifecycle.
//!
//! Unlike [`MessagingPort`](super::MessagingPort), which broadcasts
//! persistent domain events (NATS), this port is for **ephemeral
//! per-call observation**: the server-streaming `StreamDeliberation`
//! RPC wires an adapter that forwards events to an `mpsc::channel`
//! becoming the response stream, and discards them when the stream
//! closes. There is no subscription / replay semantics; the observer
//! is call-scoped.
//!
//! Keeping this separate from `MessagingPort` means `DeliberateUseCase`
//! can stay oblivious to whether a caller is listening on a stream,
//! and messaging adapters stay oblivious to live stream semantics.
//!
//! The port is implementation-optional: callers that do not care pass
//! [`NullObserver`], which discards every event.

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::entities::DeliberationPhase;
use crate::value_objects::TaskId;

#[async_trait]
pub trait DeliberationObserverPort: Send + Sync {
    /// Called when the aggregate transitions into `phase`.
    ///
    /// Observers must not block or the use case stalls; adapters that
    /// need unbounded work should spawn it. Adapters whose sink has
    /// closed (receiver dropped) should become a no-op silently —
    /// the use case does not care whether anyone is still listening.
    async fn on_phase_changed(
        &self,
        task_id: &TaskId,
        phase: DeliberationPhase,
        emitted_at: OffsetDateTime,
    );
}

/// No-op observer. Use this in composition when no stream is active.
#[derive(Debug, Default, Clone)]
pub struct NullObserver;

#[async_trait]
impl DeliberationObserverPort for NullObserver {
    async fn on_phase_changed(
        &self,
        _task_id: &TaskId,
        _phase: DeliberationPhase,
        _emitted_at: OffsetDateTime,
    ) {
    }
}
