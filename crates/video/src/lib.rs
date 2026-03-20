#![forbid(unsafe_code)]

//! Video capture, encoding, and decoding for S-KVM.
//!
//! The crate defines trait abstractions ([`capture::VideoCapture`],
//! [`encode::VideoEncoder`], [`decode::VideoDecoder`]) with two backend tiers:
//!
//! * **Raw / Dummy** — always available, no system dependencies.
//! * **FFmpeg** — behind the `ffmpeg` feature flag, requires libavcodec/libavformat/libavutil.

pub mod types;
pub mod capture;
pub mod decode;
pub mod encode;

// Convenience re-exports.
pub use capture::{DummyCapture, PipeWireCapture, VideoCapture};
#[cfg(feature = "scrap-capture")]
pub use capture::ScrapCapture;
pub use decode::{RawDecoder, VideoDecoder};
pub use encode::{RawEncoder, VideoEncoder};
pub use types::*;

#[cfg(feature = "ffmpeg")]
pub use decode::SoftwareDecoder;
#[cfg(feature = "ffmpeg")]
pub use encode::SoftwareEncoder;
