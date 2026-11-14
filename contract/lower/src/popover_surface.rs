//! A popover's default surface (LLP 1021; theme2 2026-10-21 18:45, 19:55):
//! an author who writes no surface on a `popover` gets the platform's, as
//! the web's UA style gives one a filled, bordered card and the Mac its
//! system popover. Only the compiler sees which rows the author wrote, so
//! it decides, and marks the node (`defaultSurface`); a host that paints the
//! popover itself draws the platform's surface under it (iOS's glass, from
//! the metrics' `popover-` rows), and inspect can say why.

use crate::{err, LowerError};
use contract_syntax::{Attr, Expr, Span};

/// Whether `name` is a row that draws a box's surface: its background, a
/// border's line or a shadow (a border's corner draws none), or a material,
/// in any state (`:hover background-color` and `:active background-color`
/// are authored surfaces too).
fn surface(name: &str) -> bool {
    let name = contract_syntax::state::state_of(name).map_or(name, |(_, row)| row);
    name.starts_with("background") && !matches!(name, "background-clip" | "background-attachment")
        || name == "box-shadow"
        || name.starts_with("border") && !name.ends_with("radius")
}

fn authored(attrs: &[Attr]) -> bool {
    attrs.iter().any(|a| a.written && surface(&a.name))
}

/// The mark on a popover whose author wrote no surface on it or its active
/// class arm (a sheet's implied row is no author's). Read classes before
/// expansion: an absent row and an authored `none` must stay distinct.
/// A menu (`role="menu"`) is the platform's menu, never painted.
pub(crate) fn implied<'s>(
    attrs: &[Attr],
    span: Span,
    style: &dyn Fn(&str) -> Option<&'s [Attr]>,
) -> Option<Attr> {
    let menu = attrs
        .iter()
        .any(|a| a.name == "role" && matches!(&a.value, Expr::Str(r, _) if r == "menu"));
    let popover = attrs.iter().any(|a| a.name == "popover") && !menu;
    if !popover || authored(attrs) {
        return None;
    }
    let bare = |name: &str| !style(name).is_some_and(authored);
    let value = match attrs.iter().find(|a| a.name == "class").map(|a| &a.value) {
        Some(Expr::Ident(name, _)) => Expr::Bool(bare(name), span),
        Some(Expr::Ternary(cond, yes, no, _)) => {
            let (Expr::Ident(yes, _), Expr::Ident(no, _)) = (&**yes, &**no) else {
                unreachable!("class rows checked before default surface")
            };
            let (yes, no) = (bare(yes), bare(no));
            if yes == no {
                Expr::Bool(yes, span)
            } else {
                Expr::Ternary(
                    cond.clone(),
                    Box::new(Expr::Bool(yes, span)),
                    Box::new(Expr::Bool(no, span)),
                    span,
                )
            }
        }
        _ => Expr::Bool(true, span),
    };
    (!matches!(value, Expr::Bool(false, _))).then(|| Attr {
        name: "defaultSurface".into(),
        value,
        span,
        written: crate::IMPLIED,
    })
}

/// `defaultSurface` is the compiler's: an author's would claim the
/// platform's surface where they wrote their own, or none on a node no
/// popover is.
pub(crate) fn check_authored(attrs: &[Attr]) -> Result<(), LowerError> {
    match attrs.iter().find(|a| a.name == "defaultSurface" && a.written) {
        Some(a) => err(
            "lower-default-surface",
            "`defaultSurface` is the compiler's: a `popover` whose author writes no background, border or shadow gets the platform's surface. Write a surface of your own to replace it",
            a.span,
        ),
        None => Ok(()),
    }
}
