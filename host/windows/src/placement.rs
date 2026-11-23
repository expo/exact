//! The window's place on its monitor: kept reachable when a display's scale
//! changes, and foreseen for a hidden window, which takes a scale change
//! only when it next shows.
//!
//! Windows sends `WM_DPICHANGED` when "the DPI of the monitor hosting the
//! window changes", and a window's DPI "always equals the last DPI sent by
//! WM_DPICHANGED"
//! ([WM_DPICHANGED](https://learn.microsoft.com/windows/win32/hidpi/wm-dpichanged)).
//! A window that is hidden or cloaked when the scale changes is sent none
//! unless Windows moves it, as it refits a maximized one to the new work
//! area: on Windows 11 25H2 it keeps its old DPI until it is uncloaked or
//! moved, and a `ShowWindow` under DWM's cloak does not deliver it. The change
//! keeps the window's outer top left, and winit keeps the client's logical
//! size, so the place it takes is foreseeable ([`shown`]). A capture taken
//! while the window is hidden describes that place, the coordinate space
//! the app draws in once the window shows, and the show moves the window
//! there before its first frame ([`take_dpi`]).

/// Left, top, right, bottom, in physical pixels.
pub(crate) type Rect = [i32; 4];

/// The session window as Windows reports it, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// Visible and not cloaked by the app. The shell's cloak, on another
    /// virtual desktop, leaves the window where the session's viewport is.
    pub(crate) on_screen: bool,
    pub(crate) state: State,
    /// `GetDpiForWindow`.
    pub(crate) dpi: u32,
    /// The effective DPI of the monitor holding most of the window.
    pub(crate) monitor_dpi: u32,
    /// `GetWindowRect`.
    pub(crate) outer: Rect,
    /// The client area in screen coordinates.
    pub(crate) client: Rect,
    /// The frame's thickness on each side at `monitor_dpi`
    /// (`AdjustWindowRectExForDpi`).
    pub(crate) margins: Rect,
    /// That monitor's work area.
    pub(crate) work: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum State {
    Normal,
    Minimized,
    Maximized,
    /// Covering its monitor, which winit does not resize for a new DPI.
    Fullscreen,
}

/// Where the window is, or will be once it shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Shown {
    pub(crate) outer: Rect,
    pub(crate) client: crate::displays::Client,
}

/// The DPI the window draws at once it is on screen: its own while it is
/// there, its monitor's while it is not.
pub(crate) fn shown_dpi(on_screen: bool, window: u32, monitor: u32) -> u32 {
    if on_screen || monitor == 0 {
        window
    } else {
        monitor
    }
}

/// The window's place once it shows, with `minimum` the client's least
/// size in CSS pixels. A window taking a new DPI keeps its outer top left
/// (Windows's suggested rectangle keeps it for a scale change in place) and
/// its client's logical size, rounded as winit rounds it, and is then kept
/// in the work area as [`fit`] keeps it. A maximized window fills the work
/// area with its sizing borders beyond it; a fullscreen one keeps its
/// rectangle. A minimized window is described as it is.
pub(crate) fn shown(frame: &Frame, minimum: (f32, f32)) -> Shown {
    let on_screen = frame.on_screen || frame.state == State::Minimized;
    let dpi = shown_dpi(on_screen, frame.dpi, frame.monitor_dpi);
    let place = |outer: Rect| Shown {
        outer,
        client: crate::displays::Client {
            origin: [outer[0] + frame.margins[0], outer[1] + frame.margins[1]],
            dpi,
        },
    };
    if dpi == frame.dpi || frame.dpi == 0 {
        return Shown {
            outer: frame.outer,
            client: crate::displays::Client {
                origin: [frame.client[0], frame.client[1]],
                dpi,
            },
        };
    }
    let [left, top, right, bottom] = frame.margins;
    let work = frame.work;
    match frame.state {
        State::Maximized => {
            return place([
                work[0] - left,
                work[1] - bottom,
                work[2] + right,
                work[3] + bottom,
            ])
        }
        State::Fullscreen => return place(frame.outer),
        State::Normal | State::Minimized => {}
    }
    // winit's arithmetic: to logical pixels at the old scale, then to
    // physical ones at the new.
    let [from, to] = [frame.dpi, dpi].map(|dpi| f64::from(dpi) / 96.);
    let length = |a: i32, b: i32| (f64::from(b - a) / from * to).round() as i32;
    let width = length(frame.client[0], frame.client[2]) + left + right;
    let height = length(frame.client[1], frame.client[3]) + top + bottom;
    let least = |css: f32| (f64::from(css) * to).ceil() as i32;
    let [x, y] = [frame.outer[0], frame.outer[1]];
    place(fit_rect(
        [x, y, x + width, y + height],
        work,
        (
            least(minimum.0) + left + right,
            least(minimum.1) + top + bottom,
        ),
    ))
}

fn fit_rect(rect: Rect, work: Rect, minimum: (i32, i32)) -> Rect {
    let width = (rect[2] - rect[0]).min((work[2] - work[0]).max(minimum.0));
    let height = (rect[3] - rect[1]).min((work[3] - work[1]).max(minimum.1));
    let left = rect[0].clamp(work[0], (work[2] - width).max(work[0]));
    let top = rect[1].clamp(work[1], (work[3] - height).max(work[1]));
    [left, top, left + width, top + height]
}

#[cfg(windows)]
pub(crate) use os::{fit, shown_now, take_dpi};

#[cfg(windows)]
#[allow(unsafe_code)]
mod os {
    use super::{fit_rect, Frame, Rect, Shown, State};
    use windows_sys::Win32::{
        Foundation::{HWND, POINT, RECT},
        Graphics::{
            Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWM_CLOAKED_APP},
            Gdi::{
                ClientToScreen, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
                MONITOR_DEFAULTTONEAREST,
            },
        },
        UI::{
            HiDpi::{
                AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI,
            },
            WindowsAndMessaging::{
                GetClientRect, GetWindowLongW, GetWindowRect, IsIconic, IsWindowVisible, IsZoomed,
                SetWindowPos, GWL_EXSTYLE, GWL_STYLE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
            },
        },
    };
    use winit::{
        raw_window_handle::{HasWindowHandle, RawWindowHandle},
        window::Window,
    };

    fn rect(r: RECT) -> Rect {
        [r.left, r.top, r.right, r.bottom]
    }

    /// The monitor holding most of the window `hwnd`: its rectangle and work area.
    fn monitor(hwnd: HWND) -> Option<(Rect, Rect)> {
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let on = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        (unsafe { GetMonitorInfoW(on, &mut monitor) } != 0)
            .then(|| (rect(monitor.rcMonitor), rect(monitor.rcWork)))
    }

    /// The window `hwnd` as Windows reports it now.
    fn frame(hwnd: HWND) -> Option<Frame> {
        let (mut outer, mut client) = (RECT::default(), RECT::default());
        let mut origin = POINT { x: 0, y: 0 };
        if unsafe {
            GetWindowRect(hwnd, &mut outer) == 0
                || GetClientRect(hwnd, &mut client) == 0
                || ClientToScreen(hwnd, &mut origin) == 0
        } {
            return None;
        }
        let dpi = unsafe { GetDpiForWindow(hwnd) };
        let (mut monitor_dpi, mut unused) = (0, 0);
        let on = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        if unsafe { GetDpiForMonitor(on, MDT_EFFECTIVE_DPI, &mut monitor_dpi, &mut unused) } < 0 {
            monitor_dpi = 0;
        }
        let mut cloaked = 0u32;
        let read = unsafe {
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                size_of_val(&cloaked) as u32,
            )
        };
        let on_screen =
            unsafe { IsWindowVisible(hwnd) } != 0 && (read < 0 || cloaked & DWM_CLOAKED_APP == 0);
        let mut margins = RECT::default();
        let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
        let extended = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
        let at = if monitor_dpi == 0 { dpi } else { monitor_dpi };
        if unsafe { AdjustWindowRectExForDpi(&mut margins, style, 0, extended, at) } == 0 {
            return None;
        }
        // Without its monitor, the window is not fitted to a work area.
        let unbounded = [i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4];
        let (bounds, work) = monitor(hwnd).unwrap_or((unbounded, unbounded));
        let state = if unsafe { IsIconic(hwnd) } != 0 {
            State::Minimized
        } else if unsafe { IsZoomed(hwnd) } != 0 {
            State::Maximized
        } else if rect(outer) == bounds {
            State::Fullscreen
        } else {
            State::Normal
        };
        Some(Frame {
            on_screen,
            state,
            dpi,
            monitor_dpi,
            outer: rect(outer),
            client: [
                origin.x,
                origin.y,
                origin.x + client.right,
                origin.y + client.bottom,
            ],
            margins: [-margins.left, -margins.top, margins.right, margins.bottom],
            work,
        })
    }

    /// Where the window `hwnd` is, or will be once it shows.
    pub(crate) fn shown_now(hwnd: isize, minimum: (f32, f32)) -> Option<Shown> {
        frame(hwnd as HWND).map(|frame| super::shown(&frame, minimum))
    }

    /// While the window `hwnd` is shown under DWM's cloak, before its first
    /// frame: it takes the DPI a scale change while it was hidden left
    /// pending, and the place `shown`, foreseen before the show, says.
    /// Moving the window a pixel toward the middle of its monitor makes
    /// Windows send `WM_DPICHANGED`, which winit handles; the place then
    /// overrides winit's. A minimized, maximized or fullscreen window is
    /// left to Windows.
    pub(crate) fn take_dpi(hwnd: isize, shown: Shown) {
        let hwnd = hwnd as HWND;
        let [left, top, right, bottom] = shown.outer;
        let pending = unsafe { GetDpiForWindow(hwnd) } != shown.client.dpi;
        let state = frame(hwnd).map(|f| f.state);
        if !pending || state != Some(State::Normal) {
            return;
        }
        let Some((bounds, _)) = monitor(hwnd) else {
            return;
        };
        let on = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
        let flags = SWP_NOZORDER | SWP_NOACTIVATE;
        let mut now = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut now);
            let toward = if now.left + now.right < bounds[0] + bounds[2] {
                1
            } else {
                -1
            };
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                now.left + toward,
                now.top,
                0,
                0,
                flags | SWP_NOSIZE,
            );
            // Without the new DPI, a size set now would be scaled again when it arrives.
            if GetDpiForWindow(hwnd) == shown.client.dpi
                && MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) == on
            {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    left,
                    top,
                    right - left,
                    bottom - top,
                    flags,
                );
            } else {
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    now.left,
                    now.top,
                    0,
                    0,
                    flags | SWP_NOSIZE,
                );
            }
        }
    }

    pub(crate) fn fit(window: &Window, minimum: (f32, f32)) -> bool {
        if window.fullscreen().is_some()
            || window.is_maximized()
            || window.is_minimized() == Some(true)
        {
            return false;
        }
        let Ok(handle) = window.window_handle() else {
            return false;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };
        let hwnd = handle.hwnd.get() as HWND;
        let (mut outer, mut client) = (RECT::default(), RECT::default());
        // All coordinates are physical pixels in this per-monitor-aware process.
        if unsafe { GetWindowRect(hwnd, &mut outer) == 0 || GetClientRect(hwnd, &mut client) == 0 }
        {
            return false;
        }
        let Some((_, work)) = monitor(hwnd) else {
            return false;
        };
        let scale = window.scale_factor();
        let minimum = (
            (f64::from(minimum.0) * scale).ceil() as i32 + outer.right - outer.left - client.right,
            (f64::from(minimum.1) * scale).ceil() as i32 + outer.bottom - outer.top - client.bottom,
        );
        let outer = rect(outer);
        let fitted = fit_rect(outer, work, minimum);
        if fitted == outer {
            return false;
        }
        let [left, top, right, bottom] = fitted;
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                left,
                top,
                right - left,
                bottom - top,
                SWP_NOACTIVATE | SWP_NOZORDER,
            ) != 0
        }
    }
}

#[cfg(test)]
mod tests;
