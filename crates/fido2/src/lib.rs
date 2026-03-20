#![forbid(unsafe_code)]

pub mod relay;
pub mod virtual_device;

/// Errors from the FIDO2 subsystem.
#[derive(Debug, thiserror::Error)]
pub enum Fido2Error {
    #[error("invalid CTAPHID packet: {0}")]
    InvalidPacket(String),
    #[error("device error: {0}")]
    DeviceError(String),
    #[error("relay error: {0}")]
    RelayError(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("channel closed")]
    ChannelClosed,
}
