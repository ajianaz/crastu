//! Maps the registry's [`SessionOptions`] onto ONNX Runtime session options.

use ort::ep::ExecutionProviderDispatch;
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};

use crate::{ExecutionProvider, GraphOptimization, SessionOptions};

pub(crate) fn optimization_level(level: GraphOptimization) -> GraphOptimizationLevel {
    match level {
        GraphOptimization::Disabled => GraphOptimizationLevel::Disable,
        GraphOptimization::Basic => GraphOptimizationLevel::Level1,
        GraphOptimization::Extended => GraphOptimizationLevel::Level2,
        GraphOptimization::All => GraphOptimizationLevel::All,
    }
}

/// The execution providers to register, or the reason the requested one cannot be used. The CPU
/// provider is always there (ONNX Runtime falls back to it for anything else), so it is the empty
/// list.
pub(crate) fn providers(
    provider: ExecutionProvider,
) -> Result<Vec<ExecutionProviderDispatch>, String> {
    match provider {
        ExecutionProvider::Cpu => Ok(Vec::new()),
        #[cfg(feature = "coreml")]
        // `error_on_failure`: a caller who asked for Core ML must not silently get the CPU.
        ExecutionProvider::CoreMl => {
            Ok(vec![ort::ep::CoreML::default().build().error_on_failure()])
        }
        #[cfg(not(feature = "coreml"))]
        ExecutionProvider::CoreMl => Err(
            "Core ML was asked for, but crastu-ml was built without its `coreml` feature"
                .to_string(),
        ),
    }
}

/// Applies `options` to a fresh builder.
pub(crate) fn apply(
    mut builder: SessionBuilder,
    options: &SessionOptions,
) -> Result<SessionBuilder, String> {
    let providers = providers(options.provider)?;
    let err = |what: &str, e: String| format!("{what}: {e}");
    if !providers.is_empty() {
        builder = builder
            .with_execution_providers(providers)
            .map_err(|e| err("execution provider", e.to_string()))?;
    }
    builder = builder
        .with_intra_threads(options.intra_threads.max(1))
        .map_err(|e| err("intra-op threads", e.to_string()))?;
    builder = builder
        .with_optimization_level(optimization_level(options.graph_optimization))
        .map_err(|e| err("graph optimisation level", e.to_string()))?;
    // `SetDeterministicCompute` asks kernels to prefer reproducible algorithms. That gives the same
    // result on one machine, with one runtime build and one thread count; it is not bit-exact
    // across machines, CPU features, runtime versions or thread counts.
    if options.is_deterministic() {
        builder = builder
            .with_deterministic_compute(true)
            .map_err(|e| err("deterministic compute", e.to_string()))?;
    }
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_maps_to_its_own_ort_level() {
        let all = [
            GraphOptimization::Disabled,
            GraphOptimization::Basic,
            GraphOptimization::Extended,
            GraphOptimization::All,
        ];
        let mapped: Vec<_> = all
            .iter()
            .map(|l| format!("{:?}", optimization_level(*l)))
            .collect();
        assert_eq!(mapped, ["Disable", "Level1", "Level2", "All"]);
    }

    #[test]
    fn cpu_registers_nothing_and_core_ml_needs_its_feature() {
        assert!(providers(ExecutionProvider::Cpu).unwrap().is_empty());
        match providers(ExecutionProvider::CoreMl) {
            Ok(list) => assert!(cfg!(feature = "coreml") && list.len() == 1),
            Err(reason) => assert!(!cfg!(feature = "coreml") && reason.contains("coreml")),
        }
    }
}
