//! Cross-platform display/monitor enumeration.
//!
//! Supports Linux (X11 via x11rb, Wayland via sysfs fallback) and Windows.

use crate::types::DisplayInfo;
use crate::platform::DisplayServer;

/// Enumerate all displays on the system.
/// Automatically selects the right backend for the current platform.
pub fn enumerate_displays() -> Vec<DisplayInfo> {
    #[cfg(target_os = "linux")]
    {
        match crate::platform::detect_display_server() {
            DisplayServer::Wayland => enumerate_wayland().unwrap_or_else(|_| {
                // Wayland: try DRM/sysfs fallback
                enumerate_drm().unwrap_or_else(|_| vec![default_display()])
            }),
            DisplayServer::X11 => enumerate_x11().unwrap_or_else(|_| vec![default_display()]),
            _ => enumerate_drm().unwrap_or_else(|_| vec![default_display()]),
        }
    }

    #[cfg(target_os = "windows")]
    {
        enumerate_windows().unwrap_or_else(|_| vec![default_display()])
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        vec![default_display()]
    }
}

/// Default fallback display.
fn default_display() -> DisplayInfo {
    DisplayInfo {
        id: 0,
        name: "Primary Display".to_string(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        refresh_rate: 60.0,
        scale_factor: 1.0,
        is_primary: true,
    }
}

// ==========================================================================
// Linux / X11 — via x11rb + RandR
// ==========================================================================

#[cfg(target_os = "linux")]
fn enumerate_x11() -> Result<Vec<DisplayInfo>, Box<dyn std::error::Error>> {
    use x11rb::connection::Connection;
    use x11rb::protocol::randr::ConnectionExt as _;
    use x11rb::protocol::xproto::ConnectionExt as _;

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    // Get screen resources for refresh rate info
    let resources = conn.randr_get_screen_resources_current(root)?.reply()?;

    // Get monitors (RandR 1.5+)
    let monitors = conn.randr_get_monitors(root, true)?.reply()?;

    let mut displays = Vec::new();

    for (i, monitor) in monitors.monitors.iter().enumerate() {
        let name = conn
            .get_atom_name(monitor.name)?
            .reply()
            .map(|r| String::from_utf8_lossy(&r.name).to_string())
            .unwrap_or_else(|_| format!("Monitor {}", i));

        // Find refresh rate from the first output's current mode
        let refresh_rate = if let Some(&output) = monitor.outputs.first() {
            get_refresh_rate(&conn, &resources, output).unwrap_or(60.0)
        } else {
            60.0
        };

        displays.push(DisplayInfo {
            id: i as u32,
            name,
            x: monitor.x as i32,
            y: monitor.y as i32,
            width: monitor.width as u32,
            height: monitor.height as u32,
            refresh_rate,
            scale_factor: 1.0, // X11 doesn't have native per-monitor scaling
            is_primary: monitor.primary,
        });
    }

    if displays.is_empty() {
        // Fallback: use screen dimensions
        displays.push(DisplayInfo {
            id: 0,
            name: "Screen 0".to_string(),
            x: 0,
            y: 0,
            width: screen.width_in_pixels as u32,
            height: screen.height_in_pixels as u32,
            refresh_rate: 60.0,
            scale_factor: 1.0,
            is_primary: true,
        });
    }

    Ok(displays)
}

#[cfg(target_os = "linux")]
fn get_refresh_rate(
    conn: &impl x11rb::connection::Connection,
    resources: &x11rb::protocol::randr::GetScreenResourcesCurrentReply,
    output: u32,
) -> Result<f64, Box<dyn std::error::Error>> {
    use x11rb::protocol::randr::ConnectionExt as _;

    let output_info = conn
        .randr_get_output_info(output, resources.config_timestamp)?
        .reply()?;

    if output_info.crtc == 0 {
        return Ok(60.0);
    }

    let crtc_info = conn
        .randr_get_crtc_info(output_info.crtc, resources.config_timestamp)?
        .reply()?;

    // Find the mode info for the current mode
    for mode in &resources.modes {
        if mode.id == crtc_info.mode {
            if mode.htotal > 0 && mode.vtotal > 0 {
                let rate = mode.dot_clock as f64
                    / (mode.htotal as f64 * mode.vtotal as f64);
                return Ok((rate * 100.0).round() / 100.0);
            }
        }
    }

    Ok(60.0)
}

// ==========================================================================
// Linux / Wayland — sysfs/DRM fallback
// ==========================================================================

#[cfg(target_os = "linux")]
fn enumerate_wayland() -> Result<Vec<DisplayInfo>, Box<dyn std::error::Error>> {
    // Wayland doesn't have a simple client API for display enumeration
    // without connecting to the compositor. Use DRM/sysfs as a universal fallback.
    enumerate_drm()
}

#[cfg(target_os = "linux")]
fn enumerate_drm() -> Result<Vec<DisplayInfo>, Box<dyn std::error::Error>> {
    use std::fs;
    use std::path::Path;

    let drm_path = Path::new("/sys/class/drm");
    if !drm_path.exists() {
        return Err("DRM sysfs not available".into());
    }

    let mut displays = Vec::new();
    let mut id = 0u32;

    for entry in fs::read_dir(drm_path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Look for card*-* entries (e.g., card0-HDMI-A-1, card0-DP-1)
        if !name.starts_with("card") || !name.contains('-') {
            continue;
        }

        let status_path = entry.path().join("status");
        if status_path.exists() {
            let status = fs::read_to_string(&status_path)
                .unwrap_or_default()
                .trim()
                .to_string();
            if status != "connected" {
                continue;
            }
        } else {
            continue;
        }

        // Try to read mode (resolution)
        let modes_path = entry.path().join("modes");
        let (width, height, refresh) = if modes_path.exists() {
            let modes = fs::read_to_string(&modes_path).unwrap_or_default();
            parse_first_mode(&modes)
        } else {
            (1920, 1080, 60.0)
        };

        let connector_name = name
            .split_once('-')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_else(|| name.clone());

        displays.push(DisplayInfo {
            id,
            name: connector_name,
            x: 0, // sysfs doesn't provide position
            y: 0,
            width,
            height,
            refresh_rate: refresh,
            scale_factor: 1.0,
            is_primary: id == 0,
        });

        id += 1;
    }

    if displays.is_empty() {
        return Err("No connected DRM outputs found".into());
    }

    Ok(displays)
}

#[cfg(target_os = "linux")]
fn parse_first_mode(modes: &str) -> (u32, u32, f64) {
    // Mode format: "1920x1080" or "1920x1080i" (first line is preferred mode)
    if let Some(first_line) = modes.lines().next() {
        let clean = first_line.trim().trim_end_matches('i');
        if let Some((w, h)) = clean.split_once('x') {
            let width = w.parse().unwrap_or(1920);
            let height = h.parse().unwrap_or(1080);
            return (width, height, 60.0);
        }
    }
    (1920, 1080, 60.0)
}

// ==========================================================================
// Windows — EnumDisplayMonitors + GetMonitorInfo
// ==========================================================================

#[cfg(target_os = "windows")]
fn enumerate_windows() -> Result<Vec<DisplayInfo>, Box<dyn std::error::Error>> {
    use std::mem;
    use windows::Win32::Graphics::Gdi::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    let mut displays = Vec::new();
    let mut id = 0u32;

    // Callback receives a raw pointer to our Vec
    unsafe extern "system" fn enum_callback(
        hmonitor: HMONITOR,
        _hdc: HDC,
        _rect: *mut RECT,
        data: LPARAM,
    ) -> BOOL {
        let displays = &mut *(data.0 as *mut Vec<(HMONITOR, u32)>);
        displays.push((hmonitor, displays.len() as u32));
        BOOL(1)
    }

    let mut monitors: Vec<(HMONITOR, u32)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_callback),
            LPARAM(&mut monitors as *mut _ as isize),
        )?;
    }

    for (hmonitor, idx) in &monitors {
        let mut info: MONITORINFOEXW = unsafe { mem::zeroed() };
        info.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;

        let ok = unsafe {
            GetMonitorInfoW(*hmonitor, &mut info as *mut _ as *mut MONITORINFO)
        };

        if ok.as_bool() {
            let rect = info.monitorInfo.rcMonitor;
            let name = String::from_utf16_lossy(
                &info.szDevice[..info.szDevice.iter().position(|&c| c == 0).unwrap_or(info.szDevice.len())],
            );
            let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;

            // Get refresh rate via EnumDisplaySettings
            let mut devmode: DEVMODEW = unsafe { mem::zeroed() };
            devmode.dmSize = mem::size_of::<DEVMODEW>() as u16;
            let refresh = unsafe {
                if EnumDisplaySettingsW(
                    windows::core::PCWSTR(info.szDevice.as_ptr()),
                    ENUM_CURRENT_SETTINGS,
                    &mut devmode,
                ).as_bool() {
                    devmode.dmDisplayFrequency as f64
                } else {
                    60.0
                }
            };

            displays.push(DisplayInfo {
                id: *idx,
                name,
                x: rect.left,
                y: rect.top,
                width: (rect.right - rect.left) as u32,
                height: (rect.bottom - rect.top) as u32,
                refresh_rate: refresh,
                scale_factor: 1.0, // TODO: GetDpiForMonitor
                is_primary,
            });
        }
    }

    if displays.is_empty() {
        return Err("No monitors found".into());
    }

    Ok(displays)
}
