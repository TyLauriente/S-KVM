//! Linux evdev-based input capture implementation.

use async_trait::async_trait;
use evdev::{Device, EventType, Key, RelativeAxisType};
use s_kvm_core::{InputEvent, InputEventKind, ModifierMask, MouseButton};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use super::{InputCapture, InputCaptureError};

/// Linux input capture using the evdev subsystem.
///
/// Enumerates input devices from /dev/input/, captures keyboard and mouse events,
/// translates them to the S-KVM InputEvent format, and supports exclusive grab.
pub struct LinuxInputCapture {
    /// Paths to devices we're capturing from.
    device_paths: Vec<PathBuf>,
    /// Whether capture is currently active.
    active: bool,
    /// Whether devices are currently grabbed for exclusive access.
    grabbed: bool,
    /// Handle to the capture task.
    capture_task: Option<JoinHandle<()>>,
    /// Sender used to signal stop to the capture task.
    stop_tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl LinuxInputCapture {
    /// Create a new LinuxInputCapture that will enumerate devices on start.
    pub fn new() -> Self {
        Self {
            device_paths: Vec::new(),
            active: false,
            grabbed: false,
            capture_task: None,
            stop_tx: None,
        }
    }

    /// Enumerate evdev devices and return paths to keyboards and mice.
    fn enumerate_devices() -> Result<Vec<PathBuf>, InputCaptureError> {
        let mut paths = Vec::new();

        for (path, device) in evdev::enumerate() {
            let events = device.supported_events();
            let is_keyboard = events.contains(EventType::KEY)
                && device
                    .supported_keys()
                    .map_or(false, |keys| keys.contains(Key::KEY_A));
            let is_mouse = events.contains(EventType::RELATIVE)
                && events.contains(EventType::KEY)
                && device
                    .supported_keys()
                    .map_or(false, |keys| keys.contains(Key::BTN_LEFT));

            if is_keyboard || is_mouse {
                let name = device.name().unwrap_or("unknown");
                debug!(path = %path.display(), name, is_keyboard, is_mouse, "Found input device");
                paths.push(path);
            }
        }

        if paths.is_empty() {
            warn!("No keyboard or mouse devices found");
        }

        Ok(paths)
    }
}

impl Default for LinuxInputCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Translate an evdev Key code to an S-KVM MouseButton.
fn key_to_mouse_button(key: Key) -> Option<MouseButton> {
    match key {
        Key::BTN_LEFT => Some(MouseButton::Left),
        Key::BTN_RIGHT => Some(MouseButton::Right),
        Key::BTN_MIDDLE => Some(MouseButton::Middle),
        Key::BTN_SIDE => Some(MouseButton::Back),
        Key::BTN_EXTRA => Some(MouseButton::Forward),
        _ => {
            let code = key.code();
            if (0x110..0x120).contains(&code) {
                Some(MouseButton::Other(code as u8))
            } else {
                None
            }
        }
    }
}

/// Check if a key is a modifier and return the corresponding ModifierMask flag.
fn modifier_flag_for_key(key: Key) -> Option<u16> {
    match key {
        Key::KEY_LEFTSHIFT | Key::KEY_RIGHTSHIFT => Some(ModifierMask::SHIFT),
        Key::KEY_LEFTCTRL | Key::KEY_RIGHTCTRL => Some(ModifierMask::CTRL),
        Key::KEY_LEFTALT | Key::KEY_RIGHTALT => Some(ModifierMask::ALT),
        Key::KEY_LEFTMETA | Key::KEY_RIGHTMETA => Some(ModifierMask::META),
        Key::KEY_CAPSLOCK => Some(ModifierMask::CAPS_LOCK),
        Key::KEY_NUMLOCK => Some(ModifierMask::NUM_LOCK),
        Key::KEY_SCROLLLOCK => Some(ModifierMask::SCROLL_LOCK),
        _ => None,
    }
}

/// Run the capture loop for a single evdev device, sending translated events over `tx`.
async fn capture_device(
    path: PathBuf,
    tx: mpsc::Sender<InputEvent>,
    mut stop_rx: tokio::sync::watch::Receiver<bool>,
    grabbed: bool,
) {
    let mut device = match Device::open(&path) {
        Ok(d) => d,
        Err(e) => {
            error!(path = %path.display(), error = %e, "Failed to open device");
            return;
        }
    };

    if grabbed {
        if let Err(e) = device.grab() {
            warn!(path = %path.display(), error = %e, "Failed to grab device");
        }
    }

    let mut stream = match device.into_event_stream() {
        Ok(s) => s,
        Err(e) => {
            error!(path = %path.display(), error = %e, "Failed to create event stream");
            return;
        }
    };

    let mut modifiers = ModifierMask::default();

    loop {
        tokio::select! {
            _ = stop_rx.changed() => {
                if *stop_rx.borrow() {
                    debug!(path = %path.display(), "Stopping capture");
                    break;
                }
            }
            result = stream.next_event() => {
                let ev = match result {
                    Ok(ev) => ev,
                    Err(e) => {
                        error!(path = %path.display(), error = %e, "Error reading event");
                        break;
                    }
                };

                let kind = match ev.kind() {
                    evdev::InputEventKind::Key(key) => {
                        let value = ev.value();
                        // Update modifier state
                        if let Some(flag) = modifier_flag_for_key(key) {
                            match key {
                                // Lock keys toggle on press
                                Key::KEY_CAPSLOCK | Key::KEY_NUMLOCK | Key::KEY_SCROLLLOCK => {
                                    if value == 1 {
                                        if modifiers.has(flag) {
                                            modifiers.clear(flag);
                                        } else {
                                            modifiers.set(flag);
                                        }
                                    }
                                }
                                // Regular modifiers: set on down, clear on up
                                _ => {
                                    if value == 1 {
                                        modifiers.set(flag);
                                    } else if value == 0 {
                                        modifiers.clear(flag);
                                    }
                                }
                            }
                        }

                        // Check if it's a mouse button
                        if let Some(button) = key_to_mouse_button(key) {
                            match value {
                                1 => Some(InputEventKind::MouseButtonDown { button }),
                                0 => Some(InputEventKind::MouseButtonUp { button }),
                                _ => None, // repeat events for buttons are ignored
                            }
                        } else {
                            let scan_code = key.code() as u32;
                            match value {
                                1 => Some(InputEventKind::KeyDown { scan_code, modifiers }),
                                0 => Some(InputEventKind::KeyUp { scan_code, modifiers }),
                                2 => Some(InputEventKind::KeyDown { scan_code, modifiers }), // repeat
                                _ => None,
                            }
                        }
                    }
                    evdev::InputEventKind::RelAxis(axis) => {
                        let value = ev.value();
                        match axis {
                            RelativeAxisType::REL_X => {
                                Some(InputEventKind::MouseMoveRelative { dx: value, dy: 0 })
                            }
                            RelativeAxisType::REL_Y => {
                                Some(InputEventKind::MouseMoveRelative { dx: 0, dy: value })
                            }
                            RelativeAxisType::REL_WHEEL => {
                                Some(InputEventKind::MouseScroll { dx: 0, dy: value })
                            }
                            RelativeAxisType::REL_HWHEEL => {
                                Some(InputEventKind::MouseScroll { dx: value, dy: 0 })
                            }
                            _ => None,
                        }
                    }
                    evdev::InputEventKind::AbsAxis(_axis) => {
                        let value = ev.value();
                        // For absolute axes, we'd need to track both X and Y.
                        // This is a simplification — real usage would batch X+Y together.
                        match _axis {
                            evdev::AbsoluteAxisType::ABS_X => {
                                Some(InputEventKind::MouseMoveAbsolute { x: value, y: 0 })
                            }
                            evdev::AbsoluteAxisType::ABS_Y => {
                                Some(InputEventKind::MouseMoveAbsolute { x: 0, y: value })
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                };

                if let Some(kind) = kind {
                    let input_event = InputEvent::new(kind);
                    if tx.send(input_event).await.is_err() {
                        debug!(path = %path.display(), "Event receiver dropped, stopping");
                        break;
                    }
                }
            }
        }
    }
}

#[async_trait]
impl InputCapture for LinuxInputCapture {
    async fn start(
        &mut self,
    ) -> Result<tokio::sync::mpsc::Receiver<InputEvent>, InputCaptureError> {
        if self.active {
            return Err(InputCaptureError::AlreadyCapturing);
        }

        self.device_paths = Self::enumerate_devices()?;
        if self.device_paths.is_empty() {
            return Err(InputCaptureError::DeviceOpen(
                "No input devices found".into(),
            ));
        }

        let (tx, rx) = mpsc::channel(1024);
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);

        let paths = self.device_paths.clone();
        let grabbed = self.grabbed;

        let task = tokio::spawn(async move {
            let mut handles = Vec::new();
            for path in paths {
                let tx = tx.clone();
                let stop_rx = stop_rx.clone();
                handles.push(tokio::spawn(
                    capture_device(path, tx, stop_rx, grabbed),
                ));
            }
            // Wait for all device capture tasks to complete
            for handle in handles {
                let _ = handle.await;
            }
        });

        self.capture_task = Some(task);
        self.stop_tx = Some(stop_tx);
        self.active = true;

        info!(
            device_count = self.device_paths.len(),
            "Input capture started"
        );
        Ok(rx)
    }

    async fn stop(&mut self) -> Result<(), InputCaptureError> {
        if !self.active {
            return Err(InputCaptureError::NotCapturing);
        }

        // Signal all capture tasks to stop
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(true);
        }

        // Wait for the main capture task to finish
        if let Some(task) = self.capture_task.take() {
            let _ = task.await;
        }

        self.active = false;
        info!("Input capture stopped");
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn grab(&mut self) -> Result<(), InputCaptureError> {
        if self.grabbed {
            return Ok(());
        }

        // If already capturing, we need to restart with grab enabled.
        // For simplicity, just set the flag; new captures will use it.
        // If currently active, stop and restart.
        let was_active = self.active;
        if was_active {
            self.stop().await?;
        }

        self.grabbed = true;

        if was_active {
            // Restart with grab. Caller must re-obtain the receiver.
            info!("Grab requested while active — caller should restart capture");
        }

        info!("Device grab enabled");
        Ok(())
    }

    async fn ungrab(&mut self) -> Result<(), InputCaptureError> {
        if !self.grabbed {
            return Ok(());
        }

        let was_active = self.active;
        if was_active {
            self.stop().await?;
        }

        self.grabbed = false;

        if was_active {
            info!("Ungrab requested while active — caller should restart capture");
        }

        info!("Device grab disabled");
        Ok(())
    }
}
