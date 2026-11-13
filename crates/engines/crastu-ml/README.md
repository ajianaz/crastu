# crastu-ml

Model session registry: one session per (model file, options) per process, reference counted, freed after an idle time (issue #26, decisions 31 and 36). The registry knows nothing about a runtime; a `SessionFactory` builds the sessions.

- `Registry`, `Handle`, `SessionOptions` (execution provider, intra-op threads, graph optimisation level), `Snapshot` (sessions, handles per model, approximate memory). Embedders call `Registry::sweep` now and then so idle sessions release their memory.
- `lib_path`: where the ONNX Runtime library is looked for (plain logic, no feature needed): next to the executable, then `ORT_DYLIB_PATH` (a file, or a folder that holds `libonnxruntime.dylib`, `libonnxruntime.so` or `onnxruntime.dll`).
- `onnx::OrtFactory` (feature `ort`, off by default): the real factory over `ort` 2.0.0-rc.15 with `load-dynamic`. It links no native code at build time, so the feature compiles for iOS and Android too. Register each model first, hand a clone to the registry:

```rust
let factory = OrtFactory::new(); // or OrtFactory::with_library(path)
factory.register_model(ModelId::new("silero-vad@<sha256>"), "/path/to/model.onnx");
let registry = Registry::new(factory.clone(), Duration::from_secs(60));
let handle = registry.load(&ModelId::new("silero-vad@<sha256>"), SessionOptions::default())?;
let mut session = handle.lock(); // ort runs a session through &mut
```

Notes: the runtime is process-global: the first library that loads wins and a later `with_library` naming another file is an error. ONNX Runtime telemetry is switched off; ONNX Runtime has a telemetry provider on Windows only, so that call matters there, and `ort` has no getter to test it. CPU sessions ask for deterministic compute: the same result on one machine, runtime build and thread count, not bit-exact across machines, versions or thread counts. Core ML (feature `coreml`) is not repeatable and is untested. `ort` rc.15 needs Rust 1.92 (the workspace minimum is 1.92 for this reason). `mobile-check` proves compilation only: `load-dynamic` cannot ship on iOS (the App Store forbids loading unsigned dylibs), which needs a static link later. `approx_memory` is the model file size (external weight files are not counted), and runs on one shared session take turns on its lock. `ort` accepts a runtime of ONNX Runtime 1.18 or newer; 1.28.2 (decision 28) was run.

Security: the library is looked for next to the executable first (right for a signed bundle), then in `ORT_DYLIB_PATH`; a relative value is tried against the executable folder first, then as given. A library in a user-writable folder is loaded with the app's privileges, so on Unix a library or folder that group or others can write is refused; keep the macOS hardened runtime's library validation on. No hash or signature is checked, and nothing is checked on Windows.

## Tests

`cargo test -p crastu-ml --features ort` needs no runtime. The real-session tests are `#[ignore]` and need two variables:

- `CRASTU_TEST_ORT_DYLIB`: the runtime library (for example `libonnxruntime.1.28.2.dylib` from the official `onnxruntime-osx-arm64-1.28.2.tgz` release; check its sha256 against the release page);
- `CRASTU_TEST_ONNX_MODEL`: a model with one float32 input `[1, 4]` and one float32 output `[1, 4]` computing `y = 2 * x`. To make one:

```python
import onnx
from onnx import helper, TensorProto
x = helper.make_tensor_value_info("x", TensorProto.FLOAT, [1, 4])
y = helper.make_tensor_value_info("y", TensorProto.FLOAT, [1, 4])
two = helper.make_tensor("two", TensorProto.FLOAT, [1], [2.0])
g = helper.make_graph([helper.make_node("Mul", ["x", "two"], ["y"])], "double", [x], [y], [two])
m = helper.make_model(g, opset_imports=[helper.make_opsetid("", 13)])
m.ir_version = 8
onnx.save(m, "double.onnx")
```

```sh
CRASTU_TEST_ORT_DYLIB=/path/libonnxruntime.dylib CRASTU_TEST_ONNX_MODEL=double.onnx \
  cargo test -p crastu-ml --features ort -- --ignored --test-threads=1
```

Never commit the library or a model. Run them with `--ignored`, not `--include-ignored`: a process holds one runtime for its life.
