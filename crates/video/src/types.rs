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

/// A single raw video frame.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub timestamp_us: u64,
    pub frame_number: u64,
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
