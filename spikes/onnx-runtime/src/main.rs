//! Spike #2: one ONNX Runtime shared by sherpa-onnx (Whisper) and uteke-core (EmbeddingGemma).
//!
//! Usage:
//!   spike-onnx-runtime asr   <whisper model dir> <wav>
//!   spike-onnx-runtime embed <store dir>
//!   spike-onnx-runtime both  <whisper model dir> <wav> <store dir>
//!
//! `both` runs speech recognition, then stores and recalls memories, then recognises again,
//! all in one process, and prints one JSON line: timings, resident memory after each step and
//! the ONNX Runtime images loaded into the process. uteke finds its runtime through the
//! `ORT_LIB_PATH` environment variable; its model folder through `UTEKE_HOME`.
//!
//! Unsafe code: only the dyld image list (`libc`), to see which runtimes are loaded.

use std::ffi::CStr;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, Wave};

fn rss_mb() -> f64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output();
    out.ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<f64>()
                .ok()
        })
        .map_or(-1.0, |kb| kb / 1024.0)
}

fn peak_rss_mb() -> f64 {
    // SAFETY: getrusage fills the zeroed struct we pass; on macOS ru_maxrss is in bytes.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    usage.ru_maxrss as f64 / (1024.0 * 1024.0)
}

/// Every loaded image whose path mentions an ONNX Runtime or sherpa library.
// libc marks the dyld functions deprecated in favour of the mach2 crate; one call site here, so
// no extra dependency for a spike.
#[allow(deprecated)]
fn loaded_runtimes() -> Vec<String> {
    let mut found = Vec::new();
    // SAFETY: dyld returns valid, NUL-terminated image names for indices below the count.
    unsafe {
        for i in 0..libc::_dyld_image_count() {
            let name = libc::_dyld_get_image_name(i);
            if name.is_null() {
                continue;
            }
            let name = CStr::from_ptr(name).to_string_lossy().into_owned();
            if name.contains("onnxruntime") || name.contains("sherpa") {
                found.push(name);
            }
        }
    }
    found
}

fn find(dir: &Path, suffix: &str) -> Result<String> {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with(suffix))
        })
        .collect();
    hits.sort();
    // Prefer the int8 files when both exist.
    hits.sort_by_key(|p| !p.to_string_lossy().contains("int8"));
    hits.first()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| anyhow!("no *{suffix} in {}", dir.display()))
}

fn recognizer(model_dir: &Path) -> Result<OfflineRecognizer> {
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.whisper.encoder =
        Some(find(model_dir, "-encoder.int8.onnx").or_else(|_| find(model_dir, "-encoder.onnx"))?);
    config.model_config.whisper.decoder =
        Some(find(model_dir, "-decoder.int8.onnx").or_else(|_| find(model_dir, "-decoder.onnx"))?);
    config.model_config.whisper.language = Some("en".into());
    config.model_config.whisper.task = Some("transcribe".into());
    config.model_config.tokens = Some(find(model_dir, "-tokens.txt")?);
    config.model_config.num_threads = 2;
    config.model_config.provider = Some("cpu".into());
    OfflineRecognizer::create(&config)
        .ok_or_else(|| anyhow!("OfflineRecognizer::create returned None"))
}

fn transcribe(rec: &OfflineRecognizer, wav: &str) -> Result<(String, f64, f64)> {
    let wave = Wave::read(wav).ok_or_else(|| anyhow!("cannot read {wav}"))?;
    let seconds = f64::from(wave.num_samples()) / f64::from(wave.sample_rate());
    let start = Instant::now();
    let stream = rec.create_stream();
    stream.accept_waveform(wave.sample_rate(), wave.samples());
    rec.decode(&stream);
    let text = stream.get_result().map(|r| r.text).unwrap_or_default();
    Ok((
        text.trim().to_string(),
        start.elapsed().as_secs_f64() * 1000.0,
        seconds,
    ))
}

fn embed(store: &str) -> Result<Value> {
    let start = Instant::now();
    let memory = uteke_core::Uteke::open(store).map_err(|e| anyhow!("open: {e}"))?;
    let sentences = [
        "The microphone permission is asked once during onboarding.",
        "Dictation text is pasted into the focused app and the clipboard is restored.",
        "The video preview decodes with VideoToolbox straight into a GPUI surface.",
        "Licences of every model are checked before it is offered for download.",
        "The host runs headless and speaks MCP over a local socket.",
    ];
    for s in sentences {
        memory
            .remember(s, &["spike"], None, None)
            .map_err(|e| anyhow!("remember: {e}"))?;
    }
    let first = start.elapsed().as_secs_f64() * 1000.0;
    let t = Instant::now();
    let hits = memory
        .recall(
            "how does the video preview decode frames",
            3,
            None,
            None,
            0.0,
            None,
            None,
        )
        .map_err(|e| anyhow!("recall: {e}"))?;
    let recall_ms = t.elapsed().as_secs_f64() * 1000.0;
    let top = hits
        .first()
        .map(|h| h.memory.content.clone())
        .unwrap_or_default();
    Ok(
        json!({"store_and_first_embed_ms": first, "recall_ms": recall_ms, "hits": hits.len(), "top": top}),
    )
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| {
        args.get(i)
            .map(String::as_str)
            .ok_or_else(|| anyhow!("missing argument {i}"))
    };
    let mut steps = Vec::new();
    let mut step = |name: &str, detail: Value| {
        steps.push(json!({"step": name, "rss_mb": rss_mb(), "detail": detail}));
    };
    step("start", json!({"runtimes": loaded_runtimes()}));
    match arg(0)? {
        "asr" => {
            let t = Instant::now();
            let rec = recognizer(Path::new(arg(1)?))?;
            step(
                "whisper loaded",
                json!({"load_ms": t.elapsed().as_secs_f64() * 1000.0, "runtimes": loaded_runtimes()}),
            );
            let (text, ms, secs) = transcribe(&rec, arg(2)?)?;
            step(
                "transcribed",
                json!({"text": text, "ms": ms, "audio_s": secs}),
            );
        }
        "embed" => {
            let r = embed(arg(1)?)?;
            step(
                "uteke store + recall",
                json!({"result": r, "runtimes": loaded_runtimes()}),
            );
        }
        "both" => {
            let t = Instant::now();
            let rec = recognizer(Path::new(arg(1)?))?;
            step(
                "whisper loaded",
                json!({"load_ms": t.elapsed().as_secs_f64() * 1000.0, "runtimes": loaded_runtimes()}),
            );
            let (text, ms, secs) = transcribe(&rec, arg(2)?)?;
            step(
                "transcribed",
                json!({"text": text, "ms": ms, "audio_s": secs}),
            );
            let r = embed(arg(3)?)?;
            step(
                "uteke store + recall",
                json!({"result": r, "runtimes": loaded_runtimes()}),
            );
            let (text2, ms2, _) = transcribe(&rec, arg(2)?)?;
            step(
                "transcribed again",
                json!({"same_text": text2 == text, "ms": ms2}),
            );
        }
        other => bail!("unknown command {other}"),
    }
    println!(
        "{}",
        json!({"peak_rss_mb": peak_rss_mb(), "ort_lib_path": std::env::var("ORT_LIB_PATH").ok(), "steps": steps})
    );
    Ok(())
}
