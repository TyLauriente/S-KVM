//! Shared application state for the Tauri app.

use s_kvm_config::AppConfig;
use s_kvm_core::{ConnectionState, DisplayInfo, PeerInfo, PeerId};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Shared application state managed by Tauri.
pub struct AppState {
    /// Current application configuration.
    pub config: Mutex<AppConfig>,
    /// Whether KVM is currently active.
    pub kvm_active: Mutex<bool>,
    /// Connected peers and their status.
    pub peers: Mutex<HashMap<String, PeerState>>,
    /// Application start time.
    pub start_time: Instant,
}

/// State of a connected peer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerState {
    pub info: PeerInfo,
    pub connection_state: ConnectionState,
    pub latency_ms: Option<f64>,
    pub last_seen: u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: Mutex::new(AppConfig::default()),
            kvm_active: Mutex::new(false),
            peers: Mutex::new(HashMap::new()),
            start_time: Instant::now(),
        }
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}
