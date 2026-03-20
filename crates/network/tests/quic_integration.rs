//! Integration tests for QUIC transport layer.

use s_kvm_core::protocol::{ControlMessage, ProtocolMessage, PROTOCOL_VERSION};
use s_kvm_core::types::{OsType, PeerCapabilities, PeerId, PeerInfo};
use s_kvm_network::quic::QuicTransport;
use s_kvm_network::tls::generate_self_signed_cert;
use std::net::{Ipv4Addr, SocketAddr};

/// Helper: create a loopback address with OS-assigned port.
fn loopback_addr() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0)
}

/// Helper: build a minimal PeerInfo for tests.
fn test_peer_info(name: &str) -> PeerInfo {
    PeerInfo {
        id: PeerId::new(),
        hostname: name.to_string(),
        os: OsType::Linux,
        displays: vec![],
        capabilities: PeerCapabilities::default(),
    }
}

#[tokio::test]
async fn two_endpoints_connect_over_loopback() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_id = generate_self_signed_cert("server").unwrap();
    let client_id = generate_self_signed_cert("client").unwrap();

    let server = QuicTransport::bind(loopback_addr(), &server_id).await.unwrap();
    let server_addr = server.local_addr();

    let client = QuicTransport::bind(loopback_addr(), &client_id).await.unwrap();

    // Server accepts in background while client connects.
    let accept_handle = tokio::spawn(async move { server.accept().await.unwrap() });

    let client_conn = client.connect(server_addr).await.unwrap();
    let server_conn = accept_handle.await.unwrap();

    assert!(client_conn.is_connected());
    assert!(server_conn.is_connected());
}

#[tokio::test]
async fn send_receive_reliable_message() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_id = generate_self_signed_cert("server").unwrap();
    let client_id = generate_self_signed_cert("client").unwrap();

    let server = QuicTransport::bind(loopback_addr(), &server_id).await.unwrap();
    let server_addr = server.local_addr();
    let client = QuicTransport::bind(loopback_addr(), &client_id).await.unwrap();

    let accept_handle = tokio::spawn(async move { server.accept().await.unwrap() });
    let client_conn = client.connect(server_addr).await.unwrap();
    let server_conn = accept_handle.await.unwrap();

    // Server sends a Hello message.
    let hello = ProtocolMessage::Control(ControlMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        peer_info: test_peer_info("server-node"),
    });
    server_conn.send_reliable(&hello).await.unwrap();

    // Client receives it.
    let received = client_conn.recv_reliable().await.unwrap();

    match received {
        ProtocolMessage::Control(ControlMessage::Hello {
            protocol_version,
            peer_info,
        }) => {
            assert_eq!(protocol_version, PROTOCOL_VERSION);
            assert_eq!(peer_info.hostname, "server-node");
        }
        other => panic!("Expected Hello, got {:?}", other),
    }
}

#[tokio::test]
async fn send_receive_datagram() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_id = generate_self_signed_cert("server").unwrap();
    let client_id = generate_self_signed_cert("client").unwrap();

    let server = QuicTransport::bind(loopback_addr(), &server_id).await.unwrap();
    let server_addr = server.local_addr();
    let client = QuicTransport::bind(loopback_addr(), &client_id).await.unwrap();

    let accept_handle = tokio::spawn(async move { server.accept().await.unwrap() });
    let client_conn = client.connect(server_addr).await.unwrap();
    let server_conn = accept_handle.await.unwrap();

    let payload = b"mouse-move:100,200";
    client_conn.send_datagram(payload).unwrap();

    let received = server_conn.recv_datagram().await.unwrap();
    assert_eq!(received, payload);
}

#[tokio::test]
async fn multiple_messages_in_sequence() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_id = generate_self_signed_cert("server").unwrap();
    let client_id = generate_self_signed_cert("client").unwrap();

    let server = QuicTransport::bind(loopback_addr(), &server_id).await.unwrap();
    let server_addr = server.local_addr();
    let client = QuicTransport::bind(loopback_addr(), &client_id).await.unwrap();

    let accept_handle = tokio::spawn(async move { server.accept().await.unwrap() });
    let client_conn = client.connect(server_addr).await.unwrap();
    let server_conn = accept_handle.await.unwrap();

    // Send several heartbeats from client to server.
    let timestamps: Vec<u64> = vec![1000, 2000, 3000, 4000, 5000];

    for &ts in &timestamps {
        let msg = ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us: ts });
        client_conn.send_reliable(&msg).await.unwrap();
    }

    // Receive and verify order.
    for &expected_ts in &timestamps {
        let received = server_conn.recv_reliable().await.unwrap();
        match received {
            ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us }) => {
                assert_eq!(timestamp_us, expected_ts);
            }
            other => panic!("Expected Heartbeat, got {:?}", other),
        }
    }
}

#[tokio::test]
async fn connection_close_detection() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let server_id = generate_self_signed_cert("server").unwrap();
    let client_id = generate_self_signed_cert("client").unwrap();

    let server = QuicTransport::bind(loopback_addr(), &server_id).await.unwrap();
    let server_addr = server.local_addr();
    let client = QuicTransport::bind(loopback_addr(), &client_id).await.unwrap();

    let accept_handle = tokio::spawn(async move { server.accept().await.unwrap() });
    let client_conn = client.connect(server_addr).await.unwrap();
    let server_conn = accept_handle.await.unwrap();

    assert!(client_conn.is_connected());
    assert!(server_conn.is_connected());

    // Client closes its side.
    client_conn.close();

    // Server should detect the close when it tries to receive.
    let result = server_conn.recv_reliable().await;
    assert!(result.is_err());
    assert!(!client_conn.is_connected());
}
