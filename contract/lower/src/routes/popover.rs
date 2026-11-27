//! @ref LLP 1123 D7 — a route presented as a native popover (`navigationPresentation="popover"`),
//! anchored to the element its `navigationPopoverAnchor` names, and the attributes that shape it,
//! each one UIKit property, read only on such a route.

use crate::error::err;
use crate::LowerError;
use contract_syntax::{Attr, Expr};

/// The attributes only a popover route reads.
pub(crate) const ATTRS: [&str; 5] = [
    "navigationPopoverAnchor",
    "navigationPopoverMargins",
    "navigationPopoverArrows",
    "navigationPopoverOverlapsSource",
    "navigationPopoverAdaptation",
];

/// Refuses a popover attribute on anything but a popover route, a literal popover route with no
/// anchor, and a literal margin or arrow list that is malformed; `route`: whether the element is
/// a route.
pub(crate) fn check(attrs: &[Attr], route: bool) -> Result<(), LowerError> {
    let literal = |name: &str| {
        attrs
            .iter()
            .rev()
            .find(|a| a.name == name)
            .and_then(|a| match &a.value {
                Expr::Str(s, _) => Some(s.as_str()),
                _ => None,
            })
    };
    let presentation = attrs
        .iter()
        .rev()
        .find(|a| a.name == "navigationPresentation");
    // A computed presentation may be a popover; a literal one says
    let popover =
        presentation.is_some_and(|a| !matches!(&a.value, Expr::Str(s, _) if s != "popover"));
    for name in ATTRS {
        let Some(a) = attrs.iter().rev().find(|a| a.name == name) else {
            continue;
        };
        if !route || !popover {
            return err(
                "lower-attr-value",
                format!("`{name}` is read only on a route presented as a popover (`navigationPresentation=\"popover\"`), where it shapes the popover; elsewhere no host reads it"),
                a.span,
            );
        }
    }
    if let Some(a) =
        presentation.filter(|_| route && literal("navigationPresentation") == Some("popover"))
    {
        if !attrs.iter().any(|a| a.name == "navigationPopoverAnchor")
            || literal("navigationPopoverAnchor") == Some("")
        {
            return err(
                "lower-attr-value",
                "a popover route points at the element it is anchored to: write `navigationPopoverAnchor` with that element's `id` (the button that opens it, most often)",
                a.span,
            );
        }
    }
    if let Some(a) = attrs
        .iter()
        .rev()
        .find(|a| a.name == "navigationPopoverMargins")
    {
        if let Expr::Str(s, _) = &a.value {
            let lengths: Vec<_> = s.split_whitespace().map(|w| w.parse::<f64>()).collect();
            if lengths.is_empty()
                || lengths.len() > 4
                || !lengths
                    .iter()
                    .all(|l| l.as_ref().is_ok_and(|l| l.is_finite() && *l >= 0.0))
            {
                return err(
                    "lower-attr-value",
                    "`navigationPopoverMargins` takes one to four lengths in points, at least 0, as `padding` takes them (\"16\", \"16 8\", \"16 8 24 8\"): how far the popover keeps from the window's edges",
                    a.span,
                );
            }
        }
    }
    if let Some(a) = attrs
        .iter()
        .rev()
        .find(|a| a.name == "navigationPopoverArrows")
    {
        if let Expr::Str(s, _) = &a.value {
            let words: Vec<_> = s.split_whitespace().collect();
            let directions = ["up", "down", "left", "right"];
            let any = words == ["any"];
            let each = !words.is_empty()
                && words.iter().all(|w| directions.contains(w))
                && words
                    .iter()
                    .enumerate()
                    .all(|(i, w)| !words[..i].contains(w));
            if !any && !each {
                return err(
                    "lower-attr-value",
                    "`navigationPopoverArrows` takes \"any\" (the default) or any of \"up\", \"down\", \"left\" and \"right\", each once: the directions the popover's arrow may point",
                    a.span,
                );
            }
        }
    }
    Ok(())
}
