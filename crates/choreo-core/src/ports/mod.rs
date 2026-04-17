//! Domain ports.
//!
//! Ports are narrow, segregated traits. Each one names exactly one
//! responsibility that the application layer requires from the outside
//! world (agents, message bus, clock, persistence, …). Adapters in
//! `choreo-adapters` implement these traits.
//!
//! Hexagonal discipline:
//!
//! - Dependency direction is **adapters → app → core**. Ports live in
//!   core and import nothing from app or adapters.
//! - All ports return [`crate::DomainError`] so the application layer
//!   never leaks adapter-shaped errors (I/O, wire, parsing) upward.
//! - Segregation follows ISP: no port has more than one reason to
//!   change.

mod agent;
mod agent_factory;
mod agent_registry;
mod agent_resolver;
mod clock;
mod configuration;
mod council_registry;
mod deliberation_observer;
mod deliberation_repository;
mod executor;
mod messaging;
mod scoring;
mod statistics;
mod validator;

pub use agent::{AgentPort, Critique, DraftRequest, Revision};
pub use agent_factory::{AgentDescriptor, AgentFactoryPort};
pub use agent_registry::AgentRegistryPort;
pub use agent_resolver::AgentResolverPort;
pub use clock::ClockPort;
pub use configuration::{ConfigurationPort, ServiceConfig};
pub use council_registry::CouncilRegistryPort;
pub use deliberation_observer::{DeliberationObserverPort, NullObserver};
pub use deliberation_repository::DeliberationRepositoryPort;
pub use executor::{ExecutionOutcome, ExecutorPort};
pub use messaging::{DomainEvent, MessagingPort, SubscriptionHandler};
pub use scoring::ScoringPort;
pub use statistics::StatisticsPort;
pub use validator::ValidatorPort;
