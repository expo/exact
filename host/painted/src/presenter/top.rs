//! The top layer (LLP 1115.001 D10): on a host whose shell makes windows, a
//! confirmation and a menu are separate windows of the platform's, with
//! their content painted here. On Android an alert-shaped `alertdialog` is
//! Material 3's basic dialog in a `Dialog` window and a menu popover is
//! Material 3's menu in a `PopupWindow` (`host/android/README.md`), as a
//! select's options are (`top/select.rs`).
//!
//! **Who has one.** A shell that makes windows says so
//! ([`Presenter::serve_windows`]: the Canvas host's reader, as it boots).
//! Windows draws an alert-shaped dialog and a menu as WinUI's ContentDialog
//! and MenuFlyout over the window (`top/overlay.rs`). Every other `dialog`
//! and popover on Windows, and every one on the Linux desktop, is the
//! author's subtree, laid out by the kernel and painted in the window
//! (`alert.rs`, LLP 1115.004 D10.7); Android refuses those for now.
//!
//! **How a surface is painted.** Not by the kernel's layout of the authored
//! rows, which no native host shows either. The shape rule (`top/shape.rs`)
//! reads what the subtree says (a title, a message, the buttons; a menu's
//! items), `top/look.rs` measures and paints Material's component from it,
//! and the shell's painter records that as an op stream of its own, whole
//! each time, which the shell's window replays. One surface is open at a
//! time, and it is host state: opening one commits nothing.
//!
//! **What crosses.** Out, on the one channel (`requests.rs`, kind 7): the
//! surface as a statement, told when it is not what the shell was last told
//! (it opened, a press lit a row, a type or colour preference changed, it
//! closed), with its size, its place and its stream. In, one call
//! ([`Presenter::surface_event`]): a touch in the window at the surface's
//! own points, a key, that the system dismissed the window (a tap outside a
//! menu, Back, Escape), or that the shell could not make it.
//!
//! **Its behaviour.** While a surface is open the page is inert: nothing of
//! it is hit, and keys go to the surface. The focus moves in (the page's
//! node loses it) and returns, when it closes, to the node that held it. A
//! row's tap closes the surface and then runs the row's `press`; an alert's
//! cancel closes it and runs nothing; Back and Escape are a close request,
//! which fires a modal dialog's `cancel` first (HTML) and closes it unless
//! that was prevented; a tap outside closes a menu, and does nothing to an
//! alert, which has no outside (LLP 1115.000 D13.4), unless its author wrote
//! `closedby="any"`: then it dismisses it, as it does a Material dialog left
//! to itself. System Back closes it before any route is asked
//! ([`Presenter::system_back`]).
use super::*;
use exact_kernel::invocation::Action;
use exact_kernel::NodeKey;

pub(super) mod area;
mod input;
mod invoke;
pub(super) mod look;
mod overlay;
mod select;
pub(crate) mod shape;
pub use look::Target;
use look::{Inputs, Layout};
pub use shape::{Entry, Plan, Row};

/// What a shell's windows are drawn from: it records what `paint` draws
/// for a surface of `size` points at `scale` pixels a point, and returns
/// the words its window replays. A recording is whole: it defines every
/// face it draws with.
pub trait SurfacePainter {
    /// Record one surface's picture.
    fn record(
        &mut self,
        size: (f32, f32),
        scale: f32,
        paint: &mut dyn FnMut(&mut dyn Backend),
    ) -> Vec<u32>;
}

/// What a surface is, and the window its shell makes for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    /// Material's basic dialog, in a dialog window: modal, over a scrim.
    Alert = 1,
    /// Material's menu, in a popup window at its invoker.
    Menu = 2,
}

/// The top layer's surface as its shell is told it (request kind 7).
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// Names this opening. Never reused; what the shell says back names it.
    pub serial: u32,
    /// What to show, or `None`: this surface closed.
    pub kind: Option<SurfaceKind>,
    /// Bit 0: Back and Escape dismiss it. Bit 1: a tap outside dismisses it.
    /// Bits 2 and 3: the corner of a menu its motion grows from, the one
    /// at its anchor (bit 2 its right side, bit 3 its bottom).
    pub flags: u32,
    /// Where its window goes in the page's window, and its size, in pixels
    /// (x, y, width, height). An alert's place is the platform's: centred.
    pub frame: [i32; 4],
    /// Its container's corner radius and its elevation, in pixels.
    pub shape: (f32, f32),
    /// Its picture, as the shell's painter recorded it.
    pub stream: Vec<u32>,
}

impl Surface {
    /// The words after a request's count: the serial, what it is (0 closed,
    /// 1 an alert, 2 a menu), the flags, the frame, the radius and the
    /// elevation as float bits, then the stream's count and its words.
    pub(super) fn encode(&self, out: &mut Vec<i32>) {
        let what = self.kind.map_or(0, |kind| kind as i32);
        out.extend([self.serial as i32, what, self.flags as i32]);
        out.extend(self.frame);
        out.extend([self.shape.0.to_bits() as i32, self.shape.1.to_bits() as i32]);
        out.push(self.stream.len() as i32);
        out.extend(self.stream.iter().map(|word| *word as i32));
    }
}

/// What the shell says happened in a surface's window, beside a touch
/// (0 down, 1 up, 2 move, 3 cancel): the numbers [`Presenter::surface_event`]
/// takes.
pub mod event {
    /// The system dismissed the window: a tap outside a menu, Back, Escape.
    pub const DISMISSED: i32 = 8;
    /// The shell could not make the window.
    pub const REFUSED: i32 = 9;
    /// Enter, on a hardware keyboard.
    pub const ENTER: i32 = 16;
    /// Tab, and Shift+Tab.
    pub const TAB: i32 = 17;
    /// See `TAB`.
    pub const TAB_BACK: i32 = 18;
    /// The arrows: up, down, left, right.
    pub const UP: i32 = 19;
    /// See `UP`.
    pub const DOWN: i32 = 20;
    /// See `UP`.
    pub const LEFT: i32 = 21;
    /// See `UP`.
    pub const RIGHT: i32 = 22;
    /// Space.
    pub const SPACE: i32 = 23;
}

/// The surface that is open.
struct Open {
    serial: u32,
    /// Moved on whenever its picture or its place changes.
    revision: u32,
    key: NodeKey,
    node: ViewId,
    kind: SurfaceKind,
    /// Opened by `show-modal`, not as a popover: HTML's popover commands
    /// leave it open, and its dialog `close` closes only this mode.
    modal: bool,
    /// What opened it: where a menu hangs.
    invoker: Option<ViewId>,
    /// Where a context menu was asked for, viewport points.
    point: Option<(f32, f32)>,
    /// The node that held the focus before it opened.
    restore: Option<ViewId>,
    plan: Plan,
    inputs: Inputs,
    /// A menu's anchor in the window, points.
    anchor: Rect4,
    layout: Layout,
    /// The shell's frame in window pixels, its display scale and motion origin.
    frame: [i32; 4],
    scale: f32,
    origin: u32,
    /// The row under the finger, and the row a keyboard focused.
    pressed: Option<usize>,
    /// Where a finger that went down in a scrolling menu went down and
    /// last was, and whether it has moved far enough to scroll it.
    drag: Option<(f32, f32, bool)>,
    focused: Option<usize>,
    hovered: Option<usize>,
    /// The kernel epoch its plan was read at.
    epoch: u64,
}

impl Open {
    /// Whether a tap outside dismisses it: a menu always, an alert when its
    /// `closedby` is `any`.
    fn outside(&self) -> bool {
        match &self.plan {
            Plan::Alert { outside, .. } => *outside,
            Plan::Menu(_) => true,
        }
    }
}

/// What the presenter keeps of the top layer.
#[derive(Default)]
pub(super) struct State {
    overlay: overlay::State,
    /// The shell's painter: `Some` on a host whose shell makes windows.
    painter: Option<Box<dyn SurfacePainter>>,
    open: Option<Open>,
    toggles: Vec<(NodeKey, bool, bool)>,
    closes: Vec<NodeKey>,
    /// The last serial given.
    last: u32,
    /// The surface the shell shows as it was last told: serial and revision.
    told: Option<(u32, u32)>,
    /// How many long presses a node has taken ([`Presenter::long_press`]):
    /// a reader that compares it across a contact knows whether its own was.
    long_presses: u32,
}

/// The open surface, for a driver and a test.
pub struct Shown<'a> {
    /// Its serial.
    pub serial: u32,
    /// Changes when the surface's presentation or item geometry changes.
    pub revision: u32,
    /// What it is.
    pub kind: SurfaceKind,
    /// The authored `dialog` or popover.
    pub node: ViewId,
    /// What it says.
    pub plan: &'a Plan,
    /// Its size, points.
    pub size: (f32, f32),
    /// The visible content of an overflowing desktop menu.
    pub clip: Option<Rect4>,
    /// The menu's current and maximum vertical scroll offsets.
    pub scroll: Option<(f32, f32)>,
    /// Its rows' boxes in the window, in the plan's order: how a node
    /// inside a window is addressed.
    pub targets: &'a [Target],
    /// The row a finger holds, if one does.
    pub pressed: Option<usize>,
    /// Each text it shows, by the node it stands for (an alert's headline
    /// and supporting text by the surface's own), with its box, points.
    pub texts: Vec<(ViewId, Rect4)>,
}

impl<D: DataSource> Presenter<D> {
    pub(super) fn next_surface_serial(&mut self) -> u32 {
        self.top.last += 1;
        self.top.last
    }

    /// A shell makes this presenter's top layer as windows from now on,
    /// drawn through `painter` (LLP 1115.001 D10). A host with no such
    /// shell keeps what it did: no top layer.
    pub fn serve_windows(&mut self, painter: Box<dyn SurfacePainter>) {
        self.top.painter = Some(painter);
        self.host.top_layer(true);
    }

    /// Whether a shell makes windows.
    pub(crate) fn has_windows(&self) -> bool {
        self.top.painter.is_some() || self.top.overlay.enabled
    }

    pub(super) fn separate_surface_windows(&self) -> bool {
        self.top.painter.is_some()
    }

    pub(super) fn desktop_overlays(&self) -> bool {
        self.top.overlay.enabled
    }

    /// The surface that is open, if one is.
    pub fn surface(&self) -> Option<Shown<'_>> {
        self.top.open.as_ref().map(|open| Shown {
            serial: open.serial,
            revision: open.revision,
            kind: open.kind,
            node: open.node,
            plan: &open.plan,
            size: open.layout.size,
            clip: open.layout.menu_clip(),
            scroll: open.layout.menu_scroll(),
            targets: &open.layout.targets,
            pressed: open.pressed,
            texts: open.layout.texts(),
        })
    }

    /// Whether a surface is open: the page is then inert.
    pub(crate) fn top_open(&self) -> bool {
        self.shape_surface_open() || self.alert.is_some()
    }

    pub(super) fn shape_surface_open(&self) -> bool {
        self.top.open.is_some()
    }

    /// The host was replaced (a reload): its tree is another, and the new
    /// one is told again whether a top layer is presented.
    pub(super) fn top_replaced(&mut self) {
        self.alert = None;
        self.brush.alert = None;
        self.host.top_layer(self.has_windows());
        if self.top.open.take().is_some() {
            self.surface_changed();
        }
    }

    /// The shell was made again: it is told the open surface anew.
    pub(super) fn surface_reset(&mut self) {
        self.top.told = None;
    }

    fn surface_changed(&mut self) {
        self.dirty |= self.top.overlay.enabled;
        self.report_top_layer();
        self.back_changed();
        self.holds_changed();
        // The shell comes for the statement whether or not a frame follows.
        self.executor.notify();
    }

    /// What `pop` is to the top layer, shown modally or as a popover: an
    /// alert, a menu, or nothing it shows. A select is its options' menu.
    fn surface_plan(&self, pop: ViewId, modal: bool) -> Option<Plan> {
        if self.is_select(pop) {
            return self.select_plan(pop);
        }
        let runner = self.host.runner();
        let pressed = |id: ViewId| runner.handlers_of(id).contains(&EventKind::Press);
        let kernel = self.host.kernel();
        shape::alert(kernel, pop, &pressed, modal).or_else(|| {
            if self.top.overlay.enabled {
                let role = kernel.node(pop)?.props.str(PropId::AccessibilityRole);
                let menu = role == Some("menu")
                    || role.is_none()
                        && kernel
                            .preorder_where(&[pop], |kind, _| kind == NodeType::TextInput)
                            .is_empty();
                if !menu {
                    return None;
                }
            }
            shape::menu(kernel, pop, &pressed)
        })
    }

    /// The window the shell told, and how far a menu keeps from its top and
    /// bottom: Material's 48, or the system's bars where they are taller.
    fn surface_window(&self) -> ((f32, f32), (f32, f32)) {
        if self.top.overlay.enabled {
            return (self.viewport, (4.0, 4.0));
        }
        match self.window() {
            Some(w) => {
                let inset = |i: usize| w.bars[i].max(w.cutout[i]).max(48.0);
                (w.size, (inset(0), inset(2)))
            }
            None => (self.viewport, (48.0, 48.0)),
        }
    }

    fn surface_inputs(&self) -> Inputs {
        let scale = self.text_scale;
        let (window, margins) = self.surface_window();
        Inputs {
            dark: self.dark(),
            contrast: self.colors.contrast.palette,
            bold_text: self.host.kernel().bold_text_weights(0),
            font_scale: if scale.is_finite() && scale > 0.0 {
                scale
            } else {
                1.0
            },
            window,
            room: window.1 - margins.0 - margins.1,
        }
    }

    /// `plan` measured for `pop` in the look of this host's top layer.
    fn measure_surface(&mut self, plan: &Plan, pop: ViewId, inputs: &Inputs) -> Layout {
        let measure = if self.top.overlay.enabled {
            Layout::desktop
        } else {
            Layout::of
        };
        let mut layout = measure(&mut self.text.borrow_mut(), plan, pop, inputs);
        if let Some(width) = self.select_width(pop) {
            layout.widen(width.min(inputs.window.0));
        }
        layout
    }

    /// Where the open menu hangs, in the window's points: the point a
    /// context menu was asked at, else its invoker's box as last painted.
    fn surface_anchor(&mut self) -> Rect4 {
        let Some((invoker, point)) = self.top.open.as_ref().map(|o| (o.invoker, o.point)) else {
            return (0.0, 0.0, 0.0, 0.0);
        };
        let [px, py, ..] = self.chrome().frame;
        if let Some((x, y)) = point {
            return (px + x, py + y, 0.0, 0.0);
        }
        // A button its shell's top bar shows hangs its menu from the bar.
        if let Some(at) = invoker.and_then(|id| self.bar_box(id)) {
            return at;
        }
        let b = invoker.and_then(|id| self.box_of(id));
        b.map_or((px, py, 0.0, 0.0), |b| {
            (px + b.rect.0, py + b.rect.1, b.rect.2, b.rect.3)
        })
    }

    /// How `target` is open: `Some(true)` modally, `Some(false)` as a
    /// popover, `None` when it is not the open surface.
    fn open_mode(&self, target: NodeKey) -> Option<bool> {
        self.alert_mode(target).or_else(|| {
            (self.top.open.as_ref())
                .filter(|open| open.key == target)
                .map(|open| open.modal)
        })
    }

    /// The element an action's command names by `id`, by a button's rule
    /// (`exact_kernel::invocation::element`).
    pub(super) fn element(&self, name: &str) -> Option<NodeKey> {
        exact_kernel::invocation::element(self.host.kernel(), name)
    }

    /// Present `target`, the dialog or popover `name` names, modally or as a
    /// popover: whether it was. A menu is never presented modally.
    pub(super) fn present(
        &mut self,
        target: Option<NodeKey>,
        name: &str,
        invoker: Option<ViewId>,
        point: Option<(f32, f32)>,
        modal: bool,
        now_ms: f64,
    ) -> bool {
        // Its session's roles (LLP 1121 stage 5): the surface's look resolves them.
        let _roles = self.host.runner().bind_roles();
        // A popover its author placed by `left` or `top` opens at that point,
        // as a context menu opens at the pointer (LLP 1021 D2).
        let point = point.or_else(|| {
            let node = self.host.kernel().node_by_key(target?)?;
            let style = node.computed_style(exact_kernel::StyleMask::INHERITED);
            let popover = !modal && node.props.str(PropId::Popover).is_some();
            popover
                .then(|| {
                    area::inset_point(&style, &self.host.kernel().env_of(node.id), self.viewport)
                })
                .flatten()
        });
        if self.neutral_alerts() {
            return self.alert_present(target, name, invoker, point, modal, now_ms);
        }
        let kernel = self.host.kernel();
        let pop = (target.and_then(|key| kernel.node_by_key(key)))
            .map(|node| (node.id, node.key, node.parent));
        let plan = pop.and_then(|(id, ..)| self.surface_plan(id, modal));
        if self.top.overlay.enabled && plan.is_none() {
            return self.alert_present(target, name, invoker, point, modal, now_ms);
        }
        let shows =
            |parent: Option<ViewId>| !parent.is_some_and(|p| self.host.route_visibility(p).1);
        let refusal = match (pop, &plan) {
            (None, _) => Some("no live node has that id"),
            (Some((_, _, parent)), _) if !shows(parent) => Some("it is on a route that does not show"),
            (_, Some(Plan::Menu(_))) if modal => Some("it is a menu, which its invoker opens"),
            (_, None) => Some("this host presents an alert-shaped alertdialog (text, one cancel, at most three buttons) and a menu popover (button rows); authored dialogs and popovers are presented on desktop hosts, but not yet in Android's windows (LLP 1115.001 D10)"),
            _ => None,
        };
        let (Some((id, key, _)), Some(plan), None) = (pop, plan, refusal) else {
            let what = if modal { "showModal" } else { "popover" };
            let why = refusal.unwrap_or_default();
            self.host.log(format!("{what} \"{name}\" refused: {why}"));
            return false;
        };
        self.open_surface((id, key), plan, modal, (invoker, point), now_ms);
        // Alerts need an initial action even when a pointer or a response
        // opens them. TV surfaces also start at their first row (LLP 1115.001.003 D3).
        let desktop_alert = self.top.overlay.enabled
            && self
                .top
                .open
                .as_ref()
                .is_some_and(|open| open.kind == SurfaceKind::Alert);
        if self.remote() || desktop_alert {
            self.surface_step(false);
        }
        self.queue_toggle(key, true);
        self.surface_changed();
        true
    }

    /// Open `plan`, what `id` (its node's `key`) is, as the surface, opened
    /// by `invoker` or at `point`: the surface open closes, the pointer's
    /// contact ends, and the new one is measured and placed. The focus
    /// moves in, and the page's node loses it, but a select keeps it, as a
    /// combobox does while its listbox is open.
    fn open_surface(
        &mut self,
        (id, key): (ViewId, NodeKey),
        plan: Plan,
        modal: bool,
        (invoker, point): (Option<ViewId>, Option<(f32, f32)>),
        now_ms: f64,
    ) {
        let restore = self
            .replace_surface(invoker, now_ms)
            .and_then(|key| self.host.kernel().node_by_key(key).map(|n| n.id));
        self.report_top_layer();
        if let Err(e) = self.pointer_cancel(now_ms) {
            self.host.log(e);
        }
        if !self.is_select(id) {
            if let Some(e) = self.set_focus(None, now_ms) {
                self.host.log(e);
            }
        }
        let serial = self.next_surface_serial();
        let inputs = self.surface_inputs();
        let layout = self.measure_surface(&plan, id, &inputs);
        self.top.open = Some(Open {
            serial,
            revision: 0,
            key,
            node: id,
            modal,
            kind: match plan {
                Plan::Alert { .. } => SurfaceKind::Alert,
                Plan::Menu(_) => SurfaceKind::Menu,
            },
            invoker,
            point,
            restore,
            plan,
            inputs,
            anchor: (0.0, 0.0, 0.0, 0.0),
            layout,
            frame: [0; 4],
            scale: self.brush.scale,
            origin: 0,
            pressed: None,
            drag: None,
            focused: None,
            hovered: None,
            epoch: self.host.kernel().epoch(),
        });
        let anchor = self.surface_anchor();
        if let Some(open) = self.top.open.as_mut() {
            open.anchor = anchor;
        }
        self.place_surface();
    }

    /// Close the previous surface and carry its original focus return target.
    pub(super) fn replace_surface(&mut self, invoker: Option<ViewId>, now: f64) -> Option<NodeKey> {
        let restore = if let Some(open) = &self.alert {
            open.restore
        } else {
            self.top
                .open
                .as_ref()
                .map_or(self.focus.or(invoker), |o| o.restore)
                .and_then(|id| self.host.kernel().node(id).map(|n| n.key))
        };
        self.close_surface(now);
        restore
    }

    pub(super) fn close_surface(&mut self, now_ms: f64) {
        if self.alert.is_some() {
            self.alert_close(now_ms);
            return;
        }
        let Some(open) = self.top.open.take() else {
            return;
        };
        self.report_top_layer();
        self.queue_surface_closed(open.key, open.modal);
        let back = open.restore.filter(|id| {
            self.host.kernel().node(*id).is_some()
                && !self.host.route_visibility(*id).1
                && self.focusable(*id)
        });
        if self.focus.is_none() && back.is_some() {
            if let Some(e) = self.set_focus(back, now_ms) {
                self.host.log(e);
            }
        }
        self.surface_changed();
    }

    /// Back, Escape or a tap outside, where the surface's own rule lets it
    /// go: a menu always, an alert unless its `closedby` is `none`. Whether
    /// a surface was open to take it.
    pub(super) fn dismiss_surface(&mut self, now_ms: f64) -> bool {
        if self.alert.is_some() {
            return self.alert_close_request(now_ms);
        }
        let Some(open) = &self.top.open else {
            return false;
        };
        let may = match &open.plan {
            Plan::Alert { cancellable, .. } => *cancellable,
            Plan::Menu(_) => true,
        };
        let (serial, node, modal) = (open.serial, open.node, open.modal);
        if may
            && (!modal || !self.dialog_close_prevented(node, now_ms))
            && self.top.open.as_ref().is_some_and(|o| o.serial == serial)
        {
            self.close_surface(now_ms);
        }
        true
    }

    /// HTML's cancel is sent only for a person's close request, before close.
    pub(super) fn dialog_close_prevented(&mut self, id: ViewId, now: f64) -> bool {
        let from = self.commands.len();
        let error = self.host.dispatch_at(id, Event::Cancel, now);
        if let Some(error) = error.or(self.after_commit()) {
            self.host.log(error);
        }
        self.commands.retain(|c| c.name != "stopPropagation");
        self.prevented_since(from)
    }

    /// A person chose row `index`: the surface closes, then the row's
    /// `press` runs (an alert's cancel has none), then the menu it opens,
    /// where it opens one, takes the first one's place.
    /// A point of the open surface's own space in the page's, where its
    /// row's `MouseEvent` has `clientX`/`clientY` (LLP 1021 D2).
    /// Its frame is in the window's pixels, the rest in points.
    fn surface_client(&self, (x, y): (f32, f32)) -> Option<(f32, f32)> {
        let open = self.top.open.as_ref()?;
        let [px, py, ..] = self.chrome().frame;
        Some((
            open.frame[0] as f32 / open.scale + x - px,
            open.frame[1] as f32 / open.scale + y - py,
        ))
    }

    /// Row `index` chosen; `pressed`, where a pointer chose it, in the page.
    fn choose(&mut self, index: usize, now_ms: f64, pressed: Option<(f32, f32)>) {
        if let Some(select) = self.select_surface() {
            self.choose_option(select, index, now_ms);
            return;
        }
        let Some(open) = &self.top.open else {
            return;
        };
        let Some(row) = open.plan.rows().nth(index).filter(|r| r.enabled).cloned() else {
            return;
        };
        let (invoker, point, menu) = (open.invoker, open.point, open.key);
        let key = self.host.kernel().node(row.node).map(|node| node.key);
        self.close_surface(now_ms);
        if self
            .host
            .runner()
            .handlers_of(row.node)
            .contains(&EventKind::Press)
            && !row.cancel
        {
            let (x, y) = pressed.unwrap_or_default();
            let press = Event::pressed(exact_runner::MouseEvent {
                client_x: x.into(),
                client_y: y.into(),
                held: self.modifiers(),
            });
            let error = self.host.dispatch_at(row.node, press, now_ms);
            if let Some(e) = error.or(self.after_commit()) {
                eprintln!("exact: {e}");
            }
        }
        // What the surviving row opens once its `press` has run, as HTML
        // resolves a button's invocation after its listeners: a submenu, or
        // a dialog by `show-modal`, in the menu's place.
        let kernel = self.host.kernel();
        let opens = (key.and_then(|key| kernel.node_by_key(key)))
            .filter(|row| row.props.bool(PropId::Disabled) != Some(true))
            .and_then(|row| exact_kernel::Invocation::of(kernel, &row))
            .filter(|i| i.opens() && i.target != menu);
        if let Some(i) = opens {
            let name = self.name_of(i.target);
            let modal = i.action == Action::ShowModal;
            self.present(Some(i.target), &name, invoker, point, modal, now_ms);
        }
    }

    /// A touch in the surface: a row is lit while a finger holds it, and
    /// chosen when the finger lifts in it. A finger that leaves its row
    /// lets go of it. In a menu taller than its window, a finger that moves
    /// past the touch slop scrolls the rows instead, and chooses none.
    fn surface_touch(&mut self, action: i32, x: f32, y: f32, now_ms: f64) {
        let Some(open) = self.top.open.as_mut() else {
            return;
        };
        let at = (open.layout.target_at(x, y)).filter(|i| open.layout.targets[*i].enabled);
        let before = open.pressed;
        let mut chosen = None;
        let mut scrolled = false;
        match action {
            0 => {
                open.pressed = at;
                open.drag = open.layout.menu_scroll().map(|_| (y, y, false));
            }
            2 if open.drag.is_some() => {
                let (start, last, moving) = open.drag.unwrap();
                // Android's touch slop (`ViewConfiguration`), 8 dp.
                let moving = moving || (y - start).abs() > 8.0;
                if moving {
                    open.pressed = None;
                    scrolled = open.layout.scroll_content(last - y);
                }
                open.drag = Some((start, if moving { y } else { last }, moving));
            }
            2 if open.pressed != at => open.pressed = None,
            2 => {}
            1 => {
                chosen = open.pressed.take().filter(|pressed| Some(*pressed) == at);
                open.drag = None;
            }
            _ => {
                open.pressed = None;
                open.drag = None;
            }
        }
        if open.pressed != before || scrolled {
            open.revision += 1;
            self.executor.notify();
        }
        if let Some(index) = chosen {
            let point = self.surface_client((x, y));
            self.choose(index, now_ms, point);
        }
    }

    /// Whether a surface is open to take the keys ([`Self::surface_key`]).
    pub(crate) fn surface_takes_keys(&self) -> bool {
        self.alert_takes_back() || self.top.open.is_some()
    }

    #[cfg(windows)]
    pub(super) fn desktop_surface_access(&self) -> Option<(Option<ViewId>, (f32, f32))> {
        let open = self.top.open.as_ref()?;
        let focused = open
            .focused
            .and_then(|i| open.layout.targets.get(i))
            .map(|t| t.node);
        Some((
            focused,
            (
                open.frame[0] as f32 / open.scale,
                open.frame[1] as f32 / open.scale,
            ),
        ))
    }

    /// A key while a surface is open goes to it and to nothing of the page.
    /// Tab and Up/Down move its focus; Right opens a menu row's submenu;
    /// Left/Right move between an alert's buttons. Enter and Space choose
    /// the focused row, and Escape dismisses the surface.
    pub(super) fn surface_key(&mut self, name: &str, now_ms: f64) -> bool {
        if !matches!(
            name,
            "" | "Unidentified"
                | "Shift"
                | "Control"
                | "Alt"
                | "Meta"
                | "CapsLock"
                | "NumLock"
                | "ScrollLock"
        ) {
            self.overlay_input(true);
        }
        if self.top.open.is_none() {
            return false;
        }
        if self.overlay_scroll_key(name) {
            return true;
        }
        if let Some(taken) = self.select_surface_key(name, now_ms) {
            return taken;
        }
        match name {
            "Escape" => {
                self.dismiss_surface(now_ms);
            }
            "Tab" => self.surface_step(self.modifiers().shift),
            "ArrowDown" => self.surface_step(false),
            "ArrowUp" => self.surface_step(true),
            "ArrowLeft" | "ArrowRight" => self.surface_horizontal(name == "ArrowLeft", now_ms),
            "Home" | "End" if self.top.overlay.enabled => self.surface_edge(name == "End"),
            "Enter" | " " => {
                if let Some(index) = self.top.open.as_ref().and_then(|open| open.focused) {
                    self.choose(index, now_ms, None);
                }
            }
            _ => {}
        }
        true
    }

    /// A submenu replaces its parent, so Left has no parent to return to.
    fn surface_horizontal(&mut self, backward: bool, now_ms: f64) {
        let Some(open) = self.top.open.as_ref() else {
            return;
        };
        if open.kind == SurfaceKind::Alert {
            self.surface_step(backward);
        } else if !backward {
            let index = open.focused.filter(|index| {
                open.plan
                    .rows()
                    .nth(*index)
                    .is_some_and(|row| row.enabled && row.opens.is_some())
            });
            if let Some(index) = index {
                let serial = open.serial;
                self.choose(index, now_ms, None);
                if self
                    .top
                    .open
                    .as_ref()
                    .is_some_and(|open| open.serial != serial && open.focused.is_none())
                {
                    self.surface_step(false);
                }
            }
        }
    }

    /// An authored focus command selects the row in its shell window,
    /// leaving the page's focus set aside until the surface closes.
    pub(super) fn surface_focus(&mut self, id: ViewId) -> bool {
        let Some(open) = self.top.open.as_mut() else {
            return false;
        };
        let Some(index) = open
            .layout
            .targets
            .iter()
            .position(|row| row.node == id && row.enabled)
        else {
            return false;
        };
        let revealed = open.layout.reveal_menu_target(index);
        if open.focused != Some(index) || revealed {
            open.focused = Some(index);
            open.revision += 1;
            self.executor.notify();
        }
        true
    }

    /// Move the surface's own focus to the next row that can be chosen
    /// (`backward`: the one before), from none to the first (the last).
    fn surface_step(&mut self, backward: bool) {
        let Some(open) = self.top.open.as_mut() else {
            return;
        };
        let targets = &open.layout.targets;
        let n = targets.len();
        let from = open.focused;
        let order = if from.is_some() {
            open.layout.focus_order()
        } else {
            (0..n).collect()
        };
        let from = from.and_then(|i| order.iter().position(|v| *v == i));
        let next = (1..=n)
            .map(|step| match (from, backward) {
                (Some(at), false) => (at + step) % n,
                (Some(at), true) => (at + n - step) % n,
                (None, false) => step - 1,
                (None, true) => n - step,
            })
            .map(|i| order[i])
            .find(|i| targets[*i].enabled);
        let revealed = next.is_some_and(|i| open.layout.reveal_menu_target(i));
        if next.is_some() && (next != open.focused || revealed) {
            open.focused = next;
            open.revision += 1;
            self.executor.notify();
        }
    }

    fn surface_edge(&mut self, last: bool) {
        let Some(open) = self.top.open.as_mut() else {
            return;
        };
        if open.kind != SurfaceKind::Menu {
            return;
        }
        let targets = &open.layout.targets;
        let next = if last {
            targets.iter().rposition(|row| row.enabled)
        } else {
            targets.iter().position(|row| row.enabled)
        };
        let revealed = next.is_some_and(|i| open.layout.reveal_menu_target(i));
        if next != open.focused || revealed {
            open.focused = next;
            open.revision += 1;
            self.executor.notify();
        }
    }

    /// A touch reached the page while a surface is open: it is outside the
    /// surface, since the page is inert under it. A menu closes (the
    /// platform's own popup does this itself and the touch never arrives),
    /// and so does an alert whose `closedby` is `any`; any other alert has
    /// no outside. Whether the touch was taken from the page.
    pub(super) fn top_takes_pointer(&mut self, now_ms: f64) -> bool {
        match self.top.open.as_ref().map(|open| open.outside()) {
            Some(true) => {
                self.dismiss_surface(now_ms);
            }
            Some(false) => {}
            None => return false,
        }
        true
    }

    /// Whether a surface of the top layer (a popover's, a menu's) is open over the page: nothing under it is the
    /// agent's to scroll clear of a sticky box (`clear_of_sticky`).
    pub(super) fn surface_open(&self) -> bool {
        self.top.open.is_some()
    }

    /// Whether `id` is a row of the open surface, by its authored node: it
    /// shows in the surface's window, so the agent's `reveal` has nothing
    /// to scroll into view before a tap on it.
    pub(super) fn surface_row(&self, id: ViewId) -> bool {
        (self.top.open.as_ref())
            .is_some_and(|open| open.layout.targets.iter().any(|t| t.node == id))
    }

    /// The agent's `tap` while a surface is open: a row of the surface is
    /// chosen, by its authored node; any other node is under the surface,
    /// where a tap closes a menu and does nothing through an alert.
    pub(super) fn surface_tap(&mut self, id: ViewId) -> Option<Result<String, String>> {
        if self.alert.is_some() {
            return self.alert_outside_tap(id);
        }
        let open = self.top.open.as_ref()?;
        let (serial, pop, kind, outside) = (open.serial, open.node, open.kind, open.outside());
        let row = open.layout.targets.iter().position(|t| t.node == id);
        let enabled = row.is_some_and(|index| open.layout.targets[index].enabled);
        let now = self.host.now();
        Some(match (row, kind) {
            (Some(_), _) if !enabled => Err(format!(
                "view {id} is disabled in the open surface (view {pop})"
            )),
            (Some(index), _) => {
                // The agent's tap is a pointer's, at the row's middle.
                let (x, y, w, h) = open.layout.targets[index].rect;
                let point = self.surface_client((x + w / 2., y + h / 2.));
                self.choose(index, now, point);
                Ok(format!("{{\"tapped\":{id},\"surface\":{serial}}}"))
            }
            (None, _) if outside => {
                self.dismiss_surface(now);
                let dismissed = if self.top.open.as_ref().is_some_and(|o| o.serial == serial) {
                    "null".into()
                } else {
                    serial.to_string()
                };
                Ok(format!("{{\"tapped\":{id},\"dismissed\":{dismissed}}}"))
            }
            (None, _) => Err(format!(
                "view {id} is under the open alert (view {pop}): the page is inert until it closes"
            )),
        })
    }

    pub(super) fn queue_surface_closed(&mut self, key: NodeKey, modal: bool) {
        self.queue_toggle(key, false);
        if modal
            && self
                .host
                .kernel()
                .node_by_key(key)
                .is_some_and(|n| n.props.str(PropId::SemanticTag) == Some("dialog"))
        {
            self.top.closes.push(key);
        }
    }

    pub(super) fn queue_toggle(&mut self, key: NodeKey, open: bool) {
        if let Some((_, _, new)) = self.top.toggles.iter_mut().find(|(k, _, _)| *k == key) {
            *new = open;
        } else {
            self.top.toggles.push((key, !open, open));
        }
    }

    pub(super) fn dispatch_toggles(&mut self) {
        for key in std::mem::take(&mut self.top.closes) {
            let Some(id) = self.host.kernel().node_by_key(key).map(|n| n.id) else {
                continue;
            };
            if let Some(error) = self
                .host
                .dispatch_at(id, Event::Close, self.host.now())
                .or(self.after_commit())
            {
                self.host.log(error);
            }
        }
        for (key, old_open, new_open) in std::mem::take(&mut self.top.toggles) {
            let Some(node) = self.host.kernel().node_by_key(key) else {
                continue;
            };
            if node.props.str(PropId::Popover).is_none()
                || !self
                    .host
                    .runner()
                    .handlers_of(node.id)
                    .contains(&EventKind::Toggle)
            {
                continue;
            }
            let id = node.id;
            if let Some(error) = self
                .host
                .dispatch_at(id, Event::Toggle { old_open, new_open }, self.host.now())
                .or(self.after_commit())
            {
                self.host.log(error);
            }
        }
    }

    /// Before the shell is told: an open surface whose node is gone, whose
    /// route no longer shows or whose rows are no longer its shape closes,
    /// running nothing; one that says something else, or is shown under
    /// another colour or type preference or window, is measured again.
    pub(super) fn sync_surface(&mut self) {
        let Some(open) = &self.top.open else {
            return;
        };
        let (key, pop, kind, read) = (open.key, open.node, open.kind, open.epoch);
        let kernel = self.host.kernel();
        let epoch = kernel.epoch();
        let node = kernel.node_by_key(key).filter(|node| node.id == pop);
        let shows = node.is_some_and(|node| {
            !(node.parent).is_some_and(|parent| self.host.route_visibility(parent).1)
        });
        let plan = match (shows, epoch == read) {
            (false, _) => None,
            (true, true) => Some(open.plan.clone()),
            (true, false) => self
                .surface_plan(pop, open.modal)
                .filter(|plan| matches!(plan, Plan::Alert { .. }) == (kind == SurfaceKind::Alert)),
        };
        let Some(plan) = plan else {
            let now = self.host.now();
            self.close_surface(now);
            return;
        };
        let inputs = self.surface_inputs();
        let anchor = self.surface_anchor();
        let again = (self.top.open.as_ref()).is_some_and(|o| plan != o.plan || inputs != o.inputs);
        let layout = again.then(|| self.measure_surface(&plan, pop, &inputs));
        let Some(open) = self.top.open.as_mut() else {
            return;
        };
        open.epoch = epoch;
        if let Some(layout) = layout {
            // The focus follows its row's node, not its index.
            let focused = (open.focused).and_then(|i| open.layout.targets.get(i).map(|t| t.node));
            open.layout = layout;
            (open.plan, open.inputs) = (plan, inputs);
            open.pressed = None;
            open.hovered = None;
            let targets = &open.layout.targets;
            open.focused = focused.and_then(|node| targets.iter().position(|t| t.node == node));
            if let Some(index) = open.focused {
                open.layout.reveal_menu_target(index);
            }
            open.revision += 1;
        }
        if anchor != open.anchor {
            open.anchor = anchor;
            open.revision += 1;
        }
        self.place_surface();
        self.report_top_layer();
    }

    fn place_surface(&mut self) {
        let (window, margins) = self.surface_window();
        let scale = self.brush.scale;
        let Some(open) = self.top.open.as_mut() else {
            return;
        };
        let (at, origin) = match open.kind {
            SurfaceKind::Alert if self.top.overlay.enabled => (
                (
                    (window.0 - open.layout.size.0).max(0.) / 2.,
                    (window.1 - open.layout.size.1).max(0.) / 2.,
                ),
                0,
            ),
            SurfaceKind::Alert => ((0.0, 0.0), 0),
            SurfaceKind::Menu => {
                let at = look::place(open.anchor, open.layout.size, window, margins);
                (at, look::origin(open.anchor, at, open.layout.size))
            }
        };
        let px = |v: f32| (v * scale).round() as i32;
        let frame = [
            px(at.0),
            px(at.1),
            px(open.layout.size.0),
            px(open.layout.size.1),
        ];
        if (frame, scale, origin) != (open.frame, open.scale, open.origin) {
            (open.frame, open.scale, open.origin) = (frame, scale, origin);
            open.revision += 1;
        }
    }

    /// Report the shell's frame in viewport points and whether it opened
    /// modally. A popover leaves the document reachable to structural reads.
    /// Native alerts, as on Apple, report no authored geometry, and a
    /// select's menu is no authored node's.
    pub(super) fn report_top_layer(&mut self) {
        let shown = self.top.open.as_ref().filter(|o| !self.is_select(o.node));
        let Some(open) = shown else {
            // An open authored popover is the page's own subtree, drawn at its
            // box (alert.rs): reads see it, as the web's do.
            let frames: Vec<_> = self.alert_popover_frame().into_iter().collect();
            self.host.runner_mut().top_layer_frames(&frames, None);
            return;
        };
        let rect = match open.kind {
            SurfaceKind::Alert => [0.0; 4],
            SurfaceKind::Menu => {
                let [px, py, ..] = self.chrome().frame;
                let [x, y, w, h] = open.frame.map(|v| f64::from(v) / f64::from(open.scale));
                [x - f64::from(px), y - f64::from(py), w, h]
            }
        };
        let frames = [(open.node, rect, None)];
        let modal = open.modal.then_some(0);
        self.host.runner_mut().top_layer_frames(&frames, modal);
    }

    /// The open surface as the shell is told it: its frame in the window
    /// and its picture, recorded by the shell's painter.
    fn surface_statement(&mut self) -> Option<Surface> {
        let text = self.text.clone();
        let open = self.top.open.as_ref()?;
        let scale = open.scale;
        let layout = &open.layout;
        let feedback = look::Feedback {
            pressed: open.pressed,
            focused: open.focused,
            hovered: open.hovered,
        };
        let painter = self.top.painter.as_mut()?;
        let stream = painter.record(layout.size, scale, &mut |backend| {
            layout.paint(backend, &mut text.borrow_mut(), feedback)
        });
        let cancellable = match &open.plan {
            Plan::Alert { cancellable, .. } => *cancellable,
            Plan::Menu(_) => true,
        };
        Some(Surface {
            serial: open.serial,
            kind: Some(open.kind),
            flags: u32::from(cancellable) | u32::from(open.outside()) << 1 | open.origin << 2,
            frame: open.frame,
            shape: (layout.radius * scale, layout.elevation * scale),
            stream,
        })
    }

    /// Tell the shell the top layer's surface when it is not what it was
    /// last told: the one it shows closed, another opened, or the one open
    /// changed its picture or its place. Queued with the shell's other
    /// statements (`tell_shell`), on the one channel out.
    pub(super) fn tell_surface(&mut self) {
        if self.top.painter.is_none() {
            return;
        }
        self.sync_surface();
        let now = self
            .top
            .open
            .as_ref()
            .map(|open| (open.serial, open.revision));
        if now == self.top.told {
            return;
        }
        let gone = self
            .top
            .told
            .filter(|told| now.map(|n| n.0) != Some(told.0));
        if let Some((serial, _)) = gone {
            self.queue(Request::Surface(Surface {
                serial,
                kind: None,
                flags: 0,
                frame: [0; 4],
                shape: (0.0, 0.0),
                stream: Vec::new(),
            }));
        }
        if let Some(surface) = self.surface_statement() {
            self.queue(Request::Surface(surface));
        }
        self.top.told = now;
    }

    /// The open surface for the agent's `state`: what it is, the authored
    /// node, what it says, and each row's node and box in the window.
    pub fn surface_json(&self) -> serde_json::Value {
        if self.alert.is_some() {
            return self.alert_json();
        }
        let Some(shown) = self.surface() else {
            return serde_json::Value::Null;
        };
        let rows: Vec<_> = (shown.plan.rows().zip(shown.targets))
            .map(|(row, target)| {
                let (x, y, w, h) = target.rect;
                serde_json::json!({"id": row.node, "title": row.title, "enabled": row.enabled,
                    "destructive": row.destructive, "cancel": row.cancel,
                    "x": r2(x), "y": r2(y), "w": r2(w), "h": r2(h)})
            })
            .collect();
        let (title, message) = match shown.plan {
            Plan::Alert { title, message, .. } => (title.clone(), message.clone()),
            Plan::Menu(_) => (None, None),
        };
        // A menu's section titles, in order.
        let headers: Vec<&str> = match shown.plan {
            Plan::Menu(entries) => (entries.iter())
                .filter_map(|e| match e {
                    shape::Entry::Header(title, _) => Some(title.as_str()),
                    _ => None,
                })
                .collect(),
            Plan::Alert { .. } => Vec::new(),
        };
        serde_json::json!({
            "serial": shown.serial,
            "kind": match shown.kind { SurfaceKind::Alert => "alert", SurfaceKind::Menu => "menu" },
            "node": shown.node, "title": title, "message": message,
            "w": r2(shown.size.0), "h": r2(shown.size.1), "rows": rows, "headers": headers,
        })
    }
}

#[cfg(test)]
#[path = "top_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "top_invocation_tests.rs"]
mod invocation_tests;

#[cfg(test)]
#[path = "top_section_tests.rs"]
mod section_tests;
