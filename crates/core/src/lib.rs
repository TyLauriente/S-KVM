// Windows display enumeration (EnumDisplayMonitors) requires unsafe FFI.
#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]

pub mod clipboard;
pub mod display;
pub mod events;
pub mod platform;
pub mod protocol;
pub mod types;

pub use events::*;
pub use platform::{DisplayServer, detect_display_server, current_os};
pub use types::*;
