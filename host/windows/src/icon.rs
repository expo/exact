//! The window's icon (LLP 1163 D6): the executable's icon resource, which `host/windows/build.mjs` writes from the
//! app's manifest and the app crate's build script links, at the sizes the title bar and the taskbar draw.

use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXICON, SM_CXSMICON, SYSTEM_METRICS_INDEX,
};
use winit::{dpi::PhysicalSize, platform::windows::IconExtWindows, window::Icon};

/// The title bar's icon, or none when the manifest gives the app no icon.
pub(crate) fn small() -> Option<Icon> {
    at(SM_CXSMICON)
}

/// The taskbar's icon, or none when the manifest gives the app no icon.
pub(crate) fn large() -> Option<Icon> {
    at(SM_CXICON)
}

fn at(metric: SYSTEM_METRICS_INDEX) -> Option<Icon> {
    // SAFETY: GetSystemMetrics reads a system value and takes no pointer.
    #[allow(unsafe_code)]
    let side = unsafe { GetSystemMetrics(metric) }.max(16) as u32;
    Icon::from_resource(1, Some(PhysicalSize::new(side, side))).ok()
}
