//! Tauri IPC commands — bridge between frontend and Rust backend.
//!
//! Each command tries the daemon first via IPC, falling back to local state
//! if the daemon is not connected.

use crate::daemon_client::{IpcCommand, IpcResponse};
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

/// Try to send a command via the daemon client. Returns None if the daemon
/// is not connected or the send fails (marking the client as disconnected).
async fn try_daemon_send(
    state: &State<'_, AppState>,
    cmd: IpcCommand,
) -> Option<IpcResponse> {
    let mut guard = state.daemon_client.lock().await;
    if let Some(client) = guard.as_mut() {
        match client.send(cmd).await {
            Ok(resp) => return Some(resp),
            Err(e) => {
                tracing::warn!("Daemon communication failed, falling back to local: {}", e);
                // Mark as disconnected so reconnect task will retry
                *guard = None;
            }
        }
    }
    None
}

// --- Config commands ---

#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    if let Some(IpcResponse::Config(config)) =
        try_daemon_send(&state, IpcCommand::GetConfig).await
    {
        return Ok(config);
    }
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    if let Some(resp) =
        try_daemon_send(&state, IpcCommand::SaveConfig(config.clone())).await
    {
        return match resp {
            IpcResponse::Ok => {
                *state.config.lock().map_err(|e| e.to_string())? = config;
                Ok(())
            }
            IpcResponse::Error(e) => Err(e),
            _ => Err("Unexpected daemon response".to_string()),
        };
    }
    // Fallback: save locally
    s_kvm_config::save_config(&config).map_err(|e| e.to_string())?;
    *state.config.lock().map_err(|e| e.to_string())? = config;
    tracing::info!("Configuration saved (local fallback)");
    Ok(())
}

// --- Peer commands ---

#[tauri::command]
pub async fn get_peers(state: State<'_, AppState>) -> Result<Vec<PeerStatus>, String> {
    if let Some(IpcResponse::Peers(peers)) =
        try_daemon_send(&state, IpcCommand::GetPeers).await
    {
        return Ok(peers
            .into_iter()
            .map(|p| PeerStatus {
                id: p.id,
                hostname: p.hostname,
                os: p.os,
                state: p.state,
                latency_ms: p.latency_ms,
                displays: vec![],
                capabilities: Default::default(),
            })
            .collect());
    }
    // Fallback: local state
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
    if let Some(IpcResponse::Status { connected_peers, .. }) =
        try_daemon_send(&state, IpcCommand::GetStatus).await
    {
        return if connected_peers > 0 {
            Ok(format!("connected ({} peers)", connected_peers))
        } else {
            Ok("disconnected".to_string())
        };
    }
    // Fallback: local state
    let peers = state.peers.lock().map_err(|e| e.to_string())?;
    let connected = peers
        .values()
        .filter(|p| {
            p.connection_state == ConnectionState::Connected
                || p.connection_state == ConnectionState::Authenticated
                || p.connection_state == ConnectionState::Active
        })
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

    if let Some(resp) = try_daemon_send(
        &state,
        IpcCommand::ConnectPeer {
            address: address.clone(),
            port,
        },
    )
    .await
    {
        return match resp {
            IpcResponse::Ok => Ok(()),
            IpcResponse::Error(e) => Err(e),
            _ => Err("Unexpected daemon response".to_string()),
        };
    }

    // Fallback: add a local mock peer
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

    if let Some(resp) =
        try_daemon_send(&state, IpcCommand::DisconnectPeer(peer_id.clone())).await
    {
        return match resp {
            IpcResponse::Ok => Ok(()),
            IpcResponse::Error(e) => Err(e),
            _ => Err("Unexpected daemon response".to_string()),
        };
    }

    // Fallback: remove from local state
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
    Ok(s_kvm_core::display::enumerate_displays())
}

#[tauri::command]
pub async fn update_screen_layout(
    layout: ScreenLayoutUpdate,
    state: State<'_, AppState>,
) -> Result<(), String> {
    tracing::info!("Updating screen layout with {} links", layout.links.len());

    if let Some(resp) =
        try_daemon_send(&state, IpcCommand::UpdateScreenLayout(layout.links.clone())).await
    {
        return match resp {
            IpcResponse::Ok => {
                let mut config = state.config.lock().map_err(|e| e.to_string())?;
                config.screen_links = layout.links;
                Ok(())
            }
            IpcResponse::Error(e) => Err(e),
            _ => Err("Unexpected daemon response".to_string()),
        };
    }

    // Fallback: save locally
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

    if let Some(resp) = try_daemon_send(&state, IpcCommand::StartKvm).await {
        return match resp {
            IpcResponse::Ok => {
                *state.kvm_active.lock().map_err(|e| e.to_string())? = true;
                let _ = app.emit("kvm-status-changed", true);
                Ok(())
            }
            IpcResponse::Error(e) => Err(e),
            _ => Err("Unexpected daemon response".to_string()),
        };
    }

    // Fallback: local toggle
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

    if let Some(resp) = try_daemon_send(&state, IpcCommand::StopKvm).await {
        return match resp {
            IpcResponse::Ok => {
                *state.kvm_active.lock().map_err(|e| e.to_string())? = false;
                let _ = app.emit("kvm-status-changed", false);
                Ok(())
            }
            IpcResponse::Error(e) => Err(e),
            _ => Err("Unexpected daemon response".to_string()),
        };
    }

    // Fallback: local toggle
    *state.kvm_active.lock().map_err(|e| e.to_string())? = false;
    let _ = app.emit("kvm-status-changed", false);
    Ok(())
}

#[tauri::command]
pub async fn get_kvm_status(state: State<'_, AppState>) -> Result<KvmStatus, String> {
    if let Some(IpcResponse::Status {
        active,
        connected_peers,
        uptime_seconds,
    }) = try_daemon_send(&state, IpcCommand::GetStatus).await
    {
        return Ok(KvmStatus {
            active,
            active_peer: None,
            connected_peers,
            uptime_seconds,
        });
    }

    // Fallback: local state
    let active = *state.kvm_active.lock().map_err(|e| e.to_string())?;
    let peers = state.peers.lock().map_err(|e| e.to_string())?;
    let connected = peers
        .values()
        .filter(|p| {
            p.connection_state == ConnectionState::Connected
                || p.connection_state == ConnectionState::Active
        })
        .count();

    Ok(KvmStatus {
        active,
        active_peer: None,
        connected_peers: connected,
        uptime_seconds: state.uptime_seconds(),
    })
}
