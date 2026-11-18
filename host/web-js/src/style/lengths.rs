//! Named lengths use the literal CSS projection before the browser evaluates math.

use exact_kernel::style::relative;
use exact_kernel::{StyleId, StyleProps, StyleValue};
use std::sync::LazyLock;

fn style_names() -> impl Iterator<Item = (usize, String)> {
    exact_kernel::TEXT_STYLES
        .iter()
        .enumerate()
        .flat_map(|(i, s)| {
            let name = format!("-exact-{}", s.name);
            let mut names = vec![name.clone()];
            if name.as_bytes().last().is_some_and(u8::is_ascii_digit) {
                names.push(format!(
                    "{}-{}",
                    &name[..name.len() - 1],
                    &name[name.len() - 1..]
                ));
            }
            if !s.alias.is_empty() {
                names.push(s.alias.to_string());
            }
            names.into_iter().map(move |name| (i, name))
        })
}

fn literal(id: StyleId, name: &str) -> String {
    let mut style = StyleProps::default();
    style
        .set_dynamic(id, &StyleValue::Text(name.into()))
        .expect("a named length");
    let css = exact_web::css::css_text_relative(&style, &[]).0;
    let property = super::css_property(id);
    css.split(';')
        .find_map(|d| d.strip_prefix(&format!("{property}:")))
        .expect("the literal's declaration")
        .to_string()
}

fn map(id: StyleId, names: impl Iterator<Item = String>) -> String {
    let pairs: Vec<_> = names
        .map(|name| {
            let css = literal(id, &name);
            format!("{name:?}:{css:?}")
        })
        .collect();
    format!(
        r#"(M=>v=>typeof v==="string"?cssNames(v.trim(),M):v)({{{}}})"#,
        pairs.join(",")
    )
}

/// A text style's length, including aliases, anywhere in a bound font size.
pub static TEXT_STYLE_MAP: LazyLock<String> =
    LazyLock::new(|| map(StyleId::FontSize, style_names().map(|(_, name)| name)));

static NAMED_LENGTH_MAP: LazyLock<String> = LazyLock::new(|| {
    map(
        StyleId::Width,
        exact_kernel::NAMED_LENGTHS
            .iter()
            .copied()
            .chain(exact_kernel::LENGTH_RAMPS.iter().map(|(name, _)| *name))
            .map(|name| format!("-exact-{name}")),
    )
});

/// Only the rows that accept a named length receive its map.
fn length_map(id: StyleId) -> Option<&'static str> {
    if id == StyleId::FontSize {
        Some(&TEXT_STYLE_MAP)
    } else if relative::admits_relative(id) && id != StyleId::LineHeight {
        Some(&NAMED_LENGTH_MAP)
    } else {
        None
    }
}

pub fn length_value(id: StyleId, f: &str, uses: &mut crate::code::Uses) -> Option<String> {
    let map = length_map(id)?;
    uses.rt("cssNames");
    // A font's other writes still need its style name to fill weight and leading.
    (id != StyleId::FontSize).then(|| format!("()=>{}(({f})())", uses.share('m', map.to_string())))
}

/// The weight and leading a whole text-style name supplies (LLP 1115.000 D12.2).
pub static TEXT_STYLE_ROWS: LazyLock<[String; 2]> = LazyLock::new(|| {
    use exact_kernel::style::metrics::Column;
    let (mut weights, mut leadings) = (Vec::new(), Vec::new());
    for (i, name) in style_names() {
        let weight = relative::text_style_weight(i as u8, Column::Web).to_string();
        let leading = relative::text_style_leading(i as u8, Column::Web).css();
        weights.push(format!("{name:?}:{weight:?}"));
        leadings.push(format!("{name:?}:{leading:?}"));
    }
    let map = |pairs: &[String]| {
        format!(
            r#"(M=>v=>typeof v==="string"?M[v.trim().toLowerCase()]??null:null)({{{}}})"#,
            pairs.join(",")
        )
    };
    [map(&weights), map(&leadings)]
});
