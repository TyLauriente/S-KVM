//! Peer connection lifecycle management.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Instant;

use s_kvm_core::protocol::*;
use s_kvm_core::*;
use tokio::sync::{mpsc, watch};

use crate::quic::{PeerConnection, QuicTransport};

/// State of a managed peer.
pub struct ManagedPeer {
    pub info: PeerInfo,
    pub connection: PeerConnection,
    pub state: ConnectionState,
    pub last_heartbeat: Instant,
    pub latency_ms: Option<f64>,
}

/// Events from the peer manager.
#[derive(Debug, Clone)]
pub enum PeerManagerEvent {
    PeerConnected(PeerInfo),
    PeerDisconnected(PeerId),
    PeerUpdated(PeerId),
    MessageReceived(PeerId, ProtocolMessage),
}

/// Manages all peer connections and their lifecycle.
pub struct PeerManager {
    local_info: PeerInfo,
    peers: HashMap<String, ManagedPeer>,
    event_tx: mpsc::Sender<PeerManagerEvent>,
    focus_tx: watch::Sender<FocusState>,
}

impl PeerManager {
    pub fn new(
        local_info: PeerInfo,
        event_tx: mpsc::Sender<PeerManagerEvent>,
        focus_tx: watch::Sender<FocusState>,
    ) -> Self {
        Self {
            local_info,
            peers: HashMap::new(),
            event_tx,
            focus_tx,
        }
    }

    /// Handle a new incoming connection.
    pub async fn handle_incoming(&mut self, conn: PeerConnection) -> Result<(), String> {
        tracing::info!(
            remote = %conn.remote_address(),
            "Handling incoming peer connection"
        );

        // Expect Hello message
        let msg = conn.recv_reliable().await.map_err(|e| e.to_string())?;

        if let ProtocolMessage::Control(ControlMessage::Hello {
            protocol_version,
            peer_info,
        }) = msg
        {
            if protocol_version != PROTOCOL_VERSION {
                tracing::warn!(
                    "Protocol version mismatch: {} vs {}",
                    protocol_version,
                    PROTOCOL_VERSION
                );
            }

            // Send Welcome
            let welcome = ProtocolMessage::Control(ControlMessage::Welcome {
                protocol_version: PROTOCOL_VERSION,
                peer_info: self.local_info.clone(),
            });
            conn.send_reliable(&welcome)
                .await
                .map_err(|e| e.to_string())?;

            let peer_id = peer_info.id.to_string();
            tracing::info!(
                peer = %peer_info.hostname,
                id = %peer_id,
                "Peer handshake complete"
            );

            let _ = self
                .event_tx
                .send(PeerManagerEvent::PeerConnected(peer_info.clone()))
                .await;

            self.peers.insert(
                peer_id,
                ManagedPeer {
                    info: peer_info,
                    connection: conn,
                    state: ConnectionState::Authenticated,
                    last_heartbeat: Instant::now(),
                    latency_ms: None,
                },
            );

            Ok(())
        } else {
            Err("Expected Hello message".to_string())
        }
    }

    /// Connect to a remote peer.
    pub async fn connect_to(
        &mut self,
        transport: &QuicTransport,
        addr: SocketAddr,
    ) -> Result<PeerId, String> {
        let conn = transport.connect(addr).await.map_err(|e| e.to_string())?;

        // Send Hello
        let hello = ProtocolMessage::Control(ControlMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            peer_info: self.local_info.clone(),
        });
        conn.send_reliable(&hello)
            .await
            .map_err(|e| e.to_string())?;

        // Expect Welcome
        let msg = conn.recv_reliable().await.map_err(|e| e.to_string())?;

        if let ProtocolMessage::Control(ControlMessage::Welcome {
            protocol_version: _,
            peer_info,
        }) = msg
        {
            let peer_id = peer_info.id;
            let peer_id_str = peer_id.to_string();

            tracing::info!(
                peer = %peer_info.hostname,
                id = %peer_id_str,
                "Connected to peer"
            );

            let _ = self
                .event_tx
                .send(PeerManagerEvent::PeerConnected(peer_info.clone()))
                .await;

            self.peers.insert(
                peer_id_str,
                ManagedPeer {
                    info: peer_info,
                    connection: conn,
                    state: ConnectionState::Authenticated,
                    last_heartbeat: Instant::now(),
                    latency_ms: None,
                },
            );

            Ok(peer_id)
        } else {
            Err("Expected Welcome message".to_string())
        }
    }

    /// Disconnect a peer by ID.
    pub async fn disconnect(&mut self, peer_id: &str) {
        if let Some(peer) = self.peers.remove(peer_id) {
            peer.connection.close();
            let _ = self
                .event_tx
                .send(PeerManagerEvent::PeerDisconnected(peer.info.id))
                .await;
            tracing::info!(peer = %peer.info.hostname, "Peer disconnected");
        }
    }

    /// Send a heartbeat to all connected peers.
    pub async fn send_heartbeats(&self) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;

        let msg = ProtocolMessage::Control(ControlMessage::Heartbeat {
            timestamp_us: timestamp,
        });

        for peer in self.peers.values() {
            if peer.connection.is_connected() {
                if let Err(e) = peer.connection.send_reliable(&msg).await {
                    tracing::debug!(
                        peer = %peer.info.hostname,
                        "Heartbeat failed: {}",
                        e
                    );
                }
            }
        }
    }

    /// Get the number of connected peers.
    pub fn connected_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.connection.is_connected())
            .count()
    }

    /// Get info about all peers.
    pub fn peer_infos(&self) -> Vec<&PeerInfo> {
        self.peers.values().map(|p| &p.info).collect()
    }
}
