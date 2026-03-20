//! Video decoding pipeline.

use anyhow::Result;

use crate::types::*;

/// Trait for video decoders.
pub trait VideoDecoder: Send {
    /// Decode an encoded packet, returning zero or more raw frames.
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Vec<VideoFrame>>;

    /// Flush buffered frames from the decoder.
    fn flush(&mut self) -> Result<Vec<VideoFrame>>;
}

// ---------------------------------------------------------------------------
// RawDecoder — inverse of RawEncoder, strips header and returns raw pixels
// ---------------------------------------------------------------------------

/// Decodes packets produced by [`super::encode::RawEncoder`].
pub struct RawDecoder {
    frame_count: u64,
}

impl RawDecoder {
    pub fn new() -> Self {
        Self { frame_count: 0 }
    }
}

impl Default for RawDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl VideoDecoder for RawDecoder {
    fn decode(&mut self, packet: &EncodedPacket) -> Result<Vec<VideoFrame>> {
        let (width, height, format) = read_raw_header(&packet.data)
            .ok_or_else(|| anyhow::anyhow!("invalid raw packet header"))?;

        let pixel_data = &packet.data[RAW_HEADER_SIZE..];
        let expected = format.frame_size(width, height);
        if pixel_data.len() < expected {
            anyhow::bail!(
                "raw packet too short: got {} bytes, expected {}",
                pixel_data.len(),
                expected,
            );
        }

        let frame = VideoFrame {
            data: pixel_data[..expected].to_vec(),
            width,
            height,
            format,
            timestamp_us: packet.pts as u64,
            frame_number: self.frame_count,
            dirty_rects: vec![],
        };
        self.frame_count += 1;

        Ok(vec![frame])
    }

    fn flush(&mut self) -> Result<Vec<VideoFrame>> {
        Ok(vec![])
    }
}

// ---------------------------------------------------------------------------
// SoftwareDecoder — FFmpeg-based (feature = "ffmpeg")
// ---------------------------------------------------------------------------

#[cfg(feature = "ffmpeg")]
mod ffmpeg_dec {
    use super::*;
    use ffmpeg_next as ffmpeg;
    use s_kvm_core::protocol::VideoCodec;

    /// Software decoder backed by FFmpeg.
    pub struct SoftwareDecoder {
        decoder: ffmpeg::decoder::video::Video,
        scaler: Option<ffmpeg::software::scaling::Context>,
        output_format: PixelFormat,
        frame_count: u64,
    }

    impl SoftwareDecoder {
        pub fn new(codec: VideoCodec, output_format: PixelFormat) -> Result<Self> {
            ffmpeg::init()?;

            let codec_id = video_codec_to_ffmpeg(codec);
            let dec_codec = ffmpeg::decoder::find(codec_id)
                .ok_or_else(|| anyhow::anyhow!("decoder for {:?} not found", codec))?;

            let context = ffmpeg::codec::context::Context::new_with_codec(dec_codec);
            let decoder = context.decoder().video()?;

            Ok(Self {
                decoder,
                scaler: None,
                output_format,
                frame_count: 0,
            })
        }

        fn receive_frames(&mut self) -> Result<Vec<VideoFrame>> {
            let mut frames = Vec::new();
            let mut decoded = ffmpeg::frame::Video::empty();

            while self.decoder.receive_frame(&mut decoded).is_ok() {
                // Lazily initialise scaler on first decoded frame.
                if self.scaler.is_none() {
                    let dst_fmt = pixel_format_to_ffmpeg(self.output_format);
                    self.scaler = Some(ffmpeg::software::scaling::Context::get(
                        decoded.format(),
                        decoded.width(),
                        decoded.height(),
                        dst_fmt,
                        decoded.width(),
                        decoded.height(),
                        ffmpeg::software::scaling::Flags::BILINEAR,
                    )?);
                }

                let scaler = self.scaler.as_mut().unwrap();
                let dst_fmt = pixel_format_to_ffmpeg(self.output_format);
                let mut output = ffmpeg::frame::Video::new(
                    dst_fmt,
                    decoded.width(),
                    decoded.height(),
                );
                scaler.run(&decoded, &mut output)?;

                // Collect pixel data from plane 0.
                let stride = output.stride(0);
                let row_bytes = match self.output_format {
                    PixelFormat::Bgra | PixelFormat::Rgba => decoded.width() as usize * 4,
                    _ => decoded.width() as usize,
                };
                let h = decoded.height() as usize;
                let plane = output.data(0);
                let data = if stride == row_bytes {
                    plane[..row_bytes * h].to_vec()
                } else {
                    let mut buf = Vec::with_capacity(row_bytes * h);
                    for y in 0..h {
                        buf.extend_from_slice(&plane[y * stride..y * stride + row_bytes]);
                    }
                    buf
                };

                frames.push(VideoFrame {
                    data,
                    width: decoded.width(),
                    height: decoded.height(),
                    format: self.output_format,
                    timestamp_us: decoded.pts().unwrap_or(0) as u64,
                    frame_number: self.frame_count,
                    dirty_rects: vec![],
                });
                self.frame_count += 1;
            }

            Ok(frames)
        }
    }

    impl VideoDecoder for SoftwareDecoder {
        fn decode(&mut self, packet: &EncodedPacket) -> Result<Vec<VideoFrame>> {
            let mut pkt = ffmpeg::Packet::copy(&packet.data);
            pkt.set_pts(Some(packet.pts));
            pkt.set_dts(Some(packet.dts));

            self.decoder.send_packet(&pkt)?;
            self.receive_frames()
        }

        fn flush(&mut self) -> Result<Vec<VideoFrame>> {
            self.decoder.send_eof()?;
            self.receive_frames()
        }
    }

    fn video_codec_to_ffmpeg(codec: VideoCodec) -> ffmpeg::codec::Id {
        match codec {
            VideoCodec::H264 => ffmpeg::codec::Id::H264,
            VideoCodec::H265 => ffmpeg::codec::Id::HEVC,
            VideoCodec::VP9 => ffmpeg::codec::Id::VP9,
            VideoCodec::AV1 => ffmpeg::codec::Id::AV1,
        }
    }

    fn pixel_format_to_ffmpeg(fmt: PixelFormat) -> ffmpeg::format::Pixel {
        match fmt {
            PixelFormat::Bgra => ffmpeg::format::Pixel::BGRA,
            PixelFormat::Rgba => ffmpeg::format::Pixel::RGBA,
            PixelFormat::Nv12 => ffmpeg::format::Pixel::NV12,
            PixelFormat::Yuv420p => ffmpeg::format::Pixel::YUV420P,
        }
    }
}

#[cfg(feature = "ffmpeg")]
pub use ffmpeg_dec::SoftwareDecoder;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode::{RawEncoder, VideoEncoder};
    use s_kvm_core::protocol::VideoCodec;

    #[test]
    fn raw_encode_decode_roundtrip() {
        let config = EncoderConfig {
            codec: VideoCodec::H264,
            width: 8,
            height: 8,
            ..Default::default()
        };
        let mut enc = RawEncoder::new(config);
        let mut dec = RawDecoder::new();

        let original = VideoFrame {
            data: vec![42u8; 8 * 8 * 4],
            width: 8,
            height: 8,
            format: PixelFormat::Bgra,
            timestamp_us: 1000,
            frame_number: 0,
            dirty_rects: vec![],
        };

        let packets = enc.encode(&original).unwrap();
        let frames = dec.decode(&packets[0]).unwrap();

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].width, 8);
        assert_eq!(frames[0].height, 8);
        assert_eq!(frames[0].format, PixelFormat::Bgra);
        assert_eq!(frames[0].data, original.data);
    }
}
