//! @ref LLP 1123 D16 — `-exact-safe-area-aspect-fit` claims the window's aspect-fit guide, which UIKit
//! confines to the window's near descendants and which misses child controllers' insets: it is
//! admitted on the app's first root alone, under a literal `viewport-fit="cover"`, where the root is
//! no navigation root, whose routes would be native owners the guide supplies nothing to.

use crate::error::err;
use crate::LowerError;
use contract_syntax::{Attr, Expr, Span};

const NAME: &str = "-exact-safe-area-aspect-fit";

/// Refuses the property anywhere but on the app's first root (`first`), and there without a
/// literal `viewport-fit="cover"` or on a navigation root.
pub(crate) fn check(first: bool, attrs: &[Attr], span: Span) -> Result<(), LowerError> {
    let Some(a) = attrs.iter().rev().find(|a| a.name == NAME) else {
        return Ok(());
    };
    if !first {
        return err(
            "lower-attr-value",
            format!("`{NAME}` is read only on the app's first root, the box the window's aspect-fit guide places; elsewhere no host reads it. Write it on the view's first element"),
            a.span,
        );
    }
    let cover = attrs
        .iter()
        .rev()
        .find(|a| a.name == "viewport-fit")
        .is_some_and(|a| matches!(&a.value, Expr::Str(s, _) if s == "cover"));
    if !cover {
        return err(
            "lower-attr-value",
            format!("`{NAME}` needs `viewport-fit=\"cover\"` on the same root: the guide's rectangle already keeps clear of the screen's edges, and `auto` would fit the root inside them first"),
            a.span,
        );
    }
    if attrs.iter().any(|a| a.name == "navigationKey") {
        return err(
            "lower-attr-value",
            format!("`{NAME}` is on a navigation root, whose routes are native screens the window's guide gives no room or insets to: give a full-window media screen its own app, or leave the routes to their screens' safe areas"),
            span,
        );
    }
    Ok(())
}
