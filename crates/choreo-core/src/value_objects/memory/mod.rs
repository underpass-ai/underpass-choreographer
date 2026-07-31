//! What a working session remembers, and what it may ask of memory.
//!
//! These types are shaped by what a memory kernel actually offers —
//! a scope the memory is about, axes within it, entries carrying
//! provenance, evidence backing them, and movement through time — but
//! they name none of it in a kernel's terms. An engine that spoke a
//! particular kernel's vocabulary would have to change when that
//! kernel did, and would be unusable with anything else.
//!
//! Entries are the *what* and relations are the *why*. That split is
//! the whole design: an entry states something, and only an edge says
//! how one thing came from another, so a memory of entries alone can
//! be read but not followed.

mod memory_capabilities;
mod memory_confidence;
mod memory_dimension;
mod memory_entry;
mod memory_entry_id;
mod memory_entry_kind;
mod memory_evidence;
mod memory_moment;
mod memory_provenance;
mod memory_question;
mod memory_relation;
mod memory_relation_kind;
mod memory_scope;
mod memory_write;

pub use memory_capabilities::{MemoryCapabilities, MemoryCapability};
pub use memory_confidence::MemoryConfidence;
pub use memory_dimension::MemoryDimension;
pub use memory_entry::MemoryEntry;
pub use memory_entry_id::MemoryEntryId;
pub use memory_entry_kind::MemoryEntryKind;
pub use memory_evidence::MemoryEvidence;
pub use memory_moment::MemoryMoment;
pub use memory_provenance::MemoryProvenance;
pub use memory_question::MemoryQuestion;
pub use memory_relation::MemoryRelation;
pub use memory_relation_kind::MemoryRelationKind;
pub use memory_scope::MemoryScope;
pub use memory_write::MemoryWrite;
