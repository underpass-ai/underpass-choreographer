//! Application services — compose one or more use cases.

mod auto_dispatch;
mod session_memory_projection;
mod session_memory_recorder;

pub use auto_dispatch::{AutoDispatchOutcome, AutoDispatchService};
pub use session_memory_recorder::SessionMemoryRecorder;
