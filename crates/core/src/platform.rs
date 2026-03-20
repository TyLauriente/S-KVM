//! Platform detection and runtime backend selection.

use serde::{Deserialize, Serialize};

/// Display server / windowing system in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayServer {
    /// X11 (Xorg)
    X11,
    /// Wayland compositor
    Wayland,
    /// Windows desktop
    Windows,
    /// Could not detect
    Unknown,
}

impl std::fmt::Display for DisplayServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::X11 => write!(f, "X11"),
            Self::Wayland => write!(f, "Wayland"),
            Self::Windows => write!(f, "Windows"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detect the current display server at runtime.
pub fn detect_display_server() -> DisplayServer {
    #[cfg(target_os = "windows")]
    {
        return DisplayServer::Windows;
    }

    #[cfg(target_os = "linux")]
    {
        // Check Wayland first (preferred on modern Linux)
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            return DisplayServer::Wayland;
        }
        // Fall back to X11
        if std::env::var("DISPLAY").is_ok() {
            return DisplayServer::X11;
        }
        return DisplayServer::Unknown;
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        DisplayServer::Unknown
    }
}

/// Whether we're running under Wayland with XWayland (both WAYLAND_DISPLAY and DISPLAY set).
#[cfg(target_os = "linux")]
pub fn is_xwayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok() && std::env::var("DISPLAY").is_ok()
}

/// Get the current platform's OS type.
pub fn current_os() -> crate::OsType {
    #[cfg(target_os = "linux")]
    {
        crate::OsType::Linux
    }
    #[cfg(target_os = "windows")]
    {
        crate::OsType::Windows
    }
    #[cfg(target_os = "macos")]
    {
        crate::OsType::MacOS
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        crate::OsType::Linux // default
    }
}

/// Get the current hostname.
pub fn hostname() -> String {
    hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}
