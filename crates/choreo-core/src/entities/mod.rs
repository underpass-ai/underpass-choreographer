//! Domain entities and aggregates.
//!
//! Entities have identity that persists across state changes. Aggregate
//! roots own invariants spanning multiple objects; state transitions
//! happen through their methods, not by mutating fields directly.

mod audit_chain;
mod audit_record;
mod ceremony_commit;
mod ceremony_definition;
mod ceremony_definition_analysis;
mod ceremony_definition_draft;
mod ceremony_evidence_pack;
mod ceremony_instance;
mod ceremony_intervention;
mod council;
mod deliberation;
mod external_context;
mod proposal;
mod published_ceremony_definition;
mod statistics;
mod task;
mod validation;

pub use audit_chain::AuditChain;
pub use audit_record::{AuditFact, AuditRecord, AUDIT_RECORD_SCHEMA_VERSION};
pub use ceremony_commit::{CeremonyCommit, CommitOutcome};
pub use ceremony_definition::CeremonyDefinition;
pub use ceremony_definition_draft::CeremonyDefinitionDraft;
pub use ceremony_evidence_pack::CeremonyEvidencePack;
pub use ceremony_instance::CeremonyInstance;
pub use ceremony_intervention::CeremonyIntervention;
pub use council::Council;
pub use deliberation::{Deliberation, DeliberationPhase, RankedOutcome};
pub use external_context::{ContextItem, ContextReference, ContextSummary, ExternalContextBundle};
pub use proposal::Proposal;
pub use published_ceremony_definition::{PublicationOutcome, PublishedCeremonyDefinition};
pub use statistics::Statistics;
pub use task::{Task, TaskConstraints, TaskMetadata};
pub use validation::{ValidationOutcome, ValidatorReport};
