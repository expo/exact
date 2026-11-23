//! @ref LLP 1013.002 D3 — what the compiler can prove of `portal`: an element
//! no placement may move, and a literal destination that, wherever the mover
//! exists, names only elements that cannot receive it. The runner checks the
//! rest at placement, where a destination that cannot receive the element,
//! or none, hides it.

use crate::LowerError;
use contract_checking::{Checking, Site, Visit};
use contract_syntax::{Attr, Expr};
use contract_types::Checked;
use std::rc::Rc;

/// Where a node lies, as the walk passes into it.
#[derive(Clone, Default)]
struct Within<'a> {
    /// The nearest enclosing element's tag; none for an app root.
    parent: Option<&'a str>,
    /// Its parent is a route (it carries a `navigationKey`).
    route: bool,
    /// Its parent consumes its children as cells: a list, or a grouped
    /// list's `section`.
    cells: bool,
    /// Inside an `svg`, not inside its `foreignObject`.
    svg: bool,
    /// Its parent is a route's header, whose children are bar items.
    bar: bool,
    /// Under a `for`: each row is another instance with its own descendants.
    repeated: bool,
    /// The arm taken at each enclosing `if`/`match`.
    arms: Rc<Vec<(Site, usize)>>,
}

impl Within<'_> {
    /// Whether no instance of one can exist beside an instance of the other.
    fn excludes(&self, other: &Within<'_>) -> bool {
        (self.arms.iter()).any(|(site, arm)| other.arms.iter().any(|(s, a)| s == site && a != arm))
    }
}

fn attr<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a Attr> {
    attrs.iter().rev().find(|a| a.name == name)
}

/// A literal that says something: absent and `""` are not; a computed value
/// may.
fn said(attrs: &[Attr], name: &str) -> Option<bool> {
    match attr(attrs, name).map(|a| &a.value) {
        None => Some(false),
        Some(Expr::Str(s, _)) => Some(!s.is_empty()),
        Some(_) => None,
    }
}

fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    match &attr(attrs, name)?.value {
        Expr::Str(s, _) => Some(s),
        _ => None,
    }
}

/// HTML's table and description list parts, which only their own parents hold.
fn table_part(tag: &str) -> bool {
    matches!(
        tag,
        "caption" | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td" | "dt" | "dd" | "li"
    )
}

/// A parent that consumes its children as slots rather than lays them out:
/// a scroller's content, a canvas's, a control's face, a paragraph's runs,
/// a table's or a list's parts (D3).
fn structural_parent(tag: &str) -> bool {
    matches!(
        tag,
        "scroll" | "canvas" | "button" | "link" | "select" | "table" | "dl" | "ul" | "ol"
    ) || table_part(tag)
        || crate::prose::text_container(tag)
}

/// Why no placement may move this element, when the source says so.
fn unmovable(tag: &str, attrs: &[Attr], within: &Within<'_>) -> Option<&'static str> {
    let Some(parent) = within.parent else {
        return Some("an app root has no place to move from");
    };
    if within.svg {
        return Some("an element inside an `svg` is drawn by it; move the whole `svg`");
    }
    if within.cells || structural_parent(parent) || table_part(tag) {
        return Some("its parent consumes it as a slot (a list's cell, a scroller's or a canvas's content, a control's face, a paragraph's run, a table's or a list's part), not as a layout child");
    }
    if matches!(tag, "dialog" | "option") || attr(attrs, "popover").is_some() {
        return Some("a dialog, a popover or an option is presented by the platform, not laid out in a parent");
    }
    if said(attrs, "navigationKey") != Some(false) {
        return Some("a route and a navigation root are projected by navigation");
    }
    if matches!(literal(attrs, "role"), Some("tab" | "tabpanel" | "tablist"))
        || (tag == "header" && within.route)
        || within.bar
    {
        return Some(
            "a tab, a tab panel, a route's header and its bar items are projected by navigation",
        );
    }
    if matches!(literal(attrs, "role"), Some("dialog" | "alertdialog")) {
        return Some("a dialog, a popover or an option is presented by the platform, not laid out in a parent");
    }
    None
}

/// Whether an element can receive moved content, as far as the source shows:
/// `Some(false)` when it provably cannot (D3).
fn receives(tag: &str, attrs: &[Attr], within: &Within<'_>) -> Option<bool> {
    let plain = matches!(
        tag,
        "box"
            | "view"
            | "column"
            | "row"
            | "main"
            | "section"
            | "article"
            | "aside"
            | "nav"
            | "footer"
    );
    if !plain || within.svg || within.cells || within.parent.is_some_and(structural_parent) {
        return Some(false);
    }
    if attr(attrs, "popover").is_some() {
        return Some(false);
    }
    let mut computed = false;
    for name in ["role", "navigationKey"] {
        match said(attrs, name) {
            Some(true) => return Some(false),
            Some(false) => {}
            None => computed = true,
        }
    }
    (!computed).then_some(true)
}

/// Every refusal the source proves, for each element that says `portal`.
pub(crate) fn check(checked: &Checked<'_>) -> Vec<LowerError> {
    let checking = Checking::new(checked);
    let view = checking.expanded();
    let nodes: Vec<(Visit<'_>, Within<'_>)> = view
        .structure()
        .walk_regions_with(Within::default(), |visit, within| {
            if visit.is_choice() {
                return [0, 1].map(|arm| {
                    let mut arms = (*within.arms).clone();
                    arms.push((visit.site(), arm));
                    Some(Within {
                        arms: Rc::new(arms),
                        ..within.clone()
                    })
                });
            }
            let inner = match visit.element() {
                Some((tag, _, attrs)) => Within {
                    parent: Some(tag),
                    route: said(attrs, "navigationKey") != Some(false),
                    cells: tag == "list" || (within.cells && tag == "section"),
                    bar: tag == "header" && within.route,
                    svg: (within.svg || tag == "svg") && tag != "foreignObject",
                    ..within.clone()
                },
                None => Within {
                    repeated: within.repeated || visit.repetition().is_some(),
                    ..within.clone()
                },
            };
            [Some(inner.clone()), Some(inner)]
        })
        .collect();
    // Each `id` the expansion carries, where it is, and whether that element
    // can receive content.
    let ids: Vec<(&Expr, Site, &Within<'_>, Option<bool>)> = (nodes.iter())
        .filter_map(|(visit, within)| {
            let (tag, _, attrs) = visit.element()?;
            let id = attr(attrs, "id")?;
            Some((
                &id.value,
                visit.site(),
                within,
                receives(tag, attrs, within),
            ))
        })
        .collect();
    let mut out = Vec::new();
    for (visit, within) in &nodes {
        let Some((tag, _, attrs)) = visit.element() else {
            continue;
        };
        let Some(portal) = attr(attrs, "portal") else {
            continue;
        };
        if matches!(&portal.value, Expr::Str(s, _) if s.is_empty()) {
            continue;
        }
        if let Some(why) = unmovable(tag, attrs, within) {
            out.push(LowerError {
                id: "lower-portal-mover",
                message: format!("`portal` on this `{tag}`: {why}. Put `portal` on a plain element, such as a `box` around it (LLP 1013.002 D3)"),
                span: portal.span,
            });
            continue;
        }
        let Expr::Str(name, _) = &portal.value else {
            continue;
        };
        let inside: Vec<Site> = visit.subtree().walk().map(|v| v.site()).collect();
        // The elements that may carry the id while this one exists: a
        // computed id may be any. One inside it names its own descendant only
        // when it is not repeated, since a row's mover may name another row's.
        let mut carriers = (ids.iter())
            .filter(|(e, _, at, _)| {
                !matches!(e, Expr::Str(s, _) if s != name) && !within.excludes(at)
            })
            .peekable();
        if carriers.peek().is_none()
            || !(ids.iter()).any(|(e, _, at, _)| {
                matches!(e, Expr::Str(s, _) if s == name) && !within.excludes(at)
            })
        {
            continue;
        }
        let mut cycle = true;
        let mut refused = true;
        for (_, site, _, ok) in carriers {
            let own = inside.contains(site) && !within.repeated;
            cycle &= own;
            refused &= own || *ok == Some(false);
        }
        let why = if cycle {
            "is this element or inside it, so moving it there would make a cycle"
        } else if refused {
            "cannot receive content: a destination is a plain `box`, `view`, `column`, `row`, `main`, `section`, `article`, `aside`, `nav` or `footer` with no `role`, `navigationKey` or `popover`, and not itself a slot of its parent"
        } else {
            continue;
        };
        out.push(LowerError {
            id: "lower-portal-target",
            message: format!("`portal=\"{name}\"`: the element with that `id` {why}. Point `portal` at a plain box outside this element, in a content area that admits one (LLP 1013.002 D3)"),
            span: portal.span,
        });
    }
    out
}
