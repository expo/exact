//! A value passed whole as JSON (`json<T>`, LLP 1133.011 D10): a string to
//! Contract, and to data code the type it holds. The TypeScript binding reads
//! the same grammar (`jsonHolds`, js/bake/src/__exact_binding.mjs).

use serde_json::Value;

/// What `json<T>` may hold, as a refusal names it.
pub(super) const HOLDS: &str = "`json<T>` holds a shape or DOM dictionary by name, `any`, `number`, `string`, `bool` or `enum<a|b>`, or `option<T>` or `T[]` of one";

/// Whether a declaration's type is `json<T>`, or an option or list of one.
pub(super) fn is_json(ty: &Value) -> bool {
    let mut t = ty.as_str().unwrap_or_default().trim();
    loop {
        match (t.strip_suffix("[]"), t.strip_prefix("option<")) {
            (Some(inner), _) => t = inner,
            (None, Some(inner)) => t = inner.strip_suffix('>').unwrap_or(inner),
            (None, None) => return t.starts_with("json<") && t.ends_with('>'),
        }
    }
}

/// Whether `json<held>` holds a JSON value's type, as [`HOLDS`] says.
pub(super) fn json_holds(held: &str) -> bool {
    let held = held.trim();
    if let Some(inner) = held.strip_suffix("[]") {
        return json_holds(inner);
    }
    if let Some(inner) = held
        .strip_prefix("option<")
        .and_then(|t| t.strip_suffix('>'))
    {
        return json_holds(inner);
    }
    let enumerated = held.starts_with("enum<") && held.ends_with('>') && !held[5..].contains('<');
    let named = held.starts_with(|c: char| c.is_ascii_uppercase())
        && held.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    enumerated || named || matches!(held, "any" | "number" | "string" | "bool")
}

/// Retain written keys for the positional input records of LLP 1133.039 D1.
/// Other package shapes keep JSON's sorted field order.
pub(super) fn shape_order(
    text: &str,
) -> Result<std::collections::BTreeMap<String, Vec<String>>, serde_json::Error> {
    use serde::{
        de::{IgnoredAny, MapAccess, Visitor},
        Deserialize, Deserializer,
    };
    #[derive(Deserialize)]
    struct Declaration {
        shapes: Option<std::collections::BTreeMap<String, Fields>>,
    }
    struct Fields(Vec<String>);
    impl<'de> Deserialize<'de> for Fields {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Keys;
            impl<'de> Visitor<'de> for Keys {
                type Value = Fields;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a shape's fields")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Fields, M::Error> {
                    let mut keys = Vec::new();
                    while let Some((key, _)) = map.next_entry::<String, IgnoredAny>()? {
                        keys.push(key);
                    }
                    Ok(Fields(keys))
                }
            }
            deserializer.deserialize_map(Keys)
        }
    }
    let declaration: Declaration = serde_json::from_str(text)?;
    Ok(declaration
        .shapes
        .unwrap_or_default()
        .into_iter()
        .map(|(name, fields)| (name, fields.0))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_holds_a_json_values_type_and_nothing_else() {
        for held in [
            "PublicKeyCredentialCreationOptionsJSON",
            "CloudRecordWrite[]",
            "option<Record>[][]",
            "any",
            "number[]",
            "bool",
            "enum<a|b>",
        ] {
            assert!(json_holds(held), "{held}");
        }
        for held in [
            "bytes",
            "string|bytes",
            "json<X>",
            "handle<file>",
            "unit",
            "lower",
            "Map<A>",
            "",
        ] {
            assert!(!json_holds(held), "{held}");
        }
    }

    #[test]
    fn a_list_or_option_of_json_is_json() {
        for ty in [
            "json<X>",
            "json<X[]>",
            "json<X>[]",
            "option<json<X>[]>",
            "option<json<X>>",
        ] {
            assert!(is_json(&Value::from(ty)), "{ty}");
        }
        for ty in ["X", "option<X>", "X[]"] {
            assert!(!is_json(&Value::from(ty)), "{ty}");
        }
    }

    #[test]
    fn ordinary_package_shapes_keep_their_name_order() {
        use contract_syntax::Span;
        for (text, name, expected) in [
            (
                include_str!("../../../../packages/health/exact.json"),
                "HealthBucket",
                vec!["end", "start", "value"],
            ),
            (
                include_str!("../../../../packages/barcode-detection/exact.json"),
                "Rect",
                vec!["height", "width", "x", "y"],
            ),
            (
                include_str!("../../../../packages/calls/exact.json"),
                "CallEvent",
                vec!["held", "muted"],
            ),
        ] {
            let json = serde_json::from_str(text).unwrap();
            let shapes = super::super::shapes(
                &json,
                &shape_order(text).unwrap(),
                Span {
                    line: 1,
                    col: 0,
                    end_col: 0,
                    source_id: 0,
                },
            )
            .unwrap();
            let shape = shapes.iter().find(|s| s.name == name).unwrap();
            assert_eq!(
                shape
                    .fields
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>(),
                expected,
                "{name}"
            );
        }
    }
}
