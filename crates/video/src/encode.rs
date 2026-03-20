//! Video encoding pipeline.

use anyhow::Result;

use crate::types::*;

/// Trait for video encoders.
pub trait VideoEncoder: Send {
    /// Encode a raw frame, returning zero or more encoded packets.
    fn encode(&mut self, frame: &VideoFrame) -> Result<Vec<EncodedPacket>>;

    /// Flush any buffered frames from the encoder.
    fn flush(&mut self) -> Result<Vec<EncodedPacket>>;

    /// Force the next encoded frame to be a keyframe (IDR).
    fn force_keyframe(&mut self);
}

// ---------------------------------------------------------------------------
// RawEncoder — no compression, wraps frames with a small header
// ---------------------------------------------------------------------------

/// Passes raw frame data through with a metadata header.
/// Always available — no system library dependencies.
pub struct RawEncoder {
    config: EncoderConfig,
    force_kf: bool,
    frame_count: u64,
    keyframe_interval: u64,
}

impl RawEncoder {
    pub fn new(config: EncoderConfig) -> Self {
        Self {
            config,
            force_kf: true,
            frame_count: 0,
            keyframe_interval: 30,
        }
    }
}

impl VideoEncoder for RawEncoder {
    fn encode(&mut self, frame: &VideoFrame) -> Result<Vec<EncodedPacket>> {
        let is_keyframe = self.force_kf || self.frame_count % self.keyframe_interval == 0;
        self.force_kf = false;

        let pts = self.frame_count as i64;
        self.frame_count += 1;

        let mut data = Vec::with_capacity(RAW_HEADER_SIZE + frame.data.len());
        write_raw_header(&mut data, frame.width, frame.height, frame.format);
        data.extend_from_slice(&frame.data);

        Ok(vec![EncodedPacket {
            data,
            pts,
            dts: pts,
            is_keyframe,
            codec: self.config.codec,
        }])
    }

    fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
        Ok(vec![])
    }

    fn force_keyframe(&mut self) {
        self.force_kf = true;
    }
}

// ---------------------------------------------------------------------------
// SoftwareEncoder — FFmpeg-based (feature = "ffmpeg")
// ---------------------------------------------------------------------------

#[cfg(feature = "ffmpeg")]
mod ffmpeg_enc {
    use super::*;
    use ffmpeg_next as ffmpeg;
    use s_kvm_core::protocol::VideoCodec;
    use tracing::{info, warn};

    /// Video encoder backed by FFmpeg (software or hardware-accelerated).
    ///
    /// Tries the configured `hw_accel` first; falls back to the generic
    /// software encoder if the hardware one is not available.
    pub struct SoftwareEncoder {
        encoder: ffmpeg::encoder::video::Video,
        scaler: ffmpeg::software::scaling::Context,
        config: EncoderConfig,
        force_kf: bool,
        frame_count: u64,
    }

    impl SoftwareEncoder {
        pub fn new(config: EncoderConfig) -> Result<Self> {
            ffmpeg::init()?;

            // Try hardware encoder first, then fall back to software.
            let (codec, encoder_label) = find_encoder(&config)?;

            let context = ffmpeg::codec::context::Context::new_with_codec(codec);
            let mut video = context.encoder().video()?;
            video.set_width(config.width);
            video.set_height(config.height);
            video.set_format(ffmpeg::format::Pixel::YUV420P);
            video.set_time_base(ffmpeg::Rational(1, config.fps as i32));
            video.set_bit_rate(usize::from(config.bitrate_kbps) * 1000);
            video.set_max_b_frames(config.max_b_frames as i32);

            let opts = build_encoder_opts(&config, &encoder_label);
            let encoder = video.open_as_with(codec, opts)?;

            info!(
                encoder = encoder_label,
                width = config.width,
                height = config.height,
                bitrate_kbps = config.bitrate_kbps,
                "opened video encoder"
            );

            let scaler = ffmpeg::software::scaling::Context::get(
                ffmpeg::format::Pixel::BGRA,
                config.width,
                config.height,
                ffmpeg::format::Pixel::YUV420P,
                config.width,
                config.height,
                ffmpeg::software::scaling::Flags::BILINEAR,
            )?;

            Ok(Self {
                encoder,
                scaler,
                config,
                force_kf: true,
                frame_count: 0,
            })
        }

        fn receive_packets(&mut self) -> Result<Vec<EncodedPacket>> {
            let mut packets = Vec::new();
            let mut pkt = ffmpeg::Packet::empty();
            while self.encoder.receive_packet(&mut pkt).is_ok() {
                packets.push(EncodedPacket {
                    data: pkt.data().unwrap_or(&[]).to_vec(),
                    pts: pkt.pts().unwrap_or(0),
                    dts: pkt.dts().unwrap_or(0),
                    is_keyframe: pkt.is_key(),
                    codec: self.config.codec,
                });
            }
            Ok(packets)
        }
    }

    impl VideoEncoder for SoftwareEncoder {
        fn encode(&mut self, frame: &VideoFrame) -> Result<Vec<EncodedPacket>> {
            let mut src = ffmpeg::frame::Video::new(
                ffmpeg::format::Pixel::BGRA,
                frame.width,
                frame.height,
            );

            // Copy pixel data, respecting potential stride padding.
            let stride = src.stride(0);
            let row_bytes = frame.width as usize * 4;
            let plane = src.data_mut(0);
            if stride == row_bytes {
                plane[..frame.data.len()].copy_from_slice(&frame.data);
            } else {
                for y in 0..frame.height as usize {
                    let src_off = y * row_bytes;
                    let dst_off = y * stride;
                    plane[dst_off..dst_off + row_bytes]
                        .copy_from_slice(&frame.data[src_off..src_off + row_bytes]);
                }
            }

            let mut yuv = ffmpeg::frame::Video::new(
                ffmpeg::format::Pixel::YUV420P,
                self.config.width,
                self.config.height,
            );
            self.scaler.run(&src, &mut yuv)?;

            yuv.set_pts(Some(self.frame_count as i64));

            if self.force_kf {
                yuv.set_kind(ffmpeg::picture::Type::I);
                self.force_kf = false;
            }

            self.frame_count += 1;
            self.encoder.send_frame(&yuv)?;
            self.receive_packets()
        }

        fn flush(&mut self) -> Result<Vec<EncodedPacket>> {
            self.encoder.send_eof()?;
            self.receive_packets()
        }

        fn force_keyframe(&mut self) {
            self.force_kf = true;
        }
    }

    /// Locate the best available encoder for the given config.
    /// Returns the codec and a human-readable label.
    fn find_encoder(config: &EncoderConfig) -> Result<(ffmpeg::Codec, String)> {
        // Try hardware encoder if requested.
        if config.hw_accel != HwAccel::None {
            if let Some(name) = config.hw_accel.encoder_name(config.codec) {
                if let Some(codec) = ffmpeg::encoder::find_by_name(name) {
                    return Ok((codec, name.to_string()));
                }
                warn!(
                    hw = ?config.hw_accel,
                    name,
                    "hardware encoder not found, falling back to software"
                );
            }
        }

        // Software fallback.
        let codec_id = video_codec_to_ffmpeg(config.codec);
        let codec = ffmpeg::encoder::find(codec_id)
            .ok_or_else(|| anyhow::anyhow!("encoder for {:?} not found", config.codec))?;
        let label = format!("{:?} (software)", config.codec);
        Ok((codec, label))
    }

    /// Build FFmpeg option dictionary for ultra-low-latency / KVM-style encoding.
    fn build_encoder_opts(config: &EncoderConfig, encoder_name: &str) -> ffmpeg::Dictionary {
        let mut opts = ffmpeg::Dictionary::new();
        let is_nvenc = encoder_name.contains("nvenc");
        let is_qsv = encoder_name.contains("qsv");
        let is_vaapi = encoder_name.contains("vaapi");

        // --- Rate control ---
        match config.rate_control {
            RateControl::Cbr => {
                if is_nvenc {
                    opts.set("rc", "cbr");
                } else if is_qsv {
                    opts.set("rate_control", "cbr");
                } else {
                    // x264/x265: use nal-hrd + vbv for CBR-like behaviour
                    opts.set("nal-hrd", "cbr");
                }
            }
            RateControl::Vbr => {
                if is_nvenc {
                    opts.set("rc", "vbr");
                }
            }
            RateControl::Crf { quality } => {
                if !is_nvenc && !is_qsv && !is_vaapi {
                    opts.set("crf", &quality.to_string());
                }
            }
        }

        // --- Preset ---
        if is_nvenc {
            opts.set("preset", config.preset.nvenc_preset());
            opts.set("tune", "ull"); // ultra-low-latency
            opts.set("zerolatency", "1");
            opts.set("rc-lookahead", &config.lookahead.to_string());
        } else if !is_vaapi {
            // x264 / x265 software presets
            if config.codec == VideoCodec::H264 || config.codec == VideoCodec::H265 {
                opts.set("preset", config.preset.as_str());
                opts.set("tune", "zerolatency");
            }
        }

        // --- B-frames ---
        if config.max_b_frames == 0 && !is_vaapi {
            opts.set("bf", "0");
        }

        opts
    }

    fn video_codec_to_ffmpeg(codec: VideoCodec) -> ffmpeg::codec::Id {
        match codec {
            VideoCodec::H264 => ffmpeg::codec::Id::H264,
            VideoCodec::H265 => ffmpeg::codec::Id::HEVC,
            VideoCodec::VP9 => ffmpeg::codec::Id::VP9,
            VideoCodec::AV1 => ffmpeg::codec::Id::AV1,
        }
    }
}

#[cfg(feature = "ffmpeg")]
pub use ffmpeg_enc::SoftwareEncoder;

#[cfg(test)]
mod tests {
    use super::*;
    use s_kvm_core::protocol::VideoCodec;

    #[test]
    fn raw_encoder_roundtrip_metadata() {
        let config = EncoderConfig {
            codec: VideoCodec::H264,
            width: 4,
            height: 4,
            ..Default::default()
        };
        let mut enc = RawEncoder::new(config);

        let frame = VideoFrame {
            data: vec![0u8; 4 * 4 * 4],
            width: 4,
            height: 4,
            format: PixelFormat::Bgra,
            timestamp_us: 0,
            frame_number: 0,
            dirty_rects: vec![],
        };

        let packets = enc.encode(&frame).unwrap();
        assert_eq!(packets.len(), 1);
        assert!(packets[0].is_keyframe);
        assert_eq!(packets[0].data.len(), RAW_HEADER_SIZE + 4 * 4 * 4);
    }

    #[test]
    fn raw_encoder_keyframe_interval() {
        let config = EncoderConfig::default();
        let mut enc = RawEncoder::new(config);
        let frame = VideoFrame {
            data: vec![0u8; 1920 * 1080 * 4],
            width: 1920,
            height: 1080,
            format: PixelFormat::Bgra,
            timestamp_us: 0,
            frame_number: 0,
            dirty_rects: vec![],
        };

        // Frame 0 is a keyframe.
        let p = enc.encode(&frame).unwrap();
        assert!(p[0].is_keyframe);

        // Frame 1 is not.
        let p = enc.encode(&frame).unwrap();
        assert!(!p[0].is_keyframe);

        // After force_keyframe, next frame is a keyframe.
        enc.force_keyframe();
        let p = enc.encode(&frame).unwrap();
        assert!(p[0].is_keyframe);
    }

    #[test]
    fn low_latency_config_defaults() {
        let cfg = EncoderConfig::low_latency(VideoCodec::H264, 1920, 1080, 8000);
        assert_eq!(cfg.max_b_frames, 0);
        assert_eq!(cfg.lookahead, 0);
        assert_eq!(cfg.rate_control, RateControl::Cbr);
        assert_eq!(cfg.preset, EncoderPreset::Ultrafast);
    }
}
