//! Lowering's warnings (@ref LLP 1116 D6): what compiles and runs but is
//! not what the author meant, where a host would drop it without a word.
//! Beside the refusals (`lint.rs`, [`crate::lower_all`]), never instead of
//! them: a warning fails no build and no test, and names its repair as a
//! refusal does.
//!
//! Each check names what it can establish from the expanded root, as lowering reads it.
//! Unexpanded uses and slots, and computed roles and types, stay unknown.

use crate::tags::AttrTarget;
use crate::LowerError;
use contract_syntax::{
    Attr, BinOp, Binding, Component, Expr, FnDecl, Node, Span, Stmt, StyleDecl, TemplatePart, UnOp,
};
use contract_types::Checked;
use exact_kernel::StyleId;
use exact_plan::Stdlib;
use std::collections::{BTreeMap, BTreeSet};

mod container;
mod counts;
mod fixed_beside;
mod geometry;
mod header_bar;
mod launch;
pub(crate) mod line_height;
mod miswiring;
mod nested_header;
mod parts;
mod roles;
mod root_replace;
mod routeless_header;
mod rows;
mod screens;
mod scroll_bound;
mod sounds;
mod states;
mod svg;
mod tabs;
mod text_styles;
mod title;
mod transform;
mod transform_drag;
mod width;
pub(crate) use width::horizontal_flex;

/// At most this many warnings from one compile. A blank SF Symbol's uses,
/// `tamber-unreachable` and colours that stay one scheme follow, each site briefly: a repair
/// of the first twenty would only show the next (rules/RULES.md, "Loop shape").
pub const MAX_WARNINGS: usize = 20;

/// Every warning the checked file earns, in source order of its checks.
pub fn warnings(checked: &Checked<'_>) -> Vec<LowerError> {
    crate::control_roles::unlowered();
    with_text_styles(checked, &TextStyles::new(), Vec::new())
}

pub(crate) use text_styles::{TextStyle, TextStyles};

pub(crate) fn with_text_styles(
    checked: &Checked<'_>,
    lowered: &TextStyles,
    mut out: Vec<LowerError>,
) -> Vec<LowerError> {
    let root = &checked.expanded.root;
    routeless_header::check(checked, &mut out);
    // Track the navigation root (the outermost `navigationKey`) and its `navigationBack`.
    crate::menu_rows::notes(&root.view, &mut out);
    crate::symbols::tints(&root.view, checked.file, lowered, &mut out);
    let mut stack: Vec<(&Node, bool, Option<&str>)> =
        root.view.iter().rev().map(|n| (n, false, None)).collect();
    let (mut seen, mut platform) = (BTreeSet::new(), Vec::new());
    while let Some((node, in_root, back)) = stack.pop() {
        let (children, in_root, back): (Vec<&Node>, bool, Option<&str>) = match node {
            Node::Element {
                tag,
                positional,
                attrs,
                children,
                ..
            } => {
                // A component used twice is inlined twice: one line for its source.
                let span = node.span();
                let fresh = seen.insert((span.source_id, span.line, span.col, span.end_col));
                // @ref LLP 1116.007 D3.1 — a symbol by another spelling than its role.
                if let (true, "image", Some(source)) = (fresh, tag.as_str(), positional.first()) {
                    out.extend(crate::symbols::image_source(source, &mut platform));
                }
                out.extend(roles::button(tag, attrs, children, span).filter(|_| fresh));
                out.extend(roles::dialog_role(tag, attrs, span).filter(|_| fresh));
                out.extend(roles::level_without_heading(tag, attrs, span).filter(|_| fresh));
                if fresh {
                    line_height::multiplier(attrs, &mut out);
                }
                let keyed = has(attrs, "navigationKey");
                // @ref LLP 1116.010 D5 — a name or a key that meets nothing.
                miswiring::swipe(attrs, children, fresh, &mut out);
                if tag == "image" {
                    miswiring::image_source(positional.first(), fresh, &mut out);
                }
                if keyed && !in_root {
                    let declared = checked.file.routes.as_ref().map(|r| r.span);
                    miswiring::navigation_keys(attrs, children, declared, fresh, &mut out);
                }
                miswiring::navigation_split(tag, attrs, keyed, keyed && !in_root, fresh, &mut out);
                let route = in_root && keyed && !has(attrs, "navigationBack");
                // A route that says it has no bar (LLP 1115.000 D5.2) draws its header as written.
                let no_bar = literal(attrs, "navigationBar") == Some("none");
                if route && !no_bar {
                    header_bar::route_header(children, &[], &mut out);
                    nested_header::nested_header(children, &mut out);
                    fixed_beside::fixed_beside((tag, attrs), children, &mut out);
                }
                if let (true, Some(name)) = (route, back) {
                    drawn_back(attrs, children, name, &mut out);
                }
                if literal(attrs, "role") == Some("tablist") {
                    tabs::single_tab(node, &mut out);
                    tabs::search_tab(node, &mut out);
                }
                let back = if keyed && !in_root {
                    literal(attrs, "navigationBack").filter(|name| !name.is_empty())
                } else {
                    back
                };
                (children.iter().collect(), in_root || keyed, back)
            }
            Node::When {
                then, otherwise, ..
            } => (then.iter().chain(otherwise).collect(), in_root, back),
            Node::Each { body, .. } => (body.iter().collect(), in_root, back),
            Node::Match { some, none, .. } => (some.1.iter().chain(none).collect(), in_root, back),
            Node::Use { children, .. } => (children.iter().collect(), in_root, back),
            Node::Children { .. } => (Vec::new(), in_root, back),
        };
        stack.extend(children.into_iter().rev().map(|n| (n, in_root, back)));
    }
    (checked.file.styles.iter()).for_each(|s| line_height::multiplier(&s.attrs, &mut out));
    out.extend(screens::screen_by_state(checked));
    // A `replace` the router refuses on a root route's screen, a `select` after a push.
    out.extend(root_replace::router(checked));
    out.extend(counts::count_plural(checked));
    // @ref LLP 1116.016 D4 — a row that searches another data list; LLP 1017.003 D3, its own; LLP 1142 D6, a key by position.
    out.extend(rows::row_lookup(checked));
    // @ref LLP 1116.020 D1 — a `delete=` no list row holds.
    out.extend(crate::swipe_rows::unplaced(
        &root.view,
        &checked.file.styles,
    ));
    // @ref LLP 1116.011 D2.1 — a RoundButton label of more than a word.
    out.extend(parts::round_button_label(checked));
    // @ref LLP 1116.020 D5 — an empty state drawn as a list's row.
    out.extend(parts::empty_state_in_list(checked));
    // @ref LLP 1116.021 D7 — a summary card in a grouped list's card.
    out.extend(parts::summary_card_in_list(checked));
    // @ref LLP 1121 D8 — a node's own state part it never meets.
    states::dead(&root.view, &mut out);
    performance_now(root, &checked.file.fns, &mut out);
    sounds::undeclared(checked, &mut out);
    geometry::unnamed(checked, &mut out);
    transform::owners(checked, &mut out);
    transform_drag::check(&root.view, &checked.file.styles, &mut out);
    // @ref LLP 1027.005 D6 — an identifying argument of the launch's own.
    launch::launch_identity(root, &checked.file.fns, &mut out);
    width::width_padding(&root.view, &checked.file.styles, &mut out);
    scroll_bound::scroll_grows(&root.view, &checked.file.styles, &mut out);
    container::flex_on_block(&root.view, &checked.file.styles, &mut out);
    width::text_px_width(&root.view, &checked.file.styles, lowered, &mut out);
    svg::use_foreign_object(&root.view, &mut out);
    out.extend(
        contract_analyze::warnings(checked)
            .into_iter()
            .map(|e| LowerError {
                id: e.id,
                message: e.message,
                span: e.span,
            }),
    );
    // @ref LLP 1121 D15 — the design's values of params the app no longer has.
    if let Some(source) = &checked.file.design {
        if let Ok(design) = exact_tamber::parse(&source.text) {
            let params = crate::params::tokens(checked.file);
            out.extend(crate::anatomy::unknown_params(
                &design,
                &params,
                checked.file,
                source.source_id,
            ));
        }
    }
    crate::warning_rank::keep_first(&mut out);
    out.extend(platform);
    crate::control_roles::unreachable(&root.view, &checked.file.styles, &mut out);
    crate::style_hosts::undrawn(&root.view, &checked.file.styles, &mut out);
    one_scheme(
        root,
        &checked.file.styles,
        &checked.file.fns,
        crate::theme::theme_decl(checked.file).as_ref(),
        &Tokens {
            one: crate::theme::one_scheme_tokens(checked.file),
            design: checked.file.theme.is_none(),
        },
        &mut out,
    );
    crate::theme::shown_errors(out) // a param by the name its author wrote
}

/// What the one-scheme warning knows of the app's tokens (LLP 1121 D3).
struct Tokens {
    /// The tokens one colour in light and dark, not marked `scheme=fixed`,
    /// with that colour and whether in every tuple (`theme::one_scheme_tokens`).
    one: Vec<(String, String, bool)>,
    /// The tokens are a design's (`app.tamber`), which can mark one, not an inline `theme`'s.
    design: bool,
}

fn has(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name == name)
}

/// An attribute's value when it is a string literal (the last one written).
fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    match attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .map(|a| &a.value)
    {
        Some(Expr::Str(v, _)) => Some(v),
        _ => None,
    }
}

/// Whether an attribute is written with a value the compiler cannot read.
fn computed(attrs: &[Attr], name: &str) -> bool {
    attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .is_some_and(|a| !matches!(a.value, Expr::Str(..)))
}

/// The plain boxes a bar's walk looks inside (LLP 1075.003 §9.10).
const BOXES: &[&str] = &[
    "view", "box", "column", "row", "header", "nav", "section", "footer", "article", "aside",
    "main",
];

fn kids(node: &Node) -> &[Node] {
    match node {
        Node::Element { children, .. } => children,
        _ => &[],
    }
}

/// The `header`s that may be a route's first child: its first node, or the
/// first node of a branch of the `if`s and `match`es it opens with, each
/// of which may yield nothing and leave the next to be first.
fn first_headers<'a>(children: &'a [Node], out: &mut Vec<&'a Node>) {
    for node in children {
        match node {
            Node::When {
                then, otherwise, ..
            } => {
                first_headers(then, out);
                first_headers(otherwise, out);
            }
            Node::Match { some, none, .. } => {
                first_headers(&some.1, out);
                first_headers(none, out);
            }
            Node::Element { tag, .. } => {
                if tag == "header" {
                    out.push(node);
                }
                return;
            }
            _ => return,
        }
    }
}

/// `lower-back-twice`: a route draws its own Back control (the element
/// whose `id` the root's `navigationBack` names) where iOS shows the system
/// Back too (LLP 1115.000 D5.2). Every pushed route has the navigation bar
/// unless it says `navigationBar="none"`, and the bar's Back presses the
/// authored control (LLP 1115 D5), so a control drawn in the page is a
/// second Back on the same screen.
///
/// A control in the route's first `header` is not drawn there on iOS: when
/// the header is the bar it is the bar's Back button, and a header that is
/// no bar is `lower-header-no-bar`'s to name. A sheet (`modal`,
/// `fullscreen`) is the first route of its own stack, with no system Back.
/// A computed `navigationBar`, `navigationPresentation`, `id` or `display`
/// is given the benefit of the doubt.
fn drawn_back(route: &[Attr], children: &[Node], name: &str, out: &mut Vec<LowerError>) {
    if has(route, "navigationBar") && literal(route, "navigationBar") != Some("auto") {
        return;
    }
    if has(route, "navigationPresentation") && literal(route, "navigationPresentation") != Some("")
    {
        return;
    }
    let mut headers = Vec::new();
    first_headers(children, &mut headers);
    let mut stack: Vec<&Node> = children.iter().rev().collect();
    while let Some(node) = stack.pop() {
        if headers.iter().any(|h| std::ptr::eq(*h, node)) {
            continue;
        }
        match node {
            Node::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                // Not drawn in the page: hidden, or shown over it when it
                // opens. A `display` the compiler cannot read is the benefit.
                if literal(attrs, "display") == Some("none")
                    || computed(attrs, "display")
                    || has(attrs, "popover")
                    || tag == "dialog"
                {
                    continue;
                }
                if literal(attrs, "id") == Some(name) {
                    out.push(LowerError {
                        id: "lower-back-twice",
                        message: format!("this is the route's Back control (`id=\"{name}\"`, which the root's `navigationBack` names), drawn in the page: on iOS a pushed route's navigation bar has the system Back button, which presses this one, so the screen shows two. Delete it, and the platform's Back still goes back (the bar's button, the edge swipe, the browser's Back), through the root's `navigate` or the router's own back; or move it into the route's `header` beside one `text role=\"heading\"`, where it is the bar's Back button; or, for a full-bleed screen that draws its own chrome, say `navigationBar=\"none\"` on the route (docs/recipes/navigation-stack.md)"),
                        span: node.span(),
                    });
                    continue;
                }
                stack.extend(children.iter().rev());
            }
            Node::When {
                then, otherwise, ..
            } => stack.extend(then.iter().chain(otherwise).rev()),
            Node::Each { body, .. } => stack.extend(body.iter().rev()),
            Node::Match { some, none, .. } => stack.extend(some.1.iter().chain(none).rev()),
            Node::Use { children, .. } => stack.extend(children.iter().rev()),
            Node::Children { .. } => {}
        }
    }
}

/// What a number may be, as far as the clock goes: a bit set. A value may
/// be several (a state assigned a reading in one action and a duration in
/// another). The empty set is a state or derive not yet resolved, which
/// adds nothing to what reads it (see `performance_now`).
type Kinds = u8;
/// A reading of `performanceNow()`, the milliseconds since this launch, or
/// a number made from one: a launch-relative time.
const TIME: Kinds = 1;
/// One reading less another: a span of time, the same on every launch.
const SPAN: Kinds = 2;
/// A value the source writes, or one of no clock (a literal, a count, a
/// truth, a call of literals).
const CONST: Kinds = 4;
/// A value the compiler cannot see (a source's field, a parameter): taken,
/// as an anchor, to be on the clock.
const UNSEEN: Kinds = 8;
/// The launch's wall-clock origin, `epochAtZero`.
const EPOCH: Kinds = 16;
const ALL: [Kinds; 5] = [TIME, SPAN, CONST, UNSEEN, EPOCH];

/// One kind with another, under `op`: a time with the origin added is the
/// wall-clock time, with a span added a later time; a time less a time or
/// an unseen anchor is a span, less a constant, a span or the origin still
/// a time; otherwise what either is, a constant only when both are.
fn pair(op: BinOp, x: Kinds, y: Kinds) -> Kinds {
    let join = if x == y || y == CONST {
        x
    } else if x == CONST {
        y
    } else {
        x | y
    };
    match op {
        BinOp::Add if x | y == TIME | EPOCH => UNSEEN,
        BinOp::Add if x | y == TIME | SPAN => TIME,
        BinOp::Sub if x == TIME => match y {
            TIME | UNSEEN => SPAN,
            _ => TIME,
        },
        BinOp::Sub if y == TIME => TIME,
        _ => join,
    }
}

/// `op` over every kind `a` may be and every kind `b` may be: monotone, so
/// the names' kinds only grow. Either side unresolved, nothing yet.
fn arith(op: BinOp, a: Kinds, b: Kinds) -> Kinds {
    let mut out = 0;
    for x in ALL.into_iter().filter(|x| a & x != 0) {
        for y in ALL.into_iter().filter(|y| b & y != 0) {
            out |= pair(op, x, y);
        }
    }
    out
}

/// What the expanded root's names may hold, as far as the clock goes.
struct Clock<'a> {
    fns: &'a [FnDecl],
    /// Each state's and derive's kinds, from what is assigned to it; empty
    /// while not yet resolved.
    names: BTreeMap<String, Kinds>,
}

impl Clock<'_> {
    /// Whether `e` may be a launch-relative time.
    fn mark(&self, e: &Expr, locals: &[(String, Kinds)], depth: u32) -> bool {
        self.kinds(e, locals, depth) & TIME != 0
    }

    /// What `e` may be (`pair`). `depth` bounds a `fn`'s expansion.
    fn kinds(&self, e: &Expr, locals: &[(String, Kinds)], depth: u32) -> Kinds {
        let k = |e: &Expr| self.kinds(e, locals, depth);
        match e {
            Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) => CONST,
            Expr::Ident(name, _) => match locals.iter().rev().find(|(n, _)| n == name) {
                Some(&(_, kinds)) => kinds,
                None => self.names.get(name).copied().unwrap_or(UNSEEN),
            },
            Expr::Member(_, field, _) if field == "epochAtZero" => EPOCH,
            Expr::Member(inner, _, _) => UNSEEN | (k(inner) & (TIME | EPOCH)),
            Expr::Call(name, args, _) => {
                if let Some(f) = self.fns.iter().find(|f| &f.name == name) {
                    if depth == 0 {
                        return args.iter().fold(UNSEEN, |acc, a| acc | k(a));
                    }
                    let bound: Vec<(String, Kinds)> = f
                        .params
                        .iter()
                        .zip(args)
                        .map(|(p, a)| (p.name.clone(), k(a)))
                        .collect();
                    return self.kinds(&f.body, &bound, depth - 1);
                }
                match Stdlib::from_name(name) {
                    Some(Stdlib::PerformanceNow) => TIME,
                    // A count, a position or a truth is no time.
                    Some(f) if f.returns() == "bool" => CONST,
                    Some(Stdlib::Length | Stdlib::IndexOf) => CONST,
                    _ => args
                        .iter()
                        .map(k)
                        .reduce(|a, b| arith(BinOp::Mul, a, b))
                        .unwrap_or(UNSEEN),
                }
            }
            Expr::Binary(op, a, b, _) => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                    arith(*op, k(a), k(b))
                }
                _ => CONST,
            },
            Expr::Unary(UnOp::Neg, inner, _) => k(inner),
            Expr::Unary(..) => CONST,
            Expr::Ternary(_, a, b, _) => k(a) | k(b),
            Expr::Match {
                subject,
                var,
                some,
                none,
                ..
            } => {
                let mut inner = locals.to_vec();
                inner.push((var.clone(), k(subject)));
                self.kinds(some, &inner, depth) | k(none)
            }
            Expr::Let {
                name, value, body, ..
            } => {
                let mut inner = locals.to_vec();
                inner.push((name.clone(), k(value)));
                self.kinds(body, &inner, depth)
            }
            Expr::Template(parts, _) => parts.iter().fold(CONST, |acc, p| match p {
                TemplatePart::Expr(e) => acc | (k(e) & TIME),
                TemplatePart::Text(_) => acc,
            }),
            Expr::List(items, _) => items.iter().fold(0, |acc, e| acc | k(e)),
            Expr::Some(inner, _) | Expr::NamedArg(_, inner, _) | Expr::Typed(inner, _, _) => {
                k(inner)
            }
            Expr::Arrow { body, .. } => k(body),
        }
    }
}

/// The states an assignment computes from themselves (`mark = performanceNow() - mark`).
fn self_assigned(body: &[Stmt], out: &mut BTreeSet<String>) {
    for s in body {
        match s {
            Stmt::Assign { target, expr, .. } => {
                let mut read = Vec::new();
                screens::idents(expr, &mut read);
                if read.contains(target) {
                    out.insert(target.clone());
                }
            }
            Stmt::If {
                then, otherwise, ..
            } => {
                self_assigned(then, out);
                self_assigned(otherwise, out);
            }
            Stmt::Match { some, none, .. } => {
                self_assigned(&some.1, out);
                self_assigned(none, out);
            }
            Stmt::Call { body, .. } => self_assigned(body, out),
            Stmt::Let { .. } | Stmt::Command { .. } | Stmt::Send { .. } | Stmt::Refresh { .. } => {}
        }
    }
}

/// Each state a statement list assigns, with what it may assign, and its `let`s.
fn assigned(
    clock: &Clock<'_>,
    body: &[Stmt],
    locals: &mut Vec<(String, Kinds)>,
    out: &mut BTreeMap<String, Kinds>,
) {
    let scope = locals.len();
    for s in body {
        match s {
            Stmt::Let { name, expr, .. } => {
                let kinds = clock.kinds(expr, locals, 4);
                locals.push((name.clone(), kinds));
            }
            Stmt::Assign { target, expr, .. } => {
                *out.entry(target.clone()).or_default() |= clock.kinds(expr, locals, 4);
            }
            Stmt::If {
                then, otherwise, ..
            } => {
                assigned(clock, then, locals, out);
                assigned(clock, otherwise, locals, out);
            }
            Stmt::Match {
                subject,
                some,
                none,
                ..
            } => {
                let kinds = clock.kinds(subject, locals, 4);
                locals.push((some.0.clone(), kinds));
                assigned(clock, &some.1, locals, out);
                locals.pop();
                assigned(clock, none, locals, out);
            }
            Stmt::Call { body, .. } => assigned(clock, body, locals, out),
            Stmt::Command { .. } | Stmt::Send { .. } | Stmt::Refresh { .. } => {}
        }
    }
    locals.truncate(scope);
}

/// Every argument a `send` gives a source that may be a mark.
fn sent(
    clock: &Clock<'_>,
    body: &[Stmt],
    locals: &mut Vec<(String, Kinds)>,
    out: &mut Vec<(Span, String)>,
) {
    let scope = locals.len();
    for s in body {
        match s {
            Stmt::Let { name, expr, .. } => {
                let kinds = clock.kinds(expr, locals, 4);
                locals.push((name.clone(), kinds));
            }
            Stmt::Send { source, args, .. } => {
                for a in args.iter().filter(|a| clock.mark(a, locals, 4)) {
                    out.push((a.span(), source.clone()));
                }
            }
            Stmt::If {
                then, otherwise, ..
            } => {
                sent(clock, then, locals, out);
                sent(clock, otherwise, locals, out);
            }
            Stmt::Match {
                subject,
                some,
                none,
                ..
            } => {
                let kinds = clock.kinds(subject, locals, 4);
                locals.push((some.0.clone(), kinds));
                sent(clock, &some.1, locals, out);
                locals.pop();
                sent(clock, none, locals, out);
            }
            Stmt::Call { body, .. } => sent(clock, body, locals, out),
            Stmt::Assign { .. } | Stmt::Command { .. } | Stmt::Refresh { .. } => {}
        }
    }
    locals.truncate(scope);
}

/// `type-performance-now-persisted`: a value made from `performanceNow()`
/// without `epochAtZero` added, given to a source by a `send` or kept in a
/// persisted state (LLP 1116 D6, D5).
/// `performanceNow()` counts from 0 at each launch, so a start mark a
/// source keeps reads as a time before the next launch began (stopwatch:
/// laps of `0-5:0-41`). A span, one reading less another (or less an
/// anchor the compiler cannot see, euxy's `transport.anchorAt`), is kept as
/// it is.
fn performance_now(root: &contract_syntax::Component, fns: &[FnDecl], out: &mut Vec<LowerError>) {
    // Every state and derive starts unresolved (no kinds), so what reads one
    // waits for it: a derive of the origin read before it resolves gives
    // nothing, not a guess it keeps.
    let names = root.states.iter().chain(&root.derives);
    let mut clock = Clock {
        fns,
        names: names.map(|b| (b.name.clone(), 0)).collect(),
    };
    // What each grows to, round by round, from what is assigned to it, until
    // nothing more is added: every step is monotone (`arith`), so the rounds
    // end, at most five kinds a name. An assigned state's initial number is
    // its placeholder (`state start = 0`), only a reading there counting,
    // unless an assignment computes the state from itself, whose first run
    // reads that number; one never assigned is its initial value.
    let mut targets = BTreeMap::new();
    let mut selfish = BTreeSet::new();
    for a in &root.actions {
        assigned(&clock, &a.body, &mut Vec::new(), &mut targets);
        self_assigned(&a.body, &mut selfish);
    }
    let rounds = (clock.names.len() + 1) * ALL.len();
    for _ in 0..rounds {
        let mut names = clock.names.clone();
        for s in &root.states {
            let placeholder = targets.contains_key(&s.name) && !selfish.contains(&s.name);
            let kept = if placeholder { TIME } else { !0 };
            *names.entry(s.name.clone()).or_default() |= clock.kinds(&s.expr, &[], 4) & kept;
        }
        for d in &root.derives {
            *names.entry(d.name.clone()).or_default() |= clock.kinds(&d.expr, &[], 4);
        }
        for a in &root.actions {
            assigned(&clock, &a.body, &mut Vec::new(), &mut names);
        }
        if names == clock.names {
            break;
        }
        clock.names = names;
    }
    let mut found = Vec::new();
    for a in &root.actions {
        sent(&clock, &a.body, &mut Vec::new(), &mut found);
    }
    // A persisted state outlives the launch as a source's copy does (D5).
    for s in root.states.iter().filter(|s| root.persists(&s.name)) {
        if clock.names.get(&s.name).is_some_and(|k| k & TIME != 0) {
            out.push(LowerError {
                id: "type-performance-now-persisted",
                message: format!(
                    "the persisted state `{}` holds a time computed from `performanceNow()`, the milliseconds since this launch, which start again at 0 on every launch, so the kept value is wrong after a relaunch. Keep the wall-clock time, `time.epochAtZero + performanceNow()` (with `resource time = exactTime() as shape T` and `epochAtZero: number` in `T`), or a duration, `performanceNow() - start` (docs/recipes/timer-that-survives-relaunch.md)",
                    s.name
                ),
                span: s.span,
            });
        }
    }
    // An action called from another is in both bodies: once per argument.
    let mut seen = BTreeSet::new();
    for (span, source) in found {
        if !seen.insert((span.line, span.col, span.end_col)) {
            continue;
        }
        out.push(LowerError {
            id: "type-performance-now-persisted",
            message: format!(
                "this argument to `{source}` is computed from `performanceNow()`, the milliseconds since this launch, which start again at 0 on every launch, so a time the source keeps is wrong after a relaunch. Send the wall-clock time, `time.epochAtZero + performanceNow()` (with `resource time = exactTime() as shape T` and `epochAtZero: number` in `T`), or a duration, `performanceNow() - start` (docs/contract-grammar.md, `performanceNow()`)"
            ),
            span,
        });
    }
}

/// What a colour property paints, for the role its repair names.
#[derive(Clone, Copy)]
enum Paints {
    Text,
    Surface,
    Line,
    Accent,
}

impl Paints {
    /// The rows whose colour is the page's own: text, surfaces, lines and
    /// the accent. `fill`, `stroke` and a symbol's palette are artwork's,
    /// and a shadow darkens in either scheme, as UIKit's does.
    fn of(name: &str) -> Option<Paints> {
        let rows = match crate::style_names::lookup(name)? {
            AttrTarget::Styles(rows) => rows,
            AttrTarget::Shorthand => crate::shorthands::rows(name),
            _ => return None,
        };
        rows.iter().find_map(|row| match row {
            StyleId::TextColor | StyleId::CaretColor | StyleId::TextStrokeColor => {
                Some(Paints::Text)
            }
            StyleId::BackgroundColor => Some(Paints::Surface),
            StyleId::BorderColorTop
            | StyleId::BorderColorRight
            | StyleId::BorderColorBottom
            | StyleId::BorderColorLeft
            | StyleId::ColumnRuleColor => Some(Paints::Line),
            StyleId::AccentColor | StyleId::TintColor => Some(Paints::Accent),
            _ => None,
        })
    }

    fn repair(self) -> &'static str {
        match self {
            Paints::Text => "name a role (`-exact-label`, `-exact-secondary-label`, `AccentColor`) or leave it unsaid",
            Paints::Surface => "name a role (`-exact-secondary-grouped-background`, `-exact-fill`, `AccentColor`) or leave it unsaid",
            Paints::Line => "name a role (`-exact-separator`) or leave it unsaid",
            Paints::Accent => "name a role (`-exact-system-teal`) or write one `light-dark(light, dark)` pair",
        }
    }
}

/// Whether one value of a colour property is a literal colour, the same in
/// both schemes: a hex, `rgb()`, `hsl()`, a colour in its own space or a
/// named colour. `transparent` (any colour of no alpha), `currentColor`,
/// the roles (`-exact-label`, `CanvasText`, `AccentColor`), a
/// `light-dark()` pair and `-exact-platform-color()` are not.
fn literal_colour(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if let Some(open) = lower.find('(') {
        let colour = matches!(
            &lower[..open],
            "rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "lab" | "lch" | "oklab" | "oklch" | "color"
        );
        if !colour {
            return false;
        }
    }
    match exact_color::parse(&lower) {
        Some(exact_color::Parsed::Legacy(c, _)) => c.a > 0.0,
        Some(exact_color::Parsed::Wide(c)) => c.alpha > 0.0,
        _ => false,
    }
}

/// The literal colours in one string a colour property takes: each value
/// at the top level, where white space and commas outside parentheses
/// split them (`1px solid #ddd`), so `light-dark(#fff, #000)` is one.
fn literal_colours(text: &str, out: &mut Vec<String>) {
    let (mut depth, mut start) = (0usize, None);
    let mut take = |s: &str| {
        if literal_colour(s) && !out.iter().any(|c| c == s) {
            out.push(s.to_string());
        }
    };
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if (c.is_whitespace() || c == ',') && depth == 0 => {
                if let Some(s) = start.take() {
                    take(&text[s..i]);
                }
                continue;
            }
            _ => {}
        }
        start.get_or_insert(i);
    }
    if let Some(s) = start {
        take(&text[s..]);
    }
}

/// What the view's names may hold, as far as colour goes.
struct Leaves<'a> {
    fns: &'a [FnDecl],
    derives: &'a [Binding],
    /// The theme's entries, by every spelling a row names them (LLP 1095
    /// D4): a name is the colour it stands for.
    theme: Vec<(String, &'a str)>,
    /// The entries a row names: their colours are their declaration's to
    /// report, once, not each row's.
    named: std::cell::RefCell<Vec<String>>,
}

impl Leaves<'_> {
    /// The entry a theme name is, as declared, and the colour text it
    /// stands for: a name exactly, a role's spelling without case (LLP 1095
    /// D4).
    fn entry_named(&self, word: &str) -> Option<(&str, &str)> {
        self.theme
            .iter()
            .find(|(name, _)| {
                *name == word
                    || exact_kernel::style::roles::role(name).is_some()
                        && name.eq_ignore_ascii_case(word)
            })
            .map(|(name, value)| (name.as_str(), *value))
    }

    /// The literal colours `e` may be: its string leaves, through `?:`,
    /// `match`, a template's text, a `fn`'s body and a derive. A name the
    /// view binds, a state, a field or a call of the stdlib is given the
    /// benefit of the doubt. `depth` bounds the expansion.
    fn colours(&self, e: &Expr, locals: &[String], depth: u32, out: &mut Vec<String>) {
        match e {
            Expr::Str(text, _) => {
                literal_colours(text, out);
                // A token's colours are its declaration's (LLP 1121 D3).
                for word in text.split(|c: char| c.is_whitespace() || c == ',') {
                    if let Some((name, _)) = self.entry_named(word) {
                        let mut named = self.named.borrow_mut();
                        if !named.iter().any(|n| n == name) {
                            named.push(name.to_string());
                        }
                    }
                }
            }
            Expr::Template(parts, _) => {
                for p in parts {
                    if let TemplatePart::Text(text) = p {
                        literal_colours(text, out);
                    }
                }
            }
            Expr::Ternary(_, yes, no, _) => {
                self.colours(yes, locals, depth, out);
                self.colours(no, locals, depth, out);
            }
            Expr::Match {
                var, some, none, ..
            } => {
                let mut inner = locals.to_vec();
                inner.push(var.clone());
                self.colours(some, &inner, depth, out);
                self.colours(none, locals, depth, out);
            }
            Expr::Let { name, body, .. } => {
                let mut inner = locals.to_vec();
                inner.push(name.clone());
                self.colours(body, &inner, depth, out);
            }
            Expr::Typed(inner, _, _) | Expr::Some(inner, _) => {
                self.colours(inner, locals, depth, out)
            }
            Expr::Call(name, _, _) if depth > 0 => {
                if let Some(f) = self.fns.iter().find(|f| &f.name == name) {
                    let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                    self.colours(&f.body, &params, depth - 1, out);
                }
            }
            Expr::Ident(name, _) if depth > 0 && !locals.contains(name) => {
                if let Some(d) = self.derives.iter().find(|d| &d.name == name) {
                    self.colours(&d.expr, &[], depth - 1, out);
                }
            }
            _ => {}
        }
    }
}

/// One attribute that sets a literal colour, or a token one colour in
/// every scheme, at its declaration.
struct Site {
    span: Span,
    name: String,
    paints: Paints,
    colours: Vec<String>,
    token: bool,
}

/// Every attribute of `attrs` that sets a literal colour on the page's own
/// rows, once a name.
fn colour_sites(attrs: &[Attr], leaves: &Leaves<'_>, locals: &[String], out: &mut Vec<Site>) {
    for (i, a) in attrs.iter().enumerate() {
        if attrs[i + 1..].iter().any(|b| b.name == a.name) {
            continue;
        }
        let Some(paints) = Paints::of(&a.name) else {
            continue;
        };
        let mut colours = Vec::new();
        leaves.colours(&a.value, locals, 4, &mut colours);
        if !colours.is_empty() {
            out.push(Site {
                span: a.span,
                name: a.name.clone(),
                paints,
                colours,
                token: false,
            });
        }
    }
}

/// Whether `e` is, or can choose, the string `word`.
fn names(e: &Expr, word: &str) -> bool {
    match e {
        Expr::Str(s, _) => s == word,
        Expr::Ternary(_, a, b, _) => names(a, word) || names(b, word),
        Expr::Let { body, .. } => names(body, word),
        _ => false,
    }
}

/// The sites from `first` on stand on an accent fill: their white text is
/// kept (@ref LLP 1116.020 D7). White on the accent is a filled button's
/// label (Messages' bubble): the accent stays a fill in both schemes, and
/// `check` holds the pair to 3:1.
fn on_accent_fill(sites: &mut Vec<Site>, first: usize) {
    let kept: Vec<Site> = sites
        .drain(first..)
        .filter(|s| s.name != "color" || !s.colours.iter().all(|c| white(c)))
        .collect();
    sites.extend(kept);
}

/// Whether a literal colour is white.
fn white(c: &str) -> bool {
    let c = c.trim().to_ascii_lowercase().replace(' ', "");
    matches!(
        c.as_str(),
        "white" | "#fff" | "#ffff" | "#ffffff" | "#ffffffff" | "rgb(255,255,255)"
    )
}

/// `lower-colour-one-scheme`: a literal colour (a hex, `rgb()`, `hsl()`, a
/// named colour) on a colour property of the page, outside `light-dark()`
/// (LLP 1116.002 D1). It stays the same in dark mode while the platform's
/// text and controls change: round 4's light cards under white text. The
/// one authored colour is the accent, a role or one `light-dark()` pair;
/// the rest are roles or unsaid. It still fires when the app forces a
/// `color-scheme`, which is the app with no dark mode. Artwork (`fill`,
/// `stroke`, anything in an `svg`, a game's picture) is exempt.
///
/// One line a node and property, each short, so every site is named in
/// one run; with more than one, a last line says why and what the accent
/// may be.
fn one_scheme(
    root: &Component,
    styles: &[StyleDecl],
    fns: &[FnDecl],
    theme: Option<&contract_syntax::ThemeDecl>,
    tokens: &Tokens,
    out: &mut Vec<LowerError>,
) {
    // A game (LLP 1046) reads its world through `exactSurface`: its HUD and
    // title screen are its art, over a world no scheme repaints.
    if root.resources.iter().any(|r| r.source == "exactSurface") {
        return;
    }
    let entries = theme.map_or(&[][..], |t| &t.entries[..]);
    let mut spelled = Vec::new();
    for e in entries {
        let Expr::Str(value, _) = &e.value else {
            continue;
        };
        spelled.push((e.name.clone(), value.as_str()));
        spelled.push((format!("-exact-{}", e.name), value.as_str()));
    }
    let leaves = Leaves {
        fns,
        derives: &root.derives,
        theme: spelled,
        named: Default::default(),
    };
    // A token's colour when it is one in light and dark, by any spelling.
    let one = |name: &str| {
        let bare = name.strip_prefix("-exact-").unwrap_or(name);
        tokens
            .one
            .iter()
            .find(|(n, ..)| n == name || n == bare)
            .map(|(_, c, _)| vec![c.clone()])
            .unwrap_or_default()
    };
    let mut sites = Vec::new();
    // An entry that replaces a role the page paints with no row naming it
    // (its text, its ground) is a site of its own.
    for e in entries {
        let role = exact_kernel::style::roles::role(&e.name)
            .or_else(|| exact_kernel::style::roles::role(&format!("-exact-{}", e.name)));
        let paints = match role
            .and_then(exact_kernel::style::roles::role_of)
            .map(|r| r.name)
        {
            Some("CanvasText" | "label" | "secondary-label" | "FieldText") => Paints::Text,
            Some(
                "Canvas"
                | "background"
                | "grouped-background"
                | "secondary-background"
                | "secondary-grouped-background"
                | "Field",
            ) => Paints::Surface,
            Some("separator" | "opaque-separator") => Paints::Line,
            _ => continue,
        };
        let colours = one(&e.name);
        if !colours.is_empty() {
            // A declaration: the token's repair, the mark or a pair.
            sites.push(Site {
                span: e.span,
                name: e.name.clone(),
                paints,
                colours,
                token: true,
            });
        }
    }
    for s in styles {
        let first = sites.len();
        colour_sites(&s.attrs, &leaves, &[], &mut sites);
        if width::last(&s.attrs.iter().collect::<Vec<_>>(), "background-color")
            .is_some_and(|fill| names(&fill.value, "AccentColor"))
        {
            on_accent_fill(&mut sites, first);
        }
    }
    // Each node with whether the nearest fill over it (its own included)
    // can be the accent.
    let mut stack: Vec<(&Node, Vec<String>, bool)> = root
        .view
        .iter()
        .rev()
        .map(|n| (n, Vec::new(), false))
        .collect();
    while let Some((node, locals, on_accent)) = stack.pop() {
        let mut accent = on_accent;
        let (children, locals): (Vec<&Node>, Vec<String>) = match node {
            Node::Element { tag, .. } if tag == "svg" => continue,
            Node::Element {
                attrs, children, ..
            } => {
                // The fill its class gives it counts, as its own does.
                let rows = width::rows(attrs, styles).unwrap_or_else(|| attrs.iter().collect());
                if let Some(fill) = width::last(&rows, "background-color") {
                    accent = names(&fill.value, "AccentColor");
                }
                let first = sites.len();
                colour_sites(attrs, &leaves, &locals, &mut sites);
                if accent {
                    on_accent_fill(&mut sites, first);
                }
                (children.iter().collect(), locals)
            }
            Node::When {
                then, otherwise, ..
            } => (then.iter().chain(otherwise).collect(), locals),
            Node::Each {
                var, index, body, ..
            } => {
                let mut inner = locals;
                inner.push(var.clone());
                inner.extend(index.clone());
                (body.iter().collect(), inner)
            }
            Node::Match { some, none, .. } => {
                // The `none` arm cannot see the `some` arm's name; a name
                // bound in either is the benefit of the doubt in both.
                let mut inner = locals;
                inner.push(some.0.clone());
                (some.1.iter().chain(none).collect(), inner)
            }
            Node::Use { children, .. } => (children.iter().collect(), locals),
            Node::Children { .. } => continue,
        };
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|n| (n, locals.clone(), accent)),
        );
    }
    // A token a row names that is one colour in every scheme: one line, at
    // its declaration, however many rows name it (LLP 1121 D3).
    let named = leaves.named.borrow().clone();
    for e in entries {
        let spellings = [e.name.clone(), format!("-exact-{}", e.name)];
        if !named.iter().any(|n| spellings.contains(n)) {
            continue;
        }
        let colours = one(&e.name);
        if !colours.is_empty() {
            sites.push(Site {
                span: e.span,
                name: e.name.clone(),
                paints: Paints::Text,
                colours,
                token: true,
            });
        }
    }
    // A component used twice is inlined twice: one line for its source.
    let mut seen = BTreeSet::new();
    sites.retain(|s| {
        seen.insert((
            s.span.source_id,
            s.span.line,
            s.span.col,
            s.span.end_col,
            s.name.clone(),
        ))
    });
    let why = "Leave the accent unsaid (the platform's; a brand is an `app.tamber`), or write it as a role or one `light-dark(light, dark)` pair on `accent-color`. A forced `color-scheme` is no repair: the app then has no dark mode (docs/contract-for-agents.md, \"Colours: say a role, not a value\")";
    // `#fff` on `color` stays; `#fff`, `#eee` on `background-color` stay.
    let shown = |s: &Site| {
        let mut shown: Vec<String> = s.colours.iter().take(3).map(|c| format!("`{c}`")).collect();
        if s.colours.len() > 3 {
            shown.push("…".into());
        }
        let stay = if s.colours.len() == 1 {
            "stays"
        } else {
            "stay"
        };
        format!("{} on `{}` {stay}", shown.join(", "), s.name)
    };
    // A token's line names it and both ways out (LLP 1121 D3).
    let mark = match tokens.design {
        true => "mark it `scheme=fixed` in the design if one colour is deliberate (an ink on a fixed fill, a ground under media)",
        false => "an inline `theme` cannot mark one deliberate: move it to `app.tamber` and mark it `scheme=fixed`",
    };
    let token = |s: &Site| {
        // Marked only where it is one colour in every app mode.
        let every = tokens.one.iter().any(|(n, _, all)| *all && (*n == s.name));
        if !every {
            return format!(
                "`{}` is the same colour in light and dark in some app modes ({}): give it a `light-dark(light, dark)` pair in those (`scheme=fixed` is for a token one colour in every mode)",
                s.name,
                s.colours.iter().map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
            );
        }
        format!(
            "`{}` is the same colour in light and dark ({}): give it a `light-dark(light, dark)` pair, or {mark}",
            s.name,
            s.colours.iter().take(3).map(|c| format!("`{c}`")).collect::<Vec<_>>().join(", ")
        )
    };
    if let [s] = sites.as_slice() {
        if s.token {
            out.push(LowerError {
                id: "lower-colour-one-scheme",
                message: token(s),
                span: s.span,
            });
            return;
        }
        out.push(LowerError {
            id: "lower-colour-one-scheme",
            message: format!(
                "{} the same in dark mode while the platform's text and controls change: {}. {why}",
                shown(s),
                s.paints.repair()
            ),
            span: s.span,
        });
        return;
    }
    for s in &sites {
        out.push(LowerError {
            id: "lower-colour-one-scheme",
            message: match s.token {
                true => token(s),
                false => format!("{} the same in dark mode: {}", shown(s), s.paints.repair()),
            },
            span: s.span,
        });
    }
    // Every line a token's: their closer is a token's repair.
    if let Some(first) = sites.first().filter(|_| sites.iter().all(|s| s.token)) {
        out.push(LowerError {
            id: "lower-colour-one-scheme",
            message: format!(
                "these {} tokens are the same colour in light and dark: each line above says how to repair it",
                sites.len()
            ),
            span: first.span,
        });
        return;
    }
    if let Some(first) = sites.first() {
        out.push(LowerError {
            id: "lower-colour-one-scheme",
            message: format!(
                "these {} {}colours stay the same in dark mode while the platform's text and controls change, so a light card keeps its colour under the platform's white text: name a role for each{} or leave it unsaid. {why}",
                sites.len(),
                if sites.iter().any(|s| s.token) { "" } else { "literal " },
                if sites.iter().any(|s| s.token) {
                    " literal (each token's line says its own repair)"
                } else {
                    ""
                }
            ),
            span: first.span,
        });
    }
}
