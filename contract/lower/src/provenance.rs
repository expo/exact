//! @ref LLP 1123 D5, "Provenance" — a reservation reads only incoming observations. A raw
//! one, a native target's safe area or the owner controller's system minimum, is read as the
//! host reports it, without the provenance that takes Exact's own contributions out of an
//! incoming band (D11), so a target's safe area counts what Exact reserves and a reservation
//! that read it would grow by what it reserves. Lowering refuses a raw name in a reservation's
//! value, directly, in a fallback, or through a custom property any of whose declarations
//! reads one, a property being checked by its name wherever it is declared. The kernel
//! resolves such a reservation as invalid, so the rule holds for a plan another compiler
//! wrote.

use crate::LowerError;
use contract_syntax::{Attr, Expr, Span, TemplatePart};
use contract_types::Checked;
use exact_kernel::style::calc::raw_read;
use exact_kernel::style::safe_area_properties::is_author_name;
use std::collections::{BTreeMap, BTreeSet};

/// The reservation properties (D11): what a box adds to the unsafe area inside it.
const RESERVATIONS: &[&str] = &["-exact-additional-safe-area", "-exact-safe-area-padding"];

/// A refusal for each reservation in `checked`'s expanded view and its styles that reads a
/// raw observation.
pub(crate) fn check(checked: &Checked<'_>) -> Vec<LowerError> {
    let checking = contract_checking::Checking::new(checked);
    let view = checking.expanded();
    let elements = (view.walk()).filter_map(|v| v.element().map(|(_, _, attrs)| attrs));
    let styles = checked.file.styles.iter().map(|s| s.attrs.as_slice());
    let attrs: Vec<&Attr> = (elements.chain(styles).flatten())
        .filter(|a| is_author_name(&a.name) || RESERVATIONS.contains(&a.name.as_str()))
        .collect();
    if !attrs
        .iter()
        .any(|a| RESERVATIONS.contains(&a.name.as_str()))
    {
        return Vec::new();
    }
    // Each custom property one of whose declarations reads a raw observation, or another
    // such property, with that declaration, until a pass adds none
    let mut raw: BTreeMap<&str, Span> = BTreeMap::new();
    loop {
        let before = raw.len();
        for a in attrs.iter().filter(|a| is_author_name(&a.name)) {
            let reads = texts(&a.value).any(|t| raw_read(t, &|n| raw.contains_key(n)).is_some());
            if reads && !raw.contains_key(a.name.as_str()) {
                raw.insert(&a.name, a.span);
            }
        }
        if raw.len() == before {
            break;
        }
    }
    let mut seen = BTreeSet::new();
    (attrs.iter())
        .filter(|a| RESERVATIONS.contains(&a.name.as_str()))
        .filter_map(|a| {
            let read = texts(&a.value).find_map(|t| raw_read(t, &|n| raw.contains_key(n)))?;
            let span = a.span;
            seen.insert((span.source_id, span.line, span.col))
                .then(|| refusal(a, &read, raw.get(read.as_str())))
        })
        .collect()
}

/// The refusal of `a`, which reads `read`, a raw observation, or a custom property `declared`
/// there declares from one.
fn refusal(a: &Attr, read: &str, declared: Option<&Span>) -> LowerError {
    let what = match declared {
        Some(at) => format!(", and a declaration of `{read}` on line {} reads a raw observation. Lowering checks a custom property by its name, wherever it is declared: if this reservation reads another declaration of `{read}`, give that one a name of its own", at.line),
        None => ", a raw observation".to_string(),
    };
    LowerError {
        id: "lower-reservation-reads-raw",
        message: format!(
            "`{}` reads `var({read})`{what}. A reservation reads only incoming observations (LLP 1123 D5): a native target's safe area counts what Exact reserves, so a reservation that read it would grow by what it reserves, and the system minimum is raw as well. Read the box's own `var(--exact-safe-area-inset-*)`, its keyboard's or its layout regions'",
            a.name
        ),
        span: a.span,
    }
}

/// The literal texts of an attribute's value, in every arm of a choice.
fn texts(value: &Expr) -> impl Iterator<Item = &str> {
    value.walk().flat_map(|e| match e {
        Expr::Str(s, _) => vec![s.as_str()],
        Expr::Template(parts, _) => (parts.iter())
            .filter_map(|p| match p {
                TemplatePart::Text(t) => Some(t.as_str()),
                TemplatePart::Expr(_) => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}
