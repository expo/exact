//! @ref LLP 1019 D9 — `font-variation-settings` as a style row:
//! `[["wght",650],…]`, the axes in order (FontAxes.swift reads it).
use exact_kernel::style::{FontFeatures, FontVariations};

/// Writes the row's value; `false` (nothing written) for `normal`.
pub(crate) fn font_axes_json(out: &mut String, v: &FontVariations) -> bool {
    if v.is_empty() {
        return false;
    }
    out.push('[');
    for (i, (tag, value)) in v.axes().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_tag(out, *tag);
        super::push_num(out, *value);
        out.push(']');
    }
    out.push(']');
    true
}

/// CSS `font-feature-settings` as a style row: `[["cv05",1],…]`, the
/// features in order (FontAxes.swift reads it); `false` for `normal`.
pub(crate) fn font_features_json(out: &mut String, f: &FontFeatures) -> bool {
    if f.is_empty() {
        return false;
    }
    out.push('[');
    for (i, (tag, value)) in f.features().iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_tag(out, *tag);
        out.push_str(&value.to_string());
        out.push(']');
    }
    out.push(']');
    true
}

/// `["tag",` with the tag's four bytes as a JSON string, a quote or a
/// backslash in it escaped, so Swift reads back the same four.
fn push_tag(out: &mut String, tag: u32) {
    out.push_str("[\"");
    for c in tag.to_be_bytes().map(char::from) {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push_str("\",");
}

#[cfg(test)]
mod tests {
    /// The style batch Swift paints from and the run it measures carry the
    /// same four bytes of each tag, a quote and a backslash included.
    #[test]
    fn the_style_row_and_the_measured_run_carry_the_same_features() {
        let mut style = exact_kernel::TextStyle::from_style(&exact_kernel::StyleProps::default());
        style.font_features =
            exact_kernel::style::FontFeatures::parse(r#"'a"bc', 'a\\bc' 3, "cv05""#).unwrap();
        let mut json = String::new();
        assert!(super::font_features_json(&mut json, &style.font_features));
        let row: Vec<(String, u32)> = serde_json::from_str(&json).unwrap();
        let run = crate::measure::c_run("x", style, None);
        let measured: Vec<(String, u32)> = (0..run.feature_count as usize)
            .map(|i| {
                let tag = run.feature_tags[i].to_be_bytes().map(char::from);
                (tag.iter().collect(), run.feature_values[i])
            })
            .collect();
        assert_eq!(row, measured);
        assert_eq!(
            row,
            [("a\"bc".into(), 1), ("a\\bc".into(), 3), ("cv05".into(), 1)]
        );
    }
}
