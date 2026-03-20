//! QUIC transport layer using Quinn.
//!
//! Provides a single QUIC endpoint that acts as both client and server,
//! with multi-stream architecture for different data types.

use quinn::{ClientConfig, Endpoint, ServerConfig, Connection, SendStream};
use s_kvm_core::protocol::{ProtocolMessage, serialize_message, deserialize_message};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::tls::TlsIdentity;

/// Stream IDs for the multi-stream architecture.
pub const STREAM_CONTROL: u8 = 0;
pub const STREAM_INPUT: u8 = 1;
pub const STREAM_DATA: u8 = 2;

/// QUIC transport managing a single endpoint.
pub struct QuicTransport {
    endpoint: Endpoint,
    local_addr: SocketAddr,
}

/// A connection to a remote peer with multiplexed streams.
pub struct PeerConnection {
    connection: Connection,
    /// Send stream for control messages (Stream 0).
    _control_tx: Option<SendStream>,
    /// Send stream for input events (Stream 1).
    _input_tx: Option<SendStream>,
    /// Send stream for data (clipboard, FIDO2) (Stream 2).
    _data_tx: Option<SendStream>,
}

impl QuicTransport {
    /// Create a new QUIC transport bound to the given address.
    pub async fn bind(
        addr: SocketAddr,
        identity: &TlsIdentity,
    ) -> Result<Self, QuicError> {
        let server_config = make_server_config(identity)?;
        let client_config = make_client_config(identity)?;

        let mut endpoint = Endpoint::server(server_config, addr)
            .map_err(|e| QuicError::Bind(e.to_string()))?;

        endpoint.set_default_client_config(client_config);

        let local_addr = endpoint.local_addr()
            .map_err(|e| QuicError::Bind(e.to_string()))?;

        tracing::info!(addr = %local_addr, "QUIC endpoint bound");

        Ok(Self {
            endpoint,
            local_addr,
        })
    }

    /// Get the local address this endpoint is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Connect to a remote peer.
    pub async fn connect(&self, addr: SocketAddr) -> Result<PeerConnection, QuicError> {
        let connection = self
            .endpoint
            .connect(addr, "s-kvm")
            .map_err(|e| QuicError::Connect(e.to_string()))?
            .await
            .map_err(|e| QuicError::Connect(e.to_string()))?;

        tracing::info!(
            remote = %connection.remote_address(),
            "Connected to peer"
        );

        Ok(PeerConnection {
            connection,
            _control_tx: None,
            _input_tx: None,
            _data_tx: None,
        })
    }

    /// Accept an incoming connection.
    pub async fn accept(&self) -> Result<PeerConnection, QuicError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| QuicError::Accept("Endpoint closed".to_string()))?;

        let connection = incoming
            .await
            .map_err(|e| QuicError::Accept(e.to_string()))?;

        tracing::info!(
            remote = %connection.remote_address(),
            "Accepted peer connection"
        );

        Ok(PeerConnection {
            connection,
            _control_tx: None,
            _input_tx: None,
            _data_tx: None,
        })
    }

    /// Accept connections in a loop, sending new connections to a channel.
    pub async fn accept_loop(
        &self,
        tx: mpsc::Sender<PeerConnection>,
    ) -> Result<(), QuicError> {
        loop {
            match self.accept().await {
                Ok(conn) => {
                    if tx.send(conn).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                    // Continue accepting unless endpoint is closed
                    if matches!(e, QuicError::Accept(_)) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// Close the endpoint.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }
}

impl PeerConnection {
    /// Remote address of this connection.
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Open a unidirectional stream and send a protocol message.
    pub async fn send_reliable(&self, msg: &ProtocolMessage) -> Result<(), QuicError> {
        let mut stream = self
            .connection
            .open_uni()
            .await
            .map_err(|e| QuicError::Stream(e.to_string()))?;

        let data = serialize_message(msg)
            .map_err(|e| QuicError::Serialization(e.to_string()))?;

        // Write length prefix (4 bytes) + data
        let len = (data.len() as u32).to_be_bytes();
        stream
            .write_all(&len)
            .await
            .map_err(|e| QuicError::Write(e.to_string()))?;
        stream
            .write_all(&data)
            .await
            .map_err(|e| QuicError::Write(e.to_string()))?;
        stream
            .finish()
            .map_err(|e| QuicError::Write(e.to_string()))?;

        Ok(())
    }

    /// Accept a unidirectional stream and read a protocol message.
    pub async fn recv_reliable(&self) -> Result<ProtocolMessage, QuicError> {
        let mut stream = self
            .connection
            .accept_uni()
            .await
            .map_err(|e| QuicError::Stream(e.to_string()))?;

        // Read length prefix
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| QuicError::Read(e.to_string()))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > s_kvm_core::protocol::MAX_MESSAGE_SIZE {
            return Err(QuicError::Read(format!(
                "Message too large: {} bytes",
                len
            )));
        }

        // Read data
        let mut data = vec![0u8; len];
        stream
            .read_exact(&mut data)
            .await
            .map_err(|e| QuicError::Read(e.to_string()))?;

        deserialize_message(&data)
            .map_err(|e| QuicError::Serialization(e.to_string()))
    }

    /// Send an unreliable datagram (for mouse movement, audio).
    pub fn send_datagram(&self, data: &[u8]) -> Result<(), QuicError> {
        self.connection
            .send_datagram(data.to_vec().into())
            .map_err(|e| QuicError::Datagram(e.to_string()))
    }

    /// Receive an unreliable datagram.
    pub async fn recv_datagram(&self) -> Result<Vec<u8>, QuicError> {
        let bytes = self
            .connection
            .read_datagram()
            .await
            .map_err(|e| QuicError::Datagram(e.to_string()))?;
        Ok(bytes.to_vec())
    }

    /// Check if the connection is still alive.
    pub fn is_connected(&self) -> bool {
        self.connection.close_reason().is_none()
    }

    /// Close the connection gracefully.
    pub fn close(&self) {
        self.connection.close(0u32.into(), b"goodbye");
    }

    /// Get connection statistics.
    pub fn stats(&self) -> quinn::ConnectionStats {
        self.connection.stats()
    }
}

/// Create a quinn::ServerConfig from a TLS identity.
fn make_server_config(identity: &TlsIdentity) -> Result<ServerConfig, QuicError> {
    let rustls_config = crate::tls::make_server_config(identity)
        .map_err(|e| QuicError::Tls(e.to_string()))?;

    let mut config = ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(rustls_config)
            .map_err(|e| QuicError::Tls(e.to_string()))?,
    ));

    // Enable datagrams for unreliable transport (mouse, audio)
    let transport = Arc::get_mut(&mut config.transport).unwrap();
    transport.max_concurrent_uni_streams(100u32.into());
    transport.max_concurrent_bidi_streams(10u32.into());
    transport.datagram_receive_buffer_size(Some(65536));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(5)));

    Ok(config)
}

/// Create a quinn::ClientConfig from a TLS identity.
fn make_client_config(identity: &TlsIdentity) -> Result<ClientConfig, QuicError> {
    let rustls_config = crate::tls::make_client_config(identity)
        .map_err(|e| QuicError::Tls(e.to_string()))?;

    let mut config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)
            .map_err(|e| QuicError::Tls(e.to_string()))?,
    ));

    let mut transport = quinn::TransportConfig::default();
    transport.max_concurrent_uni_streams(100u32.into());
    transport.max_concurrent_bidi_streams(10u32.into());
    transport.datagram_receive_buffer_size(Some(65536));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
    config.transport_config(Arc::new(transport));

    Ok(config)
}

#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    #[error("Failed to bind: {0}")]
    Bind(String),
    #[error("Failed to connect: {0}")]
    Connect(String),
    #[error("Failed to accept: {0}")]
    Accept(String),
    #[error("Stream error: {0}")]
    Stream(String),
    #[error("Write error: {0}")]
    Write(String),
    #[error("Read error: {0}")]
    Read(String),
    #[error("Datagram error: {0}")]
    Datagram(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("TLS error: {0}")]
    Tls(String),
}
