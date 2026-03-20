//! Linux uinput-based input injection implementation.

use async_trait::async_trait;
use evdev::uinput::VirtualDeviceBuilder;
use evdev::{AttributeSet, EventType, Key, RelativeAxisType};
use s_kvm_core::{InputEvent, InputEventKind, MouseButton};
use tracing::{debug, info};

use super::{InputInjectionError, InputInjector};

/// Linux input injector using the uinput virtual device subsystem.
///
/// Creates virtual keyboard and mouse devices via /dev/uinput and injects
/// S-KVM InputEvents by translating them to evdev events.
pub struct LinuxInputInjector {
    /// Virtual keyboard device (handles key events).
    keyboard: Option<evdev::uinput::VirtualDevice>,
    /// Virtual mouse device (handles relative movement, buttons, scroll).
    mouse: Option<evdev::uinput::VirtualDevice>,
}

impl LinuxInputInjector {
    pub fn new() -> Self {
        Self {
            keyboard: None,
            mouse: None,
        }
    }

    /// Create a virtual keyboard device with all standard keys.
    fn create_virtual_keyboard() -> Result<evdev::uinput::VirtualDevice, InputInjectionError> {
        let mut keys = AttributeSet::<Key>::new();

        // Add all standard keyboard keys (KEY_ESC through KEY_MAX range used by keyboards)
        for code in 1..=248 {
            keys.insert(Key::new(code));
        }

        let device = VirtualDeviceBuilder::new()
            .map_err(|e| InputInjectionError::DeviceCreation(format!("keyboard builder: {e}")))?
            .name("S-KVM Virtual Keyboard")
            .with_keys(&keys)
            .map_err(|e| InputInjectionError::DeviceCreation(format!("keyboard keys: {e}")))?
            .build()
            .map_err(|e| InputInjectionError::DeviceCreation(format!("keyboard build: {e}")))?;

        debug!("Created virtual keyboard device");
        Ok(device)
    }

    /// Create a virtual mouse device with relative axes, buttons, and scroll.
    fn create_virtual_mouse() -> Result<evdev::uinput::VirtualDevice, InputInjectionError> {
        let mut keys = AttributeSet::<Key>::new();
        keys.insert(Key::BTN_LEFT);
        keys.insert(Key::BTN_RIGHT);
        keys.insert(Key::BTN_MIDDLE);
        keys.insert(Key::BTN_SIDE);
        keys.insert(Key::BTN_EXTRA);

        let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
        rel_axes.insert(RelativeAxisType::REL_X);
        rel_axes.insert(RelativeAxisType::REL_Y);
        rel_axes.insert(RelativeAxisType::REL_WHEEL);
        rel_axes.insert(RelativeAxisType::REL_HWHEEL);

        let device = VirtualDeviceBuilder::new()
            .map_err(|e| InputInjectionError::DeviceCreation(format!("mouse builder: {e}")))?
            .name("S-KVM Virtual Mouse")
            .with_keys(&keys)
            .map_err(|e| InputInjectionError::DeviceCreation(format!("mouse keys: {e}")))?
            .with_relative_axes(&rel_axes)
            .map_err(|e| InputInjectionError::DeviceCreation(format!("mouse axes: {e}")))?
            .build()
            .map_err(|e| InputInjectionError::DeviceCreation(format!("mouse build: {e}")))?;

        debug!("Created virtual mouse device");
        Ok(device)
    }
}

impl Default for LinuxInputInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// Translate an S-KVM MouseButton to an evdev Key code.
fn mouse_button_to_key(button: &MouseButton) -> Key {
    match button {
        MouseButton::Left => Key::BTN_LEFT,
        MouseButton::Right => Key::BTN_RIGHT,
        MouseButton::Middle => Key::BTN_MIDDLE,
        MouseButton::Back => Key::BTN_SIDE,
        MouseButton::Forward => Key::BTN_EXTRA,
        MouseButton::Other(code) => Key::new(*code as u16),
    }
}

#[async_trait]
impl InputInjector for LinuxInputInjector {
    async fn init(&mut self) -> Result<(), InputInjectionError> {
        self.keyboard = Some(Self::create_virtual_keyboard()?);
        self.mouse = Some(Self::create_virtual_mouse()?);
        info!("Linux input injector initialized");
        Ok(())
    }

    async fn inject(&mut self, event: InputEvent) -> Result<(), InputInjectionError> {
        match event.kind {
            InputEventKind::KeyDown { scan_code, .. } => {
                let kb = self
                    .keyboard
                    .as_mut()
                    .ok_or(InputInjectionError::NotInitialized)?;
                let ev = evdev::InputEvent::new(EventType::KEY, scan_code as u16, 1);
                kb.emit(&[ev])
                    .map_err(|e| InputInjectionError::InjectionFailed(format!("key down: {e}")))?;
            }
            InputEventKind::KeyUp { scan_code, .. } => {
                let kb = self
                    .keyboard
                    .as_mut()
                    .ok_or(InputInjectionError::NotInitialized)?;
                let ev = evdev::InputEvent::new(EventType::KEY, scan_code as u16, 0);
                kb.emit(&[ev])
                    .map_err(|e| InputInjectionError::InjectionFailed(format!("key up: {e}")))?;
            }
            InputEventKind::MouseMoveRelative { dx, dy } => {
                let mouse = self
                    .mouse
                    .as_mut()
                    .ok_or(InputInjectionError::NotInitialized)?;
                let mut events = Vec::with_capacity(2);
                if dx != 0 {
                    events.push(evdev::InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_X.0,
                        dx,
                    ));
                }
                if dy != 0 {
                    events.push(evdev::InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_Y.0,
                        dy,
                    ));
                }
                if !events.is_empty() {
                    mouse.emit(&events).map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("mouse move: {e}"))
                    })?;
                }
            }
            InputEventKind::MouseMoveAbsolute { x, y } => {
                // Absolute positioning requires an absolute axis device.
                // For now, log a warning — full absolute support would need
                // a separate virtual device with ABS_X/ABS_Y configured.
                tracing::warn!(x, y, "Absolute mouse move not fully supported yet");
            }
            InputEventKind::MouseButtonDown { ref button } => {
                let mouse = self
                    .mouse
                    .as_mut()
                    .ok_or(InputInjectionError::NotInitialized)?;
                let key = mouse_button_to_key(button);
                let ev = evdev::InputEvent::new(EventType::KEY, key.code(), 1);
                mouse.emit(&[ev]).map_err(|e| {
                    InputInjectionError::InjectionFailed(format!("button down: {e}"))
                })?;
            }
            InputEventKind::MouseButtonUp { ref button } => {
                let mouse = self
                    .mouse
                    .as_mut()
                    .ok_or(InputInjectionError::NotInitialized)?;
                let key = mouse_button_to_key(button);
                let ev = evdev::InputEvent::new(EventType::KEY, key.code(), 0);
                mouse.emit(&[ev]).map_err(|e| {
                    InputInjectionError::InjectionFailed(format!("button up: {e}"))
                })?;
            }
            InputEventKind::MouseScroll { dx, dy } => {
                let mouse = self
                    .mouse
                    .as_mut()
                    .ok_or(InputInjectionError::NotInitialized)?;
                let mut events = Vec::with_capacity(2);
                if dy != 0 {
                    events.push(evdev::InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_WHEEL.0,
                        dy,
                    ));
                }
                if dx != 0 {
                    events.push(evdev::InputEvent::new(
                        EventType::RELATIVE,
                        RelativeAxisType::REL_HWHEEL.0,
                        dx,
                    ));
                }
                if !events.is_empty() {
                    mouse.emit(&events).map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("scroll: {e}"))
                    })?;
                }
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), InputInjectionError> {
        // Dropping the VirtualDevice closes the uinput file descriptor,
        // which destroys the virtual device in the kernel.
        self.keyboard = None;
        self.mouse = None;
        info!("Linux input injector shut down");
        Ok(())
    }
}
