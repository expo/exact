//! `lower-header-no-bar` and `lower-header-unplaced`: a route's first
//! `header` read as the iOS navigation bar reads it (LLP 1075.003 §9.10,
//! LLP 1116 D6).

use super::{computed, has, literal, nested_header, title, BOXES};
use crate::LowerError;
use contract_checking::{Site, Structure, Visit};
use contract_syntax::{Attr, Expr, Span};
use std::rc::Rc;

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
    /// A button that presses, or commands a popover or a dialog: a bar item.
    Pressable,
    /// `role="status"`: a readout, a bar item that nothing presses (LLP
    /// 1075.003 §9.15), what it holds its face.
    Status,
    /// A plain box, walked into.
    Box,
    /// Anything else.
    Other,
}

fn part(node: Visit<'_>) -> Part {
    let Some((tag, _, attrs)) = node.element() else {
        return Part::Doubt;
    };
    // A popover or dialog shows over the page when opened, not in the bar.
    if literal(attrs, "display") == Some("none") || top_layer(tag, attrs).is_some() {
        return Part::Hidden;
    }
    if computed(attrs, "role") || computed(attrs, "display") {
        return Part::Doubt;
    }
    let role = literal(attrs, "role");
    if role == Some("tablist") {
        return Part::Tablist;
    }
    // A heading that presses is still the heading, as the kernel's bar
    // takes it (`exact_kernel::bars`).
    if tag == "text" && role == Some("heading") {
        return Part::Heading;
    }
    // A link navigates, given the benefit as a pressed thing; a disabled button is a dimmed item.
    let disabled = |a: &Attr| a.name == "disabled" && !matches!(a.value, Expr::Bool(false, _));
    let item = attrs.iter().any(disabled) || commands(attrs);
    // A link follows its `href`, unless it has none or an empty one (`fact::HREF`).
    let follows = has(attrs, "href") && literal(attrs, "href") != Some("");
    if has(attrs, "press")
        || (tag == "button" && item)
        || (matches!(tag, "button" | "link") && follows)
    {
        return Part::Pressable;
    }
    if role == Some("status") && tag != "header" {
        return Part::Status;
    }
    match tag {
        "text" => Part::Text,
        "input" if computed(attrs, "type") => Part::Doubt,
        "input" if literal(attrs, "type") == Some("search") => Part::Search,
        t if BOXES.contains(&t) => Part::Box,
        _ => Part::Other,
    }
}

fn top_layer(tag: &str, attrs: &[Attr]) -> Option<&'static str> {
    if tag == "dialog" {
        Some("a dialog")
    } else if has(attrs, "popover") {
        Some("a popover")
    } else {
        None
    }
}

/// Whether the subtree holds something the bar decides itself (a button
/// that presses nothing too) or the compiler cannot see through. For an
/// avatar, look through choices for bar parts in their arms.
fn holds_placed(node: Visit<'_>, walk_choices: bool) -> bool {
    node.regions()[0]
        .walk_regions_with((), |k, _| {
            if walk_choices && k.is_choice() {
                return [Some(()), Some(())];
            }
            [matches!(part(k), Part::Box).then_some(()), None]
        })
        .any(|(k, ())| match part(k) {
            Part::Doubt => !walk_choices,
            Part::Heading | Part::Search | Part::Tablist | Part::Pressable | Part::Status => true,
            Part::Hidden => false,
            _ => matches!(k.element(), Some(("button" | "link", _, _))),
        })
}

/// A box that shows nothing but boxes like it (a spacer, a box of spacers),
/// as the kernel's rule reads one: no item.
fn empty_box(node: Visit<'_>) -> bool {
    node.children().all(|k| match part(k) {
        Part::Hidden => true,
        Part::Box => empty_box(k),
        _ => false,
    })
}

fn holds_heading(node: Visit<'_>) -> bool {
    node.regions()[0]
        .walk_regions_with((), |k, _| {
            if k.is_choice() {
                return [Some(()), Some(())];
            }
            [
                (matches!(part(k), Part::Box | Part::Pressable | Part::Status)
                    || matches!(part(k), Part::Other)
                        && matches!(k.element(), Some(("button" | "link", _, _))))
                .then_some(()),
                None,
            ]
        })
        .any(|(k, ())| matches!(part(k), Part::Heading))
}

fn filled(node: Visit<'_>) -> bool {
    let Some((_, _, attrs)) = node.element() else {
        return false;
    };
    has(attrs, "background-color") || has(attrs, "background")
}

/// `BadgeFace` reads a filled box for shown words, a symbol or a picture.
fn badge_face(node: Visit<'_>, allow_class: bool) -> bool {
    fn content(node: Visit<'_>, visible: bool) -> bool {
        node.children().any(|child| {
            let Some((tag, positional, attrs)) = child.element() else {
                return true;
            };
            if matches!(part(child), Part::Hidden) {
                return false;
            }
            let visible = match literal(attrs, "visibility") {
                Some("visible") => true,
                Some("hidden" | "collapse") => false,
                _ => visible || computed(attrs, "visibility"),
            };
            (visible && matches!(tag, "text" | "image") && has_face_source(positional))
                || content(child, visible)
        })
    }
    let visible = node.element().is_some_and(|(_, _, attrs)| {
        !matches!(literal(attrs, "visibility"), Some("hidden" | "collapse"))
    });
    let classed = allow_class
        && node
            .element()
            .is_some_and(|(_, _, attrs)| has(attrs, "class"));
    (filled(node) || classed) && content(node, visible)
}

fn has_face_source(positional: &[Expr]) -> bool {
    match positional.first() {
        Some(Expr::Str(s, _)) => !s.is_empty(),
        Some(_) => true,
        None => false,
    }
}

/// A passive box of texts and symbol images, one of each at least: a
/// subtitle's line of glyphs (Signal's "🔕 Muted ⏱ 1w"). A readout in it is
/// an item of its own, and the line is what is left (`HeaderTitle.line`).
fn glyph_line(node: Visit<'_>) -> bool {
    let (mut symbols, mut text) = (false, false);
    for k in node.children() {
        match part(k) {
            Part::Text => text = true,
            Part::Other if symbol(k) => symbols = true,
            Part::Hidden | Part::Status => {}
            _ => return false,
        }
    }
    symbols && text && !filled(node)
}

/// An image whose source is a symbol; a computed source may be one: the benefit.
fn symbol(node: Visit<'_>) -> bool {
    let Some((tag, positional, _)) = node.element() else {
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
fn title_row(row: Visit<'_>) -> bool {
    let Some((tag, _, attrs)) = row.element() else {
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
    for k in row.children() {
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

/// One of the bar's places (the heading passed, the subtitle, the search
/// field, the tablist) at a point of the walk, over every way the header's
/// `if`s and `match`es can go: free in all, taken in all, or taken in some.
/// What an element is because of a place (a second search field, a text
/// after the subtitle) is said only when it holds in all: it holds whenever
/// the element shows.
#[derive(Clone, Copy, Default, PartialEq)]
enum Place {
    #[default]
    Free,
    Maybe,
    Taken,
}

impl Place {
    /// After a choice: taken in every arm, free in every arm, or neither.
    fn join(arms: impl IntoIterator<Item = Place>) -> Place {
        let arms: Vec<Place> = arms.into_iter().collect();
        if arms.iter().all(|p| *p == Place::Taken) {
            Place::Taken
        } else if arms.iter().all(|p| *p == Place::Free) {
            Place::Free
        } else {
            Place::Maybe
        }
    }
}

/// What the walk of one header has seen.
#[derive(Default)]
struct Bar<'a> {
    /// The fewest and most headings on any path through the header.
    headings: (usize, usize),
    /// The last heading counted: the bar's title when it is the one.
    title: Option<Visit<'a>>,
    /// An `if`, `for`, `match`, use or slot, or a computed role: the
    /// headings counted may not be all.
    doubt: bool,
    /// The heading has been passed, in document order.
    passed: Place,
    subtitle: Place,
    /// The subtitle when it is a line of glyphs, which segments displace
    /// into an item of its own.
    line: Option<Span>,
    avatar: bool,
    search: Place,
    tablist: Place,
    tap: Place,
    uncertain_title_press: bool,
    /// A bar item, a readout, a search field or a tablist: what a header
    /// with no heading needs to be the bar.
    placed: bool,
    /// What iOS hides: what the heading's own box holds beyond what the
    /// title shows, a second search field or tablist, a button that presses
    /// nothing.
    unplaced: Vec<(Span, String)>,
    /// An element outside the heading's box that the bar shows as written,
    /// an item of its own (LLP 1141.004 D1): with no heading, it keeps the
    /// header in the page.
    hosted: Vec<(Span, String)>,
    /// The readouts (`role="status"`), bar items on iOS.
    readouts: Vec<(Span, String)>,
    /// The buttons and links the bar takes as items, and its readouts, for
    /// what the iOS bar draws of them (`refuse`).
    buttons: Vec<Visit<'a>>,
    statuses: Vec<Visit<'a>>,
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
/// tablist, the buttons that press, command a popover or a dialog or are disabled, and the
/// readouts (`role="status"`, LLP 1075.003 §9.15); the rest of the header is hidden
/// with it. With two headings the bar takes nothing and the header is shown
/// as written: `lower-header-no-bar`'s (LLP 1116.003 D2.2). With none, the
/// bar has no title and takes the header only when it places all of it
/// (`exact_kernel::bars`, issue #364).
/// An `if` or a `match` first among the route's children is read branch by branch: a
/// branch's first node is the route's first child while it is taken. Content after
/// the choice follows each branch's own content.
pub(super) fn route_header(children: Structure<'_>, out: &mut Vec<LowerError>) {
    for (header, content) in route_headers(children) {
        check_header(header, &content, out);
    }
}

/// Each `header` that is a route's first child, wherever an `if` or a
/// `match` puts it, with what follows it, unless it says it is no bar.
fn route_headers(children: Structure<'_>) -> Vec<(Visit<'_>, Rc<[Visit<'_>]>)> {
    fn first<'a>(region: Structure<'a>, after: &[Visit<'a>]) -> (Option<Site>, Rc<[Visit<'a>]>) {
        let head = region.roots().next().map(Visit::site);
        let rest = region
            .roots()
            .skip(1)
            .chain(after.iter().copied())
            .collect::<Vec<_>>();
        (head, rest.into())
    }
    children
        .walk_regions_with(first(children, &[]), |node, (head, after)| {
            if Some(node.site()) != *head || !node.is_choice() {
                return [None, None];
            }
            node.regions().map(|region| Some(first(region, after)))
        })
        .filter(|(header, (head, _))| {
            Some(header.site()) == *head
                && matches!(header.element(), Some(("header", _, attrs)) if !nested_header::no_bar(attrs))
        })
        .map(|(header, (_, content))| (header, content))
        .collect()
}

/// `lower-header-item` (LLP 1141.004 D1), an error in an app that ships iOS:
/// an element in a route's `header` that the iOS navigation bar has no place
/// for, where that header is the bar. The bar holds a closed set, each kind
/// one the compiler can see: a button or link of one symbol, one text, or
/// both, whose children write only where they sit; the one-part badge; a
/// readout (`role="status"`) of words, symbols, dots and one spinner; one
/// search field; one tablist; and the title's own parts. What the compiler
/// cannot see (an `if` it cannot settle, a computed source) is given the
/// benefit, and the host keeps such an element out of the bar.
/// `refusals`, in an app that ships iOS. Lowering has read its targets by
/// the time this runs. A terminal entry is not checked: that host has no
/// navigation bar, whatever else the app ships.
pub(crate) fn refused(
    checked: &contract_types::Checked<'_>,
    profile: crate::Profile,
) -> Vec<LowerError> {
    if matches!(profile, crate::Profile::Terminal)
        || !crate::icons::shipped().iter().any(|t| t.name == "ios")
    {
        return Vec::new();
    }
    refusals(
        &contract_checking::Checking::new(checked).expanded(),
        &checked.file.styles,
    )
}

fn refusals(
    view: &contract_checking::View<'_>,
    styles: &[contract_syntax::StyleDecl],
) -> Vec<LowerError> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    // Inside a navigation root, with the `id` its `navigationBack` names.
    let visits = view.walk_with(None, |node, root: &Option<Option<&str>>| {
        root.or_else(|| {
            let (_, _, attrs) = node.element()?;
            has(attrs, "navigationKey").then(|| literal(attrs, "navigationBack"))
        })
    });
    for (visit, root) in visits {
        let (Some((_, _, attrs)), Some(back)) = (visit.element(), root) else {
            continue;
        };
        let route = has(attrs, "navigationKey")
            && !has(attrs, "navigationBack")
            && literal(attrs, "navigationBar") != Some("none");
        if !route {
            continue;
        }
        for (header, _) in route_headers(visit.regions()[0]) {
            let span = header.span();
            if !class_says_no_bar(header, styles)
                && seen.insert((span.source_id, span.line, span.col))
            {
                refuse(header, back, &mut out);
            }
        }
    }
    out
}

/// Whether the header's `class` hides it or has it draw itself
/// (`nested_header::no_bar`, which reads what the header writes). A class
/// that is not one style or a choice of two has the benefit.
fn class_says_no_bar(header: Visit<'_>, styles: &[contract_syntax::StyleDecl]) -> bool {
    let Some((_, _, attrs)) = header.element() else {
        return false;
    };
    let Some(class) = attrs.iter().find(|a| a.name == "class") else {
        return false;
    };
    let names: Vec<&str> = match &class.value {
        Expr::Ident(name, _) => vec![name],
        Expr::Ternary(_, yes, no, _) => match (&**yes, &**no) {
            (Expr::Ident(a, _), Expr::Ident(b, _)) => vec![a, b],
            _ => return true,
        },
        _ => return true,
    };
    styles
        .iter()
        .filter(|style| names.contains(&style.name.as_str()))
        .any(|style| {
            ["appearance", "display"]
                .iter()
                .any(|row| !has(attrs, row) && literal(&style.attrs, row) == Some("none"))
        })
}

/// Whether `node` or anything in it has a `class`. The refusal reads what an
/// element writes, not its class's rows, so a face styled by one has the
/// benefit.
fn classed(node: Visit<'_>) -> bool {
    let own = |k: Visit<'_>| k.element().is_some_and(|(_, _, a)| has(a, "class"));
    own(node) || node.regions().into_iter().any(|r| r.walk().any(own))
}

/// The parts of `header`, when it is the bar, that the bar has no place for.
/// `back` is the `id` of the root's `navigationBack` control: on a pushed
/// route UIKit's back button stands for it whatever its face (`exact_kernel::bars`).
fn refuse(header: Visit<'_>, back: Option<&str>, out: &mut Vec<LowerError>) {
    let mut bar = Bar::default();
    walk(header, &mut bar);
    if !bar.is_the_bar() {
        return;
    }
    let mut found: Vec<(Span, String)> = bar.hosted.clone();
    for button in &bar.buttons {
        let id = button.element().and_then(|(_, _, a)| literal(a, "id"));
        if back.is_some() && id == back {
            continue;
        }
        // A bar item is a `button` or a `link`, or a box that presses; a
        // field, a picture or a control that presses is none.
        let tag = button.element().map_or("", |e| e.0);
        if !matches!(tag, "button" | "link") && !BOXES.contains(&tag) {
            found.push((button.span(), format!("this `{tag}`, which presses,")));
        } else if let Some(why) = face(*button) {
            found.push((button.span(), format!("this button, whose face {why},")));
        }
    }
    for readout in &bar.statuses {
        if let Some((span, why)) = readout_part(*readout) {
            found.push((span, format!("{why} in a readout")));
        }
    }
    if let (Place::Taken, Some(span)) = (bar.tablist, bar.line) {
        found.push((
            span,
            "this line of glyphs, beside a tablist's segments,".into(),
        ));
    }
    found.sort_by_key(|(span, _)| (span.line, span.col));
    for (span, what) in found {
        let what = what
            .trim_end_matches(',')
            .trim_end_matches(", before the heading")
            .trim_end_matches(", after the subtitle");
        out.push(LowerError {
            id: "lower-header-item",
            message: format!("{what} has no place in the iOS navigation bar, which this route's `header` is. The bar holds a button or link of one symbol, one text, or a symbol and a text (its `color` and font written on the button), a badge (a button whose one child is a filled box holding one text, symbol or picture), a readout (`role=\"status\"`) of words, symbols, dots and one `progress`, one search field and one tablist, besides the title's heading, subtitle and avatar. Make it one of those; or move it below the `header`, into the route's content; or, to draw the header yourself, say `navigationBar=\"none\"` on the route's element (LLP 1141.004 D1; docs/contract-for-agents.md, \"Tabs and stacks\")"),
            span,
        });
    }
}

/// Why a header button's face is not one the bar draws: its own children are
/// one symbol, one text, or both, writing only where they sit; or the
/// one-part badge. None for a face the bar draws, or one the compiler cannot
/// see.
fn face(button: Visit<'_>) -> Option<&'static str> {
    let kids: Vec<Visit<'_>> = button
        .children()
        .filter(|k| !matches!(part(*k), Part::Hidden))
        .collect();
    if kids
        .iter()
        .any(|k| matches!(part(*k), Part::Doubt) || classed(*k))
    {
        return None;
    }
    if let [only] = kids[..] {
        if BOXES.contains(&only.element().map_or("", |e| e.0)) && filled(only) {
            return badge(only);
        }
    }
    let (mut symbols, mut texts) = (0, 0);
    for k in &kids {
        let (tag, _, attrs) = k.element()?;
        match tag {
            "image" if symbol(*k) => {
                symbols += 1;
                if !attrs
                    .iter()
                    .all(|a| sits(&a.name) || SYMBOL_ROWS.contains(&a.name.as_str()))
                {
                    return Some("writes on its symbol more than where it sits and its size");
                }
            }
            "text" => {
                texts += 1;
                if k.children().next().is_some()
                    || has(attrs, "markup")
                    || has(attrs, "href")
                    || has(attrs, "press")
                {
                    return Some("holds a text with runs, Markdown, a link or a press of its own");
                }
                if !attrs.iter().all(|a| sits(&a.name)) {
                    return Some("writes on its text more than where it sits (write `color` and font on the button)");
                }
            }
            "image" => return Some("holds a picture, which a button shows only as a badge's"),
            _ => return Some("holds more than one symbol and one text"),
        }
    }
    (symbols > 1 || texts > 1).then_some("holds more than one symbol and one text")
}

/// Why a badge (a filled box, a button's one child) is not one the bar draws:
/// it holds one text, symbol or picture, and writes its fill, corners, size
/// and layout. It draws text in its size, weight and colour, a symbol in
/// its colour, and a picture in its fit.
fn badge(boxed: Visit<'_>) -> Option<&'static str> {
    let (_, _, attrs) = boxed.element()?;
    let paint = |a: &&Attr| sits(&a.name) || BADGE_ROWS.contains(&a.name.as_str());
    if !attrs.iter().all(|a| paint(&a)) {
        return Some("is a badge whose box writes more than its fill, corners, size and layout");
    }
    let parts: Vec<Visit<'_>> = boxed
        .children()
        .filter(|k| !matches!(part(*k), Part::Hidden))
        .collect();
    match parts[..] {
        [only] if matches!(only.element(), Some(("text" | "image", _, _))) => {
            let (tag, positional, attrs) = only.element()?;
            let rows: &[&str] = match (tag, positional.first()) {
                ("text", _) => &["color", "font-size", "font-weight"],
                (_, Some(Expr::Str(_, _))) if symbol(only) => &["color"],
                (_, Some(Expr::Str(_, _))) => &["object-fit"],
                _ => &["color", "object-fit"],
            };
            let ok = attrs.iter().all(|a| {
                sits(&a.name)
                    || matches!(a.name.as_str(), "aria-label" | "alt")
                    || rows.contains(&a.name.as_str())
            });
            let plain = only.children().next().is_none()
                && !has(attrs, "markup")
                && !has(attrs, "href")
                && !has(attrs, "press");
            (!ok || !plain).then_some("is a badge whose part writes more than the text's size and weight, the text or symbol's colour, or the picture's fit")
        }
        [only] if matches!(part(only), Part::Doubt) => None,
        _ => Some("is a badge holding more than one text, symbol or picture"),
    }
}

/// Whether the attribute `name` says only where an element sits, which the
/// bar's item replaces: its size, flex, margin and padding, and names.
fn sits(name: &str) -> bool {
    matches!(
        name,
        "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "flex"
            | "flex-grow"
            | "flex-shrink"
            | "flex-basis"
            | "align-self"
            | "aria-hidden"
            | "testId"
            | "id"
            | "key"
    ) || name.starts_with("margin")
        || name.starts_with("padding")
}

/// What a symbol in a button may write besides: its size, which the bar's
/// image size replaces.
const SYMBOL_ROWS: &[&str] = &["font-size", "font-weight"];
/// What a badge's box may write besides where it sits.
const BADGE_ROWS: &[&str] = &[
    "background-color",
    "background",
    "border-radius",
    "display",
    "flex-direction",
    "align-items",
    "justify-content",
    "overflow",
    "aspect-ratio",
];
/// The first part of a readout that the bar does not draw, and why: it holds
/// words, symbols, dots and boxes, and one `progress`; nothing that presses,
/// no field, no picture.
fn readout_part(readout: Visit<'_>) -> Option<(Span, String)> {
    fn walk(node: Visit<'_>, spinners: &mut usize) -> Option<(Span, String)> {
        for k in node.children() {
            let what = match (part(k), k.element()) {
                (Part::Hidden | Part::Doubt, _) => continue,
                (Part::Text, _) => continue,
                (Part::Box, _) => match walk(k, spinners) {
                    Some(found) => return Some(found),
                    None => continue,
                },
                (Part::Other, Some(("image", _, _))) if symbol(k) => continue,
                (Part::Other, Some(("progress", _, _))) => {
                    *spinners += 1;
                    if *spinners == 1 {
                        continue;
                    }
                    "a second `progress`".to_string()
                }
                (Part::Pressable, _) => "a button".to_string(),
                (_, Some((tag, _, _))) => format!("this `{tag}`"),
                _ => "this node".to_string(),
            };
            return Some((k.span(), what));
        }
        None
    }
    walk(readout, &mut 0)
}

impl Bar<'_> {
    /// No heading, and nothing but buttons, readouts, a search field and a
    /// tablist: the bar with no title.
    fn untitled(&self) -> bool {
        self.headings == (0, 0) && self.placed && self.unplaced.is_empty() && self.hosted.is_empty()
    }

    /// Whether the header is the bar: one heading, or none and only what the
    /// bar places.
    fn is_the_bar(&self) -> bool {
        !self.uncertain_title_press && (self.headings == (1, 1) || self.untitled())
    }
}

fn check_header(header: Visit<'_>, content: &[Visit<'_>], out: &mut Vec<LowerError>) {
    let mut bar = Bar::default();
    walk(header, &mut bar);
    if !bar.is_the_bar() {
        if !bar.doubt || bar.headings.0 >= 2 {
            out.push(LowerError {
                id: "lower-header-no-bar",
                message: if let ((0, 0), true, Some((_, what))) =
                    (bar.headings, bar.placed, bar.unplaced.first().or(bar.hosted.first()))
                {
                    let what = what.trim_end_matches(',').trim_end_matches(", before the heading");
                    format!("this route's `header` holds no heading, and {what} in it is not a bar item, so on iOS it is not the navigation bar: {NOT_THE_BAR} Give it one `text role=\"heading\"` (`aria-level=1` for a large title) — its buttons become the bar's items; or, for a bar with no title, move what is not a button below the `header`: a header of buttons alone is the bar{OR_NO_BAR} (docs/recipes/navigation-stack.md)")
                } else if bar.headings == (0, 0) {
                    format!("this route's `header` holds no heading, so on iOS it is not the navigation bar: {NOT_THE_BAR} Give it one `text role=\"heading\"` (`aria-level=1` for a large title) — its buttons become the bar's items{OR_NO_BAR} (docs/recipes/navigation-stack.md)")
                } else {
                    let (min, max) = bar.headings;
                    let at_least = if min == max { "" } else { "at least " };
                    format!("this route's `header` holds {at_least}{min} headings, so on iOS it is not the navigation bar: {NOT_THE_BAR} Keep one `text role=\"heading\"` and make the others plain `text` (the first text after the heading is the bar's subtitle), or move them below the `header`{OR_NO_BAR} (docs/recipes/navigation-stack.md)")
                },
                span: header.span(),
            });
        }
        return;
    }
    if let ((1, 1), Some(title)) = (bar.headings, bar.title) {
        title::title_twice(title, content, out);
    }
    // Beside a tablist's segments the title has no room for a line of
    // glyphs: it is an item of its own (`HeaderShape.presented`).
    if let (Place::Taken, Some(span)) = (bar.tablist, bar.line) {
        let what = "this line of glyphs (segments take the title)".to_string();
        bar.hosted.push((span, what));
    }
    left_out(&bar, header, out);
    for (span, what) in bar.unplaced {
        out.push(LowerError {
            id: "lower-header-unplaced",
            message: format!(
                "{what} in this route's `header` is not shown on iOS: the navigation bar there shows each element outside the box around the heading as an item, drawn as written, but of that box only the title (the heading, the symbol images beside it in its own row, the text after it, an avatar before it); it has one place for a search field and one for a tablist, and a button that presses nothing is no item. Move it out of the heading's box, so it is an item of its own, or below the `header`, into the route's content; make it a button that presses, or, if it is a status that nothing presses (a \"● Live\" beside the title), give it `role=\"status\"`; or, for a header the bar cannot hold, say `navigationBar=\"none\"` on the route's element and draw the row yourself (docs/recipes/chat-header.md; docs/contract-for-agents.md, \"Tabs and stacks\")"
            ),
            span,
        });
    }
}

/// The children of `parent`, a header or a box in it, into `bar`. A box that
/// holds the heading is the title's: nothing in it is an item of its own.
fn walk<'a>(parent: Visit<'a>, bar: &mut Bar<'a>) {
    walk_region(parent.regions()[0], (title_row(parent), false, false), bar);
}

/// Whether its author keeps `node` in the page (`appearance="none"`), with
/// what it holds.
fn opted_out(node: Visit<'_>) -> bool {
    node.element()
        .is_some_and(|(_, _, a)| literal(a, "appearance") == Some("none"))
}

/// `region` into `bar`, from `start` (a glyph row, inside the heading's box,
/// kept in the page). An `if` or a `match` is read at its place, arm by arm,
/// each alone from where the walk stands, as what it shows when taken.
fn walk_region<'a>(region: Structure<'a>, start: (bool, bool, bool), bar: &mut Bar<'a>) {
    for _ in region.walk_regions_with(start, |node, &(glyphs, title, kept)| {
        if node.is_choice() {
            bar.doubt = true;
            bar.arms(node, (glyphs, title, kept));
            return [None, None];
        }
        let kept = kept || opted_out(node);
        let descend = bar.visit(node, glyphs, title, kept);
        [
            descend.then(|| (title_row(node), title || holds_heading(node), kept)),
            None,
        ]
    }) {}
}

impl<'a> Bar<'a> {
    /// A choice's arms, each read alone from the places as they stand
    /// before it: what each hides or hosts is named, and nothing in one
    /// counts against another. After it, each place is `Place::join` of the
    /// arms'.
    fn arms(&mut self, choice: Visit<'a>, state: (bool, bool, bool)) {
        let [first, second] = choice.regions().map(|arm| {
            let mut one = Bar {
                passed: self.passed,
                subtitle: self.subtitle,
                search: self.search,
                tablist: self.tablist,
                tap: self.tap,
                ..Bar::default()
            };
            walk_region(arm, state, &mut one);
            self.hosted.append(&mut one.hosted);
            self.readouts.append(&mut one.readouts);
            self.buttons.append(&mut one.buttons);
            self.statuses.append(&mut one.statuses);
            self.unplaced.append(&mut one.unplaced);
            one
        });
        self.headings.0 += first.headings.0.min(second.headings.0);
        self.headings.1 += first.headings.1.max(second.headings.1);
        self.placed |= first.placed || second.placed;
        self.passed = Place::join([first.passed, second.passed]);
        self.subtitle = Place::join([first.subtitle, second.subtitle]);
        self.search = Place::join([first.search, second.search]);
        self.tablist = Place::join([first.tablist, second.tablist]);
        self.tap = Place::join([first.tap, second.tap]);
        self.uncertain_title_press |= first.uncertain_title_press || second.uncertain_title_press;
    }
}

impl<'a> Bar<'a> {
    /// `title`: `node` is inside the heading's own box; `kept`: its author
    /// keeps it in the page (`appearance="none"`, on it or around it), so it
    /// is no item and takes no place of the bar's.
    fn visit(&mut self, node: Visit<'a>, glyphs: bool, title: bool, kept: bool) -> bool {
        let bar = self;
        let span = node.span();
        // Outside the heading's box the bar shows it as an item; inside, the
        // title shows only its own parts.
        let mut hide = |what: String| match title {
            true => bar.unplaced.push((span, what)),
            false => bar.hosted.push((span, what)),
        };
        let part = part(node);
        // What its author keeps takes no place of the bar's; only the
        // heading in it is looked for.
        if kept && !matches!(part, Part::Heading | Part::Doubt | Part::Hidden) {
            return holds_heading(node);
        }
        match part {
            Part::Doubt => bar.doubt = true,
            Part::Hidden => {}
            Part::Heading => {
                bar.headings.0 += 1;
                bar.headings.1 += 1;
                bar.title = Some(node);
                bar.passed = Place::Taken;
            }
            Part::Search if bar.search == Place::Taken => bar
                .unplaced
                .push((span, "a second `input type=\"search\"`".into())),
            Part::Tablist if bar.tablist == Place::Taken => bar
                .unplaced
                .push((span, "a second `role=\"tablist\"`".into())),
            Part::Search => (bar.search, bar.placed) = (Place::Taken, true),
            Part::Tablist => (bar.tablist, bar.placed) = (Place::Taken, true),
            // A pressable box around the heading is the title, tapped.
            Part::Pressable
                if bar.tap != Place::Taken && has_press(node) && holds_heading(node) =>
            {
                if bar.tap == Place::Maybe {
                    bar.uncertain_title_press = true;
                    return false;
                }
                bar.tap = Place::Taken;
                return true;
            }
            // One that neither presses nor commands around the heading (a link,
            // a disabled button) is walked for it, as the kernel walks it.
            Part::Pressable if !has_press(node) && !commands_named(node) && holds_heading(node) => {
                return true
            }
            Part::Pressable => {
                bar.placed = true;
                bar.buttons.push(node);
            }
            // A status around the heading is no readout: walked for it.
            Part::Status if holds_heading(node) => return true,
            Part::Status => {
                bar.placed = true;
                let what = "this readout (`role=\"status\"`)".to_string();
                bar.readouts.push((span, what));
                bar.statuses.push(node);
            }
            // The first text after the heading is the subtitle; one whose
            // place depends on how a choice goes is given the benefit.
            Part::Text => match (bar.passed, bar.subtitle) {
                (Place::Taken, Place::Taken) => hide("this text, after the subtitle,".into()),
                (Place::Free, _) => hide("this text, before the heading,".into()),
                (Place::Taken, _) => bar.subtitle = Place::Taken,
                (Place::Maybe, _) => bar.subtitle = Place::join([bar.subtitle, Place::Taken]),
            },
            Part::Box
                if bar.passed == Place::Taken
                    && bar.subtitle != Place::Taken
                    && glyph_line(node) =>
            {
                if bar.subtitle == Place::Free && !title {
                    bar.line = Some(span);
                }
                bar.subtitle = Place::Taken;
                // A readout in the line is an item of its own.
                for k in node
                    .children()
                    .filter(|k| matches!(self::part(*k), Part::Status))
                {
                    bar.readouts
                        .push((k.span(), "this readout (`role=\"status\"`)".into()));
                    bar.statuses.push(k);
                }
            }
            Part::Box
                if title
                    && bar.passed != Place::Taken
                    && !bar.avatar
                    && badge_face(node, false)
                    && !holds_placed(node, true) =>
            {
                bar.avatar = true;
            }
            Part::Box if holds_placed(node, false) => return true,
            // A box with nothing in it lays the bar's items out (a spacer).
            Part::Box if empty_box(node) => {}
            // A plain box after the heading, the subtitle free, is read for
            // its words: its first text is the subtitle (`HeaderTitle`).
            Part::Box
                if bar.passed == Place::Taken && bar.subtitle != Place::Taken && !filled(node) =>
            {
                return true
            }
            Part::Box => hide("this box".into()),
            // Drawn in the title, before or after its text.
            Part::Other if glyphs && symbol(node) && !filled(node) => {}
            // An idle button or link around the heading is walked for it.
            Part::Other
                if matches!(node.element(), Some(("button" | "link", _, _)))
                    && holds_heading(node) =>
            {
                return true;
            }
            Part::Other => match node.element() {
                Some(("button", _, _)) => bar
                    .unplaced
                    .push((span, "a `button` that presses nothing".into())),
                Some(("link", _, _)) => bar
                    .unplaced
                    .push((span, "a `link` with no `href` that presses nothing".into())),
                Some((tag, _, _)) => hide(format!("this `{tag}`")),
                _ => hide("this node".into()),
            },
        }
        false
    }
}

/// Whether the node makes a bar item by its command (`commands`).
fn commands_named(node: Visit<'_>) -> bool {
    node.element().is_some_and(|(_, _, attrs)| commands(attrs))
}

/// Whether these attributes make a bar item, as the kernel reads an
/// invocation (`fact::COMMANDS`): a command that opens a popover or a dialog,
/// or closes a dialog (a dialog's Cancel is `command="close"` alone), with a
/// `commandfor` governing a `popovertarget` beside it, and a popover action
/// `hide` an item in no case. A computed command is given the benefit.
fn commands(attrs: &[Attr]) -> bool {
    // An empty `commandfor` names nothing, and the `popovertarget` decides.
    if has(attrs, "commandfor") && literal(attrs, "commandfor") != Some("") {
        return computed(attrs, "command")
            || literal(attrs, "command").is_some_and(|c| {
                ["show-modal", "close", "show-popover", "toggle-popover"]
                    .iter()
                    .any(|w| c.eq_ignore_ascii_case(w))
            });
    }
    // An empty `popovertarget` names nothing either.
    has(attrs, "popovertarget")
        && literal(attrs, "popovertarget") != Some("")
        && !literal(attrs, "popovertargetaction").is_some_and(|a| a.eq_ignore_ascii_case("hide"))
}

fn has_press(node: Visit<'_>) -> bool {
    node.element()
        .is_some_and(|(_, _, attrs)| has(attrs, "press"))
}

/// Whether the Mac's toolbar draws a readout (`MacBarFace`): a text, a
/// symbol, or a box of words, symbols and dots; anything else (a spinner, a
/// picture, a control) it has no face for. A part the compiler cannot see is
/// given the benefit.
fn mac_draws(readout: Visit<'_>) -> bool {
    match readout.element() {
        Some(("text", _, _)) => true,
        Some(("image", _, _)) => symbol(readout),
        Some((tag, _, _)) if BOXES.contains(&tag) => readout.children().all(|k| match part(k) {
            Part::Text | Part::Hidden | Part::Doubt => true,
            Part::Box => mac_draws(k),
            Part::Other => symbol(k),
            _ => false,
        }),
        Some(_) => false,
        None => true,
    }
}

/// `HeaderMac.showable` accepts a badge, a painted face or a toolbar item
/// of text and at most one image.
fn mac_button(button: Visit<'_>) -> bool {
    // `HeaderMac`'s `textOnly` reads the node kind, and every prose tag
    // that holds text lowers to a text node.
    fn text_only(node: Visit<'_>) -> bool {
        matches!(part(node), Part::Doubt)
            || node
                .element()
                .is_some_and(|(tag, _, _)| crate::prose::text_container(tag))
                && node.children().all(text_only)
    }

    let mut images = 0;
    let ordinary = button.children().all(|child| {
        if matches!(child.element(), Some(("image", _, _))) && child.children().next().is_none() {
            images += 1;
            true
        } else {
            text_only(child)
        }
    });
    if ordinary && images <= 1 {
        return true;
    }
    let mut shown = button
        .children()
        .filter(|k| !matches!(part(*k), Part::Hidden));
    (shown.next().is_some_and(|child| badge_face(child, true)) && shown.next().is_none())
        || mac_painted(button, false) == Some(true)
}

/// `MacBarFace` needs a sized dot or nonempty text with an authored font.
fn mac_painted(node: Visit<'_>, inherited_font: bool) -> Option<bool> {
    fn writes_font(attrs: &[Attr]) -> bool {
        ["font", "font-family", "font-size", "font-weight", "class"]
            .iter()
            .any(|name| has(attrs, name))
    }
    let font = inherited_font || node.element().is_some_and(|(_, _, a)| writes_font(a));
    node.children().try_fold(false, |painted, child| {
        let qualifies = match part(child) {
            Part::Hidden => false,
            Part::Doubt => true,
            Part::Text => child.element().is_some_and(|(_, positional, attrs)| {
                (font || writes_font(attrs))
                    && (has_face_source(positional) || child.children().next().is_some())
            }),
            Part::Box if mac_dot(child) => true,
            Part::Box => mac_painted(child, font)?,
            Part::Other if symbol(child) => false,
            _ => return None,
        };
        Some(painted || qualifies)
    })
}

fn mac_dot(node: Visit<'_>) -> bool {
    let Some((_, _, attrs)) = node.element() else {
        return false;
    };
    let positive = |name| {
        attrs
            .iter()
            .rev()
            .find(|a| a.name == name)
            .is_some_and(|a| match &a.value {
                Expr::Number(n, _) => *n > 0.,
                // The kernel resolves relative lengths with the layout's font and viewport.
                Expr::Str(s, _) => s
                    .strip_suffix("px")
                    .unwrap_or(s)
                    .parse::<f64>()
                    .map_or(true, |n| n > 0.),
                _ => true,
            })
    };
    (has(attrs, "class") || filled(node) && positive("width") && positive("height"))
        && node
            .children()
            .all(|child| matches!(part(child), Part::Hidden))
}

/// Whether the Mac's toolbar may show a tablist as its segments
/// (`HeaderMac.titleTabs`): two or more tabs, each a `button role="tab"` that
/// presses and shows a title alone. Tabs or titles the compiler cannot see
/// (an `if`, a `for`, a use, a computed text) are given the benefit.
fn mac_tabs(list: Visit<'_>) -> bool {
    let mut tabs = 0;
    for k in list.children() {
        match part(k) {
            Part::Hidden => continue,
            Part::Doubt => return true,
            _ => {}
        }
        let Some(("button", _, attrs)) = k.element() else {
            return false;
        };
        if literal(attrs, "role") != Some("tab") || !has(attrs, "press") || !mac_title(k) {
            return false;
        }
        tabs += 1;
    }
    tabs >= 2
}

/// Whether a tab shows a title alone: texts and nothing else, one of them
/// words (an `aria-label` is no title).
fn mac_title(tab: Visit<'_>) -> bool {
    let mut words = false;
    for t in tab.children() {
        match (part(t), t.element()) {
            (Part::Hidden, _) => {}
            (Part::Doubt, _) => words = true,
            (_, Some(("text", positional, _))) => {
                words |= !matches!(positional.first(), Some(Expr::Str(s, _)) if s.is_empty());
            }
            _ => return false,
        }
    }
    words
}

/// The first part of `header` the Mac's toolbar has no place for, and why, as
/// `HeaderMac.read` decides (LLP 1115.002 D2): it walks boxes, takes the
/// heading, the first text after it as the window's subtitle, the buttons
/// whose face an item shows, the readouts it draws, one search field of its
/// own and one tablist of titles; anything else keeps the whole header in
/// the page. Each arm of a choice is read alone from the places as they
/// stand before it, and what a part is because of a place (a second search
/// field, a text after the subtitle) is said only when it holds however the
/// choices go (`Place`).
fn mac_refusal(header: Visit<'_>) -> Option<(Span, &'static str)> {
    // The heading passed, the subtitle, the search field, the tablist.
    const PASSED: usize = 0;
    const SUBTITLE: usize = 1;
    const SEARCH: usize = 2;
    const TABLIST: usize = 3;
    fn walk<'a>(
        nodes: impl Iterator<Item = Visit<'a>>,
        at: &mut [Place; 4],
    ) -> Option<(Span, &'static str)> {
        for k in nodes {
            let what = match part(k) {
                _ if k.is_choice() => {
                    let mut ends = Vec::new();
                    for arm in k.regions() {
                        let mut one = *at;
                        if let Some(found) = walk(arm.roots(), &mut one) {
                            return Some(found);
                        }
                        ends.push(one);
                    }
                    for (i, place) in at.iter_mut().enumerate() {
                        *place = Place::join(ends.iter().map(|e| e[i]));
                    }
                    continue;
                }
                Part::Hidden => {
                    let layer = k.element()
                        .filter(|(_, _, attrs)| literal(attrs, "display") != Some("none"))
                        .and_then(|(tag, _, attrs)| top_layer(tag, attrs));
                    if let Some(what) = layer {
                        return Some((k.span(), what));
                    }
                    continue;
                }
                Part::Doubt => continue,
                Part::Search if opted_out(k) => "a search field the app draws (`appearance=\"none\"`)",
                Part::Search if at[SEARCH] == Place::Taken => "a second search field",
                Part::Tablist if at[TABLIST] == Place::Taken => "a second tablist",
                Part::Tablist if !mac_tabs(k) => {
                    "a tablist that is not two or more tabs each showing a title alone"
                }
                Part::Search => {
                    at[SEARCH] = Place::Taken;
                    continue;
                }
                Part::Tablist => {
                    at[TABLIST] = Place::Taken;
                    continue;
                }
                Part::Heading => {
                    at[PASSED] = Place::Taken;
                    continue;
                }
                Part::Text => match (at[PASSED], at[SUBTITLE]) {
                    (Place::Taken, Place::Taken) | (Place::Free, _) => {
                        "a text that is neither its heading nor the one after it"
                    }
                    (Place::Taken, _) => {
                        at[SUBTITLE] = Place::Taken;
                        continue;
                    }
                    (Place::Maybe, subtitle) => {
                        at[SUBTITLE] = Place::join([subtitle, Place::Taken]);
                        continue;
                    }
                },
                Part::Status if mac_draws(k) => continue,
                Part::Status => "a readout holding more than words, symbols and dots",
                Part::Pressable if has_press(k) && holds_heading(k) => "a pressable title",
                Part::Pressable if opted_out(k) => "a button that writes `appearance=\"none\"`",
                Part::Pressable if mac_button(k) => continue,
                Part::Pressable => "a button without a supported toolbar face",
                Part::Box => match walk(k.children(), at) {
                    Some(found) => return Some(found),
                    None => continue,
                },
                Part::Other => "an element other than its heading, its subtitle, buttons, readouts, a search field and a tablist",
            };
            return Some((k.span(), what));
        }
        None
    }
    walk(header.children(), &mut [Place::Free; 4])
}

/// `header-item-left-out` (LLP 1141.004 D5), information, for the targets
/// the app ships: each element iOS hosts that Android's top app bar and
/// tvOS's bar leave out, a readout too (a button whose face iOS hosts is an
/// action there, and none of this); and, once, the part of the header the
/// Mac's toolbar has no place for, which sends the whole header to the page.
fn left_out(bar: &Bar<'_>, header: Visit<'_>, out: &mut Vec<LowerError>) {
    let targets = crate::icons::shipped();
    let drops: Vec<String> = (targets.iter())
        .filter(|t| matches!(t.name.as_str(), "android" | "tvos"))
        .map(|t| t.described())
        .collect();
    let mut found: Vec<(Span, String)> = Vec::new();
    // In an app that ships iOS an element the bar has no object for is
    // refused (`lower-header-item`); a readout is the bar's.
    let ios = targets.iter().any(|t| t.name == "ios");
    let hosted = bar.hosted.iter().filter(|_| !ios);
    if !drops.is_empty() {
        for (span, what) in hosted.chain(bar.readouts.iter()) {
            let what = what
                .trim_end_matches(',')
                .trim_end_matches(", before the heading")
                .trim_end_matches(", after the subtitle");
            found.push((*span, format!("{what} in this route's `header` is a bar item on iOS, drawn as written; but on {} the bar has no place for it, and it is left out", drops.join(" and "))));
        }
    }
    if let Some(mac) = targets.iter().find(|t| t.name == "macos") {
        if let Some((span, what)) = mac_refusal(header) {
            found.push((span, format!("on {}, this route's `header` holds {what}, which the window's toolbar has no place for, so the whole header is drawn in the page as written", mac.described())));
        }
    }
    found.sort_by_key(|(span, _)| (span.line, span.col));
    for (span, says) in found {
        out.push(LowerError {
            id: "header-item-left-out",
            message: format!("{says}. The web and desktop Linux draw the header as written. To show it on every target, move it below the `header`, into the route's content (LLP 1141.004 D5; docs/contract-for-agents.md, \"Tabs and stacks\")"),
            span,
        });
    }
}
