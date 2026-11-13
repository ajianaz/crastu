# Spike: media backends behind one seam, and whether macOS needs FFmpeg at all

Issue #30. Run on 2026-11-13. Code: [`spikes/media-backends`](../../spikes/media-backends) (standalone crate, not part of the workspace; it only holds the pure-Rust trials of section 2.5 and 2.6). The Swift measurement tools were throwaway and are not committed; their source is described in the method section.

## Question

`crastu-media` plans FFmpeg 8 LGPL shared builds pinned by sha256 (decision 27 already decodes preview frames with VideoToolbox; the FFmpeg LGPL and sha256 rules are in ARCHITECTURE.md section 9). Can AVFoundation, VideoToolbox and AudioToolbox probe, decode and encode for export, with muxing in the platform or in Rust, so that the macOS release does not need FFmpeg? And what is the seam so that FFmpeg, Apple, Media Foundation and pure-Rust parts are interchangeable?

## Result in short

- **v0.3 on macOS ships FFmpeg, as the fallback and not as the default.** The native path (AVFoundation and VideoToolbox) is far cheaper where it works, but it cannot read a large part of what an auto-clipper is fed (MKV, WebM, AVI, VP9, AV1 on this M2, and more; section 3). A user who drops a `yt-dlp` merge or an OBS MKV on the app would get "cannot read". Dropping FFmpeg is not possible in v0.3 without refusing those files. Recorded as decision 37. The "one in five" share is a guess, and the LGPL macOS build source is not verified yet (follow-up 4), so the decision is to keep the fallback, not to commit to a particular FFmpeg build.
- **The native path wins on cost.** Decoding 10 s of 4K HEVC took about 0.10 CPU-seconds and 40 MB through AVFoundation, against about 7.8 CPU-seconds and 335 MB for FFmpeg's software decoder (section 2.2). Encoding H.264 at 8 Mb/s took 0.44 CPU-seconds against 29.7 for x264 `fast` (GPL, measuring only) at the same bitrate, with the same PSNR (40.0 dB) (section 2.4).
- **An LGPL FFmpeg adds no encoder quality on macOS.** An LGPL build has no x264 or x265, so its encoders are the same VideoToolbox hardware encoders (FFmpeg's `h264_videotoolbox` and AVFoundation gave 40.5 and 40.0 dB at 8 Mb/s). FFmpeg earns its place on macOS by reading what Apple cannot, not by encoding better.
- **Muxing does not need a Rust crate on macOS**: `AVAssetWriter` wrote the MP4 files used here. The pure-Rust candidates are for the other platforms and for the case where we drive VideoToolbox sessions ourselves. `muxide` 0.5.0 worked for video-only H.264 with B-frames, but its video plus AAC output was rejected by FFmpeg's decoder when the video has B-frames (section 2.5), so it is not ready to trust.
- **Licensing is not settled by this spike, and one finding needs a lawyer**: Apple's macOS licence says its AVC functionality is licensed "ONLY FOR THE PERSONAL AND NON-COMMERCIAL USE OF A CONSUMER" and that commercial use of H.264/AVC requires additional licensing (section 5). The clause limits the AVC functionality of Apple's own software; whether it reaches an app that calls the frameworks, or an FFmpeg build shipped inside the app, is not answered by the text. Open item for counsel, not an answer.

## Setup

| Item | Value |
|---|---|
| Machine | Mac mini (Mac14,3), Apple M2 (8 CPU cores), 8 GB, macOS 26.5.1. Other builds from other sessions ran on the same machine, so wall times are noisy (see the repeats below); CPU-seconds are steadier |
| FFmpeg | 9.0.2 from Homebrew, **GPL build** (`--enable-gpl`, libx264, libx265, libvpx, dav1d, VideoToolbox, AudioToolbox). Used for measuring only. Its H.264, HEVC and ProRes decoders and its VideoToolbox wrappers are the same code an LGPL build has; its `libx264` and `libx265` are not and are marked "GPL" below |
| Swift | 6.2.3; one throwaway tool `mb` using AVFoundation (`AVAssetReader`, `AVAssetWriter`) and another using `VTIsHardwareDecodeSupported` |
| Clips | Synthetic: `testsrc2` with temporal noise, 10 s, 30 fps (300 frames), BT.709 video range. H.264 1080p at 12 Mb/s and 4K at 45 Mb/s (libx264), HEVC 1080p at 10 Mb/s and 4K at 30 Mb/s (libx265, `hvc1`), HEVC Main10 1080p at 15 Mb/s, ProRes 422 (`prores_ks`, 10-bit 4:2:2, 196 MB), the 1080p H.264 file with a 90 degree display rotation in the container (`-display_rotation 90`, no re-encode), plus audio-only and odd-container files for section 3. Synthetic noise is a harsh, unrepresentative picture; real camera footage is a follow-up |
| Date of every web source | 2026-11-13 unless a date is given |

## Method

- **AVFoundation decode** (`mb decode`): `AVAssetReader` plus `AVAssetReaderTrackOutput`, output pixel format NV12 video range (`420v`; `x420` for 10-bit), `alwaysCopiesSampleData = false`, read all samples, no CPU copy. This is decode into GPU-capable pixel buffers, the path a preview sink uses. A second run copies both planes into a CPU buffer (`copy`) to put it next to FFmpeg's CPU output.
- **FFmpeg decode**: `ffmpeg -an -i clip -f null -`, software (default threads) and `-hwaccel videotoolbox`. The VideoToolbox run downloads every frame to system memory and converts it for the null sink, so it does **more** work than the AVFoundation no-copy run. Compare it with the "+ CPU copy" figures below.
- **Cost**: `/usr/bin/time -l` around each process: wall time, user plus system CPU, maximum resident set. CPU-seconds is user plus system. Process start is included for all tools.
- **Seek** (`mb seek`): for 40 random times, a new `AVAssetReader` with a 0.2 s time range, time until the first sample arrives (this includes decoding forward from the previous key frame). The FFmpeg figure is a process spawn plus `-ss` plus one frame, so it is not comparable and is only shown for orders of magnitude.
- **Encode**: the ProRes file as source (near-lossless reference), target 8 Mb/s at 1080p. AVFoundation: `AVAssetReader` to `AVAssetWriter` with H.264 or HEVC (VideoToolbox) and AAC 128 kb/s (AudioToolbox). FFmpeg: `h264_videotoolbox`, `hevc_videotoolbox`, and GPL `libx264 -preset fast`, `libx265 -preset fast`. Quality: ffmpeg `psnr` and `ssim` filters against the ProRes source, both converted to `yuv420p`.
- **Rust crates**: the spike crate runs `mp4`, `muxide` and Symphonia on the same files; crate facts come from `https://crates.io/api/v1/crates/<name>` (2026-11-13).

## 1. The seam

The seam lives in `crastu-media` (kind `engine`: pure Rust, compiles for iOS and Android). Backends are separate crates: `crastu-media-apple` (kind `os`, opts in to `unsafe` for the Objective-C and CoreVideo calls, as `crastu-os-perms` does), `crastu-media-ffmpeg` (kind `integration`), later `crastu-media-mf` (Media Foundation, Windows). A pure-Rust audio-only backend (Symphonia) is `crastu-media-symphonia`. Sketch, not final code:

```rust
/// Rational time. Presentation times are exact, never f64 seconds.
pub struct Time { pub num: i64, pub den: u32 }

pub trait Backend: Send + Sync {
    fn id(&self) -> BackendId;                   // "apple", "ffmpeg", "mf", "symphonia"
    fn capabilities(&self) -> &Capabilities;     // static, cheap, no file access
    /// Container and stream facts. Must say what it could not read (see `MediaInfo::gaps`).
    fn probe(&self, src: &Source) -> Result<MediaInfo, MediaError>;
    fn open_video(&self, src: &Source, o: &VideoOpen) -> Result<Box<dyn VideoDecoder>, MediaError>;
    fn open_audio(&self, src: &Source, o: &AudioOpen) -> Result<Box<dyn AudioDecoder>, MediaError>;
    fn open_encoder(&self, dst: &Target, spec: &EncodeSpec) -> Result<Box<dyn Encoder>, MediaError>;
}

pub trait VideoDecoder: Send {
    fn info(&self) -> &VideoStreamInfo;
    /// The frame showing at `t`: the frame with the greatest pts <= `t`.
    /// The result depends on (file, t) only, never on the order of earlier calls.
    /// t before the first pts: `Err(MediaError::BeforeStart { first })`.
    /// t at or after the end: the last frame.
    fn frame_at(&mut self, t: Time) -> Result<Frame, MediaError>;
    /// Optimisation hint only: the next frames are likely sequential from `t`.
    fn prefetch(&mut self, from: Time, count: u32) {}
}

pub struct Frame {
    pub pts: Time,
    pub duration: Option<Time>,
    pub size: (u32, u32),            // coded size, before rotation
    pub display: Orientation,        // rotation and mirror from the container, applied by the consumer
    pub colour: Colour,              // matrix, range, transfer, primaries, bit depth; always filled in
    pub buffer: FrameBuffer,
}
pub enum FrameBuffer { Platform(PlatformBuffer) /* CVPixelBuffer, D3D11 texture */, Cpu(CpuImage) }

pub trait AudioDecoder: Send {
    /// Interleaved f32 at the requested rate and channel count (resampled by the backend or the
    /// shared fallback in `crastu-media`), starting at `start`. Sample accurate and repeatable.
    fn read(&mut self, start: Time, out: &mut [f32]) -> Result<usize, MediaError>;
}

pub trait Encoder: Send {
    fn push_video(&mut self, frame: &Frame) -> Result<(), MediaError>;      // in pts order
    fn push_audio(&mut self, pcm: &AudioBuffer) -> Result<(), MediaError>;  // f32 interleaved
    fn finish(self: Box<Self>) -> Result<EncodeReport, MediaError>;         // sizes, durations, bitrate
}
```

Why "latest frame with pts <= t": a clip's cut points, caption timing and the preview must agree on one frame for one time, whichever backend decodes and whatever was asked before. Backends implement it as "seek to the key frame before `t`, decode forward, keep the last frame with pts <= `t`" with a sequential fast path; AVFoundation measured at 22 to 183 ms median per random access (section 2.3), so the preview keeps its own frame cache and asks the decoder for one frame per request, not per paint.

Capability and error model:

```rust
pub struct Capabilities {
    pub containers: Vec<ContainerSupport>,   // demux (and mux) per container
    pub video_decode: Vec<CodecSupport>,     // codec, profiles, bit depths, chroma, max size, Hw/Sw
    pub video_encode: Vec<CodecSupport>,
    pub audio_decode: Vec<AudioCodecSupport>,
    pub audio_encode: Vec<AudioCodecSupport>,
}
pub enum Support { Yes { hardware: bool }, No(Reason), DependsOnSystem }   // e.g. AV1 needs an M3 or later

pub enum MediaError {
    /// This backend cannot do it; the next backend may. Never means the file is bad.
    Unsupported { backend: BackendId, what: Unsupported },
    /// The file is damaged or cut off at `at`.
    Corrupt { at: Option<Time>, detail: String },
    BeforeStart { first: Time },
    Io(std::io::Error),
    Cancelled,
    /// Every backend that could try said `Unsupported`; lists them in the order tried.
    NoBackend { tried: Vec<(BackendId, Unsupported)> },
}
pub enum Unsupported {
    Container(String), VideoCodec { codec: String, profile: Option<String> },
    AudioCodec(String), PixelFormat(String), TenBitOrHdr, Drm, HardwareRequired, SizeLimit,
}
```

A `Registry` holds the backends in preference order per platform (macOS: apple, then ffmpeg; Windows: ffmpeg, then mf; Linux: ffmpeg). It opens each stream with the first backend whose `capabilities()` allow it, and on `MediaError::Unsupported` from `open_*` goes to the next; on `Corrupt` or `Io` it stops. Audio and video of one file may use different backends (an MKV with H.264 and AAC: FFmpeg for the container, either for the codecs). `probe` is allowed to return a partial `MediaInfo` with a `gaps: Vec<Unsupported>` list (what it could see but not decode), which is what the Media page and the import dialog show instead of a bare failure.

Findings that shape it (all from this spike):

- Apple's decoder does not rotate: the rotation is in `preferredTransform` (for the test file `rot=-90` for `-display_rotation 90`), so `display: Orientation` is part of `Frame`, and FFmpeg's auto-rotation must be turned off or reported, or the two backends give different frames. The `mp4` crate exposes the same matrix (`tkhd.matrix`: `a=0 b=-65536 c=65536 d=0`).
- Colour tags are needed on every frame (decision 27 and `docs/vendor-gpui.md`): `Colour` is mandatory.
- Containers have a start offset: `muxide` refuses audio before the first video pts and the test file's first video pts is 0.067 s (B-frame delay). The `Encoder` takes pts as given and does not guess offsets.
- A `Capabilities` check is cheap but only a hint (AV1 hardware needs the chip; `VTIsHardwareDecodeSupported` answers per machine): the authority is `open_*` returning `Unsupported`.

## 2. Measurements

### 2.1 What was measured, with which tool

Decode and encode through AVFoundation (`mb`, Swift, `AVAssetReader` and `AVAssetWriter`, which use VideoToolbox and AudioToolbox underneath: the decoding went to the hardware for H.264, HEVC and ProRes, see 2.7); FFmpeg 9.0.2 software; FFmpeg with `-hwaccel videotoolbox`; FFmpeg encoders; three Rust crates. Nothing here is estimated unless it says "estimate".

### 2.2 Decode, 10 s clips (300 frames), single run per cell

Wall is the process wall time; CPU is user plus system seconds; RSS is the maximum resident set. "AVF" is `mb decode` (no CPU readback), "FF sw" is FFmpeg software, "FF vt" is FFmpeg `-hwaccel videotoolbox` (with readback to system memory).

| Clip | AVF wall / CPU / RSS | FF sw wall / CPU / RSS | FF vt wall / CPU / RSS |
|---|---|---|---|
| H.264 1080p30 | 0.32 s / 0.08 s / 34 MB | 0.58 s / 2.04 s / 105 MB | 1.47 s / 0.37 s / 119 MB |
| HEVC 1080p30 | 0.21 s / 0.07 s / 31 MB | 0.45 s / 2.01 s / 99 MB | 1.07 s / 0.28 s / 111 MB |
| H.264 4K30 | 1.02 s / 0.12 s / 67 MB | 2.21 s / 7.52 s / 360 MB | 2.92 s / 0.59 s / 339 MB |
| HEVC 4K30 | 0.63 s / 0.10 s / 40 MB | 1.48 s / 7.81 s / 335 MB | 2.35 s / 0.51 s / 344 MB |
| HEVC Main10 1080p30 | 0.25 s / 0.06 s / 35 MB | 1.26 s / 5.05 s / 138 MB | 0.53 s / 0.18 s / 173 MB |
| ProRes 422 1080p30 | 0.59 s / 0.16 s / 130 MB | 2.03 s / 7.14 s / 128 MB | 0.86 s / 0.28 s / 201 MB |
| H.264 1080p30, rotated 90 degrees | 0.30 s / 0.12 s / 34 MB | 1.73 s / 3.65 s / 112 MB (FFmpeg rotates) | 1.08 s / 0.69 s / 128 MB |

Repeats show the noise: H.264 1080p AVFoundation ran 0.28 to 0.62 s over six runs (the one cold first run took 3.0 s: first use of the decoder after other work; not repeated); FFmpeg software 0.97 to 1.00 s; HEVC 4K AVFoundation 0.53 to 0.67 s, FFmpeg software 2.37 to 3.42 s, FFmpeg VideoToolbox 2.29 to 2.51 s. CPU-seconds moved by less than 15 percent. With a CPU copy of both NV12 planes added to the AVFoundation run (the like-for-like figure for FFmpeg VideoToolbox): H.264 1080p 0.29 to 0.32 s and 59 MB; H.264 4K 0.82 s, 0.25 CPU-seconds, 167 MB; HEVC 4K 0.57 s, 0.25 CPU-seconds, 143 MB.

Reading: all three tools decode these clips faster than real time by a wide margin (10 s of media in 0.2 to 3 s). The difference is CPU and memory: the AVFoundation path costs 0.06 to 0.16 CPU-seconds per 10 s of video, the software decoder 2 to 8. It matters for the auto-clipper, which decodes hours of video for scene cuts and face tracking: on this clip set software decode is about 25 to 85 times more CPU than AVFoundation.

### 2.3 Random access (the seam's `frame_at`)

| Clip | AVFoundation p50 / p95 / max (ms), 40 random times |
|---|---|
| H.264 1080p30 | 96 / 207 / 262 |
| HEVC 1080p30 | 33 / 116 / 129 |
| H.264 4K30 | 183 / 362 / 417 |
| HEVC 4K30 | 108 / 613 / 711 |
| ProRes 422 1080p30 | 23 / 45 / 218 |

The clips use libx264's default key frame interval (250 frames, over 8 s), so these are mostly "decode forward from the start" times; real phone files (a key frame every 1 to 2 s) will be shorter. FFmpeg process spawn plus `-ss` plus one frame: 249 ms median for H.264 1080p, 617 ms for HEVC 4K (not comparable, it includes starting the process). Takeaway: the preview needs a frame cache and a sequential fast path; a random `frame_at` per paint is too slow with either backend.

### 2.4 Encode, 1080p30 from the ProRes source, target 8 Mb/s

| Encoder | Wall | CPU (user+sys) | RSS | Video bitrate | PSNR (dB) | SSIM |
|---|---|---|---|---|---|---|
| AVFoundation H.264 (+ AAC), software ProRes read by `AVAssetReader` | 2.60 s | 0.44 s | 102 MB | 8.01 Mb/s | 40.0 | 0.957 |
| AVFoundation HEVC (+ AAC) | 2.25 s | 0.48 s | 96 MB | 7.89 Mb/s | 40.8 | 0.955 |
| FFmpeg `h264_videotoolbox` (ProRes decoded in software) | 1.87 s | 8.24 s | 207 MB | 8.08 Mb/s | 40.5 | 0.958 |
| FFmpeg `hevc_videotoolbox` (same) | 2.08 s | 8.20 s | 211 MB | 8.07 Mb/s | 41.3 | 0.956 |
| FFmpeg `h264_videotoolbox`, `-hwaccel videotoolbox` input | 1.99 s | 2.58 s | 250 MB | n/m | n/m | n/m |
| FFmpeg `hevc_videotoolbox`, `-hwaccel videotoolbox` input | 1.88 s | 2.35 s | 237 MB | n/m | n/m | n/m |
| FFmpeg `libx264 -preset fast` (GPL) | 7.45 s | 29.7 s | 613 MB | 8.08 Mb/s | 40.0 | 0.954 |
| FFmpeg `libx265 -preset fast` (GPL) | 11.56 s | 58.0 s | 552 MB | 8.05 Mb/s | 42.2 | 0.959 |

n/m: not measured. The AVFoundation rows decode ProRes with the hardware and also encode AAC (128 kb/s), so they do slightly more than the FFmpeg video-only rows and still use a fraction of the CPU. The hardware encoder runs at about 115 to 133 frames per second at 1080p with AVFoundation (160 for FFmpeg `h264_videotoolbox`), bounded by the encoder, not by the tool. 4K (source: the 4K H.264 clip, hardware decode in): AVFoundation HEVC 30 Mb/s 7.38 s (41 frames per second, 0.59 CPU-seconds, 86 MB), AVFoundation H.264 45 Mb/s 5.77 s (53 frames per second, 0.41 CPU-seconds, 88 MB), FFmpeg `hevc_videotoolbox` 7.35 s (1.23 CPU-seconds, 429 MB), FFmpeg `h264_videotoolbox` 6.78 s. 4K quality was not measured.

Quality at equal bitrate: VideoToolbox H.264 equals x264 `fast` on this synthetic source (40.0 to 40.5 dB against 40.0), VideoToolbox HEVC is 0.9 to 1.4 dB behind x265 `fast` (40.8 to 41.3 against 42.2). The source is noisy test pattern, so the gap is probably larger than on real footage; not checked on real footage. The encoder and its quality are the Mac's; they differ on every Apple chip generation (not measured here).

**AAC (AudioToolbox against FFmpeg's own encoder)**: 600 s of stereo pink noise at 128 kb/s: `aac_at` 4.27 s (24 MB), FFmpeg native `aac` 9.35 s (19 MB); output bitrates 128.0 and 128.3 kb/s. Both are far faster than real time (140 and 64 times), so audio is not a reason to choose a backend. Audio quality was not measured (the AAC patent licence is the real cost, section 5).

### 2.5 Pure-Rust MP4 demux and mux, on real files

Facts from crates.io on 2026-11-13: `muxide` 0.5.0, 2026-10-19, MIT OR Apache-2.0, 39,926 downloads, one maintainer; `mp4` 0.14.0, **2023-08-01**, MIT, 15.9 M downloads; `mp4-atom` 0.17.1, 2026-11-04, MIT OR Apache-2.0; `shiguredo_mp4` 2026.5.0, 2026-10-04, Apache-2.0; `re_mp4` 0.5.1, 2026-07-08, MIT; `mp4parse` 0.17.0, 2023-05-29, MPL-2.0; `minimp4` 0.1.2, 2024-06-19, MPL-2.0 (a binding of a C library). Only `mp4` and `muxide` were run.

| Trial | Result |
|---|---|
| `mp4` 0.14 demux, H.264 + AAC 1080p, 770 samples (16.0 MB) | header 0.65 ms, all samples read in 8.4 ms |
| `mp4` demux, H.264 4K (68.9 MB, 300 samples) | 22.5 ms |
| `mp4` demux, HEVC | samples read, but the track reports "unsupported sample entry box" (HEVC description is not parsed in 0.14: the decoder configuration must come from elsewhere) |
| `mp4` demux, ProRes `.mov`, Opus in MP4 | tracks listed as "unsupported media type"; samples still readable |
| `mp4` demux, rotated file | `tkhd` matrix readable (`a=0 b=-65536 c=65536 d=0`) |
| `mp4` demux, MKV and WebM | error "file contains a box with a larger size than it" (not ISO BMFF, as expected) |
| `muxide` 0.5, H.264 with B-frames, video only (from `mp4` samples, AVCC converted to Annex B, SPS and PPS before key frames, `write_video_with_dts`) | 300 frames written in 31.7 ms; `ffprobe` reports 300 frames, 10.000 s; `ffmpeg` decodes the whole file with **no error output** |
| `muxide`, video and AAC (ADTS built by hand) with B-frames, interleaved or video first | `ffprobe` shows both streams, but `ffmpeg` fails on the video: "Invalid NAL unit size ... missing picture in access unit" on hundreds of packets. **Not root-caused**: it may be our use or a `muxide` problem with B-frames plus audio |
| `muxide`, video and AAC, source without B-frames (`-bf 0`) | 150 video and 236 audio samples written in 9.5 ms; `ffmpeg` decodes with no error output |
| `muxide` rules met on the way | audio before the first video pts is rejected (the test file needed 4 audio frames dropped); DTS must strictly increase; input is Annex B for video and ADTS for AAC (the platform encoders and `mp4` give AVCC and raw AAC, so conversion is ours) |

Conclusion: a pure-Rust MP4 path is feasible for reading H.264 and AAC and is fast; the writer needs more proof (B-frames with audio) before it replaces `AVAssetWriter` or FFmpeg's muxer anywhere. On macOS it is not needed in v0.3. Not tried: fragmented MP4, HEVC through `muxide`, `mp4-atom`, `shiguredo_mp4`, `re_mp4`.

### 2.6 Symphonia 0.6.1 for audio

Symphonia 0.6.1 (MPL-2.0, released 2026-08-13; crates.io). Its own documentation (`lib.rs` of the crate, read from the downloaded source) lists the formats AIFF, CAF, ISO/MP4, MKV/WebM, Ogg and Wave, and the codecs AAC-LC, ADPCM, ALAC, FLAC, MP1, MP2, MP3, PCM and Vorbis. **No Opus, no AC-3, no AVI.** Run on the generated files (10 s each, decode all packets to f32):

| File | Result |
|---|---|
| AAC `.m4a`, MP4 with AAC, MKV with AAC | decoded, 7 to 12 ms |
| ALAC, FLAC, MP3, 24-bit WAV, ProRes `.mov` with PCM, MKV with FLAC | decoded, 3 to 107 ms (the ProRes file is 196 MB) |
| Opus in `.ogg`, in MP4, in WebM, in MKV | container read, **codec unsupported** |
| AC-3 raw | **no format reader** |
| MP3 in AVI | **no format reader** |

For the same audio files AVFoundation (`mb audio`, to 16 kHz mono float) decodes all of them, including Opus in Ogg and in MP4, FLAC and AC-3, in 4 to 22 ms (153 ms for the first, cold). Symphonia is therefore a good AAC, MP3, FLAC, WAV and Vorbis fallback and a useful MKV and WebM audio demuxer, but Opus (YouTube audio, Discord, many recorders) is its gap.

### 2.7 What the native path can and cannot decode on this Mac

Probed with `AVURLAsset` (`isReadable`, `isPlayable`, per track `isDecodable`) and then read with `AVAssetReader`; hardware support from `VTIsHardwareDecodeSupported`:

| Input | Result on macOS 26.5.1, Apple M2 |
|---|---|
| H.264, HEVC (8 and 10-bit), ProRes 422 in MP4 or MOV | readable, decodable; hardware decode reported for all three |
| H.264 4:4:4 and H.264 4:2:2 10-bit in MP4 and MOV | decoded (probably in software, since common hardware decoders skip those profiles; hardware use was not checked for these; 286 and 445 frames per second in the small 720p test) |
| MPEG-2 in MPEG-PS | decoded (software, 191 fps) |
| MPEG-4 Part 2 in AVI | readable, **not** decodable |
| VP9 in MP4 | readable, **not** decodable: `startReading` fails with "Cannot Decode", status `-12906`; `VTIsHardwareDecodeSupported(VP9)` is false |
| AV1 in MP4 | readable, **not** decodable on this M2, same error; `VTIsHardwareDecodeSupported(AV1)` is false |
| MKV (H.264 with AAC or FLAC), WebM (VP9 and Opus), MKV (AV1 and Opus) | **not readable** at all (`isReadable` false, no tracks) |
| AVI with H.264 and MP3 | **not readable** |
| Audio-only: AAC, ALAC, FLAC, MP3, Opus in Ogg, AC-3, PCM | readable and decodable |

## 3. What an FFmpeg-free path cannot read, and how likely a user hits it

Measured above (macOS 26.5.1 on an M2): MKV, WebM and AVI containers; VP9 and AV1 video; MPEG-4 Part 2 video; Opus in WebM and MKV through Symphonia (AVFoundation reads Opus in MP4 and Ogg). Not tested: FLV, WMV, MPEG-TS beyond MPEG-2, DNxHD, HEVC in MKV, AV1 on an M3 or later (this machine cannot), VP9 on Intel Macs.

How likely: **an estimate from reasoning, not a measurement.**

- Phone files (MOV or MP4 with H.264 or HEVC and AAC), Zoom, Meet and Teams exports (MP4, H.264, AAC): fine natively. Probably the majority of casual users.
- `yt-dlp` downloads: the `yt-dlp` README (`raw.githubusercontent.com/yt-dlp/yt-dlp/master/README.md`, 2026-11-13) documents `--merge-output-format` with `mkv`, `mp4`, `webm` and its own default format example "won't result in an mkv", i.e. merging best video and best audio can end in MKV. Which codecs the default picks (VP9 or AV1 with Opus is common for 1080p and above on YouTube) was not verified here. For an auto-clipper whose first audience is people cutting clips from long online videos, this is a likely input.
- OBS recordings: OBS offers MKV, MP4 and "Hybrid MP4" (`en-US.ini` in the OBS repository, 2026-11-13). The default format and codec were not verified here.
- AV1 on older Macs: this M2 cannot decode AV1 (measured). Apple's AV1 hardware decoding is on newer chips (the chip list was not verified from a primary source in this spike; the Apple newsroom page for M3 did not load); the Macs of the target users are a mix.

So a meaningful share of an auto-clipper's inputs, plausibly one in five or more, would fail on a native-only path (an unverified guess: no usage data exists). The native path remains the right default for speed, but refusing the rest in v0.3 is a product decision for the owner; this spike recommends against it.

Failure behaviour for the fallback: when FFmpeg is the only backend that can open a file, the user gets the same flow, slower and hotter; the Media page marks files by backend ("decoded in software").

## 4. Windows (P5)

Sources: Microsoft Learn, "Supported Media Formats in Media Foundation" (page last updated 2021-03-03, read 2026-11-13) and the codec pages (H.265 decoder and encoder, H.264 encoder), no measurements.

- Containers listed: 3GP, ASF, ADTS, AVI, MP3, MPEG-4 (`.m4a`, `.m4v`, `.mov`, `.mp4`) and others; **MKV and WebM are not listed.**
- Video listed: H.264 decoder and encoder (Windows 7, High profile encode needs Windows 8), MPEG-4 Part 2, MJPEG, WMV, DV. HEVC has its own decoder (minimum Windows 10, desktop apps only; the page names `hevcdecoder.dll` and `hevcdecoder_store.dll`, which suggests a Store extension, not verified) and an encoder (Main profile only). I found no Microsoft page for VP9 or AV1 decoders (the two URLs tried returned 404), so VP9 and AV1 support was not verified here.
- Audio listed: AAC decoder and encoder (Windows 7), MP3, WMA; no Opus, FLAC or AC-3 listed.
- Hardware encoders (NVENC, Quick Sync, AMF) reach Media Foundation as vendor MFTs and FFmpeg wraps them too (`h264_mf`, `h264_nvenc`); not verified against a primary source here.

Assessment: Media Foundation is the Windows equivalent of AVFoundation for the common H.264, HEVC and AAC case, but it lacks MKV, WebM and the open codecs, so the same gap as on macOS exists, and the HEVC question (inbox or Store extension) is open. **FFmpeg stays the default on Windows**, with Media Foundation as an optional hardware path behind the same seam (probably for HEVC and H.264 hardware encode). The preview on Windows decodes to `img()` (gpui-video spike). To measure on a Windows machine: decode and encode parity, the HEVC extension requirement and VP9 and AV1 availability on a clean install.

## 5. Licence and patent table per backend

Rule: every cell has its source and date; where sources disagree, the row says so. This is research, not legal advice.

| Backend or part | What we link or ship | Licence of that code (source) | Codec patents that apply (source) |
|---|---|---|---|
| Apple native (AVFoundation, VideoToolbox, AudioToolbox) | Nothing: system frameworks | Apple's macOS licence agreement | macOS Tahoe licence (`apple.com/legal/sla/docs/macOSTahoe.pdf`, 2026-11-13, text of the first 80 pages searched): H.264/AVC "licensed herein only for the personal and non-commercial use of a consumer", commercial use "requires additional licensing" (MPEG LA, now Via LA). Also an MPEG-4 Systems notice about encoding for title-by-title paid distribution, and an MPEG-4 Visual notice limited to personal and non-commercial use of a consumer (relevant to AVI decode). No mention of AAC, HEVC, ProRes, VP9 or AV1 in the pages searched. Whether an app that calls the frameworks needs its own licence is **not answered by these texts**: needs counsel |
| FFmpeg LGPL shared build (`crastu-media-ffmpeg`) | libavcodec, libavformat, libswresample, libswscale, plus optionally dav1d, libopus and the VideoToolbox and AudioToolbox wrappers | LGPL-2.1-or-later for the code built without `--enable-gpl` or `--enable-nonfree`; some files MIT or BSD style (FFmpeg `LICENSE.md`, `raw.githubusercontent.com/FFmpeg/FFmpeg/master/LICENSE.md`, 2026-11-13). LGPL checklist (`ffmpeg.org/legal.html`, 2026-11-13): build without those flags, link dynamically, ship the source, licence and build flags, do not rename the libraries to obscure them | `ffmpeg.org/legal.html` (2026-11-13), patent mini-FAQ: "We do not know, we are not lawyers"; says use in a commercial product "might" be a problem. Its software decoders are a "decoder" in the pool terms: H.264 Via LA AVC page (`via-la.com/licensing-programs/avc-h-264`, 2026-11-13): products sold to end users, first 100,000 units a year $0.00, 100,001 to 5,000,000 $0.20 each, above $0.10 each, enterprise cap $9.75M a year from 2017 (our reading, estimate: applies to an app that includes an AVC decoder or encoder) |
| FFmpeg GPL (`libx264`, `libx265`) | Never shipped (no GPL in process) | x264 and x265 are GPL (named as GPL libraries in `ffmpeg.org/legal.html`, 2026-11-13: "Make sure your program is not using any GPL libraries (notably libx264)") | As H.264 and HEVC above |
| openh264 built from source | Cisco's encoder and decoder | BSD-2-Clause (Cisco `LICENSE`, "Copyright (c) 2013, Cisco Systems", `raw.githubusercontent.com/cisco/openh264/master/LICENSE`, 2026-11-13; crate `openh264` 0.9.10 BSD-2-Clause, crates.io 2026-11-13) | `openh264.org/faq.html` (2026-11-13): a team using the source "is responsible for paying all applicable license fees"; Cisco covers only its own binary module, which must be downloaded at install time, "Cisco is providing no such guarantee" for other holders. So: **not covered when built from source**, as the issue says |
| openh264 Cisco binary module | The binary Cisco distributes, downloaded at install | Cisco's binary terms (not read in full here) | `openh264.org` (2026-11-13): Cisco does not pass its MPEG LA costs on for that module. Needs a download at runtime, which AGENTS.md allows only for models, updates and user-added providers: would need a new rule |
| dav1d (AV1 decode, via FFmpeg or alone) | libdav1d | BSD-2-Clause, "Copyright 2018-2025, VideoLAN and dav1d authors" (`raw.githubusercontent.com/videolan/dav1d/master/COPYING`, 2026-11-13) | AV1: the AOMedia patent licence (`aomedia.org/license/patent-license`, 2026-11-13) grants a "no-charge, royalty-free, irrevocable (except as expressly stated)" licence to the Necessary Claims of the members, with conditions. **Disagreement**: Sisvel lists an AV1 licensing programme on its site (`sisvel.com`, 2026-11-13: "Video coding platform: AV1"), so royalty-free is the AOMedia position, not a statement about every patent holder |
| libvpx (VP9) | libvpx | BSD-3-Clause style, "Copyright (c) 2010, The WebM Project authors" (`raw.githubusercontent.com/webmproject/libvpx/main/LICENSE`, 2026-11-13) | WebM Project IP grant: "perpetual, worldwide, non-exclusive, no-charge, royalty-free, irrevocable (except as stated)" (`webmproject.org/license/additional`, 2026-11-13). **Disagreement**: Sisvel lists a VP9 programme (same Sisvel page) |
| libopus / Opus | libopus | BSD-3-Clause style (Xiph, Skype, others; `raw.githubusercontent.com/xiph/opus/main/COPYING`, 2026-11-13) | `opus-codec.org/license` (2026-11-13): "royalty-free licenses for the required patents ... automatically granted to everyone", with a defensive clause |
| Symphonia 0.6.1 (audio) | Pure Rust AAC-LC, ALAC, FLAC, MP3, PCM, Vorbis, ADPCM decoders and MKV, Ogg, MP4, WAV, CAF, AIFF readers | MPL-2.0 (crates.io, 2026-11-13). File-level copyleft: changes to its files are published, linking is fine | AAC: Via LA AAC programme (`via-la.com/licensing-programs/aac`, 2026-11-13): a licence "is needed by manufacturers or developers of end-user encoder and/or decoder products"; first 500,000 units a year $0.98 each, initial fee $15,000 (from the same page). MP3 patents: not researched here |
| AAC through AudioToolbox or FFmpeg's `aac` | Platform or LGPL code | As above | Same Via LA AAC programme; whether using the Apple encoder moves the obligation is not answered by Apple's licence text (no AAC mention in the pages searched) |
| HEVC (any backend) | Platform, FFmpeg's HEVC decoder, or none | | `accessadvance.com/licensing-programs/hevc-advance` (2026-11-13): HEVC Advance pool, fair, reasonable and non-discriminatory terms, rates not on that page. `via-la.com/licensing-programs/hevc-vvc` (2026-11-13): "as of December 15, 2025, Access Advance has acquired this HEVC/VVC program". Other holders outside the pools were not researched |
| `muxide` 0.5.0, `mp4` 0.14.0 (containers) | Pure Rust MP4 | MIT OR Apache-2.0; MIT (crates.io, 2026-11-13) | A container carries no codec; the MPEG-4 Systems notice in Apple's licence (above) concerns encoding for paid title-by-title distribution only. Not researched further |
| Media Foundation (Windows) | System components | Microsoft's Windows terms (not read) | Not researched; the same pools apply to H.264, HEVC and AAC |

## 6. Proposed decision

Row for `docs/DECISIONS.md` (number 37; 36 went to the ONNX session factory):

> macOS and the media seam (spike #30): `crastu-media` defines a `Backend` trait (probe; a video decoder whose `frame_at(t)` returns the frame with the greatest pts <= t, independent of call order; an audio decoder; an encoder taking frames and f32 PCM; capabilities and an `Unsupported` error that lets the registry try the next backend). On macOS the default backend is native (AVFoundation, VideoToolbox, AudioToolbox, in an `os` crate that opts in to `unsafe`); FFmpeg (LGPL shared, pinned by sha256, as ARCHITECTURE.md section 9 says) **ships in v0.3 as the fallback** for what the native path cannot read (MKV, WebM, AVI, VP9, AV1 on this Mac, and Opus (no pure-Rust codec yet)) and for platforms without a native backend. Windows keeps FFmpeg as the default with Media Foundation as an optional hardware path; Symphonia is the audio-only fallback. No pure-Rust MP4 muxer ships until it is shown with B-frames plus audio. FFmpeg's removal on macOS is a later decision, reconsidered with real usage data. Open before release: counsel reads Apple's AVC clause, the Via LA AVC and AAC terms and the HEVC pools for an app that exports H.264 and AAC. Evidence: `docs/spikes/media-backends.md`.

## Follow-ups

1. Legal: the three licence questions in section 5 (Apple's AVC clause, Via LA AVC and AAC, HEVC pools) need an answer before an exporting release; this spike only collected the texts.
2. `muxide` plus audio with B-frames: find out whether the fault is ours (file an issue upstream with the repro) before depending on it; try `mp4-atom` and `shiguredo_mp4` too.
3. Repeat with real camera footage (iPhone HEVC with rotation and HDR, a screen recording, an OBS MKV) and on an M3 or later (AV1 hardware).
4. Measure an actual LGPL FFmpeg build for macOS: size of the dylibs (the Homebrew build is 40 MB of libraries, but it carries GPL and many codecs), startup cost, and a decoders-only configuration. Check where the macOS builds come from (BtbN builds are Windows and Linux only in OpenCut's setup).
5. Windows: the measurements and facts listed in section 4.
6. 4K encode quality and the HDR and 10-bit export path through VideoToolbox.
7. Captions are burned in with libass in the auto-clipper plan: without FFmpeg's filters the compositing path (own renderer to a frame, then the encoder) must exist for the native backend; check it before P3.

## Reproduce

```sh
# clips (synthetic), 10 s each
ffmpeg -f lavfi -i "testsrc2=size=1920x1080:rate=30,noise=alls=8:allf=t" -t 10 \
  -c:v libx264 -preset fast -b:v 12M -pix_fmt yuv420p h264_1080p30.mp4
ffmpeg -f lavfi -i "testsrc2=size=1920x1080:rate=30,noise=alls=8:allf=t" -t 10 \
  -c:v prores_ks -profile:v 2 -pix_fmt yuv422p10le prores422_1080p30.mov
ffmpeg -display_rotation 90 -i h264_1080p30.mp4 -c copy portrait_rot90_h264.mp4
# cost of a process
/usr/bin/time -l ffmpeg -an -i h264_1080p30.mp4 -f null -
/usr/bin/time -l ffmpeg -an -hwaccel videotoolbox -i h264_1080p30.mp4 -f null -
# quality
ffmpeg -i out.mp4 -i prores422_1080p30.mov -lavfi \
  "[0:v]format=yuv420p[a];[1:v]format=yuv420p[b];[a]split[a1][a2];[b]split[b1][b2];[a1][b1]psnr;[a2][b2]ssim" -f null -
# pure-Rust trials
cd spikes/media-backends && cargo build --release
./target/release/spike-media-backends demux h264_1080p30.mp4
./target/release/spike-media-backends remux h264_1080p30.mp4 out.mp4
./target/release/spike-media-backends audio flac.flac opus.ogg
```

The AVFoundation tool was about 200 lines of Swift (`AVAssetReader` with `kCVPixelBufferPixelFormatTypeKey`, `AVAssetWriter` with `AVVideoCodecKey` and an `AVFormatIDKey` of AAC) and is not committed; the numbers above are the evidence.
