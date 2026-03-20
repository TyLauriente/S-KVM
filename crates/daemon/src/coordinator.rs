//! Central coordinator that manages all daemon subsystem actors.

use anyhow::Result;
use s_kvm_config::AppConfig;
use s_kvm_core::protocol::{ControlMessage, DataMessage, InputMessage};
use s_kvm_core::{ConnectionState, FocusState, PeerId, PeerInfo};
use tokio::sync::{broadcast, mpsc, watch};
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
                        CoordinatorEvent::IncomingInput(msg) => {
                            tracing::trace!("Incoming input event");
                            // Route to input injector
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
                        CoordinatorEvent::OutgoingInput(_msg) => {
                            tracing::trace!("Outgoing input event");
                            // Network actor handles outgoing via PeerManager
                            // TODO: Route to network layer for forwarding to focused peer
                        }
                        CoordinatorEvent::ControlReceived(peer_id, msg) => {
                            tracing::debug!(peer = %peer_id, "Control message received");
                            match msg {
                                ControlMessage::HeartbeatAck { .. } => {
                                    // Latency tracking handled by PeerManager
                                }
                                ControlMessage::ScreenEnter { display_id, x, y, .. } => {
                                    let _ = focus_tx.send(FocusState {
                                        active_peer: *peer_id,
                                        active_display: *display_id,
                                        cursor_x: *x,
                                        cursor_y: *y,
                                    });
                                }
                                ControlMessage::ScreenLeave { .. } => {
                                    let _ = focus_tx.send(FocusState {
                                        active_peer: self.config.peer_id,
                                        active_display: 0,
                                        cursor_x: 0,
                                        cursor_y: 0,
                                    });
                                }
                                _ => {}
                            }
                        }
                        CoordinatorEvent::DataReceived(peer_id, msg) => {
                            match msg {
                                DataMessage::ClipboardUpdate { .. } => {
                                    tracing::debug!(peer = %peer_id, "Clipboard update received");
                                    // Route to clipboard actor for application
                                    if *peer_id != self.config.peer_id {
                                        let _ = clipboard_incoming_tx.send(msg.clone()).await;
                                    }
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

        // Graceful shutdown — wait for actors to close
        ipc_handle.abort();
        network_handle.abort();
        input_handle.abort();
        clipboard_handle.abort();

        tracing::info!("Coordinator stopped");
        Ok(())
    }
}
