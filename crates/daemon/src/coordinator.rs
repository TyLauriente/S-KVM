//! Central coordinator that manages all daemon subsystem actors.

use anyhow::Result;
use s_kvm_config::AppConfig;
use s_kvm_core::protocol::{ControlMessage, DataMessage, InputMessage};
use s_kvm_core::{ConnectionState, FocusState, PeerId, PeerInfo};
use tokio::sync::{broadcast, mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::ipc;

/// Central message types routed between actors.
#[derive(Debug, Clone)]
pub enum CoordinatorEvent {
    /// A peer connected.
    PeerConnected(PeerInfo),
    /// A peer disconnected.
    PeerDisconnected(PeerId),
    /// Focus changed to a different peer.
    FocusChanged(FocusState),
    /// Input event received from network (needs injection).
    IncomingInput(InputMessage),
    /// Input event captured locally (needs forwarding).
    OutgoingInput(InputMessage),
    /// Control message received.
    ControlReceived(PeerId, ControlMessage),
    /// Data message received (clipboard, FIDO2).
    DataReceived(PeerId, DataMessage),
    /// Configuration changed.
    ConfigChanged(AppConfig),
    /// KVM toggled on/off.
    KvmToggled(bool),
}

/// IPC command from the Tauri GUI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IpcCommand {
    GetStatus,
    GetPeers,
    GetConfig,
    SaveConfig(AppConfig),
    ConnectPeer { address: String, port: u16 },
    DisconnectPeer(String),
    StartKvm,
    StopKvm,
    UpdateScreenLayout(Vec<s_kvm_core::ScreenLink>),
}

/// IPC response to the Tauri GUI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum IpcResponse {
    Status {
        active: bool,
        connected_peers: usize,
        uptime_seconds: u64,
    },
    Peers(Vec<PeerStatusInfo>),
    Config(AppConfig),
    Ok,
    Error(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PeerStatusInfo {
    pub id: String,
    pub hostname: String,
    pub os: String,
    pub state: ConnectionState,
    pub latency_ms: Option<f64>,
}

/// The coordinator manages all subsystem actors and routes messages between them.
pub struct Coordinator {
    config: AppConfig,
    shutdown: CancellationToken,
}

impl Coordinator {
    pub fn new(config: AppConfig, shutdown: CancellationToken) -> Self {
        Self { config, shutdown }
    }

    pub async fn run(self) -> Result<()> {
        // Create communication channels between actors
        let (event_tx, mut event_rx) = mpsc::channel::<CoordinatorEvent>(256);
        let (config_tx, _config_rx) = watch::channel(self.config.clone());
        let (focus_tx, _focus_rx) = watch::channel(FocusState {
            active_peer: self.config.peer_id,
            active_display: 0,
            cursor_x: 0,
            cursor_y: 0,
        });
        let (kvm_active_tx, kvm_active_rx) = watch::channel(false);

        // Broadcast channel for events that multiple actors need
        let (broadcast_tx, _) = broadcast::channel::<CoordinatorEvent>(256);

        // Start IPC server for GUI communication
        let ipc_event_tx = event_tx.clone();
        let ipc_config = self.config.clone();
        let ipc_shutdown = self.shutdown.clone();
        let ipc_kvm_active_rx = kvm_active_rx.clone();
        let ipc_handle = tokio::spawn(async move {
            if let Err(e) = ipc::start_ipc_server(
                ipc_config,
                ipc_event_tx,
                ipc_kvm_active_rx,
                ipc_shutdown,
            )
            .await
            {
                tracing::error!("IPC server error: {}", e);
            }
        });

        tracing::info!("Coordinator started, managing subsystem actors");

        // Main event loop
        loop {
            tokio::select! {
                // Handle coordinator events
                Some(event) = event_rx.recv() => {
                    match &event {
                        CoordinatorEvent::PeerConnected(info) => {
                            tracing::info!(peer = %info.id, hostname = %info.hostname, "Peer connected");
                            let _ = broadcast_tx.send(event);
                        }
                        CoordinatorEvent::PeerDisconnected(id) => {
                            tracing::info!(peer = %id, "Peer disconnected");
                            let _ = broadcast_tx.send(event);
                        }
                        CoordinatorEvent::FocusChanged(focus) => {
                            tracing::debug!(peer = %focus.active_peer, "Focus changed");
                            let _ = focus_tx.send(focus.clone());
                            let _ = broadcast_tx.send(event);
                        }
                        CoordinatorEvent::KvmToggled(active) => {
                            tracing::info!(active = active, "KVM toggled");
                            let _ = kvm_active_tx.send(*active);
                        }
                        CoordinatorEvent::IncomingInput(_msg) => {
                            tracing::trace!("Incoming input event");
                            // TODO: Route to input injector
                        }
                        CoordinatorEvent::OutgoingInput(_msg) => {
                            tracing::trace!("Outgoing input event");
                            // TODO: Route to network layer for forwarding
                        }
                        CoordinatorEvent::ControlReceived(peer_id, _msg) => {
                            tracing::debug!(peer = %peer_id, "Control message received");
                            // TODO: Handle control messages
                        }
                        CoordinatorEvent::DataReceived(peer_id, msg) => {
                            match msg {
                                DataMessage::ClipboardUpdate { .. } => {
                                    tracing::debug!(peer = %peer_id, "Clipboard update received");
                                    // TODO: Route to clipboard manager
                                }
                                DataMessage::Fido2Request { .. } => {
                                    tracing::debug!(peer = %peer_id, "FIDO2 request received");
                                    // TODO: Route to FIDO2 relay
                                }
                                DataMessage::Fido2Response { .. } => {
                                    tracing::debug!(peer = %peer_id, "FIDO2 response received");
                                    // TODO: Route to FIDO2 relay
                                }
                            }
                        }
                        CoordinatorEvent::ConfigChanged(config) => {
                            tracing::info!("Configuration updated");
                            let _ = config_tx.send(config.clone());
                        }
                    }
                }

                // Shutdown signal
                _ = self.shutdown.cancelled() => {
                    tracing::info!("Coordinator shutting down...");
                    break;
                }
            }
        }

        // Graceful shutdown — wait for IPC to close
        ipc_handle.abort();

        tracing::info!("Coordinator stopped");
        Ok(())
    }
}
