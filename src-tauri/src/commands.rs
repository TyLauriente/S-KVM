//! Tauri IPC commands — bridge between frontend and Rust backend.

use crate::state::{AppState, PeerState};
use s_kvm_config::AppConfig;
use s_kvm_core::{ConnectionState, DisplayInfo, PeerId, PeerInfo, ScreenLink};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatus {
    pub id: String,
    pub hostname: String,
    pub os: String,
    pub state: ConnectionState,
    pub latency_ms: Option<f64>,
    pub displays: Vec<DisplayInfo>,
    pub capabilities: s_kvm_core::PeerCapabilities,
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
    pub links: Vec<ScreenLink>,
}

// --- Config commands ---

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    s_kvm_config::save_config(&config).map_err(|e| e.to_string())?;
    *state.config.lock().map_err(|e| e.to_string())? = config;
    tracing::info!("Configuration saved");
    Ok(())
}

// --- Peer commands ---

#[tauri::command]
pub async fn get_peers(state: State<'_, AppState>) -> Result<Vec<PeerStatus>, String> {
    let peers = state.peers.lock().map_err(|e| e.to_string())?;
    let result: Vec<PeerStatus> = peers
        .values()
        .map(|p| PeerStatus {
            id: p.info.id.to_string(),
            hostname: p.info.hostname.clone(),
            os: format!("{:?}", p.info.os),
            state: p.connection_state,
            latency_ms: p.latency_ms,
            displays: p.info.displays.clone(),
            capabilities: p.info.capabilities.clone(),
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub async fn get_connection_status(state: State<'_, AppState>) -> Result<String, String> {
    let peers = state.peers.lock().map_err(|e| e.to_string())?;
    let connected = peers
        .values()
        .filter(|p| p.connection_state == ConnectionState::Connected
            || p.connection_state == ConnectionState::Authenticated
            || p.connection_state == ConnectionState::Active)
        .count();

    if connected > 0 {
        Ok(format!("connected ({} peers)", connected))
    } else {
        Ok("disconnected".to_string())
    }
}

#[tauri::command]
pub async fn connect_peer(
    address: String,
    port: u16,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Connecting to peer at {}:{}", address, port);
    // TODO: Send connect command to daemon via IPC
    // For now, add a mock peer
    let peer_id = PeerId::new();
    let peer_info = PeerInfo {
        id: peer_id,
        hostname: address.clone(),
        os: s_kvm_core::OsType::Linux,
        displays: vec![],
        capabilities: Default::default(),
    };
    let peer_state = PeerState {
        info: peer_info,
        connection_state: ConnectionState::Connecting,
        latency_ms: None,
        last_seen: 0,
    };

    state
        .peers
        .lock()
        .map_err(|e| e.to_string())?
        .insert(peer_id.to_string(), peer_state);

    Ok(())
}

#[tauri::command]
pub async fn disconnect_peer(
    peer_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Disconnecting peer {}", peer_id);
    state
        .peers
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&peer_id);
    Ok(())
}

// --- Display commands ---

#[tauri::command]
pub async fn get_displays() -> Result<Vec<DisplayInfo>, String> {
    // Query system displays
    // For now, return the primary display info
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
pub async fn update_screen_layout(
    layout: ScreenLayoutUpdate,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Updating screen layout with {} links", layout.links.len());
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.screen_links = layout.links;
    s_kvm_config::save_config(&config).map_err(|e| e.to_string())?;
    Ok(())
}

// --- KVM control commands ---

#[tauri::command]
pub async fn start_kvm(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("Starting KVM");
    *state.kvm_active.lock().map_err(|e| e.to_string())? = true;
    let _ = app.emit("kvm-status-changed", true);
    Ok(())
}

#[tauri::command]
pub async fn stop_kvm(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("Stopping KVM");
    *state.kvm_active.lock().map_err(|e| e.to_string())? = false;
    let _ = app.emit("kvm-status-changed", false);
    Ok(())
}

#[tauri::command]
pub async fn get_kvm_status(state: State<'_, AppState>) -> Result<KvmStatus, String> {
    let active = *state.kvm_active.lock().map_err(|e| e.to_string())?;
    let peers = state.peers.lock().map_err(|e| e.to_string())?;
    let connected = peers
        .values()
        .filter(|p| p.connection_state == ConnectionState::Connected
            || p.connection_state == ConnectionState::Active)
        .count();

    Ok(KvmStatus {
        active,
        active_peer: None,
        connected_peers: connected,
        uptime_seconds: state.uptime_seconds(),
    })
}
