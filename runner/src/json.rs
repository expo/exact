//! JSON written by hand for the runtime's replies, and the top-level
//! scalar fields of a request (`{"op":"…"}` with a couple of flat fields)
//! read by hand: [`field_str`], [`field_num`] and [`field_bool`].

use exact_plan::{Plan, TypeKind, TypesId, Value};
use std::fmt::Write as _;
use std::sync::Arc;

/// `{"error":"…"}`.
pub fn error(message: &str) -> String {
    let mut s = String::from("{\"error\":");
    quote(message, &mut s);
    s.push('}');
    s
}

/// A plan value as JSON under its declared type: numbers, strings, booleans;
/// `null` for unit and `none`; lists; records as objects keyed by field
/// name. A value that does not match its type (never, past the runner's
/// conformance checks) falls back to the positional form.
pub fn typed_json(plan: &Plan, ty: TypesId, v: &Value, out: &mut String) {
    typed_json_by(plan, ty, v, out, number);
}

fn number(n: f64, out: &mut String) {
    let _ = write!(out, "{}", num(n));
}

/// [`num`], with `-0` kept: what a value is in JavaScript.
fn signed_number(n: f64, out: &mut String) {
    if n == 0.0 && n.is_sign_negative() {
        out.push_str("-0");
    } else {
        number(n, out);
    }
}

fn typed_json_by(
    plan: &Plan,
    ty: TypesId,
    v: &Value,
    out: &mut String,
    number: fn(f64, &mut String),
) {
    let row = plan.type_(ty);
    match (row.kind, v) {
        (_, Value::Number(n)) => number(*n, out),
        (_, Value::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
        (_, v @ exact_plan::str_value!()) => quote(v.text(), out),
        (_, Value::Unit) | (_, Value::Option(None)) => out.push_str("null"),
        (TypeKind::Option, Value::Option(Some(inner))) => match row.elem {
            Some(elem) => typed_json_by(plan, elem, inner, out, number),
            None => untyped_json_by(inner, out, number),
        },
        (_, Value::Option(Some(inner))) => untyped_json_by(inner, out, number),
        (TypeKind::List, Value::List(items)) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                match row.elem {
                    Some(elem) => typed_json_by(plan, elem, item, out, number),
                    None => untyped_json_by(item, out, number),
                }
            }
            out.push(']');
        }
        (TypeKind::Record, Value::Record(fields)) if row.fields.len as usize == fields.len() => {
            out.push('{');
            for (i, (f, value)) in row.fields.iter().zip(fields.iter()).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let field = plan.field(f);
                quote(plan.str(field.name), out);
                out.push(':');
                typed_json_by(plan, field.ty, value, out, number);
            }
            out.push('}');
        }
        (_, Value::List(_)) | (_, Value::Record(_)) => untyped_json_by(v, out, number),
    }
}

/// [`surface_args`], over the plan: a Canvas 2D engine's view of it.
pub type SurfaceArgs<'a> =
    &'a dyn Fn(&crate::SurfaceUpdate, Option<(&[Value], &[Arc<str>])>) -> (Vec<Arc<str>>, Arc<str>);

/// A surface's arguments as a TypeScript `draw` receives them (LLP 1056 D1):
/// each value's JSON under its declared type, and the whole, an array or an
/// object by the authored names. A value equal to the one `kept` pieces were
/// made from keeps its piece, so a commit serializes only what changed.
pub fn surface_args(
    plan: &Plan,
    u: &crate::SurfaceUpdate,
    kept: Option<(&[Value], &[Arc<str>])>,
) -> (Vec<Arc<str>>, Arc<str>) {
    let pieces: Vec<Arc<str>> = (u.values.iter().enumerate())
        .map(|(i, v)| match kept {
            Some((values, pieces)) if values.get(i) == Some(v) && i < pieces.len() => {
                pieces[i].clone()
            }
            _ => {
                let mut s = String::new();
                match u.types.get(i).copied().flatten() {
                    Some(ty) => typed_json_by(plan, ty, v, &mut s, signed_number),
                    None => untyped_json_by(v, &mut s, signed_number),
                }
                s.into()
            }
        })
        .collect();
    let named = !u.names.is_empty();
    let mut s = String::from(if named { "{" } else { "[" });
    for (i, piece) in pieces.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        if named {
            quote(&u.names[i], &mut s);
            s.push(':');
        }
        s.push_str(piece);
    }
    s.push(if named { '}' } else { ']' });
    (pieces, s.into())
}

/// [`untyped_json`], with `-0` kept.
pub fn untyped_signed_json(v: &Value, out: &mut String) {
    untyped_json_by(v, out, signed_number);
}

/// A plan value as JSON with no type to hand: records positional.
pub fn untyped_json(v: &Value, out: &mut String) {
    untyped_json_by(v, out, number);
}

fn untyped_json_by(v: &Value, out: &mut String, number: fn(f64, &mut String)) {
    match v {
        Value::Number(n) => number(*n, out),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        v @ exact_plan::str_value!() => quote(v.text(), out),
        Value::Unit | Value::Option(None) => out.push_str("null"),
        Value::Option(Some(inner)) => untyped_json_by(inner, out, number),
        Value::List(items) | Value::Record(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                untyped_json_by(item, out, number);
            }
            out.push(']');
        }
    }
}

/// Format a finite number as JSON, or `null`, without a temporary string.
pub fn num(n: f64) -> Num {
    Num(n)
}

/// [`num`]'s number: a piece of `exact_num::text!` for the journal's lines,
/// which skip `core::fmt`, and `Display` (the same text) for the agent's
/// replies.
pub struct Num(f64);

impl std::fmt::Display for Num {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use exact_num::Piece;
        let mut text = String::new();
        self.push_to(&mut text);
        f.write_str(&text)
    }
}

impl exact_num::Piece for Num {
    fn push_to(&self, out: &mut String) {
        let n = self.0;
        if !n.is_finite() {
            out.push_str("null");
        } else if n == n.trunc() && n.abs() < 1e15 {
            (n as i64).push_to(out);
        } else {
            exact_num::Shortest(n).push_to(out);
        }
    }
}

/// A JSON string.
pub fn quote(s: &str, out: &mut String) {
    out.push('"');
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let i = cursor;
        let byte = bytes[i];
        cursor += 1;
        if !matches!(byte, b'"' | b'\\' | 0..=0x1f) {
            continue;
        }
        // Every escape is ASCII, so both slice boundaries are UTF-8 boundaries.
        // Copy ordinary text together instead of decoding and pushing each char.
        out.push_str(&s[start..i]);
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            _ => {
                let _ = write!(out, "\\u{byte:04x}");
            }
        }
        start = i + 1;
    }
    out.push_str(&s[start..]);
    out.push('"');
}

/// The string value of a top-level `"key":"…"` field in a JSON object, with
/// JSON's escapes decoded (surrogate pairs included). `None` when the key is
/// absent at the top level, the value is not a string, or the string never
/// ends.
pub fn field_str(json: &str, key: &str) -> Option<String> {
    let rest = after_key(json, key)?;
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'u' => {
                    let mut unit = hex4(&mut chars)?;
                    if (0xD800..0xDC00).contains(&unit) {
                        // A high surrogate: the low one must follow as `\uXXXX`.
                        if chars.next()? != '\\' || chars.next()? != 'u' {
                            return None;
                        }
                        let low = hex4(&mut chars)?;
                        if !(0xDC00..0xE000).contains(&low) {
                            return None;
                        }
                        unit = 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
                    }
                    out.push(char::from_u32(unit)?);
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

fn hex4(chars: &mut std::str::Chars<'_>) -> Option<u32> {
    let hex: String = chars.by_ref().take(4).collect();
    if hex.len() != 4 {
        return None;
    }
    u32::from_str_radix(&hex, 16).ok()
}

/// The numeric value of a top-level `"key":N` field.
pub fn field_num(json: &str, key: &str) -> Option<f64> {
    let rest = after_key(json, key)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
        .unwrap_or(rest.len());
    exact_num::parse_f64(&rest[..end]).ok()
}

/// Whether a top-level `"key":true` field is set.
pub fn field_bool(json: &str, key: &str) -> bool {
    after_key(json, key).is_some_and(|rest| rest.starts_with("true"))
}

/// What follows `"key":` at the top level of a JSON object — a one-pass scan
/// that steps over string tokens and nested objects and arrays, so a key
/// inside a string value or a nested object never matches. The whole
/// parser: requests are flat.
pub(crate) fn after_key<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let b = json.as_bytes();
    let mut i = 0;
    let mut depth = 0i32;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let start = i + 1;
                let mut j = start;
                loop {
                    let c = *b.get(j)?;
                    if c == b'\\' {
                        j += 2;
                    } else if c == b'"' {
                        break;
                    } else {
                        j += 1;
                    }
                }
                let token = &json[start..j.min(b.len())];
                i = j + 1;
                if depth == 1 && token == key {
                    let rest = json.get(i..)?.trim_start();
                    if let Some(rest) = rest.strip_prefix(':') {
                        return Some(rest.trim_start());
                    }
                }
            }
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}
