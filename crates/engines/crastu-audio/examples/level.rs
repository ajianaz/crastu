//! Prints the live microphone level meter for 5 seconds.
//!
//! `cargo run -p crastu-audio --example level --features mic [-- "device name"]`
//!
//! Needs a microphone and, on macOS, the microphone permission for the terminal running it:
//! this is the hands-on check, no test covers it.

use std::time::{Duration, Instant};

use crastu_audio::{
    AudioSource, Converter, EnergyGate, MicSource, ReadResult, VoiceGate, input_device_names,
};

fn bar(db: f32) -> String {
    let n = (((db + 60.0) / 60.0).clamp(0.0, 1.0) * 40.0) as usize;
    format!("{:<40}", "#".repeat(n))
}

fn main() {
    let name = std::env::args().nth(1);
    match input_device_names() {
        Ok(names) => println!("input devices: {names:?}"),
        Err(e) => println!("cannot list devices: {e}"),
    }
    let mut mic = match MicSource::new(name.as_deref()) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cannot open the microphone: {e}");
            std::process::exit(1);
        }
    };
    println!("capturing at {} Hz for 5 seconds", mic.sample_rate());
    if let Err(e) = mic.start() {
        eprintln!("cannot start: {e}");
        std::process::exit(1);
    }
    let mut converter = match Converter::new(mic.sample_rate()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("unusable sample rate: {e}");
            std::process::exit(1);
        }
    };
    let mut gate = EnergyGate::default();
    let mut buf = vec![0.0f32; 4096];
    let mut frames = Vec::new();
    let (start, mut last) = (Instant::now(), Instant::now());
    while start.elapsed() < Duration::from_secs(5) {
        match mic.read(&mut buf) {
            Ok(ReadResult::Samples(n)) => converter.push(&buf[..n], &mut frames),
            Ok(ReadResult::Pending) => std::thread::sleep(Duration::from_millis(5)),
            Ok(ReadResult::End) => break,
            Err(e) => {
                eprintln!("capture error: {e}");
                break;
            }
        }
        let mut speech = false;
        for f in &frames {
            speech |= gate.process(&f.samples).is_speech();
        }
        if let Some(f) = frames.last()
            && last.elapsed() >= Duration::from_millis(100)
        {
            last = Instant::now();
            println!(
                "[{}] peak {:6.1} dB  rms {:6.1} dB {}{}",
                bar(f.level.rms_db),
                f.level.peak_db,
                f.level.rms_db,
                if speech { " speech" } else { "" },
                if f.clipped { " CLIP" } else { "" },
            );
        }
        frames.clear();
    }
    mic.stop();
    println!("{:?}", mic.stats());
}
