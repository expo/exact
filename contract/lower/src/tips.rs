//! A tip over an element (LLP 1035.006.000): its title, message and symbol,
//! and whether its lesson is done, refused here where the shape is wrong.

use crate::{err, LowerError};
use contract_syntax::{Attr, Span};

const PARTS: [&str; 3] = ["tipMessage", "tipImage", "tipDone"];

/// D1: the platform remembers a tip by its element's `id`, so a tip needs
/// one; the other parts belong beside a `tip`; and its image is a symbol.
pub(crate) fn check(attrs: &[Attr], span: Span) -> Result<(), LowerError> {
    let tip = attrs.iter().find(|a| a.name == "tip");
    if tip.is_none() {
        if let Some(part) = attrs.iter().find(|a| PARTS.contains(&a.name.as_str())) {
            return err(
                "lower-tip-parts",
                format!(
                    "`{}` belongs beside a `tip`, the tip's title (LLP 1035.006.000 D1)",
                    part.name
                ),
                part.span,
            );
        }
        return Ok(());
    }
    if !attrs.iter().any(|a| a.name == "id") {
        return err(
            "lower-tip-id",
            "a tip needs its element's `id`: the platform remembers by it whether the person has seen or done the tip, across launches and whatever the title's language (LLP 1035.006.000 D1)",
            tip.map_or(span, |a| a.span),
        );
    }
    if let Some(image) = attrs.iter().find(|a| a.name == "tipImage") {
        for source in crate::symbols::literals(&image.value) {
            if !source.is_empty() && !source.starts_with("symbol:") {
                return err(
                    "lower-tip-image",
                    format!("`tipImage` takes a symbol, `symbol:<role>` or `symbol:sf/<name>`; given `{source}`"),
                    image.span,
                );
            }
        }
        let roles = crate::symbols::roles(&image.value);
        crate::symbols::check(roles.as_ref().unwrap_or(&image.value), image.span)?;
    }
    Ok(())
}
