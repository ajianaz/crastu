//! The microphone through cpal (cargo feature `mic`).
//!
//! cpal's `Stream` is not `Send` on every platform, so a stream lives on its own thread: the
//! thread builds it, plays it, reports the result, and drops it when told to stop. The capture
//! callback downmixes to mono and pushes into a ring buffer without ever blocking or allocating
//! (see [`crate::ring_buffer`] and [`Sink`]); [`MicSource::read`] drains the ring.
//!
//! The device and the stream configuration are chosen once, in [`MicSource::new`], and
//! [`MicSource::start`] builds the stream from exactly those, so `sample_rate()`, the ring size
//! and a [`crate::Converter`] built from them always describe the stream that runs. If the
//! device went away or refuses that configuration, `start` fails with an error.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, SampleFormat, SizedSample, StreamConfig};

use crate::convert::downmix_interleaved;
use crate::error::AudioError;
use crate::resample::validate_rate;
use crate::ring::{Consumer, Producer, ring_buffer};
use crate::source::{AudioSource, ReadResult};

/// Seconds of audio the ring holds before the newest samples are dropped.
const RING_SECONDS: usize = 4;
/// Samples of one callback block the sink converts at a time (its buffers are this big, once).
/// A larger callback is handled in several blocks, so nothing is dropped and nothing allocates.
const BLOCK_SAMPLES: usize = 16_384;
/// More channels than this is not a microphone.
const MAX_CHANNELS: u16 = 64;

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|x| x.name().to_string())
        .unwrap_or_else(|_| d.to_string())
}

/// Names of the input devices the system offers.
pub fn input_device_names() -> Result<Vec<String>, AudioError> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(map_err)?;
    Ok(devices.map(|d| device_name(&d)).collect())
}

fn map_err(e: cpal::Error) -> AudioError {
    match e.kind() {
        ErrorKind::PermissionDenied => AudioError::PermissionDenied,
        ErrorKind::DeviceNotAvailable => AudioError::NoDevice(None),
        ErrorKind::UnsupportedConfig => AudioError::UnsupportedFormat(e.to_string()),
        _ => AudioError::Device(e.to_string()),
    }
}

/// Whether a stream error ends the capture. The others (a reroute to another device, an
/// overrun, a busy device, a refused real-time priority) leave the stream running; they are
/// only counted in [`MicStats::stream_events`].
fn is_fatal(kind: ErrorKind) -> bool {
    !matches!(
        kind,
        ErrorKind::DeviceChanged
            | ErrorKind::Xrun
            | ErrorKind::DeviceBusy
            | ErrorKind::RealtimeDenied
    )
}

/// What the stream's error callback does with an error: a fatal one is kept (the first wins),
/// any other is counted.
fn record_error(e: cpal::Error, fatal: &Mutex<Option<AudioError>>, events: &AtomicU64) {
    if is_fatal(e.kind()) {
        let mut slot = fatal.lock().unwrap_or_else(|p| p.into_inner());
        if slot.is_none() {
            *slot = Some(map_err(e));
        }
    } else {
        events.fetch_add(1, Ordering::Relaxed);
    }
}

/// A new capture starts with no error and no events from the one before.
fn reset_errors(fatal: &Mutex<Option<AudioError>>, events: &AtomicU64) {
    *fatal.lock().unwrap_or_else(|p| p.into_inner()) = None;
    events.store(0, Ordering::Relaxed);
}

/// Ring capacity for a sample rate.
fn ring_capacity(rate: u32) -> usize {
    rate as usize * RING_SECONDS
}

fn pick_device(name: Option<&str>) -> Result<cpal::Device, AudioError> {
    let host = cpal::default_host();
    match name {
        None => host
            .default_input_device()
            .ok_or(AudioError::NoDevice(None)),
        Some(n) => host
            .input_devices()
            .map_err(map_err)?
            .find(|d| device_name(d) == n)
            .ok_or_else(|| AudioError::NoDevice(Some(n.to_string()))),
    }
}

/// What the capture callback does with a block of device samples: convert to `f32`, downmix to
/// mono, push into the ring. Its buffers are allocated once, so the callback never allocates,
/// whatever size cpal's blocks are.
struct Sink {
    channels: usize,
    converted: Vec<f32>,
    mono: Vec<f32>,
    block: usize,
    producer: Producer,
}

impl Sink {
    fn new(channels: u16, producer: Producer) -> Self {
        let channels = usize::from(channels.max(1));
        let block = (BLOCK_SAMPLES / channels).max(1) * channels;
        Sink {
            channels,
            converted: Vec::with_capacity(block),
            mono: Vec::with_capacity(block / channels),
            block,
            producer,
        }
    }

    fn feed<T: SizedSample>(&mut self, data: &[T])
    where
        f32: cpal::FromSample<T>,
    {
        for part in data.chunks(self.block) {
            self.converted.clear();
            self.converted
                .extend(part.iter().map(|s| s.to_sample::<f32>()));
            self.mono.clear();
            downmix_interleaved(&self.converted, self.channels, &mut self.mono);
            self.producer.push(&self.mono);
        }
    }
}

/// Counters of a running (or finished) capture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MicStats {
    /// Samples dropped because the consumer fell behind.
    pub dropped_frames: u64,
    /// Non-fatal stream events (reroutes, overruns, busy device).
    pub stream_events: u64,
}

fn build<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    producer: Producer,
    fatal: Arc<Mutex<Option<AudioError>>>,
    events: Arc<AtomicU64>,
) -> Result<cpal::Stream, cpal::Error>
where
    f32: cpal::FromSample<T>,
{
    let mut sink = Sink::new(config.channels, producer);
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| sink.feed(data),
        move |e| record_error(e, &fatal, &events),
        None,
    )
}

/// The microphone as an [`AudioSource`]: the default input device, or one picked by name.
pub struct MicSource {
    device: cpal::Device,
    config: StreamConfig,
    format: SampleFormat,
    consumer: Option<Consumer>,
    /// The first fatal stream error; every `read` returns it from then on.
    fatal: Arc<Mutex<Option<AudioError>>>,
    events: Arc<AtomicU64>,
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    stopped: bool,
}

impl std::fmt::Debug for MicSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicSource")
            .field("config", &self.config)
            .field("format", &self.format)
            .finish_non_exhaustive()
    }
}

impl MicSource {
    /// Opens the default input device (`None`) or the one with this name, and fixes its default
    /// input configuration. Nothing is captured until [`AudioSource::start`], which is when the
    /// system may ask for permission.
    pub fn new(device: Option<&str>) -> Result<Self, AudioError> {
        let device = pick_device(device)?;
        let supported = device.default_input_config().map_err(map_err)?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        validate_rate(config.sample_rate)?;
        if config.channels == 0 || config.channels > MAX_CHANNELS {
            return Err(AudioError::UnsupportedFormat(format!(
                "{} input channels",
                config.channels
            )));
        }
        Ok(MicSource {
            device,
            config,
            format,
            consumer: None,
            fatal: Arc::new(Mutex::new(None)),
            events: Arc::new(AtomicU64::new(0)),
            stop: None,
            thread: None,
            stopped: false,
        })
    }

    /// Dropped samples and non-fatal stream events so far.
    pub fn stats(&self) -> MicStats {
        MicStats {
            dropped_frames: self.consumer.as_ref().map_or(0, Consumer::dropped_frames),
            stream_events: self.events.load(Ordering::Relaxed),
        }
    }

    /// Samples the capture callback had to drop because the consumer fell behind.
    pub fn dropped_frames(&self) -> u64 {
        self.stats().dropped_frames
    }

    fn open_stream(
        device: &cpal::Device,
        config: StreamConfig,
        format: SampleFormat,
        producer: Producer,
        fatal: Arc<Mutex<Option<AudioError>>>,
        events: Arc<AtomicU64>,
    ) -> Result<cpal::Stream, AudioError> {
        let r = match format {
            SampleFormat::F32 => build::<f32>(device, config, producer, fatal, events),
            SampleFormat::I16 => build::<i16>(device, config, producer, fatal, events),
            SampleFormat::U16 => build::<u16>(device, config, producer, fatal, events),
            SampleFormat::I32 => build::<i32>(device, config, producer, fatal, events),
            other => return Err(AudioError::UnsupportedFormat(format!("{other:?}"))),
        };
        let stream = r.map_err(map_err)?;
        stream.play().map_err(map_err)?;
        Ok(stream)
    }
}

impl AudioSource for MicSource {
    fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    fn start(&mut self) -> Result<(), AudioError> {
        if self.thread.is_some() {
            return Err(AudioError::State("already started"));
        }
        reset_errors(&self.fatal, &self.events);
        let (producer, consumer) = ring_buffer(ring_capacity(self.config.sample_rate));
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AudioError>>();
        let (device, config, format) = (self.device.clone(), self.config, self.format);
        let (fatal, events) = (self.fatal.clone(), self.events.clone());
        let handle = std::thread::Builder::new()
            .name("crastu-mic".into())
            .spawn(move || {
                match Self::open_stream(&device, config, format, producer, fatal, events) {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        // Blocks until stop() sends or drops the sender; then the stream drops.
                        let _ = stop_rx.recv();
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                }
            })
            .map_err(|e| AudioError::Device(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = handle.join();
                return Err(e);
            }
            Err(_) => return Err(AudioError::Device("capture thread died".into())),
        }
        self.consumer = Some(consumer);
        self.stop = Some(stop_tx);
        self.thread = Some(handle);
        self.stopped = false;
        Ok(())
    }

    fn stop(&mut self) {
        self.stopped = true;
        drop(self.stop.take());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn read(&mut self, buf: &mut [f32]) -> Result<ReadResult, AudioError> {
        if let Some(e) = self.fatal.lock().unwrap_or_else(|p| p.into_inner()).clone() {
            return Err(e);
        }
        let Some(consumer) = &self.consumer else {
            return if self.stopped {
                Ok(ReadResult::End)
            } else {
                Err(AudioError::State("read before start"))
            };
        };
        let n = consumer.pop(buf);
        Ok(if n > 0 {
            ReadResult::Samples(n)
        } else if self.stopped {
            ReadResult::End
        } else {
            ReadResult::Pending
        })
    }
}

impl Drop for MicSource {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fatal_kinds_end_the_stream() {
        for k in [
            ErrorKind::DeviceChanged,
            ErrorKind::Xrun,
            ErrorKind::DeviceBusy,
            ErrorKind::RealtimeDenied,
        ] {
            assert!(!is_fatal(k), "{k:?}");
        }
        for k in [
            ErrorKind::DeviceNotAvailable,
            ErrorKind::StreamInvalidated,
            ErrorKind::PermissionDenied,
            ErrorKind::BackendError,
        ] {
            assert!(is_fatal(k), "{k:?}");
        }
    }

    #[test]
    fn a_new_capture_forgets_the_old_fatal_error_and_counters() {
        let (fatal, events) = (Mutex::new(None), AtomicU64::new(0));
        record_error(cpal::Error::new(ErrorKind::Xrun), &fatal, &events);
        record_error(cpal::Error::new(ErrorKind::DeviceChanged), &fatal, &events);
        assert_eq!(events.load(Ordering::Relaxed), 2);
        assert!(fatal.lock().unwrap().is_none());
        record_error(
            cpal::Error::new(ErrorKind::DeviceNotAvailable),
            &fatal,
            &events,
        );
        record_error(
            cpal::Error::new(ErrorKind::PermissionDenied),
            &fatal,
            &events,
        );
        // The first fatal error stays.
        assert_eq!(*fatal.lock().unwrap(), Some(AudioError::NoDevice(None)));
        reset_errors(&fatal, &events);
        assert!(fatal.lock().unwrap().is_none());
        assert_eq!(events.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn the_ring_is_sized_from_the_stream_rate() {
        assert_eq!(ring_capacity(48_000), 192_000);
        assert_eq!(ring_capacity(44_100), 176_400);
    }

    #[test]
    fn the_sink_downmixes_converts_and_never_regrows_its_buffers() {
        let (producer, consumer) = ring_buffer(1 << 20);
        let mut sink = Sink::new(2, producer);
        let (cc, mc) = (sink.converted.capacity(), sink.mono.capacity());
        // i16 stereo, much larger than one block: left 16384 and right -16384 cancel.
        let data: Vec<i16> = (0..BLOCK_SAMPLES * 5)
            .map(|i| if i % 2 == 0 { 16384 } else { -16384 })
            .collect();
        sink.feed(&data);
        assert_eq!(consumer.len(), BLOCK_SAMPLES * 5 / 2);
        let mut out = vec![1.0f32; 100];
        assert_eq!(consumer.pop(&mut out), 100);
        assert!(out.iter().all(|v| *v == 0.0));
        assert_eq!((sink.converted.capacity(), sink.mono.capacity()), (cc, mc));
        // f32 mono passes through.
        let (producer, consumer) = ring_buffer(16);
        let mut sink = Sink::new(1, producer);
        sink.feed(&[0.25f32, -0.5]);
        let mut out = [0.0; 2];
        assert_eq!(consumer.pop(&mut out), 2);
        assert_eq!(out, [0.25, -0.5]);
    }

    #[test]
    fn a_block_boundary_never_splits_a_frame() {
        // 3 channels: the block is a multiple of 3, so every frame stays whole.
        let (producer, consumer) = ring_buffer(1 << 16);
        let mut sink = Sink::new(3, producer);
        assert_eq!(sink.block % 3, 0);
        let data: Vec<f32> = (0..sink.block * 2 + 6).map(|_| 0.3).collect();
        sink.feed(&data);
        assert_eq!(consumer.len(), data.len() / 3);
    }
}
