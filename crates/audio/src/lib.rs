#![forbid(unsafe_code)]

pub mod capture;
pub mod codec;
pub mod playback;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

/// A frame of interleaved PCM audio samples (f32, normalized to [-1.0, 1.0]).
#[derive(Debug, Clone)]
pub struct AudioFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Presentation timestamp in microseconds.
    pub timestamp_us: u64,
}

/// An Opus-encoded audio packet ready for network transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncodedAudioPacket {
    pub data: Vec<u8>,
    pub duration_us: u64,
    pub sequence: u64,
    pub timestamp_us: u64,
}

/// Information about an available audio device.
#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_input: bool,
    pub is_loopback: bool,
}

/// Configuration for the full audio pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioPipelineConfig {
    /// Sample rate in Hz (Opus native rate is 48000).
    pub sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    pub channels: u16,
    /// Opus frame duration in milliseconds (5, 10, or 20).
    pub frame_duration_ms: u32,
    /// Opus bitrate in kbps (64–128).
    pub bitrate_kbps: u32,
    /// Jitter buffer depth in milliseconds (10–20 for LAN).
    pub jitter_buffer_ms: u32,
    /// Enable Opus forward error correction.
    pub fec_enabled: bool,
}

impl Default for AudioPipelineConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            frame_duration_ms: 10,
            bitrate_kbps: 128,
            jitter_buffer_ms: 15,
            fec_enabled: true,
        }
    }
}

/// Errors from the audio subsystem.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio device not found: {0}")]
    DeviceNotFound(String),
    #[error("audio format not supported: {0}")]
    FormatNotSupported(String),
    #[error("audio stream error: {0}")]
    StreamError(String),
    #[error("opus codec error: {0}")]
    CodecError(String),
    #[error("audio channel closed")]
    ChannelClosed,
    #[error("device error: {0}")]
    DeviceError(String),
}

// ---------------------------------------------------------------------------
// AudioPipeline — chains capture → encode → [network] → decode → playback
// ---------------------------------------------------------------------------

/// Full audio pipeline that wires capture, codec, and playback together.
///
/// Use [`start_sender`](AudioPipeline::start_sender) on the source machine and
/// [`start_receiver`](AudioPipeline::start_receiver) on the target machine.
pub struct AudioPipeline {
    config: AudioPipelineConfig,
    sender: Option<SenderHandle>,
    receiver: Option<ReceiverHandle>,
}

struct SenderHandle {
    capture: capture::AudioCapture,
    _encode_task: tokio::task::JoinHandle<()>,
}

struct ReceiverHandle {
    playback: playback::AudioPlayback,
    _decode_task: tokio::task::JoinHandle<()>,
}

impl AudioPipeline {
    pub fn new(config: AudioPipelineConfig) -> Self {
        Self {
            config,
            sender: None,
            receiver: None,
        }
    }

    /// Start the capture side: capture → encode → `encoded_tx`.
    pub fn start_sender(
        &mut self,
        encoded_tx: mpsc::Sender<EncodedAudioPacket>,
    ) -> Result<(), AudioError> {
        let cfg = &self.config;

        let encoder = codec::OpusEncoder::new(codec::EncoderConfig {
            sample_rate: cfg.sample_rate,
            channels: cfg.channels,
            bitrate_kbps: cfg.bitrate_kbps,
            frame_duration_ms: cfg.frame_duration_ms,
            fec_enabled: cfg.fec_enabled,
        })?;

        let (frame_tx, mut frame_rx) = mpsc::channel::<AudioFrame>(64);

        let mut cap = capture::AudioCapture::new(capture::CaptureConfig {
            device_name: None,
            loopback: true,
            sample_rate: cfg.sample_rate,
            channels: cfg.channels,
            frame_duration_ms: cfg.frame_duration_ms,
        })?;
        cap.start(frame_tx)?;

        let frame_dur_us = cfg.frame_duration_ms as u64 * 1000;
        let encode_task = tokio::spawn(async move {
            let mut sequence: u64 = 0;
            while let Some(frame) = frame_rx.recv().await {
                let ts = frame.timestamp_us;
                match encoder.encode(&frame.samples).await {
                    Ok(data) => {
                        let pkt = EncodedAudioPacket {
                            data,
                            duration_us: frame_dur_us,
                            sequence,
                            timestamp_us: ts,
                        };
                        sequence += 1;
                        if encoded_tx.send(pkt).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => tracing::warn!("opus encode error: {e}"),
                }
            }
        });

        self.sender = Some(SenderHandle {
            capture: cap,
            _encode_task: encode_task,
        });
        Ok(())
    }

    /// Start the playback side: `encoded_rx` → decode → playback.
    pub fn start_receiver(
        &mut self,
        mut encoded_rx: mpsc::Receiver<EncodedAudioPacket>,
    ) -> Result<(), AudioError> {
        let cfg = &self.config;

        let decoder = codec::OpusDecoder::new(codec::DecoderConfig {
            sample_rate: cfg.sample_rate,
            channels: cfg.channels,
        })?;

        let (frame_tx, frame_rx) = mpsc::channel::<AudioFrame>(64);
        let sample_rate = cfg.sample_rate;
        let channels = cfg.channels;

        let mut pb = playback::AudioPlayback::new(playback::PlaybackConfig {
            device_name: None,
            sample_rate,
            channels,
            jitter_buffer_ms: cfg.jitter_buffer_ms,
        })?;
        pb.start(frame_rx)?;

        let decode_task = tokio::spawn(async move {
            let mut last_seq: Option<u64> = None;
            while let Some(pkt) = encoded_rx.recv().await {
                let lost = match last_seq {
                    Some(prev) => pkt.sequence > prev + 1,
                    None => false,
                };
                last_seq = Some(pkt.sequence);

                match decoder.decode(&pkt.data, lost).await {
                    Ok(samples) => {
                        let frame = AudioFrame {
                            samples,
                            sample_rate,
                            channels,
                            timestamp_us: pkt.timestamp_us,
                        };
                        if frame_tx.send(frame).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => tracing::warn!("opus decode error: {e}"),
                }
            }
        });

        self.receiver = Some(ReceiverHandle {
            playback: pb,
            _decode_task: decode_task,
        });
        Ok(())
    }

    /// Stop all audio processing.
    pub fn stop(&mut self) {
        if let Some(mut s) = self.sender.take() {
            s.capture.stop();
            s._encode_task.abort();
        }
        if let Some(mut r) = self.receiver.take() {
            r.playback.stop();
            r._decode_task.abort();
        }
    }

    /// Trigger a smooth crossfade on the playback side (for KVM switch transitions).
    pub fn trigger_crossfade(&self, duration_ms: u32) {
        if let Some(r) = &self.receiver {
            r.playback.trigger_crossfade(duration_ms);
        }
    }
}

impl Drop for AudioPipeline {
    fn drop(&mut self) {
        self.stop();
    }
}
