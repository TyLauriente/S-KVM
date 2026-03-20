//! KVM network protocol message types.

use crate::types::*;
use crate::events::InputEvent;
use serde::{Deserialize, Serialize};

/// Protocol version for compatibility checking.
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum message size in bytes.
pub const MAX_MESSAGE_SIZE: usize = 64 * 1024; // 64KB

/// Top-level protocol message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolMessage {
    /// Control channel messages (Stream 0).
    Control(ControlMessage),
    /// Input event messages (Stream 1).
    Input(InputMessage),
    /// Data channel messages (Stream 2 — clipboard, FIDO2).
    Data(DataMessage),
}

/// Control channel messages — handshake, screen negotiation, heartbeat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    /// Initial handshake from connecting peer.
    Hello {
        protocol_version: u32,
        peer_info: PeerInfo,
    },
    /// Response to Hello.
    Welcome {
        protocol_version: u32,
        peer_info: PeerInfo,
    },
    /// Screen layout update from a peer.
    ScreenLayout {
        displays: Vec<DisplayInfo>,
    },
    /// Cursor has entered this peer's screen.
    ScreenEnter {
        display_id: u32,
        x: i32,
        y: i32,
        modifiers: crate::events::ModifierMask,
    },
    /// Cursor has left this peer's screen.
    ScreenLeave {
        display_id: u32,
    },
    /// Periodic keepalive.
    Heartbeat {
        timestamp_us: u64,
    },
    /// Response to heartbeat (for latency measurement).
    HeartbeatAck {
        original_timestamp_us: u64,
        reply_timestamp_us: u64,
    },
    /// Request to start video streaming for a display.
    StartVideoStream {
        display_id: u32,
        preferred_codec: VideoCodec,
        max_fps: u32,
        max_bitrate_kbps: u32,
    },
    /// Stop video streaming.
    StopVideoStream {
        display_id: u32,
    },
    /// Request to start audio streaming.
    StartAudioStream {
        sample_rate: u32,
        channels: u16,
    },
    /// Stop audio streaming.
    StopAudioStream,
    /// Peer is disconnecting gracefully.
    Goodbye {
        reason: String,
    },
}

/// Input event messages — forwarded keyboard/mouse events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputMessage {
    /// A single input event.
    Event(InputEvent),
    /// Batch of input events (for efficiency).
    EventBatch(Vec<InputEvent>),
}

/// Data channel messages — clipboard, FIDO2, file transfer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataMessage {
    /// Clipboard content update.
    ClipboardUpdate {
        content_type: ClipboardContentType,
        data: Vec<u8>,
    },
    /// FIDO2/CTAP2 request forwarding.
    Fido2Request {
        request_id: u32,
        command: u8,
        payload: Vec<u8>,
    },
    /// FIDO2/CTAP2 response.
    Fido2Response {
        request_id: u32,
        status: u8,
        payload: Vec<u8>,
    },
}

/// Clipboard content MIME types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipboardContentType {
    PlainText,
    Html,
    RichText,
    Image,
    Files,
    Custom(String),
}

/// Supported video codecs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    H265,
    VP9,
    AV1,
}

/// Serialize a protocol message to binary (bincode).
pub fn serialize_message(msg: &ProtocolMessage) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

/// Deserialize a protocol message from binary.
pub fn deserialize_message(data: &[u8]) -> Result<ProtocolMessage, bincode::Error> {
    bincode::deserialize(data)
}
