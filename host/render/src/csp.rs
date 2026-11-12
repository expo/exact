//! The Content-Security-Policy of the pages the render server serves (LLP
//! 1048.000 D11), the boundary there, since the web runtime's admission is
//! parity (LLP 1133.036 D5), and which the app may widen.
use std::fmt::Write as _;
use std::path::Path;

/// Exact's policy (D11): scripts only from the app's origin or by hash, the
/// wasm, and fetches and sockets to the origins the app's grants name.
pub(crate) fn csp(grants: &str, dist: &Path) -> String {
    let mut connect = String::from("'self'");
    for line in grants.lines() {
        let mut words = line.split_whitespace();
        let Some(capability @ ("net.fetch" | "net.websocket")) = words.next() else {
            continue;
        };
        if let Some(target) = words.next() {
            // An origin is scheme://host[:port]: the target up to its path.
            // A scheme-wide line (`ws:`) is already a CSP scheme source.
            // `local-network` (LLP 1133.036 D5): CSP has no source for an
            // address range, and a scheme source would admit the internet,
            // so a served page reaches `.local` names alone by it.
            let origin = match target.strip_suffix(':').map(str::to_ascii_lowercase) {
                _ if target == "local-network" && capability == "net.fetch" => {
                    Some("http://*.local:* https://*.local:*".to_owned())
                }
                _ if target == "local-network" => Some("ws://*.local:* wss://*.local:*".to_owned()),
                Some(s) if matches!(&*s, "http" | "https" | "ws" | "wss") => Some(s + ":"),
                _ => target.split_once("://").map(|(scheme, rest)| {
                    format!("{scheme}://{}", rest.split('/').next().unwrap_or(""))
                }),
            };
            if let Some(origin) = origin {
                connect.push(' ');
                connect.push_str(&origin);
            }
        }
    }
    // The admitted TypeScript module and its host prelude run in a private
    // same-origin realm. Admit their exact baked bytes, never arbitrary inline JS.
    // A document's one inline script, the host's capture script, likewise.
    use sha2::{Digest, Sha256};
    let hash = |bytes: &[u8]| exact_data::envelope::base64(&Sha256::digest(bytes));
    // The JavaScript runtime's capture script too (LLP 1071): its pages carry
    // that one; and the script a boot document's page removes it with (LLP
    // 1048.005).
    let mut scripts = format!(
        "'self' 'wasm-unsafe-eval' 'sha256-{}' 'sha256-{}' 'sha256-{}' 'sha256-{}'",
        hash(crate::page::capture().as_bytes()),
        hash(crate::page::capture_js().as_bytes()),
        hash(crate::page::scroll_document_js().as_bytes()),
        hash(crate::direct::boot_swap_js().as_bytes())
    );
    for file in ["module-binary.js", "module-prelude.js", "app.js"] {
        if let Ok(bytes) = std::fs::read(dist.join(file)) {
            let _ = write!(scripts, " 'sha256-{}'", hash(&bytes));
        }
    }
    format!(
        "default-src 'self'; script-src {scripts}; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; media-src 'self' blob: https:; font-src 'self' data:; connect-src {connect}; worker-src 'self' blob:; frame-src 'self' https:; object-src 'none'; base-uri 'self'; frame-ancestors 'self'"
    )
}

/// The CSP header lines a page carries (D11): Exact's policy with the shell's
/// `host.web.head` scripts admitted (D3), then the app's part of it from
/// `.gen/csp.json`, which the web build writes from `app.json`'s
/// `host.web.csp` and `host.web.cspReportOnly`. `origin` is `--origin`: a
/// script `src` there is `'self'`'s already.
pub(crate) fn headers(
    grants: &str,
    dist: &Path,
    shell: &str,
    origin: Option<&str>,
) -> Result<String, String> {
    let mut exact = directives(&csp(grants, dist));
    let mut scripts = String::new();
    use sha2::{Digest, Sha256};
    for script in crate::page::head_scripts(shell) {
        if let Some(src) = &script.src {
            if let Some(from) = script_origin(src, origin) {
                scripts.push(' ');
                scripts.push_str(&from);
            }
        } else if runs(script.kind.as_deref()) {
            // As the browser hashes it: after HTML's input stream turns a CR
            // LF pair and a lone CR into LF (HTML, 13.2.3.5).
            let text = script.text.replace("\r\n", "\n").replace('\r', "\n");
            let hash = exact_data::envelope::base64(&Sha256::digest(text.as_bytes()));
            let _ = write!(scripts, " 'sha256-{hash}'");
        }
    }
    if let Some((_, value)) = exact.iter_mut().find(|(name, _)| name == "script-src") {
        value.push_str(&scripts);
    }
    let app: serde_json::Value = match std::fs::read_to_string(dist.join(".gen/csp.json")) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!(".gen/csp.json: {e}"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::Value::Null,
        Err(e) => return Err(format!(".gen/csp.json: {e}")),
    };
    if !app.is_object() && !app.is_null() {
        return Err(".gen/csp.json: not an object".into());
    }
    let mut out = String::new();
    for (key, header, default) in [
        ("csp", "Content-Security-Policy", true),
        (
            "cspReportOnly",
            "Content-Security-Policy-Report-Only",
            false,
        ),
    ] {
        let policy = match app.get(key) {
            None | Some(serde_json::Value::Null) if default => Some(exact.clone()),
            None | Some(serde_json::Value::Null) | Some(serde_json::Value::Bool(false)) => None,
            Some(serde_json::Value::Object(said)) => {
                Some(widened(&exact, said).map_err(|e| format!("host.web.{key}: {e}"))?)
            }
            Some(other) => {
                return Err(format!(
                    "host.web.{key}: {other} is not false or an object of directives"
                ))
            }
        };
        if let Some(policy) = policy {
            let value = policy
                .iter()
                .map(|(name, value)| format!("{name} {value}").trim_end().to_string())
                .collect::<Vec<_>>()
                .join("; ");
            let _ = write!(out, "{header}: {value}\r\n");
        }
    }
    Ok(out)
}

/// A policy's directives, in order, each a name and its value.
fn directives(policy: &str) -> Vec<(String, String)> {
    policy
        .split(';')
        .filter_map(|directive| {
            let directive = directive.trim();
            let (name, value) = directive.split_once(' ').unwrap_or((directive, ""));
            (!name.is_empty()).then(|| (name.to_string(), value.trim().to_string()))
        })
        .collect()
}

/// Whether CSP governs an inline script of this `type` (HTML, "prepare the
/// script element"): a classic script, a module, an import map or
/// speculation rules. Any other type is a data block, which never runs.
fn runs(kind: Option<&str>) -> bool {
    let Some(kind) = kind.map(|kind| kind.trim().to_ascii_lowercase()) else {
        return true;
    };
    const JAVASCRIPT: [&str; 16] = [
        "application/ecmascript",
        "application/javascript",
        "application/x-ecmascript",
        "application/x-javascript",
        "text/ecmascript",
        "text/javascript",
        "text/javascript1.0",
        "text/javascript1.1",
        "text/javascript1.2",
        "text/javascript1.3",
        "text/javascript1.4",
        "text/javascript1.5",
        "text/jscript",
        "text/livescript",
        "text/x-ecmascript",
        "text/x-javascript",
    ];
    kind.is_empty()
        || JAVASCRIPT.contains(&kind.as_str())
        || matches!(kind.as_str(), "module" | "importmap" | "speculationrules")
}

/// A script `src`'s origin as a CSP host source, or `None` when it is the
/// page's own (`origin`, or a path on it). The origin, not the path: a script
/// that adds another from its own origin would otherwise need the app's
/// policy, and CSP ignores a path after a redirect. A protocol-relative `src`
/// takes the page's scheme, so its source names none: a host source without
/// one matches the page's scheme and `https` (CSP3, "Does url match
/// expression in origin with redirect count?").
fn script_origin(src: &str, origin: Option<&str>) -> Option<String> {
    let base = url::Url::parse(origin.unwrap_or("https://exact.invalid/")).ok()?;
    let url = base.join(src.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.origin() == base.origin() {
        return None;
    }
    if src.trim_start().starts_with("//") {
        let host = url.host_str()?;
        return Some(
            url.port()
                .map_or(host.to_string(), |port| format!("{host}:{port}")),
        );
    }
    Some(url.origin().ascii_serialization())
}

/// The directives a fetch directive Exact does not write falls back to, in
/// order (CSP3, "Get the effective directive for request" and its fallback
/// list); `None` for a directive with no fallback.
fn fallback(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "script-src-elem" | "script-src-attr" => &["script-src", "default-src"],
        "style-src-elem" | "style-src-attr" => &["style-src", "default-src"],
        "worker-src" => &["child-src", "script-src", "default-src"],
        "frame-src" => &["child-src", "default-src"],
        "child-src" | "connect-src" | "font-src" | "img-src" | "manifest-src" | "media-src"
        | "object-src" | "script-src" | "style-src" => &["default-src"],
        _ => return None,
    })
}

/// `exact` with the app's directives (D11): a list adds its sources after
/// Exact's, or after what a directive Exact does not write fell back to; a
/// string replaces the value, its word `exact` standing for those same
/// sources. A directive Exact does not write goes after Exact's.
fn widened(
    exact: &[(String, String)],
    said: &serde_json::Map<String, serde_json::Value>,
) -> Result<Vec<(String, String)>, String> {
    let mut policy = exact.to_vec();
    let value_of = |name: &str| {
        exact
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.clone())
    };
    for (name, value) in said {
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(format!("{name:?} is not a directive name in lower case"));
        }
        let ours = value_of(name)
            .or_else(|| fallback(name)?.iter().find_map(|f| value_of(f)))
            .unwrap_or_default();
        let written = match value {
            serde_json::Value::Array(items) => {
                let mut sources = ours.clone();
                for item in items {
                    let Some(source) = item
                        .as_str()
                        .filter(|s| !s.is_empty() && !s.bytes().any(|b| b.is_ascii_whitespace()))
                    else {
                        return Err(format!("{name}: {item} is not one source"));
                    };
                    sources.push(' ');
                    sources.push_str(source);
                }
                sources.trim().to_string()
            }
            serde_json::Value::String(text) => text
                .split_ascii_whitespace()
                .map(|word| if word == "exact" { ours.as_str() } else { word })
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" "),
            other => {
                return Err(format!(
                    "{name}: {other} is not a list of sources or a string"
                ))
            }
        };
        // CSP3's `directive-value` (§2.3): visible ASCII but `,` (a second
        // policy) and `;` (a second directive), and the spaces between sources.
        if !written
            .bytes()
            .all(|b| matches!(b, b'\t' | b' ' | 0x21..=0x2B | 0x2D..=0x3A | 0x3C..=0x7E))
        {
            return Err(format!(
                "{name}: {written:?} holds a comma, a semicolon, a line break or a character outside ASCII"
            ));
        }
        match policy.iter_mut().find(|(n, _)| n == name) {
            Some((_, slot)) => *slot = written,
            None => policy.push((name.clone(), written)),
        }
    }
    Ok(policy)
}
