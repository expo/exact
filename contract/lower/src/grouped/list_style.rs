//! @ref LLP 1084 D2, LLP 1127 D17 — a grouped list's `listStyle`: its
//! names, the one a list's attributes ask for, and which lay out as a plain
//! list.

use crate::{err, LowerError};
use contract_syntax::{Attr, Expr};

/// `listStyle`'s names (D2), UIKit's list appearances; a host with no
/// sidebar list draws `sidebar` (LLP 1127 D17) as the plain list.
const STYLES: &[&str] = &["inset-grouped", "grouped", "plain", "sidebar"];

/// Whether `style` lays out as the plain list: `plain`, and `sidebar` where
/// the host has no sidebar list.
pub(super) fn plain(style: &str) -> bool {
    matches!(style, "plain" | "sidebar")
}

/// The style a grouped list's attributes ask for: `None` for any other
/// element. `appearance` must be a literal (it decides what the node is),
/// and `listStyle` is only a grouped list's.
pub(crate) fn style(tag: &str, attrs: &[Attr]) -> Result<Option<&'static str>, LowerError> {
    let grouped = match attrs.iter().rev().find(|a| a.name == "appearance") {
        Some(a) if tag == "list" => match &a.value {
            Expr::Str(v, _) => v == "auto",
            _ => {
                return err(
                    "lower-grouped-list",
                    "a `list`'s `appearance` is a literal: `\"auto\"` makes it a grouped list. To switch, write `if` with two lists",
                    a.span,
                )
            }
        },
        None if tag == "list" && attrs.iter().any(|a| a.name == "reorderdrop") =>
            attrs.iter().any(|a| a.name == "listStyle" && matches!(&a.value, Expr::Str(s, _) if s == "inset-grouped")),
        _ => false,
    };
    let named = attrs.iter().rev().find(|a| a.name == "listStyle");
    if !grouped {
        return match named {
            Some(a) => err(
                "lower-grouped-list",
                "`listStyle` styles a grouped list: `list appearance=\"auto\"`",
                a.span,
            ),
            None => Ok(None),
        };
    }
    if let Some(a) = attrs
        .iter()
        .find(|a| a.name == "virtualized" && !matches!(a.value, Expr::Bool(false, _)))
    {
        return err(
            "lower-grouped-list",
            "a grouped list builds every row; write appearance=\"none\" for a virtualized list, or remove `virtualized` to keep the grouped list",
            a.span,
        );
    }
    match named.map(|a| (&a.value, a.span)) {
        None => Ok(Some(STYLES[0])),
        Some((Expr::Str(v, _), span)) => match STYLES.iter().find(|s| **s == v) {
            Some(s) => Ok(Some(s)),
            None => err(
                "lower-grouped-list",
                format!(
                    "`listStyle=\"{v}\"` is not a list style; styles: {}",
                    STYLES.join(", ")
                ),
                span,
            ),
        },
        Some((_, span)) => err(
            "lower-grouped-list",
            "`listStyle` is a literal: the sheet Contract writes for the other hosts is chosen when the view compiles",
            span,
        ),
    }
}
