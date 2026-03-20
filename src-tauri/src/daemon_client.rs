//! IPC client for communicating with the S-KVM daemon process.
//!
//! Connects via Unix domain sockets on Linux or named pipes on Windows.
//! Protocol: JSON-line (one JSON object per line, newline delimited).

use s_kvm_config::AppConfig;
use s_kvm_core::{ConnectionState, ScreenLink};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// --- Duplicated IPC types (daemon crate is a binary, can't import) ---

/// Command sent from the GUI to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IpcCommand {
    GetStatus,
    GetPeers,
    GetConfig,
    SaveConfig(AppConfig),
    ConnectPeer { address: String, port: u16 },
    DisconnectPeer(String),
    StartKvm,
    StopKvm,
    UpdateScreenLayout(Vec<ScreenLink>),
}

/// Response from the daemon to the GUI.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Peer status information from the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerStatusInfo {
    pub id: String,
    pub hostname: String,
    pub os: String,
    pub state: ConnectionState,
    pub latency_ms: Option<f64>,
}

// --- Socket path helpers (must match daemon) ---

/// Get the IPC socket path (Unix).
#[cfg(unix)]
fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("s-kvm-daemon.sock")
}

/// Get the IPC named pipe name (Windows).
#[cfg(windows)]
fn pipe_name() -> String {
    r"\\.\pipe\s-kvm-daemon".to_string()
}

// --- Platform-specific stream wrapper ---

/// Wrapper around the platform-specific async stream for reading/writing.
enum IpcStream {
    #[cfg(unix)]
    Unix(tokio::net::UnixStream),
    #[cfg(windows)]
    NamedPipe(tokio::net::windows::named_pipe::NamedPipeClient),
}

impl tokio::io::AsyncRead for IpcStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_read(cx, buf),
            #[cfg(windows)]
            IpcStream::NamedPipe(s) => std::pin::Pin::new(s).poll_read(cx, buf),
        }
    }
}

impl tokio::io::AsyncWrite for IpcStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_write(cx, buf),
            #[cfg(windows)]
            IpcStream::NamedPipe(s) => std::pin::Pin::new(s).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_flush(cx),
            #[cfg(windows)]
            IpcStream::NamedPipe(s) => std::pin::Pin::new(s).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            #[cfg(unix)]
            IpcStream::Unix(s) => std::pin::Pin::new(s).poll_shutdown(cx),
            #[cfg(windows)]
            IpcStream::NamedPipe(s) => std::pin::Pin::new(s).poll_shutdown(cx),
        }
    }
}

// --- DaemonClient ---

/// Client for communicating with the S-KVM daemon over IPC.
pub struct DaemonClient {
    reader: BufReader<tokio::io::ReadHalf<IpcStream>>,
    writer: tokio::io::WriteHalf<IpcStream>,
}

impl DaemonClient {
    /// Connect to the daemon's IPC socket.
    pub async fn connect() -> Result<Self, std::io::Error> {
        let stream = Self::connect_platform().await?;
        let (reader, writer) = tokio::io::split(stream);
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
        })
    }

    #[cfg(unix)]
    async fn connect_platform() -> Result<IpcStream, std::io::Error> {
        let path = socket_path();
        tracing::debug!(path = %path.display(), "Connecting to daemon IPC socket");
        let stream = tokio::net::UnixStream::connect(&path).await?;
        Ok(IpcStream::Unix(stream))
    }

    #[cfg(windows)]
    async fn connect_platform() -> Result<IpcStream, std::io::Error> {
        let name = pipe_name();
        tracing::debug!(pipe = %name, "Connecting to daemon IPC named pipe");
        let client = tokio::net::windows::named_pipe::ClientOptions::new().open(&name)?;
        Ok(IpcStream::NamedPipe(client))
    }

    /// Send a command to the daemon and wait for the response.
    pub async fn send(&mut self, cmd: IpcCommand) -> Result<IpcResponse, DaemonClientError> {
        // Serialize command as JSON + newline
        let mut json = serde_json::to_string(&cmd).map_err(DaemonClientError::Serialize)?;
        json.push('\n');

        self.writer
            .write_all(json.as_bytes())
            .await
            .map_err(DaemonClientError::Io)?;
        self.writer.flush().await.map_err(DaemonClientError::Io)?;

        // Read response line
        let mut line = String::new();
        let bytes_read = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(DaemonClientError::Io)?;

        if bytes_read == 0 {
            return Err(DaemonClientError::ConnectionClosed);
        }

        let response: IpcResponse =
            serde_json::from_str(line.trim()).map_err(DaemonClientError::Deserialize)?;

        Ok(response)
    }
}

/// Errors that can occur when communicating with the daemon.
#[derive(Debug)]
pub enum DaemonClientError {
    Io(std::io::Error),
    Serialize(serde_json::Error),
    Deserialize(serde_json::Error),
    ConnectionClosed,
}

impl std::fmt::Display for DaemonClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IPC I/O error: {}", e),
            Self::Serialize(e) => write!(f, "Failed to serialize command: {}", e),
            Self::Deserialize(e) => write!(f, "Failed to deserialize response: {}", e),
            Self::ConnectionClosed => write!(f, "Daemon connection closed"),
        }
    }
}

impl std::error::Error for DaemonClientError {}
