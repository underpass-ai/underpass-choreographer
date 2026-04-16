//! NATS adapter.
//!
//! Two adapters live here: [`NatsMessaging`] publishes outbound
//! domain events, and [`NatsTriggerSubscriber`] consumes inbound
//! [`TriggerEvent`]s and hands them to the `AutoDispatchService`.
//!
//! Both honour the AsyncAPI contract in
//! `specs/asyncapi/choreographer.asyncapi.yaml`: subjects under a
//! configurable prefix (default `choreo.*`), JSON payloads with the
//! envelope fields flattened at the root of each message.
//!
//! [`TriggerEvent`]: choreo_core::events::TriggerEvent

mod config;
mod messaging;
mod subscriber;

pub use config::{NatsConfig, NatsSubjects};
pub use messaging::NatsMessaging;
pub use subscriber::NatsTriggerSubscriber;
