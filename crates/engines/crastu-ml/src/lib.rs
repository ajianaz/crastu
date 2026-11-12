//! Model session registry.
//!
//! One session per (model file, options) per process, shared by reference counted handles and
//! freed after an idle time. The registry knows nothing about a runtime: a [`SessionFactory`]
//! builds the sessions, so the logic compiles everywhere and is tested with a fake factory.

mod clock;
mod options;
mod registry;

#[cfg(test)]
mod tests;

pub use clock::{Clock, ManualClock, SystemClock};
pub use options::{
    ExecutionProvider, GraphOptimization, ModelId, SessionOptions, default_intra_threads,
};
pub use registry::{Handle, Registry, SessionError, SessionFactory, SessionStats, Snapshot};
