//! Shared types for the video pipeline.

use s_kvm_core::protocol::VideoCodec;
use serde::{Deserialize, Serialize};

/// Pixel format of raw frame data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    /// 4 bytes per pixel: Blue, Green, Red, Alpha.
    Bgra,
    /// 4 bytes per pixel: Red, Green, Blue, Alpha.
    Rgba,
    /// Planar YUV 4:2:0 with interleaved UV plane.
    Nv12,
    /// Planar YUV 4:2:0 with separate U and V planes.
    Yuv420p,
}

impl PixelFormat {
    /// Total byte size for a frame of this format at the given resolution.
    pub fn frame_size(self, width: u32, height: u32) -> usize {
        let (w, h) = (width as usize, height as usize);
        match self {
            Self::Bgra | Self::Rgba => w * h * 4,
            Self::Nv12 | Self::Yuv420p => w * h * 3 / 2,
        }
    }

    fn to_tag(self) -> u8 {
        match self {
            Self::Bgra => 0,
            Self::Rgba => 1,
            Self::Nv12 => 2,
            Self::Yuv420p => 3,
        }
    }

    pub(crate) fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Self::Bgra),
            1 => Some(Self::Rgba),
            2 => Some(Self::Nv12),
            3 => Some(Self::Yuv420p),
            _ => None,
        }
    }
}

/// A rectangular region within a frame that changed since the last frame.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// A single raw video frame.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub timestamp_us: u64,
    pub frame_number: u64,
    /// Regions that changed since the previous frame (empty = full frame changed).
    pub dirty_rects: Vec<DirtyRect>,
}

/// Region of a display to capture.
#[derive(Debug, Clone)]
pub struct CaptureRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Configuration for starting a capture session.
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    pub display_id: u32,
    pub fps: u32,
    pub region: Option<CaptureRegion>,
}

/// An encoded video packet.
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    pub data: Vec<u8>,
    pub pts: i64,
    pub dts: i64,
    pub is_keyframe: bool,
    pub codec: VideoCodec,
}

/// Hardware acceleration backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HwAccel {
    /// No hardware acceleration — pure software encoding/decoding.
    None,
    /// NVIDIA NVENC/NVDEC.
    Nvenc,
    /// Intel Quick Sync Video.
    Qsv,
    /// VA-API (Linux).
    Vaapi,
    /// AMD AMF.
    Amf,
}

impl HwAccel {
    /// FFmpeg encoder name suffix for this accelerator (e.g. `"h264_nvenc"`).
    pub fn encoder_name(self, codec: VideoCodec) -> Option<&'static str> {
        match (self, codec) {
            (Self::None, _) => Option::None,
            (Self::Nvenc, VideoCodec::H264) => Some("h264_nvenc"),
            (Self::Nvenc, VideoCodec::H265) => Some("hevc_nvenc"),
            (Self::Nvenc, VideoCodec::AV1) => Some("av1_nvenc"),
            (Self::Qsv, VideoCodec::H264) => Some("h264_qsv"),
            (Self::Qsv, VideoCodec::H265) => Some("hevc_qsv"),
            (Self::Qsv, VideoCodec::AV1) => Some("av1_qsv"),
            (Self::Vaapi, VideoCodec::H264) => Some("h264_vaapi"),
            (Self::Vaapi, VideoCodec::H265) => Some("hevc_vaapi"),
            (Self::Vaapi, VideoCodec::AV1) => Some("av1_vaapi"),
            (Self::Amf, VideoCodec::H264) => Some("h264_amf"),
            (Self::Amf, VideoCodec::H265) => Some("hevc_amf"),
            _ => Option::None,
        }
    }
}

/// Rate control mode for the encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateControl {
    /// Constant Bitrate — target the configured bitrate_kbps exactly.
    Cbr,
    /// Variable Bitrate — encoder decides bitrate within bounds.
    Vbr,
    /// Constant Rate Factor — quality-based (CRF value stored in bitrate_kbps field is ignored).
    Crf { quality: u8 },
}

impl Default for RateControl {
    fn default() -> Self {
        Self::Cbr
    }
}

/// Encoder quality/speed preset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Fast,
    Medium,
    Slow,
}

impl EncoderPreset {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
        }
    }

    /// NVENC preset equivalent (P1 = fastest/lowest latency, P7 = slowest/best quality).
    pub fn nvenc_preset(self) -> &'static str {
        match self {
            Self::Ultrafast | Self::Superfast => "p1",
            Self::Veryfast => "p2",
            Self::Fast => "p3",
            Self::Medium => "p4",
            Self::Slow => "p7",
        }
    }
}

impl Default for EncoderPreset {
    fn default() -> Self {
        Self::Veryfast
    }
}

/// Configuration for a video encoder.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub preset: EncoderPreset,
    /// Preferred hardware acceleration (falls back to software if unavailable).
    pub hw_accel: HwAccel,
    /// Rate control mode.
    pub rate_control: RateControl,
    /// Maximum number of B-frames (0 for lowest latency).
    pub max_b_frames: u32,
    /// Encoder lookahead depth (0 for lowest latency).
    pub lookahead: u32,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            fps: 60,
            bitrate_kbps: 5000,
            preset: EncoderPreset::default(),
            hw_accel: HwAccel::None,
            rate_control: RateControl::Cbr,
            max_b_frames: 0,
            lookahead: 0,
        }
    }
}

impl EncoderConfig {
    /// Ultra-low-latency preset for real-time KVM use.
    pub fn low_latency(codec: VideoCodec, width: u32, height: u32, bitrate_kbps: u32) -> Self {
        Self {
            codec,
            width,
            height,
            fps: 60,
            bitrate_kbps,
            preset: EncoderPreset::Ultrafast,
            hw_accel: HwAccel::None,
            rate_control: RateControl::Cbr,
            max_b_frames: 0,
            lookahead: 0,
        }
    }
}

// Re-export raw-encoding header helpers used by encode/decode.

/// Raw-encoding header: width(4 LE) + height(4 LE) + format_tag(1) = 9 bytes.
pub(crate) const RAW_HEADER_SIZE: usize = 9;

pub(crate) fn write_raw_header(buf: &mut Vec<u8>, width: u32, height: u32, format: PixelFormat) {
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.push(format.to_tag());
}

pub(crate) fn read_raw_header(buf: &[u8]) -> Option<(u32, u32, PixelFormat)> {
    if buf.len() < RAW_HEADER_SIZE {
        return None;
    }
    let width = u32::from_le_bytes(buf[0..4].try_into().ok()?);
    let height = u32::from_le_bytes(buf[4..8].try_into().ok()?);
    let format = PixelFormat::from_tag(buf[8])?;
    Some((width, height, format))
}

/// Detect available hardware acceleration backends on this system.
///
/// When the `ffmpeg` feature is enabled, this probes FFmpeg for available
/// hardware encoders. Without it, returns an empty list.
pub fn detect_hw_accels(codec: VideoCodec) -> Vec<HwAccel> {
    #[cfg(feature = "ffmpeg")]
    {
        _detect_hw_accels_ffmpeg(codec)
    }
    #[cfg(not(feature = "ffmpeg"))]
    {
        let _ = codec;
        vec![]
    }
}

#[cfg(feature = "ffmpeg")]
fn _detect_hw_accels_ffmpeg(codec: VideoCodec) -> Vec<HwAccel> {
    use ffmpeg_next as ffmpeg;
    let _ = ffmpeg::init();

    let candidates = [HwAccel::Nvenc, HwAccel::Qsv, HwAccel::Vaapi, HwAccel::Amf];
    candidates
        .iter()
        .filter(|accel| {
            if let Some(name) = accel.encoder_name(codec) {
                ffmpeg::encoder::find_by_name(name).is_some()
            } else {
                false
            }
        })
        .copied()
        .collect()
}
