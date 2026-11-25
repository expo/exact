//! `swipe` (LLP 1057.001 §8): one handler, and beside it exactly one of
//! `swipe-flick`, the platform's discrete swipe in a direction, or
//! `swipe-follow`, a swipe the node follows left or right. The selector is the
//! compile-time half of the handler's name: lowering gives `swipe` the kind the
//! plan has always had for each gesture (`Swipe`, `Swiperight`, `Swipeleft`),
//! and the table's rows, keyed by what the plan holds, are main's. On a native
//! module's tag none of this applies: its names are the module's.
use crate::native;
use crate::tags::{p, AttrTarget};
use crate::LowerError;
use contract_checking::Checking;
use contract_syntax::{Attr, Expr, File, Node, StyleDecl, TemplatePart};

/// The swipe handlers, `swiperight`, `swipeleft` and `swipe`, and
/// `swipe-direction`, `swipe`'s kernel prop, for the tag table (`tags::attr`).
pub(crate) fn attr(name: &str) -> Option<AttrTarget> {
    match name {
        "swiperight" => Some(AttrTarget::Handler("swiperight")),
        "swipeleft" => Some(AttrTarget::Handler("swipeleft")),
        "swipe" => Some(AttrTarget::Handler("swipe")),
        "swipe-direction" => Some(AttrTarget::Prop(p("swipeDirection"))),
        _ => None,
    }
}

/// The directions a swipe takes, as UIKit's recognizer names them.
const DIRECTIONS: [&str; 4] = ["left", "right", "up", "down"];

/// The names no element writes since the fold, which stay table rows for what
/// the plan holds and a native module's tag.
const RETIRED: [&str; 3] = ["swiperight", "swipeleft", "swipe-direction"];

/// What a retired name's error says, which marks it as the one at its attribute.
const RETIRED_SAYS: &str = "is no longer an attribute";

/// One error at a retired name or a swipe's selector, the swipe rules', which
/// says what to write, in place of any other check's at the same attribute.
pub(crate) fn supersede(all: &mut Vec<LowerError>) {
    let own = |e: &LowerError| {
        e.id == "lower-swipe-direction"
            || e.id == "lower-unknown-attr" && e.message.contains(RETIRED_SAYS)
    };
    let at: Vec<_> = all.iter().filter(|e| own(e)).map(|e| e.span).collect();
    all.retain(|e| own(e) || !at.contains(&e.span));
}

/// The guide's sections for each gesture, which every refusal of a retired
/// name ends with.
const FOLLOW_GUIDE: &str = "docs/contract-for-agents.md#swipe-to-reply";
const FLICK_GUIDE: &str = "docs/contract-for-agents.md#a-flick-that-puts-the-keys-away";

/// What `contract vocab` says of a retired name, and the one test of "a
/// retired name": nothing for any other.
pub fn retired(name: &str) -> Option<&'static str> {
    Some(match name {
        "swiperight" => "`swiperight` is retired: a swipe the node follows to the right is `swipe=action swipe-follow=\"right\"`, and `tree` still names its event `swiperight`. On a native module's tag the name stays reserved for a prop its package declares",
        "swipeleft" => "`swipeleft` is retired: a swipe the node follows to the left is `swipe=action swipe-follow=\"left\"`, and `tree` still names its event `swipeleft`; a row's Delete is `delete=`. On a native module's tag the name stays reserved for a prop its package declares",
        "swipe-direction" => "`swipe-direction` is retired: a flick's direction is `swipe-flick`, written beside `swipe`, and `tree` still lists it as `swipeDirection`. On a native module's tag the name stays reserved for a prop its package declares",
        _ => return None,
    })
}

/// The kind an element's `swipe` lowers to: a following swipe's for the
/// literal `swipe-follow` on a tag that is not a module's, the flick's
/// otherwise (a module's tag, no selector, or a value `judge_written` refused).
pub fn event(tag: &str, attrs: &[Attr]) -> &'static str {
    match attrs
        .iter()
        .find(|a| a.name == "swipe-follow")
        .map(|a| &a.value)
    {
        Some(Expr::Str(v, _)) if !native::is_module_tag(tag) && v == "right" => "swiperight",
        Some(Expr::Str(v, _)) if !native::is_module_tag(tag) && v == "left" => "swipeleft",
        _ => "swipe",
    }
}

/// Whether the element's `swipe` is a flick as written: beside `swipe-flick`
/// alone, with no retired name, on a tag that is not a module's.
pub(crate) fn flick(tag: &str, attrs: &[Attr]) -> bool {
    !native::is_module_tag(tag)
        && attrs.iter().any(|a| a.name == "swipe-flick")
        && !attrs
            .iter()
            .any(|a| a.name == "swipe-follow" || RETIRED.contains(&a.name.as_str()))
}

/// The table name an attribute is looked up under: a flick's direction is the
/// `swipe-direction` row, and the follow selector has none, being the
/// handler's name. A module tag's names are their own.
pub fn row<'a>(tag: &str, name: &'a str) -> Option<&'a str> {
    match name {
        _ if native::is_module_tag(tag) => Some(name),
        "swipe-flick" => Some("swipe-direction"),
        "swipe-follow" => None,
        _ => Some(name),
    }
}

/// What the table says of an attribute, looked up under its `row`: a flick's
/// direction binds as `swipe-direction` does, and messages name what was written.
pub(crate) fn attr_valued(tag: &str, a: &Attr) -> Option<AttrTarget> {
    crate::tags::attr_valued(row(tag, &a.name).unwrap_or(&a.name), &a.value)
}

/// Whether lowering's attribute loop leaves the attribute out, having judged
/// it: the follow selector, which `event` reads, and a retired name, which
/// `judge_written` refused.
pub(crate) fn skipped(tag: &str, a: &Attr) -> bool {
    !native::is_module_tag(tag) && (a.name == "swipe-follow" || RETIRED.contains(&a.name.as_str()))
}

/// The pair checks on the element as lowered, on a tag that is not a
/// module's: `swipe` with exactly one selector, a selector beside `swipe`, and
/// a literal flick direction of the four, written or passed through a prop.
/// An element with a retired name gets none, `judge_written` having refused it.
pub(crate) fn refusal(tag: &str, attrs: &[Attr], a: &Attr) -> Option<LowerError> {
    if native::is_module_tag(tag) || attrs.iter().any(|b| RETIRED.contains(&b.name.as_str())) {
        return None;
    }
    let has = |name: &str| attrs.iter().any(|b| b.name == name);
    let message: String = match (a.name.as_str(), &a.value) {
        ("swipe", _) if !has("swipe-flick") && !has("swipe-follow") => {
            "`swipe` needs exactly one of `swipe-flick`, the platform's flick one way (`left`, `right`, `up` or `down`), or `swipe-follow`, a swipe the node follows (`\"left\"` or `\"right\"`)".into()
        }
        ("swipe", _) if has("swipe-flick") && has("swipe-follow") => {
            "`swipe` takes one of `swipe-flick` or `swipe-follow`, not both: an element names one gesture".into()
        }
        ("swipe-flick", Expr::Str(s, _)) if has("swipe") && !DIRECTIONS.contains(&s.as_str()) => {
            "`swipe-flick` is one of `left`, `right`, `up` or `down`; for a second direction, give a nested node its own `swipe`".to_string()
        }
        ("swipe-flick" | "swipe-follow", _) if !has("swipe") => {
            format!("`{}` names a swipe's gesture and its direction, beside `swipe=action`", a.name)
        }
        _ => return None,
    };
    Some(LowerError {
        id: "lower-swipe-direction",
        message,
        span: a.span,
    })
}

/// What every component's written view says, before expansion and before
/// templates fold, judged in every declaration: the follow selector beside
/// `swipe` is the quoted `"left"` or `"right"` written on the element, and no
/// element writes a retired name. A module's tag is skipped by the tag alone.
pub(crate) fn judge_written(file: &File) -> Vec<LowerError> {
    let mut out = vec![];
    for c in &file.components {
        for (tag, attrs) in swipe_elements(&c.view) {
            if native::is_module_tag(tag) {
                continue;
            }
            let has = |name: &str| attrs.iter().any(|a| a.name == name);
            let old: Vec<&Attr> = attrs
                .iter()
                .filter(|a| RETIRED.contains(&a.name.as_str()))
                .collect();
            for a in &old {
                let set: Vec<&str> = [
                    "swipe",
                    "swiperight",
                    "swipeleft",
                    "swipe-direction",
                    "swipe-flick",
                    "swipe-follow",
                ]
                .into_iter()
                .filter(|n| has(n))
                .collect();
                let guide = if a.name == "swipe-direction" {
                    FLICK_GUIDE
                } else {
                    FOLLOW_GUIDE
                };
                let message = match set.as_slice() {
                    ["swiperight"] => format!("`swiperight` {RETIRED_SAYS}: write `swipe=` with the same action and `swipe-follow=\"right\"` ({guide})"),
                    ["swipeleft"] => format!("`swipeleft` {RETIRED_SAYS}: write `swipe=` with the same action and `swipe-follow=\"left\"` ({guide})"),
                    ["swipe", "swipe-direction"] => format!("`swipe-direction` {RETIRED_SAYS}: rename it `swipe-flick`, keeping its value ({guide})"),
                    _ => format!("`{}` {RETIRED_SAYS}: an element takes one `swipe` and, beside it, exactly one of `swipe-flick` or `swipe-follow`, each naming one direction ({guide})", a.name),
                };
                out.push(LowerError {
                    id: "lower-unknown-attr",
                    message,
                    span: a.span,
                });
            }
            let follow = attrs.iter().find(|a| a.name == "swipe-follow");
            if let (true, Some(a)) = (has("swipe") && old.is_empty(), follow) {
                if !matches!(&a.value, Expr::Str(v, _) if v == "left" || v == "right") {
                    out.push(LowerError {
                        id: "lower-swipe-direction",
                        message: "`swipe-follow` names the gesture, so it is the quoted `\"left\"` or `\"right\"` written on the element, not an expression, a template or a prop's value; for a direction that depends on data, write two `if` branches".into(),
                        span: a.span,
                    });
                }
            }
        }
    }
    out
}

/// Every element of a written view, by its tag and attributes.
fn swipe_elements(written_view: &[Node]) -> impl Iterator<Item = (&String, &Vec<Attr>)> {
    Node::walk(written_view).filter_map(|n| match n {
        Node::Element { tag, attrs, .. } => Some((tag, attrs)),
        _ => None,
    })
}

/// `lower-swipe-unseen`: a `swipe` inside a grouped list. UIKit draws its
/// sections itself, their cards, standard cells, accessories, header and
/// footer strings, and mounts the author's views only for a row it cannot draw
/// as its own, so no swipe in it is the platform's to promise.
pub(crate) fn unseen_in_grouped(checking: &Checking<'_>, styles: &[StyleDecl]) -> Vec<LowerError> {
    // A list's appearance as lowering reads it, a class's rows included
    let style = |name: &str| {
        styles
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.attrs.as_slice())
    };
    let grouped = |attrs: &[Attr]| {
        let class = crate::class::rows(attrs, &style, &[]).ok().flatten();
        let read: Vec<Attr> = class
            .map(|(_, rows)| rows)
            .unwrap_or_default()
            .into_iter()
            .chain(attrs.iter().filter(|a| a.name != "class").cloned())
            .collect();
        // `appearance="auto"` alone makes it grouped (`grouped::style`), written plainly or in backticks
        read.iter()
            .rev()
            .find(|a| a.name == "appearance")
            .is_some_and(|a| match &a.value {
                Expr::Str(v, _) => v == "auto",
                // A template of literal parts, as lowering folds it (`au${"to"}`)
                Expr::Template(parts, _) => {
                    let mut folded = String::new();
                    parts.iter().all(|p| match p {
                        TemplatePart::Text(t) | TemplatePart::Expr(Expr::Str(t, _)) => {
                            folded.push_str(t);
                            true
                        }
                        TemplatePart::Expr(_) => false,
                    }) && folded == "auto"
                }
                _ => false,
            })
    };
    let mut out: Vec<LowerError> = Vec::new();
    // Each declaration's own view, and the inlined root, so a component's row inside its caller's grouped list is named
    // too, once however often it is used
    let views = std::iter::once(checking.expanded()).chain(
        checking
            .declarations()
            .enumerate()
            .map(|(index, _)| checking.authored(index)),
    );
    for view in views {
        let walk = view.walk_with(false, |node, inside| match node.element() {
            Some(("list", _, attrs)) if !*inside => grouped(attrs),
            _ => *inside,
        });
        for (node, inside) in walk {
            let Some((tag, _, attrs)) = node.element() else {
                continue;
            };
            if let (true, Some(a)) = (
                inside && event(tag, attrs) == "swipe",
                attrs.iter().find(|a| a.name == "swipe"),
            ) {
                if out.iter().any(|e| e.span == a.span) {
                    continue;
                }
                out.push(LowerError {
                    id: "lower-swipe-unseen",
                    message: "`swipe` inside a grouped list: UIKit draws its sections itself, their standard cells, accessories, headers and footers, and mounts your views only for a row it cannot draw as its own, so the platform's swipe is not promised there; put `swipe` outside the list (LLP 1057.001 §8)".into(),
                    span: a.span,
                });
            }
        }
    }
    out
}
