//! End-to-end integration tests proving two S-KVM peers can connect and communicate.

use s_kvm_core::events::{InputEvent, InputEventKind, ModifierMask, MouseButton};
use s_kvm_core::protocol::*;
use s_kvm_core::types::{OsType, PeerCapabilities, PeerId, PeerInfo};
use s_kvm_network::quic::{PeerConnection, QuicTransport};
use s_kvm_network::tls::generate_self_signed_cert;
use std::net::{Ipv4Addr, SocketAddr};

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

/// Helper: set up two connected peers over loopback.
/// Returns (peer_a_conn, peer_b_conn).
async fn setup_connected_peers() -> (PeerConnection, PeerConnection) {
    let id_a = generate_self_signed_cert("peer-a").unwrap();
    let id_b = generate_self_signed_cert("peer-b").unwrap();

    let transport_b = QuicTransport::bind(loopback_addr(), &id_b).await.unwrap();
    let b_addr = transport_b.local_addr();

    let transport_a = QuicTransport::bind(loopback_addr(), &id_a).await.unwrap();

    let accept_handle = tokio::spawn(async move { transport_b.accept().await.unwrap() });
    let conn_a = transport_a.connect(b_addr).await.unwrap();
    let conn_b = accept_handle.await.unwrap();

    (conn_a, conn_b)
}

#[tokio::test]
async fn full_peer_lifecycle() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    // 1. Generate two TLS identities
    let id_a = generate_self_signed_cert("peer-a").unwrap();
    let id_b = generate_self_signed_cert("peer-b").unwrap();

    assert!(!id_a.fingerprint.is_empty());
    assert!(!id_b.fingerprint.is_empty());
    assert_ne!(id_a.fingerprint, id_b.fingerprint);

    // 2. Bind two transports on loopback
    let transport_b = QuicTransport::bind(loopback_addr(), &id_b).await.unwrap();
    let b_addr = transport_b.local_addr();
    let transport_a = QuicTransport::bind(loopback_addr(), &id_a).await.unwrap();

    // 3. peer_a connects to peer_b
    let accept_handle = tokio::spawn(async move { transport_b.accept().await.unwrap() });
    let conn_a = transport_a.connect(b_addr).await.unwrap();
    let conn_b = accept_handle.await.unwrap();

    assert!(conn_a.is_connected());
    assert!(conn_b.is_connected());

    // 4. Handshake: peer_a sends Hello, peer_b responds with Welcome
    let peer_a_info = test_peer_info("machine-a");
    let peer_b_info = test_peer_info("machine-b");

    let hello = ProtocolMessage::Control(ControlMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        peer_info: peer_a_info.clone(),
    });
    conn_a.send_reliable(&hello).await.unwrap();

    // 5. peer_b receives Hello, verifies peer info
    let received = conn_b.recv_reliable().await.unwrap();
    match &received {
        ProtocolMessage::Control(ControlMessage::Hello {
            protocol_version,
            peer_info,
        }) => {
            assert_eq!(*protocol_version, PROTOCOL_VERSION);
            assert_eq!(peer_info.hostname, "machine-a");
        }
        other => panic!("Expected Hello, got {:?}", other),
    }

    // peer_b sends Welcome back
    let welcome = ProtocolMessage::Control(ControlMessage::Welcome {
        protocol_version: PROTOCOL_VERSION,
        peer_info: peer_b_info.clone(),
    });
    conn_b.send_reliable(&welcome).await.unwrap();

    let received = conn_a.recv_reliable().await.unwrap();
    match &received {
        ProtocolMessage::Control(ControlMessage::Welcome {
            protocol_version,
            peer_info,
        }) => {
            assert_eq!(*protocol_version, PROTOCOL_VERSION);
            assert_eq!(peer_info.hostname, "machine-b");
        }
        other => panic!("Expected Welcome, got {:?}", other),
    }

    // 6. Heartbeat exchange
    let heartbeat_ts = 1234567890u64;
    let heartbeat = ProtocolMessage::Control(ControlMessage::Heartbeat {
        timestamp_us: heartbeat_ts,
    });
    conn_a.send_reliable(&heartbeat).await.unwrap();

    let received = conn_b.recv_reliable().await.unwrap();
    match &received {
        ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us }) => {
            assert_eq!(*timestamp_us, heartbeat_ts);
        }
        other => panic!("Expected Heartbeat, got {:?}", other),
    }

    // peer_b sends HeartbeatAck
    let ack = ProtocolMessage::Control(ControlMessage::HeartbeatAck {
        original_timestamp_us: heartbeat_ts,
        reply_timestamp_us: heartbeat_ts + 500,
    });
    conn_b.send_reliable(&ack).await.unwrap();

    let received = conn_a.recv_reliable().await.unwrap();
    match &received {
        ProtocolMessage::Control(ControlMessage::HeartbeatAck {
            original_timestamp_us,
            reply_timestamp_us,
        }) => {
            assert_eq!(*original_timestamp_us, heartbeat_ts);
            assert_eq!(*reply_timestamp_us, heartbeat_ts + 500);
        }
        other => panic!("Expected HeartbeatAck, got {:?}", other),
    }

    // 7. Graceful disconnect: peer_a sends Goodbye then closes
    let goodbye = ProtocolMessage::Control(ControlMessage::Goodbye {
        reason: "test complete".to_string(),
    });
    conn_a.send_reliable(&goodbye).await.unwrap();

    let received = conn_b.recv_reliable().await.unwrap();
    match &received {
        ProtocolMessage::Control(ControlMessage::Goodbye { reason }) => {
            assert_eq!(reason, "test complete");
        }
        other => panic!("Expected Goodbye, got {:?}", other),
    }

    conn_a.close();

    // 8. peer_b detects the disconnect
    let result = conn_b.recv_reliable().await;
    assert!(result.is_err());
    assert!(!conn_a.is_connected());
}

#[tokio::test]
async fn input_event_forwarding() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (conn_a, conn_b) = setup_connected_peers().await;

    // 1. peer_a sends a KeyDown event (scan code 30 = 'A')
    let key_event = InputEvent {
        timestamp_us: 100_000,
        kind: InputEventKind::KeyDown {
            scan_code: 30,
            modifiers: ModifierMask(0),
        },
    };
    let msg = ProtocolMessage::Input(InputMessage::Event(key_event));
    conn_a.send_reliable(&msg).await.unwrap();

    // 2. peer_b receives and verifies
    let received = conn_b.recv_reliable().await.unwrap();
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
        other => panic!("Expected Input::Event, got {:?}", other),
    }

    // 3. Send a KeyDown with Shift modifier
    let shifted_key = InputEvent {
        timestamp_us: 100_001,
        kind: InputEventKind::KeyDown {
            scan_code: 30,
            modifiers: ModifierMask(ModifierMask::SHIFT),
        },
    };
    let msg = ProtocolMessage::Input(InputMessage::Event(shifted_key));
    conn_a.send_reliable(&msg).await.unwrap();

    let received = conn_b.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Input(InputMessage::Event(evt)) => {
            match evt.kind {
                InputEventKind::KeyDown {
                    scan_code,
                    modifiers,
                } => {
                    assert_eq!(scan_code, 30);
                    assert!(modifiers.has(ModifierMask::SHIFT));
                }
                other => panic!("Expected KeyDown, got {:?}", other),
            }
        }
        other => panic!("Expected Input::Event, got {:?}", other),
    }

    // 4. Send a batch of mouse movement events
    let mouse_events: Vec<InputEvent> = (0..5)
        .map(|i| InputEvent {
            timestamp_us: 200_000 + i as u64,
            kind: InputEventKind::MouseMoveRelative {
                dx: i * 10,
                dy: i * 5,
            },
        })
        .collect();
    let batch_msg = ProtocolMessage::Input(InputMessage::EventBatch(mouse_events.clone()));
    conn_a.send_reliable(&batch_msg).await.unwrap();

    // 5. Verify all arrive in order
    let received = conn_b.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Input(InputMessage::EventBatch(events)) => {
            assert_eq!(events.len(), 5);
            for (i, evt) in events.iter().enumerate() {
                assert_eq!(evt.timestamp_us, 200_000 + i as u64);
                match evt.kind {
                    InputEventKind::MouseMoveRelative { dx, dy } => {
                        assert_eq!(dx, (i as i32) * 10);
                        assert_eq!(dy, (i as i32) * 5);
                    }
                    ref other => panic!("Expected MouseMoveRelative, got {:?}", other),
                }
            }
        }
        other => panic!("Expected Input::EventBatch, got {:?}", other),
    }
}

#[tokio::test]
async fn clipboard_sync() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (conn_a, conn_b) = setup_connected_peers().await;

    // peer_a sends clipboard text
    let clipboard_text = "Hello from peer A! 🖥️ Special chars: <>&\"'";
    let msg = ProtocolMessage::Data(DataMessage::ClipboardUpdate {
        content_type: ClipboardContentType::PlainText,
        data: clipboard_text.as_bytes().to_vec(),
    });
    conn_a.send_reliable(&msg).await.unwrap();

    // peer_b receives and verifies
    let received = conn_b.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Data(DataMessage::ClipboardUpdate { content_type, data }) => {
            assert!(matches!(content_type, ClipboardContentType::PlainText));
            let text = String::from_utf8(data).unwrap();
            assert_eq!(text, clipboard_text);
        }
        other => panic!("Expected ClipboardUpdate, got {:?}", other),
    }

    // Also test HTML clipboard content
    let html_content = "<b>Bold text</b>";
    let msg = ProtocolMessage::Data(DataMessage::ClipboardUpdate {
        content_type: ClipboardContentType::Html,
        data: html_content.as_bytes().to_vec(),
    });
    conn_a.send_reliable(&msg).await.unwrap();

    let received = conn_b.recv_reliable().await.unwrap();
    match received {
        ProtocolMessage::Data(DataMessage::ClipboardUpdate { content_type, data }) => {
            assert!(matches!(content_type, ClipboardContentType::Html));
            assert_eq!(String::from_utf8(data).unwrap(), html_content);
        }
        other => panic!("Expected ClipboardUpdate, got {:?}", other),
    }
}

#[tokio::test]
async fn datagram_mouse_movement() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (conn_a, conn_b) = setup_connected_peers().await;

    // peer_a sends mouse movement as unreliable datagram (bincode-serialized InputEvent)
    let mouse_event = InputEvent {
        timestamp_us: 500_000,
        kind: InputEventKind::MouseMoveRelative { dx: 42, dy: -17 },
    };
    let data = bincode::serialize(&mouse_event).unwrap();
    conn_a.send_datagram(&data).unwrap();

    // peer_b receives the datagram
    let received_data = conn_b.recv_datagram().await.unwrap();
    let received_event: InputEvent = bincode::deserialize(&received_data).unwrap();

    // Verify the mouse delta values
    assert_eq!(received_event.timestamp_us, 500_000);
    match received_event.kind {
        InputEventKind::MouseMoveRelative { dx, dy } => {
            assert_eq!(dx, 42);
            assert_eq!(dy, -17);
        }
        other => panic!("Expected MouseMoveRelative, got {:?}", other),
    }

    // Send another datagram with absolute position
    let abs_event = InputEvent {
        timestamp_us: 500_001,
        kind: InputEventKind::MouseMoveAbsolute { x: 1920, y: 1080 },
    };
    let data = bincode::serialize(&abs_event).unwrap();
    conn_a.send_datagram(&data).unwrap();

    let received_data = conn_b.recv_datagram().await.unwrap();
    let received_event: InputEvent = bincode::deserialize(&received_data).unwrap();
    match received_event.kind {
        InputEventKind::MouseMoveAbsolute { x, y } => {
            assert_eq!(x, 1920);
            assert_eq!(y, 1080);
        }
        other => panic!("Expected MouseMoveAbsolute, got {:?}", other),
    }
}

#[tokio::test]
async fn multiple_sequential_messages() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (conn_a, conn_b) = setup_connected_peers().await;

    // Build 20 different ProtocolMessages of various types
    let messages: Vec<ProtocolMessage> = (0..20u64)
        .map(|i| match i % 4 {
            0 => ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us: i * 1000 }),
            1 => ProtocolMessage::Input(InputMessage::Event(InputEvent {
                timestamp_us: i * 1000,
                kind: InputEventKind::KeyDown {
                    scan_code: i as u32,
                    modifiers: ModifierMask(0),
                },
            })),
            2 => ProtocolMessage::Data(DataMessage::ClipboardUpdate {
                content_type: ClipboardContentType::PlainText,
                data: format!("clipboard-{}", i).into_bytes(),
            }),
            3 => ProtocolMessage::Input(InputMessage::Event(InputEvent {
                timestamp_us: i * 1000,
                kind: InputEventKind::MouseButtonDown {
                    button: MouseButton::Left,
                },
            })),
            _ => unreachable!(),
        })
        .collect();

    // Send all 20 messages from peer_a
    for msg in &messages {
        conn_a.send_reliable(msg).await.unwrap();
    }

    // Receive and verify all 20 arrive in order with correct content
    for (i, original) in messages.iter().enumerate() {
        let received = conn_b.recv_reliable().await.unwrap();
        match (original, &received) {
            (
                ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us: orig_ts }),
                ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us: recv_ts }),
            ) => {
                assert_eq!(orig_ts, recv_ts, "Heartbeat mismatch at message {}", i);
            }
            (
                ProtocolMessage::Input(InputMessage::Event(orig_evt)),
                ProtocolMessage::Input(InputMessage::Event(recv_evt)),
            ) => {
                assert_eq!(
                    orig_evt.timestamp_us, recv_evt.timestamp_us,
                    "Input event timestamp mismatch at message {}",
                    i
                );
            }
            (
                ProtocolMessage::Data(DataMessage::ClipboardUpdate {
                    data: orig_data, ..
                }),
                ProtocolMessage::Data(DataMessage::ClipboardUpdate {
                    data: recv_data, ..
                }),
            ) => {
                assert_eq!(
                    orig_data, recv_data,
                    "Clipboard data mismatch at message {}",
                    i
                );
            }
            _ => {
                // Verify variant matches at minimum
                assert_eq!(
                    std::mem::discriminant(original),
                    std::mem::discriminant(&received),
                    "Message type mismatch at index {}",
                    i
                );
            }
        }
    }
}
