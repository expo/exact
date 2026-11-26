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
    /// finger is, and the touch presses nothing when it lifts. A node with
    /// `draggable` nearer the finger, or with no menu at all, takes it
    /// instead for the platform's drag (LLP 1094.000 D7, D8). One with both
    /// opens its menu, and the finger moving on lifts its drag
    /// ([`Self::lift_moved`]). Whether a node took it. Nothing on a host
    /// with no top layer.
    pub fn long_press(&mut self, x: f32, y: f32, now_ms: f64) -> bool {
        let menu = self.context_menu_at(x, y);
        let source = self.lift_source_at(x, y);
        if self.recognize_arrange_hold(now_ms) {
            return true;
        }
        if let Some(source) = source {
            if menu
                .as_ref()
                .is_none_or(|(id, ..)| self.inside(source, *id))
                && self.drag_out(source, x, y, now_ms, false)
            {
                self.top.long_presses = self.top.long_presses.wrapping_add(1);
                // The platform's own long-click feedback, as the menu's below.
                self.request(Request::Haptic(Haptic::ImpactHeavy));
                return true;
            }
        }
        self.top.lift = source.map(|source| (source, x, y));
        self.open_context_menu(menu, x, y, now_ms)
    }

    pub(in crate::presenter) fn arrange_hold_at(
        &mut self,
        x: f32,
        y: f32,
    ) -> Option<exact_runner::ReorderBinding> {
        let hit = self.hit(x, y)?;
        let super::super::contact::Candidate::Arrange(binding) =
            self.arrange_candidate(self.host.kernel().node(hit)?.key)?
        else {
            return None;
        };
        let handle = self.host.kernel().node_by_key(binding.handle)?.id;
        let menu = self.context_menu_at(x, y);
        let source = self.lift_source_at(x, y);
        (!menu.is_some_and(|(id, ..)| self.inside(id, handle))
            && !source.is_some_and(|id| self.inside(id, handle)))
        .then_some(binding)
    }

    pub(crate) fn recognize_arrange_hold(&mut self, now: f64) -> bool {
        let Some((binding, (x, y))) = self.pending_arrange() else {
            return false;
        };
        if self.arrange_hold_at(x, y) != Some(binding) || !self.lift_arrange(binding, now) {
            return false;
        }
        self.top.long_presses = self.top.long_presses.wrapping_add(1);
        self.request(Request::Haptic(Haptic::ImpactHeavy));
        true
    }

    /// The context menu a long press at (`x`, `y`) found, opened: what a
    /// finger held there opens when no drag takes it, and what an
    /// assistive service's long click opens. Whether there was one.
    pub(in crate::presenter) fn open_context_menu(
        &mut self,
        menu: Option<(ViewId, Option<String>, bool)>,
        x: f32,
        y: f32,
        now_ms: f64,
    ) -> bool {
        let Some((id, names, hears)) = menu else {
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

    /// The node a long press at (`x`, `y`) would give to: its id, the
    /// context menu it names and whether it hears `contextmenu`. The
    /// Android reader asks it with the hit reply, so that a hosted view
    /// under the finger does not start its own long click for a press
    /// exact2 takes.
    pub(in crate::presenter) fn context_menu_at(
        &mut self,
        x: f32,
        y: f32,
    ) -> Option<(ViewId, Option<String>, bool)> {
        if (!self.has_windows() && !self.neutral_alerts()) || self.page_inert() {
            return None;
        }
        let hit = self.hit(x, y)?;
        let (kernel, runner) = (self.host.kernel(), self.host.runner());
        let mut at = Some(hit);
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
                return Some((node.id, names.map(str::to_string), hears));
            }
            at = node.parent;
        }
        None
    }

    /// A popover's inset anchor point from its authored `left` or `top` (LLP 1021 D2).
    pub(super) fn popover_inset_point(
        &self,
        target: Option<NodeKey>,
        modal: bool,
    ) -> Option<(f32, f32)> {
        let node = self.host.kernel().node_by_key(target?)?;
        let style = node.computed_style(exact_kernel::StyleMask::INHERITED);
        (!modal && node.props.str(PropId::Popover).is_some())
            .then(|| area::inset_point(&style, &self.host.kernel().env_of(node.id), self.viewport))
            .flatten()
    }

    /// Dispatch `toggle` (`closed` -> `open`, preserving any queued `old_open`)
    /// before reading a popover's shape when it currently has no rows (e.g. rows
    /// guarded by `if open` mounted by its own `toggle` handler), matching HTML
    /// and iOS pull-down menus (#1877). Closes any other open surface first and
    /// delivers its close toggle so replacement ordering is preserved.
    pub(super) fn eager_open_popover_toggle(
        &mut self,
        target: Option<NodeKey>,
        invoker: Option<ViewId>,
        now_ms: f64,
    ) -> (bool, Option<NodeKey>) {
        let Some(key) = target else {
            return (false, None);
        };
        if self.open_mode(key).is_some() {
            return (false, None);
        }
        let listens = self.host.kernel().node_by_key(key).is_some_and(|n| {
            !n.parent.is_some_and(|p| self.host.route_visibility(p).1)
                && n.props.str(PropId::Popover).is_some()
                && self
                    .host
                    .runner()
                    .handlers_of(n.id)
                    .contains(&EventKind::Toggle)
        });
        if !listens {
            return (false, None);
        }
        let replaced_restore = if self.top_open() {
            let restore = self.replace_surface(invoker, now_ms);
            self.dispatch_toggles();
            if self.top_open() {
                return (false, restore);
            }
            restore
        } else {
            None
        };
        let Some(id) = self.host.kernel().node_by_key(key).and_then(|n| {
            (!n.parent.is_some_and(|p| self.host.route_visibility(p).1)
                && n.props.str(PropId::Popover).is_some())
            .then_some(n.id)
        }) else {
            return (false, replaced_restore);
        };
        // Preserve any prior queued `old_open` (e.g. hide then show in one turn).
        let old_open = self
            .top
            .toggles
            .iter()
            .position(|(k, _, _)| *k == key)
            .is_some_and(|idx| self.top.toggles.remove(idx).1);
        if let Some(error) = self
            .host
            .dispatch_at(
                id,
                Event::Toggle {
                    old_open,
                    new_open: true,
                },
                now_ms,
            )
            .or(self.after_commit())
        {
            self.host.log(error);
        }
        (true, replaced_restore)
    }

    /// Finish toggle accounting after an eager open toggle: drop duplicate open
    /// toggles only while the target remains open, and otherwise deliver a
    /// `open -> closed` toggle so author state reverts cleanly.
    pub(super) fn settle_eager_toggle(&mut self, key: Option<NodeKey>, opened: bool) {
        let Some(key) = key else {
            return;
        };
        let still_open = opened && self.open_mode(key).is_some();
        if still_open {
            self.top.toggles.retain(|(k, _, new)| !(*k == key && *new));
        } else {
            self.top.toggles.retain(|(k, _, _)| *k != key);
            self.top.toggles.push((key, true, false));
        }
    }

    /// Revalidate a surface target after synchronous cancel/blur handlers in
    /// `open_surface`: returns the fresh `(id, plan)` when still presentable,
    /// otherwise restores focus and resolves the back layer.
    pub(super) fn revalidate_surface(
        &mut self,
        key: NodeKey,
        modal: bool,
        restore_key: Option<NodeKey>,
        request: Option<u32>,
        now_ms: f64,
    ) -> Option<(ViewId, super::Plan)> {
        let valid = (!self.top_open())
            .then(|| {
                let node = self.host.kernel().node_by_key(key)?;
                if node.parent.is_some_and(|p| self.host.route_visibility(p).1) {
                    return None;
                }
                let plan = self.surface_plan(node.id, modal)?;
                (!(modal && matches!(plan, super::Plan::Menu(_)))).then_some((node.id, plan))
            })
            .flatten();
        if valid.is_some() {
            return valid;
        }
        let back = restore_key
            .and_then(|k| self.host.kernel().node_by_key(k).map(|n| n.id))
            .filter(|id| self.focusable(*id) && self.focus_refusal(*id).is_none());
        if !self.top_open() && self.focus.is_none() && back.is_some() {
            if let Some(e) = self.set_focus(back, now_ms) {
                self.host.log(e);
            }
        }
        self.resolve_back_layer(request);
        None
    }

    /// Finish opening a desktop overlay or painted modal through `alert_present`,
    /// preserving any replaced surface's focus target and settling an eager toggle.
    pub(super) fn present_overlay(
        &mut self,
        target: Option<NodeKey>,
        name: &str,
        (invoker, point): (Option<ViewId>, Option<(f32, f32)>),
        modal: bool,
        now_ms: f64,
        (toggled, replaced): (bool, Option<NodeKey>),
    ) -> bool {
        let opened = self.alert_present(target, name, invoker, point, modal, now_ms);
        if let (true, Some(r)) = (opened, replaced) {
            if let Some(a) = self.alert.as_mut() {
                a.restore = Some(r);
            }
        }
        if toggled {
            self.settle_eager_toggle(target, opened);
        }
        opened
    }

    /// Finish presenting a shell surface: step remote/desktop alerts, settle or
    /// queue the toggle, and notify surface listeners.
    pub(super) fn finish_surface_open(&mut self, key: NodeKey, toggled: bool) {
        let desktop_alert = self.top.overlay.enabled
            && self
                .top
                .open
                .as_ref()
                .is_some_and(|o| o.kind == super::SurfaceKind::Alert);
        if self.remote() || desktop_alert {
            self.surface_step(false);
        }
        if toggled {
            self.settle_eager_toggle(Some(key), true);
        } else {
            self.queue_toggle(key, true);
        }
        self.surface_changed();
    }

    /// How many long presses a node has taken, wrapping.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn long_presses(&self) -> u32 {
        self.top.long_presses
    }
}
