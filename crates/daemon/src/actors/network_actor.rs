//! Network actor — manages QUIC connections, mDNS discovery, and peer lifecycle.

use s_kvm_core::PeerId;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::coordinator::CoordinatorEvent;

/// The network actor manages all networking functionality.
pub struct NetworkActor {
    /// Channel to send events to the coordinator.
    event_tx: mpsc::Sender<CoordinatorEvent>,
    /// This machine's peer ID.
    local_peer_id: PeerId,
    /// Listen port for QUIC.
    listen_port: u16,
    /// Whether mDNS discovery is enabled.
    mdns_enabled: bool,
    /// Cancellation token.
    shutdown: CancellationToken,
}

impl NetworkActor {
    pub fn new(
        event_tx: mpsc::Sender<CoordinatorEvent>,
        local_peer_id: PeerId,
        listen_port: u16,
        mdns_enabled: bool,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            event_tx,
            local_peer_id,
            listen_port,
            mdns_enabled,
            shutdown,
        }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        tracing::info!(
            port = self.listen_port,
            mdns = self.mdns_enabled,
            "Network actor starting"
        );

        // TODO: Initialize TLS certificates
        // TODO: Create QUIC endpoint
        // TODO: Start mDNS discovery if enabled
        // TODO: Accept incoming connections
        // TODO: Handle connection lifecycle

        // For now, just wait for shutdown
        self.shutdown.cancelled().await;

        tracing::info!("Network actor stopped");
        Ok(())
    }
}
