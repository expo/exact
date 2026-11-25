//! `bake-geometry-feedback` (LLP 1123 D6): a geometry fact may feed nothing that moves the
//! geometry it reads through the host: a route's bar and navigation, the tab bar, a toolbar the
//! host places, the system bars, a presentation, a scroll position, a reveal, focus, or the
//! window's fitting and orientation. The host reports such a move in a later turn, and the
//! run-time bound on a chain of commits ends with its turn, so the bake refuses every path from
//! a fact to one: directly, through a branch of the view or of an action, through derives,
//! resources, stored state and mutations, and through an action that runs without the person,
//! a task's, a sequence's, that of an event the person does not make, or the `then` of a
//! mutation one of those sends. An event the person makes runs its action once per event, so
//! what the action reads of a fact at that moment cannot feed back, as a UIKit app chooses a
//! popover or a sheet by the width it has when a button is pressed. The bake does not yet work
//! out which owner a node is in, so it refuses a path to any owner's chrome, not only the
//! reading node's owner and its ancestors.

use contract_checking::{Site, Structure, Visit};
use contract_lower::tags::{self, AttrTarget};
use contract_lower::LowerError;
use contract_syntax::inline::calls::shown;
use contract_syntax::{Attr, Component, Expr, ModuleDecl, Span, Stmt, StyleDecl};
use contract_types::Checked;
use exact_runner::native_geometry::is_geometry;
use std::collections::{BTreeMap, BTreeSet};

/// The refusals of the paths from a geometry fact to what moves it in `checked`'s expanded root.
pub(crate) fn check(checked: &Checked<'_>) -> Vec<LowerError> {
    let root = &checked.expanded.root;
    let fed: BTreeMap<&str, Held> = (root.resources.iter())
        .filter(|r| is_geometry(&r.source))
        .map(|r| {
            (
                r.name.as_str(),
                Held {
                    fact: &r.name,
                    writer: None,
                },
            )
        })
        .collect();
    if fed.is_empty() {
        return Vec::new();
    }
    let checking = contract_checking::Checking::new(checked);
    let mut fence = Fence {
        root,
        task_args: &checked.expanded.task_args,
        then_args: &checked.expanded.mutation_then_args,
        styles: &checked.file.styles,
        modules: &checked.file.modules,
        view_nodes: checking.expanded().structure(),
        slot: checked.file.routes.as_ref().map(|r| r.slot.as_str()),
        fed,
        runs: BTreeMap::new(),
        auto: BTreeMap::new(),
        view: None,
    };
    // Each pass adds what the last one made known, keeping the first way each name was reached;
    // a pass that adds nothing reports the sinks
    loop {
        let mut flows = Vec::new();
        fence.pass(&mut flows);
        let mut grew = false;
        let mut sinks = Vec::new();
        for flow in flows {
            grew |= match flow {
                Flow::Fed(name, read, writer) => first(
                    &mut fence.fed,
                    name,
                    Held {
                        fact: read.fact,
                        writer,
                    },
                ),
                Flow::Runs(action, read) => first(&mut fence.runs, action, read),
                Flow::Auto(action, trigger) => first(&mut fence.auto, action, trigger),
                Flow::View(read) => {
                    let fresh = fence.view.is_none();
                    fence.view.get_or_insert(read);
                    fresh
                }
                Flow::Sink(sink) => {
                    sinks.push(sink);
                    false
                }
            };
        }
        if !grew {
            return fence.refusals(sinks);
        }
    }
}

/// Insert `value` at `key` unless the map holds one there; whether it did.
fn first<'a, V>(map: &mut BTreeMap<&'a str, V>, key: &'a str, value: V) -> bool {
    match map.entry(key) {
        std::collections::btree_map::Entry::Vacant(e) => {
            e.insert(value);
            true
        }
        std::collections::btree_map::Entry::Occupied(_) => false,
    }
}

/// What `root`'s names hold, as far as its geometry facts go.
struct Fence<'a> {
    root: &'a Component,
    /// Each task's and each mutation's `then` action's captured props, as the inliner lifted
    /// a child's action with them (`Expanded::task_args`, `mutation_then_args`).
    task_args: &'a [Vec<Expr>],
    then_args: &'a [Vec<Expr>],
    styles: &'a [StyleDecl],
    /// The modules' events, each bound to an action of the root.
    modules: &'a [ModuleDecl],
    view_nodes: Structure<'a>,
    slot: Option<&'a str>,
    /// Each name a fact feeds: a fact itself, and each derive, state, resource, mutation and
    /// view binder whose value one decides.
    fed: BTreeMap<&'a str, Held<'a>>,
    /// Each action that runs on a fact, with what it runs on.
    runs: BTreeMap<&'a str, Read<'a>>,
    /// Each action that runs without the person, with what runs it.
    auto: BTreeMap<&'a str, Trigger<'a>>,
    /// The first fact the view reads: the layout follows it, so every action a layout event
    /// raises runs on it.
    view: Option<Read<'a>>,
}

/// The fact a name holds, and the action that wrote it there, if one did.
#[derive(Clone, Copy)]
struct Held<'a> {
    fact: &'a str,
    writer: Option<&'a str>,
}

/// A name read and the fact it holds.
#[derive(Clone, Copy)]
struct Read<'a> {
    name: &'a str,
    fact: &'a str,
}

/// A local name and the fact it holds, if any.
type Local<'a> = (&'a str, Option<Read<'a>>);

/// What runs an action without the person.
#[derive(Clone, Copy)]
enum Trigger<'a> {
    /// The task of this name.
    Task(&'a str),
    /// The sequence of this name, when it ends.
    Sequence(&'a str),
    /// The event a layout raises.
    Layout(&'a str),
    /// Another event the person does not make.
    Event(&'a str),
    /// The reply to the mutation of this name.
    Reply(&'a str),
}

/// What a pass learns: a name a fact feeds and the action that wrote it, an action that runs
/// on one or without the person, that the view reads one, or a sink one reaches.
enum Flow<'a> {
    Fed(&'a str, Read<'a>, Option<&'a str>),
    Runs(&'a str, Read<'a>),
    Auto(&'a str, Trigger<'a>),
    View(Read<'a>),
    Sink(Sink<'a>),
}

/// A place a fact reaches that moves it.
struct Sink<'a> {
    /// What is written there, as the message names it: an attribute, a command, an assignment.
    what: String,
    /// What it moves, as the message says it.
    moves: &'static str,
    read: Read<'a>,
    how: How,
    /// The action it stands in.
    action: Option<&'a str>,
    span: Span,
}

/// How a fact reaches a sink.
#[derive(Clone, Copy)]
enum How {
    /// The sink's value is computed from it.
    Follows,
    /// A branch on it decides whether the sink is there.
    Branch,
    /// The sink is a branch on it.
    Chooses,
    /// It decides that the sink's action runs: a task's gate or key, a sequence's, an event's
    /// arguments, or the element or view its event follows.
    Runs,
}

/// What reaches a statement: what decides that it runs, else the fact its value reads.
fn via<'a>(on: Option<(Read<'a>, How)>, value: Option<Read<'a>>) -> Option<(Read<'a>, How)> {
    on.or(value.map(|read| (read, How::Follows)))
}

const BAR: &str = "the route's bar (its first `header`)";
const TAB_BAR: &str = "the tab bar";
const TOOLBAR: &str = "a toolbar the host places";
const ROUTE: &str = "a route's bar or navigation";
const PRESENTATION: &str = "a presentation";
const SCROLL: &str = "a scroll position";
const FOCUS: &str = "focus";
const WINDOW: &str = "the window's fitting or orientation";

/// The events a layout raises, whose actions run on whatever the layout read.
const LAYOUT_EVENTS: &[&str] = &[
    "resize",
    "intersect",
    "reachstart",
    "reachend",
    "transformgeometry",
];

/// The events the person makes, each of which runs its action once; `navigate` is the person's
/// but on an `iframe`, whose guest reports it. A scroll a layout clamps or a command raises,
/// and a blur when the focused field goes, are counted among them.
const PERSON_EVENTS: &[&str] = &[
    "press",
    "dblclick",
    "contextmenu",
    "hover",
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "pan",
    "panrelease",
    "wheel",
    "scroll",
    "scrollend",
    "scrolldragstart",
    "scrolldragend",
    "swipeleft",
    "swiperight",
    "swipe",
    "key",
    "keyup",
    "keycapture",
    "beforeinput",
    "input",
    "change",
    "select",
    "selectionchange",
    "submit",
    "copy",
    "cut",
    "paste",
    "drop",
    "dragenter",
    "dragleave",
    "reorderdrop",
    "reordertargetchange",
    "historyswipe",
    "navigate",
    "refresh",
    "atompress",
    "heightrelease",
    "transformrelease",
    "focus",
    "blur",
    "focusin",
    "focusout",
    "toggle",
    "close",
    "cancel",
    "nexttrack",
    "previoustrack",
    "seekbackward",
    "seekforward",
    "seekto",
    "stop",
];

/// What a command moves, if it presents, scrolls, reveals, focuses or fits the window.
fn command_moves(name: &str) -> Option<&'static str> {
    Some(match name {
        "showModal"
        | "close"
        | "showPopover"
        | "hidePopover"
        | "togglePopover"
        | "showPicker"
        | "requestFullscreen"
        | "exitFullscreen"
        | "requestPictureInPicture"
        | "exitPictureInPicture"
        | "share"
        | "saveFile"
        | "showOpenFilePicker"
        | "showDirectoryPicker"
        | "showSaveFilePicker"
        | "requestPermission" => PRESENTATION,
        "scrollBy" => SCROLL,
        "scrollIntoView" => "a reveal",
        "focus" | "blur" | "selectText" | "setSelectionRange" | "removeAllRanges"
        | "setRangeText" => FOCUS,
        "setPageZoom" => WINDOW,
        _ => return None,
    })
}

/// What an attribute moves when its value follows a fact, if it sets a route's bar or
/// navigation, a presentation, a scroll position, focus or the window's fitting. An attribute
/// that moves no geometry (the status bar's style, the window's title, a sheet's grabber and
/// `closedby`) is not one, nor is an invoker, which presents only when the person presses it.
fn attr_moves(name: &str) -> Option<&'static str> {
    const PRESENTS: &[&str] = &[
        "navigationPresentation",
        "navigationDetent",
        "navigationInitialDetent",
        "navigationSelectedDetent",
        "navigationSizing",
        "navigationEdgeAttachedInCompactHeight",
        "navigationPopoverAnchor",
        "navigationPopoverMargins",
        "navigationPopoverArrows",
        "navigationPopoverOverlapsSource",
        "navigationPopoverAdaptation",
        "navigationSource",
        "sharedElement",
        "popover",
        "tip",
        "emojiPicker",
        "entersFullScreenWhenPlaybackBegins",
    ];
    const SCROLLS: &[&str] = &[
        "scrollTop",
        "scrollLeft",
        "scrollZoom",
        "scrollFollowEnd",
        "scroll-start",
        "scroll-restoration",
        "refreshing",
    ];
    if PRESENTS.contains(&name) {
        Some(PRESENTATION)
    } else if name == "-exact-safe-area-aspect-fit" {
        Some(WINDOW)
    } else if SCROLLS.contains(&name)
        || ["scroll-snap-", "scroll-padding", "scroll-margin"]
            .iter()
            .any(|p| name.starts_with(p))
    {
        Some(SCROLL)
    } else {
        mounted_moves(name)
    }
}

/// What an attribute moves when a branch on a fact decides whether its node is there: a route's
/// bar or navigation, a toolbar's place, focus, the system bars or the window's fitting, each
/// of which acts when the node mounts.
fn mounted_moves(name: &str) -> Option<&'static str> {
    match name {
        "navigationGrabber" | "navigationScrollExpands" | "navigationUndimmedDetent" => None,
        _ if name.starts_with("navigation") => Some(ROUTE),
        "toolbarPlacement" => Some(TOOLBAR),
        "autofocus" => Some(FOCUS),
        "system-bars-hidden" => Some("the system bars"),
        "orientationlock"
        | "viewport-fit"
        | "interactive-widget"
        | "-exact-safe-area-aspect-fit" => Some(WINDOW),
        _ => None,
    }
}

/// Where a view node sits.
#[derive(Clone)]
struct Place<'a> {
    /// The fact a branch around it reads, which decides whether it is there.
    branch: Option<Read<'a>>,
    locals: Vec<Local<'a>>,
    /// Inside a navigation root, and a route.
    in_root: bool,
    in_route: bool,
    /// The chrome it is in: a route's bar, the tab bar or a toolbar the host places.
    chrome: Option<&'static str>,
    /// The bar headers among the nodes a route's children open with.
    headers: std::rc::Rc<BTreeSet<Site>>,
}

impl<'a> Fence<'a> {
    fn pass(&self, out: &mut Vec<Flow<'a>>) {
        let root = self.root;
        for b in root.states.iter().chain(&root.derives) {
            if let Some(read) = self.reads(&b.expr, &[]) {
                out.push(Flow::Fed(&b.name, read, self.writer(read)));
            }
        }
        for r in root.resources.iter().filter(|r| !is_geometry(&r.source)) {
            // Its arguments and key read each item, which a gate and a list never name
            let item: Vec<Local<'a>> = r.each.iter().map(|e| (e.item.as_str(), None)).collect();
            let each = r.each.iter().flat_map(|e| [Some(&e.list), e.key.as_ref()]);
            let read = (r.args.iter().chain(&r.gate).chain(each.flatten()))
                .find_map(|e| self.reads(e, &item));
            if let Some(read) = read {
                out.push(Flow::Fed(&r.name, read, self.writer(read)));
            }
        }
        for t in &root.tasks {
            let (action, trigger) = (t.timer.1.as_str(), Trigger::Task(&t.name));
            out.push(Flow::Auto(action, trigger));
            let read = (t.gate.iter().chain(&t.key).chain([&t.timer.0]))
                .chain(t.options.iter().map(|(_, e, _)| e))
                .find_map(|e| self.reads(e, &[]));
            out.extend(read.map(|read| Flow::Runs(action, read)));
        }
        for s in &root.sequences {
            let Some((action, _)) = &s.ended else {
                continue;
            };
            out.push(Flow::Auto(action, Trigger::Sequence(&s.name)));
            let read = (s.gate.iter().chain(&s.key).chain([&s.tracks]))
                .chain(s.options.iter().map(|(_, e, _)| e))
                .find_map(|e| self.reads(e, &[]));
            out.extend(read.map(|read| Flow::Runs(action, read)));
        }
        // A module's event the device or the platform raises, not the person's act
        for m in self.modules.iter().filter(|m| m.raised != "activation") {
            if let Some(action) = &m.handler {
                out.push(Flow::Auto(action, Trigger::Event(&m.declared)));
            }
        }
        let captured = self.captured();
        for a in &root.actions {
            let name = a.name.as_str();
            let on = self.runs.get(name).map(|&read| (read, How::Runs));
            // An action only the person runs reads a fact once per event, which cannot feed back
            if on.is_some() || self.auto.contains_key(name) {
                let mut locals = captured.get(name).cloned().unwrap_or_default();
                self.body(name, &a.body, on, &mut locals, out);
            }
        }
        let place = Place {
            branch: None,
            locals: Vec::new(),
            in_root: false,
            in_route: false,
            chrome: None,
            headers: Default::default(),
        };
        let walk = (self.view_nodes).walk_regions_with(place, |node, at| self.node(node, at, out));
        walk.for_each(drop);
    }

    /// What each lifted action's captured props hold, from the tasks that run it and the
    /// mutations whose `then` it is: a captured prop is fed where any of them passes a fed value.
    fn captured(&self) -> BTreeMap<&'a str, Vec<Local<'a>>> {
        let root = self.root;
        let tasks = (root.tasks.iter().map(|t| t.timer.1.as_str())).zip(self.task_args);
        let thens = (root.mutations.iter().zip(self.then_args))
            .filter_map(|(m, args)| Some((m.then.as_ref()?.0.as_str(), args)));
        let mut out: BTreeMap<&'a str, Vec<Local<'a>>> = BTreeMap::new();
        for (action, args) in tasks.chain(thens) {
            let Some(a) = root.actions.iter().find(|a| a.name == action) else {
                continue;
            };
            let bound = out.entry(action).or_default();
            let params = a.params.iter().filter(|p| p.name.starts_with("@capture:"));
            for (p, arg) in params.zip(args) {
                let read = self.reads(arg, &[]);
                match bound.iter_mut().find(|(n, _)| *n == p.name) {
                    Some(local) => local.1 = local.1.or(read),
                    None => bound.push((&p.name, read)),
                }
            }
        }
        out
    }

    /// The action that wrote the fact `read` holds into the name it reads, or into one that name
    /// follows.
    fn writer(&self, read: Read<'a>) -> Option<&'a str> {
        self.fed.get(read.name).and_then(|h| h.writer)
    }

    /// The first fact `e` reads, with the name that holds it.
    fn reads(&self, e: &'a Expr, locals: &[Local<'a>]) -> Option<Read<'a>> {
        match e {
            Expr::Ident(name, _) => match locals.iter().rev().find(|(l, _)| l == name) {
                Some(&(_, read)) => read,
                None => (self.fed.get(name.as_str())).map(|h| Read { name, fact: h.fact }),
            },
            _ => e.scoped_operands().find_map(|(x, binds)| {
                if binds.is_empty() {
                    return self.reads(x, locals);
                }
                let mut inner = locals.to_vec();
                inner.extend(binds.iter().map(|b| (b, None)));
                self.reads(x, &inner)
            }),
        }
    }

    fn any(&self, es: &'a [Expr], locals: &[Local<'a>]) -> Option<Read<'a>> {
        es.iter().find_map(|e| self.reads(e, locals))
    }

    /// The statements of `action`, `on` what decides that they run when a fact does.
    fn body(
        &self,
        action: &'a str,
        body: &'a [Stmt],
        on: Option<(Read<'a>, How)>,
        locals: &mut Vec<Local<'a>>,
        out: &mut Vec<Flow<'a>>,
    ) {
        let depth = locals.len();
        let sink = |what: String, moves, (read, how): (Read<'a>, How), span| Sink {
            what,
            moves,
            read,
            how,
            action: Some(action),
            span,
        };
        for s in body {
            match s {
                Stmt::Let { name, expr, .. } => {
                    let read = via(on, self.reads(expr, locals)).map(|(read, _)| read);
                    locals.push((name, read));
                }
                Stmt::Assign { target, expr, span } => {
                    let Some(reached) = via(on, self.reads(expr, locals)) else {
                        continue;
                    };
                    if Some(target.as_str()) == self.slot {
                        let what = format!("`{target} = …`");
                        out.push(Flow::Sink(sink(what, PRESENTATION, reached, *span)));
                    } else {
                        out.push(Flow::Fed(target, reached.0, Some(action)));
                    }
                }
                Stmt::Command { name, args, span } => {
                    let (Some(moves), Some(reached)) =
                        (command_moves(name), via(on, self.any(args, locals)))
                    else {
                        continue;
                    };
                    out.push(Flow::Sink(sink(
                        format!("`{name}(…)`"),
                        moves,
                        reached,
                        *span,
                    )));
                }
                Stmt::Send { target, args, .. } => {
                    let mutation = self.root.mutations.iter().find(|m| &m.name == target);
                    if let Some((then, _)) = mutation.and_then(|m| m.then.as_ref()) {
                        out.push(Flow::Auto(then, Trigger::Reply(target)));
                    }
                    let Some((read, _)) = via(on, self.any(args, locals)) else {
                        continue;
                    };
                    out.push(Flow::Fed(target, read, Some(action)));
                    if let Some(m) = mutation {
                        (m.refreshes.iter())
                            .for_each(|(r, _)| out.push(Flow::Fed(r, read, Some(action))));
                        if let Some((then, _)) = &m.then {
                            out.push(Flow::Runs(then, read));
                        }
                    }
                }
                Stmt::Refresh { target, .. } => {
                    if let Some((read, _)) = on {
                        out.push(Flow::Fed(target, read, Some(action)));
                    }
                }
                Stmt::If {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    let inner = on.or(self.reads(cond, locals).map(|read| (read, How::Branch)));
                    self.body(action, then, inner, locals, out);
                    self.body(action, otherwise, inner, locals, out);
                }
                Stmt::Match {
                    subject,
                    some: (bound, some),
                    none,
                    ..
                } => {
                    let inner = on.or(self.reads(subject, locals).map(|read| (read, How::Branch)));
                    locals.push((bound, inner.map(|(read, _)| read)));
                    self.body(action, some, inner, locals, out);
                    locals.pop();
                    self.body(action, none, inner, locals, out);
                }
                // The expanded call's body binds its parameters as `let`s
                Stmt::Call { body, .. } => self.body(action, body, on, locals, out),
            }
        }
        locals.truncate(depth);
    }

    /// What `node` at `at` gives its two regions.
    fn node(
        &self,
        node: Visit<'a>,
        at: &Place<'a>,
        out: &mut Vec<Flow<'a>>,
    ) -> [Option<Place<'a>>; 2] {
        if let Some((tag, positional, attrs)) = node.element() {
            let mut inner = at.clone();
            let keyed = has(attrs, "navigationKey");
            let route = at.in_root && keyed && !has(attrs, "navigationBack");
            inner.in_root |= keyed;
            inner.in_route |= route;
            // A `display` that follows a fact decides whether the node shows, as a branch does
            let displayed = (attrs.iter())
                .filter(|a| {
                    a.name == "display"
                        || (a.name == "class" && self.class_has(&a.value, "display"))
                })
                .find_map(|a| self.reads(&a.value, &at.locals));
            inner.branch = inner.branch.or(displayed);
            // A computed `role` may be the tab bar's, so it counts as one
            let tab_bar = at.in_root
                && !at.in_route
                && has(attrs, "role")
                && literal(attrs, "role").is_none_or(|r| r == "tablist");
            inner.chrome = (at.chrome)
                .or(at.headers.contains(&node.site()).then_some(BAR))
                .or(tab_bar.then_some(TAB_BAR))
                .or(has(attrs, "toolbarPlacement").then_some(TOOLBAR));
            self.element(tag, positional, attrs, node.span(), &inner, out);
            let mut bars = BTreeSet::new();
            if route && literal(attrs, "navigationBar") != Some("none") {
                let mut leading = Vec::new();
                first_headers(node.regions()[0], &mut bars, &mut leading);
                // A branch before the bar's header decides whether the route has a bar
                for region in leading {
                    let read = region
                        .expressions()
                        .find_map(|e| self.reads(e, &inner.locals));
                    if let Some(read) = read {
                        let what = format!("`{}`", keyword(region));
                        out.push(Flow::Sink(view_sink(
                            what,
                            BAR,
                            read,
                            How::Chooses,
                            region.span(),
                        )));
                    }
                }
            }
            inner.headers = bars.into();
            return [Some(inner.clone()), Some(inner)];
        }
        if let Some((_, var, index, list, key)) = node.repetition() {
            let read = self.reads(list, &at.locals);
            let bound: Vec<&str> = [var].into_iter().chain(index).collect();
            let mut inner = self.branch(at, read, &bound, out);
            // A key decides each row's identity, so a row whose key follows a fact is made again
            inner.branch = inner.branch.or(self.reads(key, &inner.locals));
            return [Some(inner.clone()), Some(inner)];
        }
        if node.is_choice() {
            let read = node.expressions().find_map(|e| self.reads(e, &at.locals));
            let [some, none] = node.bindings().map(|b| b.iter().collect::<Vec<_>>());
            let arms = [
                self.branch(at, read, &some, out),
                self.branch(at, read, &none, out),
            ];
            return arms.map(Some);
        }
        [Some(at.clone()), Some(at.clone())]
    }

    /// `at` under a branch on `read`, which binds `bound` to what it reads.
    fn branch(
        &self,
        at: &Place<'a>,
        read: Option<Read<'a>>,
        bound: &[&'a str],
        out: &mut Vec<Flow<'a>>,
    ) -> Place<'a> {
        let mut inner = at.clone();
        inner.branch = at.branch.or(read);
        inner.locals.extend(bound.iter().map(|&b| (b, read)));
        // A lifted child's declarations read a binder by its expanded name
        if let Some(read) = read {
            let writer = self.writer(read);
            out.extend(bound.iter().map(|&b| Flow::Fed(b, read, writer)));
        }
        inner
    }

    /// An element's attributes and positional values at `at`.
    fn element(
        &self,
        tag: &'a str,
        positional: &'a [Expr],
        attrs: &'a [Attr],
        span: Span,
        at: &Place<'a>,
        out: &mut Vec<Flow<'a>>,
    ) {
        let (handlers, values): (Vec<&Attr>, Vec<&Attr>) =
            (attrs.iter()).partition(|a| self.event(tag, a).is_some());
        let read = |e| self.reads(e, &at.locals);
        // What the element reads: a branch around it, or one of its values
        let element = (at.branch)
            .or_else(|| values.iter().find_map(|a| read(&a.value)))
            .or_else(|| positional.iter().find_map(read));
        for a in handlers {
            let Some(event) = self.event(tag, a) else {
                continue;
            };
            let (action, args): (&str, &[Expr]) = match &a.value {
                Expr::Ident(n, _) => (n, &[]),
                Expr::Call(n, args, _) => (n, args),
                _ => continue,
            };
            if PERSON_EVENTS.contains(&event) && !(event == "navigate" && tag == "iframe") {
                continue;
            }
            let layout = LAYOUT_EVENTS.contains(&event);
            let trigger = if layout {
                Trigger::Layout(event)
            } else {
                Trigger::Event(event)
            };
            out.push(Flow::Auto(action, trigger));
            let ran = (self.any(args, &at.locals))
                .or(element)
                .or(self.view.filter(|_| layout));
            out.extend(ran.map(|read| Flow::Runs(action, read)));
        }
        // A branch on a fact decides whether the element is there, and with it each attribute
        // that acts when its node mounts
        if let Some(read) = at.branch {
            out.push(Flow::View(read));
            let moves = (values.iter().find_map(|a| mounted_moves(&a.name))).or(at.chrome);
            if let Some(moves) = moves {
                let what = format!("`{tag}`");
                out.push(Flow::Sink(view_sink(what, moves, read, How::Branch, span)));
            }
        }
        for a in values {
            let Some(read) = read(&a.value) else {
                continue;
            };
            out.push(Flow::View(read));
            let moves = match a.name.as_str() {
                "class" => self.class_moves(&a.value),
                name => attr_moves(name),
            };
            if let Some(moves) = moves.or(at.chrome) {
                let what = format!("`{}`", a.name);
                out.push(Flow::Sink(view_sink(
                    what,
                    moves,
                    read,
                    How::Follows,
                    a.span,
                )));
            }
        }
        if let Some(read) = self.any(positional, &at.locals) {
            out.push(Flow::View(read));
            if let Some(chrome) = at.chrome {
                out.push(Flow::Sink(view_sink(
                    format!("`{tag}`"),
                    chrome,
                    read,
                    How::Follows,
                    span,
                )));
            }
        }
    }

    /// The event an attribute of `tag` handles: a handler's, or a package view's declared
    /// event, an attribute a module tag has that names an action of the root.
    fn event(&self, tag: &str, a: &'a Attr) -> Option<&'a str> {
        if let Some(AttrTarget::Handler(event)) = tags::attr_valued(&a.name, &a.value) {
            return Some(event);
        }
        let (Expr::Ident(action, _) | Expr::Call(action, _, _)) = &a.value else {
            return None;
        };
        let declared =
            contract_lower::is_module_tag(tag) && tags::attr_valued(&a.name, &a.value).is_none();
        (declared && self.root.actions.iter().any(|x| &x.name == action)).then_some(a.name.as_str())
    }

    /// The styles a `class` value names.
    fn styles_named(&self, value: &'a Expr) -> impl Iterator<Item = &'a StyleDecl> {
        let mut names = Vec::new();
        for e in value.walk() {
            if let Expr::Ident(n, _) | Expr::Str(n, _) = e {
                names.extend(n.split_whitespace());
            }
        }
        (self.styles.iter()).filter(move |s| names.contains(&s.name.as_str()))
    }

    /// Whether a style a `class` value names has the row `name`.
    fn class_has(&self, value: &'a Expr, name: &str) -> bool {
        self.styles_named(value)
            .any(|s| s.attrs.iter().any(|a| a.name == name))
    }

    /// What a `class` chooses, if one of the styles its value names moves something.
    fn class_moves(&self, value: &'a Expr) -> Option<&'static str> {
        (self.styles_named(value).flat_map(|s| &s.attrs)).find_map(|a| attr_moves(&a.name))
    }

    /// One refusal per place, a component used twice being inlined twice.
    fn refusals(&self, sinks: Vec<Sink<'_>>) -> Vec<LowerError> {
        let mut seen = BTreeSet::new();
        sinks
            .into_iter()
            .filter(|s| seen.insert((s.span.source_id, s.span.line, s.span.col, s.what.clone())))
            .map(|s| LowerError {
                id: "bake-geometry-feedback",
                message: self.message(&s),
                span: s.span,
            })
            .collect()
    }

    fn message(&self, s: &Sink<'_>) -> String {
        let fact = shown(s.read.fact);
        let writer = self.fed.get(s.read.name).and_then(|h| h.writer);
        let held = match (s.read.name == s.read.fact, writer) {
            (true, _) => format!("the geometry fact `{fact}`"),
            (false, Some(w)) => format!(
                "`{}`, which holds the geometry fact `{fact}` through action `{}`",
                shown(s.read.name),
                shown(w)
            ),
            (false, None) => {
                format!(
                    "`{}`, which holds the geometry fact `{fact}`",
                    shown(s.read.name)
                )
            }
        };
        let how = match (s.how, s.action) {
            (How::Runs, Some(a)) => format!("is in action `{}`, which runs on", shown(a)),
            (How::Branch, _) => "is under a branch on".to_string(),
            (How::Chooses, _) => "branches on".to_string(),
            _ => "follows".to_string(),
        };
        // What runs the action the sink stands in, or the action that wrote what it reads
        let actor = s.action.or(writer);
        let runs = actor
            .and_then(|a| Some((a, self.auto.get(a)?)))
            .map(|(a, t)| trigger(shown(a), *t))
            .unwrap_or_default();
        format!(
            "{} {how} {held}, so {} would follow where layout puts a box. {runs}That moves the geometry the fact reads, and the host reports the move in a later turn, which the bound on a chain of commits does not hold, so the two could chase each other without end (LLP 1123 D6). Decide it in an action the person runs, or from state such an action writes, and let a geometry fact decide only content: sizes, text and which content shows",
            s.what, s.moves
        )
    }
}

/// What runs `action`, as a sentence.
fn trigger(action: &str, t: Trigger<'_>) -> String {
    match t {
        Trigger::Task(task) => format!("The task `{}` runs `{action}`. ", shown(task)),
        Trigger::Sequence(s) => {
            format!("The sequence `{}` runs `{action}` when it ends. ", shown(s))
        }
        Trigger::Layout(event) => format!("`{event}` runs `{action}` at every layout. "),
        Trigger::Event(event) => format!("`{event}` runs `{action}` without the person. "),
        Trigger::Reply(m) => format!("The reply to `{}` runs `{action}`. ", shown(m)),
    }
}

fn view_sink<'a>(
    what: String,
    moves: &'static str,
    read: Read<'a>,
    how: How,
    span: Span,
) -> Sink<'a> {
    Sink {
        what,
        moves,
        read,
        how,
        action: None,
        span,
    }
}

/// The `header`s that may be a route's first child, and the regions before them: its first
/// node, or the first node of a branch of the `if`s, `match`es and `for`s it opens with, each
/// of which may yield nothing and leave the next to be first. A `header` that says
/// `appearance="none"` is the route's content, not its bar.
fn first_headers<'a>(
    children: Structure<'a>,
    out: &mut BTreeSet<Site>,
    leading: &mut Vec<Visit<'a>>,
) {
    use std::{cell::Cell, rc::Rc};
    let walk = children.walk_regions_with(Rc::new(Cell::new(false)), |node, stopped| {
        if stopped.get() {
            return [None, None];
        }
        if node.is_choice() || node.repetition().is_some() {
            leading.push(node);
            return [
                Some(Rc::new(Cell::new(false))),
                Some(Rc::new(Cell::new(false))),
            ];
        }
        stopped.set(true);
        let header = node.element().filter(|(tag, _, attrs)| {
            *tag == "header" && literal(attrs, "appearance") != Some("none")
        });
        if header.is_some() {
            out.insert(node.site());
        }
        [None, None]
    });
    walk.for_each(drop);
}

/// The word a region is written with.
fn keyword(region: Visit<'_>) -> &'static str {
    match (
        region.repetition().is_some(),
        region.bindings()[0].is_empty(),
    ) {
        (true, _) => "for",
        (false, true) => "if",
        (false, false) => "match",
    }
}

fn has(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name == name)
}

/// The last written value of `name`, where it is a literal.
fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            Expr::Str(s, _) => Some(s.as_str()),
            _ => None,
        })
}
