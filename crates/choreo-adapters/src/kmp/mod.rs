//! Memory that outlives a session, kept by an external memory kernel.
//!
//! The engine's memory ports say what a working session may remember
//! and ask; this module makes them talk to a kernel that already does
//! graph-temporal memory, over that kernel's own tool protocol.
//!
//! # Why a tool client and not a library
//!
//! Linking the kernel's crates would tie this crate's publishable
//! surface to another workspace's release cadence, and would make the
//! engine's memory unusable with anything but that one kernel. A tool
//! call over a pipe costs a process and some JSON, and buys a boundary
//! that a second implementation could stand behind. Nothing here
//! depends on the kernel except the names of its tools and the shape
//! of their arguments.
//!
//! # What crosses the boundary, and what does not
//!
//! Decisions, observations, constraints and outcomes cross, with the
//! evidence attached to them and enough provenance to walk a memory
//! back to the session that produced it. Transcripts do not: the port
//! gives them no kind, and this adapter has nowhere to put one.
//!
//! Two things a caller hands over do **not** survive the round trip,
//! because the kernel's read surface does not return them: an entry's
//! `detail` bag and an evidence item's `source_id`. Both are sent —
//! the kernel keeps an append-only log of what it was given — but
//! neither comes back, so a recalled entry carries an empty detail and
//! evidence without a source. That loss is pinned by a test rather
//! than described only here, because a paragraph nobody runs is how a
//! known limitation becomes a surprise.

mod error;
mod mapping;
mod session_memory;
mod stdio;
mod transport;

pub use session_memory::KernelSessionMemory;
pub use stdio::{StdioKernelTransport, StdioKernelTransportConfig};
pub use transport::{KernelAnswer, KernelTransport, KernelTransportError};
