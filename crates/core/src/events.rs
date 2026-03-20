//! Input event types for the KVM protocol.

use serde::{Deserialize, Serialize};

/// A hardware scan code (physical key position).
pub type ScanCode = u32;

/// Modifier key bitmask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ModifierMask(pub u16);

impl ModifierMask {
    pub const SHIFT: u16 = 1 << 0;
    pub const CTRL: u16 = 1 << 1;
    pub const ALT: u16 = 1 << 2;
    pub const META: u16 = 1 << 3;
    pub const CAPS_LOCK: u16 = 1 << 4;
    pub const NUM_LOCK: u16 = 1 << 5;
    pub const SCROLL_LOCK: u16 = 1 << 6;

    pub fn has(&self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    pub fn set(&mut self, flag: u16) {
        self.0 |= flag;
    }

    pub fn clear(&mut self, flag: u16) {
        self.0 &= !flag;
    }
}

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    Other(u8),
}

/// A single input event — the fundamental unit of the KVM protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputEvent {
    /// Microsecond timestamp (monotonic).
    pub timestamp_us: u64,
    /// The actual event data.
    pub kind: InputEventKind,
}

/// The different kinds of input events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InputEventKind {
    /// Key pressed down.
    KeyDown {
        scan_code: ScanCode,
        modifiers: ModifierMask,
    },
    /// Key released.
    KeyUp {
        scan_code: ScanCode,
        modifiers: ModifierMask,
    },
    /// Mouse moved (relative delta).
    MouseMoveRelative {
        dx: i32,
        dy: i32,
    },
    /// Mouse moved (absolute position).
    MouseMoveAbsolute {
        x: i32,
        y: i32,
    },
    /// Mouse button pressed.
    MouseButtonDown {
        button: MouseButton,
    },
    /// Mouse button released.
    MouseButtonUp {
        button: MouseButton,
    },
    /// Mouse scroll wheel.
    MouseScroll {
        dx: i32,
        dy: i32,
    },
}

impl InputEvent {
    pub fn new(kind: InputEventKind) -> Self {
        Self {
            timestamp_us: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_micros() as u64,
            kind,
        }
    }
}
