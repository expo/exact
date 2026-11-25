//! @ref LLP 1188 D3 — `arrangement`: a box whose two children are the panes of UIKit's arrangement on iOS 27.1 and later,
//! and CSS's boxes elsewhere. Apple: "Avoid placing an arrangement view inside a navigation split view, list, scroll view,
//! or other container that might cause part of your view to become inaccessible", and "place navigation containers like
//! navigation split views and tab views around it rather than within it". What the source proves is refused; the host
//! decides the rest, and lays the box out in CSS where it has no two children.

use crate::LowerError;
use contract_checking::{Checking, Visit};
use contract_syntax::{Attr, Expr};
use contract_types::Checked;

/// The words `arrangement` takes.
pub(crate) const WORDS: &[&str] = &[
    "none",
    "split",
    "split horizontal",
    "split vertical",
    "overlay",
    "overlay horizontal",
    "overlay vertical",
];

/// What `arrangement` takes, for its refusal and `contract vocab`.
pub(crate) const TAKES: &str = "`arrangement` takes \"split\" (its two children side by side in a wide box, stacked in a tall one, and either side of a fold) or \"overlay\" (the first over the second, and either side of a fold), each alone or followed by \"horizontal\" or \"vertical\" (the one axis it may use), or \"none\" (the default: CSS's layout)";

/// The attribute that arranges its box by a literal; a package view's own `arrangement` is the package's.
fn arranged<'a>(tag: &str, attrs: &'a [Attr]) -> Option<&'a Attr> {
    attrs.iter().find(|a| {
        a.name == "arrangement"
            && matches!(&a.value, Expr::Str(word, _) if word != "none")
            && !crate::native::declared(tag, a)
    })
}

fn has(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name == name)
}

/// Whether what is written here is shown elsewhere.
fn elsewhere(tag: &str, attrs: &[Attr]) -> bool {
    tag == "dialog" || has(attrs, "popover") || has(attrs, "portal")
}

pub(crate) fn check(checked: &Checked<'_>) -> Vec<LowerError> {
    let checking = Checking::new(checked);
    let visits = checking
        .expanded()
        .walk_with(None, |node, inside| match node.element() {
            Some(("scroll", ..)) => Some("a `scroll`"),
            Some(("list", ..)) => Some("a `list`"),
            Some((tag, _, attrs)) if arranged(tag, attrs).is_some() => {
                Some("another `arrangement`")
            }
            Some((tag, _, attrs)) if elsewhere(tag, attrs) || has(attrs, "navigationKey") => None,
            _ => *inside,
        });
    visits
        .filter_map(|(node, inside)| {
            let (tag, _, attrs) = node.element()?;
            let span = arranged(tag, attrs)?.span;
            let message = refusal(node, tag, attrs, inside.filter(|_| !elsewhere(tag, attrs)))?;
            Some(LowerError {
                id: "lower-arrangement",
                message,
                span,
            })
        })
        .collect()
}

/// Why the arranged `node`, a `tag` whose nearest container that cannot hold one is `inside`, cannot stand.
fn refusal(node: Visit<'_>, tag: &str, attrs: &[Attr], inside: Option<&str>) -> Option<String> {
    if matches!(tag, "scroll" | "list") {
        return Some(format!("`arrangement` is on a `{tag}`, whose children scroll. Put it on a `row` or `column` whose two children are the panes, and the scroller in a pane"));
    }
    if let Some(container) = inside {
        return Some(format!("`arrangement` is inside {container}, where part of a pane can be out of reach. Put the arrangement in the route, and what scrolls in its panes"));
    }
    let within = node.regions().into_iter().flat_map(|s| {
        s.walk_with(false, |node, away| {
            *away || matches!(node.element(), Some((tag, _, attrs)) if elsewhere(tag, attrs))
        })
    });
    for (node, away) in within {
        if !away && matches!(node.element(), Some((_, _, attrs)) if has(attrs, "navigationKey")) {
            return Some("`arrangement` holds a navigation stack, which UIKit keeps around an arrangement. Put the arrangement inside a route".into());
        }
    }
    let children: Vec<_> = node.children().map(|c| c.element()).collect();
    // A portal's box is no child here, and can be one more in a box with an `id`
    let certain = (children.len() > 2 || !has(attrs, "id"))
        && children
            .iter()
            .all(|c| matches!(c, Some((_, _, attrs)) if !has(attrs, "portal")));
    (children.len() != 2 && certain).then(|| {
        let count = match children.len() {
            0 => "none".to_string(),
            1 => "one".to_string(),
            n => n.to_string(),
        };
        format!("`arrangement` takes two children, the primary and the secondary, and this `{tag}` has {count}. Put the ones that belong together in a `column`")
    })
}
