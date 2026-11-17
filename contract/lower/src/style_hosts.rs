//! `style-undrawn` (LLP 1159.000 D4): for each target the app ships, each
//! style row the app sets that the target's host does not draw there, from
//! the schema's `hosts` column (`vocab::style_hosts`). A lint: the app runs
//! and the node draws as if the row were unset. A `best-effort` entry says
//! nothing here; `contract vocab` prints it.

use crate::control_roles::Rows;
use crate::{vocab, LowerError};
use contract_checking::View;
use contract_syntax::{Attr, Expr, Span, StyleDecl};
use exact_kernel::StyleId;

/// One row a declaration sets where a host does not draw it.
struct Found<'t> {
    /// The row and the scope its entry has, with why: one line each.
    row: StyleId,
    on: Option<&'static str>,
    why: &'static str,
    /// The attribute as first written, its span and line, and how many set it.
    name: String,
    span: Span,
    line: u32,
    count: usize,
    targets: Vec<&'t crate::icons::Target>,
}

/// The host whose column judges a target: tvOS runs iOS's presenter. A
/// target with no column (the web, which is the oracle; the terminal, which
/// has its own; watchOS, whose presenter the column was not read for) is
/// not judged.
fn column(target: &str) -> Option<&'static str> {
    Some(match target {
        "ios" | "tvos" => "ios",
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        "android" => "android",
        _ => return None,
    })
}

/// Whether a value may set the row: anything but a literal `none`, `normal`
/// or the row's default.
fn sets(row: StyleId, value: &Expr) -> bool {
    match value {
        Expr::None(_) => false,
        Expr::Str(v, _) => {
            let v = v.trim();
            !(v.eq_ignore_ascii_case("none")
                || v.eq_ignore_ascii_case("normal")
                || vocab::default(row).is_some_and(|d| d.eq_ignore_ascii_case(v)))
        }
        _ => true,
    }
}

/// The rows an attribute writes, a state part's (`:hover filter`) being the
/// row it names.
fn rows(a: &Attr) -> Vec<StyleId> {
    match contract_syntax::state::state_of(&a.name) {
        Some((_, row)) => match crate::tags::attr(row) {
            Some(crate::tags::AttrTarget::Styles(ids)) => ids.to_vec(),
            _ => Vec::new(),
        },
        None => crate::tags::rows(a),
    }
}

/// Every undrawn row, once a row, scope and reason, naming each target.
pub(crate) fn undrawn(view: &View<'_>, styles: &[StyleDecl], out: &mut Vec<LowerError>) {
    let targets = crate::icons::shipped();
    if !targets.iter().any(|t| column(&t.name).is_some()) {
        return;
    }
    let mut found: Vec<Found> = Vec::new();
    for (node, in_svg) in view.walk_with(false, |node, in_svg| {
        *in_svg || node.element().is_some_and(|(tag, _, _)| tag == "svg")
    }) {
        if let Some((tag, _, attrs)) = node.element() {
            // The `svg` element is its content's root: a row on it is its
            // content's, not a box's.
            let here = in_svg || tag == "svg";
            let scope = if here { "svg" } else { "box" };
            for a in Rows::of(attrs, styles).all().filter(|a| a.written) {
                for row in rows(a).into_iter().filter(|row| sets(*row, &a.value)) {
                    for entry in vocab::style_hosts(row) {
                        if !entry.none || entry.on.is_some_and(|on| on != scope) {
                            continue;
                        }
                        let Some(target) =
                            targets.iter().find(|t| column(&t.name) == Some(entry.host))
                        else {
                            continue;
                        };
                        match found
                            .iter_mut()
                            .find(|f| f.row == row && f.on == entry.on && f.why == entry.why)
                        {
                            Some(f) => {
                                if !f.targets.iter().any(|t| t.name == target.name) {
                                    f.targets.push(target);
                                }
                                if f.span != a.span {
                                    f.count += 1;
                                }
                            }
                            None => found.push(Found {
                                row,
                                on: entry.on,
                                why: entry.why,
                                name: a.name.clone(),
                                span: a.span,
                                line: a.span.line,
                                count: 1,
                                targets: vec![target],
                            }),
                        }
                    }
                }
            }
        }
    }
    for f in found {
        let ships: Vec<String> = f.targets.iter().map(|t| t.described()).collect();
        let set = match f.count {
            1 => format!("set at line {}", f.line),
            n => format!("set {n} times, the first at line {}", f.line),
        };
        out.push(LowerError {
            id: "style-undrawn",
            message: format!(
                "`{}` is not drawn on {}: {}. The node draws there as if it were unset ({set}; LLP 1159.000). `contract vocab {}` says what each host does",
                f.name,
                ships.join(" and "),
                f.why,
                f.name
            ),
            span: f.span,
        });
    }
}
