//! The microphone through cpal (cargo feature `mic`).
//!
//! cpal's `Stream` is not `Send` on every platform, so a stream lives on its own thread: the
//! thread builds it, plays it, reports the result, and drops it when told to stop. The capture
//! callback downmixes to mono and pushes into a ring buffer without ever blocking (see
//! [`crate::ring_buffer`]); [`MicSource::read`] drains the ring.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, SampleFormat, SizedSample, StreamConfig};

use crate::convert::downmix_interleaved;
use crate::error::AudioError;
use crate::ring::{Consumer, Producer, ring_buffer};
use crate::source::{AudioSource, ReadResult};

/// Seconds of audio the ring holds before the newest samples are dropped.
const RING_SECONDS: usize = 4;

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

fn build<T: SizedSample + Send + 'static>(
    device: &cpal::Device,
    config: StreamConfig,
    producer: Producer,
    error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, cpal::Error>
where
    f32: cpal::FromSample<T>,
{
    let channels = usize::from(config.channels);
    // Both buffers are reused, so after the first callback nothing allocates.
    let mut converted: Vec<f32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], _| {
            converted.clear();
            converted.extend(data.iter().map(|s| s.to_sample::<f32>()));
            mono.clear();
            downmix_interleaved(&converted, channels, &mut mono);
            producer.push(&mono);
        },
        move |e| {
            if let Ok(mut slot) = error.try_lock() {
                *slot = Some(e.to_string());
            }
        },
        None,
    )
}

/// The microphone as an [`AudioSource`]: the default input device, or one picked by name.
pub struct MicSource {
    device: Option<String>,
    rate: u32,
    channels: u16,
    format: SampleFormat,
    consumer: Option<Consumer>,
    error: Arc<Mutex<Option<String>>>,
    stop: Option<Sender<()>>,
    thread: Option<JoinHandle<()>>,
    stopped: bool,
}

impl std::fmt::Debug for MicSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MicSource")
            .field("device", &self.device)
            .field("rate", &self.rate)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

impl MicSource {
    /// Opens the default input device (`None`) or the one with this name. Nothing is captured
    /// until [`AudioSource::start`], which is when the system may ask for permission.
    pub fn new(device: Option<&str>) -> Result<Self, AudioError> {
        let dev = pick_device(device)?;
        let cfg = dev.default_input_config().map_err(map_err)?;
        Ok(MicSource {
            device: device.map(str::to_string),
            rate: cfg.sample_rate(),
            channels: cfg.channels(),
            format: cfg.sample_format(),
            consumer: None,
            error: Arc::new(Mutex::new(None)),
            stop: None,
            thread: None,
            stopped: false,
        })
    }

    /// Samples the capture callback had to drop because the consumer fell behind (0 before
    /// the first start).
    pub fn dropped_frames(&self) -> u64 {
        self.consumer.as_ref().map_or(0, Consumer::dropped_frames)
    }

    fn open_stream(
        device: Option<String>,
        format: SampleFormat,
        producer: Producer,
        error: Arc<Mutex<Option<String>>>,
    ) -> Result<cpal::Stream, AudioError> {
        let dev = pick_device(device.as_deref())?;
        let supported = dev.default_input_config().map_err(map_err)?;
        let config: StreamConfig = supported.into();
        let r = match format {
            SampleFormat::F32 => build::<f32>(&dev, config, producer, error),
            SampleFormat::I16 => build::<i16>(&dev, config, producer, error),
            SampleFormat::U16 => build::<u16>(&dev, config, producer, error),
            SampleFormat::I32 => build::<i32>(&dev, config, producer, error),
            other => {
                return Err(AudioError::UnsupportedFormat(format!("{other:?}")));
            }
        };
        let stream = r.map_err(map_err)?;
        stream.play().map_err(map_err)?;
        Ok(stream)
    }
}

impl AudioSource for MicSource {
    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn start(&mut self) -> Result<(), AudioError> {
        if self.thread.is_some() {
            return Err(AudioError::State("already started"));
        }
        let (producer, consumer) = ring_buffer(self.rate as usize * RING_SECONDS);
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), AudioError>>();
        let (device, format, error) = (self.device.clone(), self.format, self.error.clone());
        let handle = std::thread::Builder::new()
            .name("crastu-mic".into())
            .spawn(
                move || match Self::open_stream(device, format, producer, error) {
                    Ok(stream) => {
                        let _ = ready_tx.send(Ok(()));
                        // Blocks until stop() sends or drops the sender; then the stream drops.
                        let _ = stop_rx.recv();
                        drop(stream);
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                    }
                },
            )
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
        let Some(consumer) = &self.consumer else {
            return Err(AudioError::State("read before start"));
        };
        if let Some(e) = self.error.lock().ok().and_then(|mut s| s.take()) {
            return Err(AudioError::Device(e));
        }
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
