//! The monitors, the pointer and their changes, read from Windows.
#![allow(unsafe_code)]
use super::{Client, Monitor};
use exact_painted::desktop::{Pointer, Screens};
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use windows::{Foundation::TypedEventHandler, UI::ViewManagement::UISettings};
use windows_sys::Win32::{
    Devices::Display::{
        DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
        DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL, DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS,
        DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
    },
    Foundation::{ERROR_SUCCESS, HWND, LPARAM, POINT, RECT},
    Graphics::Gdi::{
        ClientToScreen, EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromWindow,
        HDC, HMONITOR, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    },
    UI::{
        HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            GetCursorPos, MONITORINFOF_PRIMARY, SPI_SETWORKAREA, WM_DISPLAYCHANGE, WM_DPICHANGED,
            WM_NCDESTROY, WM_SETTINGCHANGE, WM_WINDOWPOSCHANGED,
        },
    },
};

// The values `super::built_in` reads are Windows's.
const _: () = assert!(
    super::INTERNAL == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_INTERNAL
        && super::DISPLAYPORT_EMBEDDED == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_DISPLAYPORT_EMBEDDED
        && super::UDI_EMBEDDED == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_UDI_EMBEDDED
        && super::LVDS == DISPLAYCONFIG_OUTPUT_TECHNOLOGY_LVDS
);

/// A session's label and what to call when the displays change.
type Watcher = (String, Box<dyn Fn() + Send>);

/// The sessions reading the displays. The window's subclass tells them on
/// the loop's thread, and the Text size setting's handler on one of WinRT's.
type Watchers = Arc<Mutex<Vec<Watcher>>>;

fn locked(watchers: &Watchers) -> MutexGuard<'_, Vec<Watcher>> {
    watchers.lock().unwrap_or_else(|e| e.into_inner())
}

fn tell(watchers: &Watchers) {
    for (_, changed) in locked(watchers).iter() {
        changed();
    }
}

/// What the window's subclass shares with the service, both on the loop's
/// thread.
#[derive(Default)]
struct Watch {
    watchers: Watchers,
    /// The monitor the window was on when last compared.
    on: Cell<isize>,
}

/// The host's display service for the session window `hwnd`.
struct Displays {
    hwnd: isize,
    watch: Rc<Watch>,
    /// Whether the display changes are heard.
    hears: Cell<bool>,
    /// The system's settings, made at the first read, for the Text size
    /// setting; `None` where WinRT has none, which reads as the default.
    settings: OnceCell<Option<UISettings>>,
}

/// Report the displays to every session on this thread, measured against
/// the session window `hwnd`.
pub(crate) fn install(hwnd: isize) {
    let displays = Rc::new(Displays {
        hwnd,
        watch: Rc::default(),
        hears: Cell::new(false),
        settings: OnceCell::new(),
    });
    CURRENT.with(|current| *current.borrow_mut() = Some(displays.clone()));
    exact_painted::desktop::install_displays(displays);
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Displays>>> = const { RefCell::new(None) };
}

/// No HWND is needed to enumerate the real desktop. A hidden window keeps
/// its client geometry; a session without one reports null clientRect.
pub(crate) fn capture_snapshot() -> Vec<(exact_painted::desktop::Screen, [i32; 4])> {
    let _dpi = crate::screen_capture::os::Dpi::new();
    CURRENT.with(|current| {
        let current = current.borrow();
        let fallback = Displays {
            hwnd: 0,
            watch: Rc::default(),
            hears: Cell::new(false),
            settings: OnceCell::new(),
        };
        let display = current.as_deref().unwrap_or(&fallback);
        let monitors = split(monitors(display.text_size()));
        super::capture_rects(&monitors, display.client())
    })
}

unsafe extern "system" fn each(monitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> i32 {
    unsafe { (*(list as *mut Vec<HMONITOR>)).push(monitor) };
    1
}

/// A fixed UTF-16 buffer up to its terminator.
fn text(wide: &[u16]) -> &[u16] {
    &wide[..wide.iter().position(|&c| c == 0).unwrap_or(wide.len())]
}

/// Each active path's GDI device name (`\\.\DISPLAY1`), its monitor's EDID
/// name and whether it is a built-in panel. Empty when the configuration
/// changes between the two calls; the change's own message asks again.
fn targets() -> Vec<(Vec<u16>, String, bool)> {
    let (mut paths, mut modes) = (0, 0);
    if unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut paths, &mut modes) }
        != ERROR_SUCCESS
    {
        return Vec::new();
    }
    let mut path = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
    let mut mode = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
    if unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut paths,
            path.as_mut_ptr(),
            &mut modes,
            mode.as_mut_ptr(),
            std::ptr::null_mut(),
        )
    } != ERROR_SUCCESS
    {
        return Vec::new();
    }
    path.truncate(paths as usize);
    let header = |kind, size: usize, adapter, id| DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: kind,
        size: size as u32,
        adapterId: adapter,
        id,
    };
    (path.iter())
        .filter_map(|p| {
            let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                    std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>(),
                    p.sourceInfo.adapterId,
                    p.sourceInfo.id,
                ),
                ..Default::default()
            };
            if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
                return None;
            }
            let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
                header: header(
                    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                    std::mem::size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(),
                    p.targetInfo.adapterId,
                    p.targetInfo.id,
                ),
                ..Default::default()
            };
            // Bit 0 is `friendlyNameFromEdid`: without it the name is not the monitor's.
            let named = unsafe { DisplayConfigGetDeviceInfo(&mut target.header) } == 0
                && unsafe { target.flags.Anonymous.value } & 1 != 0;
            let label = if named {
                String::from_utf16_lossy(text(&target.monitorFriendlyDeviceName))
            } else {
                String::new()
            };
            let internal = super::built_in(p.targetInfo.outputTechnology);
            Some((text(&source.viewGdiDeviceName).to_vec(), label, internal))
        })
        .collect()
}

/// Every monitor, with its handle, at the Text size setting `text_size`.
fn monitors(text_size: f32) -> Vec<(HMONITOR, Monitor)> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    let list = &mut handles as *mut Vec<HMONITOR> as LPARAM;
    unsafe { EnumDisplayMonitors(std::ptr::null_mut(), std::ptr::null(), Some(each), list) };
    let targets = targets();
    (handles.into_iter())
        .filter_map(|handle| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(handle, (&mut info as *mut MONITORINFOEXW).cast()) } == 0 {
                return None;
            }
            let (mut dpi, mut unused) = (96, 96);
            if unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused) } < 0 {
                dpi = 96;
            }
            let rect = |r: RECT| [r.left, r.top, r.right, r.bottom];
            let (label, internal) = (targets.iter())
                .find(|(device, ..)| device == text(&info.szDevice))
                .map(|(_, label, internal)| (label.clone(), *internal))
                .unwrap_or_default();
            Some((
                handle,
                Monitor {
                    rect: rect(info.monitorInfo.rcMonitor),
                    work: rect(info.monitorInfo.rcWork),
                    scale: super::scale(dpi.max(1), text_size),
                    primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                    internal,
                    label,
                },
            ))
        })
        .collect()
}

impl Displays {
    fn window(&self) -> HWND {
        self.hwnd as HWND
    }

    fn client(&self) -> Option<Client> {
        let mut origin = POINT { x: 0, y: 0 };
        if unsafe { ClientToScreen(self.window(), &mut origin) } == 0 {
            return None;
        }
        let dpi = unsafe { GetDpiForWindow(self.window()) };
        (dpi != 0).then_some(Client {
            origin: [origin.x, origin.y],
            dpi,
        })
    }

    fn settings(&self) -> Option<&UISettings> {
        let made = self.settings.get_or_init(|| {
            exact_painted::com::ensure_mta().ok()?;
            UISettings::new().ok()
        });
        made.as_ref()
    }

    /// `UISettings.TextScaleFactor`, which Chromium reads for the same
    /// product (`ui/display/win/uwp_text_scale_factor.cc`): 1 or more, and 1
    /// where it cannot be read.
    fn text_size(&self) -> f32 {
        let read = self.settings().and_then(|s| s.TextScaleFactor().ok());
        read.map_or(1., |scale| (scale as f32).max(1.))
    }

    /// The monitors, and the index of the one holding most of the window.
    fn read(&self) -> (Vec<(HMONITOR, Monitor)>, Option<usize>) {
        let read = monitors(self.text_size());
        let on = unsafe { MonitorFromWindow(self.window(), MONITOR_DEFAULTTONEAREST) };
        let current = read.iter().position(|(handle, _)| *handle == on);
        (read, current)
    }

    /// Hear the window's display changes, and the Text size setting's, from
    /// the first reader on.
    fn hear(&self) {
        if self.hears.replace(true) {
            return;
        }
        if let Some(settings) = self.settings() {
            let watchers = self.watch.watchers.clone();
            let changed = TypedEventHandler::new(move |_, _| {
                tell(&watchers);
                Ok(())
            });
            // The registration lasts as long as `settings`, which this keeps.
            if let Err(error) = settings.TextScaleFactorChanged(&changed) {
                eprintln!("exact-windows: text-size changes for the displays: {error}");
            }
        }
        let on = unsafe { MonitorFromWindow(self.window(), MONITOR_DEFAULTTONEAREST) };
        self.watch.on.set(on as isize);
        let data = Rc::into_raw(self.watch.clone());
        // The window owns this reference until WM_NCDESTROY.
        if unsafe { SetWindowSubclass(self.window(), Some(heard), 0, data as usize) } == 0 {
            unsafe { drop(Rc::from_raw(data)) };
            eprintln!("exact-windows: could not observe display changes");
        }
    }
}

fn split(read: Vec<(HMONITOR, Monitor)>) -> Vec<Monitor> {
    read.into_iter().map(|(_, monitor)| monitor).collect()
}

impl exact_painted::desktop::Displays for Displays {
    fn screens(&self) -> Screens {
        let (read, current) = self.read();
        super::screens(&split(read), current, self.client())
    }

    fn pointer(&self) -> Option<Pointer> {
        let mut at = POINT { x: 0, y: 0 };
        // Fails where the process may not read the input desktop.
        if unsafe { GetCursorPos(&mut at) } == 0 {
            return None;
        }
        let (read, current) = self.read();
        let holder = unsafe { MonitorFromPoint(at, MONITOR_DEFAULTTONEAREST) };
        let on = read.iter().position(|(handle, _)| *handle == holder)?;
        super::pointer(&split(read), current, on, [at.x, at.y], self.client())
    }

    fn watch_displays(&self, session: &str, changed: Box<dyn Fn() + Send>) {
        locked(&self.watch.watchers).push((session.to_owned(), changed));
        self.hear();
    }

    fn unwatch_displays(&self, session: &str) {
        locked(&self.watch.watchers).retain(|(s, _)| s != session);
    }
}

unsafe extern "system" fn heard(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    subclass: usize,
    data: usize,
) -> isize {
    unsafe {
        let watch = data as *const Watch;
        match message {
            WM_NCDESTROY => {
                RemoveWindowSubclass(window, Some(heard), subclass);
                drop(Rc::from_raw(watch));
            }
            WM_DISPLAYCHANGE | WM_DPICHANGED => tell(&(*watch).watchers),
            WM_SETTINGCHANGE if wparam == SPI_SETWORKAREA as usize => tell(&(*watch).watchers),
            WM_WINDOWPOSCHANGED => {
                let on = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) as isize;
                if (*watch).on.replace(on) != on {
                    tell(&(*watch).watchers);
                }
            }
            _ => {}
        }
        DefSubclassProc(window, message, wparam, lparam)
    }
}
