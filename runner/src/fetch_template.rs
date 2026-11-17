//! A declared source's request, written from its row and its arguments, and
//! its reply read (LLP 1162.001 D1, D2). Pure: the runner's `fetches.rs`
//! applies them, and `host/web-js/fetches.js` repeats them for the JS target.

use exact_plan::Value;
use serde_json::Value as Json;

/// An argument as text: a string as it is, a number as JavaScript's
/// `String(n)` writes it, a bool `true` or `false`.
fn text(value: &Value) -> Option<String> {
    match value {
        Value::Number(n) => {
            let mut out = String::new();
            crate::stdlib::push_number(*n, &mut out);
            Some(out)
        }
        Value::Bool(b) => Some(if *b { "true" } else { "false" }.into()),
        v => v.as_str().map(str::to_string),
    }
}

/// RFC 6570 §3.2.2's simple string expansion: every byte of the UTF-8 text
/// but RFC 3986's unreserved characters (`A–Z a–z 0–9 - . _ ~`), as `%XX`.
pub fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(char::from(b"0123456789ABCDEF"[usize::from(b >> 4)]));
            out.push(char::from(b"0123456789ABCDEF"[usize::from(b & 15)]));
        }
    }
    out
}

/// What fills a template: a parameter's argument, or a kept secret.
pub enum Part<'a> {
    /// The parameter `name`.
    Param(&'a str),
    /// The kept secret `name` (`{secret:name}`).
    Secret(&'a str),
}

/// `template` with each `{…}` replaced by what `fill` gives for it; `None`
/// when `fill` gives nothing. The compiler checked the braces.
pub fn expand(template: &str, mut fill: impl FnMut(Part<'_>) -> Option<String>) -> Option<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}')? + open;
        let inner = &rest[open + 1..close];
        let part = match inner.strip_prefix("secret:") {
            Some(name) => Part::Secret(name),
            None => Part::Param(inner),
        };
        out.push_str(&fill(part)?);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// The argument of parameter `name` as text, by position in `names`.
pub fn argument(names: &[&str], args: &[Value], name: &str) -> Option<String> {
    let i = names.iter().position(|n| *n == name)?;
    text(args.get(i)?)
}

/// The value at RFC 6901 JSON Pointer `pointer` in `json`.
pub fn pointer<'a>(json: &'a Json, pointer: &str) -> Option<&'a Json> {
    if pointer.is_empty() {
        return Some(json);
    }
    let mut at = json;
    for token in pointer.strip_prefix('/')?.split('/') {
        let token = token.replace("~1", "/").replace("~0", "~");
        at = match at {
            Json::Object(map) => map.get(&token)?,
            // An array index is digits without a leading zero (RFC 6901 §4).
            Json::Array(items) if token == "0" || !token.starts_with('0') => {
                items.get(token.parse::<usize>().ok()?)?
            }
            _ => return None,
        };
    }
    Some(at)
}

/// A GraphQL reply's first error (GraphQL §7.1.2), as `(message, code)`:
/// its `message`, and its `extensions.code` when that is a string.
pub fn graphql_error(json: &Json) -> Option<(String, String)> {
    let first = json.get("errors")?.as_array()?.first()?;
    let message = (first.get("message").and_then(Json::as_str))
        .unwrap_or("the GraphQL operation failed")
        .to_string();
    let code = (first.pointer("/extensions/code").and_then(Json::as_str))
        .unwrap_or("")
        .to_string();
    Some((message, code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_encodes_all_but_the_unreserved() {
        assert_eq!(encode("Palo Alto & 1/2"), "Palo%20Alto%20%26%201%2F2");
        assert_eq!(encode("a-b.c_d~e"), "a-b.c_d~e");
        assert_eq!(encode("é"), "%C3%A9");
        let names = ["q", "n", "b"];
        let args = [Value::str("a b"), Value::Number(52.5), Value::Bool(true)];
        let url = expand("https://x.test/s?q={q}&n={n}&b={b}", |p| match p {
            Part::Param(name) => argument(&names, &args, name).map(|t| encode(&t)),
            Part::Secret(_) => None,
        });
        assert_eq!(
            url.as_deref(),
            Some("https://x.test/s?q=a%20b&n=52.5&b=true")
        );
        assert_eq!(expand("Bearer {secret:t}", |_| None), None);
    }

    #[test]
    fn a_pointer_reads_rfc_6901() {
        let json: Json = serde_json::from_str(r#"{"a":{"b/c":[1,{"~d":2}]},"":3}"#).unwrap();
        assert_eq!(pointer(&json, ""), Some(&json));
        assert_eq!(pointer(&json, "/a/b~1c/0"), Some(&Json::from(1)));
        assert_eq!(pointer(&json, "/a/b~1c/1/~0d"), Some(&Json::from(2)));
        assert_eq!(pointer(&json, "/"), Some(&Json::from(3)));
        assert_eq!(pointer(&json, "/a/b~1c/01"), None);
        assert_eq!(pointer(&json, "/a/missing"), None);
    }

    #[test]
    fn a_graphql_error_is_its_first() {
        let json: Json = serde_json::from_str(
            r#"{"data":null,"errors":[{"message":"no repo","extensions":{"code":"NOT_FOUND"}},{"message":"x"}]}"#,
        )
        .unwrap();
        assert_eq!(
            graphql_error(&json),
            Some(("no repo".into(), "NOT_FOUND".into()))
        );
        assert_eq!(graphql_error(&serde_json::json!({"data": 1})), None);
        assert_eq!(graphql_error(&serde_json::json!({"errors": []})), None);
    }
}
