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
//! Everything a caller hands over survives the round trip, including
//! an entry's `detail` and an evidence item's source. That was not
//! true when this adapter was written: the kernel took both and gave
//! neither back, and the loss was pinned by a test that said the day
//! they came back it would fail and the paragraph describing the
//! limitation would be deleted rather than left to age.
//!
//! Worth keeping the second half of that story. The kernel started
//! returning them and **the test did not fail**, because this adapter
//! was still discarding both before the assertion could see them — it
//! had pinned its own workaround rather than the limitation. A test
//! written to notice somebody else's change has to read past the code
//! that works around it.

mod error;
mod mapping;
mod session_memory;
mod stdio;
mod transport;

pub use session_memory::KernelSessionMemory;
pub use stdio::{StdioKernelTransport, StdioKernelTransportConfig};
pub use transport::{KernelAnswer, KernelTransport, KernelTransportError};
