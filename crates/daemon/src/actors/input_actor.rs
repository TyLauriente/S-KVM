//! Input actor — manages input capture, injection, and edge detection.

use s_kvm_core::protocol::InputMessage;
use s_kvm_core::{FocusState, InputEvent, PeerId};
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
    /// Sender half for injecting remote input events.
    inject_tx: mpsc::Sender<InputEvent>,
    /// Receiver half for injecting remote input events.
    inject_rx: mpsc::Receiver<InputEvent>,
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
        let (inject_tx, inject_rx) = mpsc::channel(512);
        Self {
            event_tx,
            kvm_active_rx,
            focus_rx,
            local_peer_id,
            inject_tx,
            inject_rx,
            shutdown,
        }
    }

    /// Get a sender for injecting remote input events.
    pub fn inject_sender(&self) -> mpsc::Sender<InputEvent> {
        self.inject_tx.clone()
    }

    /// Run the input actor.
    pub async fn run(mut self) -> anyhow::Result<()> {
        tracing::info!("Input actor starting");

        // Create platform-specific capture and injector backends
        let mut capture = s_kvm_input::create_capture();
        let mut injector = s_kvm_input::create_injector();

        // Initialize the injector
        if let Err(e) = injector.init().await {
            tracing::error!("Failed to initialize input injector: {}", e);
        }

        // Receiver for captured input events (set when capture starts)
        let mut capture_rx: Option<mpsc::Receiver<InputEvent>> = None;

        loop {
            tokio::select! {
                // Handle captured input events
                Some(event) = async {
                    match capture_rx.as_mut() {
                        Some(rx) => rx.recv().await,
                        None => std::future::pending().await,
                    }
                } => {
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
                Some(event) = self.inject_rx.recv() => {
                    if let Err(e) = injector.inject(event).await {
                        tracing::warn!("Input injection failed: {}", e);
                    }
                }

                // KVM state changed
                Ok(()) = self.kvm_active_rx.changed() => {
                    let active = *self.kvm_active_rx.borrow();
                    tracing::info!(active = active, "Input actor: KVM state changed");

                    if active {
                        // Start capturing input
                        match capture.start().await {
                            Ok(rx) => {
                                capture_rx = Some(rx);
                                tracing::info!("Input capture started");
                            }
                            Err(e) => {
                                tracing::error!("Failed to start input capture: {}", e);
                            }
                        }
                    } else {
                        // Stop capturing and ungrab
                        if capture.is_active() {
                            let _ = capture.ungrab().await;
                            if let Err(e) = capture.stop().await {
                                tracing::warn!("Failed to stop input capture: {}", e);
                            }
                            capture_rx = None;
                            tracing::info!("Input capture stopped");
                        }
                    }
                }

                // Focus changed — handle grab/ungrab
                Ok(()) = self.focus_rx.changed() => {
                    let focus = self.focus_rx.borrow().clone();
                    let kvm_active = *self.kvm_active_rx.borrow();

                    if kvm_active && capture.is_active() {
                        if focus.active_peer != self.local_peer_id {
                            // Focus moved to remote — grab input
                            if let Err(e) = capture.grab().await {
                                tracing::warn!("Failed to grab input: {}", e);
                            }
                        } else {
                            // Focus is local — ungrab input
                            if let Err(e) = capture.ungrab().await {
                                tracing::warn!("Failed to ungrab input: {}", e);
                            }
                        }
                    }
                }

                // Shutdown
                _ = self.shutdown.cancelled() => {
                    tracing::info!("Input actor shutting down");
                    break;
                }
            }
        }

        // Cleanup
        if capture.is_active() {
            let _ = capture.ungrab().await;
            let _ = capture.stop().await;
        }
        let _ = injector.shutdown().await;

        tracing::info!("Input actor stopped");
        Ok(())
    }
}
