//! What a working session remembers, and what it may ask of memory.
//!
//! These types are shaped by what a memory kernel actually offers —
//! a scope the memory is about, axes within it, entries carrying
//! provenance, evidence backing them, and movement through time — but
//! they name none of it in a kernel's terms. An engine that spoke a
//! particular kernel's vocabulary would have to change when that
//! kernel did, and would be unusable with anything else.

mod memory_capabilities;
mod memory_dimension;
mod memory_entry;
mod memory_entry_kind;
mod memory_evidence;
mod memory_moment;
mod memory_provenance;
mod memory_question;
mod memory_scope;

pub use memory_capabilities::{MemoryCapabilities, MemoryCapability};
pub use memory_dimension::MemoryDimension;
pub use memory_entry::MemoryEntry;
pub use memory_entry_kind::MemoryEntryKind;
pub use memory_evidence::MemoryEvidence;
pub use memory_moment::MemoryMoment;
pub use memory_provenance::MemoryProvenance;
pub use memory_question::MemoryQuestion;
pub use memory_scope::MemoryScope;
