//! `swipe` (LLP 1057.001 §8): the platform's discrete swipe, in the one
//! direction `swipe-direction` names.
use crate::tags::{p, AttrTarget};
use crate::LowerError;
use contract_checking::Checking;
use contract_syntax::{Attr, Expr, StyleDecl, TemplatePart};

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

/// A `swipe` without a `swipe-direction`, or a literal direction that is not
/// one of the four. A computed direction that names none leaves the swipe off.
pub(crate) fn refusal(attrs: &[Attr], a: &Attr) -> Option<LowerError> {
    let message = match (a.name.as_str(), &a.value) {
        ("swipe", _) if !attrs.iter().any(|b| b.name == "swipe-direction") => {
            "`swipe` needs a `swipe-direction`, the one way the platform recognizes its swipe: `left`, `right`, `up` or `down`"
        }
        ("swipe-direction", Expr::Str(s, _)) if !DIRECTIONS.contains(&s.as_str()) => {
            "`swipe-direction` is one of `left`, `right`, `up` or `down`; for a second direction, give a nested node its own `swipe`"
        }
        _ => return None,
    };
    Some(LowerError {
        id: "lower-swipe-direction",
        message: message.into(),
        span: a.span,
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
            let Some((_, _, attrs)) = node.element() else {
                continue;
            };
            if let (true, Some(a)) = (inside, attrs.iter().find(|a| a.name == "swipe")) {
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
