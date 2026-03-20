//! Virtual FIDO2 HID device using Linux uhid.
//!
//! Creates a virtual USB HID device that appears as a FIDO2 authenticator
//! to the operating system and web browsers. On non-Linux platforms, creation
//! returns an error.
//!
//! # I/O model
//!
//! A dedicated OS thread manages the blocking uhid device. It communicates
//! with the async relay via `tokio::sync::mpsc` channels:
//!
//! - **incoming** (device → relay): CTAPHID packets sent by the host
//! - **outgoing** (relay → device): CTAPHID response packets for the host
//!
//! The device thread writes pending outgoing packets before each blocking
//! read, so responses are delivered after the preceding request is received.

use tokio::sync::mpsc;

use crate::Fido2Error;

/// FIDO2 HID report descriptor.
///
/// Defines a device on usage page 0xF1D0 (FIDO Alliance) with 64-byte
/// input and output reports — the standard descriptor for FIDO U2F / FIDO2
/// authenticators.
#[allow(dead_code)]
const FIDO_HID_REPORT_DESCRIPTOR: &[u8] = &[
    0x06, 0xD0, 0xF1, // Usage Page (FIDO Alliance)
    0x09, 0x01,        // Usage (U2F HID Authenticator Device)
    0xA1, 0x01,        // Collection (Application)
    0x09, 0x20,        //   Usage (Input Report Data)
    0x15, 0x00,        //   Logical Minimum (0)
    0x26, 0xFF, 0x00,  //   Logical Maximum (255)
    0x75, 0x08,        //   Report Size (8)
    0x95, 0x40,        //   Report Count (64)
    0x81, 0x02,        //   Input (Data, Variable, Absolute)
    0x09, 0x21,        //   Usage (Output Report Data)
    0x15, 0x00,        //   Logical Minimum (0)
    0x26, 0xFF, 0x00,  //   Logical Maximum (255)
    0x75, 0x08,        //   Report Size (8)
    0x95, 0x40,        //   Report Count (64)
    0x91, 0x02,        //   Output (Data, Variable, Absolute)
    0xC0,              // End Collection
];

/// Virtual FIDO2 HID device.
pub struct VirtualFidoDevice {
    /// Receive CTAPHID packets from the host (browser/OS).
    pub incoming_rx: mpsc::Receiver<Vec<u8>>,
    /// Send CTAPHID response packets to the host.
    pub outgoing_tx: mpsc::Sender<Vec<u8>>,
    /// Signal the background thread to stop.
    #[cfg(target_os = "linux")]
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Background thread handle.
    #[cfg(target_os = "linux")]
    _worker: Option<std::thread::JoinHandle<()>>,
}

// =========================================================================
// Linux implementation (uhid)
// =========================================================================

#[cfg(target_os = "linux")]
impl VirtualFidoDevice {
    /// Create and register a virtual FIDO2 HID device.
    ///
    /// Returns a handle whose `incoming_rx` / `outgoing_tx` channels
    /// connect to the relay.
    pub fn create() -> Result<Self, Fido2Error> {
        use std::sync::{
            atomic::AtomicBool,
            Arc,
        };
        use uhid_virt::{Bus, CreateParams, UHIDDevice};

        let device = UHIDDevice::create(CreateParams {
            name: String::from("S-KVM Virtual FIDO2 Authenticator"),
            phys: String::new(),
            uniq: String::from("s-kvm-fido2-0"),
            bus: Bus::USB,
            vendor: 0x1209,  // pid.codes open-source VID
            product: 0xF1D0, // FIDO-like PID
            version: 0,
            country: 0,
            rd_data: FIDO_HID_REPORT_DESCRIPTOR.to_vec(),
        })
        .map_err(|e| Fido2Error::DeviceError(format!("failed to create uhid device: {e}")))?;

        let (incoming_tx, incoming_rx) = mpsc::channel::<Vec<u8>>(64);
        let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<Vec<u8>>(64);
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_flag = shutdown.clone();

        let worker = std::thread::Builder::new()
            .name("s-kvm-fido2-vhid".into())
            .spawn(move || {
                Self::device_loop(device, incoming_tx, &mut outgoing_rx, &shutdown_flag);
            })
            .map_err(|e| Fido2Error::DeviceError(format!("failed to spawn device thread: {e}")))?;

        tracing::info!("Virtual FIDO2 HID device created");

        Ok(Self {
            incoming_rx,
            outgoing_tx,
            shutdown,
            _worker: Some(worker),
        })
    }

    /// Signal the device to shut down.
    pub fn shutdown(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Background thread: reads from uhid, writes responses, loops.
    fn device_loop(
        mut device: uhid_virt::UHIDDevice<std::fs::File>,
        incoming_tx: mpsc::Sender<Vec<u8>>,
        outgoing_rx: &mut mpsc::Receiver<Vec<u8>>,
        shutdown: &std::sync::atomic::AtomicBool,
    ) {
        use std::sync::atomic::Ordering;
        use uhid_virt::OutputEvent;

        tracing::info!("Virtual FIDO2 device thread started");

        while !shutdown.load(Ordering::Relaxed) {
            // Flush pending response packets to the host.
            while let Ok(data) = outgoing_rx.try_recv() {
                if let Err(e) = device.write(&data) {
                    tracing::error!("uhid write error: {e}");
                }
            }

            // Read the next HID output report from the host (blocking).
            match device.read() {
                Ok(OutputEvent::Output { data }) => {
                    if incoming_tx.blocking_send(data).is_err() {
                        tracing::debug!("Incoming channel closed");
                        break;
                    }
                }
                Ok(OutputEvent::Stop) => {
                    tracing::info!("uhid device stopped by kernel");
                    break;
                }
                Err(_) => {
                    if !shutdown.load(Ordering::Relaxed) {
                        tracing::error!("uhid read error");
                    }
                    break;
                }
                _ => {} // Start, Open, Close — informational, ignore
            }
        }

        if let Err(e) = device.destroy() {
            tracing::warn!("Failed to destroy uhid device: {e}");
        }
        tracing::info!("Virtual FIDO2 device thread stopped");
    }
}

#[cfg(target_os = "linux")]
impl Drop for VirtualFidoDevice {
    fn drop(&mut self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

// =========================================================================
// Non-Linux stub
// =========================================================================

#[cfg(not(target_os = "linux"))]
impl VirtualFidoDevice {
    /// Always returns an error on non-Linux platforms.
    pub fn create() -> Result<Self, Fido2Error> {
        Err(Fido2Error::DeviceError(
            "virtual FIDO2 device requires Linux (uhid)".into(),
        ))
    }

    /// No-op on non-Linux.
    pub fn shutdown(&self) {}
}
