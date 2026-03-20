//! Clipboard sharing between peers.

use crate::protocol::{ClipboardContentType, DataMessage};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

/// Clipboard manager handles monitoring and setting clipboard content.
pub struct ClipboardManager {
    /// Channel to send clipboard updates to the network layer.
    outgoing_tx: mpsc::Sender<DataMessage>,
    /// Whether clipboard monitoring is active.
    active: Arc<Mutex<bool>>,
    /// Last known clipboard content hash (to detect changes).
    last_hash: Arc<Mutex<u64>>,
    /// Maximum clipboard data size in bytes.
    max_size: usize,
    /// Sync mode.
    sync_mode: ClipboardSyncMode,
}

/// When to sync clipboard between peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ClipboardSyncMode {
    /// Sync clipboard on every change (real-time).
    OnChange,
    /// Sync clipboard only when focus changes between peers.
    OnFocusChange,
    /// Clipboard sharing disabled.
    Disabled,
}

impl Default for ClipboardSyncMode {
    fn default() -> Self {
        Self::OnFocusChange
    }
}

impl ClipboardManager {
    pub fn new(
        outgoing_tx: mpsc::Sender<DataMessage>,
        max_size: usize,
        sync_mode: ClipboardSyncMode,
    ) -> Self {
        Self {
            outgoing_tx,
            active: Arc::new(Mutex::new(false)),
            last_hash: Arc::new(Mutex::new(0)),
            max_size,
            sync_mode,
        }
    }

    /// Start monitoring the clipboard for changes.
    pub async fn start_monitoring(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<(), ClipboardError> {
        let active = self.active.clone();
        let last_hash = self.last_hash.clone();
        let outgoing_tx = self.outgoing_tx.clone();
        let max_size = self.max_size;

        *active.lock().await = true;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if !*active.lock().await {
                            break;
                        }

                        // Read clipboard in blocking task (arboard is not async)
                        let result = tokio::task::spawn_blocking(|| {
                            let clipboard = arboard::Clipboard::new();
                            match clipboard {
                                Ok(mut cb) => {
                                    // Try text first
                                    if let Ok(text) = cb.get_text() {
                                        if !text.is_empty() {
                                            let hash = simple_hash(&text);
                                            return Some((ClipboardContentType::PlainText, text.into_bytes(), hash));
                                        }
                                    }
                                    None
                                }
                                Err(e) => {
                                    tracing::debug!("Clipboard access error: {}", e);
                                    None
                                }
                            }
                        }).await;

                        if let Ok(Some((content_type, data, hash))) = result {
                            if data.len() > max_size {
                                tracing::debug!("Clipboard data too large ({} bytes), skipping", data.len());
                                continue;
                            }

                            let mut last = last_hash.lock().await;
                            if hash != *last {
                                *last = hash;
                                let msg = DataMessage::ClipboardUpdate {
                                    content_type,
                                    data,
                                };
                                if outgoing_tx.send(msg).await.is_err() {
                                    tracing::debug!("Clipboard outgoing channel closed");
                                    break;
                                }
                                tracing::debug!("Clipboard change detected, sent update");
                            }
                        }
                    }
                    _ = shutdown.changed() => {
                        break;
                    }
                }
            }

            tracing::debug!("Clipboard monitoring stopped");
        });

        Ok(())
    }

    /// Stop clipboard monitoring.
    pub async fn stop_monitoring(&self) {
        *self.active.lock().await = false;
    }

    /// Handle an incoming clipboard update from a remote peer.
    pub async fn handle_remote_update(
        &self,
        content_type: ClipboardContentType,
        data: Vec<u8>,
    ) -> Result<(), ClipboardError> {
        if data.len() > self.max_size {
            return Err(ClipboardError::TooLarge(data.len(), self.max_size));
        }

        match content_type {
            ClipboardContentType::PlainText => {
                let text = String::from_utf8(data)
                    .map_err(|e| ClipboardError::InvalidData(e.to_string()))?;

                // Update the hash so we don't re-send this back
                let hash = simple_hash(&text);
                *self.last_hash.lock().await = hash;

                // Set clipboard in blocking task
                tokio::task::spawn_blocking(move || {
                    let clipboard = arboard::Clipboard::new();
                    match clipboard {
                        Ok(mut cb) => {
                            if let Err(e) = cb.set_text(&text) {
                                tracing::warn!("Failed to set clipboard: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to access clipboard: {}", e);
                        }
                    }
                })
                .await
                .map_err(|e| ClipboardError::Internal(e.to_string()))?;

                tracing::debug!("Applied remote clipboard update (text)");
            }
            ClipboardContentType::Html => {
                let html = String::from_utf8(data)
                    .map_err(|e| ClipboardError::InvalidData(e.to_string()))?;

                *self.last_hash.lock().await = simple_hash(&html);

                tokio::task::spawn_blocking(move || {
                    let clipboard = arboard::Clipboard::new();
                    match clipboard {
                        Ok(mut cb) => {
                            // arboard supports HTML via set_html
                            if let Err(e) = cb.set_html(&html, None) {
                                tracing::warn!("Failed to set HTML clipboard: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to access clipboard: {}", e);
                        }
                    }
                })
                .await
                .map_err(|e| ClipboardError::Internal(e.to_string()))?;

                tracing::debug!("Applied remote clipboard update (HTML)");
            }
            _ => {
                tracing::debug!("Unsupported clipboard content type: {:?}", content_type);
            }
        }

        Ok(())
    }

    /// Get the current sync mode.
    pub fn sync_mode(&self) -> ClipboardSyncMode {
        self.sync_mode
    }

    /// Trigger a one-time clipboard sync (used for OnFocusChange mode).
    pub async fn sync_now(&self) -> Result<(), ClipboardError> {
        let outgoing_tx = self.outgoing_tx.clone();
        let max_size = self.max_size;
        let last_hash = self.last_hash.clone();

        let result = tokio::task::spawn_blocking(|| {
            let clipboard = arboard::Clipboard::new();
            match clipboard {
                Ok(mut cb) => {
                    if let Ok(text) = cb.get_text() {
                        if !text.is_empty() {
                            let hash = simple_hash(&text);
                            return Some((ClipboardContentType::PlainText, text.into_bytes(), hash));
                        }
                    }
                    None
                }
                Err(_) => None,
            }
        })
        .await
        .map_err(|e| ClipboardError::Internal(e.to_string()))?;

        if let Some((content_type, data, hash)) = result {
            if data.len() > max_size {
                return Err(ClipboardError::TooLarge(data.len(), max_size));
            }

            let mut last = last_hash.lock().await;
            if hash != *last {
                *last = hash;
                let msg = DataMessage::ClipboardUpdate {
                    content_type,
                    data,
                };
                outgoing_tx
                    .send(msg)
                    .await
                    .map_err(|_| ClipboardError::ChannelClosed)?;
            }
        }

        Ok(())
    }
}

/// Simple FNV-1a hash for change detection.
fn simple_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    #[error("Clipboard data too large: {0} bytes (max {1})")]
    TooLarge(usize, usize),
    #[error("Invalid clipboard data: {0}")]
    InvalidData(String),
    #[error("Clipboard channel closed")]
    ChannelClosed,
    #[error("Internal error: {0}")]
    Internal(String),
}
