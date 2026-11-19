//! `lower-env-in-owner` (LLP 1123 D9): a route reads the window's
//! `env(-exact-<region>-inset-*)` or `env(viewport-segment-*)`. A route is a
//! content owner whose layout regions and segments are its own, read as
//! `var(--exact-…)`: a pane placed on the window's segment misses the fold by
//! the bar's height. `env(safe-area-inset-*)` reads what its context leaves
//! (`exact_kernel`'s inset rule), so it is the route's already. A dialog or a
//! popover is the document's (D9), so `env()` is right inside one. A computed
//! value is given the benefit.

use crate::LowerError;
use contract_checking::Visit;
use contract_syntax::{Attr, Expr, StyleDecl};
use std::collections::BTreeSet;

/// Warns once for each row of a route's root or its content, written there
/// or in a class it names, that reads one of the window's owner values: the
/// root's own padding clears what the route already clears. A dialog's or a
/// popover's subtree is the document's.
pub(super) fn env_in_owner(
    route: Visit<'_>,
    styles: &[StyleDecl],
    seen: &mut BTreeSet<(u32, u32, u32)>,
    out: &mut Vec<LowerError>,
) {
    let documents = |node: Visit<'_>| {
        node.element().is_some_and(|(tag, _, attrs)| {
            tag == "dialog" || attrs.iter().any(|a| a.name == "popover")
        })
    };
    let rows = route.subtree().walk_regions_with((), |node, _| {
        let descend = (!documents(node)).then_some(());
        [descend, descend]
    });
    for (node, ()) in rows {
        let Some((_, _, attrs)) = node.element().filter(|_| !documents(node)) else {
            continue;
        };
        let classes = attrs
            .iter()
            .filter(|a| a.name == "class")
            .filter_map(|a| match &a.value {
                Expr::Ident(name, _) => styles.iter().find(|s| &s.name == name),
                _ => None,
            });
        for row in attrs.iter().chain(classes.flat_map(|s| &s.attrs)) {
            warn(row, seen, out);
        }
    }
}

fn warn(row: &Attr, seen: &mut BTreeSet<(u32, u32, u32)>, out: &mut Vec<LowerError>) {
    let Expr::Str(value, _) = &row.value else {
        return;
    };
    let Some(owned) = owner_spelling(value) else {
        return;
    };
    if !seen.insert((row.span.source_id, row.span.line, row.span.col)) {
        return;
    }
    let name = &row.name;
    out.push(LowerError {
        id: "lower-env-in-owner",
        message: format!("`{name}=\"{value}\"` reads the window's `env()` inside a route, which has layout regions and fold segments of its own, measured from where its content starts (LLP 1123 D9): a pane on the window's segment misses the fold by the bar's height. Write the route's: `{name}=\"{owned}\"`. In a dialog or a popover, which are the window's, `env()` is right"),
        span: row.span,
    });
}

/// `value` with each `env()` an owner gives its own value for spelled as
/// that value's `var(--exact-…)`.
fn owner_spelling(value: &str) -> Option<String> {
    let mut out = String::new();
    let (mut rest, mut changed) = (value, false);
    while let Some(at) = rest.find("env(") {
        out.push_str(&rest[..at]);
        let inner = &rest[at + 4..];
        let end = closing(inner)?;
        let (name, fallback) = match inner[..end].split_once(',') {
            Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
            None => (inner[..end].trim(), None),
        };
        let owned = if let Some(field) = name.strip_prefix("viewport-segment-") {
            let mut parts = field.split_whitespace();
            match (parts.next(), parts.next(), parts.next(), parts.next()) {
                (Some(f), Some(x), Some(y), None) => {
                    Some(format!("--exact-viewport-segment-{f}-{x}-{y}"))
                }
                _ => None,
            }
        } else {
            name.strip_prefix("-exact-")
                .filter(|n| n.contains("-inset-"))
                .map(|n| format!("--exact-{n}"))
        };
        match owned.filter(|n| exact_kernel::style::calc::host_name(n)) {
            Some(n) => {
                out.push_str(&match fallback {
                    Some(f) => format!("var({n}, {f})"),
                    None => format!("var({n})"),
                });
                changed = true;
            }
            None => out.push_str(&rest[at..at + 4 + end + 1]),
        }
        rest = &inner[end + 1..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

/// The index of the `)` that closes the function whose arguments `text` starts with.
fn closing(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth == 0 => return Some(i),
            ')' => depth -= 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::owner_spelling;

    #[test]
    fn each_owner_value_is_spelled_as_its_var() {
        // The safe area's inset reads the route's remainder already
        assert_eq!(owner_spelling("calc(env(safe-area-inset-top) + 8px)"), None);
        assert_eq!(
            owner_spelling("env(viewport-segment-left 1 0)").as_deref(),
            Some("var(--exact-viewport-segment-left-1-0)")
        );
        assert_eq!(
            owner_spelling("max(0px, env(-exact-readable-content-corners-inset-left))").as_deref(),
            Some("max(0px, var(--exact-readable-content-corners-inset-left))")
        );
        assert_eq!(owner_spelling("env(keyboard-inset-height)"), None);
        assert_eq!(owner_spelling("env(titlebar-area-x)"), None);
        // A fallback is kept, a function in it whole.
        assert_eq!(
            owner_spelling("env(safe-area-inset-top, 0px)"),
            None
        );
        assert_eq!(
            owner_spelling("calc(env(viewport-segment-width 0 0, calc(50% - 1px)) + 2px)")
                .as_deref(),
            Some("calc(var(--exact-viewport-segment-width-0-0, calc(50% - 1px)) + 2px)")
        );
        assert_eq!(
            owner_spelling("env(-exact-safe-area-corners-inset-left,0px)").as_deref(),
            Some("var(--exact-safe-area-corners-inset-left, 0px)")
        );
    }
}
