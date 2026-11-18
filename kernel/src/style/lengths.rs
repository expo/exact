use super::Dimension;

/// A CSS length in an absolute unit (96 px to the inch), or a percentage
/// written as text, with the CSS number grammar `px` lengths use (LLP
/// 1055.000 D4). Unitless text stays refused.
pub(super) fn absolute_length(token: &str) -> Option<Dimension> {
    let number = |n: &str| parse_pixel_length(&format!("{n}px"));
    if let Some(n) = token.strip_suffix('%') {
        return number(n).map(Dimension::Percent);
    }
    let split = token.len().checked_sub(2)?;
    let (n, unit) = (token.get(..split)?, token.get(split..)?);
    let scale = match unit.to_ascii_lowercase().as_str() {
        "in" => 96.0,
        "cm" => 96.0 / 2.54,
        "mm" => 96.0 / 25.4,
        "pt" => 96.0 / 72.0,
        "pc" => 16.0,
        _ => return None,
    };
    number(n).map(|v| Dimension::Points(v * scale))
}

/// CSS pixel length or unitless zero, shared by dimensions and translation.
pub(crate) fn parse_pixel_length(token: &str) -> Option<f32> {
    let pixels = token
        .get(token.len().saturating_sub(2)..)
        .is_some_and(|unit| unit.eq_ignore_ascii_case("px"));
    let number = if pixels {
        &token[..token.len() - 2]
    } else {
        token
    };
    let value = parse_css_number(number)?;
    let mantissa = number.split(['e', 'E']).next()?;
    if value.abs() > f32::MAX as f64
        || (!pixels && mantissa.bytes().any(|b| b.is_ascii_digit() && b != b'0'))
    {
        return None;
    }
    Some(value as f32)
}

/// A finite CSS number token, before a property's unit conversion and range check.
pub(crate) fn parse_css_number(number: &str) -> Option<f64> {
    // Rust floats accept spellings outside CSS number tokens. Check the
    // decimal/exponent grammar before the range-preserving conversion.
    let unsigned = number.strip_prefix(['+', '-']).unwrap_or(number);
    let (mantissa, exponent) = unsigned.find(['e', 'E']).map_or((unsigned, None), |i| {
        (&unsigned[..i], Some(&unsigned[i + 1..]))
    });
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let valid_mantissa = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole.is_empty() || digits(whole)) && digits(fraction),
        None => digits(mantissa),
    };
    if !valid_mantissa || exponent.is_some_and(|e| !digits(e.strip_prefix(['+', '-']).unwrap_or(e)))
    {
        return None;
    }
    let value = exact_num::parse_f64(number).ok()?;
    value.is_finite().then_some(value)
}
