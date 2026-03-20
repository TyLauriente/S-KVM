//! End-to-end daemon integration tests over loopback.
//!
//! Simulates two S-KVM instances using the PeerManager abstraction layer,
//! proving the full daemon pipeline: handshake, input forwarding, clipboard
//! sync, heartbeat exchange, and graceful disconnect.

use s_kvm_core::events::{InputEvent, InputEventKind, ModifierMask};
use s_kvm_core::protocol::*;
use s_kvm_core::types::{OsType, PeerCapabilities, PeerId, PeerInfo};
use s_kvm_network::peer_manager::{PeerManager, PeerManagerEvent};
use s_kvm_network::quic::{PeerConnection, QuicTransport};
use s_kvm_network::tls::generate_self_signed_cert;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::sync::mpsc;

/// Helper: loopback address with OS-assigned port.
fn loopback_addr() -> SocketAddr {
    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0)
}

/// Helper: build a test PeerInfo.
fn test_peer_info(name: &str) -> PeerInfo {
    PeerInfo {
        id: PeerId::new(),
        hostname: name.to_string(),
        os: OsType::Linux,
        displays: vec![],
        capabilities: PeerCapabilities::default(),
    }
}

/// Set up a PeerManager (server) with a raw PeerConnection (client).
///
/// The PeerManager handles the incoming handshake automatically, while
/// the raw connection does the handshake manually. This lets us test
/// PeerManager's send APIs while having a raw connection for receiving.
///
/// Returns (manager, manager_event_rx, raw_client_connection).
async fn setup_manager_with_raw_peer() -> (PeerManager, mpsc::Receiver<PeerManagerEvent>, PeerConnection)
{
    let id_a = generate_self_signed_cert("daemon-a").unwrap();
    let id_b = generate_self_signed_cert("daemon-b").unwrap();

    let transport_a = QuicTransport::bind(loopback_addr(), &id_a).await.unwrap();
    let transport_b = QuicTransport::bind(loopback_addr(), &id_b).await.unwrap();
    let addr_a = transport_a.local_addr();

    let (pm_tx, pm_rx) = mpsc::channel(64);
    let mut manager = PeerManager::new(test_peer_info("daemon-a"), pm_tx);

    // Client B connects to server A
    let accept_handle = tokio::spawn(async move { transport_a.accept().await.unwrap() });
    let raw_conn = transport_b.connect(addr_a).await.unwrap();
    let accepted_conn = accept_handle.await.unwrap();

    // Client B sends Hello (as a real S-KVM peer would)
    let hello = ProtocolMessage::Control(ControlMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        peer_info: test_peer_info("daemon-b"),
    });
    raw_conn.send_reliable(&hello).await.unwrap();

    // PeerManager A handles incoming: reads Hello, sends Welcome, stores peer
    manager.handle_incoming(accepted_conn).await.unwrap();

    // Client B receives the Welcome response
    let welcome = raw_conn.recv_reliable().await.unwrap();
    match welcome {
        ProtocolMessage::Control(ControlMessage::Welcome {
            protocol_version,
            peer_info,
        }) => {
            assert_eq!(protocol_version, PROTOCOL_VERSION);
            assert_eq!(peer_info.hostname, "daemon-a");
        }
        other => panic!("Expected Welcome, got {:?}", other),
    }

    (manager, pm_rx, raw_conn)
}

/// Set up two PeerManagers connected to each other via handshake.
///
/// manager_a.connect_to() and manager_b.handle_incoming() run concurrently
/// to avoid deadlock (connect_to waits for Welcome, handle_incoming sends it).
///
/// Returns (manager_a, event_rx_a, manager_b, event_rx_b, peer_b_id, peer_a_id).
async fn setup_two_managers() -> (
    PeerManager,
    mpsc::Receiver<PeerManagerEvent>,
    PeerManager,
    mpsc::Receiver<PeerManagerEvent>,
    PeerId,
    PeerId,
) {
    let id_a = generate_self_signed_cert("instance-a").unwrap();
    let id_b = generate_self_signed_cert("instance-b").unwrap();

    let transport_a = QuicTransport::bind(loopback_addr(), &id_a).await.unwrap();
    let transport_b = QuicTransport::bind(loopback_addr(), &id_b).await.unwrap();
    let addr_b = transport_b.local_addr();

    let (pm_a_tx, pm_a_rx) = mpsc::channel(64);
    let (pm_b_tx, pm_b_rx) = mpsc::channel(64);

    let info_a = test_peer_info("instance-a");
    let info_b = test_peer_info("instance-b");
    let peer_a_id = info_a.id;

    let mut manager_a = PeerManager::new(info_a, pm_a_tx);
    let mut manager_b = PeerManager::new(info_b, pm_b_tx);

    // Run accept+handle_incoming concurrently with connect_to to avoid deadlock:
    // connect_to sends Hello and blocks waiting for Welcome,
    // handle_incoming reads Hello and sends Welcome.
    let handle_b = tokio::spawn(async move {
        let conn = transport_b.accept().await.unwrap();
        manager_b.handle_incoming(conn).await.unwrap();
        manager_b
    });

    let peer_b_id = manager_a.connect_to(&transport_a, addr_b).await.unwrap();
    let manager_b = handle_b.await.unwrap();

    (manager_a, pm_a_rx, manager_b, pm_b_rx, peer_b_id, peer_a_id)
}

// ── Test 1: Full PeerManager handshake on both sides ─────────────────

#[tokio::test]
async fn peer_manager_handshake_both_sides() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (manager_a, mut pm_a_rx, manager_b, mut pm_b_rx, peer_b_id, _peer_a_id) =
        setup_two_managers().await;

    // Verify PeerConnected events on both sides
    let event_a = pm_a_rx.recv().await.unwrap();
    match event_a {
        PeerManagerEvent::PeerConnected(info) => {
            assert_eq!(info.hostname, "instance-b");
            assert_eq!(info.id, peer_b_id);
        }
        other => panic!("Expected PeerConnected on A, got {:?}", other),
    }

    let event_b = pm_b_rx.recv().await.unwrap();
    match event_b {
        PeerManagerEvent::PeerConnected(info) => {
            assert_eq!(info.hostname, "instance-a");
        }
        other => panic!("Expected PeerConnected on B, got {:?}", other),
    }

    // Verify both managers track one connected peer
    assert_eq!(manager_a.connected_count(), 1);
    assert_eq!(manager_b.connected_count(), 1);

    // Verify peer info
    let a_peers = manager_a.peer_infos();
    assert_eq!(a_peers.len(), 1);
    assert_eq!(a_peers[0].hostname, "instance-b");

    let b_peers = manager_b.peer_infos();
    assert_eq!(b_peers.len(), 1);
    assert_eq!(b_peers[0].hostname, "instance-a");
}

// ── Test 2: Input event forwarding through PeerManager ───────────────

#[tokio::test]
async fn daemon_input_forwarding() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (manager, mut pm_rx, raw_conn) = setup_manager_with_raw_peer().await;

    // Verify PeerConnected event was emitted
    let event = pm_rx.recv().await.unwrap();
    assert!(matches!(event, PeerManagerEvent::PeerConnected(_)));

    // PeerManager sends a KeyDown input event (scan code 30 = 'A')
    let input_msg = ProtocolMessage::Input(InputMessage::Event(InputEvent {
        timestamp_us: 100_000,
        kind: InputEventKind::KeyDown {
            scan_code: 30,
            modifiers: ModifierMask(0),
        },
    }));
    manager.send_to_focused(&input_msg).await;

    // Raw peer receives the input event
    let received = raw_conn.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Input(InputMessage::Event(evt)) => {
            assert_eq!(evt.timestamp_us, 100_000);
            match evt.kind {
                InputEventKind::KeyDown {
                    scan_code,
                    modifiers,
                } => {
                    assert_eq!(scan_code, 30);
                    assert_eq!(modifiers.0, 0);
                }
                other => panic!("Expected KeyDown, got {:?}", other),
            }
        }
        other => panic!("Expected Input event, got {:?}", other),
    }

    // Send an input batch (mouse movements)
    let batch = ProtocolMessage::Input(InputMessage::EventBatch(
        (0..3)
            .map(|i| InputEvent {
                timestamp_us: 200_000 + i as u64,
                kind: InputEventKind::MouseMoveRelative {
                    dx: i * 10,
                    dy: i * 5,
                },
            })
            .collect(),
    ));
    manager.send_to_focused(&batch).await;

    let received = raw_conn.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Input(InputMessage::EventBatch(events)) => {
            assert_eq!(events.len(), 3);
            for (i, evt) in events.iter().enumerate() {
                match evt.kind {
                    InputEventKind::MouseMoveRelative { dx, dy } => {
                        assert_eq!(dx, (i as i32) * 10);
                        assert_eq!(dy, (i as i32) * 5);
                    }
                    ref other => panic!("Expected MouseMoveRelative, got {:?}", other),
                }
            }
        }
        other => panic!("Expected EventBatch, got {:?}", other),
    }
}

// ── Test 3: Clipboard sync through PeerManager ──────────────────────

#[tokio::test]
async fn daemon_clipboard_sync() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (manager, mut pm_rx, raw_conn) = setup_manager_with_raw_peer().await;
    let _ = pm_rx.recv().await; // consume PeerConnected

    // Send clipboard update via PeerManager
    let clipboard_text = "Copied from daemon-a to daemon-b";
    let clip_msg = ProtocolMessage::Data(DataMessage::ClipboardUpdate {
        content_type: ClipboardContentType::PlainText,
        data: clipboard_text.as_bytes().to_vec(),
    });
    manager.send_to_focused(&clip_msg).await;

    // Raw peer receives clipboard data
    let received = raw_conn.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Data(DataMessage::ClipboardUpdate { content_type, data }) => {
            assert!(matches!(content_type, ClipboardContentType::PlainText));
            assert_eq!(String::from_utf8(data).unwrap(), clipboard_text);
        }
        other => panic!("Expected ClipboardUpdate, got {:?}", other),
    }
}

// ── Test 4: Heartbeat exchange through PeerManager ──────────────────

#[tokio::test]
async fn daemon_heartbeat_exchange() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (manager, mut pm_rx, raw_conn) = setup_manager_with_raw_peer().await;
    let _ = pm_rx.recv().await; // consume PeerConnected

    // PeerManager sends heartbeat to all connected peers
    manager.send_heartbeats().await;

    // Raw peer receives the heartbeat
    let received = raw_conn.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us }) => {
            // Timestamp should be a recent microsecond value
            assert!(timestamp_us > 0);
        }
        other => panic!("Expected Heartbeat, got {:?}", other),
    }
}

// ── Test 5: Graceful disconnect through PeerManager ─────────────────

#[tokio::test]
async fn daemon_peer_disconnect() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (mut manager_a, mut pm_a_rx, _manager_b, _pm_b_rx, peer_b_id, _peer_a_id) =
        setup_two_managers().await;

    // Consume PeerConnected event
    let event = pm_a_rx.recv().await.unwrap();
    assert!(matches!(event, PeerManagerEvent::PeerConnected(_)));
    assert_eq!(manager_a.connected_count(), 1);

    // Disconnect the peer
    manager_a.disconnect(&peer_b_id.to_string()).await;

    // Verify PeerDisconnected event
    let event = pm_a_rx.recv().await.unwrap();
    match event {
        PeerManagerEvent::PeerDisconnected(id) => {
            assert_eq!(id, peer_b_id);
        }
        other => panic!("Expected PeerDisconnected, got {:?}", other),
    }

    // Verify manager no longer tracks the peer
    assert_eq!(manager_a.connected_count(), 0);
    assert!(manager_a.peer_infos().is_empty());
}

// ── Test 6: Full daemon pipeline — all steps in sequence ─────────────

#[tokio::test]
async fn full_daemon_pipeline_e2e() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // --- Step 1: Start two QUIC endpoints on 127.0.0.1 ---
    let id_a = generate_self_signed_cert("kvm-desktop").unwrap();
    let id_b = generate_self_signed_cert("kvm-laptop").unwrap();

    let transport_a = QuicTransport::bind(loopback_addr(), &id_a).await.unwrap();
    let transport_b = QuicTransport::bind(loopback_addr(), &id_b).await.unwrap();
    let addr_b = transport_b.local_addr();

    let info_a = test_peer_info("kvm-desktop");
    let info_b = test_peer_info("kvm-laptop");
    let peer_a_id = info_a.id;

    let (pm_a_tx, mut pm_a_rx) = mpsc::channel(64);
    let (pm_b_tx, mut pm_b_rx) = mpsc::channel(64);

    let mut manager_a = PeerManager::new(info_a, pm_a_tx);
    let manager_b_info = info_b;
    let mut manager_b = PeerManager::new(manager_b_info, pm_b_tx);

    // --- Step 2: Manual connect (not mDNS) ---
    // Run concurrently: connect_to waits for Welcome, handle_incoming sends it
    let handle_b = tokio::spawn(async move {
        let conn = transport_b.accept().await.unwrap();
        manager_b.handle_incoming(conn).await.unwrap();
        manager_b
    });

    let peer_b_id = manager_a.connect_to(&transport_a, addr_b).await.unwrap();
    let mut manager_b = handle_b.await.unwrap();

    // --- Step 3: Verify Hello/Welcome handshake completed ---
    let event_a = pm_a_rx.recv().await.unwrap();
    match &event_a {
        PeerManagerEvent::PeerConnected(info) => {
            assert_eq!(info.hostname, "kvm-laptop");
        }
        other => panic!("Expected PeerConnected on desktop, got {:?}", other),
    }

    let event_b = pm_b_rx.recv().await.unwrap();
    match &event_b {
        PeerManagerEvent::PeerConnected(info) => {
            assert_eq!(info.hostname, "kvm-desktop");
        }
        other => panic!("Expected PeerConnected on laptop, got {:?}", other),
    }

    assert_eq!(manager_a.connected_count(), 1);
    assert_eq!(manager_b.connected_count(), 1);

    // --- Step 4 & 5: Forward input event and verify delivery ---
    // PeerManager.send_to_focused() sends reliably to all connected peers.
    // Direct receive verification is in daemon_input_forwarding test;
    // here we verify the operation completes without error on connected peers.
    let key_event = ProtocolMessage::Input(InputMessage::Event(InputEvent {
        timestamp_us: 1_000_000,
        kind: InputEventKind::KeyDown {
            scan_code: 30,
            modifiers: ModifierMask(ModifierMask::CTRL),
        },
    }));
    manager_a.send_to_focused(&key_event).await;

    // --- Step 6: Test clipboard sync ---
    let clipboard_msg = ProtocolMessage::Data(DataMessage::ClipboardUpdate {
        content_type: ClipboardContentType::PlainText,
        data: b"shared clipboard text".to_vec(),
    });
    manager_a.send_to_focused(&clipboard_msg).await;

    // --- Step 7: Test heartbeat exchange ---
    manager_a.send_heartbeats().await;
    manager_b.send_heartbeats().await;

    // Verify both sides still connected after all operations
    assert_eq!(manager_a.connected_count(), 1);
    assert_eq!(manager_b.connected_count(), 1);

    // Graceful disconnect
    manager_a.disconnect(&peer_b_id.to_string()).await;
    let dc_event = pm_a_rx.recv().await.unwrap();
    match dc_event {
        PeerManagerEvent::PeerDisconnected(id) => assert_eq!(id, peer_b_id),
        other => panic!("Expected PeerDisconnected, got {:?}", other),
    }
    assert_eq!(manager_a.connected_count(), 0);

    // Manager B disconnects its side too
    manager_b.disconnect(&peer_a_id.to_string()).await;
    let dc_event_b = pm_b_rx.recv().await.unwrap();
    match dc_event_b {
        PeerManagerEvent::PeerDisconnected(id) => assert_eq!(id, peer_a_id),
        other => panic!("Expected PeerDisconnected, got {:?}", other),
    }
    assert_eq!(manager_b.connected_count(), 0);
}
