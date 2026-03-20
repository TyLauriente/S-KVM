//! Tauri IPC commands — bridge between frontend and Rust backend.

use s_kvm_config::AppConfig;
use s_kvm_core::{ConnectionState, DisplayInfo, PeerInfo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatus {
    pub info: PeerInfo,
    pub state: ConnectionState,
    pub latency_ms: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KvmStatus {
    pub active: bool,
    pub active_peer: Option<String>,
    pub connected_peers: usize,
    pub uptime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenLayoutUpdate {
    pub links: Vec<s_kvm_core::ScreenLink>,
}

// --- Config commands ---

#[tauri::command]
pub async fn get_config() -> Result<AppConfig, String> {
    s_kvm_config::load_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_config(config: AppConfig) -> Result<(), String> {
    s_kvm_config::save_config(&config).map_err(|e| e.to_string())
}

// --- Peer commands ---

#[tauri::command]
pub async fn get_peers() -> Result<Vec<PeerStatus>, String> {
    // TODO: Query daemon for connected peers
    Ok(vec![])
}

#[tauri::command]
pub async fn get_connection_status() -> Result<String, String> {
    // TODO: Query daemon for overall connection status
    Ok("disconnected".to_string())
}

#[tauri::command]
pub async fn connect_peer(address: String, port: u16) -> Result<(), String> {
    tracing::info!("Connecting to peer at {}:{}", address, port);
    // TODO: Initiate QUIC connection via daemon
    Ok(())
}

#[tauri::command]
pub async fn disconnect_peer(peer_id: String) -> Result<(), String> {
    tracing::info!("Disconnecting peer {}", peer_id);
    // TODO: Disconnect via daemon
    Ok(())
}

// --- Display commands ---

#[tauri::command]
pub async fn get_displays() -> Result<Vec<DisplayInfo>, String> {
    // TODO: Query system for display information
    // Return mock data for now
    Ok(vec![DisplayInfo {
        id: 0,
        name: "Primary Display".to_string(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        refresh_rate: 60.0,
        scale_factor: 1.0,
        is_primary: true,
    }])
}

#[tauri::command]
pub async fn update_screen_layout(layout: ScreenLayoutUpdate) -> Result<(), String> {
    tracing::info!("Updating screen layout with {} links", layout.links.len());
    // TODO: Update config and notify daemon
    Ok(())
}

// --- KVM control commands ---

#[tauri::command]
pub async fn start_kvm() -> Result<(), String> {
    tracing::info!("Starting KVM");
    // TODO: Start input capture and forwarding
    Ok(())
}

#[tauri::command]
pub async fn stop_kvm() -> Result<(), String> {
    tracing::info!("Stopping KVM");
    // TODO: Stop input capture and forwarding
    Ok(())
}

#[tauri::command]
pub async fn get_kvm_status() -> Result<KvmStatus, String> {
    Ok(KvmStatus {
        active: false,
        active_peer: None,
        connected_peers: 0,
        uptime_seconds: 0,
    })
}
