// With the `shared` feature the program loads libsherpa-onnx-c-api and libonnxruntime at run
// time. sherpa-onnx-sys emits an rpath for its own targets only: link arguments from a
// dependency's build script do not reach the final binary, so the application sets its own.
fn main() {
    println!("cargo:rerun-if-env-changed=SHERPA_ONNX_LIB_DIR");
    if std::env::var_os("CARGO_FEATURE_SHARED").is_some() {
        if let Some(dir) = std::env::var_os("SHERPA_ONNX_LIB_DIR") {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{}", dir.to_string_lossy());
        }
        // Where a packaged app would keep them next to the executable.
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
    }
}
