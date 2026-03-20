//! Opus codec wrappers for low-latency audio encoding and decoding.
//!
//! Uses `RESTRICTED_LOWDELAY` application mode for minimum latency.
//! Encoding and decoding run on the blocking thread pool via `spawn_blocking`
//! to avoid stalling the async runtime.

use crate::AudioError;
use std::sync::Arc;

/// Configuration for the Opus encoder.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub sample_rate: u32,
    pub channels: u16,
    /// Target bitrate in kbps (64–128).
    pub bitrate_kbps: u32,
    /// Frame duration in milliseconds (5, 10, or 20).
    pub frame_duration_ms: u32,
    /// Enable forward error correction.
    pub fec_enabled: bool,
}

/// Configuration for the Opus decoder.
#[derive(Debug, Clone)]
pub struct DecoderConfig {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Opus encoder wrapper optimized for low-latency KVM audio.
pub struct OpusEncoder {
    inner: Arc<std::sync::Mutex<OpusEncoderInner>>,
    frame_size: usize,
    channels: u16,
}

struct OpusEncoderInner {
    encoder: opus::Encoder,
    output_buf: Vec<u8>,
}

/// Opus decoder wrapper with FEC support.
pub struct OpusDecoder {
    inner: Arc<std::sync::Mutex<OpusDecoderInner>>,
    channels: u16,
}

struct OpusDecoderInner {
    decoder: opus::Decoder,
    output_buf: Vec<f32>,
}

impl OpusEncoder {
    /// Create a new Opus encoder.
    ///
    /// Uses `Application::LowDelay` for minimum algorithmic delay.
    pub fn new(config: EncoderConfig) -> Result<Self, AudioError> {
        let ch = to_opus_channels(config.channels)?;

        let mut encoder =
            opus::Encoder::new(config.sample_rate, ch, opus::Application::LowDelay)
                .map_err(|e| AudioError::CodecError(e.to_string()))?;

        encoder
            .set_bitrate(opus::Bitrate::Bits(config.bitrate_kbps as i32 * 1000))
            .map_err(|e| AudioError::CodecError(e.to_string()))?;

        if config.fec_enabled {
            encoder
                .set_inband_fec(true)
                .map_err(|e| AudioError::CodecError(e.to_string()))?;
            encoder
                .set_packet_loss_perc(5)
                .map_err(|e| AudioError::CodecError(e.to_string()))?;
        }

        // Samples per channel per frame.
        let frame_size =
            (config.sample_rate * config.frame_duration_ms / 1000) as usize;

        Ok(Self {
            inner: Arc::new(std::sync::Mutex::new(OpusEncoderInner {
                encoder,
                output_buf: vec![0u8; 4000],
            })),
            frame_size,
            channels: config.channels,
        })
    }

    /// Encode interleaved PCM f32 samples to an Opus packet.
    ///
    /// `samples` must contain exactly [`frame_samples`](Self::frame_samples) values.
    pub async fn encode(&self, samples: &[f32]) -> Result<Vec<u8>, AudioError> {
        let inner = Arc::clone(&self.inner);
        let samples = samples.to_vec();

        tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| AudioError::CodecError("encoder lock poisoned".into()))?;
            let OpusEncoderInner { encoder, output_buf } = &mut *guard;
            let len = encoder
                .encode_float(&samples, output_buf)
                .map_err(|e| AudioError::CodecError(e.to_string()))?;
            Ok(output_buf[..len].to_vec())
        })
        .await
        .map_err(|e| AudioError::CodecError(format!("spawn_blocking: {e}")))?
    }

    /// Samples per channel in one frame.
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Total interleaved samples per frame (`frame_size * channels`).
    pub fn frame_samples(&self) -> usize {
        self.frame_size * self.channels as usize
    }
}

impl OpusDecoder {
    /// Create a new Opus decoder.
    pub fn new(config: DecoderConfig) -> Result<Self, AudioError> {
        let ch = to_opus_channels(config.channels)?;

        let decoder = opus::Decoder::new(config.sample_rate, ch)
            .map_err(|e| AudioError::CodecError(e.to_string()))?;

        // Buffer large enough for the maximum Opus frame (120 ms).
        let max_samples =
            (config.sample_rate as usize / 1000) * 120 * config.channels as usize;

        Ok(Self {
            inner: Arc::new(std::sync::Mutex::new(OpusDecoderInner {
                decoder,
                output_buf: vec![0.0f32; max_samples],
            })),
            channels: config.channels,
        })
    }

    /// Decode an Opus packet to interleaved PCM f32 samples.
    ///
    /// When `use_fec` is true the decoder uses forward-error-correction data
    /// embedded in this packet to reconstruct the *previous* lost packet.
    pub async fn decode(
        &self,
        data: &[u8],
        use_fec: bool,
    ) -> Result<Vec<f32>, AudioError> {
        let inner = Arc::clone(&self.inner);
        let data = data.to_vec();
        let channels = self.channels;

        tokio::task::spawn_blocking(move || {
            let mut guard = inner
                .lock()
                .map_err(|_| AudioError::CodecError("decoder lock poisoned".into()))?;
            let OpusDecoderInner { decoder, output_buf } = &mut *guard;
            let samples_per_ch = decoder
                .decode_float(&data, output_buf, use_fec)
                .map_err(|e| AudioError::CodecError(e.to_string()))?;
            let total = samples_per_ch * channels as usize;
            Ok(output_buf[..total].to_vec())
        })
        .await
        .map_err(|e| AudioError::CodecError(format!("spawn_blocking: {e}")))?
    }
}

fn to_opus_channels(channels: u16) -> Result<opus::Channels, AudioError> {
    match channels {
        1 => Ok(opus::Channels::Mono),
        2 => Ok(opus::Channels::Stereo),
        n => Err(AudioError::FormatNotSupported(format!(
            "opus supports 1 or 2 channels, got {n}"
        ))),
    }
}
