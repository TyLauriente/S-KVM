//! Display capture trait and implementations.

use async_trait::async_trait;
use anyhow::Result;
use s_kvm_core::types::DisplayInfo;
use tokio::sync::mpsc;

use crate::types::{CaptureConfig, PixelFormat, VideoFrame};

/// Trait for capturing video frames from a display.
#[async_trait]
pub trait VideoCapture: Send + Sync {
    /// Begin capturing frames, returning a channel receiver that yields them.
    async fn start(&mut self, config: CaptureConfig) -> Result<mpsc::Receiver<VideoFrame>>;

    /// Stop capturing.
    async fn stop(&mut self) -> Result<()>;

    /// List available displays.
    fn displays(&self) -> Vec<DisplayInfo>;
}

// ---------------------------------------------------------------------------
// DummyCapture — generates solid-color test frames
// ---------------------------------------------------------------------------

/// A capture source that produces synthetic solid-color frames.
/// Useful for testing the pipeline without a real display.
pub struct DummyCapture {
    width: u32,
    height: u32,
    task: Option<tokio::task::JoinHandle<()>>,
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl DummyCapture {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            task: None,
            stop_tx: None,
        }
    }
}

#[async_trait]
impl VideoCapture for DummyCapture {
    async fn start(&mut self, config: CaptureConfig) -> Result<mpsc::Receiver<VideoFrame>> {
        // Stop any existing capture first.
        self.stop().await?;

        let (tx, rx) = mpsc::channel(4);
        let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel::<()>();

        let width = config.region.as_ref().map_or(self.width, |r| r.width);
        let height = config.region.as_ref().map_or(self.height, |r| r.height);
        let fps = config.fps.max(1);

        let task = tokio::spawn(async move {
            let interval = std::time::Duration::from_micros(1_000_000 / u64::from(fps));
            let start = std::time::Instant::now();
            let mut frame_number: u64 = 0;

            loop {
                if stop_rx.try_recv().is_ok() {
                    break;
                }

                let hue = (frame_number % 360) as f64;
                let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);

                let pixel_count = (width * height) as usize;
                let mut data = Vec::with_capacity(pixel_count * 4);
                for _ in 0..pixel_count {
                    data.push(b);
                    data.push(g);
                    data.push(r);
                    data.push(255);
                }

                let frame = VideoFrame {
                    data,
                    width,
                    height,
                    format: PixelFormat::Bgra,
                    timestamp_us: start.elapsed().as_micros() as u64,
                    frame_number,
                    dirty_rects: vec![], // Dummy frames are always full-frame updates
                };

                if tx.send(frame).await.is_err() {
                    break;
                }

                frame_number += 1;
                tokio::time::sleep(interval).await;
            }
        });

        self.task = Some(task);
        self.stop_tx = Some(stop_tx);
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<()> {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        Ok(())
    }

    fn displays(&self) -> Vec<DisplayInfo> {
        vec![DisplayInfo {
            id: 0,
            name: "Dummy Display".into(),
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
            refresh_rate: 60.0,
            scale_factor: 1.0,
            is_primary: true,
        }]
    }
}

// ---------------------------------------------------------------------------
// PipeWireCapture — Linux Wayland screen capture (stub)
// ---------------------------------------------------------------------------

/// PipeWire-based screen capture for Wayland compositors.
///
/// This is a placeholder — the real implementation will use `pipewire-rs`
/// with the ScreenCast portal (xdg-desktop-portal) to negotiate DMA-BUF
/// frame access.
pub struct PipeWireCapture {
    _private: (),
}

impl PipeWireCapture {
    /// Create a new PipeWire capture source.
    ///
    /// In the real implementation this will connect to PipeWire and open a
    /// ScreenCast session via the xdg-desktop-portal D-Bus interface.
    pub fn new() -> Result<Self> {
        anyhow::bail!(
            "PipeWire capture is not yet implemented — use DummyCapture for testing"
        );
    }
}

#[async_trait]
impl VideoCapture for PipeWireCapture {
    async fn start(&mut self, _config: CaptureConfig) -> Result<mpsc::Receiver<VideoFrame>> {
        anyhow::bail!("PipeWire capture not implemented")
    }

    async fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    fn displays(&self) -> Vec<DisplayInfo> {
        // Real implementation: query PipeWire/portal for available monitors
        vec![]
    }
}

/// Convert HSV (h in 0..360, s/v in 0..1) to RGB bytes.
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dummy_capture_produces_frames() {
        let mut cap = DummyCapture::new(64, 64);
        let config = CaptureConfig {
            display_id: 0,
            fps: 30,
            region: None,
        };
        let mut rx = cap.start(config).await.unwrap();
        let frame = rx.recv().await.unwrap();

        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(frame.format, PixelFormat::Bgra);
        assert_eq!(frame.data.len(), 64 * 64 * 4);

        cap.stop().await.unwrap();
    }

    #[test]
    fn dummy_lists_one_display() {
        let cap = DummyCapture::new(1920, 1080);
        let displays = cap.displays();
        assert_eq!(displays.len(), 1);
        assert_eq!(displays[0].width, 1920);
    }

    #[test]
    fn pipewire_capture_returns_not_implemented() {
        assert!(PipeWireCapture::new().is_err());
    }
}
