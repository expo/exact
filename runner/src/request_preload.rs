//! Engine-free process-start request definitions (LLP 1162.000 D2).

use crate::Request;
use std::collections::BTreeMap;

/// A string parsed at bake; secrets are read only when the request leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloadPart {
    /// Bytes written by the author, with escaped braces decoded.
    Literal(&'static str),
    /// The name of a kept secret.
    Secret(&'static str),
}

/// When a host may start a manifest request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloadWhen {
    /// Any process start, including a background start.
    Start,
    /// Once the host knows the launch is in the foreground.
    Foreground,
}

/// A checked manifest request, readable before any app code runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preload {
    /// The manifest entry's name.
    pub name: &'static str,
    /// HTTP method, defaulting to GET.
    pub method: &'static str,
    /// Parsed URL template.
    pub url: &'static [PreloadPart],
    /// Header names and parsed values.
    pub headers: &'static [(&'static str, &'static [PreloadPart])],
    /// Parsed UTF-8 body, if supplied.
    pub body: Option<&'static [PreloadPart]>,
    /// Whole-exchange deadline; absent keeps the host's default.
    pub timeout_ms: Option<u32>,
    /// The launch that may send this request.
    pub when: PreloadWhen,
    /// Unique secret names used by URL, headers or body, sorted by name.
    pub secrets: &'static [&'static str],
}

/// A secret the device could not supply; no request was produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingPreloadSecret(pub &'static str);

impl Preload {
    /// Fill every reference from one read of each secret, or return the missing name.
    pub fn fill(
        &self,
        mut lookup: impl FnMut(&str) -> Option<String>,
    ) -> Result<Request, MissingPreloadSecret> {
        let secrets: BTreeMap<_, _> = self
            .secrets
            .iter()
            .map(|&name| {
                lookup(name)
                    .map(|s| (name, s))
                    .ok_or(MissingPreloadSecret(name))
            })
            .collect::<Result<_, _>>()?;
        let fill = |parts: &[PreloadPart]| -> Result<String, MissingPreloadSecret> {
            let mut text = String::new();
            for part in parts {
                text.push_str(match part {
                    PreloadPart::Literal(text) => text,
                    PreloadPart::Secret(name) => {
                        secrets.get(name).ok_or(MissingPreloadSecret(name))?
                    }
                });
            }
            Ok(text)
        };
        let mut request = Request::get(&fill(self.url)?);
        request.method = self.method.into();
        request.headers = self
            .headers
            .iter()
            .map(|(name, parts)| Ok(((*name).into(), fill(parts)?)))
            .collect::<Result<_, _>>()?;
        request.body = self
            .body
            .map(fill)
            .transpose()?
            .unwrap_or_default()
            .into_bytes();
        request.timeout_ms = self.timeout_ms;
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DEFINITION: Preload = Preload {
        name: "feed",
        method: "POST",
        url: &[
            PreloadPart::Literal("https://api.example/"),
            PreloadPart::Secret("session"),
        ],
        headers: &[(
            "Authorization",
            &[
                PreloadPart::Literal("Bearer "),
                PreloadPart::Secret("session"),
            ],
        )],
        body: Some(&[
            PreloadPart::Secret("session"),
            PreloadPart::Literal("{"),
            PreloadPart::Secret("other"),
        ]),
        timeout_ms: Some(42),
        when: PreloadWhen::Foreground,
        secrets: &["other", "session"],
    };

    #[test]
    fn filling_replaces_every_reference_from_one_read_per_secret() {
        let mut reads = Vec::new();
        let request = DEFINITION
            .fill(|name| {
                reads.push(name.to_owned());
                Some(format!("value-{name}"))
            })
            .unwrap();
        assert_eq!(reads, ["other", "session"]);
        assert_eq!(request.url, "https://api.example/value-session");
        assert_eq!(
            request.headers,
            [("Authorization".into(), "Bearer value-session".into())]
        );
        assert_eq!(request.body, b"value-session{value-other");
        assert_eq!(request.method, "POST");
        assert_eq!(request.timeout_ms, Some(42));
    }

    #[test]
    fn a_missing_secret_returns_its_name_and_no_request() {
        assert_eq!(
            DEFINITION.fill(|name| (name == "other").then(|| "yes".into())),
            Err(MissingPreloadSecret("session"))
        );
    }

    #[test]
    fn a_literal_definition_never_reads_secrets() {
        let definition = Preload {
            url: &[PreloadPart::Literal("https://api.example/")],
            headers: &[],
            body: None,
            secrets: &[],
            ..DEFINITION
        };
        assert_eq!(
            definition
                .fill(|_| panic!("no secret reference"))
                .unwrap()
                .url,
            "https://api.example/"
        );
    }
}
