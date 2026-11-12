//! The window's life apart from drawing. An app that keeps running
//! (`host.windows.window.keepRunning`, LLP 1133.025 D4) has its window
//! hidden by the person's close (the close button, Alt+F4, the taskbar's
//! Close window), with nothing asked, since nothing unloads; the page is
//! put away, as the back/forward cache keeps one, until the window shows
//! again. The app's own `close()` still closes it. A login item registered
//! `hidden` shows no window at its launch (D6): a frame painted to pixels
//! that nothing presents is the launch's first pixel, and nothing is drawn
//! into the window until it shows. A later launch of the app
//! (`crate::instance`) or a bare `focus()` shows it.
use super::*;
use crate::instance::{Claim, Handed, Instance};

/// This launch's claim to be the app's one instance (D6), or the exit code
/// it ends with: 0 once the running instance has its arguments, 1 when that
/// instance did not answer. A measured run (`frame_limit`, the readiness
/// probe) stands alone.
pub(super) fn claim(measured: bool, compat: &str, name: &str) -> Result<Option<Instance>, i32> {
    if measured || std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
        return Ok(None);
    }
    let args: Vec<String> = std::env::args().collect();
    match crate::instance::claim(&crate::instance::key(compat, name), &args) {
        Claim::First(instance) => Ok(Some(instance)),
        Claim::Handed => Err(0),
        Claim::Busy => {
            eprintln!("exact-windows: {name} is running and did not answer; this launch ends");
            Err(1)
        }
        Claim::Alone => Ok(None),
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
    /// The app's `hideWindow()`: the window put away as the person's close
    /// puts it away under `keepRunning`; refused where the app would end
    /// without its window, as on the Mac.
    pub(super) fn hide_by_app(&mut self) {
        if self.keep_running {
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
        if !self.closing && self.keep_running {
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
        window.set_visible(false);
        self.hidden = true;
        self.caret.clear();
        self.tooltip.clear();
        self.tell_page(|page| {
            page.hidden = true;
            page.persisted = true;
        });
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
            self.hidden = false;
            window.set_visible(true);
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

    /// Whether this launch shows no window: the login item's, registered
    /// `hidden` (LLP 1133.025 D6). A measured run always shows its window.
    pub(super) fn launches_hidden(&self) -> bool {
        let args: Vec<String> = std::env::args().collect();
        self.frame_limit.is_none() && crate::desktop::login_launch(&args) == Some(true)
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
        let Some(launches) = self.instance.as_ref().map(Instance::take) else {
            return;
        };
        for args in launches {
            match crate::instance::handed(&args) {
                Handed::Nothing => {}
                Handed::Show => self.bring_forward(),
                Handed::Open(href) => {
                    self.bring_forward();
                    if let Some(p) = &mut self.presenter {
                        p.advance(self.started.elapsed().as_secs_f64() * 1000.);
                        let now = p.host().now();
                        if let Some(refused) = p.open_location(&href, now) {
                            p.host_mut().log(refused);
                        }
                    }
                }
            }
        }
    }
}
