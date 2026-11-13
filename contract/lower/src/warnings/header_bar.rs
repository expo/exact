//! `lower-header-no-bar` and `lower-header-unplaced`: a route's first
//! `header` read as the iOS navigation bar reads it (LLP 1075.003 §9.10,
//! LLP 1116 D6).

use super::{computed, has, kids, literal, nested_header, title, BOXES};
use crate::LowerError;
use contract_syntax::{Attr, Expr, Node, Span};

/// What the iOS bar makes of one node in a route's header.
enum Part {
    /// An `if`, `for`, `match`, use or slot, or a computed role or type: the benefit of the doubt.
    Doubt,
    /// Not shown in the header on any host: `display="none"`, or a popover
    /// or dialog, which shows over the page when it opens.
    Hidden,
    /// `text role="heading"`: the bar's title. An `aria-level` with no role
    /// is not one (LLP 1115.000 D12.5), there or to the bar.
    Heading,
    /// Another `text`: the subtitle when it is the first after the heading.
    Text,
    /// `input type="search"`: the bar's search field.
    Search,
    /// `role="tablist"`: the bar's segmented control.
    Tablist,
    /// A button that presses, or opens a popover or a dialog: a bar item.
    Pressable,
    /// `role="status"`: a readout, a bar item that nothing presses (LLP
    /// 1075.003 §9.15), what it holds its face.
    Status,
    /// A plain box, walked into.
    Box,
    /// Anything else.
    Other,
}

fn part(node: &Node) -> Part {
    let Node::Element { tag, attrs, .. } = node else {
        return Part::Doubt;
    };
    // A popover or dialog shows over the page when opened, not in the bar.
    if literal(attrs, "display") == Some("none") || has(attrs, "popover") || tag == "dialog" {
        return Part::Hidden;
    }
    if computed(attrs, "role") || computed(attrs, "display") {
        return Part::Doubt;
    }
    let role = literal(attrs, "role");
    if role == Some("tablist") {
        return Part::Tablist;
    }
    // A link navigates, given the benefit as a pressed thing; a disabled button is a dimmed item.
    let disabled = |a: &Attr| a.name == "disabled" && !matches!(a.value, Expr::Bool(false, _));
    // A command that opens what it names (`Invocation::opens`); a computed one is given the benefit.
    let opens = |c: &str| {
        ["show-modal", "show-popover", "toggle-popover"]
            .iter()
            .any(|w| c.eq_ignore_ascii_case(w))
    };
    let item = attrs.iter().any(disabled)
        || (has(attrs, "popovertarget") && literal(attrs, "popovertargetaction") != Some("hide"))
        || (has(attrs, "commandfor")
            && (computed(attrs, "command") || literal(attrs, "command").is_some_and(opens)));
    if has(attrs, "press") || (tag == "button" && item) || tag == "link" {
        return Part::Pressable;
    }
    if role == Some("status") && tag != "header" {
        return Part::Status;
    }
    match tag.as_str() {
        "text" if role == Some("heading") => Part::Heading,
        "text" => Part::Text,
        "input" if computed(attrs, "type") => Part::Doubt,
        "input" if literal(attrs, "type") == Some("search") => Part::Search,
        t if BOXES.contains(&t) => Part::Box,
        _ => Part::Other,
    }
}

/// Whether the subtree holds something the bar places or the compiler
/// cannot see through: then a box around it is walked, not dropped whole.
fn holds_placed(node: &Node) -> bool {
    kids(node).iter().any(|k| match part(k) {
        Part::Doubt
        | Part::Heading
        | Part::Search
        | Part::Tablist
        | Part::Pressable
        | Part::Status => true,
        Part::Box => holds_placed(k),
        Part::Hidden | Part::Text | Part::Other => false,
    })
}

fn holds_heading(node: &Node) -> bool {
    kids(node).iter().any(|k| match part(k) {
        Part::Heading => true,
        Part::Box | Part::Pressable | Part::Status => holds_heading(k),
        _ => false,
    })
}

/// A box with a literal fill: before the heading, the title's avatar.
fn filled(node: &Node) -> bool {
    let Node::Element { attrs, .. } = node else {
        return false;
    };
    has(attrs, "background-color") || has(attrs, "background")
}

/// A passive box of texts and symbol images, one of each at least: a
/// subtitle's line of glyphs (Signal's "🔕 Muted ⏱ 1w").
fn glyph_line(node: &Node) -> bool {
    let (mut symbols, mut text) = (false, false);
    for k in kids(node) {
        match part(k) {
            Part::Text => text = true,
            Part::Other if symbol(k) => symbols = true,
            Part::Hidden => {}
            _ => return false,
        }
    }
    symbols && text && !filled(node)
}

/// An image whose source is a symbol; a computed source may be one: the benefit.
fn symbol(node: &Node) -> bool {
    let Node::Element {
        tag, positional, ..
    } = node
    else {
        return false;
    };
    tag == "image"
        && match positional.first() {
            Some(Expr::Str(s, _)) => s.starts_with("symbol:"),
            Some(_) => true,
            None => false,
        }
}

/// Whether `row` is the heading's row, whose unfilled symbol images the
/// iOS title draws beside its text (`HeaderTitle`,
/// NavigationTitleIOS.swift): a flex row holding the heading, each other
/// child shown there a bar item or such an image.
fn title_row(row: &Node) -> bool {
    let Node::Element {
        tag,
        attrs,
        children,
        ..
    } = row
    else {
        return false;
    };
    let flex = match literal(attrs, "display") {
        Some(display) => display == "flex",
        None => tag == "row",
    };
    if !flex || literal(attrs, "flex-direction").is_some_and(|d| d != "row") {
        return false;
    }
    let mut heading = false;
    for k in children {
        match part(k) {
            Part::Heading => heading = true,
            Part::Hidden
            | Part::Doubt
            | Part::Pressable
            | Part::Status
            | Part::Search
            | Part::Tablist => {}
            Part::Other if symbol(k) && !filled(k) => {}
            _ => return false,
        }
    }
    heading
}

/// What the walk of one header has seen.
#[derive(Default)]
struct Bar<'a> {
    headings: usize,
    /// The last heading counted: the bar's title when it is the one.
    title: Option<&'a Node>,
    /// An `if`, `for`, `match`, use or slot, or a computed role: the
    /// headings counted may not be all.
    doubt: bool,
    /// The heading has been passed, in document order.
    passed: bool,
    subtitle: bool,
    avatar: bool,
    search: bool,
    tablist: bool,
    tap: bool,
    /// A bar item, a readout, a search field or a tablist: what a header
    /// with no heading needs to be the bar.
    placed: bool,
    unplaced: Vec<(Span, String)>,
}

/// A route's `header` that is not the bar, in `lower-header-no-bar`'s words (LLP 1115.000 D5.2).
pub(super) const NOT_THE_BAR: &str = "the header is drawn in the page as written, and the bar has no title: a pushed route's holds the system Back button alone, above the header, and a stack's first route has no bar.";

/// The other repairs `lower-header-no-bar` names: a site's header, and the route's own chrome.
pub(super) const OR_NO_BAR: &str = "; or, for a site's header, say `appearance=\"none\"` on it; or, if this screen draws its own chrome, say `navigationBar=\"none\"` on the route's element (the one with `navigationKey`, not its line under `routes`)";

/// `lower-header-unplaced`: a node in a route's first `header` that the
/// iOS navigation bar does not place (LLP 1075.003 §9.10, LLP 1116 D6). The
/// bar takes one heading as its title (with the symbol images beside it in
/// its row, `title_row`), the first text after it as the
/// subtitle (a filled box before it as the avatar), one search field, one
/// tablist, the buttons that press, open a popover or a dialog or are disabled, and the
/// readouts (`role="status"`, LLP 1075.003 §9.15); the rest of the header is hidden
/// with it. With two headings the bar takes nothing and the header is shown
/// as written: `lower-header-no-bar`'s (LLP 1116.003 D2.2). With none, the
/// bar has no title and takes the header only when it places all of it
/// (`exact_kernel::bars`, issue #364).
/// An `if` or a `match` first among the route's children is read branch by branch: a
/// branch's first node is the route's first child while it is taken, and `after` the
/// route's nodes past that choice.
pub(super) fn route_header<'a>(
    children: &'a [Node],
    after: &[&'a Node],
    out: &mut Vec<LowerError>,
) {
    if let Some(choice @ (Node::When { .. } | Node::Match { .. })) = children.first() {
        let rest: Vec<&Node> = children[1..].iter().chain(after.iter().copied()).collect();
        for branch in choice.child_lists() {
            route_header(branch, &rest, out);
        }
        return;
    }
    let Some(header @ Node::Element { tag, attrs, .. }) = children.first() else {
        return;
    };
    if tag != "header" || nested_header::no_bar(attrs) {
        return;
    }
    let mut bar = Bar::default();
    walk(header, &mut bar);
    // No heading, and nothing the bar has no place for: the bar with no title.
    let untitled = bar.headings == 0 && bar.placed && !bar.avatar && bar.unplaced.is_empty();
    if bar.headings != 1 && !untitled {
        if !bar.doubt {
            out.push(LowerError {
                id: "lower-header-no-bar",
                message: if bar.headings == 0 && bar.placed {
                    let what = match bar.unplaced.first() {
                        Some((_, what)) => what.trim_end_matches(',').trim_end_matches(", before the heading").to_string(),
                        None => "its filled box".into(),
                    };
                    format!("this route's `header` holds no heading, and {what} in it is not a bar item, so on iOS it is not the navigation bar: {NOT_THE_BAR} Give it one `text role=\"heading\"` (`aria-level=1` for a large title) — its buttons become the bar's items; or, for a bar with no title, move what is not a button below the `header`: a header of buttons alone is the bar{OR_NO_BAR} (docs/recipes/navigation-stack.md)")
                } else if bar.headings == 0 {
                    format!("this route's `header` holds no heading, so on iOS it is not the navigation bar: {NOT_THE_BAR} Give it one `text role=\"heading\"` (`aria-level=1` for a large title) — its buttons become the bar's items{OR_NO_BAR} (docs/recipes/navigation-stack.md)")
                } else {
                    format!("this route's `header` holds {} headings, so on iOS it is not the navigation bar: {NOT_THE_BAR} Keep one `text role=\"heading\"` and make the others plain `text` (the first text after the heading is the bar's subtitle), or move them below the `header`{OR_NO_BAR} (docs/recipes/navigation-stack.md)", bar.headings)
                },
                span: header.span(),
            });
        }
        return;
    }
    if let Some(title) = bar.title {
        let content: Vec<&Node> = children[1..].iter().chain(after.iter().copied()).collect();
        title::title_twice(title, &content, out);
    }
    for (span, what) in bar.unplaced {
        out.push(LowerError {
            id: "lower-header-unplaced",
            message: format!(
                "{what} in this route's `header` is not shown on iOS: the navigation bar there takes one heading as its title (with the symbol images beside it in its own row), the first text after it as the subtitle, one `input type=\"search\"`, one `role=\"tablist\"`, the buttons that press or are disabled and the readouts (`role=\"status\"`), and hides the rest of the header. Move it below the `header`, into the route's content (the scroller's first row), make it a button that presses, or, if it is a status that nothing presses (a \"● Live\" beside the title), give it `role=\"status\"` (docs/contract-for-agents.md, \"Tabs and stacks\")"
            ),
            span,
        });
    }
}

/// The children of `parent`, a header or a box in it, into `bar`.
fn walk<'a>(parent: &'a Node, bar: &mut Bar<'a>) {
    let glyphs = title_row(parent);
    for node in kids(parent) {
        let span = node.span();
        match part(node) {
            Part::Doubt => bar.doubt = true,
            Part::Hidden => {}
            Part::Heading => {
                bar.headings += 1;
                bar.title = Some(node);
                bar.passed = true;
            }
            Part::Search if !bar.search => (bar.search, bar.placed) = (true, true),
            Part::Tablist if !bar.tablist => (bar.tablist, bar.placed) = (true, true),
            Part::Search => bar
                .unplaced
                .push((span, "a second `input type=\"search\"`".into())),
            Part::Tablist => bar
                .unplaced
                .push((span, "a second `role=\"tablist\"`".into())),
            // A pressable box around the heading is the title, tapped.
            Part::Pressable if !bar.tap && has_press(node) && holds_heading(node) => {
                bar.tap = true;
                walk(node, bar);
            }
            Part::Pressable => bar.placed = true,
            // A status around the heading is no readout: walked for it.
            Part::Status if holds_heading(node) => walk(node, bar),
            Part::Status => bar.placed = true,
            Part::Text if bar.passed && !bar.subtitle => bar.subtitle = true,
            Part::Text => bar.unplaced.push((
                span,
                if bar.passed {
                    "this text, after the subtitle,".into()
                } else {
                    "this text, before the heading,".into()
                },
            )),
            Part::Box if bar.passed && !bar.subtitle && glyph_line(node) => bar.subtitle = true,
            Part::Box if !bar.passed && !bar.avatar && filled(node) && !holds_heading(node) => {
                bar.avatar = true;
            }
            Part::Box if holds_placed(node) => walk(node, bar),
            // A box with nothing in it lays the bar's items out (a spacer).
            Part::Box if kids(node).is_empty() => {}
            Part::Box => bar.unplaced.push((span, "this box".into())),
            // Drawn in the title, before or after its text.
            Part::Other if glyphs && symbol(node) && !filled(node) => {}
            Part::Other => {
                let what = match node {
                    Node::Element { tag, .. } if tag == "button" => {
                        "a `button` that presses nothing".into()
                    }
                    Node::Element { tag, .. } => format!("this `{tag}`"),
                    _ => "this node".into(),
                };
                bar.unplaced.push((span, what));
            }
        }
    }
}

fn has_press(node: &Node) -> bool {
    matches!(node, Node::Element { attrs, .. } if has(attrs, "press"))
}
