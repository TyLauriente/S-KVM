//! Actor modules for daemon subsystems.
//!
//! Each actor runs as an independent tokio task, communicating
//! with the coordinator via channels.

pub mod clipboard_actor;
pub mod input_actor;
pub mod network_actor;
