//! Spike #30: pure-Rust MP4 demux (`mp4`), MP4 mux (`muxide`) and audio decode (`symphonia`),
//! tried on real files. Throwaway code: it measures, it does not ship.
//!
//! ```text
//! spike-media-backends demux <file.mp4>
//! spike-media-backends remux <in.mp4> <out.mp4>     (H.264 + AAC only)
//! spike-media-backends audio <file>...
//! ```

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use muxide::api::{AacProfile, AudioCodec, MuxerBuilder, VideoCodec};
use symphonia::core::audio::sample::Sample;
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("demux") => demux(&args[2]),
        Some("remux") => remux(&args[2], &args[3]),
        Some("audio") => {
            for path in &args[2..] {
                match audio(path) {
                    Ok(line) => println!("{line}"),
                    Err(e) => println!("{path}: FAILED {e}"),
                }
            }
            Ok(())
        }
        _ => bail!("usage: demux <mp4> | remux <in.mp4> <out.mp4> | audio <file>..."),
    }
}

fn open_mp4(path: &str) -> Result<mp4::Mp4Reader<BufReader<File>>> {
    let file = File::open(path)?;
    let size = file.metadata()?.len();
    Ok(mp4::Mp4Reader::read_header(BufReader::new(file), size)?)
}

fn demux(path: &str) -> Result<()> {
    let start = Instant::now();
    let mut reader = open_mp4(path)?;
    let header_ms = start.elapsed().as_secs_f64() * 1000.0;
    let ids: Vec<u32> = reader.tracks().keys().copied().collect();
    let mut bytes = 0usize;
    let mut samples = 0u32;
    for id in &ids {
        let track = &reader.tracks()[id];
        let kind = track
            .media_type()
            .map(|m| format!("{m:?}"))
            .unwrap_or_else(|e| format!("? ({e})"));
        let boxt = track
            .box_type()
            .map(|b| b.to_string())
            .unwrap_or_else(|e| format!("? ({e})"));
        let m = &track.trak.tkhd.matrix;
        println!(
            "track {id}: {kind} {boxt} {}x{} samples={} tkhd matrix a={} b={} c={} d={}",
            track.width(),
            track.height(),
            track.sample_count(),
            m.a,
            m.b,
            m.c,
            m.d
        );
        let count = track.sample_count();
        for s in 1..=count {
            if let Some(sample) = reader.read_sample(*id, s)? {
                bytes += sample.bytes.len();
                samples += 1;
            }
        }
    }
    println!(
        "{path}: header {header_ms:.2} ms, read {samples} samples ({:.1} MB) in {:.1} ms total",
        bytes as f64 / 1e6,
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

/// AVCC (4-byte lengths) to Annex B, with SPS and PPS in front of every key frame.
fn avcc_to_annexb(data: &[u8], sps: &[u8], pps: &[u8], key: bool) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() + 64);
    if key {
        for nal in [sps, pps] {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(nal);
        }
    }
    let mut i = 0;
    while i + 4 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        i += 4;
        if len == 0 || i + len > data.len() {
            bail!(
                "truncated or empty NAL unit (length {len} at offset {i} of {})",
                data.len()
            );
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[i..i + len]);
        i += len;
    }
    Ok(out)
}

/// A raw AAC-LC frame in an ADTS header (muxide wants ADTS).
fn adts(frame: &[u8], sample_rate: u32, channels: u8) -> Vec<u8> {
    let idx = [
        96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000,
    ]
    .iter()
    .position(|r| *r == sample_rate)
    .unwrap_or(3) as u8;
    let len = frame.len() + 7;
    let mut h = [0u8; 7];
    h[0] = 0xFF;
    h[1] = 0xF1; // MPEG-4, layer 0, no CRC
    h[2] = (1 << 6) | (idx << 2) | (channels >> 2); // AAC-LC
    h[3] = ((channels & 3) << 6) | ((len >> 11) as u8 & 3);
    h[4] = ((len >> 3) & 0xFF) as u8;
    h[5] = (((len & 7) as u8) << 5) | 0x1F;
    h[6] = 0xFC;
    let mut out = h.to_vec();
    out.extend_from_slice(frame);
    out
}

fn remux(input: &str, output: &str) -> Result<()> {
    let start = Instant::now();
    let mut reader = open_mp4(input)?;
    let mut video = None;
    let mut audio = None;
    for (id, t) in reader.tracks() {
        match t.media_type() {
            Ok(mp4::MediaType::H264) => video = Some(*id),
            Ok(mp4::MediaType::AAC) => audio = Some(*id),
            _ => {}
        }
    }
    let vid = video.context(
        "no H.264 track (the mp4 crate reads H.264 and H.265 headers only for AVC here)",
    )?;
    let (w, h, fps, sps, pps, vts) = {
        let t = &reader.tracks()[&vid];
        (
            t.width() as u32,
            t.height() as u32,
            t.frame_rate(),
            t.sequence_parameter_set()?.to_vec(),
            t.picture_parameter_set()?.to_vec(),
            t.timescale() as f64,
        )
    };
    let mut builder =
        MuxerBuilder::new(BufWriter::new(File::create(output)?)).video(VideoCodec::H264, w, h, fps);
    let mut ats = 48000.0;
    if let Some(aid) = audio {
        let t = &reader.tracks()[&aid];
        ats = t.timescale() as f64;
        builder = builder.audio(AudioCodec::Aac(AacProfile::Lc), t.timescale(), 2);
    }
    let mut muxer = builder.build()?;
    // Interleave by decode time: all video first is legal but not nice; merge on time.
    let vcount = reader.tracks()[&vid].sample_count();
    let acount = audio
        .map(|a| reader.tracks()[&a].sample_count())
        .unwrap_or(0);
    let first_vpts = {
        let s = reader.read_sample(vid, 1)?.context("video sample")?;
        (s.start_time as i64 + s.rendering_offset as i64) as f64 / vts
    };
    let (mut vi, mut ai) = (1u32, 1u32);
    let mut written = (0u32, 0u32);
    while vi <= vcount || ai <= acount {
        let take_video = if std::env::var_os("SPIKE_VIDEO_FIRST").is_some() {
            vi <= vcount
        } else if ai > acount {
            true
        } else if vi > vcount {
            false
        } else {
            let v = reader.read_sample(vid, vi)?.context("video sample")?;
            let a = reader
                .read_sample(audio.unwrap(), ai)?
                .context("audio sample")?;
            (v.start_time as f64 / vts) <= (a.start_time as f64 / ats)
        };
        if take_video {
            let s = reader.read_sample(vid, vi)?.context("video sample")?;
            let pts = (s.start_time as i64 + s.rendering_offset as i64) as f64 / vts;
            let dts = s.start_time as f64 / vts;
            let data = avcc_to_annexb(&s.bytes, &sps, &pps, s.is_sync)?;
            muxer.write_video_with_dts(pts, dts, &data, s.is_sync)?;
            vi += 1;
            written.0 += 1;
        } else {
            let s = reader
                .read_sample(audio.unwrap(), ai)?
                .context("audio sample")?;
            // muxide rejects audio that starts before the first video pts (gotcha found here):
            // drop the audio frames that come before it.
            let t = s.start_time as f64 / ats;
            if t >= first_vpts {
                muxer.write_audio(t, &adts(&s.bytes, ats as u32, 2))?;
                written.1 += 1;
            }
            ai += 1;
        }
    }
    muxer.finish()?;
    println!(
        "remuxed {} video and {} audio samples to {output} in {:.1} ms",
        written.0,
        written.1,
        start.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

fn audio(path: &str) -> Result<String> {
    let start = Instant::now();
    let mss = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| anyhow::anyhow!("container not recognised: {e}"))?;
    let track = format
        .default_track(TrackType::Audio)
        .context("no audio track")?;
    let params = track
        .codec_params
        .as_ref()
        .context("no codec parameters")?
        .audio()
        .context("not an audio track")?;
    let codec = format!("{:?}", params.codec);
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| anyhow::anyhow!("codec {codec}: {e}"))?;
    let id = track.id;
    let mut samples: Vec<f32> = Vec::new();
    let mut total = 0usize;
    while let Some(packet) = format.next_packet()? {
        if packet.track_id != id {
            continue;
        }
        if let Ok(buf) = decoder.decode(&packet) {
            samples.resize(buf.samples_interleaved(), f32::MID);
            buf.copy_to_slice_interleaved(&mut samples);
            total += samples.len();
        }
    }
    Ok(format!(
        "{path}: codec {codec}, {total} interleaved samples in {:.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    ))
}
