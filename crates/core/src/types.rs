//! Core types used across all S-KVM subsystems.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a peer in the KVM network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub Uuid);

impl PeerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PeerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for PeerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Information about a peer machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: PeerId,
    pub hostname: String,
    pub os: OsType,
    pub displays: Vec<DisplayInfo>,
    pub capabilities: PeerCapabilities,
}

/// Operating system type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsType {
    Linux,
    Windows,
    MacOS,
}

/// Information about a display/monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub id: u32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub refresh_rate: f64,
    pub scale_factor: f64,
    pub is_primary: bool,
}

/// What a peer supports.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub input_forwarding: bool,
    pub display_streaming: bool,
    pub audio_sharing: bool,
    pub clipboard_sharing: bool,
    pub fido2_forwarding: bool,
}

impl Default for PeerCapabilities {
    fn default() -> Self {
        Self {
            input_forwarding: true,
            display_streaming: false,
            audio_sharing: false,
            clipboard_sharing: true,
            fido2_forwarding: false,
        }
    }
}

/// Screen edge for cursor transition detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScreenEdge {
    Top,
    Bottom,
    Left,
    Right,
}

/// Screen layout link — maps an edge of one screen to another peer's screen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenLink {
    pub source_display: u32,
    pub source_edge: ScreenEdge,
    pub target_peer: PeerId,
    pub target_display: u32,
    /// Offset along the edge (for partial-edge mappings).
    pub offset: i32,
}

/// Connection state of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Authenticated,
    Active,
}

/// Which peer currently has keyboard/mouse focus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FocusState {
    pub active_peer: PeerId,
    pub active_display: u32,
    pub cursor_x: i32,
    pub cursor_y: i32,
}
