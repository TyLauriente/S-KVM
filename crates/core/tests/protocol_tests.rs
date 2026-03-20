//! Protocol message serialization tests.

use s_kvm_core::events::*;
use s_kvm_core::protocol::*;
use s_kvm_core::types::*;

#[test]
fn serialize_deserialize_input_event() {
    let event = InputEvent::new(InputEventKind::KeyDown {
        scan_code: 30, // 'A' key
        modifiers: ModifierMask(ModifierMask::SHIFT),
    });

    let msg = ProtocolMessage::Input(InputMessage::Event(event.clone()));
    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Input(InputMessage::Event(e)) => {
            assert!(matches!(e.kind, InputEventKind::KeyDown { scan_code: 30, .. }));
        }
        _ => panic!("Expected Input message"),
    }
}

#[test]
fn serialize_deserialize_mouse_move() {
    let event = InputEvent::new(InputEventKind::MouseMoveRelative { dx: -42, dy: 100 });

    let msg = ProtocolMessage::Input(InputMessage::Event(event));
    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Input(InputMessage::Event(e)) => {
            match e.kind {
                InputEventKind::MouseMoveRelative { dx, dy } => {
                    assert_eq!(dx, -42);
                    assert_eq!(dy, 100);
                }
                _ => panic!("Expected MouseMoveRelative"),
            }
        }
        _ => panic!("Expected Input message"),
    }
}

#[test]
fn serialize_deserialize_hello() {
    let peer_info = PeerInfo {
        id: PeerId::new(),
        hostname: "test-machine".to_string(),
        os: OsType::Linux,
        displays: vec![DisplayInfo {
            id: 0,
            name: "Primary".to_string(),
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
            refresh_rate: 144.0,
            scale_factor: 1.0,
            is_primary: true,
        }],
        capabilities: PeerCapabilities::default(),
    };

    let msg = ProtocolMessage::Control(ControlMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        peer_info: peer_info.clone(),
    });

    let bytes = serialize_message(&msg).unwrap();
    assert!(bytes.len() < MAX_MESSAGE_SIZE);

    let decoded = deserialize_message(&bytes).unwrap();
    match decoded {
        ProtocolMessage::Control(ControlMessage::Hello {
            protocol_version,
            peer_info: decoded_info,
        }) => {
            assert_eq!(protocol_version, PROTOCOL_VERSION);
            assert_eq!(decoded_info.hostname, "test-machine");
            assert_eq!(decoded_info.displays.len(), 1);
            assert_eq!(decoded_info.displays[0].width, 1920);
        }
        _ => panic!("Expected Hello message"),
    }
}

#[test]
fn serialize_deserialize_clipboard_update() {
    let data = "Hello, clipboard!".as_bytes().to_vec();
    let msg = ProtocolMessage::Data(DataMessage::ClipboardUpdate {
        content_type: ClipboardContentType::PlainText,
        data: data.clone(),
    });

    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Data(DataMessage::ClipboardUpdate {
            content_type,
            data: decoded_data,
        }) => {
            assert!(matches!(content_type, ClipboardContentType::PlainText));
            assert_eq!(decoded_data, data);
        }
        _ => panic!("Expected ClipboardUpdate"),
    }
}

#[test]
fn serialize_deserialize_heartbeat() {
    let msg = ProtocolMessage::Control(ControlMessage::Heartbeat {
        timestamp_us: 1234567890,
    });

    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us }) => {
            assert_eq!(timestamp_us, 1234567890);
        }
        _ => panic!("Expected Heartbeat"),
    }
}

#[test]
fn serialize_deserialize_screen_enter() {
    let msg = ProtocolMessage::Control(ControlMessage::ScreenEnter {
        display_id: 1,
        x: 100,
        y: 200,
        modifiers: ModifierMask(ModifierMask::CTRL | ModifierMask::ALT),
    });

    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Control(ControlMessage::ScreenEnter {
            display_id,
            x,
            y,
            modifiers,
        }) => {
            assert_eq!(display_id, 1);
            assert_eq!(x, 100);
            assert_eq!(y, 200);
            assert!(modifiers.has(ModifierMask::CTRL));
            assert!(modifiers.has(ModifierMask::ALT));
            assert!(!modifiers.has(ModifierMask::SHIFT));
        }
        _ => panic!("Expected ScreenEnter"),
    }
}

#[test]
fn event_batch_roundtrip() {
    let events = vec![
        InputEvent::new(InputEventKind::KeyDown {
            scan_code: 30,
            modifiers: ModifierMask::default(),
        }),
        InputEvent::new(InputEventKind::KeyUp {
            scan_code: 30,
            modifiers: ModifierMask::default(),
        }),
        InputEvent::new(InputEventKind::MouseMoveRelative { dx: 10, dy: -5 }),
    ];

    let msg = ProtocolMessage::Input(InputMessage::EventBatch(events));
    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Input(InputMessage::EventBatch(batch)) => {
            assert_eq!(batch.len(), 3);
        }
        _ => panic!("Expected EventBatch"),
    }
}

#[test]
fn fido2_request_roundtrip() {
    let msg = ProtocolMessage::Data(DataMessage::Fido2Request {
        request_id: 42,
        command: 0x02, // authenticatorGetAssertion
        payload: vec![0xA1, 0x01, 0x68, 0x65, 0x78, 0x61, 0x6D, 0x70, 0x6C, 0x65],
    });

    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Data(DataMessage::Fido2Request {
            request_id,
            command,
            payload,
        }) => {
            assert_eq!(request_id, 42);
            assert_eq!(command, 0x02);
            assert_eq!(payload.len(), 10);
        }
        _ => panic!("Expected Fido2Request"),
    }
}

#[test]
fn modifier_mask_operations() {
    let mut m = ModifierMask::default();
    assert_eq!(m.0, 0);
    assert!(!m.has(ModifierMask::SHIFT));

    m.set(ModifierMask::SHIFT);
    assert!(m.has(ModifierMask::SHIFT));
    assert!(!m.has(ModifierMask::CTRL));

    m.set(ModifierMask::CTRL);
    assert!(m.has(ModifierMask::SHIFT));
    assert!(m.has(ModifierMask::CTRL));

    m.clear(ModifierMask::SHIFT);
    assert!(!m.has(ModifierMask::SHIFT));
    assert!(m.has(ModifierMask::CTRL));
}

#[test]
fn input_event_message_is_compact() {
    // Per design doc: target 8-32 bytes per event
    let event = InputEvent::new(InputEventKind::MouseMoveRelative { dx: 10, dy: -5 });
    let msg = ProtocolMessage::Input(InputMessage::Event(event));
    let bytes = serialize_message(&msg).unwrap();

    // The full ProtocolMessage wrapper adds overhead, but the core event should be compact
    // A bincode-serialized mouse move should be well under 64 bytes
    assert!(bytes.len() < 64, "Mouse move message was {} bytes", bytes.len());
}

#[test]
fn video_stream_messages() {
    let msg = ProtocolMessage::Control(ControlMessage::StartVideoStream {
        display_id: 0,
        preferred_codec: VideoCodec::H264,
        max_fps: 60,
        max_bitrate_kbps: 20000,
    });

    let bytes = serialize_message(&msg).unwrap();
    let decoded = deserialize_message(&bytes).unwrap();

    match decoded {
        ProtocolMessage::Control(ControlMessage::StartVideoStream {
            display_id,
            preferred_codec,
            max_fps,
            max_bitrate_kbps,
        }) => {
            assert_eq!(display_id, 0);
            assert_eq!(preferred_codec, VideoCodec::H264);
            assert_eq!(max_fps, 60);
            assert_eq!(max_bitrate_kbps, 20000);
        }
        _ => panic!("Expected StartVideoStream"),
    }
}
