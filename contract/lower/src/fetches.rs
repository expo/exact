//! Sources declared in Contract (LLP 1162.001): each `source` checked, its
//! signature made the one its calls agree with, and its request written to
//! the plan's `fetches` table for the runner to make.

use crate::LowerError;
use contract_syntax::{SourceDecl, Span};
use contract_types::Ty;
use std::collections::BTreeMap;

/// The methods a declaration may name (LLP 1162.001 D1).
const METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

/// Fetch's forbidden request-header names, which a browser drops
/// (fetch.spec.whatwg.org, "forbidden request-header"), and the prefixes.
const FORBIDDEN: [&str; 21] = [
    "accept-charset",
    "accept-encoding",
    "access-control-request-headers",
    "access-control-request-method",
    "connection",
    "content-length",
    "cookie",
    "cookie2",
    "date",
    "dnt",
    "expect",
    "host",
    "keep-alive",
    "origin",
    "referer",
    "set-cookie",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "via",
];

/// A declaration, checked: what `PlanBuilder::fetch` writes.
pub(crate) struct Fetch {
    pub method: String,
    pub url: String,
    pub origin: String,
    pub params: Vec<String>,
    pub headers: Vec<(String, String)>,
    pub body: &'static str,
    pub document: String,
    pub pointer: String,
}

fn fail(id: &'static str, message: String, span: Span) -> LowerError {
    LowerError { id, message, span }
}

/// The parameters a template may put in as text.
fn textual(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Number | Ty::Bool | Ty::Enum(_))
}

/// Each `{…}` of `template`, with its byte range, or why it has a stray brace.
fn expressions(template: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut rest = template;
    loop {
        let open = rest.find('{');
        let close = rest.find('}');
        match (open, close) {
            (None, None) => return Ok(out),
            (Some(o), Some(c)) if o < c => {
                let inner = &rest[o + 1..c];
                if inner.contains('{') {
                    return Err("a `{` opens an expression that never closes".into());
                }
                out.push(inner);
                rest = &rest[c + 1..];
            }
            (Some(_), None) => return Err("a `{` opens an expression that never closes".into()),
            _ => return Err("a `}` closes no expression".into()),
        }
    }
}

/// `scheme://host[:port]`, lowercased, the default port left out, from the
/// literal head of `url`; or why the head names no origin.
fn origin(url: &str) -> Result<String, String> {
    let head = &url[..url.find('{').unwrap_or(url.len())];
    let (scheme, rest) = head
        .split_once("://")
        .ok_or("the URL is absolute: `https://host/path`")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(format!(
            "the URL's scheme is `https` or `http`, not `{scheme}`"
        ));
    }
    // The authority ends at a path, a query or a fragment, or at the end of
    // a URL with no `{…}` at all (`https://api.example.com`).
    let end = match rest.find(['/', '?', '#']) {
        Some(end) => end,
        None if head.len() == url.len() => rest.len(),
        None => return Err("the URL's origin is written out, before any `{…}`: `https://host/{id}`, not `https://{host}/…`".into()),
    };
    let authority = &rest[..end];
    if authority.is_empty() {
        return Err("the URL names no host".into());
    }
    if authority.contains('@') {
        return Err("the URL has no user or password".into());
    }
    if authority.contains(char::is_whitespace) || authority.contains('\\') {
        return Err("the URL's host has a space or `\\`".into());
    }
    let authority = authority.to_ascii_lowercase();
    let authority = match (scheme.as_str(), authority.rsplit_once(':')) {
        ("https", Some((host, "443"))) | ("http", Some((host, "80"))) => host.to_string(),
        _ => authority,
    };
    Ok(format!("{scheme}://{authority}"))
}

/// Whether `name` is an HTTP field name (RFC 9110 §5.6.2, a `token`).
fn token(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// A kept secret's name, as `secret.keep` takes one.
fn secret_name(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Check one declaration against its parameters' types, `params`.
pub(crate) fn check(decl: &SourceDecl, params: &[Ty]) -> Result<Fetch, Vec<LowerError>> {
    let name = &decl.name;
    let mut errors = Vec::new();
    let mut seen: BTreeMap<&str, Span> = BTreeMap::new();
    let mut fetch = Fetch {
        method: String::new(),
        url: String::new(),
        origin: String::new(),
        params: decl.params.iter().map(|p| p.name.clone()).collect(),
        headers: Vec::new(),
        body: "",
        document: String::new(),
        pointer: String::new(),
    };
    let param = |p: &str| decl.params.iter().position(|q| q.name == p);
    // A `{…}` in a URL or header names a parameter of a textual type; in a
    // header it may name a kept secret.
    let template = |text: &str, secrets: bool| -> Result<(), String> {
        for e in expressions(text)? {
            if let Some(secret) = e.strip_prefix("secret:") {
                if !secrets {
                    return Err(format!("`{{{e}}}`: a kept secret goes in a header, never the URL, which the journal names"));
                }
                if !secret_name(secret) {
                    return Err(format!(
                        "`{{{e}}}`: a secret's name is 1 to 64 of `a-z 0-9 . _ -`"
                    ));
                }
                continue;
            }
            match param(e) {
                Some(i) if textual(&params[i]) => {}
                Some(i) => {
                    return Err(format!(
                        "`{{{e}}}` is a `{}`; a parameter written into a URL or a header is a `string`, `number` or `bool` (a record goes in `body json`)",
                        params[i]
                    ))
                }
                None => {
                    let names: Vec<String> =
                        decl.params.iter().map(|p| format!("`{}`", p.name)).collect();
                    return Err(format!(
                        "`{{{e}}}` names no parameter of `{name}` ({})",
                        if names.is_empty() { "it has none".into() } else { names.join(", ") }
                    ));
                }
            }
        }
        Ok(())
    };
    let mut method = None;
    for line in &decl.lines {
        let at = line.span;
        let mut bad = |why: String| errors.push(fail("source-line", why, at));
        let key = line.key.as_str();
        let one = |words: &[String]| (words.len() == 1).then(|| words[0].clone());
        if key != "header" {
            if let Some(first) = seen.insert(key, at) {
                bad(format!(
                    "`{key}` is written twice in `source {name}` (first on line {})",
                    first.line
                ));
                continue;
            }
        }
        match key {
            "url" => match one(&line.words) {
                Some(url) => {
                    if url.contains('#') {
                        bad("the URL has no fragment (`#…`): a fragment never reaches the server".into());
                    }
                    match (origin(&url), template(&url, false)) {
                        (Ok(o), Ok(())) => fetch.origin = o,
                        (Err(why), _) | (_, Err(why)) => bad(why),
                    }
                    fetch.url = url;
                }
                None => bad("`url` takes the URL in quotes: `url \"https://api.example.com/items/{id}\"`".into()),
            },
            "method" => match one(&line.words) {
                Some(m) if METHODS.contains(&m.as_str()) => method = Some(m),
                _ => bad(format!("`method` is one of {}, in capitals", METHODS.map(|m| format!("`\"{m}\"`")).join(", "))),
            },
            "header" => match line.words.as_slice() {
                [field, value] => {
                    let lower = field.to_ascii_lowercase();
                    if !token(field) {
                        bad(format!("`{field}` is not a header name"));
                    } else if FORBIDDEN.contains(&lower.as_str())
                        || lower.starts_with("proxy-")
                        || lower.starts_with("sec-")
                    {
                        bad(format!("`{field}` is a header the browser sets itself and drops from a request; leave it to the host"));
                    } else if fetch.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(field)) {
                        bad(format!("the header `{field}` is written twice"));
                    } else if let Err(why) = template(value, true) {
                        bad(why);
                    } else if value.contains(['\r', '\n', '\0']) {
                        bad(format!("the header `{field}`'s value has a line break"));
                    }
                    fetch.headers.push((field.clone(), value.clone()));
                }
                _ => bad("`header` takes a name and a value, each in quotes: `header \"Authorization\" \"Bearer {secret:api.token}\"`".into()),
            },
            "body" => match one(&line.words).as_deref() {
                Some("json") => fetch.body = "json",
                _ => bad("`body json` sends the parameters as one JSON object; it is the only body a declared source sends".into()),
            },
            "graphql" => match one(&line.words) {
                Some(document) if !document.trim().is_empty() => {
                    fetch.body = "graphql";
                    fetch.document = document;
                }
                _ => bad("`graphql` takes the operation in quotes: `graphql \"query($id: ID!) { node(id: $id) { id } }\"`".into()),
            },
            "pointer" => match one(&line.words) {
                Some(p) if pointer_ok(&p) => fetch.pointer = p,
                _ => bad("`pointer` takes a JSON Pointer in quotes (RFC 6901): `\"/data/items\"`, `\"\"` for the whole reply; `~` is written `~0` and `/` in a key `~1`".into()),
            },
            other => bad(format!(
                "`{other}` is no line of a `source`: it takes `url`, `method`, `header`, `body json`, `graphql` and `pointer`"
            )),
        }
    }
    if !seen.contains_key("url") {
        errors.push(fail(
            "source-line",
            format!("`source {name}` needs a `url` line"),
            decl.span,
        ));
    }
    if seen.contains_key("graphql") && (seen.contains_key("method") || seen.contains_key("body")) {
        errors.push(fail(
            "source-line",
            format!("`source {name}` is a GraphQL operation, a `POST` of its document and variables: it takes no `method` or `body`"),
            decl.span,
        ));
    }
    fetch.method = match (&method, fetch.body) {
        (_, "graphql") => "POST".into(),
        (Some(m), "json") if m == "GET" => {
            errors.push(fail(
                "source-line",
                format!("`source {name}` sends a body, which a `GET` cannot: name its `method`"),
                decl.span,
            ));
            m.clone()
        }
        (None, "json") => {
            errors.push(fail(
                "source-line",
                format!("`source {name}` sends a body: name its `method` (`\"POST\"`, `\"PUT\"`, `\"PATCH\"`)"),
                decl.span,
            ));
            "POST".into()
        }
        (Some(m), _) => m.clone(),
        (None, _) => "GET".into(),
    };
    if errors.is_empty() {
        Ok(fetch)
    } else {
        Err(errors)
    }
}

type Signatures = BTreeMap<String, (Vec<Ty>, Ty)>;

impl crate::Lowerer<'_> {
    /// Check every `source` of `file`, and make each one a call uses the
    /// signature in `sources`: the calls' agreed one must be the declared
    /// one (`type-source-declared`). The checked requests of the used ones,
    /// by name.
    pub(crate) fn declared_sources(
        &self,
        file: &contract_syntax::File,
        sources: &mut Signatures,
    ) -> Result<BTreeMap<String, Fetch>, Vec<LowerError>> {
        let mut errors = Vec::new();
        let mut out = BTreeMap::new();
        for decl in &file.sources {
            let name = &decl.name;
            let resolve = |t: &contract_syntax::TypeExpr| {
                self.types
                    .shapes
                    .resolve(t)
                    .map_err(|e| fail(e.id, e.message, e.span))
            };
            let params: Result<Vec<Ty>, _> = (decl.params.iter())
                .map(|p| resolve(p.ty.as_ref().expect("a source's parameter is typed")))
                .collect();
            let (params, shape) = match (params, resolve(&decl.shape)) {
                (Ok(p), Ok(s)) => (p, s),
                (Err(e), _) | (_, Err(e)) => {
                    errors.push(e);
                    continue;
                }
            };
            let owned = if exact_plan::runner_owned_source(name) {
                Some("the runner's own")
            } else if file.modules.iter().any(|m| &m.name == name) {
                Some("a native module's")
            } else {
                None
            };
            if let Some(whose) = owned {
                errors.push(fail(
                    "source-declared-taken",
                    format!(
                        "`{name}` is {whose} source; a declared source takes a name of its own"
                    ),
                    decl.span,
                ));
                continue;
            }
            let fetch = match check(decl, &params) {
                Ok(f) => f,
                Err(e) => {
                    errors.extend(e);
                    continue;
                }
            };
            let Some((used, answers)) = sources.get(name) else {
                continue;
            };
            let fits = |used: &Ty, declared: &Ty| {
                used.unify(declared).as_ref() == Some(declared)
                    || (*declared == Ty::String && matches!(used, Ty::Enum(_)))
            };
            let same = used.len() == params.len()
                && used.iter().zip(&params).all(|(u, d)| fits(u, d))
                && answers == &shape;
            if !same {
                let list = |ts: &[Ty]| {
                    ts.iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                errors.push(fail(
                    "type-source-declared",
                    format!(
                        "`{name}` is declared `({}) as shape {shape}`, and called as `({}) as shape {answers}`: a call passes the declared parameters and declares the same shape",
                        list(&params),
                        list(used)
                    ),
                    decl.span,
                ));
                continue;
            }
            sources.insert(name.clone(), (params, shape));
            out.insert(name.clone(), fetch);
        }
        if errors.is_empty() {
            Ok(out)
        } else {
            Err(errors)
        }
    }
}

/// RFC 6901: empty, or `/` and reference tokens in which `~` is `~0` or `~1`.
fn pointer_ok(p: &str) -> bool {
    (p.is_empty() || p.starts_with('/'))
        && p.match_indices('~')
            .all(|(i, _)| matches!(p.as_bytes().get(i + 1), Some(b'0' | b'1')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_origin_is_the_literal_head_of_the_url() {
        // A URL that is its origin alone ends the authority at its end.
        assert_eq!(
            origin("https://api.example.com"),
            Ok("https://api.example.com".into())
        );
        assert_eq!(
            origin("http://127.0.0.1:8080"),
            Ok("http://127.0.0.1:8080".into())
        );
        assert_eq!(
            origin("https://API.example.com/v1/{id}"),
            Ok("https://api.example.com".into())
        );
        assert_eq!(
            origin("https://api.example.com:443/x"),
            Ok("https://api.example.com".into())
        );
        assert_eq!(
            origin("http://127.0.0.1:8080/x?q={q}"),
            Ok("http://127.0.0.1:8080".into())
        );
        assert_eq!(origin("http://[::1]:80/x"), Ok("http://[::1]".into()));
        assert!(origin("https://{host}/x").is_err());
        assert!(origin("https://api.example.com{path}").is_err());
        assert!(origin("ftp://example.com/x").is_err());
        assert!(origin("https://user@example.com/x").is_err());
        assert!(origin("/relative").is_err());
    }

    #[test]
    fn braces_open_and_close_expressions() {
        assert_eq!(expressions("a{b}c{d}"), Ok(vec!["b", "d"]));
        assert!(expressions("a{b").is_err());
        assert!(expressions("a}b").is_err());
        assert!(expressions("a{{b}}").is_err());
    }

    #[test]
    fn a_pointer_is_rfc_6901s() {
        assert!(pointer_ok(""));
        assert!(pointer_ok("/data/items/0"));
        assert!(pointer_ok("/a~1b/~0c"));
        assert!(!pointer_ok("data.items"));
        assert!(!pointer_ok("/a~2"));
    }
}
