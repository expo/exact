//! LLP 1123 C11b: only UIKit projects a route's last toolbar row into a bar.
mod collision;

use super::*;
use contract_checking::{Checking, Structure};
use contract_syntax::TemplatePart;

/// A toolbar spacer is an empty flex item on hosts that lay out the row.
pub(crate) const SPACE: &[(StyleId, &str)] = &[
    (StyleId::FlexGrow, "1"),
    (StyleId::FlexShrink, "1"),
    (StyleId::FlexBasis, "0%"),
];

fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    match &attrs.iter().rev().find(|a| a.name == name)?.value {
        Expr::Str(s, _) => Some(s),
        Expr::Template(parts, _) => match parts.as_slice() {
            [TemplatePart::Text(s)] => Some(s),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn bottom(attrs: &[Attr]) -> bool {
    literal(attrs, "toolbarPlacement") == Some("bottom")
}

pub(crate) fn check_row(
    place: &NavPlace,
    tag: &str,
    attrs: &[Attr],
    span: Span,
) -> Result<(), LowerError> {
    if !matches!(place, NavPlace::RouteColumn)
        || tag != "row"
        || literal(attrs, "role") != Some("toolbar")
    {
        return err(
            "lower-toolbar-place",
            "Put the bottom toolbar on a `row role=\"toolbar\"` directly inside the route's keyed `column`, after its scroller",
            span,
        );
    }
    Ok(())
}

pub(crate) fn check(checked: &Checked<'_>, warnings: &mut Vec<LowerError>) -> Vec<LowerError> {
    let checking = Checking::new(checked);
    let targets = crate::icons::shipped();
    let ios = targets.iter().find(|t| t.name == "ios");
    let iphone =
        ios.is_some_and(|t| t.includes_iphone) && !checked.types.shapes.fns.contains_key("top");
    let mut marked = std::collections::BTreeMap::new();
    let mut errors = Vec::new();
    let view = checking.expanded();
    for (node, parent) in view.walk_with(None, |v, parent| {
        v.element().map(|(tag, _, attrs)| (tag, attrs)).or(*parent)
    }) {
        let Some((tag, pos, attrs)) = node.element() else {
            continue;
        };
        if tag == "spacer"
            && (!parent.is_some_and(|(p, a)| {
                p == "row"
                    && literal(a, "role") == Some("toolbar")
                    && (bottom(a) || !a.iter().any(|attr| attr.name == "toolbarPlacement"))
            }) || !pos.is_empty()
                || node.children().next().is_some())
        {
            errors.push(LowerError {
                id: "lower-toolbar-space",
                span: node.span(),
                message: "Put an empty `spacer` in a bottom toolbar row or an unplaced toolbar row. For other flexible gaps use `view flex=1`".into(),
            });
        }
        if ios.is_some()
            && parent.is_some_and(|(_, a)| bottom(a))
            && !matches!(tag, "button" | "spacer")
        {
            errors.push(LowerError {
                id: "lower-toolbar-item",
                span: node.span(),
                message: format!("The iOS bottom toolbar holds buttons and flexible spacers. Move this `{tag}` outside the toolbar row"),
            });
        }
        if tag == "column" && attrs.iter().any(|a| a.name == "navigationKey") {
            match sites(node.regions()[0], warnings) {
                Ok(Some(span)) if iphone => {
                    marked.insert(node.site(), span);
                }
                Err(error) => errors.push(error),
                _ => {}
            }
        }
    }
    if !marked.is_empty() {
        if let Some(routes) = &checked.file.routes {
            collision::check(view.structure(), routes, &marked, warnings);
        }
    }
    errors
}

fn sites(
    children: Structure<'_>,
    warnings: &mut Vec<LowerError>,
) -> Result<Option<Span>, LowerError> {
    let mut toolbar = None;
    let mut possible_bar = false;
    let mut unconditional_bar = false;
    let mut pending = Vec::new();
    for (node, unconditional) in children.walk_regions_with(true, |node, _| {
        if node.element().is_some() {
            [None, None]
        } else {
            [Some(false), Some(false)]
        }
    }) {
        let Some((tag, _, attrs)) = node.element() else {
            continue;
        };
        let bar = bottom(attrs);
        if !possible_bar
            && bar
            && unconditional
            && tag == "row"
            && literal(attrs, "role") == Some("toolbar")
            && collision::visible(attrs)
            && node.children().any(|item| {
                item.element()
                    .is_some_and(|(tag, _, a)| tag == "button" && collision::visible(a))
            })
        {
            toolbar = Some(node.span());
        }
        possible_bar |= attrs
            .iter()
            .any(|a| a.name == "class" || a.name == "toolbarPlacement");
        if unconditional {
            if bar && unconditional_bar {
                return err(
                    "lower-toolbar-count",
                    "A route has one bottom toolbar. Put its buttons and spacers in the same row",
                    node.span(),
                );
            }
            unconditional_bar |= bar;
            for span in pending.drain(..) {
                warnings.push(LowerError {
                    id: "lower-toolbar-last",
                    span,
                    message: "Put the toolbar row last in the route's column, after the scroller, so it sits at the bottom on hosts that draw the row in place".into(),
                });
            }
        }
        if bar {
            pending.push(node.span());
        }
    }
    Ok(toolbar)
}
