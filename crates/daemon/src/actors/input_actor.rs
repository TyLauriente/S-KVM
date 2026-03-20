//! Input actor — manages input capture, injection, and edge detection.

use s_kvm_core::{InputEvent, FocusState, PeerId};
use s_kvm_core::protocol::InputMessage;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::coordinator::CoordinatorEvent;

/// The input actor manages input capture and injection.
pub struct InputActor {
    /// Channel to send events to the coordinator.
    event_tx: mpsc::Sender<CoordinatorEvent>,
    /// Watch receiver for KVM active state.
    kvm_active_rx: watch::Receiver<bool>,
    /// Watch receiver for focus state.
    focus_rx: watch::Receiver<FocusState>,
    /// This machine's peer ID.
    local_peer_id: PeerId,
    /// Cancellation token.
    shutdown: CancellationToken,
}

impl InputActor {
    pub fn new(
        event_tx: mpsc::Sender<CoordinatorEvent>,
        kvm_active_rx: watch::Receiver<bool>,
        focus_rx: watch::Receiver<FocusState>,
        local_peer_id: PeerId,
        shutdown: CancellationToken,
    ) -> Self {
        Self {
            event_tx,
            kvm_active_rx,
            focus_rx,
            local_peer_id,
            shutdown,
        }
    }

    /// Run the input actor.
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Input actor starting");

        // Channel for receiving input events from capture
        let (_input_tx, mut input_rx) = mpsc::channel::<InputEvent>(512);

        // Channel for receiving events to inject
        let (_inject_tx, mut inject_rx) = mpsc::channel::<InputEvent>(512);

        loop {
            tokio::select! {
                // Handle captured input events
                Some(event) = input_rx.recv() => {
                    let focus = self.focus_rx.borrow().clone();
                    let kvm_active = *self.kvm_active_rx.borrow();

                    if kvm_active && focus.active_peer != self.local_peer_id {
                        // Forward to remote peer
                        let msg = InputMessage::Event(event);
                        let _ = self.event_tx.send(CoordinatorEvent::OutgoingInput(msg)).await;
                    }
                    // If focus is local, events pass through normally (not grabbed)
                }

                // Handle events to inject (from remote peer)
                Some(_event) = inject_rx.recv() => {
                    // TODO: Inject via InputInjector
                    tracing::trace!("Injecting input event");
                }

                // KVM state changed
                Ok(()) = self.kvm_active_rx.changed() => {
                    let active = *self.kvm_active_rx.borrow();
                    tracing::info!(active = active, "Input actor: KVM state changed");
                    // TODO: Start/stop input capture and grab based on state
                }

                // Shutdown
                _ = self.shutdown.cancelled() => {
                    tracing::info!("Input actor shutting down");
                    break;
                }
            }
        }

        // Cleanup
        tracing::info!("Input actor stopped");
        Ok(())
    }

    /// Get a sender for injecting remote input events.
    pub fn inject_sender(&self) -> mpsc::Sender<InputEvent> {
        // This would be created during setup and shared with the network actor
        // For now, create a dummy channel
        let (tx, _) = mpsc::channel(512);
        tx
    }
}
