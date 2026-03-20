//! Windows SendInput-based input injection implementation.

use async_trait::async_trait;
use s_kvm_core::{InputEvent, InputEventKind, MouseButton};
use tracing::{debug, info, warn};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL,
    MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MOUSE_EVENT_FLAGS,
};

use super::{InputInjectionError, InputInjector};

/// XBUTTON1 data value for SendInput.
const XBUTTON1: u32 = 0x0001;
/// XBUTTON2 data value for SendInput.
const XBUTTON2: u32 = 0x0002;

/// Windows input injector using the `SendInput` API.
///
/// Translates `InputEvent` values into Win32 `INPUT` structures and dispatches
/// them via `SendInput`.
pub struct WindowsInputInjector {
    initialized: bool,
}

impl WindowsInputInjector {
    pub fn new() -> Self {
        Self { initialized: false }
    }
}

impl Default for WindowsInputInjector {
    fn default() -> Self {
        Self::new()
    }
}

/// Send a keyboard input via SendInput.
fn send_keyboard_input(scan_code: u32, is_key_up: bool) -> Result<(), InputInjectionError> {
    let mut flags = KEYEVENTF_SCANCODE;
    let mut sc = scan_code;

    // Extended keys have 0xE0 prefix in the scan code
    if sc > 0xFF {
        flags |= KEYEVENTF_EXTENDEDKEY;
        sc &= 0xFF; // Strip the prefix, the EXTENDEDKEY flag handles it
    }

    if is_key_up {
        flags |= KEYEVENTF_KEYUP;
    }

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: Default::default(),
                wScan: sc as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err(InputInjectionError::InjectionFailed(
            "SendInput returned 0 for keyboard event".into(),
        ));
    }

    Ok(())
}

/// Send a mouse input via SendInput.
fn send_mouse_input(
    flags: MOUSE_EVENT_FLAGS,
    dx: i32,
    dy: i32,
    mouse_data: u32,
) -> Result<(), InputInjectionError> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: mouse_data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    let sent = unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) };
    if sent == 0 {
        return Err(InputInjectionError::InjectionFailed(
            "SendInput returned 0 for mouse event".into(),
        ));
    }

    Ok(())
}

/// Map a MouseButton to (down_flags, up_flags, mouse_data).
fn mouse_button_flags(
    button: &MouseButton,
    is_down: bool,
) -> (MOUSE_EVENT_FLAGS, u32) {
    match button {
        MouseButton::Left => {
            let flag = if is_down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP };
            (flag, 0)
        }
        MouseButton::Right => {
            let flag = if is_down { MOUSEEVENTF_RIGHTDOWN } else { MOUSEEVENTF_RIGHTUP };
            (flag, 0)
        }
        MouseButton::Middle => {
            let flag = if is_down { MOUSEEVENTF_MIDDLEDOWN } else { MOUSEEVENTF_MIDDLEUP };
            (flag, 0)
        }
        MouseButton::Back => {
            let flag = if is_down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP };
            (flag, XBUTTON1)
        }
        MouseButton::Forward => {
            let flag = if is_down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP };
            (flag, XBUTTON2)
        }
        MouseButton::Other(code) => {
            // Best-effort: treat unknown buttons as XBUTTON with the code as data
            let flag = if is_down { MOUSEEVENTF_XDOWN } else { MOUSEEVENTF_XUP };
            (flag, *code as u32)
        }
    }
}

#[async_trait]
impl InputInjector for WindowsInputInjector {
    async fn init(&mut self) -> Result<(), InputInjectionError> {
        self.initialized = true;
        info!("Windows input injector initialized");
        Ok(())
    }

    async fn inject(&mut self, event: InputEvent) -> Result<(), InputInjectionError> {
        if !self.initialized {
            return Err(InputInjectionError::NotInitialized);
        }

        match event.kind {
            InputEventKind::KeyDown { scan_code, .. } => {
                tokio::task::spawn_blocking(move || send_keyboard_input(scan_code, false))
                    .await
                    .map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("join error: {e}"))
                    })??;
            }
            InputEventKind::KeyUp { scan_code, .. } => {
                tokio::task::spawn_blocking(move || send_keyboard_input(scan_code, true))
                    .await
                    .map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("join error: {e}"))
                    })??;
            }
            InputEventKind::MouseMoveRelative { dx, dy } => {
                tokio::task::spawn_blocking(move || {
                    send_mouse_input(MOUSEEVENTF_MOVE, dx, dy, 0)
                })
                .await
                .map_err(|e| {
                    InputInjectionError::InjectionFailed(format!("join error: {e}"))
                })??;
            }
            InputEventKind::MouseMoveAbsolute { x, y } => {
                tokio::task::spawn_blocking(move || {
                    // Normalize to 0–65535 range for SendInput absolute coordinates.
                    // This requires knowing the screen dimensions. We use
                    // GetSystemMetrics to query them.
                    let screen_width =
                        unsafe { windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                            windows::Win32::UI::WindowsAndMessaging::SM_CXSCREEN,
                        ) };
                    let screen_height =
                        unsafe { windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
                            windows::Win32::UI::WindowsAndMessaging::SM_CYSCREEN,
                        ) };

                    if screen_width == 0 || screen_height == 0 {
                        return Err(InputInjectionError::InjectionFailed(
                            "Could not determine screen dimensions".into(),
                        ));
                    }

                    let norm_x = (x * 65535) / screen_width;
                    let norm_y = (y * 65535) / screen_height;

                    send_mouse_input(
                        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                        norm_x,
                        norm_y,
                        0,
                    )
                })
                .await
                .map_err(|e| {
                    InputInjectionError::InjectionFailed(format!("join error: {e}"))
                })??;
            }
            InputEventKind::MouseButtonDown { ref button } => {
                let (flags, data) = mouse_button_flags(button, true);
                tokio::task::spawn_blocking(move || send_mouse_input(flags, 0, 0, data))
                    .await
                    .map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("join error: {e}"))
                    })??;
            }
            InputEventKind::MouseButtonUp { ref button } => {
                let (flags, data) = mouse_button_flags(button, false);
                tokio::task::spawn_blocking(move || send_mouse_input(flags, 0, 0, data))
                    .await
                    .map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("join error: {e}"))
                    })??;
            }
            InputEventKind::MouseScroll { dx, dy } => {
                if dy != 0 {
                    let dy_val = dy;
                    tokio::task::spawn_blocking(move || {
                        send_mouse_input(MOUSEEVENTF_WHEEL, 0, 0, dy_val as u32)
                    })
                    .await
                    .map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("join error: {e}"))
                    })??;
                }
                if dx != 0 {
                    let dx_val = dx;
                    tokio::task::spawn_blocking(move || {
                        send_mouse_input(MOUSEEVENTF_HWHEEL, 0, 0, dx_val as u32)
                    })
                    .await
                    .map_err(|e| {
                        InputInjectionError::InjectionFailed(format!("join error: {e}"))
                    })??;
                }
            }
        }

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), InputInjectionError> {
        self.initialized = false;
        info!("Windows input injector shut down");
        Ok(())
    }
}
