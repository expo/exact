//! The window's life apart from drawing. An app that keeps running
//! (`host.windows.window.keepRunning`, LLP 1133.025 D4) has its window
//! hidden by the person's close (the close button, Alt+F4, the taskbar's
//! Close window), with nothing asked, since nothing unloads; the page is
//! put away, as the back/forward cache keeps one, until the window shows
//! again. A status item in the notification area does not make the close
//! hide the window, as it does on the Mac: Windows' guidelines advise
//! against Close minimizing to the notification area
//! ([Notifications and the notification area](https://learn.microsoft.com/windows/win32/uxguide/winenv-notification)).
//! The app's own `close()` still closes it. A login item registered
//! `hidden` shows no window at its launch (D6). Its offscreen CPU frame
//! stands for first pixel. The selected painter opens when the window is
//! first ready to draw. A later launch of the app
//! (`crate::instance`) or a bare `focus()` shows it.
use super::*;
use crate::instance::{Activation, Claim, Handed, Instance};

/// DWM composes a cloaked window without putting it on the desktop. Winit's
/// visible flag stays clear until the frame is ready because setting it can
/// activate the window and take keyboard input away from the current app.
/// How long after a show is asked for its first present waits for the
/// pictures its draws asked for. On the NucBox (Ryzen AI Max+ 395) a
/// 3440 × 1440 PNG reads and decodes on a raster worker in 15 ms, and a
/// freeze overlay's two 2560 × 1440 JPEG stills land 45 to 60 ms after the
/// show begins; a capture's own PNG pixels take no decode. 100 ms covers those
/// with room, and is about what the overlay of expo/exact#1315 took to reveal
/// before its decodes left the presentation thread (93 ms): no reveal waits
/// longer, and a picture still loading then lands in a later frame.
pub(super) const REVEAL_PICTURES: std::time::Duration = std::time::Duration::from_millis(100);

pub(super) struct PendingShow {
    /// When the show was asked for: its pictures' wait counts from here.
    pub(super) began: std::time::Instant,
    window: Rc<Window>,
    hwnd: windows_sys::Win32::Foundation::HWND,
}

impl PendingShow {
    /// `minimum` is the client's least size in CSS pixels.
    #[allow(unsafe_code)]
    fn begin(window: &Rc<Window>, minimum: (f32, f32)) -> Option<Self> {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
        let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        let pending = Self {
            began: std::time::Instant::now(),
            window: window.clone(),
            hwnd: handle.hwnd.get() as _,
        };
        let shown = crate::placement::shown_now(pending.hwnd as isize, minimum);
        pending.cloak(true).ok()?;
        unsafe { ShowWindow(pending.hwnd, SW_SHOWNOACTIVATE) };
        if let Some(shown) = shown {
            crate::placement::take_dpi(pending.hwnd as isize, shown);
        }
        Some(pending)
    }

    #[allow(unsafe_code)]
    fn cloak(&self, enabled: bool) -> Result<(), String> {
        use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK};
        let value = i32::from(enabled);
        let hr = unsafe {
            DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_CLOAK as u32,
                (&value as *const i32).cast(),
                size_of_val(&value) as u32,
            )
        };
        if hr < 0 {
            return Err(format!("DWM could not change the window's cloak ({hr:#x})"));
        }
        Ok(())
    }

    pub(super) fn reveal(self) -> Result<(), String> {
        self.cloak(false)?;
        self.window.set_visible(true);
        self.window.focus_window();
        Ok(())
    }

    #[allow(unsafe_code)]
    fn cancel(self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
        // Winit still considers the window hidden, so undo the native show first.
        unsafe { ShowWindow(self.hwnd, SW_HIDE) };
        if let Err(error) = self.cloak(false) {
            eprintln!("exact-windows: {error}");
        }
    }
}

/// This launch's claim to be the app's one instance (D6), or the exit code
/// it ends with: 0 once the running instance has its arguments, 1 when that
/// instance did not answer. A measured run (`frame_limit`, the readiness
/// probe) stands alone.
pub(super) fn claim(
    measured: bool,
    compat: &str,
    name: &str,
    args: &[String],
) -> Result<Option<Instance>, i32> {
    if measured || std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
        return Ok(None);
    }
    match crate::instance::claim(&crate::instance::key(compat, name), args) {
        Claim::First(instance) => Ok(Some(instance)),
        Claim::Handed => Err(0),
        Claim::Busy => {
            eprintln!("exact-windows: {name} is running and did not answer; this launch ends");
            Err(1)
        }
        Claim::Alone => Ok(None),
    }
}

/// The initial location boots the plan. Its document fields exist after
/// mount, when the same delivery as a subsequent launch can take them.
pub(super) fn cold_documents(config: &mut Config, compat: &str, args: &[String]) -> Vec<String> {
    let Handed::Open(items) = crate::instance::handed(args) else {
        return Vec::new();
    };
    let mut documents = Vec::new();
    let mut location = None;
    for item in items {
        match item {
            Activation::Document(path) => documents.push(path),
            Activation::Location(href) => location = Some(href),
        }
    }
    if !documents.is_empty() {
        config.launch = app::launch_location(
            location
                .into_iter()
                .chain(std::env::var("EXACT_LAUNCH_URL").ok()),
            compat,
        );
    }
    documents
}

fn deliver_document<D: DataSource>(p: &mut Presenter<D>, path: String) {
    if let Err(refused) = p.open_document(path) {
        eprintln!("exact-windows: {refused}");
        p.host_mut().log(refused);
    }
}

/// The app's bare `focus()` and `hideWindow()`, carried to the loop's next
/// turn, since the presenter asks them from inside its own.
pub(super) fn window_commands<D: DataSource>(
    p: &mut Presenter<D>,
    proxy: &EventLoopProxy<crate::accessibility::Event>,
) {
    let focus = proxy.clone();
    p.set_window_focuser(move || drop(focus.send_event(crate::accessibility::Event::Focus)));
    let hide = proxy.clone();
    p.set_window_hider(move || drop(hide.send_event(crate::accessibility::Event::Hide)));
}

impl<D: DataSource + Default + 'static> App<D> {
    /// Whether the person's close hides the window: only under
    /// `keepRunning`.
    pub(super) fn hides_on_close(&self) -> bool {
        self.keep_running
    }

    /// The app's `hideWindow()`: the window put away as the person's close
    /// puts it away; refused where that close ends the app, as on the Mac.
    pub(super) fn hide_by_app(&mut self) {
        if self.hides_on_close() {
            return self.hide();
        }
        if let Some(p) = &mut self.presenter {
            p.host_mut().log("hideWindow refused: the app ends without its window; it keeps running with host.windows.window.keepRunning");
        }
    }

    /// Nothing of the window can be seen: minimized, covered or hidden.
    pub(super) fn unseen(&self) -> bool {
        self.minimized || self.occluded || self.hidden
    }

    /// The window closes: the app's `close()`, or the person's close, which
    /// hides the window of an app that keeps running and otherwise asks
    /// `beforeunload` first (studio diary R17), as the Mac does; a handler
    /// that prevents it keeps the window, and the app's own question ends
    /// in its `close()`, which is not asked again.
    pub(super) fn close_requested(&mut self, events: &ActiveEventLoop) {
        if !self.closing && self.hides_on_close() {
            return self.hide();
        }
        if let Some(p) = self.presenter.as_mut().filter(|_| !self.closing) {
            p.advance(self.started.elapsed().as_secs_f64() * 1000.);
            let now = p.host().now();
            if !p.before_unload(now) {
                // What the handler committed (its question) is drawn.
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
                return;
            }
        }
        // The window is going: its hatch, then the app's, ends.
        if let Some(p) = &mut self.presenter {
            p.end_hatches();
            p.end_packages();
        }
        events.exit();
    }

    /// The window hidden by the person's close, the session running on.
    pub(super) fn hide(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        if let Some(pending) = self.pending_show.take() {
            pending.cancel();
        }
        window.set_visible(false);
        self.hidden = true;
        self.caret.clear();
        self.tooltip.clear();
        self.tell_page(|page| {
            page.hidden = true;
            page.persisted = true;
        });
        self.surface = None;
        if let Some(p) = &mut self.presenter {
            p.suspend_window_painter(self.painter);
        }
    }

    /// The app's bare `focus()`, or a later launch: the window shown if it
    /// is hidden, restored if minimized, and brought to the front. Windows
    /// lets a process take the foreground only while it has the person's
    /// latest input, or was handed it (a later launch hands it,
    /// `crate::instance`); otherwise the taskbar button flashes instead
    /// ([SetForegroundWindow](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setforegroundwindow)).
    pub(super) fn bring_forward(&mut self) {
        let Some(window) = self.window.clone() else {
            return;
        };
        if self.hidden {
            if let Some(p) = &mut self.presenter {
                p.prepare_window_show();
            }
            self.hidden = false;
            self.pending_show = PendingShow::begin(&window, self.min_size);
            if self.pending_show.is_none() {
                window.set_visible(true);
            }
            // A page put away comes back (`pageshow` with `persisted`); a
            // hidden launch's window shows for the first time, no restore.
            self.tell_page(|page| {
                page.hidden = false;
                if std::mem::take(&mut page.persisted) {
                    page.restores = page.restores.wrapping_add(1);
                }
            });
            if let Some(p) = &mut self.presenter {
                p.repaint();
            }
        }
        if self.pending_show.is_some() {
            window.request_redraw();
            return;
        }
        if window.is_minimized() == Some(true) {
            window.set_minimized(false);
        }
        window.focus_window();
        window.request_redraw();
    }

    /// The page's facts as the window has them: a Windows window that is
    /// not showing is not the app in the background (LLP 1133.025 D11 item 7).
    pub(super) fn tell_page(&mut self, change: impl FnOnce(&mut exact_runner::Page)) {
        let unseen = self.unseen();
        let Some(p) = &mut self.presenter else {
            return;
        };
        let mut page = p.host().runner().page();
        change(&mut page);
        page.background = Some(false);
        if let Some(error) = p.set_page(page) {
            eprintln!("exact-windows: page: {error}");
        }
        p.surface_lifecycle(unseen, !self.focused);
    }

    /// While the window is hidden: a frame painted to pixels that nothing
    /// presents is the launch's first pixel (LLP 1133.025 D6), since the
    /// window's composition takes no present while it is not visible; the
    /// data module, its global shortcuts and timers start from it. The
    /// window's own picture is drawn when it shows (`draw`).
    pub(super) fn unshown_first_pixel(&mut self) {
        if let Some(p) = self.presenter.as_mut().filter(|_| self.hidden) {
            p.first_pixel_unshown();
        }
    }

    /// What later launches of the app asked of it.
    pub(super) fn handed_launches(&mut self) {
        let Some(p) = &mut self.presenter else { return };
        for path in std::mem::take(&mut self.cold_documents) {
            deliver_document(p, path);
        }
        let Some(launches) = self.instance.as_ref().map(Instance::take) else {
            return;
        };
        for args in launches {
            match crate::instance::handed(&args) {
                Handed::Nothing => {}
                Handed::Show => self.bring_forward(),
                Handed::Open(items) => {
                    self.bring_forward();
                    if let Some(p) = &mut self.presenter {
                        p.advance(self.started.elapsed().as_secs_f64() * 1000.);
                        for item in items {
                            match item {
                                Activation::Document(path) => deliver_document(p, path),
                                Activation::Location(href) => {
                                    if let Some(refused) = p.open_location(&href, p.host().now()) {
                                        p.host_mut().log(refused);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
