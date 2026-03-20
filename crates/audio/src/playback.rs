//! Audio playback with an adaptive jitter buffer.
//!
//! Incoming decoded [`AudioFrame`]s are pushed into a lock-free ring buffer
//! (via [`ringbuf`]) that serves as the jitter buffer. The cpal output
//! callback pulls samples from this buffer in real time.
//!
//! * **Underrun** — the callback outputs silence for any samples not available.
//! * **Overrun** — excess samples are dropped on the producer side.
//! * **Crossfade** — a linear fade-out envelope can be triggered for smooth
//!   audio transitions during KVM switches.

use crate::{AudioDeviceInfo, AudioError, AudioFrame};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::HeapRb;
use ringbuf::traits::{Consumer, Producer, Split};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Configuration for audio playback.
#[derive(Debug, Clone)]
pub struct PlaybackConfig {
    /// Specific output device name, or `None` for the default.
    pub device_name: Option<String>,
    /// Sample rate in Hz.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u16,
    /// Target jitter buffer depth in milliseconds.
    pub jitter_buffer_ms: u32,
}

/// Plays audio received through a channel, backed by a jitter buffer.
pub struct AudioPlayback {
    device: cpal::Device,
    stream_config: cpal::StreamConfig,
    stream: Option<cpal::Stream>,
    jitter_buffer_samples: usize,
    // Shared with the output callback for crossfade control.
    crossfade_total: Arc<AtomicU32>,
    crossfade_remaining: Arc<AtomicU32>,
    fill_task: Option<tokio::task::JoinHandle<()>>,
}

impl AudioPlayback {
    /// Create a new playback instance (does not start playing yet).
    pub fn new(config: PlaybackConfig) -> Result<Self, AudioError> {
        let host = cpal::default_host();

        let device = match &config.device_name {
            Some(name) => host
                .output_devices()
                .map_err(|e| AudioError::DeviceError(e.to_string()))?
                .find(|d| d.name().unwrap_or_default() == *name)
                .ok_or_else(|| {
                    AudioError::DeviceNotFound(format!(
                        "output device '{name}' not found"
                    ))
                })?,
            None => host.default_output_device().ok_or_else(|| {
                AudioError::DeviceNotFound(
                    "no default output device".into(),
                )
            })?,
        };

        let stream_config = cpal::StreamConfig {
            channels: config.channels,
            sample_rate: cpal::SampleRate(config.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let jitter_buffer_samples = (config.sample_rate
            * config.channels as u32
            * config.jitter_buffer_ms
            / 1000) as usize;

        Ok(Self {
            device,
            stream_config,
            stream: None,
            jitter_buffer_samples,
            crossfade_total: Arc::new(AtomicU32::new(0)),
            crossfade_remaining: Arc::new(AtomicU32::new(0)),
            fill_task: None,
        })
    }

    /// Enumerate available output devices.
    pub fn list_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
        let host = cpal::default_host();
        let mut devices = Vec::new();
        for dev in host
            .output_devices()
            .map_err(|e| AudioError::DeviceError(e.to_string()))?
        {
            devices.push(AudioDeviceInfo {
                name: dev.name().unwrap_or_default(),
                is_input: false,
                is_loopback: false,
            });
        }
        Ok(devices)
    }

    /// Start playback. Decoded [`AudioFrame`]s should be sent to `frame_rx`.
    pub fn start(
        &mut self,
        mut frame_rx: mpsc::Receiver<AudioFrame>,
    ) -> Result<(), AudioError> {
        // Ring buffer sized at 3× the jitter depth to absorb bursts.
        let capacity = self.jitter_buffer_samples.max(1) * 3;
        let rb = HeapRb::<f32>::new(capacity);
        let (mut prod, mut cons) = rb.split();

        let cf_total = Arc::clone(&self.crossfade_total);
        let cf_remaining = Arc::clone(&self.crossfade_remaining);

        let stream = self
            .device
            .build_output_stream(
                &self.stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Pull from the jitter buffer.
                    let read = cons.pop_slice(data);
                    // Silence on underrun.
                    for s in &mut data[read..] {
                        *s = 0.0;
                    }

                    // Apply crossfade envelope if active.
                    let remaining = cf_remaining.load(Ordering::Relaxed);
                    if remaining > 0 {
                        let total = cf_total.load(Ordering::Relaxed) as f32;
                        if total > 0.0 {
                            let to_fade = (remaining as usize).min(data.len());
                            for (i, sample) in data.iter_mut().enumerate() {
                                if i < to_fade {
                                    let r = remaining as f32 - i as f32;
                                    *sample *= r / total;
                                } else {
                                    *sample = 0.0;
                                }
                            }
                            let new_remaining =
                                (remaining as usize).saturating_sub(data.len())
                                    as u32;
                            cf_remaining
                                .store(new_remaining, Ordering::Relaxed);
                        }
                    }
                },
                |err| tracing::error!("playback stream error: {err}"),
                None,
            )
            .map_err(|e| AudioError::StreamError(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::StreamError(e.to_string()))?;
        self.stream = Some(stream);

        // Background task: receive decoded frames and push into jitter buffer.
        let fill_task = tokio::spawn(async move {
            while let Some(frame) = frame_rx.recv().await {
                let mut offset = 0;
                while offset < frame.samples.len() {
                    let pushed =
                        prod.push_slice(&frame.samples[offset..]);
                    if pushed == 0 {
                        // Buffer full (overrun) — drop the rest of this frame.
                        tracing::debug!(
                            "jitter buffer overrun, dropping samples"
                        );
                        break;
                    }
                    offset += pushed;
                }
            }
        });
        self.fill_task = Some(fill_task);

        Ok(())
    }

    /// Stop playback and release resources.
    pub fn stop(&mut self) {
        self.stream.take();
        if let Some(task) = self.fill_task.take() {
            task.abort();
        }
    }

    /// Trigger a linear fade-out over `duration_ms` milliseconds.
    ///
    /// Call this when the KVM switches away from this machine to avoid
    /// audio pops/clicks.
    pub fn trigger_crossfade(&self, duration_ms: u32) {
        let total_samples = self.stream_config.sample_rate.0
            * self.stream_config.channels as u32
            * duration_ms
            / 1000;
        self.crossfade_total
            .store(total_samples, Ordering::Relaxed);
        self.crossfade_remaining
            .store(total_samples, Ordering::Release);
    }
}
