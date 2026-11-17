//! The window's size in physical pixels. Windows Desk is per-monitor DPI aware
//! (v2), so `CreateWindowExW` and `WM_GETMINMAXINFO` take physical pixels and
//! the app scales its device-independent design by the window's DPI / 96
//! ([High DPI desktop applications](https://learn.microsoft.com/windows/win32/hidpi/high-dpi-desktop-application-development-on-windows)).

/// The outer size the window opens at, in device-independent pixels.
pub const OPEN: (f32, f32) = (1040., 860.);
/// The smallest outer size the controls lay out in, in device-independent pixels.
pub const LEAST: (f32, f32) = (760., 710.);

/// `design` at `scale`, no larger than `work`, the monitor's work area: a
/// window or a minimum taller than the work area would leave controls where
/// no resize brings them into view.
pub fn size(design: (f32, f32), scale: f32, work: (i32, i32)) -> (i32, i32) {
    (
        ((design.0 * scale) as i32).min(work.0),
        ((design.1 * scale) as i32).min(work.1),
    )
}

/// The origin that keeps a window of `size` at `at` inside `work`
/// (left, top, right, bottom), moved no further than that takes.
pub fn inside(at: (i32, i32), size: (i32, i32), work: [i32; 4]) -> (i32, i32) {
    let [left, top, right, bottom] = work;
    (
        at.0.min(right - size.0).max(left),
        at.1.min(bottom - size.1).max(top),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// At 150% on a 1080p display (a 1920 by 1032 work area under the
    /// taskbar) the design is taller than the work area: the window opens
    /// at the work area's height, its minimum no taller, and inside it.
    #[test]
    fn the_window_opens_at_its_scaled_size_within_the_work_area() {
        let work = [0, 0, 1920, 1032];
        let screen = (1920, 1032);
        assert_eq!(size(OPEN, 1., screen), (1040, 860));
        assert_eq!(size(LEAST, 1., screen), (760, 710));
        assert_eq!(size(OPEN, 1.25, screen), (1300, 1032));
        assert_eq!(size(OPEN, 1.5, screen), (1560, 1032));
        assert_eq!(size(LEAST, 1.5, screen), (1140, 1032));
        for scale in [1., 1.25, 1.5, 1.75, 2.] {
            let (open, least) = (size(OPEN, scale, screen), size(LEAST, scale, screen));
            assert!(open.0 >= least.0 && open.1 >= least.1, "{scale}");
        }
        assert_eq!(inside((120, 90), (1560, 1032), work), (120, 0));
        assert_eq!(inside((600, 90), (1560, 1032), work), (360, 0));
        assert_eq!(inside((120, 90), (1040, 860), work), (120, 90));
        let second = [1920, 0, 3840, 1032];
        assert_eq!(inside((1900, 50), (1040, 860), second), (1920, 50));
    }
}
