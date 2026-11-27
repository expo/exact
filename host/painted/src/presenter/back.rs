//! System Back, in order (LLP 1115.001 D6; LLP 1115 D5, "Back is always
//! there"): the route's part of it, which is the rule the iOS host follows
//! (`NavigationBack.way`), over the same runner.
//!
//! From the navigation root's selected route, Back goes to the first of:
//!
//! 1. **the authored Back control**: an element of that route whose `id` is
//!    the root's `navigationBack`, that takes `press` and is enabled (the
//!    lowest view id among several). It is pressed. A route that declares
//!    one that does not resolve (disabled, or no `press`) refuses Back while
//!    there is a visit beneath it: the author's say;
//! 2. **the root's `navigate`**, with the location of the visit beneath, for
//!    a route that declares no control;
//! 3. **the runner's own `back`** of the router, only for a root with no
//!    `navigate` handler.
//!
//! With no navigation root, or no visit beneath and no control, nothing
//! takes it, and the shell leaves the app. A node that merely has
//! `id="back"` is not a Back control: the root names it.
//!
//! Who asks: Android's system Back (`canvas/keys.rs`), and on a desktop
//! (LLP 1115.000 D5.4) a mouse's back button and the keys a desktop goes
//! Back by ([`Presenter::back_key`]), whose default action it is. A mouse's
//! forward button goes Forward ([`Presenter::system_forward`], D5.5).
//!
//! Before the route come the soft keyboard, which Android closes itself, and
//! the top layer: an open menu or alert closes first (D6 step 2, D10;
//! `top.rs`), at the head of [`Presenter::system_back`]. On Android the
//! surface is a window of its own, so the system sends Back to it and the
//! shell says it was dismissed; this is the same step for a Back that
//! reaches the presenter. [`Presenter::back_goes`] is what the shell's
//! back callback is enabled by: the presenter tells the shell whenever it
//! changes (`requests.rs`, D5), so Back at the first screen is the system's.
use super::*;

/// How Back goes from the selected route.
#[derive(Debug, Clone, PartialEq)]
enum Back {
    /// Press this authored control.
    Press(ViewId),
    /// Deliver this location to this navigation root's `navigate`.
    Navigate(ViewId, String),
    /// The runner's `back` of the router, from this visit.
    Pop(u64),
    /// The route declares a Back control that does not resolve.
    Refused,
}

/// The open Exact layers and the selected sheet, if the route presents one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct BackLayers {
    pub(crate) route: bool,
    pub(crate) sheet: ViewId,
    pub(crate) stacked_sheets: Vec<ViewId>,
    pub(crate) popover: ViewId,
    pub(crate) menu: ViewId,
    pub(crate) window: u32,
    opening: Vec<(i32, u32)>,
}

impl BackLayers {
    fn words(&self) -> Vec<i32> {
        let (active, id) = self.target();
        let mut out = vec![i32::from(self.route), active, id as i32];
        for &id in &self.stacked_sheets {
            out.extend([2, id as i32]);
        }
        for (kind, id) in [(3, self.popover), (4, self.menu), (5, self.window)]
            .into_iter()
            .chain(self.opening.iter().copied())
        {
            if id != 0 {
                out.extend([kind, id as i32]);
            }
        }
        out
    }

    fn target(&self) -> (i32, u32) {
        if let Some(&opening) = self.opening.last() {
            opening
        } else if self.window != 0 {
            (5, self.window)
        } else if self.menu != 0 {
            (4, self.menu)
        } else if self.popover != 0 {
            (3, self.popover)
        } else if self.sheet != 0 {
            (2, self.sheet)
        } else if self.route {
            (1, 1)
        } else {
            (0, 0)
        }
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn takes(&self, kind: i32, id: ViewId) -> bool {
        self.target() == (kind, id)
    }
}

impl<D: DataSource> Presenter<D> {
    /// The navigation root that shows: the first node in tree order that
    /// carries `navigationBack` (lowering gives the first root with a
    /// `navigationKey` one, empty where the author named no control).
    pub(super) fn navigation_root(&self) -> Option<ViewId> {
        let kernel = self.host.kernel();
        if !kernel.has_prop(PropId::NavigationBack) {
            return None;
        }
        kernel
            .preorder_where(&self.host.runner().roots(), |_, props| {
                props.str(PropId::NavigationBack).is_some()
            })
            .into_iter()
            .find(|id| !self.host.route_visibility(*id).1)
    }

    /// How Back goes from the selected route now, resolved at use.
    fn back(&self) -> Option<Back> {
        let kernel = self.host.kernel();
        let root = self.navigation_root()?;
        let node = kernel.node(root)?;
        let name = node.props.str(PropId::NavigationBack)?;
        let selected = node.props.str(PropId::NavigationKey)?;
        // The selected route: the route under the root (its child, or a
        // tabpanel's) whose key is the root's.
        let route = kernel
            .preorder_where(&[root], |_, props| {
                props.str(PropId::NavigationKey) == Some(selected)
            })
            .into_iter()
            .find(|id| *id != root)?;
        let within = |mut id: ViewId| loop {
            if id == route {
                break true;
            }
            match kernel.node(id).and_then(|n| n.parent) {
                Some(parent) => id = parent,
                None => break false,
            }
        };
        let declared: Vec<ViewId> = match name.is_empty() {
            true => Vec::new(),
            false => kernel
                .find_by_id(name)
                .iter()
                .filter_map(|key| kernel.node_by_key(*key))
                .map(|n| n.id)
                .filter(|id| within(*id))
                .collect(),
        };
        let control = declared
            .iter()
            .copied()
            .filter(|id| {
                // It hears `press`, or invokes something (a confirmation it asks first).
                let pressable = (self.host.runner().handlers_of(*id)).contains(&EventKind::Press)
                    || kernel
                        .node(*id)
                        .is_some_and(|n| exact_kernel::Invocation::of(kernel, &n).is_some());
                let disabled = kernel
                    .node(*id)
                    .is_some_and(|n| n.props.bool(PropId::Disabled) == Some(true));
                // A control its shell's bar stands for (the Up arrow's) is
                // still the route's own: only its route's state counts.
                pressable && !disabled && !self.host.authored_visibility(*id).1
            })
            .min();
        if let Some(control) = control {
            return Some(Back::Press(control));
        }
        let visit = selected.parse::<u64>().ok()?;
        let beneath = self
            .host
            .runner()
            .location_beneath(visit)
            .filter(|location| !location.is_empty())?;
        if !declared.is_empty() {
            return Some(Back::Refused);
        }
        let hears = (self.host.runner().handlers_of(root)).contains(&EventKind::Navigate);
        Some(match hears {
            true => Back::Navigate(root, beneath),
            false => Back::Pop(visit),
        })
    }

    /// Whether the key `name` (`KeyboardEvent.key`), with the modifiers held
    /// now, is one a desktop goes Back by (LLP 1115.000 D5.4): Alt+Left, as a
    /// browser and GTK's and Qt's apps take it, or a keyboard's own Back key
    /// (the web's `BrowserBack`, evdev's `KEY_BACK`) whatever is held.
    pub(crate) fn back_key(&self, name: &str) -> bool {
        let held = self.modifiers();
        name == "BrowserBack"
            || name == "ArrowLeft" && held.alt && !(held.shift || held.ctrl || held.meta)
    }

    /// The same for Forward (LLP 1115.000 D5.5): Alt+Right, or a keyboard's
    /// Forward key (`BrowserForward`, evdev's `KEY_FORWARD`).
    pub(crate) fn forward_key(&self, name: &str) -> bool {
        let held = self.modifiers();
        name == "BrowserForward"
            || name == "ArrowRight" && held.alt && !(held.shift || held.ctrl || held.meta)
    }

    /// A Back key's keydown, after the page's shortcuts: Back is its default
    /// action, as a browser's Alt+Left is, and a Forward key's is Forward. The `key` handlers at the focus
    /// and above hear it first, and one that prevents it keeps the key (an
    /// editor's own Alt+Left); a field's caret never takes it. A held key's
    /// repeats go back no further. With nowhere to go back to nothing
    /// happens: a desktop app is not left by its Back key.
    pub(crate) fn back_key_down(&mut self, name: &str, code: &str, repeat: bool, now_ms: f64) {
        let kernel = self.host.kernel();
        // With nothing focused the root's handlers hear it (`key_event`).
        let hears = self.focus.is_none_or(|id| {
            self.display.allows(kernel, id)
                && kernel
                    .node(id)
                    .is_some_and(|n| !crate::surfaces::controls::disabled_control(&n))
        });
        if hears {
            let (error, prevented) = self.key_event(EventKind::Key, name, code, repeat, now_ms);
            if let Some(e) = error {
                eprintln!("exact: {e}");
            }
            if prevented {
                return;
            }
        }
        if !repeat && self.forward_key(name) {
            self.system_forward(now_ms);
        } else if !repeat {
            self.system_back(now_ms);
        }
    }

    /// Whether Back would go to the route now, as D6 orders it, and could
    /// take it away: no surface of the top layer takes it first and the
    /// route's Back control is not refused. What a predictive Back may
    /// preview (`stage/touch.rs`); a handler can still keep the route.
    pub(super) fn back_previewable(&self) -> bool {
        !self.surface_takes_back()
            && matches!(
                self.back(),
                Some(Back::Press(_) | Back::Navigate(..) | Back::Pop(_))
            )
    }

    /// Whether system Back would be taken now: what a shell's back callback
    /// is enabled by, so that Back with nowhere to go leaves the app with
    /// the system's own animation (D6).
    pub fn back_goes(&mut self) -> bool {
        self.surface_takes_back() || self.back().is_some() || self.remote_back_target().is_some()
    }

    /// The first hosted view needs the current identities before it registers a callback.
    /// A replacement activity starts with the ordinary one-word Back request again.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn hosted_back(&mut self, notify: Option<fn(&[i32])>) {
        self.requests.back = None;
        self.requests.hosted_back = notify;
        self.requests.back_layers = None;
        self.back_changed();
    }

    /// Admit a layer on the reader's main thread before returning to the presenter.
    /// Asynchronous window and frame reports carry no dispatcher registrations.
    pub(super) fn update_hosted_back(&mut self) {
        let Some(notify) = self.requests.hosted_back else {
            return;
        };
        let layers = self.back_layers();
        if self.requests.back_layers.as_ref() != Some(&layers) {
            notify(&layers.words());
            self.requests.back_layers = Some(layers);
        }
    }

    /// A replacement requests its own callback before closing the old surface. A blur
    /// handler during that close can itself request a presentation.
    pub(super) fn request_back_layer(&mut self, kind: i32) -> Option<u32> {
        self.requests.hosted_back?;
        let serial = self.next_surface_serial();
        self.requests.opening_back_layers.push((kind, serial));
        self.update_hosted_back();
        Some(serial)
    }

    pub(super) fn resolve_back_layer(&mut self, request: Option<u32>) {
        if let Some(serial) = request {
            let opening = self.requests.opening_back_layers.pop();
            debug_assert_eq!(opening.map(|(_, id)| id), Some(serial));
        }
        self.update_hosted_back();
    }

    pub(crate) fn back_layers(&mut self) -> BackLayers {
        let route = self.back().is_some() || self.remote_back_target().is_some();
        let (stacked_sheets, selected) = self.back_sheets();
        let sheet = selected.filter(|_| route);
        BackLayers {
            route,
            sheet: sheet.unwrap_or_default(),
            stacked_sheets,
            popover: self
                .alert
                .as_ref()
                .filter(|_| self.alert_root().is_some() && self.alert_takes_back())
                .map_or(0, |open| open.serial),
            menu: self.menu.unwrap_or_default(),
            window: self.surface().map_or(0, |window| window.serial),
            opening: self.requests.opening_back_layers.clone(),
        }
    }

    /// Whether an open surface of the top layer hears Back: not a manual
    /// popover, whose page keeps Back (HTML: it has no close watcher).
    fn surface_takes_back(&self) -> bool {
        self.menu.is_some() || self.shape_surface_open() || self.alert_takes_back()
    }

    /// The agent's `back` (LLP 1116.006 D4): system Back, as a desktop's
    /// Back key takes it, answered with what it went to: `dialog` while a
    /// surface of the top layer is open (the web driver's word), else
    /// `route`, and why, when the surface stayed open or the route's Back
    /// control refused it (journaled, as a key's is). `None` with nowhere
    /// to go back to.
    pub(crate) fn agent_back(&mut self, now_ms: f64) -> Option<serde_json::Value> {
        let (what, refused) = if self.surface_takes_back() {
            self.system_back(now_ms);
            ("dialog", self.surface_takes_back().then_some("the surface stayed open: its closedby is \"none\", it is a manual popover, or a `cancel` handler prevented the close"))
        } else {
            let refused = matches!(self.back()?, Back::Refused);
            self.system_back(now_ms);
            (
                "route",
                refused.then_some(
                    "the route's navigationBack control is disabled or has no press handler",
                ),
            )
        };
        let mut answer = serde_json::json!({ "back": what, "delivery": "platform" });
        if let Some(why) = refused {
            answer["refused"] = why.into();
        }
        Some(answer)
    }

    /// System Back at `now_ms`: whether the app took it. `false` is the
    /// shell's to act on (Android finishes the activity).
    pub fn system_back(&mut self, now_ms: f64) -> bool {
        // D6 step 2: an open select's menu, or an open surface of the top
        // layer (D10), closes before any route is asked, where its own rule
        // lets Back close it.
        if self.close_menu() || self.dismiss_surface(now_ms) {
            return true;
        }
        self.route_back(now_ms)
    }

    /// A stale callback may finish its own preview, but cannot start or move another layer's.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn dispatched_progress(
        &mut self,
        kind: i32,
        id: ViewId,
        phase: i32,
        progress: f32,
        edge: i32,
        now_ms: f64,
    ) -> bool {
        if !self.back_layers().takes(kind, id) {
            if self.back_previewing() {
                self.back_progress(2, 0.0, edge, now_ms);
            }
            return false;
        }
        if phase != 0 && !self.back_previewing() {
            return false;
        }
        self.back_progress(phase, progress, edge, now_ms)
    }

    /// Commit only the callback AndroidX selected. The reader and presenter run on different
    /// threads, so a layer can disappear between receiving Back and executing it.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn dispatched_back(&mut self, kind: i32, id: ViewId, now_ms: f64) -> i32 {
        let layers = self.back_layers();
        if !layers.takes(kind, id) {
            if self.back_previewing() {
                self.back_progress(2, 0.0, 0, now_ms);
            }
            // Redispatch on the reader with the targets this turn observed.
            self.update_hosted_back();
            return -1;
        }
        let taken = match kind {
            1 | 2 => self.route_back(now_ms),
            3 | 5 => self.dismiss_surface(now_ms),
            4 => self.close_menu(),
            _ => unreachable!(),
        };
        i32::from(taken)
    }

    fn route_back(&mut self, now_ms: f64) -> bool {
        let Some(back) = self.back() else {
            // A TV's remote has no Escape key: its Back is one (LLP 1115.001.003 D3).
            return self.remote_back(now_ms);
        };
        let error = match back {
            Back::Press(control) => {
                self.activate_with(control, now_ms, false);
                None
            }
            Back::Navigate(root, location) => self
                .host
                .dispatch_at(root, Event::Navigate(location), now_ms)
                .or(self.after_commit()),
            Back::Pop(visit) => match self.host.host_back(visit, now_ms) {
                Ok(_) => self.after_commit(),
                Err(error) => Some(error),
            },
            Back::Refused => Some(
                "system Back refused: the route's navigationBack control is disabled or has no press handler"
                    .into(),
            ),
        };
        if let Some(error) = error {
            self.host.log(error);
        }
        self.stage_back_done(now_ms);
        true
    }

    /// The platform's Forward (LLP 1115.000 D5.5): to the location Back
    /// left on the selected stack ([`Runner::location_ahead`]), delivered
    /// as the web host delivers a browser's Forward (LLP 1038 D7): to the
    /// navigation root's `navigate`, else by the runner's own `go`. Whether
    /// it went: not with nothing ahead, nor while a surface of the top
    /// layer is open, which Forward does not close.
    pub fn system_forward(&mut self, now_ms: f64) -> bool {
        if self.surface_takes_back() {
            return false;
        }
        if self.host.runner().location_ahead().is_none() {
            return false;
        }
        let Some(root) = self.navigation_root() else {
            return false;
        };
        let error = self.host.host_forward(root, now_ms).or(self.after_commit());
        if let Some(error) = error {
            self.host.log(error);
        }
        true
    }
}

#[cfg(test)]
#[path = "back_tests.rs"]
mod tests;
