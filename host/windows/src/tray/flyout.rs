//! The status menu's popup window: the shared presenter's MenuFlyout,
//! painted into a window of its own beside the notification icon, which
//! can open while the app's window is hidden (LLP 1115.004 D10.1). Its
//! room excludes the icon and taskbar, even when auto-hide leaves the
//! taskbar inside the monitor's work area. The Desktop Window Manager
//! rounds its corners as a Windows 11 flyout's
//! (`DWMWCP_ROUND`, 8 px) and the class's drop shadow is a menu's
//! (`CS_DROPSHADOW`). It takes the foreground, so that its keys reach it
//! and a click anywhere else, which activates another window, dismisses
//! it. Its pointer and keys are handed to the loop's next turn.
#![allow(unsafe_code)]
use super::placement::{Edge, Placement, Rect};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::{
        Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND},
        Gdi::{
            BeginPaint, EndPaint, GetMonitorInfoW, InvalidateRect, MonitorFromPoint,
            SetDIBitsToDevice, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, MONITORINFO,
            MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
        },
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Controls::WM_MOUSELEAVE,
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        Input::KeyboardAndMouse::{
            ReleaseCapture, SetCapture, SetFocus, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
        },
        Shell::{
            DefSubclassProc, RemoveWindowSubclass, SHAppBarMessage, SetWindowSubclass, ABE_BOTTOM,
            ABE_LEFT, ABE_RIGHT, ABE_TOP, ABM_GETAUTOHIDEBAREX, ABM_GETTASKBARPOS, APPBARDATA,
        },
        WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, GetWindowRect,
            LoadCursorW, RegisterClassExW, SetForegroundWindow, SetWindowPos, ShowWindow,
            SystemParametersInfoW, CS_DROPSHADOW, HWND_TOPMOST, IDC_ARROW, SM_MENUDROPALIGNMENT,
            SPI_GETWHEELSCROLLLINES, SWP_NOACTIVATE, SW_HIDE, SW_SHOW, WA_INACTIVE, WM_ACTIVATE,
            WM_CANCELMODE, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
            WM_MOUSEWHEEL, WM_NCDESTROY, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN,
            WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
        },
    },
};

/// What the menu's window heard, in its own device pixels.
pub(crate) enum Input {
    /// 0 down, 1 up, 2 move.
    Pointer(i32, f32, f32),
    /// The pointer left the window.
    Left,
    /// The wheel, points down the menu.
    Wheel(f32),
    /// A key, by `KeyboardEvent.key`'s name.
    Key(String),
    /// Another window was activated, Alt or F10 was pressed, or the system
    /// cancelled the menu's mode.
    Dismissed,
}

#[derive(Default)]
struct State {
    input: RefCell<Vec<Input>>,
    wake: RefCell<Option<Box<dyn Fn()>>>,
    /// The picture as BGRA rows, and its size in pixels.
    pixels: RefCell<(Vec<u8>, i32, i32)>,
    /// Whether the window is shown: what it hears while it is hidden, or
    /// hiding, is no one's.
    shown: Cell<bool>,
    tracking: Cell<bool>,
}

impl State {
    fn hear(&self, input: Input) {
        self.input.borrow_mut().push(input);
        if let Some(wake) = &*self.wake.borrow() {
            wake();
        }
    }
}

/// The menu's window, and the opening it shows.
pub(crate) struct Flyout {
    window: HWND,
    state: Rc<State>,
    /// The opening shown.
    shown: Option<u32>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

impl From<RECT> for Rect {
    fn from(rect: RECT) -> Self {
        Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

/// `ABM_GETTASKBARPOS` names the system taskbar. An auto-hide bar on a
/// secondary monitor is queried by its monitor and edge instead.
/// https://learn.microsoft.com/windows/win32/shell/abm-getautohidebarex
fn taskbar(monitor: RECT) -> Option<(Edge, Rect)> {
    let edges = [
        (ABE_LEFT, Edge::Left),
        (ABE_TOP, Edge::Top),
        (ABE_RIGHT, Edge::Right),
        (ABE_BOTTOM, Edge::Bottom),
    ];
    let mut data = APPBARDATA {
        cbSize: std::mem::size_of::<APPBARDATA>() as u32,
        ..Default::default()
    };
    if unsafe { SHAppBarMessage(ABM_GETTASKBARPOS, &mut data) } != 0
        && Rect::from(data.rc).intersects(monitor.into())
    {
        // Only rc is documented as output for ABM_GETTASKBARPOS.
        return Some((Edge::of_bar(data.rc.into(), monitor.into()), data.rc.into()));
    }
    for (value, edge) in edges {
        data.uEdge = value;
        data.rc = monitor;
        let window = unsafe { SHAppBarMessage(ABM_GETAUTOHIDEBAREX, &mut data) } as HWND;
        let mut rect = RECT::default();
        if !window.is_null() && unsafe { GetWindowRect(window, &mut rect) } != 0 {
            return Some((edge, rect.into()));
        }
    }
    None
}

impl Flyout {
    pub(crate) fn new() -> Option<Flyout> {
        let class = wide("ExactStatusMenu");
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let registration = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DROPSHADOW,
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            lpszClassName: class.as_ptr(),
            ..unsafe { std::mem::zeroed() }
        };
        // A second registration in the process fails, and the first serves.
        unsafe { RegisterClassExW(&registration) };
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                class.as_ptr(),
                std::ptr::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        let state = Rc::new(State::default());
        let data = Rc::into_raw(state.clone());
        // The window holds this reference until WM_NCDESTROY, on this thread.
        if window.is_null()
            || unsafe { SetWindowSubclass(window, Some(receive), 0, data as usize) } == 0
        {
            eprintln!(
                "exact-windows: status menu: {}",
                std::io::Error::last_os_error()
            );
            unsafe {
                drop(Rc::from_raw(data));
                if !window.is_null() {
                    DestroyWindow(window);
                }
            }
            return None;
        }
        let round = DWMWCP_ROUND;
        // Windows 10 has no such attribute, and keeps a menu's square corners.
        unsafe {
            DwmSetWindowAttribute(
                window,
                DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                (&round as *const i32).cast(),
                4,
            )
        };
        Some(Flyout {
            window,
            state,
            shown: None,
        })
    }

    pub(crate) fn set_wake(&self, wake: impl Fn() + 'static) {
        *self.state.wake.borrow_mut() = Some(Box::new(wake));
    }

    pub(crate) fn take(&self) -> Vec<Input> {
        std::mem::take(&mut *self.state.input.borrow_mut())
    }

    /// The icon's monitor, available room and scale. The callback's point
    /// is used only if the shell cannot locate the icon.
    pub(crate) fn anchor(icon: Option<Rect>, at: (i32, i32)) -> (Placement, f32) {
        let icon = icon.unwrap_or(Rect {
            left: at.0,
            top: at.1,
            right: at.0 + 1,
            bottom: at.1 + 1,
        });
        let at = (
            icon.left + (icon.right - icon.left) / 2,
            icon.top + (icon.bottom - icon.top) / 2,
        );
        let monitor =
            unsafe { MonitorFromPoint(POINT { x: at.0, y: at.1 }, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..unsafe { std::mem::zeroed() }
        };
        let (mut dpi, mut unused) = (96u32, 96u32);
        unsafe {
            GetMonitorInfoW(monitor, &mut info);
            GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused);
        }
        let scale = dpi.max(96) as f32 / 96.;
        (
            Placement::new(
                icon,
                info.rcMonitor.into(),
                info.rcWork.into(),
                taskbar(info.rcMonitor),
            ),
            scale,
        )
    }

    /// Show opening `serial` beside its icon, with premultiplied RGBA
    /// pixels. A new picture keeps the same anchor.
    pub(crate) fn show(&mut self, serial: u32, anchor: Placement, rgba: &[u8], size: (u32, u32)) {
        let bgra: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|px| [px[2], px[1], px[0], 255])
            .collect();
        *self.state.pixels.borrow_mut() = (bgra, size.0 as i32, size.1 as i32);
        let right_align = unsafe { GetSystemMetrics(SM_MENUDROPALIGNMENT) } != 0;
        let (width, height) = (size.0 as i32, size.1 as i32);
        let (x, y) = anchor.position((width, height), right_align);
        unsafe {
            SetWindowPos(
                self.window,
                HWND_TOPMOST,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE,
            );
            InvalidateRect(self.window, std::ptr::null(), 0);
        }
        if self.shown != Some(serial) && !self.state.shown.get() {
            self.state.shown.set(true);
            unsafe {
                ShowWindow(self.window, SW_SHOW);
                // The shell lets the icon's process take the foreground for
                // the person's click on it.
                SetForegroundWindow(self.window);
                SetFocus(self.window);
            }
        }
        self.shown = Some(serial);
    }

    pub(crate) fn hide(&mut self) {
        if self.shown.take().is_none() {
            return;
        }
        self.state.shown.set(false);
        unsafe {
            ReleaseCapture();
            ShowWindow(self.window, SW_HIDE);
        }
        self.state.input.borrow_mut().clear();
    }
}

impl Drop for Flyout {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.window) };
    }
}

unsafe extern "system" fn receive(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    subclass: usize,
    data: usize,
) -> isize {
    unsafe {
        let state = &*(data as *const State);
        let at = || {
            let x = lparam as usize as u16 as i16 as f32;
            let y = (lparam as usize >> 16) as u16 as i16 as f32;
            (x, y)
        };
        let shown = state.shown.get();
        match message {
            WM_PAINT => {
                let mut paint: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(window, &mut paint);
                let pixels = state.pixels.borrow();
                let (bits, w, h) = (&pixels.0, pixels.1, pixels.2);
                if !bits.is_empty() {
                    let mut info: BITMAPINFO = std::mem::zeroed();
                    info.bmiHeader = BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: w,
                        biHeight: -h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB,
                        ..std::mem::zeroed()
                    };
                    SetDIBitsToDevice(
                        dc,
                        0,
                        0,
                        w as u32,
                        h as u32,
                        0,
                        0,
                        0,
                        h as u32,
                        bits.as_ptr().cast(),
                        &info,
                        DIB_RGB_COLORS,
                    );
                }
                EndPaint(window, &paint);
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_MOUSEMOVE if shown => {
                if !state.tracking.replace(true) {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: window,
                        dwHoverTime: 0,
                    };
                    TrackMouseEvent(&mut track);
                }
                let (x, y) = at();
                state.hear(Input::Pointer(2, x, y));
            }
            WM_MOUSELEAVE => {
                state.tracking.set(false);
                if shown {
                    state.hear(Input::Left);
                }
            }
            // A menu takes either button, as `TPM_RIGHTBUTTON` lets one.
            WM_LBUTTONDOWN | WM_RBUTTONDOWN if shown => {
                SetCapture(window);
                let (x, y) = at();
                state.hear(Input::Pointer(0, x, y));
            }
            WM_LBUTTONUP | WM_RBUTTONUP if shown => {
                ReleaseCapture();
                let (x, y) = at();
                state.hear(Input::Pointer(1, x, y));
            }
            WM_MOUSEWHEEL if shown => {
                let mut lines = 3u32;
                SystemParametersInfoW(
                    SPI_GETWHEELSCROLLLINES,
                    0,
                    (&mut lines as *mut u32).cast(),
                    0,
                );
                let delta = (wparam >> 16) as u16 as i16;
                state.hear(Input::Wheel(super::wheel(delta, lines)));
                return 0;
            }
            WM_KEYDOWN if shown => {
                if let Some(key) = super::key(wparam as u32) {
                    state.hear(Input::Key(key));
                }
                return 0;
            }
            // Alt or F10 closes a menu, as it closes Windows' own.
            WM_SYSKEYDOWN if shown => {
                state.hear(Input::Dismissed);
                return 0;
            }
            WM_ACTIVATE if shown && (wparam & 0xFFFF) as u32 == WA_INACTIVE => {
                state.hear(Input::Dismissed)
            }
            WM_CANCELMODE if shown => state.hear(Input::Dismissed),
            WM_NCDESTROY => {
                RemoveWindowSubclass(window, Some(receive), subclass);
                drop(Rc::from_raw(data as *const State));
            }
            _ => {}
        }
        DefSubclassProc(window, message, wparam, lparam)
    }
}
