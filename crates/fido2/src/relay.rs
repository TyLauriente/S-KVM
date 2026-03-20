//! CTAP2 message relay for FIDO2 forwarding over the S-KVM network.
//!
//! This module implements the CTAPHID (HID-level) framing protocol and relays
//! CTAP2 CBOR commands from a local virtual FIDO2 device to a remote peer via
//! the S-KVM network protocol ([`DataMessage::Fido2Request`] / [`Fido2Response`]).

use std::collections::{HashMap, HashSet};

use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::Fido2Error;

// =========================================================================
// CTAPHID constants
// =========================================================================

/// CTAPHID packet size (always 64 bytes for FIDO HID).
const HID_PACKET_SIZE: usize = 64;
/// Data capacity in an initialisation packet (64 − 7 header bytes).
const INIT_DATA_SIZE: usize = HID_PACKET_SIZE - 7;
/// Data capacity in a continuation packet (64 − 5 header bytes).
const CONT_DATA_SIZE: usize = HID_PACKET_SIZE - 5;
/// Broadcast channel used for CTAPHID_INIT.
const CID_BROADCAST: u32 = 0xFFFF_FFFF;

// CTAPHID command codes (without the TYPE bit; bit 7 is set in the packet).
const CTAPHID_PING: u8 = 0x01;
#[allow(dead_code)]
const CTAPHID_MSG: u8 = 0x03;
const CTAPHID_INIT: u8 = 0x06;
const CTAPHID_CBOR: u8 = 0x10;
const CTAPHID_CANCEL: u8 = 0x11;
#[allow(dead_code)]
const CTAPHID_KEEPALIVE: u8 = 0x3B;
const CTAPHID_ERROR: u8 = 0x3F;

// CTAPHID error codes
const ERR_INVALID_CMD: u8 = 0x01;
#[allow(dead_code)]
const ERR_INVALID_PAR: u8 = 0x02;
const ERR_INVALID_LEN: u8 = 0x03;
const ERR_INVALID_SEQ: u8 = 0x04;
#[allow(dead_code)]
const ERR_MSG_TIMEOUT: u8 = 0x05;
#[allow(dead_code)]
const ERR_CHANNEL_BUSY: u8 = 0x06;
const ERR_INVALID_CHANNEL: u8 = 0x0B;
const ERR_OTHER: u8 = 0x7F;

// CTAP2 command codes
const CTAP2_MAKE_CREDENTIAL: u8 = 0x01;
const CTAP2_GET_ASSERTION: u8 = 0x02;
const CTAP2_GET_INFO: u8 = 0x04;

// =========================================================================
// Public types for network integration
// =========================================================================

/// A FIDO2 request to be sent over the network.
#[derive(Debug, Clone)]
pub struct NetworkFido2Request {
    pub request_id: u32,
    pub command: u8,
    pub payload: Vec<u8>,
}

/// A FIDO2 response received from the network.
#[derive(Debug, Clone)]
pub struct NetworkFido2Response {
    pub request_id: u32,
    pub status: u8,
    pub payload: Vec<u8>,
}

// =========================================================================
// Internal types
// =========================================================================

/// A fully reassembled CTAPHID message.
struct CtapHidMessage {
    channel_id: u32,
    command: u8,
    data: Vec<u8>,
}

/// Accumulates continuation packets until a full CTAPHID message is ready.
struct MessageAssembler {
    channel_id: u32,
    command: u8,
    expected_len: usize,
    data: Vec<u8>,
    next_seq: u8,
}

impl MessageAssembler {
    fn new(channel_id: u32, command: u8, expected_len: usize, initial_data: &[u8]) -> Self {
        let take = initial_data.len().min(expected_len);
        let mut data = Vec::with_capacity(expected_len);
        data.extend_from_slice(&initial_data[..take]);
        Self {
            channel_id,
            command,
            expected_len,
            data,
            next_seq: 0,
        }
    }

    fn is_complete(&self) -> bool {
        self.data.len() >= self.expected_len
    }

    fn add_continuation(&mut self, seq: u8, payload: &[u8]) -> Result<(), Fido2Error> {
        if seq != self.next_seq {
            return Err(Fido2Error::InvalidPacket(format!(
                "expected seq {}, got {seq}",
                self.next_seq
            )));
        }
        self.next_seq = self.next_seq.wrapping_add(1);
        let remaining = self.expected_len - self.data.len();
        let take = payload.len().min(remaining);
        self.data.extend_from_slice(&payload[..take]);
        Ok(())
    }

    fn into_message(self) -> CtapHidMessage {
        CtapHidMessage {
            channel_id: self.channel_id,
            command: self.command,
            data: self.data,
        }
    }
}

/// Tracks a request that has been forwarded to the network.
struct PendingRequest {
    channel_id: u32,
    #[allow(dead_code)]
    command: u8,
}

// =========================================================================
// Packet serialisation
// =========================================================================

/// Serialize a [`CtapHidMessage`] into one or more 64-byte HID packets.
fn serialize_response(msg: &CtapHidMessage) -> Vec<Vec<u8>> {
    let data = &msg.data;
    let total_len = data.len();
    let mut packets = Vec::new();

    // ── Initialisation packet ───────────────────────────────────────
    let mut pkt = vec![0u8; HID_PACKET_SIZE];
    pkt[0..4].copy_from_slice(&msg.channel_id.to_be_bytes());
    pkt[4] = 0x80 | msg.command;
    pkt[5..7].copy_from_slice(&(total_len as u16).to_be_bytes());

    let first_chunk = total_len.min(INIT_DATA_SIZE);
    pkt[7..7 + first_chunk].copy_from_slice(&data[..first_chunk]);
    packets.push(pkt);

    // ── Continuation packets ────────────────────────────────────────
    let mut offset = first_chunk;
    let mut seq: u8 = 0;
    while offset < total_len {
        let mut pkt = vec![0u8; HID_PACKET_SIZE];
        pkt[0..4].copy_from_slice(&msg.channel_id.to_be_bytes());
        pkt[4] = seq;

        let chunk = (total_len - offset).min(CONT_DATA_SIZE);
        pkt[5..5 + chunk].copy_from_slice(&data[offset..offset + chunk]);
        packets.push(pkt);

        offset += chunk;
        seq = seq.wrapping_add(1);
    }

    packets
}

// =========================================================================
// Fido2Relay
// =========================================================================

/// Relays CTAP2 commands between a local virtual FIDO2 device and a remote
/// peer over the S-KVM network.
///
/// # Channel layout
///
/// ```text
/// VirtualDevice ──[device_rx]──▶ Fido2Relay ──[request_tx]──▶ Network
/// VirtualDevice ◀──[device_tx]── Fido2Relay ◀──[response_rx]── Network
/// ```
pub struct Fido2Relay {
    /// Receive raw 64-byte packets from the virtual device (host → us).
    device_rx: mpsc::Receiver<Vec<u8>>,
    /// Send raw 64-byte packets to the virtual device (us → host).
    device_tx: mpsc::Sender<Vec<u8>>,
    /// Send FIDO2 requests to the network layer.
    request_tx: mpsc::Sender<NetworkFido2Request>,
    /// Receive FIDO2 responses from the network layer.
    response_rx: mpsc::Receiver<NetworkFido2Response>,
    /// In-progress message assembly (one at a time).
    assembler: Option<MessageAssembler>,
    /// Allocated CTAPHID channel IDs.
    channels: HashSet<u32>,
    /// Next channel ID to hand out.
    next_channel_id: u32,
    /// Requests forwarded to the network, keyed by request_id.
    pending: HashMap<u32, PendingRequest>,
    /// Monotonically increasing request ID.
    next_request_id: u32,
}

impl Fido2Relay {
    /// Create a new relay.
    ///
    /// * `device_rx`   – packets coming from the virtual HID device
    /// * `device_tx`   – packets going to the virtual HID device
    /// * `request_tx`  – outgoing FIDO2 requests for the network layer
    /// * `response_rx` – incoming FIDO2 responses from the network layer
    pub fn new(
        device_rx: mpsc::Receiver<Vec<u8>>,
        device_tx: mpsc::Sender<Vec<u8>>,
        request_tx: mpsc::Sender<NetworkFido2Request>,
        response_rx: mpsc::Receiver<NetworkFido2Response>,
    ) -> Self {
        Self {
            device_rx,
            device_tx,
            request_tx,
            response_rx,
            assembler: None,
            channels: HashSet::new(),
            next_channel_id: 1, // 0 is reserved, 0xFFFFFFFF is broadcast
            pending: HashMap::new(),
            next_request_id: 1,
        }
    }

    /// Run the relay event loop.
    pub async fn run(mut self) -> Result<(), Fido2Error> {
        info!("FIDO2 relay started");
        loop {
            tokio::select! {
                packet = self.device_rx.recv() => {
                    match packet {
                        Some(data) => self.handle_device_packet(&data).await?,
                        None => {
                            info!("Device channel closed, relay shutting down");
                            break;
                        }
                    }
                }
                response = self.response_rx.recv() => {
                    match response {
                        Some(resp) => self.handle_network_response(resp).await,
                        None => {
                            info!("Network channel closed, relay shutting down");
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // ── Device packet handling ──────────────────────────────────────

    async fn handle_device_packet(&mut self, raw: &[u8]) -> Result<(), Fido2Error> {
        if raw.len() < 5 {
            warn!("Ignoring short CTAPHID packet ({} bytes)", raw.len());
            return Ok(());
        }

        let channel_id = u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]);
        let cmd_byte = raw[4];

        if cmd_byte & 0x80 != 0 {
            // ── Initialisation packet ───────────────────────────────
            let command = cmd_byte & 0x7F;
            if raw.len() < 7 {
                warn!("Init packet header too short");
                return Ok(());
            }
            let payload_len = u16::from_be_bytes([raw[5], raw[6]]) as usize;
            let data = &raw[7..raw.len().min(7 + INIT_DATA_SIZE)];

            let assembler = MessageAssembler::new(channel_id, command, payload_len, data);
            if assembler.is_complete() {
                self.handle_message(assembler.into_message()).await;
            } else {
                self.assembler = Some(assembler);
            }
        } else {
            // ── Continuation packet ─────────────────────────────────
            let seq = cmd_byte;
            let data = &raw[5..raw.len().min(5 + CONT_DATA_SIZE)];

            if let Some(ref mut asm) = self.assembler {
                if asm.channel_id != channel_id {
                    warn!("Continuation for wrong channel");
                    self.send_error(channel_id, ERR_INVALID_CHANNEL).await;
                    return Ok(());
                }
                if let Err(e) = asm.add_continuation(seq, data) {
                    warn!("Invalid continuation: {e}");
                    self.assembler = None;
                    self.send_error(channel_id, ERR_INVALID_SEQ).await;
                    return Ok(());
                }
                if asm.is_complete() {
                    let msg = self.assembler.take().unwrap().into_message();
                    self.handle_message(msg).await;
                }
            } else {
                warn!("Continuation packet without preceding init");
                self.send_error(channel_id, ERR_INVALID_SEQ).await;
            }
        }

        Ok(())
    }

    // ── Message dispatch ────────────────────────────────────────────

    async fn handle_message(&mut self, msg: CtapHidMessage) {
        debug!(
            "CTAPHID message: channel={:#010X} cmd={:#04X} len={}",
            msg.channel_id,
            msg.command,
            msg.data.len()
        );

        match msg.command {
            CTAPHID_INIT => {
                let resp = self.handle_init(msg.channel_id, &msg.data);
                self.send_message(&resp).await;
            }
            CTAPHID_PING => {
                // Echo the payload back.
                let resp = CtapHidMessage {
                    channel_id: msg.channel_id,
                    command: CTAPHID_PING,
                    data: msg.data,
                };
                self.send_message(&resp).await;
            }
            CTAPHID_CBOR => {
                self.handle_cbor(msg.channel_id, &msg.data).await;
            }
            CTAPHID_CANCEL => {
                let before = self.pending.len();
                self.pending
                    .retain(|_, p| p.channel_id != msg.channel_id);
                let cancelled = before - self.pending.len();
                if cancelled > 0 {
                    info!(
                        "Cancelled {cancelled} pending request(s) for channel {:#010X}",
                        msg.channel_id
                    );
                }
            }
            other => {
                warn!("Unsupported CTAPHID command: {other:#04X}");
                self.send_error(msg.channel_id, ERR_INVALID_CMD).await;
            }
        }
    }

    // ── CTAPHID_INIT ────────────────────────────────────────────────

    fn handle_init(&mut self, channel_id: u32, nonce: &[u8]) -> CtapHidMessage {
        let new_cid = self.allocate_channel();

        let mut resp = Vec::with_capacity(17);

        // 8-byte nonce echo
        let nonce_len = nonce.len().min(8);
        resp.extend_from_slice(&nonce[..nonce_len]);
        resp.resize(8, 0);

        // 4-byte new channel ID
        resp.extend_from_slice(&new_cid.to_be_bytes());

        // CTAPHID protocol version (2)
        resp.push(2);
        // Device version major.minor.build
        resp.push(0);
        resp.push(1);
        resp.push(0);
        // Capabilities: 0x04 = CBOR supported
        resp.push(0x04);

        info!("CTAPHID INIT: allocated channel {new_cid:#010X}");

        CtapHidMessage {
            channel_id,
            command: CTAPHID_INIT,
            data: resp,
        }
    }

    fn allocate_channel(&mut self) -> u32 {
        let cid = self.next_channel_id;
        self.next_channel_id = self.next_channel_id.wrapping_add(1);
        if self.next_channel_id == 0 || self.next_channel_id == CID_BROADCAST {
            self.next_channel_id = 1;
        }
        self.channels.insert(cid);
        cid
    }

    // ── CTAPHID_CBOR → network ──────────────────────────────────────

    async fn handle_cbor(&mut self, channel_id: u32, payload: &[u8]) {
        if payload.is_empty() {
            self.send_error(channel_id, ERR_INVALID_LEN).await;
            return;
        }

        let ctap2_cmd = payload[0];
        let ctap2_payload = &payload[1..];

        let cmd_name = match ctap2_cmd {
            CTAP2_MAKE_CREDENTIAL => "authenticatorMakeCredential",
            CTAP2_GET_ASSERTION => "authenticatorGetAssertion",
            CTAP2_GET_INFO => "authenticatorGetInfo",
            _ => "unknown",
        };

        info!(
            "FIDO2 relay: forwarding {cmd_name} (0x{ctap2_cmd:02X}), {} bytes",
            ctap2_payload.len()
        );

        let request_id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);

        self.pending.insert(
            request_id,
            PendingRequest {
                channel_id,
                command: ctap2_cmd,
            },
        );

        let req = NetworkFido2Request {
            request_id,
            command: ctap2_cmd,
            payload: ctap2_payload.to_vec(),
        };

        if self.request_tx.send(req).await.is_err() {
            error!("Failed to forward FIDO2 request: network channel closed");
            self.pending.remove(&request_id);
            self.send_error(channel_id, ERR_OTHER).await;
        }
    }

    // ── Network response → device ───────────────────────────────────

    async fn handle_network_response(&mut self, response: NetworkFido2Response) {
        let Some(pending) = self.pending.remove(&response.request_id) else {
            warn!(
                "Received FIDO2 response for unknown request {}",
                response.request_id
            );
            return;
        };

        info!(
            "FIDO2 relay: received response for request {} (status 0x{:02X})",
            response.request_id, response.status
        );

        // CTAP2 response = status byte + CBOR payload
        let mut data = Vec::with_capacity(1 + response.payload.len());
        data.push(response.status);
        data.extend_from_slice(&response.payload);

        let msg = CtapHidMessage {
            channel_id: pending.channel_id,
            command: CTAPHID_CBOR,
            data,
        };
        self.send_message(&msg).await;
    }

    // ── Helpers ─────────────────────────────────────────────────────

    async fn send_message(&self, msg: &CtapHidMessage) {
        let packets = serialize_response(msg);
        for pkt in packets {
            if self.device_tx.send(pkt).await.is_err() {
                error!("Failed to send packet to virtual device");
                break;
            }
        }
    }

    async fn send_error(&self, channel_id: u32, error_code: u8) {
        let msg = CtapHidMessage {
            channel_id,
            command: CTAPHID_ERROR,
            data: vec![error_code],
        };
        self.send_message(&msg).await;
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialize_single_packet_response() {
        let msg = CtapHidMessage {
            channel_id: 0x00000001,
            command: CTAPHID_INIT,
            data: vec![0xAA; 10],
        };
        let packets = serialize_response(&msg);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].len(), HID_PACKET_SIZE);

        // Channel ID
        assert_eq!(&packets[0][0..4], &[0, 0, 0, 1]);
        // Command with TYPE bit
        assert_eq!(packets[0][4], 0x80 | CTAPHID_INIT);
        // Payload length
        assert_eq!(&packets[0][5..7], &[0, 10]);
        // Data
        assert_eq!(&packets[0][7..17], &[0xAA; 10]);
    }

    #[test]
    fn serialize_multi_packet_response() {
        // 60 bytes > INIT_DATA_SIZE (57), so needs one continuation
        let msg = CtapHidMessage {
            channel_id: 0x00000002,
            command: CTAPHID_CBOR,
            data: vec![0xBB; 60],
        };
        let packets = serialize_response(&msg);
        assert_eq!(packets.len(), 2);

        // Init packet: 57 bytes of data
        assert_eq!(packets[0][4], 0x80 | CTAPHID_CBOR);
        assert_eq!(&packets[0][5..7], &[0, 60]);
        assert_eq!(&packets[0][7..64], &[0xBB; 57]);

        // Continuation packet: remaining 3 bytes
        assert_eq!(&packets[1][0..4], &[0, 0, 0, 2]);
        assert_eq!(packets[1][4], 0); // seq 0
        assert_eq!(&packets[1][5..8], &[0xBB; 3]);
    }

    #[test]
    fn assembler_single_packet() {
        let asm = MessageAssembler::new(1, CTAPHID_CBOR, 5, &[1, 2, 3, 4, 5]);
        assert!(asm.is_complete());
        let msg = asm.into_message();
        assert_eq!(msg.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn assembler_multi_packet() {
        let mut asm = MessageAssembler::new(1, CTAPHID_CBOR, 10, &[1, 2, 3, 4, 5]);
        assert!(!asm.is_complete());
        asm.add_continuation(0, &[6, 7, 8, 9, 10]).unwrap();
        assert!(asm.is_complete());
        let msg = asm.into_message();
        assert_eq!(msg.data, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn assembler_rejects_wrong_sequence() {
        let mut asm = MessageAssembler::new(1, CTAPHID_CBOR, 20, &[0; 5]);
        assert!(asm.add_continuation(1, &[0; 5]).is_err());
    }

    #[tokio::test]
    async fn relay_handles_init() {
        let (dev_tx_in, dev_rx) = mpsc::channel(16);
        let (dev_tx, mut dev_rx_out) = mpsc::channel(16);
        let (req_tx, _req_rx) = mpsc::channel(16);
        let (_resp_tx, resp_rx) = mpsc::channel(16);

        let relay = Fido2Relay::new(dev_rx, dev_tx, req_tx, resp_rx);
        let handle = tokio::spawn(relay.run());

        // Send an INIT packet on broadcast channel
        let mut pkt = vec![0u8; 64];
        pkt[0..4].copy_from_slice(&CID_BROADCAST.to_be_bytes());
        pkt[4] = 0x80 | CTAPHID_INIT;
        pkt[5..7].copy_from_slice(&8u16.to_be_bytes());
        pkt[7..15].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

        dev_tx_in.send(pkt).await.unwrap();

        // Read response
        let resp = dev_rx_out.recv().await.unwrap();
        assert_eq!(resp.len(), 64);
        assert_eq!(resp[4], 0x80 | CTAPHID_INIT);
        // Nonce echoed
        assert_eq!(&resp[7..15], &[1, 2, 3, 4, 5, 6, 7, 8]);
        // Allocated channel ID = 1
        assert_eq!(&resp[15..19], &1u32.to_be_bytes());

        drop(dev_tx_in);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn relay_forwards_cbor_to_network() {
        let (dev_tx_in, dev_rx) = mpsc::channel(16);
        let (dev_tx, _dev_rx_out) = mpsc::channel(16);
        let (req_tx, mut req_rx) = mpsc::channel(16);
        let (_resp_tx, resp_rx) = mpsc::channel(16);

        let relay = Fido2Relay::new(dev_rx, dev_tx, req_tx, resp_rx);
        let handle = tokio::spawn(relay.run());

        // Send a CBOR packet (authenticatorGetInfo = 0x04)
        let mut pkt = vec![0u8; 64];
        pkt[0..4].copy_from_slice(&1u32.to_be_bytes());
        pkt[4] = 0x80 | CTAPHID_CBOR;
        pkt[5..7].copy_from_slice(&1u16.to_be_bytes());
        pkt[7] = CTAP2_GET_INFO;

        dev_tx_in.send(pkt).await.unwrap();

        let req = req_rx.recv().await.unwrap();
        assert_eq!(req.command, CTAP2_GET_INFO);
        assert_eq!(req.request_id, 1);

        drop(dev_tx_in);
        let _ = handle.await;
    }
}
