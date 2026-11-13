//! Tests of the ONNX factory. The ones that need no runtime run everywhere the feature is on.
//! The real-session tests are `#[ignore]` and driven by two variables (see the crate README):
//!
//! - `CRASTU_TEST_ORT_DYLIB`: the ONNX Runtime library file;
//! - `CRASTU_TEST_ONNX_MODEL`: a model with one float32 input of shape `[1, 4]` and one float32
//!   output of the same shape, computing `y = 2 * x` (the README shows how to make one).
//!
//! The process holds one runtime for its whole life, so run the ignored tests on their own
//! (`--ignored`, not `--include-ignored`): a real session in the same process would make the
//! "library missing" test see the runtime that is already loaded.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ort::session::Session;
use ort::value::Tensor;

use super::*;
use crate::{ManualClock, Registry};

fn model_id() -> ModelId {
    ModelId::new("test-model")
}

#[test]
fn unregistered_model_is_a_clear_error_not_a_panic() {
    let err = OrtFactory::new()
        .create(&model_id(), &SessionOptions::default())
        .unwrap_err();
    assert!(err.reason.contains("no file is registered"), "{err}");
    assert_eq!(err.model, "test-model");
}

#[test]
fn missing_model_file_is_an_error_that_names_the_file() {
    let factory = OrtFactory::new();
    factory.register_model(model_id(), "/nonexistent/model.onnx");
    let err = factory
        .create(&model_id(), &SessionOptions::default())
        .unwrap_err();
    assert!(err.reason.contains("/nonexistent/model.onnx"), "{err}");
}

#[test]
fn missing_runtime_names_the_path_and_does_not_panic() {
    let dir = std::env::temp_dir().join(format!("crastu-ml-no-ort-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("m.onnx");
    std::fs::write(&model, b"not a model").unwrap();
    let library = dir.join("libnot-onnxruntime.dylib");

    let factory = OrtFactory::with_library(&library);
    factory.register_model(model_id(), &model);
    let err = factory
        .create(&model_id(), &SessionOptions::default())
        .unwrap_err();
    assert!(err.reason.contains("cannot load the ONNX Runtime"), "{err}");
    assert!(err.reason.contains(&library.display().to_string()), "{err}");
    assert!(factory.preload().is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_factory_is_a_registry_factory_and_clones_share_models() {
    let factory = OrtFactory::new();
    let registry = Registry::new(factory.clone(), Duration::from_secs(1));
    factory.register_model(model_id(), "/nonexistent/model.onnx");
    // The clone inside the registry sees the registration made through the other one.
    let err = registry
        .load(&model_id(), SessionOptions::default())
        .map(drop)
        .unwrap_err();
    assert!(err.reason.contains("/nonexistent/model.onnx"), "{err}");
    assert_eq!(
        registry.snapshot().session_count(),
        0,
        "a failure is not cached"
    );
}

fn real_paths() -> Option<(PathBuf, PathBuf)> {
    let dylib = std::env::var_os("CRASTU_TEST_ORT_DYLIB")?;
    let model = std::env::var_os("CRASTU_TEST_ONNX_MODEL")?;
    Some((dylib.into(), model.into()))
}

fn run_doubling(session: &OrtSession) -> Vec<f32> {
    let mut guard = session.lock();
    let input_name = guard.inputs()[0].name().to_string();
    let tensor = Tensor::from_array(([1usize, 4], vec![1.0f32, 2.0, 3.0, 4.0])).unwrap();
    let outputs = guard.run(ort::inputs![input_name => tensor]).unwrap();
    let (_, data) = outputs[0].try_extract_tensor::<f32>().unwrap();
    data.to_vec()
}

#[test]
#[ignore = "needs CRASTU_TEST_ORT_DYLIB and CRASTU_TEST_ONNX_MODEL"]
fn real_session_is_shared_between_two_handles_and_freed_after_both_drop() {
    let Some((dylib, model)) = real_paths() else {
        panic!("set CRASTU_TEST_ORT_DYLIB and CRASTU_TEST_ONNX_MODEL");
    };
    let factory = OrtFactory::with_library(dylib);
    factory.register_model(model_id(), &model);
    let clock = Arc::new(ManualClock::default());
    let registry = Registry::with_clock(factory, Duration::from_secs(30), clock.clone());
    let options = SessionOptions::default().with_intra_threads(2);

    let a = registry.load(&model_id(), options).expect("first load");
    let b = registry.load(&model_id(), options).expect("second load");
    assert!(
        std::ptr::eq::<OrtSession>(&*a, &*b),
        "two handles must hold one session"
    );
    let snap = registry.snapshot();
    assert_eq!(snap.session_count(), 1);
    assert_eq!(snap.handles_for(&model_id()), 2);
    let size = std::fs::metadata(&model).unwrap().len();
    assert_eq!(snap.approx_memory(), Some(size));

    assert_eq!(run_doubling(&a), [2.0, 4.0, 6.0, 8.0]);
    assert_eq!(run_doubling(&b), [2.0, 4.0, 6.0, 8.0]);
    assert!(options.is_deterministic());

    // Other options are another session.
    let c = registry
        .load(&model_id(), options.with_intra_threads(1))
        .expect("third load");
    assert!(!std::ptr::eq::<OrtSession>(&*a, &*c));
    assert_eq!(registry.snapshot().session_count(), 2);
    drop(c);

    drop(a);
    assert_eq!(registry.snapshot().handles_for(&model_id()), 1);
    drop(b);
    // Idle, but kept until the idle time has passed.
    assert_eq!(registry.snapshot().total_handles(), 0);
    assert_eq!(registry.snapshot().session_count(), 2);
    clock.advance(Duration::from_secs(31));
    registry.sweep();
    assert_eq!(registry.snapshot().session_count(), 0, "freed after both");
}

/// The runtime found by the search (no `with_library`), through `ORT_DYLIB_PATH` set on a child.
#[test]
#[ignore = "needs CRASTU_TEST_ORT_DYLIB and CRASTU_TEST_ONNX_MODEL"]
fn runtime_is_found_through_ort_dylib_path_in_a_child_process() {
    let Some((dylib, model)) = real_paths() else {
        panic!("set CRASTU_TEST_ORT_DYLIB and CRASTU_TEST_ONNX_MODEL");
    };
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "onnx::tests::child_uses_the_search",
            "--nocapture",
        ])
        .env(lib_path::ENV_VAR, dylib)
        .env("CRASTU_TEST_ONNX_MODEL", model)
        .env_remove("CRASTU_TEST_ORT_DYLIB")
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "child failed: {text}");
    assert!(text.contains("child-ran-through-search"), "{text}");
}

#[test]
#[ignore = "run by the test above with ORT_DYLIB_PATH set"]
fn child_uses_the_search() {
    let Some(model) = std::env::var_os("CRASTU_TEST_ONNX_MODEL") else {
        return;
    };
    if std::env::var_os(lib_path::ENV_VAR).is_none() {
        return;
    }
    let factory = OrtFactory::new();
    factory.register_model(model_id(), model);
    let session = factory
        .create(&model_id(), &SessionOptions::default())
        .expect("search finds the runtime");
    assert_eq!(run_doubling(&session), [2.0, 4.0, 6.0, 8.0]);
    let _: &Mutex<Session> = &session.session;
    println!("child-ran-through-search");
}
