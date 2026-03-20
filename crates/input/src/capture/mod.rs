//! Input capture trait and platform implementations.

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

use async_trait::async_trait;
use s_kvm_core::InputEvent;

#[cfg(target_os = "linux")]
pub use linux::LinuxInputCapture;

#[cfg(target_os = "windows")]
pub use windows::WindowsInputCapture;

/// Trait for capturing input events from the local system.
#[async_trait]
pub trait InputCapture: Send + Sync {
    /// Start capturing input events. Returns a receiver for events.
    async fn start(&mut self) -> Result<tokio::sync::mpsc::Receiver<InputEvent>, InputCaptureError>;

    /// Stop capturing and release any grabbed devices.
    async fn stop(&mut self) -> Result<(), InputCaptureError>;

    /// Whether capture is currently active.
    fn is_active(&self) -> bool;

    /// Grab exclusive access to input devices (blocks local input).
    async fn grab(&mut self) -> Result<(), InputCaptureError>;

    /// Release exclusive access (restore local input).
    async fn ungrab(&mut self) -> Result<(), InputCaptureError>;
}

#[derive(Debug, thiserror::Error)]
pub enum InputCaptureError {
    #[error("Failed to open input device: {0}")]
    DeviceOpen(String),
    #[error("Failed to grab device: {0}")]
    GrabFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Platform not supported")]
    PlatformNotSupported,
    #[error("Already capturing")]
    AlreadyCapturing,
    #[error("Not capturing")]
    NotCapturing,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
