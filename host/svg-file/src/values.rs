//! SVG presentation values converted to the kernel's row values.
use std::borrow::Cow;

pub(super) fn presentation<'a>(name: &str, value: &'a str, weight: u16) -> Cow<'a, str> {
    let value = value.trim();
    let converted = match name {
        "font-weight" => match value.to_ascii_lowercase().as_str() {
            "normal" => Some(400),
            "bold" => Some(700),
            relative => exact_kernel::svg::scene::relative_font_weight(relative, weight),
        }
        .map(|n| n.to_string()),
        "font-style" if value.eq_ignore_ascii_case("oblique") => Some("italic".into()),
        "font-size" => crate::length_pixels(value, false).map(|n| n.to_string()),
        "opacity" | "fill-opacity" | "stroke-opacity" | "stop-opacity" | "flood-opacity" => value
            .strip_suffix('%')
            .and_then(|n| exact_num::parse_f64(n.trim()).ok())
            .filter(|n| n.is_finite())
            .map(|n| (n / 100.0).to_string()),
        _ => None,
    };
    converted.map_or(Cow::Borrowed(value), Cow::Owned)
}

pub(super) fn families(value: &str) -> Option<Vec<(bool, String)>> {
    use cssparser::{ParseError, Parser, Token};
    let mut count = 0;
    Parser::new(value)
        .parse_comma_separated(|p| {
            count += 1;
            if count > 64 {
                return Err(ParseError::<()>::custom(()));
            }
            if let Ok(name) = p.try_parse(|p| p.expect_string_cloned()) {
                if name.is_empty() {
                    return Err(ParseError::<()>::custom(()));
                }
                return Ok((true, name.to_string()));
            }
            let first = p.expect_ident_cloned()?;
            let mut name = first.to_string();
            let (installed, canonical) =
                exact_kernel::style::theme::typeface::family_members(&name).remove(0);
            let generic = !installed;
            while !p.is_exhausted() {
                if generic {
                    return Err(ParseError::custom(()));
                }
                let Token::Ident(next) = p.next()? else {
                    return Err(ParseError::custom(()));
                };
                name.push(' ');
                name.push_str(next);
            }
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "initial" | "inherit" | "unset" | "revert" | "revert-layer" | "default"
            ) {
                return Err(ParseError::custom(()));
            }
            if generic {
                name = canonical;
            }
            Ok((!generic, name))
        })
        .ok()
}
