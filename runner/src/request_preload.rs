//! Engine-free process-start request definitions (LLP 1162.000 D2).

use crate::{Request, Response};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Capabilities available before an engine or runner exists (LLP 1162.000 D2).
pub trait LaunchContext {
    /// A granted, unguarded kept secret as it was when the function began,
    /// or as the function last wrote it (LLP 1162.000 D2).
    fn secret(&mut self, name: &str) -> Option<String>;
    /// Persist a granted secret only if nobody else has written it since the
    /// function began or since its own last successful write (LLP 1162.000 D2).
    fn set_secret(&mut self, name: &str, value: &str) -> Result<(), String>;
    /// Durably forget a granted secret under the same version check as `set_secret`.
    fn forget_secret(&mut self, name: &str) -> Result<(), String>;
    /// Start a named preload, or return its refusal reason (LLP 1162.000 D1–D3).
    /// Refuse it if somebody else has since written a secret the function read
    /// or wrote, including when a later `secret` call returned the snapshot.
    fn preload(&mut self, request: LaunchRequest) -> Result<(), String>;
    /// Fetch synchronously, under the binary's grants, without the HTTP cache.
    fn fetch(&mut self, request: Request) -> Result<Response, String>;
}

/// A crate's optional process-start entry point, retained from the binary.
#[derive(Clone)]
pub struct Launch {
    /// The crate name, used without payload text in the journal.
    pub name: &'static str,
    /// Runs only on the host's launch thread.
    pub run: Arc<LaunchFunction>,
}
/// The engine-free entry point's signature.
pub type LaunchFunction = dyn Fn(&mut dyn LaunchContext) + Send + Sync;
impl Launch {
    /// Register a function with `env!("CARGO_PKG_NAME")` as its name.
    pub fn new(
        name: &'static str,
        run: impl Fn(&mut dyn LaunchContext) + Send + Sync + 'static,
    ) -> Self {
        Self {
            name,
            run: Arc::new(run),
        }
    }
}

/// One named request handed to a crate's launch context.
pub struct LaunchRequest {
    /// Unique among this crate's and the manifest's names.
    pub name: String,
    /// A bounded plain HTTP request; its `timeout_ms` is its deadline.
    pub request: Request,
    /// Foreground requests wait for the host's Opened event.
    pub when: PreloadWhen,
}
impl LaunchRequest {
    /// A request sent at process start by default.
    pub fn new(name: impl Into<String>, request: Request) -> Self {
        Self {
            name: name.into(),
            request,
            when: PreloadWhen::Start,
        }
    }
}

/// A string parsed at bake; secrets and launch settings are filled when the request leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreloadPart {
    /// Bytes written by the author, with escaped braces decoded.
    Literal(&'static str),
    /// The name of a kept secret.
    Secret(&'static str),
    /// A launch setting and its build value (LLP 1162.000 D2).
    Env(&'static str, &'static str),
}

/// When a host may start a launch request.
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

/// A reference that cannot safely become a request (LLP 1162.000 D2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreloadFillError {
    /// The device could not supply this secret.
    MissingSecret(&'static str),
    /// The URL is not plain HTTP, or a secret could enter its authority.
    InvalidUrl,
}

impl Preload {
    /// Fill references once per secret, checking the URL before reading any secret (LLP 1162.000 D2–D3).
    /// `parse_url` validates plain HTTP and returns the normalized scheme and authority.
    pub fn fill<T: PartialEq>(
        &self,
        mut lookup: impl FnMut(&str) -> Option<String>,
        parse_url: impl Fn(&str) -> Option<T>,
    ) -> Result<Request, PreloadFillError> {
        let placeholder = |parts: &[PreloadPart]| -> String {
            parts
                .iter()
                .map(|part| match part {
                    PreloadPart::Literal(text) => (*text).to_owned(),
                    PreloadPart::Env(name, build) => {
                        crate::launch_env::value(name).unwrap_or_else(|| (*build).into())
                    }
                    PreloadPart::Secret(_) => "secret".into(),
                })
                .collect()
        };
        let safe_url = placeholder(self.url);
        parse_url(&safe_url).ok_or(PreloadFillError::InvalidUrl)?;
        if let Some(first) = self
            .url
            .iter()
            .position(|p| matches!(p, PreloadPart::Secret(_)))
        {
            let prefix = placeholder(&self.url[..first]);
            if secret_in_origin(&safe_url, &prefix, &parse_url) {
                return Err(PreloadFillError::InvalidUrl);
            }
        }
        let secrets: BTreeMap<_, _> = self
            .secrets
            .iter()
            .map(|&name| {
                lookup(name)
                    .map(|s| (name, s))
                    .ok_or(PreloadFillError::MissingSecret(name))
            })
            .collect::<Result<_, _>>()?;
        let fill = |parts: &[PreloadPart]| -> Result<String, PreloadFillError> {
            let mut text = String::new();
            for part in parts {
                text.push_str(match part {
                    PreloadPart::Literal(text) => text,
                    PreloadPart::Env(name, build) => {
                        text.push_str(crate::launch_env::value(name).as_deref().unwrap_or(build));
                        continue;
                    }
                    PreloadPart::Secret(name) => secrets
                        .get(name)
                        .ok_or(PreloadFillError::MissingSecret(name))?,
                });
            }
            Ok(text)
        };
        let mut request = Request::get(&fill(self.url)?);
        // The app's own launch read, made ahead for it (LLP 1162.000): only
        // the app's read takes its reply, and its redirects are the app's.
        request.reach.origin = crate::CallOrigin::App;
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

/// Whether inserting a secret after `prefix` changes the parsed scheme or authority.
pub fn secret_in_origin<T: PartialEq>(
    target: &str,
    prefix: &str,
    parse_url: impl Fn(&str) -> Option<T>,
) -> bool {
    let Some(tail) = target.strip_prefix(prefix) else {
        return true;
    };
    let Some(parsed) = parse_url(target) else {
        return true;
    };
    let Some(marked) = parse_url(&format!("{prefix}exactpreloadsecret{tail}")) else {
        return true;
    };
    parsed != marked
}
