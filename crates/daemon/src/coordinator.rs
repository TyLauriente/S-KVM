//! Central coordinator that manages all daemon subsystem actors.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use s_kvm_config::AppConfig;
use s_kvm_core::protocol::{ControlMessage, DataMessage, InputMessage};
use s_kvm_core::{ConnectionState, FocusState, PeerId, PeerInfo};
use tokio::sync::{broadcast, mpsc, watch, Mutex};
use tokio_util::sync::CancellationToken;

use crate::actors::clipboard_actor::ClipboardActor;
use crate::actors::input_actor::InputActor;
use crate::actors::network_actor::NetworkActor;
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

/// Shared daemon state accessible by both the coordinator and IPC handlers.
#[derive(Clone)]
pub struct DaemonState {
    pub peers: Arc<Mutex<Vec<PeerStatusInfo>>>,
    pub kvm_active: Arc<AtomicBool>,
    pub start_time: std::time::Instant,
}

impl DaemonState {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(Vec::new())),
            kvm_active: Arc::new(AtomicBool::new(false)),
            start_time: std::time::Instant::now(),
        }
    }
}

/// Commands sent from the coordinator to the network actor.
#[derive(Debug)]
pub enum NetworkCommand {
    /// Send input to the currently focused peer.
    SendInput(InputMessage),
    /// Connect to a peer at the given address.
    ConnectPeer { address: String, port: u16 },
    /// Disconnect a peer by ID string.
    DisconnectPeer(String),
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
        let (focus_tx, focus_rx) = watch::channel(FocusState {
            active_peer: self.config.peer_id,
            active_display: 0,
            cursor_x: 0,
            cursor_y: 0,
        });
        let (kvm_active_tx, kvm_active_rx) = watch::channel(false);

        // Broadcast channel for events that multiple actors need
        let (broadcast_tx, _) = broadcast::channel::<CoordinatorEvent>(256);

        // Channel for sending commands to the network actor
        let (network_cmd_tx, network_cmd_rx) = mpsc::channel::<NetworkCommand>(64);

        // Shared daemon state for IPC queries
        let daemon_state = DaemonState::new();

        // Start IPC server for GUI communication
        let ipc_event_tx = event_tx.clone();
        let ipc_config = self.config.clone();
        let ipc_shutdown = self.shutdown.clone();
        let ipc_kvm_active_rx = kvm_active_rx.clone();
        let ipc_daemon_state = daemon_state.clone();
        let ipc_network_cmd_tx = network_cmd_tx.clone();
        let ipc_handle = tokio::spawn(async move {
            if let Err(e) = ipc::start_ipc_server(
                ipc_config,
                ipc_event_tx,
                ipc_kvm_active_rx,
                ipc_daemon_state,
                ipc_network_cmd_tx,
                ipc_shutdown,
            )
            .await
            {
                tracing::error!("IPC server error: {}", e);
            }
        });

        // --- Resolve TLS cert/key paths ---
        let config_dir = s_kvm_config::config_dir();
        let cert_path = self
            .config
            .security
            .cert_path
            .clone()
            .unwrap_or_else(|| config_dir.join("cert.der"));
        let key_path = self
            .config
            .security
            .key_path
            .clone()
            .unwrap_or_else(|| config_dir.join("key.der"));

        // --- Spawn InputActor ---
        let input_actor = InputActor::new(
            event_tx.clone(),
            kvm_active_rx.clone(),
            focus_rx.clone(),
            self.config.peer_id,
            self.shutdown.clone(),
        );
        let inject_tx = input_actor.inject_sender();
        let input_handle = tokio::spawn(async move {
            if let Err(e) = input_actor.run().await {
                tracing::error!("Input actor error: {}", e);
            }
        });

        // --- Spawn ClipboardActor ---
        let clipboard_actor = ClipboardActor::new(
            event_tx.clone(),
            kvm_active_rx.clone(),
            self.config.peer_id,
            self.shutdown.clone(),
        );
        let clipboard_incoming_tx = clipboard_actor.incoming_sender();
        let clipboard_handle = tokio::spawn(async move {
            if let Err(e) = clipboard_actor.run().await {
                tracing::error!("Clipboard actor error: {}", e);
            }
        });

        // --- Spawn NetworkActor ---
        let network_actor = NetworkActor::new(
            event_tx.clone(),
            self.config.peer_id,
            self.config.machine_name.clone(),
            self.config.network.listen_port,
            self.config.network.mdns_enabled,
            cert_path,
            key_path,
            focus_tx.clone(),
            network_cmd_rx,
            self.shutdown.clone(),
        );
        let network_handle = tokio::spawn(async move {
            if let Err(e) = network_actor.run().await {
                tracing::error!("Network actor error: {}", e);
            }
        });

        tracing::info!("Coordinator started, all actors spawned");

        // Main event loop
        loop {
            tokio::select! {
                // Handle coordinator events
                Some(event) = event_rx.recv() => {
                    match &event {
                        CoordinatorEvent::PeerConnected(info) => {
                            tracing::info!(peer = %info.id, hostname = %info.hostname, "Peer connected");
                            {
                                let mut peers = daemon_state.peers.lock().await;
                                peers.push(PeerStatusInfo {
                                    id: info.id.to_string(),
                                    hostname: info.hostname.clone(),
                                    os: format!("{:?}", info.os),
                                    state: ConnectionState::Active,
                                    latency_ms: None,
                                });
                            }
                            let _ = broadcast_tx.send(event);
                        }
                        CoordinatorEvent::PeerDisconnected(id) => {
                            tracing::info!(peer = %id, "Peer disconnected");
                            {
                                let mut peers = daemon_state.peers.lock().await;
                                peers.retain(|p| p.id != id.to_string());
                            }
                            let _ = broadcast_tx.send(event);
                        }
                        CoordinatorEvent::FocusChanged(focus) => {
                            tracing::debug!(peer = %focus.active_peer, "Focus changed");
                            let _ = focus_tx.send(focus.clone());
                            let _ = broadcast_tx.send(event);
                        }
                        CoordinatorEvent::KvmToggled(active) => {
                            tracing::info!(active = active, "KVM toggled");
                            daemon_state.kvm_active.store(*active, Ordering::Relaxed);
                            let _ = kvm_active_tx.send(*active);
                        }
                        CoordinatorEvent::IncomingInput(msg) => {
                            tracing::trace!("Incoming input event");
                            match msg {
                                InputMessage::Event(input_event) => {
                                    let _ = inject_tx.send(input_event.clone()).await;
                                }
                                InputMessage::EventBatch(events) => {
                                    for input_event in events {
                                        let _ = inject_tx.send(input_event.clone()).await;
                                    }
                                }
                            }
                        }
                        CoordinatorEvent::OutgoingInput(msg) => {
                            tracing::trace!("Outgoing input event");
                            let _ = network_cmd_tx
                                .send(NetworkCommand::SendInput(msg.clone()))
                                .await;
                        }
                        CoordinatorEvent::ControlReceived(peer_id, msg) => {
                            tracing::debug!(peer = %peer_id, "Control message received");
                            match msg {
                                ControlMessage::HeartbeatAck { .. } => {
                                    // Latency tracking handled by PeerManager
                                }
                                ControlMessage::ScreenEnter { display_id, x, y, .. } => {
                                    let focus = FocusState {
                                        active_peer: *peer_id,
                                        active_display: *display_id,
                                        cursor_x: *x,
                                        cursor_y: *y,
                                    };
                                    let _ = focus_tx.send(focus.clone());
                                    let _ = event_tx
                                        .send(CoordinatorEvent::FocusChanged(focus))
                                        .await;
                                }
                                ControlMessage::ScreenLeave { .. } => {
                                    let focus = FocusState {
                                        active_peer: self.config.peer_id,
                                        active_display: 0,
                                        cursor_x: 0,
                                        cursor_y: 0,
                                    };
                                    let _ = focus_tx.send(focus.clone());
                                    let _ = event_tx
                                        .send(CoordinatorEvent::FocusChanged(focus))
                                        .await;
                                }
                                _ => {}
                            }
                        }
                        CoordinatorEvent::DataReceived(peer_id, msg) => {
                            match msg {
                                DataMessage::ClipboardUpdate { .. } => {
                                    tracing::debug!(peer = %peer_id, "Clipboard update received");
                                    if *peer_id != self.config.peer_id {
                                        let _ = clipboard_incoming_tx.send(msg.clone()).await;
                                    }
                                }
                                DataMessage::Fido2Request { request_id, command, .. } => {
                                    tracing::info!(peer = %peer_id, request_id, command, "FIDO2 request received (relay not yet active)");
                                }
                                DataMessage::Fido2Response { request_id, status, .. } => {
                                    tracing::info!(peer = %peer_id, request_id, status, "FIDO2 response received (relay not yet active)");
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

        // Graceful shutdown — wait for actors to close
        ipc_handle.abort();
        network_handle.abort();
        input_handle.abort();
        clipboard_handle.abort();

        tracing::info!("Coordinator stopped");
        Ok(())
    }
}
