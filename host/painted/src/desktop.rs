//! The host services LLP 1133.025's desktop modules use (D11 item 2): the
//! process's one hot-key handler, the taskbar button and the launch. Each
//! platform object is one per process, so the host keeps it and a package
//! reaches it through its [`Context`](crate::package::Context), keyed by the
//! calling session.
//!
//! A host that has these installs its [`Services`] before the loop starts
//! (the Windows host, `host/windows/src/desktop.rs`); this Linux host binds
//! none (LLP 1133.025 D14), so [`services`] is `None` and the packages
//! answer `NotSupportedError`.
//!
//! A hot key's press is an activation the host raises (D11 item 3): the
//! host's callback calls [`activate`], and the presenter's next turn raises
//! the registration's event with a person's origin on the runner, which a
//! package's own `emit` never gets.
//!
//! The displays and the pointer (LLP 1178 D6) are a service of their own,
//! [`Displays`], which a host installs with [`install_displays`]: the
//! Linux host binds it ([`FullScreen`]) though it binds none of the rest,
//! and the Windows host installs its monitors' (`host/windows/src/displays.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Cursor preferences exported by GTK desktops through the settings portal.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CursorSettings {
    /// Whether a focused insertion caret blinks.
    pub blink: bool,
    /// Length of the full blink cycle.
    pub cycle: std::time::Duration,
    /// Time since the last interaction before the caret stays visible.
    pub timeout: std::time::Duration,
}

#[cfg(target_os = "linux")]
impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            blink: true,
            cycle: std::time::Duration::from_millis(1200),
            timeout: std::time::Duration::from_secs(10),
        }
    }
}

/// Preferences a Wayland window follows for its lifetime.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct WindowSettings {
    /// System dark or light preference. An unset preference uses the host default.
    pub dark: Option<bool>,
    /// Desktop insertion caret preferences.
    pub cursor: CursorSettings,
}

/// Keeps the settings subscription alive until the window closes.
#[cfg(target_os = "linux")]
pub struct SettingsWatch<T> {
    /// The initial preferences available before first paint.
    pub initial: T,
    pub(crate) socket: std::os::unix::net::UnixStream,
}

#[cfg(target_os = "linux")]
impl<T> Drop for SettingsWatch<T> {
    fn drop(&mut self) {
        let _ = self.socket.shutdown(std::net::Shutdown::Both);
    }
}

/// A subscription to the XDG colour scheme.
#[cfg(target_os = "linux")]
pub type Appearance = SettingsWatch<Option<bool>>;

/// Report the initial dark or light preference and subsequent portal changes.
/// A value of `None` restores the host default.
#[cfg(target_os = "linux")]
pub fn watch_appearance(changed: impl Fn(Option<bool>) + Send + 'static) -> Option<Appearance> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok()?;
    crate::portal::watch_settings(&address, |s| s.dark, changed)
}

/// Report the colour scheme and cursor preferences together on the window's subscription.
#[cfg(target_os = "linux")]
pub fn watch_window_settings(
    changed: impl Fn(WindowSettings) + Send + 'static,
) -> Option<SettingsWatch<WindowSettings>> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok()?;
    crate::portal::watch_settings(&address, |s| s, changed)
}

/// Why a chord is not registered, in `@exact/global-shortcuts`' words.
pub type Refusal = &'static str;

/// The login item as the platform keeps it.
#[derive(Clone, Debug, PartialEq)]
pub struct Login {
    /// `enabled`, `disabled`, `requires-approval` or `unknown`.
    pub state: &'static str,
    /// Whether the registration asked for a launch with no window.
    pub hidden: bool,
}

/// What a desktop host keeps for every session of the process. Called on
/// the loop's thread.
pub trait Services {
    /// Register `keys` (an `aria-keyshortcuts` chord) for `session`'s `id`;
    /// a press raises `extension`'s `event` with `[id, {id, keys}]`. With
    /// `repeat`, a held chord raises a press per keyboard repeat.
    fn register_hotkey(
        &self,
        session: &str,
        id: &str,
        keys: &str,
        repeat: bool,
        extension: &'static str,
        event: &'static str,
    ) -> Result<(), Refusal>;
    /// `session` no longer reads `keys` for `id`.
    fn unregister_hotkey(&self, session: &str, id: &str, keys: &str);
    /// Whether `keys` could be registered now, keeping nothing.
    fn check_hotkey(&self, keys: &str) -> Result<(), Refusal>;
    /// Ask for the person's attention on the app's taskbar button or tile:
    /// `count` flashes when the author names a number, which takes the
    /// place of a critical request's flashing; `Err` says why nothing was
    /// asked.
    fn attention(&self, critical: bool, count: Option<u32>) -> Result<(), String>;
    /// The login item's registration as it stands.
    fn login(&self) -> Login;
    /// Register the login item (`Some(hidden)`) or remove it (`None`).
    fn set_login(&self, register: Option<bool>) -> Result<Login, String>;
    /// Open the system's settings for the login item.
    fn open_login_settings(&self) -> Result<(), String>;
    /// This launch was the login item's: `Some(hidden)`, read before the
    /// session booted.
    fn login_launch(&self) -> Option<bool>;
    /// Call `changed`, from any thread, when the system takes or gives back
    /// a chord it can reserve (Windows: bare Print Screen, as Snipping
    /// Tool's setting changes), until `session` is unwatched.
    fn watch_reserved(&self, session: &str, changed: Box<dyn Fn() + Send>) {
        let _ = (session, changed);
    }
    /// Stop calling `session`'s [`Services::watch_reserved`] callback.
    fn unwatch_reserved(&self, session: &str) {
        let _ = session;
    }
}

/// One display (LLP 1178 D1), by the Window Management API's fields: its
/// rectangle and work area in CSS pixels of the space whose origin is the
/// primary display's top left, y downward.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    /// `left`, `top`, `width`, `height`.
    pub rect: [f64; 4],
    /// `availLeft`, `availTop`, `availWidth`, `availHeight`: the display
    /// less the system's bars.
    pub avail: [f64; 4],
    /// `devicePixelRatio`.
    pub scale: f64,
    /// `isPrimary`.
    pub primary: bool,
    /// `isInternal`: built into the device (a laptop's panel).
    pub internal: bool,
    /// `label`, the name the system gives the display.
    pub label: String,
    /// Where the display lies in the session's viewport, in its CSS pixels
    /// (`x`, `y`, `width`, `height`; D2's `clientRect`), measured through
    /// the window's physical origin and scale; `None` with no window.
    pub client: Option<[f64; 4]>,
}

impl Screen {
    /// The package answer, with viewport geometry only on a send.
    pub fn json(&self, client: bool) -> serde_json::Value {
        use serde_json::{json, Value};
        let [left, top, width, height] = self.rect;
        let [avail_left, avail_top, avail_width, avail_height] = self.avail;
        let mut out = json!({
            "left": left, "top": top, "width": width, "height": height,
            "availLeft": avail_left, "availTop": avail_top,
            "availWidth": avail_width, "availHeight": avail_height,
            "devicePixelRatio": self.scale, "isPrimary": self.primary,
            "isInternal": self.internal, "label": self.label,
        });
        if client {
            out["clientRect"] = self.client.map(|[x, y, width, height]| {
                json!({"x": x, "y": y, "width": width, "height": height})
            }).unwrap_or(Value::Null);
        }
        out
    }
}

/// The displays, in the specification's order (by `left`, then `top`), and
/// the index of `currentScreen`: the one holding most of the session's
/// window, else the primary.
#[derive(Clone, Debug, PartialEq)]
pub struct Screens {
    /// Every display.
    pub screens: Vec<Screen>,
    /// `currentScreen`'s index in `screens`.
    pub current: usize,
}

/// Where the pointer is (LLP 1178 D2), with the displays as they were read
/// for it, so its `screen` and that screen's `clientRect` agree.
#[derive(Clone, Debug, PartialEq)]
pub struct Pointer {
    /// `screenX`, `screenY`, in [`Screen::rect`]'s space.
    pub at: [f64; 2],
    /// `clientX`, `clientY`, in the viewport's CSS pixels; `None` with no
    /// window.
    pub client: Option<[f64; 2]>,
    /// The displays, and the index of the one holding the pointer.
    pub screens: Screens,
    /// The index in `screens.screens` of the display holding the pointer.
    pub on: usize,
}

/// What a host knows of the displays and the pointer (LLP 1178 D6), which
/// `@exact/window-management` reads. Called on the loop's thread.
pub trait Displays {
    /// The displays now.
    fn screens(&self) -> Screens;
    /// The pointer now, or `None` where the host has none to report.
    fn pointer(&self) -> Option<Pointer>;
    /// Call `changed`, from any thread, when the displays or
    /// `currentScreen` change, until `session` is unwatched.
    fn watch_displays(&self, session: &str, changed: Box<dyn Fn() + Send>);
    /// Stop calling `session`'s [`Displays::watch_displays`] callback.
    fn unwatch_displays(&self, session: &str);
}

/// The one display of a host whose session fills it, as the Linux host's
/// KMS connector and its headless buffer do: the display is the viewport at
/// the presenter's scale, its work area all of it, and the pointer the one
/// the presenter draws. The presenter tells it of each change ([`surface`]).
pub struct FullScreen {
    /// The display's name: the connector's (`eDP-1`), or empty headless.
    pub label: String,
    /// Whether the connector is a built-in panel (eDP, LVDS, DSI).
    pub internal: bool,
}

/// The presenter's viewport, scale and pointer, as last told.
#[derive(Clone, Copy, Default, PartialEq)]
struct Surface {
    viewport: (f32, f32),
    scale: f32,
    pointer: Option<(f32, f32)>,
}

impl FullScreen {
    /// A display with no connector: the headless buffer's.
    pub fn headless() -> FullScreen {
        FullScreen {
            label: String::new(),
            internal: false,
        }
    }

    fn screens_of(&self, s: Surface) -> Screens {
        let (w, h) = (f64::from(s.viewport.0), f64::from(s.viewport.1));
        let rect = [0., 0., w, h];
        Screens {
            screens: vec![Screen {
                rect,
                avail: rect,
                scale: f64::from(s.scale),
                primary: true,
                internal: self.internal,
                label: self.label.clone(),
                client: Some(rect),
            }],
            current: 0,
        }
    }
}

impl Displays for FullScreen {
    fn screens(&self) -> Screens {
        self.screens_of(SURFACE.with(Cell::get))
    }

    fn pointer(&self) -> Option<Pointer> {
        let s = SURFACE.with(Cell::get);
        let (x, y) = s.pointer?;
        let at = [f64::from(x), f64::from(y)];
        Some(Pointer {
            at,
            client: Some(at),
            screens: self.screens_of(s),
            on: 0,
        })
    }

    fn watch_displays(&self, session: &str, changed: Box<dyn Fn() + Send>) {
        WATCHERS.with(|w| w.borrow_mut().push((session.to_owned(), changed)));
    }

    fn unwatch_displays(&self, session: &str) {
        WATCHERS.with(|w| w.borrow_mut().retain(|(s, _)| s != session));
    }
}

/// The presenter's viewport, scale and pointer now: [`FullScreen`]'s
/// display. A change of the viewport or the scale is a change of that
/// display, and its watchers are called.
pub(crate) fn surface(viewport: (f32, f32), scale: f32, pointer: Option<(f32, f32)>) {
    let now = Surface {
        viewport,
        scale,
        pointer,
    };
    let was = SURFACE.with(|s| s.replace(now));
    if (was.viewport, was.scale) != (now.viewport, now.scale) && was != Surface::default() {
        WATCHERS.with(|w| w.borrow().iter().for_each(|(_, changed)| changed()));
    }
}

/// The host's display service, from now on, for every session on this thread.
pub fn install_displays(displays: Rc<dyn Displays>) {
    DISPLAYS.with(|d| *d.borrow_mut() = Some(displays));
}

/// The host's display service, or `None` where the host keeps none.
pub fn displays() -> Option<Rc<dyn Displays>> {
    DISPLAYS.with(|d| d.borrow().clone())
}

/// A press the host heard, waiting for the presenter's turn.
struct Activation {
    extension: &'static str,
    event: &'static str,
    args: String,
}

/// A watcher of the displays: its session, and what to call.
type Watcher = (String, Box<dyn Fn() + Send>);

thread_local! {
    static SERVICES: RefCell<Option<Rc<dyn Services>>> = const { RefCell::new(None) };
    static DISPLAYS: RefCell<Option<Rc<dyn Displays>>> = const { RefCell::new(None) };
    static SURFACE: Cell<Surface> = Cell::new(Surface { viewport: (0., 0.), scale: 0., pointer: None });
    static WATCHERS: RefCell<Vec<Watcher>> = const { RefCell::new(Vec::new()) };
    static ACTIVATIONS: RefCell<Vec<Activation>> = const { RefCell::new(Vec::new()) };
}

/// The host's services, from now on, for every session on this thread.
pub fn install(services: Rc<dyn Services>) {
    SERVICES.with(|s| *s.borrow_mut() = Some(services));
}

/// The host's services, or `None` where the host keeps none.
pub fn services() -> Option<Rc<dyn Services>> {
    SERVICES.with(|s| s.borrow().clone())
}

/// The host's platform callback heard a person's act for `extension`'s
/// `event`: raised on the next turn with a person's origin. `args` is a
/// JSON array. The host wakes its loop after.
pub fn activate(extension: &'static str, event: &'static str, args: String) {
    ACTIVATIONS.with(|a| {
        a.borrow_mut().push(Activation {
            extension,
            event,
            args,
        })
    });
}

/// The activations heard since the last turn, raised on `runner`.
pub(crate) fn raise<D: exact_runner::DataSource>(runner: &mut exact_runner::Runner<D>) {
    let heard = ACTIVATIONS.with(|a| std::mem::take(&mut *a.borrow_mut()));
    for a in heard {
        let origin = exact_runner::CallOrigin::App;
        if let Err(e) = runner.raise_module_event(a.extension, a.event, &a.args, origin) {
            runner.log(e);
        }
    }
}

#[cfg(test)]
#[path = "desktop_tests.rs"]
mod tests;
