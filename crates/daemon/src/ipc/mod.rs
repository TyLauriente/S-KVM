//! IPC server for communication between the Tauri GUI and the daemon.
//!
//! Uses Unix domain sockets on Linux and named pipes on Windows.

use anyhow::Result;
use s_kvm_config::AppConfig;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

use crate::coordinator::{CoordinatorEvent, IpcCommand, IpcResponse};

/// Get the IPC socket path (Unix only).
#[cfg(unix)]
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("s-kvm-daemon.sock")
}

/// Get the IPC named pipe name (Windows only).
#[cfg(windows)]
pub fn pipe_name() -> String {
    r"\\.\pipe\s-kvm-daemon".to_string()
}

/// Start the IPC server (platform dispatcher).
pub async fn start_ipc_server(
    config: AppConfig,
    event_tx: mpsc::Sender<CoordinatorEvent>,
    kvm_active_rx: watch::Receiver<bool>,
    shutdown: CancellationToken,
) -> Result<()> {
    #[cfg(unix)]
    {
        start_ipc_server_unix(config, event_tx, kvm_active_rx, shutdown).await
    }
    #[cfg(windows)]
    {
        start_ipc_server_windows(config, event_tx, kvm_active_rx, shutdown).await
    }
}

/// Unix IPC server using Unix domain sockets.
#[cfg(unix)]
async fn start_ipc_server_unix(
    config: AppConfig,
    event_tx: mpsc::Sender<CoordinatorEvent>,
    kvm_active_rx: watch::Receiver<bool>,
    shutdown: CancellationToken,
) -> Result<()> {
    let path = socket_path();

    // Remove stale socket file if it exists
    if path.exists() {
        std::fs::remove_file(&path)?;
    }

    tracing::info!(path = %path.display(), "Starting IPC server");

    let listener = tokio::net::UnixListener::bind(&path)?;

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let config = config.clone();
                        let event_tx = event_tx.clone();
                        let kvm_active_rx = kvm_active_rx.clone();
                        let (reader, writer) = stream.into_split();
                        tokio::spawn(async move {
                            if let Err(e) = handle_ipc_connection(
                                reader, writer, config, event_tx, kvm_active_rx,
                            ).await {
                                tracing::error!("IPC connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("IPC accept error: {}", e);
                    }
                }
            }
            _ = shutdown.cancelled() => {
                tracing::info!("IPC server shutting down");
                break;
            }
        }
    }

    // Cleanup socket file
    let _ = std::fs::remove_file(&path);

    Ok(())
}

/// Windows IPC server using named pipes.
#[cfg(windows)]
async fn start_ipc_server_windows(
    config: AppConfig,
    event_tx: mpsc::Sender<CoordinatorEvent>,
    kvm_active_rx: watch::Receiver<bool>,
    shutdown: CancellationToken,
) -> Result<()> {
    use tokio::net::windows::named_pipe::ServerOptions;

    let name = pipe_name();
    tracing::info!(pipe = %name, "Starting IPC server");

    // Create the first pipe instance
    let mut server = ServerOptions::new(&name).create()?;

    loop {
        tokio::select! {
            result = server.connect() => {
                match result {
                    Ok(()) => {
                        // Hand off the connected pipe and create a new one for the next client
                        let connected = server;
                        server = ServerOptions::new(&name).create()?;

                        let config = config.clone();
                        let event_tx = event_tx.clone();
                        let kvm_active_rx = kvm_active_rx.clone();
                        let (reader, writer) = tokio::io::split(connected);
                        tokio::spawn(async move {
                            if let Err(e) = handle_ipc_connection(
                                reader, writer, config, event_tx, kvm_active_rx,
                            ).await {
                                tracing::error!("IPC connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("IPC accept error: {}", e);
                    }
                }
            }
            _ = shutdown.cancelled() => {
                tracing::info!("IPC server shutting down");
                break;
            }
        }
    }

    Ok(())
}

/// Handle a single IPC connection. Works with any async reader/writer pair.
async fn handle_ipc_connection<R, W>(
    reader: R,
    mut writer: W,
    config: AppConfig,
    event_tx: mpsc::Sender<CoordinatorEvent>,
    kvm_active_rx: watch::Receiver<bool>,
) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            break; // Connection closed
        }

        let command: IpcCommand = match serde_json::from_str(line.trim()) {
            Ok(cmd) => cmd,
            Err(e) => {
                let response = IpcResponse::Error(format!("Invalid command: {}", e));
                let json = serde_json::to_string(&response)? + "\n";
                writer.write_all(json.as_bytes()).await?;
                continue;
            }
        };

        let response = match command {
            IpcCommand::GetStatus => {
                let active = *kvm_active_rx.borrow();
                IpcResponse::Status {
                    active,
                    connected_peers: 0, // TODO: query from peer manager
                    uptime_seconds: 0,  // TODO: track uptime
                }
            }
            IpcCommand::GetConfig => IpcResponse::Config(config.clone()),
            IpcCommand::SaveConfig(new_config) => {
                let save_result =
                    s_kvm_config::save_config(&new_config).map_err(|e| e.to_string());
                match save_result {
                    Ok(()) => {
                        let _ = event_tx
                            .send(CoordinatorEvent::ConfigChanged(new_config))
                            .await;
                        IpcResponse::Ok
                    }
                    Err(e) => IpcResponse::Error(format!("Failed to save config: {}", e)),
                }
            }
            IpcCommand::StartKvm => {
                let _ = event_tx.send(CoordinatorEvent::KvmToggled(true)).await;
                IpcResponse::Ok
            }
            IpcCommand::StopKvm => {
                let _ = event_tx.send(CoordinatorEvent::KvmToggled(false)).await;
                IpcResponse::Ok
            }
            IpcCommand::GetPeers => {
                // TODO: Query actual peers from peer manager
                IpcResponse::Peers(vec![])
            }
            IpcCommand::ConnectPeer { address, port } => {
                tracing::info!("IPC: Connect to peer {}:{}", address, port);
                // TODO: Initiate connection via network actor
                IpcResponse::Ok
            }
            IpcCommand::DisconnectPeer(peer_id) => {
                tracing::info!("IPC: Disconnect peer {}", peer_id);
                // TODO: Disconnect via network actor
                IpcResponse::Ok
            }
            IpcCommand::UpdateScreenLayout(links) => {
                tracing::info!("IPC: Update screen layout with {} links", links.len());
                // TODO: Update screen layout
                IpcResponse::Ok
            }
        };

        let json = serde_json::to_string(&response)? + "\n";
        writer.write_all(json.as_bytes()).await?;
    }

    Ok(())
}
