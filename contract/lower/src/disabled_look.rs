//! @ref LLP 1115.000 D6 (amended 2026-10-13, Charlie's ruling) — a default
//! yields to the author's version of the same state: a disabled custom
//! pressable dims on the hosts that dim it, unless its author styles its
//! own disabled state. Decided here, once, from the element as written:
//! Contract marks the node (`disabledStyled`), and every host that dims
//! reads the mark.
//!
//! A node styles its own disabled state when it has `disabled` and
//!
//! - writes `opacity`, whatever its value: the author said how faint it is;
//! - or writes a colour, a background, a border (or its colour) or a filter
//!   whose expression reads a name its `disabled` expression reads from
//!   outside it (a slot, a derive, a prop; not a callback's parameter or a
//!   `match` binding): the look changes with what disables it.
//!
//! A literal `disabled` reads nothing, so only an `opacity` marks it.

use contract_syntax::{Attr, Expr};

/// The paint a disabled look is drawn in.
const PAINT: &[&str] = &[
    "color",
    "background",
    "background-color",
    "background-image",
    "border",
    "border-top",
    "border-right",
    "border-bottom",
    "border-left",
    "border-color",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "filter",
];

/// Whether the element, as its classes expanded it, styles its own disabled state.
pub(crate) fn styled(attrs: &[Attr]) -> bool {
    let Some(disabled) = attrs.iter().rev().find(|a| a.name == "disabled") else {
        return false;
    };
    if attrs.iter().any(|a| a.name == "opacity") {
        return true;
    }
    let mut reads = Vec::new();
    free_reads(&disabled.value, &mut Vec::new(), &mut reads);
    !reads.is_empty()
        && attrs
            .iter()
            .filter(|a| PAINT.contains(&a.name.as_str()))
            .any(|a| {
                let mut paint = Vec::new();
                free_reads(&a.value, &mut Vec::new(), &mut paint);
                paint.iter().any(|n| reads.contains(n))
            })
}

/// The names an expression reads from outside it: not a callback's
/// parameters, a `match` arm's binding or a `let`'s name in its own scope
/// (`bound`); a call reads its arguments, not its name.
pub(crate) fn free_reads(e: &Expr, bound: &mut Vec<String>, out: &mut Vec<String>) {
    if let Expr::Ident(n, _) = e {
        if !bound.contains(n) {
            out.push(n.clone());
        }
    }
    for (x, binds) in e.scoped_operands() {
        let depth = bound.len();
        bound.extend(binds.iter().map(String::from));
        free_reads(x, bound, out);
        bound.truncate(depth);
    }
}
