//! `lower-header-nested`: a route's `header` one or more boxes down from
//! it, the first child of a box that is the route's first child, as a
//! screen component's root `column` holds it when the route column that
//! uses the component is in the root file. iOS makes the bar only of the
//! route's own first child (`exact_kernel::bars::header_shape`), so this
//! header is drawn in the page as written. The euxy port's tabs did this.
//! A header that draws itself (`appearance="none"`) is not warned, here or
//! as the route's first child (`no_bar`).

use super::header_bar::{NOT_THE_BAR, OR_NO_BAR};
use super::{computed, literal};
use crate::LowerError;
use contract_checking::Structure;

/// The boxes a route's first child may be on the way down to its header:
/// plain layout boxes. A `section` is left out (a grouped list's section
/// opens with its own `header`), and so is a scroller, a list and anything
/// else that gives its first child a meaning of its own.
pub(super) const WRAPPERS: &[&str] = &["view", "box", "column", "row", "main", "article"];

/// The `header` at the end of the route's chain of first children, past at
/// least one box, with the tag of the box that is the route's first child.
/// An `if`, `match`, `for`, use or slot on the way is given the benefit.
pub(super) fn nested_header(children: Structure<'_>, out: &mut Vec<LowerError>) {
    let Some(first) = children.roots().next() else {
        return;
    };
    let Some((outer, _, attrs)) = first.element() else {
        return;
    };
    if !WRAPPERS.contains(&outer) || hidden(attrs) {
        return;
    }
    let region = first.regions()[0];
    for (node, head) in
        region.walk_regions_with(region.roots().next().map(|n| n.site()), |node, head| {
            let descend = Some(node.site()) == *head
                && node
                    .element()
                    .is_some_and(|(tag, _, attrs)| WRAPPERS.contains(&tag) && !hidden(attrs));
            [
                descend.then(|| node.children().next().map(|n| n.site())),
                None,
            ]
        })
    {
        if Some(node.site()) != head {
            continue;
        }
        let Some((tag, _, attrs)) = node.element() else {
            continue;
        };
        if tag == "header" && !no_bar(attrs) {
            out.push(LowerError {
                id: "lower-header-nested",
                message: format!("this `header` is not its route's first child (it is inside the `{outer}` that is), so on iOS it is not the navigation bar: {NOT_THE_BAR} Make it the route's first child: move the route's `navigationKey`, with its `navigationPresentation`, `navigationScroll` and the rest, onto the box that holds the header (a screen component's root), or move the header up{OR_NO_BAR} (docs/recipes/navigation-stack.md)"),
                span: node.span(),
            });
            return;
        }
    }
}

fn hidden(attrs: &[contract_syntax::Attr]) -> bool {
    literal(attrs, "display") == Some("none") || computed(attrs, "display")
}

/// A header hidden, or drawing itself (`appearance="none"`, LLP 1075.003
/// §9.10, as a site's does; a computed one has the benefit): iOS makes no bar
/// of it, not even a pushed route's system Back alone, so it hides nothing.
pub(super) fn no_bar(attrs: &[contract_syntax::Attr]) -> bool {
    hidden(attrs) || literal(attrs, "appearance") == Some("none") || computed(attrs, "appearance")
}
