// Windows input APIs (SetWindowsHookEx, SendInput) require unsafe FFI calls.
// On non-Windows platforms, we keep the strict forbid.
#![cfg_attr(not(target_os = "windows"), forbid(unsafe_code))]

pub mod capture;
pub mod edge;
pub mod injection;

pub use capture::{InputCapture, InputCaptureError};
pub use injection::{InputInjector, InputInjectionError};

/// Create the platform-appropriate input capture backend.
pub fn create_capture() -> Box<dyn InputCapture> {
    #[cfg(target_os = "linux")]
    {
        Box::new(capture::linux::LinuxInputCapture::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(capture::windows::WindowsInputCapture::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        compile_error!("Unsupported platform for input capture")
    }
}

/// Create the platform-appropriate input injector backend.
pub fn create_injector() -> Box<dyn InputInjector> {
    #[cfg(target_os = "linux")]
    {
        Box::new(injection::linux::LinuxInputInjector::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(injection::windows::WindowsInputInjector::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        compile_error!("Unsupported platform for input injection")
    }
}
