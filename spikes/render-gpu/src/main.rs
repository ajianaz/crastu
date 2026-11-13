//! Spike #35: Vello on wgpu (Metal) and Parley, headless. Commands: `ppm`, `det`, `speed`.

mod cpu;
mod frame;
mod gpu;

use anyhow::Result;
use frame::Frames;
use gpu::Gpu;
use sha2::{Digest, Sha256};
use std::time::Instant;
use vello::Scene;

const FONT: &str = "../../crates/ui/crastu-ui/assets/fonts/inter/Inter-SemiBold.ttf";

fn hash(px: &[u8]) -> [u8; 32] {
    Sha256::digest(px).into()
}

fn hex(h: &[u8; 32]) -> String {
    h[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Renders frame `f` (time f/30) and returns its pixels.
fn frame_px(
    g: &mut Gpu,
    fr: &mut Frames,
    scene: &mut Scene,
    f: u32,
    px: &mut Vec<u8>,
) -> Result<()> {
    fr.draw(scene, f as f64 / 30.0);
    g.render(scene)?;
    g.read_pixels(px)
}

/// Compares two pixel buffers: (differing bytes, differing pixels, max channel delta).
fn diff(a: &[u8], b: &[u8]) -> (usize, usize, u8) {
    let (mut bytes, mut pixels, mut max) = (0, 0, 0u8);
    for (pa, pb) in a.chunks(4).zip(b.chunks(4)) {
        let mut d = false;
        for (x, y) in pa.iter().zip(pb) {
            if x != y {
                bytes += 1;
                d = true;
                max = max.max(x.abs_diff(*y));
            }
        }
        pixels += d as usize;
    }
    (bytes, pixels, max)
}

fn write_ppm(path: &str, px: &[u8]) -> Result<()> {
    let mut out = format!("P6\n{} {}\n255\n", gpu::W, gpu::H).into_bytes();
    for p in px.chunks(4) {
        out.extend_from_slice(&p[..3]);
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// A fixed LCG shuffle so "random order" is reproducible.
fn shuffled(n: usize) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    let mut s = 0x2545F4914F6CDD1Du64;
    for i in (1..n).rev() {
        s = s
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        v.swap(i, (s >> 33) as usize % (i + 1));
    }
    v
}

fn det(dump: Option<&str>) -> Result<()> {
    const N: usize = 120;
    let frames_of = |i: usize| (i * 15) as u32; // 120 frames spread over 60 s
    let mut px = Vec::new();
    let mut scene = Scene::new();
    let mut g = Gpu::new()?;
    println!("adapter: {}", g.adapter_info);
    let mut fr = Frames::new(FONT, false)?;
    println!("font family: {}", fr.font_name);

    // A: the same frame twice.
    frame_px(&mut g, &mut fr, &mut scene, 37, &mut px)?;
    let a1 = hash(&px);
    let keep37 = px.clone();
    frame_px(&mut g, &mut fr, &mut scene, 37, &mut px)?;
    let a2 = hash(&px);
    println!(
        "A same frame twice: {} {} equal={}",
        hex(&a1),
        hex(&a2),
        a1 == a2
    );
    let _ = keep37;

    // B1: forward pass; keep every 10th frame's pixels for a diff report.
    let mut fwd = vec![[0u8; 32]; N];
    let mut kept: Vec<(usize, Vec<u8>)> = vec![];
    for i in 0..N {
        frame_px(&mut g, &mut fr, &mut scene, frames_of(i), &mut px)?;
        fwd[i] = hash(&px);
        if i % 10 == 0 {
            kept.push((i, px.clone()));
        }
    }
    // B2: random order, same device.
    let order = shuffled(N);
    let mut rnd = vec![[0u8; 32]; N];
    let report = |name: &str,
                  other: &[[u8; 32]],
                  px: &mut Vec<u8>,
                  kept: &[(usize, Vec<u8>)],
                  rer: &mut dyn FnMut(usize, &mut Vec<u8>) -> Result<()>|
     -> Result<()> {
        let bad: Vec<usize> = (0..N).filter(|&i| fwd[i] != other[i]).collect();
        println!(
            "{name}: {} of {N} frames differ from the forward pass {:?}",
            bad.len(),
            bad
        );
        for &(i, ref ref_px) in kept {
            if bad.contains(&i) {
                rer(i, px)?;
                let (b, p, m) = diff(ref_px, px);
                println!("   frame idx {i}: {b} bytes, {p} pixels differ, max channel delta {m}");
            }
        }
        Ok(())
    };
    for &i in &order {
        frame_px(&mut g, &mut fr, &mut scene, frames_of(i), &mut px)?;
        rnd[i] = hash(&px);
    }
    report(
        "B random order, same device",
        &rnd,
        &mut px,
        &kept,
        &mut |i, px| frame_px(&mut g, &mut fr, &mut scene, frames_of(i), px),
    )?;

    // C: a brand new instance, device, renderer, font context.
    drop(fr);
    drop(g);
    let mut g2 = Gpu::new()?;
    let mut fr2 = Frames::new(FONT, false)?;
    let mut fresh = vec![[0u8; 32]; N];
    for i in 0..N {
        frame_px(&mut g2, &mut fr2, &mut scene, frames_of(i), &mut px)?;
        fresh[i] = hash(&px);
    }
    report(
        "C fresh device, forward",
        &fresh,
        &mut px,
        &kept,
        &mut |i, px| frame_px(&mut g2, &mut fr2, &mut scene, frames_of(i), px),
    )?;
    // D: fresh device again, random order.
    drop(fr2);
    drop(g2);
    let mut g3 = Gpu::new()?;
    let mut fr3 = Frames::new(FONT, false)?;
    let mut fresh_rnd = vec![[0u8; 32]; N];
    for &i in order.iter().rev() {
        frame_px(&mut g3, &mut fr3, &mut scene, frames_of(i), &mut px)?;
        fresh_rnd[i] = hash(&px);
    }
    report(
        "D fresh device, reverse-shuffled order",
        &fresh_rnd,
        &mut px,
        &kept,
        &mut |i, px| frame_px(&mut g3, &mut fr3, &mut scene, frames_of(i), px),
    )?;
    let mut all = Sha256::new();
    for h in &fwd {
        all.update(h);
    }
    println!("combined forward hash {}", hex(&all.finalize().into()));
    if let Some(path) = dump {
        let s: String = fwd.iter().map(|h| format!("{}\n", hex(h))).collect();
        std::fs::write(path, s)?;
    }
    Ok(())
}

/// Renders one frame `n` times on one device and reports the distinct results.
fn flaky(frame: u32, n: u32) -> Result<()> {
    let mut g = Gpu::new()?;
    let mut fr = Frames::new(FONT, false)?;
    let (mut scene, mut px) = (Scene::new(), Vec::new());
    let mut first: Option<Vec<u8>> = None;
    let mut seen: std::collections::BTreeMap<String, u32> = Default::default();
    for _ in 0..n {
        if std::env::var_os("NEWSCENE").is_some() {
            scene = Scene::new();
        }
        frame_px(&mut g, &mut fr, &mut scene, frame, &mut px)?;
        let e = scene.encoding();
        let mut hh = Sha256::new();
        for v in e.path_data.iter().chain(e.draw_data.iter()) {
            hh.update(v.to_le_bytes());
        }
        // Also the glyph side: positioned glyphs, every transform and the per-run properties.
        hh.update(format!("{:?}", e.resources.glyphs).as_bytes());
        hh.update(format!("{:?}", e.transforms).as_bytes());
        for r in &e.resources.glyph_runs {
            hh.update(
                format!(
                    "{:?} {:?} {:?} {}",
                    r.transform, r.glyph_transform, r.glyphs, r.font_size
                )
                .as_bytes(),
            );
        }
        let key = format!(
            "{} scene-data={}",
            hex(&hash(&px)),
            hex(&hh.finalize().into())
        );
        *seen.entry(key).or_default() += 1;
        match &first {
            None => first = Some(px.clone()),
            Some(f) if f != &px => {
                let (b, p, m) = diff(f, &px);
                println!("  differs from first: {b} bytes, {p} pixels, max delta {m}");
                // where
                let w = gpu::W as usize;
                let mut minx = usize::MAX;
                let mut maxx = 0;
                let mut miny = usize::MAX;
                let mut maxy = 0;
                for (i, (a, c)) in f.chunks(4).zip(px.chunks(4)).enumerate() {
                    if a != c {
                        let (x, y) = (i % w, i / w);
                        minx = minx.min(x);
                        maxx = maxx.max(x);
                        miny = miny.min(y);
                        maxy = maxy.max(y);
                    }
                }
                println!("  bbox x {minx}..{maxx} y {miny}..{maxy}");
            }
            _ => {}
        }
    }
    println!("frame {frame} x{n}: distinct results {:?}", seen);
    Ok(())
}

/// Renders all 1800 frames twice on one device and lists the frames whose bytes differ.
fn scan() -> Result<()> {
    let mut g = Gpu::new()?;
    let mut fr = Frames::new(FONT, false)?;
    let (mut scene, mut px) = (Scene::new(), Vec::new());
    let mut a = Vec::new();
    for f in 0..1800 {
        frame_px(&mut g, &mut fr, &mut scene, f, &mut px)?;
        a.push(hash(&px));
    }
    let mut bad = vec![];
    for f in (0..1800).rev() {
        frame_px(&mut g, &mut fr, &mut scene, f, &mut px)?;
        if hash(&px) != a[f as usize] {
            bad.push(f);
        }
    }
    println!(
        "scan: {} of 1800 frames differ between a forward and a reverse pass: {:?}",
        bad.len(),
        bad
    );
    Ok(())
}

/// CPU renderer: frames per second, and the same frame twice and in two orders.
fn cpu(threads: u16, frames: u32) -> Result<()> {
    let mut c = cpu::Cpu::new(gpu::W as u16, gpu::H as u16, threads);
    let mut fr = Frames::new(FONT, false)?;
    fr.draw(&mut c, 20.0 / 30.0);
    let h20 = hash(c.pixels());
    write_ppm_rgba("/tmp/rg20_cpu.ppm", c.pixels())?;
    println!("cpu frame 20 sha {}", hex(&h20));
    let start = Instant::now();
    let mut fwd = vec![];
    let mut t_hash = 0.0;
    for f in 0..frames {
        fr.draw(&mut c, f as f64 / 30.0);
        let px = c.pixels();
        let h = Instant::now();
        fwd.push(hash(px));
        t_hash += h.elapsed().as_secs_f64();
    }
    let total = start.elapsed().as_secs_f64() - t_hash;
    println!("cpu: SHA-256 of the frames excluded from the time: {t_hash:.2} s");
    println!(
        "cpu threads={threads}: {frames} frames in {total:.2} s, {:.1} fps, real-time factor {:.2}x (30 fps)",
        frames as f64 / total,
        frames as f64 / 30.0 / total
    );
    let mut bad = 0;
    for f in (0..frames).rev() {
        fr.draw(&mut c, f as f64 / 30.0);
        if hash(c.pixels()) != fwd[f as usize] {
            bad += 1;
        }
    }
    println!("cpu: {bad} of {frames} frames differ between forward and reverse pass");
    Ok(())
}

fn write_ppm_rgba(path: &str, px: &[u8]) -> Result<()> {
    write_ppm(path, px)
}

fn speed(readback: bool, relayout: bool) -> Result<()> {
    const FRAMES: u32 = 1800;
    let n = Instant::now();
    let mut g = Gpu::new()?;
    println!(
        "Gpu::new (instance, device, Vello renderer incl. shaders): {:.0} ms",
        n.elapsed().as_secs_f64() * 1e3
    );
    let mut fr = Frames::new(FONT, relayout)?;
    let mut scene = Scene::new();
    let mut px = Vec::new();
    // Warm up (shader pipelines compile on first use) and time the setup separately.
    let w = Instant::now();
    frame_px(&mut g, &mut fr, &mut scene, 0, &mut px)?;
    println!(
        "first frame (cold): {:.0} ms",
        w.elapsed().as_secs_f64() * 1e3
    );
    let (mut t_scene, mut t_render, mut t_read) = (0.0, 0.0, 0.0);
    let mut acc = Sha256::new();
    let start = Instant::now();
    for f in 0..FRAMES {
        let a = Instant::now();
        fr.draw(&mut scene, f as f64 / 30.0);
        let b = Instant::now();
        g.render(&scene)?;
        if !readback {
            g.wait();
        }
        let c = Instant::now();
        if readback {
            g.read_pixels(&mut px)?;
            acc.update(&px[..64]);
        }
        let d = Instant::now();
        t_scene += (b - a).as_secs_f64();
        t_render += (c - b).as_secs_f64();
        t_read += (d - c).as_secs_f64();
    }
    let total = start.elapsed().as_secs_f64();
    println!(
        "{FRAMES} frames readback={readback} relayout={relayout}: {total:.2} s total, {:.1} fps, real-time factor {:.2}x",
        FRAMES as f64 / total,
        60.0 / total
    );
    println!(
        "  scene build {:.2} s, render submit(+wait) {:.2} s, readback {:.2} s",
        t_scene, t_render, t_read
    );
    let _ = acc;
    // Pure readback cost: the same finished texture copied 300 times.
    let r = Instant::now();
    for _ in 0..300 {
        g.read_pixels(&mut px)?;
    }
    println!(
        "  pure readback of an idle texture: {:.2} ms per frame",
        r.elapsed().as_secs_f64() * 1e3 / 300.0
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("ppm") => {
            let t: u32 = args[2].parse()?;
            let mut g = Gpu::new()?;
            let mut fr = Frames::new(FONT, false)?;
            let (mut scene, mut px) = (Scene::new(), Vec::new());
            frame_px(&mut g, &mut fr, &mut scene, t, &mut px)?;
            write_ppm(&args[3], &px)?;
            println!("wrote {} sha {}", args[3], hex(&hash(&px)));
        }
        Some("flaky") => flaky(args[2].parse()?, args[3].parse()?)?,
        Some("scan") => scan()?,
        Some("cpu") => cpu(args[2].parse()?, args[3].parse()?)?,
        Some("det") => det(args.get(2).map(String::as_str))?,
        Some("speed") => speed(
            !args.iter().any(|a| a == "--no-readback"),
            args.iter().any(|a| a == "--relayout"),
        )?,
        _ => eprintln!(
            "usage: spike-render-gpu ppm <frame> <out.ppm> | det [dump] | speed [--no-readback] [--relayout]"
        ),
    }
    Ok(())
}
