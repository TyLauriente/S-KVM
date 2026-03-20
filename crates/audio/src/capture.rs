//! Audio capture using cpal — supports both microphone input and loopback
//! (system audio) capture via PulseAudio/PipeWire monitor devices.

use crate::{AudioDeviceInfo, AudioError, AudioFrame};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tokio::sync::mpsc;

/// Configuration for audio capture.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Specific device name, or `None` for the default.
    pub device_name: Option<String>,
    /// If true, attempt to capture system audio output (loopback/monitor).
    pub loopback: bool,
    /// Desired sample rate in Hz.
    pub sample_rate: u32,
    /// Desired number of channels.
    pub channels: u16,
    /// Frame duration in ms — controls how many samples per [`AudioFrame`].
    pub frame_duration_ms: u32,
}

/// Captures audio from an input or loopback device and emits [`AudioFrame`]s.
pub struct AudioCapture {
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
    frame_samples: usize,
}

impl AudioCapture {
    /// Create a new capture instance (does not start recording yet).
    pub fn new(config: CaptureConfig) -> Result<Self, AudioError> {
        let host = cpal::default_host();

        let device = match &config.device_name {
            Some(name) => find_device_by_name(&host, name)?,
            None if config.loopback => find_loopback_device(&host)?,
            None => host
                .default_input_device()
                .ok_or_else(|| {
                    AudioError::DeviceNotFound("no default input device".into())
                })?,
        };

        let stream_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // Total interleaved samples per frame.
        let frame_samples = (config.sample_rate
            * config.channels as u32
            * config.frame_duration_ms
            / 1000) as usize;

        Ok(Self {
            device,
            stream_config,
            stream: None,
            frame_samples,
        })
    }

    /// Enumerate available input devices.
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
        let host = cpal::default_host();
        let mut devices = Vec::new();
        for dev in host
            .input_devices()
            .map_err(|e| AudioError::DeviceError(e.to_string()))?
        {
            let name = dev.name().unwrap_or_default();
            let lower = name.to_lowercase();
            devices.push(AudioDeviceInfo {
                name,
                is_input: true,
                is_loopback: lower.contains("monitor")
                    || lower.contains("loopback"),
            });
        }
        Ok(devices)
    }

    /// Start capturing. Frames are sent through `tx`.
    ///
    /// Samples are accumulated in the real-time audio callback until a full
    /// frame (determined by `frame_duration_ms`) is ready, then sent via
    /// [`try_send`](mpsc::Sender::try_send) to avoid blocking the audio thread.
    pub fn start(&mut self, tx: mpsc::Sender<AudioFrame>) -> Result<(), AudioError> {
        let sr = self.stream_config.sample_rate.0;
        let ch = self.stream_config.channels;
        let frame_samples = self.frame_samples;
        let frame_dur_us =
            (frame_samples as u64 * 1_000_000) / (sr as u64 * ch as u64);

        let mut acc: Vec<f32> = Vec::with_capacity(frame_samples * 2);
        let mut ts: u64 = 0;

        let stream = self
            .device
            .build_input_stream(
                &self.stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    acc.extend_from_slice(data);
                    while acc.len() >= frame_samples {
                        let samples: Vec<f32> =
                            acc.drain(..frame_samples).collect();
                        let _ = tx.try_send(AudioFrame {
                            samples,
                            sample_rate: sr,
                            channels: ch,
                            timestamp_us: ts,
                        });
                        ts += frame_dur_us;
                    }
                },
                |err| tracing::error!("capture stream error: {err}"),
                None,
            )
            .map_err(|e| AudioError::StreamError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::StreamError(e.to_string()))?;

        self.stream = Some(stream);
        Ok(())
    }

    /// Stop capturing. The underlying cpal stream is dropped.
    pub fn stop(&mut self) {
        self.stream.take();
    }
}

// ---------------------------------------------------------------------------
// Device helpers
// ---------------------------------------------------------------------------

fn find_device_by_name(
    host: &cpal::Host,
    name: &str,
) -> Result<cpal::Device, AudioError> {
    for dev in host
        .input_devices()
        .map_err(|e| AudioError::DeviceError(e.to_string()))?
    {
        if dev.name().unwrap_or_default() == name {
            return Ok(dev);
        }
    }
    Err(AudioError::DeviceNotFound(format!(
        "input device '{name}' not found"
    )))
}

fn find_loopback_device(
    host: &cpal::Host,
) -> Result<cpal::Device, AudioError> {
    // On Linux (PulseAudio / PipeWire), monitor sources appear as input devices.
    for dev in host
        .input_devices()
        .map_err(|e| AudioError::DeviceError(e.to_string()))?
    {
        if let Ok(name) = dev.name() {
            let lower = name.to_lowercase();
            if lower.contains("monitor") || lower.contains("loopback") {
                return Ok(dev);
            }
        }
    }
    Err(AudioError::DeviceNotFound(
        "no loopback/monitor device found; \
         ensure PulseAudio/PipeWire monitor sources are available"
            .into(),
    ))
}
