//! Property-based tests using proptest.

use proptest::prelude::*;
use s_kvm_core::events::*;
use s_kvm_core::protocol::*;
use s_kvm_core::types::*;

// --- Arbitrary strategies ---

fn arb_scan_code() -> impl Strategy<Value = u32> {
    1u32..256
}

fn arb_modifier_mask() -> impl Strategy<Value = ModifierMask> {
    (0u16..128).prop_map(ModifierMask)
}

fn arb_mouse_button() -> impl Strategy<Value = MouseButton> {
    prop_oneof![
        Just(MouseButton::Left),
        Just(MouseButton::Right),
        Just(MouseButton::Middle),
        Just(MouseButton::Back),
        Just(MouseButton::Forward),
        (0u8..10).prop_map(MouseButton::Other),
    ]
}

fn arb_input_event_kind() -> impl Strategy<Value = InputEventKind> {
    prop_oneof![
        (arb_scan_code(), arb_modifier_mask())
            .prop_map(|(sc, m)| InputEventKind::KeyDown { scan_code: sc, modifiers: m }),
        (arb_scan_code(), arb_modifier_mask())
            .prop_map(|(sc, m)| InputEventKind::KeyUp { scan_code: sc, modifiers: m }),
        (-10000i32..10000, -10000i32..10000)
            .prop_map(|(dx, dy)| InputEventKind::MouseMoveRelative { dx, dy }),
        (0i32..8000, 0i32..4500)
            .prop_map(|(x, y)| InputEventKind::MouseMoveAbsolute { x, y }),
        arb_mouse_button().prop_map(|b| InputEventKind::MouseButtonDown { button: b }),
        arb_mouse_button().prop_map(|b| InputEventKind::MouseButtonUp { button: b }),
        (-120i32..120, -120i32..120)
            .prop_map(|(dx, dy)| InputEventKind::MouseScroll { dx, dy }),
    ]
}

fn arb_input_event() -> impl Strategy<Value = InputEvent> {
    (any::<u64>(), arb_input_event_kind()).prop_map(|(ts, kind)| InputEvent {
        timestamp_us: ts,
        kind,
    })
}

// --- Property tests ---

proptest! {
    /// Protocol message serialization must roundtrip perfectly.
    #[test]
    fn input_event_serialization_roundtrip(event in arb_input_event()) {
        let msg = ProtocolMessage::Input(InputMessage::Event(event.clone()));
        let bytes = serialize_message(&msg).unwrap();
        let decoded = deserialize_message(&bytes).unwrap();

        match decoded {
            ProtocolMessage::Input(InputMessage::Event(e)) => {
                assert_eq!(e.timestamp_us, event.timestamp_us);
                // Verify the event kind matches (deep equality via bincode roundtrip)
                let original_bytes = bincode::serialize(&event.kind).unwrap();
                let decoded_bytes = bincode::serialize(&e.kind).unwrap();
                assert_eq!(original_bytes, decoded_bytes);
            }
            _ => panic!("Expected Input message"),
        }
    }

    /// Event batches must roundtrip.
    #[test]
    fn event_batch_roundtrip(events in prop::collection::vec(arb_input_event(), 1..20)) {
        let msg = ProtocolMessage::Input(InputMessage::EventBatch(events.clone()));
        let bytes = serialize_message(&msg).unwrap();
        let decoded = deserialize_message(&bytes).unwrap();

        match decoded {
            ProtocolMessage::Input(InputMessage::EventBatch(batch)) => {
                assert_eq!(batch.len(), events.len());
            }
            _ => panic!("Expected EventBatch"),
        }
    }

    /// Heartbeat timestamps roundtrip exactly.
    #[test]
    fn heartbeat_roundtrip(ts in any::<u64>()) {
        let msg = ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us: ts });
        let bytes = serialize_message(&msg).unwrap();
        let decoded = deserialize_message(&bytes).unwrap();

        match decoded {
            ProtocolMessage::Control(ControlMessage::Heartbeat { timestamp_us }) => {
                assert_eq!(timestamp_us, ts);
            }
            _ => panic!("Expected Heartbeat"),
        }
    }

    /// Screen enter coordinates roundtrip.
    #[test]
    fn screen_enter_roundtrip(
        display_id in 0u32..16,
        x in -8000i32..8000,
        y in -4500i32..4500,
        mods in arb_modifier_mask(),
    ) {
        let msg = ProtocolMessage::Control(ControlMessage::ScreenEnter {
            display_id,
            x,
            y,
            modifiers: mods,
        });
        let bytes = serialize_message(&msg).unwrap();
        let decoded = deserialize_message(&bytes).unwrap();

        match decoded {
            ProtocolMessage::Control(ControlMessage::ScreenEnter {
                display_id: did,
                x: dx,
                y: dy,
                modifiers: dm,
            }) => {
                assert_eq!(did, display_id);
                assert_eq!(dx, x);
                assert_eq!(dy, y);
                assert_eq!(dm.0, mods.0);
            }
            _ => panic!("Expected ScreenEnter"),
        }
    }

    /// Clipboard data roundtrips.
    #[test]
    fn clipboard_data_roundtrip(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        let msg = ProtocolMessage::Data(DataMessage::ClipboardUpdate {
            content_type: ClipboardContentType::PlainText,
            data: data.clone(),
        });
        let bytes = serialize_message(&msg).unwrap();
        let decoded = deserialize_message(&bytes).unwrap();

        match decoded {
            ProtocolMessage::Data(DataMessage::ClipboardUpdate { data: d, .. }) => {
                assert_eq!(d, data);
            }
            _ => panic!("Expected ClipboardUpdate"),
        }
    }

    /// Modifier mask set/clear/has is consistent.
    #[test]
    fn modifier_mask_consistency(flags in prop::collection::vec(
        prop::sample::select(vec![
            ModifierMask::SHIFT,
            ModifierMask::CTRL,
            ModifierMask::ALT,
            ModifierMask::META,
            ModifierMask::CAPS_LOCK,
            ModifierMask::NUM_LOCK,
            ModifierMask::SCROLL_LOCK,
        ]),
        0..7,
    )) {
        let mut m = ModifierMask::default();
        for &flag in &flags {
            m.set(flag);
            assert!(m.has(flag));
        }
        for &flag in &flags {
            m.clear(flag);
            assert!(!m.has(flag));
        }
    }
}
