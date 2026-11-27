//! @ref LLP 1123 D4 — a route's controller allocation (`navigationExtendedLayout`,
//! `navigationExtendsUnderOpaqueBars`) and its vertical bar (`navigationVerticalBar`,
//! `navigationVerticalBarCompression`), each one UIKit property of the route's controller, read
//! only on a route.

use crate::error::err;
use crate::LowerError;
use contract_syntax::{Attr, Expr};

/// Each attribute and the words it takes, but for `navigationExtendedLayout`'s edge list.
const WORDS: [(&str, &[&str], &str); 4] = [
    ("navigationExtendedLayout", &["all", "none"], "\"all\" (the default: the route's view lies under every translucent bar), \"none\" (under none), or the edges it lies under, each once, from \"top\", \"left\", \"bottom\" and \"right\""),
    ("navigationExtendsUnderOpaqueBars", &["true", "false"], "\"true\" (the route's view lies under opaque bars too) or \"false\" (the default: under translucent ones only)"),
    ("navigationVerticalBar", &["automatic", "disabled"], "\"automatic\" (the default: on an iPhone Duo the route's bars may move into the vertical bar) or \"disabled\" (they stay at the top and bottom)"),
    ("navigationVerticalBarCompression", &["automatic", "prefers-bar-items", "prefers-tab-bar"], "\"automatic\" (the default, the platform's), \"prefers-bar-items\" (the tab bar compresses first) or \"prefers-tab-bar\" (the bar items compress first), where a route's bar items and the tab bar share the vertical bar"),
];

const EDGES: [&str; 4] = ["top", "left", "bottom", "right"];

/// Refuses these attributes on anything but a route, and a literal value that is none of the
/// attribute's words; `route`: whether the element is a route.
pub(crate) fn check(attrs: &[Attr], route: bool) -> Result<(), LowerError> {
    for (name, words, takes) in WORDS {
        let Some(a) = attrs.iter().rev().find(|a| a.name == name) else {
            continue;
        };
        if !route {
            return err(
                "lower-attr-value",
                format!("`{name}` is read only on a route (an element with a `navigationKey`), whose controller it sets; elsewhere no host reads it"),
                a.span,
            );
        }
        let Expr::Str(s, _) = &a.value else { continue };
        let edges = || {
            let said: Vec<&str> = s.split_whitespace().collect();
            !said.is_empty()
                && said.iter().all(|w| EDGES.contains(w))
                && said.iter().enumerate().all(|(i, w)| !said[..i].contains(w))
        };
        if !words.contains(&s.as_str()) && !(name == "navigationExtendedLayout" && edges()) {
            return err(
                "lower-attr-value",
                format!("`{name}` takes {takes}"),
                a.span,
            );
        }
    }
    Ok(())
}
