//! Application configuration and settings.

use s_kvm_core::{PeerId, ScreenLink};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// This machine's peer ID (generated once, persisted).
    pub peer_id: PeerId,
    /// Human-readable machine name.
    pub machine_name: String,
    /// Network settings.
    pub network: NetworkConfig,
    /// Screen layout links.
    pub screen_links: Vec<ScreenLink>,
    /// Input settings.
    pub input: InputConfig,
    /// Video streaming settings.
    pub video: VideoConfig,
    /// Audio settings.
    pub audio: AudioConfig,
    /// Security settings.
    pub security: SecurityConfig,
    /// Hotkey bindings.
    pub hotkeys: HotkeyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Port for QUIC listener.
    pub listen_port: u16,
    /// Enable mDNS discovery.
    pub mdns_enabled: bool,
    /// Service type for mDNS.
    pub mdns_service_type: String,
    /// Manually configured peers.
    pub static_peers: Vec<StaticPeer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticPeer {
    pub address: String,
    pub port: u16,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputConfig {
    /// Whether to forward mouse acceleration or raw deltas.
    pub raw_mouse_deltas: bool,
    /// Edge switch delay in milliseconds (debounce).
    pub edge_switch_delay_ms: u32,
    /// Dead zone in pixels at screen edges.
    pub edge_dead_zone: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    /// Enable display streaming.
    pub enabled: bool,
    /// Maximum FPS.
    pub max_fps: u32,
    /// Target bitrate in kbps.
    pub target_bitrate_kbps: u32,
    /// Preferred codec.
    pub codec: s_kvm_core::protocol::VideoCodec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Enable audio sharing.
    pub enabled: bool,
    /// Opus bitrate in kbps.
    pub bitrate_kbps: u32,
    /// Frame size in ms (5, 10, or 20).
    pub frame_size_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Path to TLS certificate.
    pub cert_path: Option<PathBuf>,
    /// Path to TLS private key.
    pub key_path: Option<PathBuf>,
    /// Trusted peer fingerprints (TOFU).
    pub trusted_fingerprints: Vec<TrustedFingerprint>,
    /// Require pairing for new connections.
    pub require_pairing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedFingerprint {
    pub peer_id: PeerId,
    pub fingerprint: String,
    pub first_seen: String,
    pub hostname: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Toggle KVM active/inactive.
    pub toggle_active: String,
    /// Switch to specific screen (by index).
    pub switch_screen: Vec<String>,
    /// Lock to current screen.
    pub lock_screen: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            peer_id: PeerId::new(),
            machine_name: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            network: NetworkConfig {
                listen_port: 24800,
                mdns_enabled: true,
                mdns_service_type: "_softkvm._tcp.local.".to_string(),
                static_peers: vec![],
            },
            screen_links: vec![],
            input: InputConfig {
                raw_mouse_deltas: true,
                edge_switch_delay_ms: 50,
                edge_dead_zone: 2,
            },
            video: VideoConfig {
                enabled: false,
                max_fps: 60,
                target_bitrate_kbps: 20_000,
                codec: s_kvm_core::protocol::VideoCodec::H264,
            },
            audio: AudioConfig {
                enabled: false,
                bitrate_kbps: 128,
                frame_size_ms: 10,
            },
            security: SecurityConfig {
                cert_path: None,
                key_path: None,
                trusted_fingerprints: vec![],
                require_pairing: true,
            },
            hotkeys: HotkeyConfig {
                toggle_active: "Ctrl+Alt+K".to_string(),
                switch_screen: vec![
                    "Ctrl+Alt+1".to_string(),
                    "Ctrl+Alt+2".to_string(),
                    "Ctrl+Alt+3".to_string(),
                    "Ctrl+Alt+4".to_string(),
                ],
                lock_screen: "Ctrl+Alt+L".to_string(),
            },
        }
    }
}

/// Load config from the standard location.
pub fn config_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "skvm", "S-KVM")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Load configuration from disk, creating defaults if not found.
pub fn load_config() -> Result<AppConfig, Box<dyn std::error::Error>> {
    let config_path = config_dir().join("config.toml");
    if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        let config: AppConfig = toml::from_str(&content)?;
        Ok(config)
    } else {
        let config = AppConfig::default();
        save_config(&config)?;
        Ok(config)
    }
}

/// Save configuration to disk.
pub fn save_config(config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = config_dir().join("config.toml");
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&config_path, content)?;
    Ok(())
}
