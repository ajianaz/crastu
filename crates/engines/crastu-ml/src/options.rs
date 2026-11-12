//! Identity and options of a model session. Together they are the registry key.

use std::fmt;

/// Identity of a model file. The caller composes it so that a changed file is a new id (for
/// example the path plus the size and the sha256 pin from `crastu-models`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelId(String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a session runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExecutionProvider {
    /// Plain CPU: the default, and deterministic.
    #[default]
    Cpu,
    /// Core ML on Apple hardware: opt in, and not bit-for-bit deterministic.
    CoreMl,
}

/// How far the runtime rewrites the graph before running it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphOptimization {
    Disabled,
    Basic,
    Extended,
    #[default]
    All,
}

/// Options a session is created with. Two loads share a session only if these are equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionOptions {
    pub provider: ExecutionProvider,
    /// Intra-op threads, at least 1.
    pub intra_threads: usize,
    pub graph_optimization: GraphOptimization,
}

/// The thread count used when none is chosen: the core count, capped at 4.
pub fn default_intra_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4)
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            provider: ExecutionProvider::Cpu,
            intra_threads: default_intra_threads(),
            graph_optimization: GraphOptimization::default(),
        }
    }
}

impl SessionOptions {
    pub fn with_provider(mut self, provider: ExecutionProvider) -> Self {
        self.provider = provider;
        self
    }

    pub fn with_intra_threads(mut self, threads: usize) -> Self {
        self.intra_threads = threads.max(1);
        self
    }

    pub fn with_graph_optimization(mut self, level: GraphOptimization) -> Self {
        self.graph_optimization = level;
        self
    }

    /// True when equal inputs give equal outputs, so a cache may rely on the result. Only the CPU
    /// provider is deterministic; Core ML results can differ between runs and devices.
    pub fn is_deterministic(&self) -> bool {
        self.provider == ExecutionProvider::Cpu
    }
}
