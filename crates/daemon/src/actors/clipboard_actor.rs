//! Clipboard actor — monitors clipboard changes and syncs with peers.

use s_kvm_core::clipboard::{ClipboardManager, ClipboardSyncMode};
use s_kvm_core::protocol::{ClipboardContentType, DataMessage};
use s_kvm_core::PeerId;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::coordinator::CoordinatorEvent;

/// The clipboard actor manages clipboard monitoring and synchronization.
pub struct ClipboardActor {
    /// Channel to send events to the coordinator.
    event_tx: mpsc::Sender<CoordinatorEvent>,
    /// Watch receiver for KVM active state.
    kvm_active_rx: watch::Receiver<bool>,
    /// This machine's peer ID.
    local_peer_id: PeerId,
    /// Receiver for incoming remote clipboard updates.
    incoming_rx: mpsc::Receiver<DataMessage>,
    /// Sender for incoming remote clipboard updates (shared with coordinator).
    incoming_tx: mpsc::Sender<DataMessage>,
    /// Cancellation token.
    shutdown: CancellationToken,
}

impl ClipboardActor {
    pub fn new(
        event_tx: mpsc::Sender<CoordinatorEvent>,
        kvm_active_rx: watch::Receiver<bool>,
        local_peer_id: PeerId,
        shutdown: CancellationToken,
    ) -> Self {
        let (incoming_tx, incoming_rx) = mpsc::channel(32);
        Self {
            event_tx,
            kvm_active_rx,
            local_peer_id,
            incoming_rx,
            incoming_tx,
            shutdown,
        }
    }

    /// Get a sender for routing incoming remote clipboard updates to this actor.
    pub fn incoming_sender(&self) -> mpsc::Sender<DataMessage> {
        self.incoming_tx.clone()
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Clipboard actor starting");

        // Create outgoing channel: ClipboardManager sends DataMessages here
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<DataMessage>(32);

        let clipboard_manager = ClipboardManager::new(
            outgoing_tx,
            10 * 1024 * 1024, // 10 MB max clipboard size
            ClipboardSyncMode::default(),
        );

        // Shutdown watch channel for ClipboardManager's monitoring
        let (monitor_shutdown_tx, monitor_shutdown_rx) = watch::channel(false);
        let mut monitoring = false;

        loop {
            tokio::select! {
                // Outgoing clipboard changes (local → remote)
                Some(msg) = outgoing_rx.recv() => {
                    let _ = self.event_tx.send(
                        CoordinatorEvent::DataReceived(self.local_peer_id, msg)
                    ).await;
                }

                // Incoming clipboard updates (remote → local)
                Some(msg) = self.incoming_rx.recv() => {
                    if let DataMessage::ClipboardUpdate { content_type, data } = msg {
                        if let Err(e) = clipboard_manager.handle_remote_update(content_type, data).await {
                            tracing::warn!("Failed to apply remote clipboard update: {}", e);
                        }
                    }
                }

                // KVM state changed
                Ok(()) = self.kvm_active_rx.changed() => {
                    let active = *self.kvm_active_rx.borrow();
                    tracing::info!(active = active, "Clipboard actor: KVM state changed");

                    if active && !monitoring {
                        if let Err(e) = clipboard_manager.start_monitoring(monitor_shutdown_rx.clone()).await {
                            tracing::warn!("Failed to start clipboard monitoring: {}", e);
                        } else {
                            monitoring = true;
                            tracing::info!("Clipboard monitoring started");
                        }
                    } else if !active && monitoring {
                        clipboard_manager.stop_monitoring().await;
                        monitoring = false;
                        tracing::info!("Clipboard monitoring stopped");
                    }
                }

                // Shutdown
                _ = self.shutdown.cancelled() => {
                    tracing::info!("Clipboard actor shutting down");
                    break;
                }
            }
        }

        // Cleanup
        let _ = monitor_shutdown_tx.send(true);
        if monitoring {
            clipboard_manager.stop_monitoring().await;
        }

        tracing::info!("Clipboard actor stopped");
        Ok(())
    }
}
