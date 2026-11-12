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
}
