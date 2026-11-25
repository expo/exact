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

/// Collect the interface's own and imported record declarations before checking its agent words.
pub(crate) fn agent_shapes(root: &std::path::Path, package: &str) -> Result<Value, String> {
    let root = std::path::absolute(root.join(".")).map_err(|e| e.to_string())?;
    let mut consulted = Vec::new();
    let source =
        crate::resolve::resolve(package, &root, &crate::Origin::App, &root, &mut consulted)
            .map_err(|(_, why)| why)?;
    let declaration: Value =
        serde_json::from_slice(&std::fs::read(&source.key).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut shapes = declaration["shapes"].clone();
    super::implementation::shape_imports(
        &declaration,
        source.key.parent().unwrap(),
        &source.origin,
        &mut consulted,
        contract_syntax::Span::default(),
        |name, shape| shapes[name] = shape.clone(),
    )?;
    Ok(shapes)
}

// Compiler-declared records such as DOMRect have the same field types in a package.
fn builtin_fields(name: &str) -> Option<Vec<contract_syntax::Field>> {
    use contract_syntax::{Field, Span, TypeExpr};
    use contract_types::Ty;
    fn expression(ty: &Ty, span: Span) -> TypeExpr {
        match ty {
            Ty::List(t) => TypeExpr::List(Box::new(expression(t, span)), span),
            Ty::Option(t) => TypeExpr::Option(Box::new(expression(t, span)), span),
            Ty::Enum(words) => TypeExpr::Enum(words.clone(), span),
            ty => TypeExpr::Named(ty.to_string(), span),
        }
    }
    static RECORDS: std::sync::LazyLock<std::collections::BTreeMap<String, Vec<(String, Ty)>>> =
        std::sync::LazyLock::new(|| {
            contract_types::check_declarations(&contract_syntax::File::default(), None)
                .unwrap()
                .map
        });
    let span = Span::default();
    Some(
        RECORDS
            .get(name)?
            .iter()
            .map(|(name, ty)| Field {
                name: name.clone(),
                ty: expression(ty, span),
                span,
            })
            .collect(),
    )
}

/// Check an agent word with the declaration grammar used for interface names.
/// A record whose fields belong to the implementation still takes an object.
pub(crate) fn agent_value_fits(ty: &str, value: &Value, shapes: &Value) -> Result<bool, String> {
    use contract_syntax::{Span, TypeExpr};
    // Contract sees a json<T> as a string. A package receives the held JSON value.
    fn raw(ty: &str, value: &Value, shapes: &Value) -> Result<bool, String> {
        let ty = ty.trim();
        if ty == "any" {
            return Ok(true);
        }
        if let Some(inner) = ty.strip_suffix("[]") {
            let Some(items) = value.as_array() else {
                return Ok(false);
            };
            for item in items {
                if !raw(inner, item, shapes)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        if let Some(inner) = ty.strip_prefix("option<").and_then(|s| s.strip_suffix('>')) {
            return Ok(value.is_null() || raw(inner, value, shapes)?);
        }
        if let Some(inner) = ty.strip_prefix("json<").and_then(|s| s.strip_suffix('>')) {
            return raw(inner, value, shapes);
        }
        fits(&super::type_expr(ty, Span::default())?, value, shapes)
    }
    fn fits(ty: &TypeExpr, value: &Value, shapes: &Value) -> Result<bool, String> {
        Ok(match ty {
            TypeExpr::List(item, _) => {
                let Some(items) = value.as_array() else {
                    return Ok(false);
                };
                for value in items {
                    if !fits(item, value, shapes)? {
                        return Ok(false);
                    }
                }
                true
            }
            TypeExpr::Option(item, _) => value.is_null() || fits(item, value, shapes)?,
            TypeExpr::Enum(words, _) => {
                value.as_str().is_some_and(|v| words.iter().any(|w| w == v))
            }
            TypeExpr::Named(name, span) => match name.as_str() {
                "number" => value.is_number(),
                "bool" => value.is_boolean(),
                "unit" => value.is_null(),
                "string" => value.is_string(),
                _ => {
                    let Some(object) = value.as_object() else {
                        return Ok(false);
                    };
                    let shape = &shapes[name];
                    let fields = if let Some(keys) = super::keyed(shape) {
                        super::keyed_fields(name, keys, *span)?
                    } else if shape.is_object() {
                        super::fields(super::object(shape), *span)?
                    } else if let Some(fields) = builtin_fields(name) {
                        fields
                    } else {
                        return Ok(true);
                    };
                    for field in &fields {
                        match object.get(&field.name) {
                            Some(v) => {
                                let spec = &super::keyed(shape).map_or(shape, |_| &shape["record"])
                                    [&field.name];
                                let ty = spec
                                    .as_str()
                                    .or_else(|| spec["type"].as_str())
                                    .unwrap_or("");
                                let valid = if is_json(&Value::from(ty)) {
                                    (v.is_null() && matches!(field.ty, TypeExpr::Option(..)))
                                        || raw(ty, v, shapes)?
                                } else {
                                    fits(&field.ty, v, shapes)?
                                };
                                if !valid {
                                    return Ok(false);
                                }
                            }
                            None if matches!(field.ty, TypeExpr::Option(..)) => (),
                            _ => return Ok(false),
                        }
                    }
                    object.keys().all(|k| fields.iter().any(|f| f.name == *k))
                }
            },
        })
    }
    raw(ty, value, shapes)
}
