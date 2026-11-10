//! The primary display's work area, used to center the app's window.

/// A rectangle of the desktop in points, `y` growing down.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct WorkArea {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The primary display's work area in points: the screen without the taskbar.
#[cfg(windows)]
pub fn primary_work_area() -> Option<WorkArea> {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SPI_GETWORKAREA, SystemParametersInfoW};

    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: `SPI_GETWORKAREA` writes one RECT through the pointer this
    // function owns and reads nothing else.
    let filled =
        unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, (&raw mut rect).cast::<c_void>(), 0) };
    if filled == 0 {
        return None;
    }
    // SAFETY: `GetDpiForSystem` takes no arguments.
    let dpi = unsafe { GetDpiForSystem() };
    let scale = if dpi == 0 { 1.0 } else { f64::from(dpi) / 96.0 };
    Some(WorkArea {
        x: f64::from(rect.left) / scale,
        y: f64::from(rect.top) / scale,
        width: f64::from(rect.right - rect.left) / scale,
        height: f64::from(rect.bottom - rect.top) / scale,
    })
}

#[cfg(not(windows))]
pub fn primary_work_area() -> Option<WorkArea> {
    None
}
