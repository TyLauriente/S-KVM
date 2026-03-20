//! Network actor — manages QUIC connections, mDNS discovery, and peer lifecycle.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use s_kvm_core::protocol::ProtocolMessage;
use s_kvm_core::{FocusState, PeerId, PeerCapabilities, PeerInfo};
use s_kvm_network::discovery::{DiscoveryEvent, DiscoveryService};
use s_kvm_network::peer_manager::{PeerManager, PeerManagerEvent};
use s_kvm_network::quic::QuicTransport;
use s_kvm_network::tls;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::coordinator::CoordinatorEvent;

/// The network actor manages all networking functionality.
pub struct NetworkActor {
    /// Channel to send events to the coordinator.
    event_tx: mpsc::Sender<CoordinatorEvent>,
    /// This machine's peer ID.
    local_peer_id: PeerId,
    /// Machine hostname.
    hostname: String,
    /// Listen port for QUIC.
    listen_port: u16,
    /// Whether mDNS discovery is enabled.
    mdns_enabled: bool,
    /// TLS cert path.
    cert_path: PathBuf,
    /// TLS key path.
    key_path: PathBuf,
    /// Focus watch sender (passed to PeerManager).
    focus_tx: watch::Sender<FocusState>,
    /// Cancellation token.
    shutdown: CancellationToken,
}

impl NetworkActor {
    pub fn new(
        event_tx: mpsc::Sender<CoordinatorEvent>,
        local_peer_id: PeerId,
        hostname: String,
        listen_port: u16,
        mdns_enabled: bool,
        cert_path: PathBuf,
        key_path: PathBuf,
        focus_tx: watch::Sender<FocusState>,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            event_tx,
            local_peer_id,
            hostname,
            listen_port,
            mdns_enabled,
            cert_path,
            key_path,
            focus_tx,
            shutdown,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        tracing::info!(
            port = self.listen_port,
            mdns = self.mdns_enabled,
            "Network actor starting"
        );

        // Load or generate TLS identity
        let identity = tls::load_or_generate_identity(
            &self.cert_path,
            &self.key_path,
            &self.hostname,
        )
        .map_err(|e| anyhow::anyhow!("TLS identity error: {}", e))?;
        tracing::info!(fingerprint = %identity.fingerprint, "TLS identity ready");

        // Create QUIC transport
        let bind_addr: SocketAddr = ([0, 0, 0, 0], self.listen_port).into();
        let transport = Arc::new(
            QuicTransport::bind(bind_addr, &identity)
                .await
                .map_err(|e| anyhow::anyhow!("QUIC bind error: {}", e))?,
        );
        tracing::info!(addr = %transport.local_addr(), "QUIC endpoint ready");

        // Build local peer info
        let local_info = PeerInfo {
            id: self.local_peer_id,
            hostname: self.hostname.clone(),
            os: s_kvm_core::current_os(),
            displays: s_kvm_core::display::enumerate_displays(),
            capabilities: PeerCapabilities::default(),
        };

        // Create PeerManager event channel
        let (pm_event_tx, mut pm_event_rx) = mpsc::channel::<PeerManagerEvent>(64);
        let mut peer_manager = PeerManager::new(local_info, pm_event_tx, self.focus_tx);

        // Accept incoming connections via channel
        let (incoming_tx, mut incoming_rx) = mpsc::channel(16);
        let accept_transport = transport.clone();
        let accept_shutdown = self.shutdown.clone();
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = accept_transport.accept() => {
                        match result {
                            Ok(conn) => {
                                if incoming_tx.send(conn).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                tracing::error!("Accept error: {}", e);
                                break;
                            }
                        }
                    }
                    _ = accept_shutdown.cancelled() => break,
                }
            }
        });

        // Start mDNS discovery if enabled
        let mut mdns_rx: Option<mpsc::Receiver<DiscoveryEvent>> = None;
        let mut _discovery_service: Option<DiscoveryService> = None;
        if self.mdns_enabled {
            match DiscoveryService::new(
                self.local_peer_id,
                self.hostname.clone(),
                self.listen_port,
            ) {
                Ok(discovery) => {
                    if let Err(e) = discovery.advertise() {
                        tracing::warn!("mDNS advertise failed: {}", e);
                    }
                    match discovery.browse() {
                        Ok(rx) => {
                            mdns_rx = Some(rx);
                            tracing::info!("mDNS discovery started");
                        }
                        Err(e) => {
                            tracing::warn!("mDNS browse failed: {}", e);
                        }
                    }
                    _discovery_service = Some(discovery);
                }
                Err(e) => {
                    tracing::warn!("mDNS init failed: {}", e);
                }
            }
        }

        // Heartbeat interval
        let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(5));
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Main event loop
        loop {
            tokio::select! {
                // Accept incoming QUIC connections
                Some(conn) = incoming_rx.recv() => {
                    let remote = conn.remote_address();
                    tracing::info!(remote = %remote, "Handling incoming connection");
                    if let Err(e) = peer_manager.handle_incoming(conn).await {
                        tracing::warn!(remote = %remote, "Incoming handshake failed: {}", e);
                    }
                }

                // mDNS discovery events
                Some(event) = async {
                    match mdns_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
                    match event {
                        DiscoveryEvent::PeerDiscovered { peer_id, hostname, address, port, .. } => {
                            // Don't connect to ourselves
                            if peer_id == self.local_peer_id {
                                continue;
                            }
                            tracing::info!(
                                hostname = %hostname,
                                address = %address,
                                port = port,
                                "mDNS: discovered peer, connecting..."
                            );
                            if let Ok(addr) = format!("{}:{}", address, port).parse::<SocketAddr>() {
                                if let Err(e) = peer_manager.connect_to(&transport, addr).await {
                                    tracing::warn!("Failed to connect to discovered peer {}: {}", hostname, e);
                                }
                            }
                        }
                        DiscoveryEvent::PeerLost { hostname } => {
                            tracing::info!(hostname = %hostname, "mDNS: peer lost");
                        }
                    }
                }

                // PeerManager events → forward to coordinator
                Some(pm_event) = pm_event_rx.recv() => {
                    match pm_event {
                        PeerManagerEvent::PeerConnected(info) => {
                            let _ = self.event_tx.send(CoordinatorEvent::PeerConnected(info)).await;
                        }
                        PeerManagerEvent::PeerDisconnected(id) => {
                            let _ = self.event_tx.send(CoordinatorEvent::PeerDisconnected(id)).await;
                        }
                        PeerManagerEvent::MessageReceived(peer_id, msg) => {
                            match msg {
                                ProtocolMessage::Control(ctrl) => {
                                    let _ = self.event_tx.send(
                                        CoordinatorEvent::ControlReceived(peer_id, ctrl)
                                    ).await;
                                }
                                ProtocolMessage::Input(input) => {
                                    let _ = self.event_tx.send(
                                        CoordinatorEvent::IncomingInput(input)
                                    ).await;
                                }
                                ProtocolMessage::Data(data) => {
                                    let _ = self.event_tx.send(
                                        CoordinatorEvent::DataReceived(peer_id, data)
                                    ).await;
                                }
                            }
                        }
                        PeerManagerEvent::PeerUpdated(_) => {
                            // Latency updates, etc. — no action needed
                        }
                    }
                }

                // Send heartbeats periodically
                _ = heartbeat_interval.tick() => {
                    peer_manager.send_heartbeats().await;
                }

                // Shutdown
                _ = self.shutdown.cancelled() => {
                    tracing::info!("Network actor shutting down...");
                    break;
                }
            }
        }

        // Cleanup
        transport.close();
        if let Some(discovery) = _discovery_service {
            let _ = discovery.shutdown();
        }

        tracing::info!("Network actor stopped");
        Ok(())
    }
}
