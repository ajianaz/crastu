//! Spike #3: show decoded video frames in GPUI on macOS.
//!
//! Usage: `spike-gpui-video <img|surface-copy|surface-vt> <video file> [seconds]`
//!
//! Paths compared:
//! - `img`: software decode, swscale to BGRA, `img(ImageSource::Render)`. Works on every
//!   GPUI platform; each frame is uploaded to the sprite atlas.
//! - `surface-copy`: software decode, swscale to NV12 full range, copy into a pooled
//!   IOSurface-backed `CVPixelBuffer`, `surface()`. macOS only.
//! - `surface-vt`: VideoToolbox hardware decode straight into a `CVPixelBuffer`, `surface()`.
//!   Zero copy. macOS only.
//!
//! The decoder thread paces frames at the stream frame rate and loops the file. The UI
//! shows the newest frame; older queued frames are counted as skipped. After a 2 s warm-up
//! the program measures for `seconds` (default 20) and prints one JSON line of results.

use std::{
    env,
    path::PathBuf,
    ptr::{self, NonNull},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow, bail};
use ffmpeg::{
    format::Pixel,
    software::scaling::{self, Flags},
    sys,
};
use ffmpeg_next as ffmpeg;
use futures::{StreamExt, channel::mpsc};
use gpui::{
    App, Bounds, Context, ImageSource, IntoElement, ObjectFit, ParentElement, Render, RenderImage,
    Styled, StyledImage, Window, WindowBounds, WindowKind, WindowOptions, div, img, prelude::*, px,
    rgb, size, surface,
};
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferGetBaseAddressOfPlane, CVPixelBufferGetBytesPerRowOfPlane,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferPool,
    CVPixelBufferUnlockBaseAddress, kCVPixelBufferHeightKey, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelBufferWidthKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVReturnSuccess,
};

const WARMUP: Duration = Duration::from_secs(2);

static SENT: AtomicU64 = AtomicU64::new(0);
static RECEIVED: AtomicU64 = AtomicU64::new(0);
static RENDERED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Img,
    SurfaceCopy,
    SurfaceVt,
}

impl Mode {
    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "img" => Self::Img,
            "surface-copy" => Self::SurfaceCopy,
            "surface-vt" => Self::SurfaceVt,
            other => bail!("unknown mode {other}"),
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Img => "img",
            Self::SurfaceCopy => "surface-copy",
            Self::SurfaceVt => "surface-vt",
        }
    }
}

/// A CoreVideo buffer that may cross threads. CVPixelBuffers are reference counted and
/// immutable once handed over, so moving the retained handle between threads is fine.
struct SendBuffer(CFRetained<CVPixelBuffer>);
// SAFETY: CVPixelBuffer is a thread-safe CF type; the decoder no longer writes to it.
unsafe impl Send for SendBuffer {}

enum Picture {
    Image(Arc<RenderImage>),
    Surface(CFRetained<CVPixelBuffer>),
}

struct DecodedFrame {
    picture: PictureMsg,
    decode: Duration,
    convert: Duration,
}

enum PictureMsg {
    Image(Arc<RenderImage>),
    Surface(SendBuffer),
}

#[derive(Default)]
struct Samples(Vec<f64>);

impl Samples {
    fn push(&mut self, d: Duration) {
        self.0.push(d.as_secs_f64() * 1000.0);
    }

    fn summary(&self) -> String {
        if self.0.is_empty() {
            return "null".into();
        }
        let mut v = self.0.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let pct = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
        format!(
            "{{\"mean\":{mean:.2},\"p50\":{:.2},\"p95\":{:.2},\"p99\":{:.2},\"max\":{:.2}}}",
            pct(0.5),
            pct(0.95),
            pct(0.99),
            v[v.len() - 1]
        )
    }
}

#[derive(Default)]
struct Stats {
    shown: u64,
    skipped: u64,
    present_interval: Samples,
    decode: Samples,
    convert: Samples,
    last_shown: Option<Instant>,
}

struct Player {
    mode: Mode,
    current: Option<Picture>,
    pending: Option<DecodedFrame>,
    stats: Stats,
    measuring: bool,
    pixel_format: Option<u32>,
}

impl Render for Player {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(frame) = self.pending.take() {
            RENDERED.fetch_add(1, Ordering::Relaxed);
            let next = match frame.picture {
                PictureMsg::Image(image) => Picture::Image(image),
                PictureMsg::Surface(SendBuffer(buffer)) => {
                    self.pixel_format = Some(CVPixelBufferGetPixelFormatType(&buffer));
                    Picture::Surface(buffer)
                }
            };
            // Free the previous frame's atlas texture; otherwise every frame stays resident.
            if let Some(Picture::Image(previous)) = self.current.replace(next) {
                let _ = window.drop_image(previous);
            }
            let now = Instant::now();
            if self.measuring {
                self.stats.shown += 1;
                self.stats.decode.push(frame.decode);
                self.stats.convert.push(frame.convert);
                if let Some(last) = self.stats.last_shown {
                    self.stats.present_interval.push(now - last);
                }
            }
            self.stats.last_shown = Some(now);
        }

        let video = match &self.current {
            Some(Picture::Image(image)) => img(ImageSource::Render(image.clone()))
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            Some(Picture::Surface(buffer)) => surface(buffer.clone())
                .size_full()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div().into_any_element(),
        };

        div()
            .size_full()
            .relative()
            .bg(rgb(0x000000))
            .child(video)
            .child(
                div()
                    .absolute()
                    .top_2()
                    .left_2()
                    .px_2()
                    .bg(rgb(0x1b1d22))
                    .text_color(rgb(0xe6e6e6))
                    .text_sm()
                    .child(format!(
                        "{}  shown {}  skipped {}",
                        self.mode.name(),
                        self.stats.shown,
                        self.stats.skipped
                    )),
            )
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        bail!("usage: spike-gpui-video <img|surface-copy|surface-vt> <file> [seconds]");
    }
    let mode = Mode::parse(&args[1])?;
    let path = PathBuf::from(&args[2]);
    let seconds: u64 = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(20);
    ffmpeg::init()?;

    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.0), px(720.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    // Keep the window above the terminal: GPUI stops drawing occluded windows.
                    kind: if env::var_os("SPIKE_NORMAL_WINDOW").is_some() {
                        WindowKind::Normal
                    } else {
                        WindowKind::PopUp
                    },
                    ..Default::default()
                },
                |_, cx| {
                    cx.new(|_| Player {
                        mode,
                        current: None,
                        pending: None,
                        stats: Stats::default(),
                        measuring: false,
                        pixel_format: None,
                    })
                },
            )
            .expect("open window");
        cx.activate(true);
        let player = window.update(cx, |_, _, cx| cx.entity()).expect("player");

        let (tx, mut rx) = mpsc::unbounded::<DecodedFrame>();
        let stop = Arc::new(AtomicBool::new(false));
        let decoder_stop = stop.clone();
        let decoder = thread::spawn(move || decode_loop(&path, mode, tx, decoder_stop));

        // Deliver frames to the view; keep only the newest one per wake-up.
        let weak = player.downgrade();
        cx.spawn(async move |cx| {
            while let Some(mut frame) = rx.next().await {
                let mut skipped = 0;
                while let Ok(newer) = rx.try_recv() {
                    frame = newer;
                    skipped += 1;
                }
                RECEIVED.fetch_add(1 + skipped, Ordering::Relaxed);
                let alive = weak.update(cx, |player, cx| {
                    if player.pending.is_some() {
                        // The previous frame never reached render().
                        skipped += 1;
                    }
                    if player.measuring {
                        player.stats.skipped += skipped;
                    }
                    player.pending = Some(frame);
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        // Warm up, measure, report, quit.
        let weak = player.downgrade();
        cx.spawn(async move |cx| {
            cx.background_executor().timer(WARMUP).await;
            let _ = weak.update(cx, |player, _| {
                player.stats = Stats::default();
                player.measuring = true;
            });
            let cpu_start = cpu_time();
            let wall_start = Instant::now();
            cx.background_executor()
                .timer(Duration::from_secs(seconds))
                .await;
            let cpu = cpu_time() - cpu_start;
            let wall = wall_start.elapsed();
            stop.store(true, Ordering::Relaxed);
            let _ = weak.update(cx, |player, _| {
                let s = &player.stats;
                println!(
                    "{{\"mode\":\"{}\",\"seconds\":{:.1},\"fps_shown\":{:.2},\"shown\":{},\"skipped\":{},\
                     \"cpu_percent_of_one_core\":{:.1},\"peak_rss_mb\":{:.1},\"pixel_format\":\"{}\",\
                     \"present_interval_ms\":{},\"decode_ms\":{},\"convert_ms\":{}}}",
                    player.mode.name(),
                    wall.as_secs_f64(),
                    s.shown as f64 / wall.as_secs_f64(),
                    s.shown,
                    s.skipped,
                    cpu.as_secs_f64() / wall.as_secs_f64() * 100.0,
                    peak_rss_mb(),
                    player.pixel_format.map(fourcc).unwrap_or_else(|| "-".into()),
                    s.present_interval.summary(),
                    s.decode.summary(),
                    s.convert.summary(),
                );
            });
            cx.update(|cx| cx.quit());
        })
        .detach();

        if env::var_os("SPIKE_DEBUG").is_some() {
            thread::spawn(|| {
                loop {
                    thread::sleep(Duration::from_secs(1));
                    eprintln!(
                        "sent {} received {} rendered {}",
                        SENT.load(Ordering::Relaxed),
                        RECEIVED.load(Ordering::Relaxed),
                        RENDERED.load(Ordering::Relaxed)
                    );
                }
            });
        }

        thread::spawn(move || {
            if let Ok(Err(error)) = decoder.join() {
                eprintln!("decoder failed: {error:#}");
                std::process::exit(1);
            }
        });
    });
    Ok(())
}

fn fourcc(code: u32) -> String {
    code.to_be_bytes().iter().map(|b| *b as char).collect()
}

fn cpu_time() -> Duration {
    // SAFETY: getrusage writes into the zeroed struct we pass.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
    tv(usage.ru_utime) + tv(usage.ru_stime)
}

fn peak_rss_mb() -> f64 {
    // SAFETY: as above. On macOS ru_maxrss is in bytes.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    usage.ru_maxrss as f64 / (1024.0 * 1024.0)
}

// ---------------------------------------------------------------------------------------
// Decoding

fn decode_loop(
    path: &PathBuf,
    mode: Mode,
    tx: mpsc::UnboundedSender<DecodedFrame>,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let mut input = ffmpeg::format::input(path).context("open input")?;
    let stream = input
        .streams()
        .best(ffmpeg::media::Type::Video)
        .ok_or_else(|| anyhow!("no video stream"))?;
    let stream_index = stream.index();
    let rate = stream.avg_frame_rate();
    let fps = if rate.denominator() != 0 {
        f64::from(rate.numerator()) / f64::from(rate.denominator())
    } else {
        30.0
    };
    let mut context = ffmpeg::codec::context::Context::from_parameters(stream.parameters())?;
    if mode == Mode::SurfaceVt {
        enable_videotoolbox(&mut context)?;
    } else {
        // Frame threads, count chosen by FFmpeg from the core count. decode_ms then measures
        // latency through the thread pipeline; CPU use is the cost figure.
        context.set_threading(ffmpeg::codec::threading::Config {
            kind: ffmpeg::codec::threading::Type::Frame,
            count: 0,
        });
    }
    let mut decoder = context.decoder().video()?;
    let (width, height) = (decoder.width(), decoder.height());
    let source = Source {
        format: decoder.format(),
        full_range: decoder.color_range() == ffmpeg::color::Range::JPEG,
    };

    let mut converter = Converter::new(mode, source, width, height)?;
    let frame_period = Duration::from_secs_f64(1.0 / fps);
    let start = Instant::now();
    let mut index: u32 = 0;
    let mut frame = ffmpeg::frame::Video::empty();

    'outer: while !stop.load(Ordering::Relaxed) {
        for (packet_stream, packet) in input.packets() {
            if stop.load(Ordering::Relaxed) {
                break 'outer;
            }
            if packet_stream.index() != stream_index {
                continue;
            }
            let decode_start = Instant::now();
            decoder.send_packet(&packet)?;
            while decoder.receive_frame(&mut frame).is_ok() {
                let decode = decode_start.elapsed();
                let convert_start = Instant::now();
                let picture = converter.convert(&frame)?;
                let convert = convert_start.elapsed();

                // Pace to the stream frame rate.
                let due = start + frame_period * index;
                index += 1;
                if let Some(wait) = due.checked_duration_since(Instant::now()) {
                    thread::sleep(wait);
                }
                SENT.fetch_add(1, Ordering::Relaxed);
                if tx
                    .unbounded_send(DecodedFrame {
                        picture,
                        decode,
                        convert,
                    })
                    .is_err()
                {
                    break 'outer;
                }
            }
        }
        // Loop the file.
        input.seek(0, ..)?;
        decoder.flush();
    }
    Ok(())
}

/// Ask FFmpeg for VideoToolbox hardware decoding. Output frames carry a CVPixelBuffer in
/// `data[3]`. GPUI's `surface()` only accepts NV12 full range (`420f`), while H.264 is usually
/// video range, so `get_format` also marks the stream as full range: FFmpeg then asks
/// VideoToolbox for `420f` buffers and VideoToolbox expands the range during decode.
fn enable_videotoolbox(context: &mut ffmpeg::codec::context::Context) -> Result<()> {
    unsafe extern "C" fn get_format(
        ctx: *mut sys::AVCodecContext,
        formats: *const sys::AVPixelFormat,
    ) -> sys::AVPixelFormat {
        // SAFETY: FFmpeg passes a list terminated by AV_PIX_FMT_NONE and a live context.
        unsafe {
            let mut p = formats;
            while *p != sys::AVPixelFormat::AV_PIX_FMT_NONE {
                if *p == sys::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX {
                    if env::var_os("SPIKE_KEEP_RANGE").is_none() {
                        (*ctx).color_range = sys::AVColorRange::AVCOL_RANGE_JPEG;
                    }
                    return *p;
                }
                p = p.add(1);
            }
            sys::AVPixelFormat::AV_PIX_FMT_NONE
        }
    }

    // SAFETY: the context is not opened yet; we install a device reference and a callback.
    unsafe {
        let mut device: *mut sys::AVBufferRef = ptr::null_mut();
        let err = sys::av_hwdevice_ctx_create(
            &mut device,
            sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            ptr::null(),
            ptr::null_mut(),
            0,
        );
        if err < 0 {
            bail!("av_hwdevice_ctx_create failed: {err}");
        }
        let ctx = context.as_mut_ptr();
        (*ctx).hw_device_ctx = device;
        (*ctx).get_format = Some(get_format);
    }
    Ok(())
}

enum Converter {
    Bgra {
        scaler: scaling::Context,
        out: ffmpeg::frame::Video,
    },
    Nv12 {
        scaler: scaling::Context,
        out: ffmpeg::frame::Video,
        pool: CFRetained<CVPixelBufferPool>,
    },
    Passthrough,
}

/// The decoder's output format, so the scaler reads frames as they are (8 or 10 bit,
/// 4:2:0, 4:2:2 or 4:4:4, video or full range) instead of assuming YUV420P.
#[derive(Clone, Copy)]
struct Source {
    format: Pixel,
    full_range: bool,
}

impl Source {
    fn is_full_range(self) -> bool {
        self.full_range
            || matches!(
                self.format,
                Pixel::YUVJ420P | Pixel::YUVJ422P | Pixel::YUVJ444P | Pixel::YUVJ440P
            )
    }
}

impl Converter {
    fn new(mode: Mode, source: Source, width: u32, height: u32) -> Result<Self> {
        Ok(match mode {
            Mode::Img => Converter::Bgra {
                scaler: scaler(source, width, height, Pixel::BGRA)?,
                out: ffmpeg::frame::Video::empty(),
            },
            Mode::SurfaceCopy => Converter::Nv12 {
                scaler: scaler(source, width, height, Pixel::NV12)?,
                out: ffmpeg::frame::Video::empty(),
                pool: nv12_pool(width, height)?,
            },
            Mode::SurfaceVt => Converter::Passthrough,
        })
    }

    fn convert(&mut self, frame: &ffmpeg::frame::Video) -> Result<PictureMsg> {
        match self {
            Converter::Bgra { scaler, out } => {
                scaler.run(frame, out)?;
                let (w, h) = (out.width() as usize, out.height() as usize);
                let stride = out.stride(0);
                let data = out.data(0);
                let mut bytes = Vec::with_capacity(w * h * 4);
                for row in 0..h {
                    bytes.extend_from_slice(&data[row * stride..row * stride + w * 4]);
                }
                // RenderImage stores BGRA bytes in an RgbaImage container.
                let buffer = image::RgbaImage::from_raw(w as u32, h as u32, bytes)
                    .ok_or_else(|| anyhow!("frame size"))?;
                let image = RenderImage::new(smallvec::smallvec![image::Frame::new(buffer)]);
                Ok(PictureMsg::Image(Arc::new(image)))
            }
            Converter::Nv12 { scaler, out, pool } => {
                scaler.run(frame, out)?;
                let buffer = pooled_buffer(pool)?;
                copy_nv12(out, &buffer)?;
                Ok(PictureMsg::Surface(SendBuffer(buffer)))
            }
            Converter::Passthrough => {
                // SAFETY: VideoToolbox frames carry a CVPixelBufferRef in data[3]; retaining it
                // keeps the buffer alive after FFmpeg reuses the AVFrame.
                let buffer = unsafe {
                    let raw = (*frame.as_ptr()).data[3] as *mut CVPixelBuffer;
                    let raw = NonNull::new(raw).ok_or_else(|| {
                        anyhow!(
                            "frame is not a VideoToolbox frame (format {:?})",
                            frame.format()
                        )
                    })?;
                    CFRetained::retain(raw)
                };
                Ok(PictureMsg::Surface(SendBuffer(buffer)))
            }
        }
    }
}

/// A software scaler from the decoder's format to `target`, with BT.709 coefficients for HD
/// input. NV12 output is full range to match GPUI's `surface()` shader.
fn scaler(source: Source, width: u32, height: u32, target: Pixel) -> Result<scaling::Context> {
    let mut scaler = scaling::Context::get(
        source.format,
        width,
        height,
        target,
        width,
        height,
        Flags::BILINEAR,
    )?;
    // SAFETY: the scaler is live; the tables are static FFmpeg data.
    unsafe {
        let table = sys::sws_getCoefficients(sys::SWS_CS_ITU709);
        let dst_full = i32::from(target == Pixel::NV12 || target == Pixel::BGRA);
        sys::sws_setColorspaceDetails(
            scaler.as_mut_ptr(),
            table,
            i32::from(source.is_full_range()),
            table,
            dst_full,
            0,
            1 << 16,
            1 << 16,
        );
    }
    Ok(scaler)
}

fn nv12_pool(width: u32, height: u32) -> Result<CFRetained<CVPixelBufferPool>> {
    let format = CFNumber::new_i32(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange as i32);
    let w = CFNumber::new_i32(width as i32);
    let h = CFNumber::new_i32(height as i32);
    let io_surface: CFRetained<CFDictionary<CFString, CFType>> =
        CFDictionary::from_slices(&[], &[]);
    // SAFETY: the keys are CoreVideo constants.
    let keys: [&CFString; 4] = unsafe {
        [
            kCVPixelBufferPixelFormatTypeKey,
            kCVPixelBufferWidthKey,
            kCVPixelBufferHeightKey,
            kCVPixelBufferIOSurfacePropertiesKey,
        ]
    };
    let values: [&CFType; 4] = [&format, &w, &h, &io_surface];
    let attributes = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);
    let mut pool: *mut CVPixelBufferPool = ptr::null_mut();
    // SAFETY: the attributes dictionary is valid and the out pointer is writable.
    let status = unsafe {
        CVPixelBufferPool::create(
            None,
            None,
            Some(attributes.as_opaque()),
            NonNull::from(&mut pool),
        )
    };
    if status != kCVReturnSuccess {
        bail!("CVPixelBufferPoolCreate: {status}");
    }
    let pool = NonNull::new(pool).ok_or_else(|| anyhow!("null pool"))?;
    // SAFETY: Create returns a +1 reference.
    Ok(unsafe { CFRetained::from_raw(pool) })
}

fn pooled_buffer(pool: &CVPixelBufferPool) -> Result<CFRetained<CVPixelBuffer>> {
    let mut buffer: *mut CVPixelBuffer = ptr::null_mut();
    // SAFETY: the pool is live and the out pointer is writable.
    let status =
        unsafe { CVPixelBufferPool::create_pixel_buffer(None, pool, NonNull::from(&mut buffer)) };
    if status != kCVReturnSuccess {
        bail!("CVPixelBufferPoolCreatePixelBuffer: {status}");
    }
    let buffer = NonNull::new(buffer).ok_or_else(|| anyhow!("null buffer"))?;
    // SAFETY: Create returns a +1 reference.
    Ok(unsafe { CFRetained::from_raw(buffer) })
}

fn copy_nv12(src: &ffmpeg::frame::Video, dst: &CVPixelBuffer) -> Result<()> {
    let flags = CVPixelBufferLockFlags(0);
    // SAFETY: the buffer is live; it is unlocked below on every path.
    if unsafe { CVPixelBufferLockBaseAddress(dst, flags) } != kCVReturnSuccess {
        bail!("lock pixel buffer");
    }
    let result = copy_nv12_locked(src, dst);
    // SAFETY: matches the successful lock above.
    unsafe { CVPixelBufferUnlockBaseAddress(dst, flags) };
    result
}

/// Copies the two NV12 planes row by row. Every bound is checked before writing: the
/// destination must have two planes, a non-null base address, rows at least `width` bytes
/// long and enough rows; the source rows are taken through checked slices.
fn copy_nv12_locked(src: &ffmpeg::frame::Video, dst: &CVPixelBuffer) -> Result<()> {
    if src.format() != Pixel::NV12 {
        bail!("expected an NV12 frame, got {:?}", src.format());
    }
    if CVPixelBufferGetPlaneCount(dst) != 2 {
        bail!("pixel buffer is not biplanar");
    }
    let width = src.width() as usize;
    let height = src.height() as usize;
    for (plane, rows) in [(0usize, height), (1usize, height.div_ceil(2))] {
        let base = CVPixelBufferGetBaseAddressOfPlane(dst, plane) as *mut u8;
        if base.is_null() {
            bail!("plane {plane} has no base address");
        }
        let dst_stride = CVPixelBufferGetBytesPerRowOfPlane(dst, plane);
        let dst_rows = CVPixelBufferGetHeightOfPlane(dst, plane);
        if dst_stride < width || dst_rows < rows {
            bail!("plane {plane} is {dst_stride} bytes x {dst_rows} rows, need {width} x {rows}");
        }
        let src_stride = src.stride(plane);
        let data = src.data(plane);
        for row in 0..rows {
            let line = data
                .get(row * src_stride..row * src_stride + width)
                .ok_or_else(|| anyhow!("source plane {plane} row {row} out of bounds"))?;
            // SAFETY: base is non-null and the plane holds dst_rows rows of dst_stride bytes;
            // row < dst_rows and width <= dst_stride, so the write stays inside the plane.
            unsafe {
                ptr::copy_nonoverlapping(line.as_ptr(), base.add(row * dst_stride), width);
            }
        }
    }
    Ok(())
}
