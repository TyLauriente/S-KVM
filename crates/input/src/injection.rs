//! Input injection trait and platform implementations.

use async_trait::async_trait;
use s_kvm_core::InputEvent;

/// Trait for injecting input events into the local system.
#[async_trait]
pub trait InputInjector: Send + Sync {
    /// Initialize the injector (create virtual devices, etc.).
    async fn init(&mut self) -> Result<(), InputInjectionError>;

    /// Inject a single input event.
    async fn inject(&mut self, event: InputEvent) -> Result<(), InputInjectionError>;

    /// Inject a batch of events.
    async fn inject_batch(&mut self, events: Vec<InputEvent>) -> Result<(), InputInjectionError> {
        for event in events {
            self.inject(event).await?;
        }
        Ok(())
    }

    /// Clean up (destroy virtual devices, etc.).
    async fn shutdown(&mut self) -> Result<(), InputInjectionError>;
}

#[derive(Debug, thiserror::Error)]
pub enum InputInjectionError {
    #[error("Failed to create virtual device: {0}")]
    DeviceCreation(String),
    #[error("Failed to inject event: {0}")]
    InjectionFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Not initialized")]
    NotInitialized,
    #[error("Platform not supported")]
    PlatformNotSupported,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
