//! Manifest requests checked against the baked grant ceiling (LLP 1162.000 D2).

use ibex2::grant::{GrantSet, Operation};
use ibex2::stdlib::{fetch, url};
use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Default)]
struct Entries(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Ordered;
        impl<'de> Visitor<'de> for Ordered {
            type Value = Entries;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object of named preload entries")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Entries, M::Error> {
                let mut entries = Vec::new();
                while let Some(name) = map.next_key::<String>()? {
                    let entry = map
                        .next_value::<Entry>()
                        .map_err(|e| serde::de::Error::custom(format!("preload.{name}.{e}")))?;
                    entries.push((name, entry.0));
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(Ordered)
    }
}

struct Entry(Value);

struct HeaderValues(Vec<(String, Value)>);

impl<'de> Deserialize<'de> for HeaderValues {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Headers;
        impl<'de> Visitor<'de> for Headers {
            type Value = HeaderValues;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object of header values")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<HeaderValues, M::Error> {
                let mut values = Vec::new();
                while let Some(pair) = map.next_entry()? {
                    values.push(pair);
                }
                Ok(HeaderValues(values))
            }
        }
        deserializer.deserialize_map(Headers)
    }
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Entry;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an object of request fields")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Entry, M::Error> {
                let mut fields = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    let value = if key == "headers" {
                        let headers = map
                            .next_value::<HeaderValues>()
                            .map_err(|e| serde::de::Error::custom(format!("headers: {e}")))?;
                        let mut names = BTreeSet::new();
                        let mut values = serde_json::Map::new();
                        for (name, value) in headers.0 {
                            if !names.insert(name.to_ascii_lowercase()) {
                                return Err(serde::de::Error::custom(format!(
                                    "headers.{name}: repeated header ignoring case"
                                )));
                            }
                            values.insert(name, value);
                        }
                        Value::Object(values)
                    } else {
                        map.next_value()?
                    };
                    fields.insert(key, value);
                }
                Ok(Entry(Value::Object(fields)))
            }
        }
        deserializer.deserialize_map(Fields)
    }
}

#[derive(serde::Deserialize)]
struct ManifestEntries {
    #[serde(default)]
    preload: Entries,
}

pub(crate) fn checked(
    app: &Path,
    manifest: &contract::Manifest,
    grants: Option<&str>,
) -> Result<Value, String> {
    if manifest.json.get("preload").is_none() {
        return Ok(Value::Null);
    }
    let path = if app.join(".shells/app.json").is_file() {
        app.join(".shells/app.json")
    } else {
        app.join("app.json")
    };
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let entries: ManifestEntries = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    check(entries.preload.0, grants)
}

fn parts(text: &str) -> Result<Vec<Value>, String> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        literal.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        if let Some(tail) = rest.strip_prefix('{') {
            literal.push('{');
            rest = tail;
            continue;
        }
        let reference = rest
            .strip_prefix("secret:")
            .ok_or("write a literal { as {{")?;
        let end = reference.find('}').ok_or("unterminated secret reference")?;
        let name = &reference[..end];
        if name.is_empty() || name.contains('{') {
            return Err("empty or invalid secret reference".into());
        }
        if !literal.is_empty() {
            parts.push(json!(["literal", std::mem::take(&mut literal)]));
        }
        parts.push(json!(["secret", name]));
        rest = &reference[end + 1..];
    }
    literal.push_str(rest);
    if !literal.is_empty() {
        parts.push(json!(["literal", literal]));
    }
    Ok(parts)
}

fn placeholder(parts: &[Value]) -> String {
    parts
        .iter()
        .map(|p| {
            if p[0] == "literal" {
                p[1].as_str().unwrap()
            } else {
                "secret"
            }
        })
        .collect()
}

fn check(entries: Vec<(String, Value)>, grants: Option<&str>) -> Result<Value, String> {
    let grants = grants
        .map(|spec| GrantSet::parse(&exact_runner::io_grants(spec)))
        .transpose()
        .map_err(|e| format!("preload: invalid grants: {e}"))?;
    let mut result = Vec::new();
    let mut names = BTreeSet::new();
    for (index, (name, value)) in entries.into_iter().enumerate() {
        let fail = |field: &str, why: &str| format!("preload.{name}.{field}: {why}");
        if index >= 8 {
            return Err(fail("name", "at most eight entries"));
        }
        if !name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !names.insert(name.clone())
        {
            return Err(fail("name", "must be unique and match ^[a-z][a-z0-9-]*$"));
        }
        let object = value
            .as_object()
            .ok_or_else(|| fail("entry", "expected an object"))?;
        for key in object.keys() {
            if !["url", "method", "headers", "body", "timeoutMs", "when"].contains(&key.as_str()) {
                return Err(fail(key, "not a known key"));
            }
        }
        let string = |field: &str| -> Result<Option<&str>, String> {
            object
                .get(field)
                .map(|v| v.as_str().ok_or_else(|| fail(field, "expected a string")))
                .transpose()
        };
        let mut secrets = BTreeSet::new();
        let mut template = |field: &str, text: &str| -> Result<Vec<Value>, String> {
            let parts = parts(text).map_err(|e| fail(field, &e))?;
            for part in parts.iter().filter(|p| p[0] == "secret") {
                let secret = part[1].as_str().unwrap();
                if grants.as_ref().is_some_and(|grants| {
                    !grants.permits(&Operation::SecretKeep {
                        name: secret.into(),
                    })
                }) {
                    return Err(fail(
                        field,
                        &format!("secret {secret} needs secret.keep {secret}"),
                    ));
                }
                secrets.insert(secret.to_owned());
            }
            Ok(parts)
        };
        let url = template(
            "url",
            string("url")?.ok_or_else(|| fail("url", "required"))?,
        )?;
        let target = placeholder(&url);
        if let Some(first) = url.iter().position(|part| part[0] == "secret") {
            let clean = |text: &str| -> String {
                text.chars()
                    .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
                    .collect()
            };
            let cleaned = clean(&target);
            let cleaned = cleaned.trim_start_matches(|c| c <= ' ');
            let prefix = clean(&placeholder(&url[..first]));
            let prefix = prefix.trim_start_matches(|c| c <= ' ');
            let authority_end = cleaned.split_once(':').map_or(cleaned.len(), |(_, tail)| {
                let authority = tail.trim_start_matches(['/', '\\']);
                cleaned.len() - authority.len()
                    + authority
                        .find(['/', '\\', '?', '#'])
                        .unwrap_or(authority.len())
            });
            if prefix.len() < authority_end {
                return Err(fail(
                    "url",
                    "secret references are allowed only in the URL path or query",
                ));
            }
        }
        let parsed = url::parse(&target, None)
            .map_err(|_| fail("url", "expected an absolute http: or https: URL"))?;
        if !["http:", "https:"].contains(&parsed.protocol.as_str()) || parsed.hostname.is_empty() {
            return Err(fail("url", "expected an absolute http: or https: URL"));
        }
        let authority = target.split_once(':').map(|(_, tail)| {
            tail.trim_start_matches(['/', '\\'])
                .split(['/', '\\', '?', '#'])
                .next()
                .unwrap_or("")
        });
        if !parsed.username.is_empty()
            || !parsed.password.is_empty()
            || authority.is_some_and(|a| a.contains('@'))
        {
            return Err(fail("url", "userinfo is not allowed"));
        }
        if target.contains('#') {
            return Err(fail("url", "a fragment is not allowed"));
        }
        let origin = fetch::origin_of(&target).map_err(|_| fail("url", "invalid origin"))?;
        if grants
            .as_ref()
            .is_some_and(|grants| !grants.permits(&Operation::Fetch { origin }))
        {
            return Err(fail("url", "origin is outside the app's net.fetch grants"));
        }
        let method = string("method")?.unwrap_or("GET");
        if !fetch::is_valid_name(method) {
            return Err(fail("method", "expected an HTTP token"));
        }
        let method = method.to_ascii_uppercase();
        let body = string("body")?
            .map(|text| {
                if ["GET", "HEAD"].contains(&method.as_str()) {
                    return Err(fail("body", "GET and HEAD cannot have a body"));
                }
                if text.len() > 64 * 1024 {
                    return Err(fail("body", "larger than 64 KiB"));
                }
                template("body", text)
            })
            .transpose()?;
        let timeout = object
            .get("timeoutMs")
            .map(|v| {
                v.as_f64()
                    .filter(|&n| {
                        n > 0.0
                            && n <= f64::from(exact_runner::request::MAX_TIMEOUT_MS)
                            && n.fract() == 0.0
                    })
                    .map(|n| n as u32)
                    .ok_or_else(|| {
                        fail(
                            "timeoutMs",
                            "expected a positive integer no greater than 3600000",
                        )
                    })
            })
            .transpose()?;
        let when = string("when")?.unwrap_or("start");
        if !["start", "foreground"].contains(&when) {
            return Err(fail("when", "expected start or foreground"));
        }
        let mut headers = Vec::new();
        let mut header_names = BTreeSet::new();
        if let Some(header) = object.get("headers") {
            for (key, value) in header
                .as_object()
                .ok_or_else(|| fail("headers", "expected an object"))?
            {
                let field = format!("headers.{key}");
                if !fetch::is_valid_name(key) {
                    return Err(fail(&field, "expected an HTTP field name"));
                }
                if !header_names.insert(key.to_ascii_lowercase()) {
                    return Err(fail(&field, "repeated header ignoring case"));
                }
                let mut guarded = fetch::Headers::default();
                guarded.set(key, "placeholder");
                if !guarded.for_request().has(key) {
                    return Err(fail(&field, "fetch may not set this header"));
                }
                let value = value
                    .as_str()
                    .ok_or_else(|| fail(&field, "expected a string"))?;
                headers.push(json!([key, template(&field, value)?]));
            }
        }
        result.push(json!({"name":name,"method":method,"url":url,"headers":headers,"body":body,"timeoutMs":timeout,"when":when,"secrets":secrets}));
    }
    Ok(Value::Array(result))
}

#[cfg(test)]
mod tests;
