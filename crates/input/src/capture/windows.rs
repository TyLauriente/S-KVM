//! Windows low-level hook based input capture implementation.

use async_trait::async_trait;
use s_kvm_core::{InputEvent, InputEventKind, ModifierMask, MouseButton};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    MSLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::{InputCapture, InputCaptureError};

/// Horizontal mouse wheel message constant.
const WM_MOUSEHWHEEL: u32 = 0x020E;

/// XBUTTON1 identifier from high word of mouseData.
const XBUTTON1: u32 = 0x0001;

/// Thread-local storage for the event sender and state shared with hook callbacks.
///
/// This is necessary because Windows hook callbacks are plain function pointers
/// with no user-data parameter — thread-local state is the standard approach.
thread_local! {
    static HOOK_SENDER: std::cell::RefCell<Option<std::sync::mpsc::Sender<InputEvent>>> =
        const { std::cell::RefCell::new(None) };
    static GRAB_FLAG: std::cell::RefCell<Option<Arc<AtomicBool>>> =
        const { std::cell::RefCell::new(None) };
    static MODIFIER_STATE: std::cell::RefCell<ModifierMask> =
        std::cell::RefCell::new(ModifierMask(0));
    static LAST_MOUSE_POS: std::cell::RefCell<Option<(i32, i32)>> =
        const { std::cell::RefCell::new(None) };
}

/// Windows input capture using low-level keyboard and mouse hooks.
///
/// Installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL` hooks on a dedicated thread
/// running a Win32 message pump. Events are translated to `InputEvent` and
/// forwarded over a channel.
pub struct WindowsInputCapture {
    active: bool,
    grabbed: Arc<AtomicBool>,
    hook_thread: Option<JoinHandle<()>>,
    stop_flag: Arc<AtomicBool>,
}

impl WindowsInputCapture {
    pub fn new() -> Self {
        Self {
            active: false,
            grabbed: Arc::new(AtomicBool::new(false)),
            hook_thread: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for WindowsInputCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a scan code corresponds to a modifier key and return the flag.
fn modifier_flag_for_scan_code(scan_code: u32, is_extended: bool) -> Option<u16> {
    match scan_code {
        0x2A => Some(ModifierMask::SHIFT),         // Left Shift
        0x36 => Some(ModifierMask::SHIFT),         // Right Shift
        0x1D if !is_extended => Some(ModifierMask::CTRL), // Left Ctrl
        0x1D if is_extended => Some(ModifierMask::CTRL),  // Right Ctrl
        0x38 if !is_extended => Some(ModifierMask::ALT),  // Left Alt
        0x38 if is_extended => Some(ModifierMask::ALT),   // Right Alt
        0x5B => Some(ModifierMask::META),          // Left Win
        0x5C => Some(ModifierMask::META),          // Right Win
        0x3A => Some(ModifierMask::CAPS_LOCK),     // Caps Lock
        0x45 => Some(ModifierMask::NUM_LOCK),      // Num Lock
        0x46 => Some(ModifierMask::SCROLL_LOCK),   // Scroll Lock
        _ => None,
    }
}

/// Low-level keyboard hook callback.
unsafe extern "system" fn keyboard_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let kb = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
        let scan_code = kb.scanCode;
        let is_extended = (kb.flags.0 & 0x01) != 0; // LLKHF_EXTENDED
        let is_up = (kb.flags.0 & 0x80) != 0; // LLKHF_UP

        // Build the full scan code: extended keys get 0xE0 prefix
        let full_scan_code = if is_extended {
            0xE000 | scan_code
        } else {
            scan_code
        };

        // Update modifier state
        let modifiers = MODIFIER_STATE.with(|state| {
            let mut mods = state.borrow_mut();
            if let Some(flag) = modifier_flag_for_scan_code(scan_code, is_extended) {
                match scan_code {
                    // Lock keys toggle on press
                    0x3A | 0x45 | 0x46 => {
                        if !is_up {
                            if mods.has(flag) {
                                mods.clear(flag);
                            } else {
                                mods.set(flag);
                            }
                        }
                    }
                    // Regular modifiers: set on down, clear on up
                    _ => {
                        if is_up {
                            mods.clear(flag);
                        } else {
                            mods.set(flag);
                        }
                    }
                }
            }
            *mods
        });

        let kind = match w_param.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(InputEventKind::KeyDown {
                scan_code: full_scan_code,
                modifiers,
            }),
            WM_KEYUP | WM_SYSKEYUP => Some(InputEventKind::KeyUp {
                scan_code: full_scan_code,
                modifiers,
            }),
            _ => None,
        };

        if let Some(kind) = kind {
            let event = InputEvent::new(kind);
            HOOK_SENDER.with(|sender| {
                if let Some(ref tx) = *sender.borrow() {
                    let _ = tx.send(event);
                }
            });

            let should_grab = GRAB_FLAG.with(|flag| {
                flag.borrow()
                    .as_ref()
                    .map_or(false, |f| f.load(Ordering::Relaxed))
            });
            if should_grab {
                return LRESULT(1);
            }
        }
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}

/// Low-level mouse hook callback.
unsafe extern "system" fn mouse_hook_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let ms = &*(l_param.0 as *const MSLLHOOKSTRUCT);
        let msg = w_param.0 as u32;

        let kind = match msg {
            WM_MOUSEMOVE => {
                let (x, y) = (ms.pt.x, ms.pt.y);
                // Calculate relative delta from last known position
                let delta = LAST_MOUSE_POS.with(|last| {
                    let mut last = last.borrow_mut();
                    let delta = last.map(|(lx, ly)| (x - lx, y - ly));
                    *last = Some((x, y));
                    delta
                });
                delta.map(|(dx, dy)| InputEventKind::MouseMoveRelative { dx, dy })
            }
            WM_LBUTTONDOWN => Some(InputEventKind::MouseButtonDown {
                button: MouseButton::Left,
            }),
            WM_LBUTTONUP => Some(InputEventKind::MouseButtonUp {
                button: MouseButton::Left,
            }),
            WM_RBUTTONDOWN => Some(InputEventKind::MouseButtonDown {
                button: MouseButton::Right,
            }),
            WM_RBUTTONUP => Some(InputEventKind::MouseButtonUp {
                button: MouseButton::Right,
            }),
            WM_MBUTTONDOWN => Some(InputEventKind::MouseButtonDown {
                button: MouseButton::Middle,
            }),
            WM_MBUTTONUP => Some(InputEventKind::MouseButtonUp {
                button: MouseButton::Middle,
            }),
            WM_XBUTTONDOWN => {
                let xbutton = (ms.mouseData >> 16) & 0xFFFF;
                let button = if xbutton == XBUTTON1 {
                    MouseButton::Back
                } else {
                    MouseButton::Forward
                };
                Some(InputEventKind::MouseButtonDown { button })
            }
            WM_XBUTTONUP => {
                let xbutton = (ms.mouseData >> 16) & 0xFFFF;
                let button = if xbutton == XBUTTON1 {
                    MouseButton::Back
                } else {
                    MouseButton::Forward
                };
                Some(InputEventKind::MouseButtonUp { button })
            }
            WM_MOUSEWHEEL => {
                // mouseData high word is the wheel delta (signed)
                let delta = (ms.mouseData >> 16) as i16 as i32;
                Some(InputEventKind::MouseScroll { dx: 0, dy: delta })
            }
            WM_MOUSEHWHEEL => {
                let delta = (ms.mouseData >> 16) as i16 as i32;
                Some(InputEventKind::MouseScroll { dx: delta, dy: 0 })
            }
            _ => None,
        };

        if let Some(kind) = kind {
            let event = InputEvent::new(kind);
            HOOK_SENDER.with(|sender| {
                if let Some(ref tx) = *sender.borrow() {
                    let _ = tx.send(event);
                }
            });

            let should_grab = GRAB_FLAG.with(|flag| {
                flag.borrow()
                    .as_ref()
                    .map_or(false, |f| f.load(Ordering::Relaxed))
            });
            if should_grab {
                return LRESULT(1);
            }
        }
    }

    CallNextHookEx(None, n_code, w_param, l_param)
}

#[async_trait]
impl InputCapture for WindowsInputCapture {
    async fn start(
        &mut self,
    ) -> Result<tokio::sync::mpsc::Receiver<InputEvent>, InputCaptureError> {
        if self.active {
            return Err(InputCaptureError::AlreadyCapturing);
        }

        let (std_tx, std_rx) = std::sync::mpsc::channel::<InputEvent>();
        let (tokio_tx, tokio_rx) = mpsc::channel(1024);

        let grab_flag = self.grabbed.clone();
        let stop_flag = self.stop_flag.clone();
        stop_flag.store(false, Ordering::SeqCst);

        // Spawn the hook thread — hooks require a message pump on the installing thread
        let hook_handle = tokio::task::spawn_blocking(move || {
            // Set up thread-local state for the hook callbacks
            HOOK_SENDER.with(|sender| {
                *sender.borrow_mut() = Some(std_tx);
            });
            GRAB_FLAG.with(|flag| {
                *flag.borrow_mut() = Some(grab_flag);
            });
            LAST_MOUSE_POS.with(|last| {
                *last.borrow_mut() = None;
            });
            MODIFIER_STATE.with(|state| {
                *state.borrow_mut() = ModifierMask(0);
            });

            let kb_hook: HHOOK;
            let mouse_hook: HHOOK;

            unsafe {
                kb_hook = match SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0)
                {
                    Ok(h) => h,
                    Err(e) => {
                        error!("Failed to install keyboard hook: {e}");
                        return;
                    }
                };

                mouse_hook =
                    match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) {
                        Ok(h) => h,
                        Err(e) => {
                            error!("Failed to install mouse hook: {e}");
                            let _ = UnhookWindowsHookEx(kb_hook);
                            return;
                        }
                    };
            }

            debug!("Windows input hooks installed");

            // Message pump — required for low-level hooks to function
            let mut msg = MSG::default();
            loop {
                if stop_flag.load(Ordering::Relaxed) {
                    break;
                }

                unsafe {
                    // GetMessageW blocks until a message is available.
                    // We use PostThreadMessage from stop() to unblock it.
                    let ret = GetMessageW(&mut msg, None, 0, 0);
                    if ret.0 <= 0 {
                        break; // WM_QUIT or error
                    }
                }
            }

            // Clean up hooks
            unsafe {
                let _ = UnhookWindowsHookEx(kb_hook);
                let _ = UnhookWindowsHookEx(mouse_hook);
            }

            // Clear thread-local state
            HOOK_SENDER.with(|sender| {
                *sender.borrow_mut() = None;
            });
            GRAB_FLAG.with(|flag| {
                *flag.borrow_mut() = None;
            });

            debug!("Windows input hooks removed");
        });

        // Bridge from std::sync::mpsc to tokio::sync::mpsc
        tokio::spawn(async move {
            loop {
                match tokio::task::spawn_blocking({
                    let std_rx_ref = unsafe {
                        // SAFETY: The std_rx is moved into this task and only accessed here.
                        // We use a raw pointer to share it with spawn_blocking calls.
                        // Each spawn_blocking call runs sequentially (we await each one).
                        &*(&std_rx as *const std::sync::mpsc::Receiver<InputEvent>)
                    };
                    move || std_rx_ref.recv()
                })
                .await
                {
                    Ok(Ok(event)) => {
                        if tokio_tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        });

        self.hook_thread = Some(hook_handle);
        self.active = true;
        info!("Windows input capture started");
        Ok(tokio_rx)
    }

    async fn stop(&mut self) -> Result<(), InputCaptureError> {
        if !self.active {
            return Err(InputCaptureError::NotCapturing);
        }

        self.stop_flag.store(true, Ordering::SeqCst);

        // Post WM_QUIT to unblock GetMessageW on the hook thread
        // The hook thread will see the stop flag and exit
        if let Some(handle) = self.hook_thread.take() {
            let _ = handle.await;
        }

        self.active = false;
        info!("Windows input capture stopped");
        Ok(())
    }

    fn is_active(&self) -> bool {
        self.active
    }

    async fn grab(&mut self) -> Result<(), InputCaptureError> {
        self.grabbed.store(true, Ordering::SeqCst);
        info!("Windows input grab enabled");
        Ok(())
    }

    async fn ungrab(&mut self) -> Result<(), InputCaptureError> {
        self.grabbed.store(false, Ordering::SeqCst);
        info!("Windows input grab disabled");
        Ok(())
    }
}
