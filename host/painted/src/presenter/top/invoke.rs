//! What opens and closes a surface of the top layer: a button's invocation
//! (`commandfor`, `popovertarget`), an action's popover command, `showModal`
//! and `close`, and a long press on a node that names a context menu.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// The button a press at `hit` invokes the top layer through: the
    /// nearest button at or above it, when it names something (a non-empty
    /// `commandfor` or `popovertarget`). With no top layer, `hit` itself where
    /// it is a popover's invoker, which is then refused
    /// ([`Presenter::invoke`]), as it always was.
    pub(crate) fn top_invoker(&self, hit: ViewId) -> Option<ViewId> {
        let kernel = self.host.kernel();
        if !self.has_windows() && !self.neutral_alerts() {
            return crate::navigation::popover_invoker(kernel, hit).then_some(hit);
        }
        let mut at = Some(hit);
        while let Some(id) = at {
            let node = kernel.node(id)?;
            if node.props.bool(PropId::Disabled) == Some(true) {
                return None;
            }
            if shape::is_button(&node) {
                let names = |prop| node.props.str(prop).is_some_and(|name| !name.is_empty());
                return (names(PropId::Commandfor) || names(PropId::Popovertarget)).then_some(id);
            }
            at = node.parent;
        }
        None
    }

    /// A press on `invoker` ([`Presenter::top_invoker`]): its own `press`
    /// runs first, as a click's listeners run before its default action,
    /// then what the surviving button invokes ([`Presenter::follow_invocation`]).
    /// The node pressed, or `None` on a host with no top layer, which
    /// refuses it.
    pub(crate) fn invoke(&mut self, invoker: ViewId, now_ms: f64, pointer: bool) -> Option<ViewId> {
        self.overlay_input(!pointer);
        if !self.has_windows() && !self.neutral_alerts() {
            self.host.log(crate::navigation::POPOVER_UNSUPPORTED);
            return None;
        }
        let key = self.host.kernel().node(invoker)?.key;
        if (self.host.runner().handlers_of(invoker)).contains(&EventKind::Press) {
            self.dispatch_press(invoker, now_ms, pointer);
        }
        self.follow_invocation(key, invoker, now_ms);
        Some(invoker)
    }

    /// After a button's `press` has run: what the button `key` holds invokes
    /// then, if it is still that node, enabled and shown (a bar's button is
    /// out of the page and still its route's to press). A popover opens or
    /// closes by its action, a dialog opens modally by `show-modal` and
    /// closes by `close`; a popover command leaves a surface open modally
    /// alone, and `close` one open as a popover.
    pub(crate) fn follow_invocation(&mut self, key: NodeKey, invoker: ViewId, now_ms: f64) {
        if !self.has_windows() && !self.neutral_alerts() {
            return;
        }
        let kernel = self.host.kernel();
        let Some(node) = kernel.node_by_key(key).filter(|node| {
            node.props.bool(PropId::Disabled) != Some(true)
                && node.style.display != exact_kernel::Display::None
                && !self.host.authored_visibility(node.id).1
        }) else {
            return;
        };
        let Some(invocation) = exact_kernel::Invocation::of(kernel, &node) else {
            return;
        };
        let (target, name) = (invocation.target, self.name_of(invocation.target));
        match (invocation.action, self.open_mode(target)) {
            (Action::Show | Action::Toggle, None) => {
                self.present(Some(target), &name, Some(invoker), None, false, now_ms);
            }
            (Action::ShowModal, None) => {
                self.present(Some(target), &name, Some(invoker), None, true, now_ms);
            }
            (Action::Hide | Action::Toggle, Some(false)) | (Action::Close, Some(true)) => {
                self.close_surface(now_ms)
            }
            _ => {}
        }
    }

    /// The `id` of the node `key` holds, for what the log says.
    pub(super) fn name_of(&self, key: NodeKey) -> String {
        let node = self.host.kernel().node_by_key(key);
        node.and_then(|node| node.props.str(PropId::Id))
            .unwrap_or_default()
            .to_string()
    }

    /// A node pressed by a contact that kept its own press feedback: the
    /// top layer's invoker where it is one, else its `press`.
    pub(in crate::presenter) fn activate(&mut self, id: ViewId, now_ms: f64) {
        self.activate_with(id, now_ms, true);
    }

    /// `id` activated by a pointer or by a key (Enter, Space, a shortcut):
    /// an invoker's `press` and then what it invokes, else its `press`.
    pub(in crate::presenter) fn activate_with(&mut self, id: ViewId, now_ms: f64, pointer: bool) {
        self.overlay_input(!pointer);
        let serial = self.top.last;
        if (self.has_windows() || self.neutral_alerts()) && self.top_invoker(id) == Some(id) {
            self.invoke(id, now_ms, pointer);
        } else {
            self.press_then_follow(id, now_ms, pointer);
        }
        if !pointer
            && self.top.overlay.enabled
            && self
                .top
                .open
                .as_ref()
                .is_some_and(|open| open.serial != serial && open.focused.is_none())
        {
            self.surface_edge(false);
        }
    }

    /// `id`'s `press`, then what it invokes if it is a button its handler
    /// gave something to invoke.
    pub(crate) fn press_then_follow(&mut self, id: ViewId, now_ms: f64, pointer: bool) {
        let key = self.host.kernel().node(id).map(|node| node.key);
        self.dispatch_press(id, now_ms, pointer);
        if let Some(key) = key {
            self.follow_invocation(key, id, now_ms);
        }
    }

    pub(in crate::presenter) fn popover_command(
        &mut self,
        command: &str,
        args: &[exact_plan::Value],
    ) {
        if !self.has_windows() && !self.neutral_alerts() {
            self.host.log(format!(
                "{command} unsupported: this host has no popover presentation"
            ));
            return;
        }
        let Some(name) = args.first().and_then(exact_plan::Value::as_str) else {
            return;
        };
        let target = self.element(name).filter(|key| {
            (self.host.kernel().node_by_key(*key))
                .is_some_and(|n| n.props.str(PropId::Popover).is_some())
        });
        let Some(target) = target else {
            self.host
                .log(format!("{command} refused: no popover has that id"));
            return;
        };
        let source_name = args.get(1).and_then(exact_plan::Value::as_str);
        let source = source_name
            .and_then(|name| self.element(name))
            .and_then(|key| self.host.kernel().node_by_key(key).map(|n| n.id));
        if source_name.is_some() && source.is_none() {
            self.host
                .log(format!("{command} refused: no source has that id"));
            return;
        }
        match self.open_mode(target) {
            Some(true) => self.host.log(format!(
                "{command} ignored: \"{name}\" is open as a modal dialog, which popover commands leave alone"
            )),
            Some(false) if command == "hidePopover" || command == "togglePopover" => {
                self.close_surface(self.host.now())
            }
            None if command != "hidePopover" => {
                self.present(Some(target), name, source, None, false, self.host.now());
            }
            _ => {}
        }
    }

    /// `showModal(id)` from an action (LLP 1115 D6): the confirmation whose
    /// `id` that is, with no invoker (`source`: the node whose input ran
    /// the action, where the focus returns).
    pub(in crate::presenter) fn show_modal(
        &mut self,
        args: &[exact_plan::Value],
        source: Option<ViewId>,
    ) {
        let Some(name) = args.first().and_then(exact_plan::Value::as_str) else {
            return self.host.log("showModal requires an element id");
        };
        let target = self.element(name);
        if target.is_some_and(|key| self.open_mode(key).is_some()) {
            return self
                .host
                .log(format!("showModal \"{name}\" ignored: it is open"));
        }
        let now = self.host.now();
        self.present(target, name, source, None, true, now);
    }

    /// `close(id)` from an action: the open surface, when it is that node
    /// open modally, closes as its cancel closes it (HTML's `close` does
    /// nothing to a dialog shown as a popover).
    pub(in crate::presenter) fn close_modal(&mut self, args: &[exact_plan::Value]) {
        let name = args.first().and_then(exact_plan::Value::as_str);
        let target = name.and_then(|name| self.element(name));
        if target.is_some_and(|key| self.open_mode(key) == Some(true)) {
            let now = self.host.now();
            self.close_surface(now);
        }
    }

    /// A finger held still at (`x`, `y`) for the platform's long-press
    /// time, which the shell measures: the nearest node at or above it that
    /// names a context menu (`contextPopover`) or hears `contextmenu` takes
    /// it. Its `contextmenu` runs first, then its menu opens where the
    /// finger is, and the touch presses nothing when it lifts. Whether a
    /// node took it. Nothing on a host with no top layer.
    pub fn long_press(&mut self, x: f32, y: f32, now_ms: f64) -> bool {
        if (!self.has_windows() && !self.neutral_alerts()) || self.page_inert() {
            return false;
        }
        let Some(hit) = self.hit(x, y) else {
            return false;
        };
        let (kernel, runner) = (self.host.kernel(), self.host.runner());
        let mut at = Some(hit);
        let mut found = None;
        while let Some(node) = at.and_then(|id| kernel.node(id)) {
            if node.props.bool(PropId::Disabled) == Some(true) {
                break;
            }
            let names = node
                .props
                .str(PropId::ContextPopover)
                .filter(|n| !n.is_empty());
            let hears = runner
                .handlers_of(node.id)
                .contains(&EventKind::Contextmenu);
            if names.is_some() || hears {
                found = Some((node.id, names.map(str::to_string), hears));
                break;
            }
            at = node.parent;
        }
        let Some((id, names, hears)) = found else {
            return false;
        };
        self.top.long_presses = self.top.long_presses.wrapping_add(1);
        if let Err(e) = self.pointer_cancel(now_ms) {
            self.host.log(e);
        }
        // The platform's own long-press click.
        self.request(Request::Haptic(Haptic::ImpactHeavy));
        if hears {
            let error = self.host.dispatch_at(id, Event::Contextmenu, now_ms);
            if let Some(e) = error.or(self.after_commit()) {
                eprintln!("exact: {e}");
            }
        }
        if let Some(name) = names {
            let menu = self.element(&name);
            self.present(menu, &name, Some(id), Some((x, y)), false, now_ms);
        }
        true
    }

    /// How many long presses a node has taken, wrapping.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn long_presses(&self) -> u32 {
        self.top.long_presses
    }
}
