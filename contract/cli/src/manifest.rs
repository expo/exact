//! The app manifest (`app.json`, LLP 1030 D2; 1030.000 D7): the identity and
//! policy every platform derives from, read and validated here because the
//! compiler checks a plan's app identity against it. The compatibility id
//! and the embedded receipt that also read it are the bake's (`exact-bake`).

use std::path::Path;

mod widgets;
pub use widgets::{Choices, Configuration, Parameter, Widget};
mod activities;
pub use activities::{Activity, AndroidActivity};
mod controls;
pub use controls::{Control, ControlAction};
mod watch;
pub use watch::{WatchEntry, WatchSync, WATCH_KIND};
mod variant;

/// The unsuffixed id of the selected variant whose id is `id` (LLP 1030.008
/// D2), for a check that admits a data source naming its product.
pub fn selected_base(id: &str) -> Option<String> {
    variant::base_of(id)
}

/// The app manifest (`app.json`, LLP 1030 D2; 1030.000 D7) as the bake
/// reads it — the JSON, and the identity every platform derives from it.
/// An app without one gets the defaults `scripts/app.mjs` gives it.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// The manifest's JSON (an object; empty when the app declares none).
    pub json: serde_json::Value,
    /// The app identity, reverse-DNS (`app.id`).
    pub id: String,
    /// The name people see (`app.name`).
    pub name: String,
    /// Whether the app has an `app.json` at all.
    pub declared: bool,
}

impl Manifest {
    /// The unsuffixed `app.id` when a variant is selected (LLP 1030.008 D2).
    pub fn variant_base(&self) -> Option<&str> {
        self.json
            .pointer("/variant/base")
            .and_then(serde_json::Value::as_str)
    }
    /// Resolve the replacement executor baked into a platform/environment.
    /// Matches `scripts/app.mjs::rustPolicy` (LLP 1029.000 §2): global,
    /// environment, platform, then platform environment; true means auto.
    pub fn rust_mode(&self, platform: &str, development: bool) -> Result<&'static str, String> {
        if !matches!(
            platform,
            "web" | "ios" | "macos" | "linux" | "android" | "windows"
        ) {
            return Err(format!("unknown Rust replacement platform: {platform}"));
        }
        let policy = self.json.get("rust");
        if let Some(value) = policy {
            validate_rust_policy(value, 2)?;
        }
        let environment = if development { "dev" } else { "prod" };
        let surface = policy
            .and_then(|p| p.get("platforms"))
            .and_then(|p| p.get(platform));
        let mut mode = "auto";
        for value in [
            policy,
            policy.and_then(|p| p.get(environment)),
            surface,
            surface.and_then(|p| p.get(environment)),
        ]
        .into_iter()
        .flatten()
        {
            mode = match value {
                serde_json::Value::Bool(true) => "auto",
                serde_json::Value::Bool(false) => "off",
                serde_json::Value::String(s) => s,
                serde_json::Value::Object(o) => {
                    o.get("mode").and_then(|v| v.as_str()).unwrap_or(mode)
                }
                _ => unreachable!("policy validated above"),
            };
        }
        match (mode, platform) {
            ("off", _) => Ok("off"),
            ("native" | "tiered", "web" | "ios") => Err(format!(
                "rust: {mode} replacement is unavailable on {platform}; use wasm or off"
            )),
            (_, "web") => Ok("browser"),
            ("tiered", _) => Ok("tiered"),
            ("wasm", _) | ("auto", "ios") => Ok("wasm"),
            ("auto", "android") if !development => Ok("wasm"),
            _ => Ok("native"),
        }
    }

    /// Where a language's module runs on `platform` (LLP 1027.002 D1, §6):
    /// `typescript.placement` or `rust.placement`, overridden by
    /// `platforms.<platform>.placement`; `main` unless declared. The
    /// environment (`EXACT_TYPESCRIPT_PLACEMENT`, `EXACT_RUST_PLACEMENT`)
    /// overrides a bake for a measurement. The compatibility id carries the
    /// result, so a changed placement is a rebuild, never an update.
    pub fn placement(&self, language: &str, platform: &str) -> Result<&'static str, String> {
        let variable = match language {
            "typescript" => "EXACT_TYPESCRIPT_PLACEMENT",
            "rust" => "EXACT_RUST_PLACEMENT",
            other => return Err(format!("unknown placement language: {other}")),
        };
        let name = |value: &serde_json::Value, at: &str| -> Result<&'static str, String> {
            match value.as_str() {
                Some("main") => Ok("main"),
                Some("worker") => Ok("worker"),
                _ => Err(format!("{at} must be \"main\" or \"worker\"")),
            }
        };
        let section = self.json.get(language);
        if language == "typescript" {
            if let Some(section) = section {
                validate_placement_policy(section)?;
            }
        }
        let mut placement = "main";
        if let Some(value) = section.and_then(|s| s.get("placement")) {
            placement = name(value, &format!("{language}.placement"))?;
        }
        if let Some(value) = section
            .and_then(|s| s.get("platforms"))
            .and_then(|p| p.get(platform))
            .and_then(|p| p.get("placement"))
        {
            placement = name(value, &format!("{language}.platforms.{platform}.placement"))?;
        }
        match std::env::var(variable) {
            Ok(value) => placement = name(&serde_json::Value::String(value), variable)?,
            Err(std::env::VarError::NotPresent) => {}
            Err(_) => return Err(format!("{variable} is not UTF-8")),
        }
        Ok(placement)
    }

    /// `app.json` in `app_dir`, or the derived defaults: `com.exact.<name>`
    /// and the directory's name capitalized.
    pub fn read(app_dir: &Path) -> Result<Manifest, String> {
        let selected = variant::selected(app_dir)?;
        Self::read_selected(app_dir, selected.as_deref())
    }

    fn read_selected(app_dir: &Path, selected: Option<&str>) -> Result<Manifest, String> {
        let name = app_dir
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .or_else(|| {
                app_dir
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "app".into());
        let display = {
            let mut c = name.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        };
        // Game bakes resolve authored defaults once; the compiler reads that same dialect.
        let resolved = app_dir.join(".shells/app.json");
        let path = if resolved.is_file() {
            resolved
        } else {
            app_dir.join("app.json")
        };
        if !path.exists() {
            if let Some(name) = selected {
                variant::resolve(&mut serde_json::json!({}), name)?;
            }
            return Ok(Manifest {
                json: serde_json::json!({}),
                id: format!("com.exact.{name}"),
                name: display,
                declared: false,
            });
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut json: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        if let Some(name) = selected {
            variant::resolve(&mut json, name).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        canonicalize_document_handlers(&mut json).map_err(|problem| {
            format!(
                "{} does not conform to scripts/app.schema.json:\n  {problem}",
                path.display()
            )
        })?;
        let app = json.get("app").and_then(|a| a.as_object());
        let id = app
            .and_then(|a| a.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{}: `app.id` is required", path.display()))?
            .to_string();
        let name = app
            .and_then(|a| a.get("name"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{}: `app.name` is required", path.display()))?
            .to_string();
        let manifest = Manifest {
            json,
            id,
            name,
            declared: true,
        };
        manifest.widgets()?;
        manifest.live_activities()?;
        Ok(manifest)
    }

    /// Whether a game's generated `.shells/app.json`, which [`Manifest::read`]
    /// prefers, was resolved from another `app.json` than `app_dir` holds now.
    /// Contract preparation and the bake record the authored file's digest beside it
    /// (`game/app/shells.mjs`); a missing record counts as another.
    pub fn generated_stale(app_dir: &Path) -> bool {
        use sha2::{Digest, Sha256};
        if !app_dir.join(".shells/app.json").is_file() {
            return false;
        }
        let authored = std::fs::read(app_dir.join("app.json")).map_or_else(
            |_| "none".to_owned(),
            |bytes| format!("{:x}", Sha256::digest(bytes)),
        );
        std::fs::read_to_string(app_dir.join(".shells/app-source.sha256"))
            .map_or(true, |recorded| recorded.trim() != authored)
    }

    /// `host.<platform>` as an object (empty when absent).
    pub fn host(&self, platform: &str) -> serde_json::Map<String, serde_json::Value> {
        self.json
            .get("host")
            .and_then(|h| h.get(platform))
            .and_then(|p| p.as_object())
            .cloned()
            .unwrap_or_default()
    }

    /// The painter explicitly selected for this platform (LLP 1149 D12).
    pub fn painter(&self, platform: &str) -> Result<Option<&'static str>, String> {
        match self.host(platform).get("painter") {
            None => Ok(None),
            Some(value) if platform == "windows" && value.as_str() == Some("cpu") => {
                Ok(Some("cpu"))
            }
            Some(value)
                if matches!(platform, "windows" | "linux" | "android")
                    && value.as_str() == Some("ink") =>
            {
                Ok(Some("ink"))
            }
            Some(_) => Err(format!(
                "host.{platform}.painter must be \"ink\" on Windows, Linux or Android, or \"cpu\" on Windows"
            )),
        }
    }

    /// `host.windows.window` as the Rust expression of the
    /// `exact_windows::Window` the app opens in: the fields it names, the
    /// host's defaults for the rest (no `..` when it names all five, which
    /// Clippy's `needless_update` would flag in the generated entry).
    pub fn windows_window(&self) -> Result<String, String> {
        let window = self.host("windows");
        let Some(window) = window.get("window") else {
            return Ok("exact_windows::Window::default()".into());
        };
        let mut fields = String::new();
        let mut named = 0;
        for (key, field) in [
            ("width", "width"),
            ("height", "height"),
            ("minWidth", "min_width"),
            ("minHeight", "min_height"),
        ] {
            if let Some(value) = window.get(key) {
                let n = value
                    .as_f64()
                    .filter(|n| (1.0..=16384.0).contains(n))
                    .ok_or_else(|| {
                        format!("host.windows.window.{key} is a number from 1 to 16384")
                    })?;
                fields.push_str(&format!("{field}: {n:?}_f32, "));
                named += 1;
            }
        }
        if let Some(value) = window.get("keepRunning") {
            let keep = value
                .as_bool()
                .ok_or("host.windows.window.keepRunning is true or false")?;
            fields.push_str(&format!("keep_running: {keep}, "));
            named += 1;
        }
        let rest = if named < 5 {
            "..exact_windows::Window::default() "
        } else {
            ""
        };
        Ok(format!("exact_windows::Window {{ {fields}{rest}}}"))
    }

    /// `deploy.signing.keys`: canonical base64 of 32 raw Ed25519 bytes.
    /// Missing keys remain absent; malformed configured keys are refused.
    pub fn keys(&self) -> Result<serde_json::Value, String> {
        let value = self.json.pointer("/deploy/signing/keys");
        let keys = match value {
            Some(serde_json::Value::Object(keys)) => keys,
            None | Some(serde_json::Value::Null) => return Ok(serde_json::Value::Null),
            _ => return Err("deploy.signing.keys must be an object".into()),
        };
        const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        for (id, value) in keys {
            let valid = value.as_str().is_some_and(|key| {
                key.len() == 44
                    && key.ends_with('=')
                    && key
                        .bytes()
                        .take(43)
                        .all(|b| ALPHABET.as_bytes().contains(&b))
                    && ALPHABET
                        .find(key.as_bytes()[42] as char)
                        .is_some_and(|n| n % 4 == 0)
            });
            if id.is_empty() || !valid {
                return Err(format!(
                    "deploy.signing.keys[{id:?}] must name a base64 Ed25519 public key (32 bytes)"
                ));
            }
        }
        Ok(serde_json::Value::Object(keys.clone()))
    }

    /// The channel this build bakes in (`deploy.channel`; else the only key
    /// of `deploy.channels`; else `prod`) and its origin (`deploy.channels.
    /// <channel>`, else `app.origin`): where the binary's update store checks
    /// (LLP 1030.000 D4). A stream is `(channel, compatibility id)`; the
    /// channel is not part of the id. The origin `"appdrop"` is AppDrop's
    /// origin for the signing key (`appdrop_origin`).
    pub fn channel(&self) -> Result<(String, Option<String>), String> {
        let deploy = self.json.get("deploy");
        let channels = deploy
            .and_then(|d| d.get("channels"))
            .and_then(|c| c.as_object());
        let channel = deploy
            .and_then(|d| d.get("channel"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| match channels {
                Some(c) if c.len() == 1 => c.keys().next().cloned(),
                _ => None,
            })
            .unwrap_or_else(|| "prod".to_string());
        let origin = channels
            .and_then(|c| c.get(&channel))
            .and_then(|v| v.as_str())
            .or_else(|| {
                self.json
                    .get("app")
                    .and_then(|a| a.get("origin"))
                    .and_then(|v| v.as_str())
            })
            .map(str::to_string);
        let origin = match origin.as_deref() {
            Some("appdrop") => Some(self.appdrop_origin()?),
            _ => origin,
        };
        Ok((channel, origin))
    }

    /// AppDrop's update origin for this app: `<base>/u/<key id>`, the key id
    /// being the first 32 hex digits of the sha256 of the raw public key
    /// `deploy.signing.key` names (or the only one in `deploy.signing.keys`).
    /// Only that key's holder can publish there, and no account is needed.
    /// The base is `https://appdrop.app` unless `EXACT_APPDROP_ORIGIN` names
    /// another (a local AppDrop, for testing). Matches `scripts/origin.mjs`.
    pub fn appdrop_origin(&self) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let keys = self.keys()?;
        let keys = keys.as_object().filter(|keys| !keys.is_empty()).ok_or(
            "the origin \"appdrop\" is named after the update key: add deploy.signing.keys",
        )?;
        let named = self
            .json
            .pointer("/deploy/signing/key")
            .and_then(|v| v.as_str());
        let key = match named {
            Some(id) => keys.get(id).ok_or_else(|| {
                format!("deploy.signing.key names {id:?}, which deploy.signing.keys lacks")
            })?,
            None if keys.len() == 1 => keys.values().next().unwrap(),
            None => {
                return Err(
                    "the origin \"appdrop\" needs deploy.signing.key to pick one of deploy.signing.keys"
                        .into(),
                )
            }
        };
        let raw = base64_decode(key.as_str().unwrap_or_default())
            .ok_or("deploy.signing.keys must be base64")?;
        let digest = Sha256::digest(&raw);
        let id: String = digest[..16].iter().map(|b| format!("{b:02x}")).collect();
        let base =
            std::env::var("EXACT_APPDROP_ORIGIN").unwrap_or_else(|_| "https://appdrop.app".into());
        Ok(format!("{}/u/{id}", base.trim_end_matches('/')))
    }

    /// `deploy.activate` (LLP 1030.000 D4): when a staged bundle applies —
    /// `next-launch` (the default; `deliveryActivate` applies it sooner),
    /// `app-decides` (only `deliveryActivate` does) or `immediate` (as soon
    /// as a check stages it). Policy, not identity:
    /// it rides beside the id, never in it.
    pub fn activate(&self) -> String {
        self.json
            .get("deploy")
            .and_then(|d| d.get("activate"))
            .and_then(|v| v.as_str())
            .unwrap_or("next-launch")
            .to_string()
    }

    /// `rust.data` (LLP 1153.003 D2): the data crate the host crates link, or `None` when the manifest names none.
    pub fn rust_data(&self) -> Result<Option<RustData>, String> {
        match self.json.pointer("/rust/data") {
            None => Ok(None),
            Some(value) => RustData::from_json(value).map(Some),
        }
    }

    /// `deploy.store.<platform>`: `"A"` (an update store) or `"0"` (none);
    /// Native defaults to `"A"`; the web host links no updater (LLP 1030 D4).
    pub fn store(&self, platform: &str) -> String {
        self.json
            .get("deploy")
            .and_then(|d| d.get("store"))
            .and_then(|s| s.get(platform))
            .and_then(|v| v.as_str())
            .unwrap_or(if platform == "web" { "0" } else { "A" })
            .to_string()
    }
}

/// `rust.data` (LLP 1153.003 D2): the crate that answers the app's sources, its `DataSource` type and the
/// constructor the hosts and the web's module run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustData {
    /// The data crate's Cargo package (`pinapp-data`).
    pub krate: String,
    /// The source type, by its path from the crate's library (`pinapp_data::Pins`).
    pub ty: String,
    /// The constructor: `source`, or `<type>::default()` when the manifest names none.
    pub source: String,
}

impl RustData {
    fn from_json(value: &serde_json::Value) -> Result<RustData, String> {
        let object = value.as_object().ok_or("rust.data must be an object")?;
        if let Some(key) = object
            .keys()
            .find(|k| !matches!(k.as_str(), "crate" | "type" | "source"))
        {
            return Err(format!("rust.data takes crate, type and source, not {key}"));
        }
        let text = |key: &str| {
            object.get(key).map(|v| {
                v.as_str()
                    .ok_or(format!("rust.data.{key} must be a string"))
            })
        };
        let krate = text("crate")
            .ok_or("rust.data requires the data crate's Cargo package in `crate`")??;
        if krate.is_empty()
            || !krate
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err(format!(
                "rust.data.crate is not a Cargo package name: {krate}"
            ));
        }
        let ty = text("type").ok_or("rust.data requires the source type's path in `type`")??;
        let segments: Vec<&str> = ty.split("::").collect();
        if segments.len() < 2 || !segments.iter().all(|s| identifier(s)) {
            return Err(format!(
                "rust.data.type is a path from the data crate's library, as `pinapp_data::Pins`: {ty}"
            ));
        }
        let source = match text("source").transpose()? {
            None => format!("{ty}::default()"),
            Some(source) if constructs(ty, source) => source.to_string(),
            Some(source) => {
                return Err(format!(
                    "rust.data.source is `{ty}`, `{ty}::default()` or `{ty}::new(…)` with literal arguments, not {source}; a source built from anything else is a module of its own (rust.module.package)"
                ))
            }
        };
        Ok(RustData {
            krate: krate.to_string(),
            ty: ty.to_string(),
            source,
        })
    }
}

fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `source` constructs `ty` in the grammar the web build copies into a module of its own: the type itself
/// (a unit struct), its `default()`, or its `new(…)` with literal arguments (`host/web-js/module.mjs`).
fn constructs(ty: &str, source: &str) -> bool {
    let Some(rest) = source.strip_prefix(ty) else {
        return false;
    };
    if rest.is_empty() || rest == "::default()" {
        return true;
    }
    let Some(args) = rest
        .strip_prefix("::new(")
        .and_then(|r| r.strip_suffix(')'))
    else {
        return false;
    };
    literal_arguments(args)
}

/// Comma-separated literals: numbers (with an optional suffix), strings, characters, `true` and `false`.
fn literal_arguments(args: &str) -> bool {
    let mut items = Vec::new();
    let (mut start, mut quote, mut escaped) = (0, None, false);
    for (i, c) in args.char_indices() {
        match quote {
            Some(_) if escaped => escaped = false,
            Some(_) if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == ',' => {
                items.push(&args[start..i]);
                start = i + 1;
            }
            None => {}
        }
    }
    if quote.is_some() {
        return false;
    }
    items.push(&args[start..]);
    if items.len() == 1 && items[0].trim().is_empty() {
        return true;
    }
    items.iter().map(|item| item.trim()).all(|item| {
        matches!(item, "true" | "false")
            || (item.len() >= 2
                && (item.starts_with('"') && item.ends_with('"')
                    || item.starts_with('\'') && item.ends_with('\'')))
            || item
                .trim_start_matches('-')
                .starts_with(|c: char| c.is_ascii_digit())
                && item
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '+' | '-'))
    })
}

/// Standard base64 with padding, as `keys` validated it.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = Vec::new();
    for byte in text.trim_end_matches('=').bytes() {
        bits = bits << 6 | ALPHABET.iter().position(|&c| c == byte)? as u32;
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
        }
    }
    Some(out)
}

// Depth 2 is global (platform overrides and the module declaration), depth
// 1 is a platform (environment overrides), depth 0 is an environment choice.
fn validate_rust_policy(value: &serde_json::Value, depth: u8) -> Result<(), String> {
    use serde_json::Value;
    let valid_mode = |v: &Value| {
        v.as_str()
            .is_some_and(|m| matches!(m, "auto" | "native" | "tiered" | "wasm" | "off"))
    };
    match value {
        Value::Bool(_) => Ok(()),
        Value::String(_) if valid_mode(value) => Ok(()),
        Value::Object(object) => {
            for (key, value) in object {
                match key.as_str() {
                    "mode" if valid_mode(value) => {}
                    "dev" | "prod" if depth > 0 => validate_rust_policy(value, 0)?,
                    "platforms" if depth == 2 => {
                        let platforms = value
                            .as_object()
                            .ok_or("rust.platforms must be an object")?;
                        for (platform, policy) in platforms {
                            if !matches!(
                                platform.as_str(),
                                "web" | "ios" | "macos" | "linux" | "android" | "windows"
                            ) {
                                return Err(format!(
                                    "unknown Rust replacement platform: {platform}"
                                ));
                            }
                            validate_rust_policy(policy, 1)?;
                        }
                    }
                    "placement" if depth >= 1 => {
                        if !value
                            .as_str()
                            .is_some_and(|p| matches!(p, "main" | "worker"))
                        {
                            return Err("rust.placement must be \"main\" or \"worker\"".into());
                        }
                    }
                    "module" if depth == 2 => {
                        let module = value.as_object().ok_or("rust.module must be an object")?;
                        if module.len() != 1
                            || !module
                                .get("package")
                                .and_then(Value::as_str)
                                .is_some_and(|s| {
                                    !s.is_empty()
                                        && s.bytes().all(|c| {
                                            c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')
                                        })
                                })
                        {
                            return Err(
                                "rust.module requires a Cargo package name in `package`".into()
                            );
                        }
                    }
                    "data" if depth == 2 => {
                        RustData::from_json(value)?;
                    }
                    _ => return Err(format!("invalid rust policy key or value: {key}")),
                }
            }
            Ok(())
        }
        _ => Err(
            "rust policy must be boolean, auto/native/tiered/wasm/off, or an override object"
                .into(),
        ),
    }
}

/// TypeScript placement (LLP 1027.002 §6) and source mounts; no other keys.
fn validate_placement_policy(value: &serde_json::Value) -> Result<(), String> {
    let object = value.as_object().ok_or("typescript must be an object")?;
    let placement = |v: &serde_json::Value, at: &str| -> Result<(), String> {
        if v.as_str().is_some_and(|p| matches!(p, "main" | "worker")) {
            Ok(())
        } else {
            Err(format!("{at} must be \"main\" or \"worker\""))
        }
    };
    for (key, value) in object {
        match key.as_str() {
            "placement" => placement(value, "typescript.placement")?,
            "platforms" => {
                let platforms = value
                    .as_object()
                    .ok_or("typescript.platforms must be an object")?;
                for (platform, policy) in platforms {
                    if !matches!(
                        platform.as_str(),
                        "web" | "ios" | "macos" | "linux" | "android" | "windows"
                    ) {
                        return Err(format!("unknown placement platform: {platform}"));
                    }
                    let policy = policy.as_object().ok_or_else(|| {
                        format!("typescript.platforms.{platform} must be an object")
                    })?;
                    for (key, value) in policy {
                        match key.as_str() {
                            "placement" => placement(
                                value,
                                &format!("typescript.platforms.{platform}.placement"),
                            )?,
                            other => return Err(format!("invalid typescript policy key: {other}")),
                        }
                    }
                }
            }
            // Mounted directories: the bake resolves and checks them (`js/bake`).
            "sources" => {
                value
                    .as_object()
                    .ok_or("typescript.sources must be an object of name to directory")?;
            }
            other => return Err(format!("invalid typescript policy key: {other}")),
        }
    }
    Ok(())
}

fn canonicalize_document_handlers(json: &mut serde_json::Value) -> Result<(), String> {
    let Some(declared) = json.get_mut("file_handlers") else {
        return Ok(());
    };
    if !declared.is_array() {
        return Err(format!("file_handlers: {declared} is not an array"));
    }
    let handlers = declared.as_array_mut().unwrap();
    for (handler_index, handler) in handlers.iter_mut().enumerate() {
        if !handler.is_object() {
            return Err(format!(
                "file_handlers[{handler_index}]: {handler} is not an object"
            ));
        }
        let handler = handler.as_object_mut().unwrap();
        if let Some(name) = handler.get("name") {
            let name = name.as_str().ok_or_else(|| {
                format!("file_handlers[{handler_index}].name: {name} is not a string")
            })?;
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err(format!(
                    "file_handlers[{handler_index}].name: {} is not a label: use a non-empty string with no control character",
                    serde_json::to_string(name).unwrap_or_default()
                ));
            }
        }
        let accept = handler
            .get_mut("accept")
            .ok_or_else(|| format!("file_handlers[{handler_index}].accept: is required"))?;
        if !accept.is_object() {
            return Err(format!(
                "file_handlers[{handler_index}].accept: {accept} is not an object"
            ));
        }
        let accept = accept.as_object_mut().unwrap();
        for (mime, extensions) in accept {
            if !extensions.is_array() {
                return Err(format!(
                    "file_handlers[{handler_index}].accept.{mime}: {extensions} is not an array"
                ));
            }
            let extensions = extensions.as_array_mut().unwrap();
            for (extension_index, extension) in extensions.iter_mut().enumerate() {
                let written = extension.as_str().map(str::to_owned).ok_or_else(|| {
                    format!(
                        "file_handlers[{handler_index}].accept.{mime}[{extension_index}]: {extension} is not a string"
                    )
                })?;
                let canonical = format!(
                    "{}{}",
                    if written.starts_with('.') { "" } else { "." },
                    written.to_ascii_lowercase()
                );
                let valid = canonical.len() <= 16
                    && !canonical.ends_with('.')
                    && canonical
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.'));
                if !valid {
                    return Err(format!(
                        "file_handlers[{handler_index}].accept.{mime}[{extension_index}]: {} is not a file suffix: use at most 16 ASCII alphanumeric, \"+\" or \".\" code points, with an optional leading dot and no trailing dot",
                        serde_json::to_string(&written).unwrap_or_default()
                    ));
                }
                *extension = serde_json::Value::String(canonical);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod placement_tests {
    use super::Manifest;

    fn manifest(json: serde_json::Value) -> Manifest {
        Manifest {
            json,
            id: "test.app".into(),
            name: "Test".into(),
            declared: true,
        }
    }

    /// A mounted source directory is the bake's to check, not a placement key.
    #[test]
    fn typescript_sources_sit_beside_the_placement() {
        let m = manifest(
            serde_json::json!({"typescript": {"placement": "worker", "sources": {"core": "../core"}}}),
        );
        assert_eq!(m.placement("typescript", "ios").unwrap(), "worker");
        let bad = manifest(serde_json::json!({"typescript": {"sources": ["../core"]}}));
        assert!(bad
            .placement("typescript", "ios")
            .unwrap_err()
            .contains("typescript.sources"));
        let unknown = manifest(serde_json::json!({"typescript": {"elsewhere": 1}}));
        assert!(unknown.placement("typescript", "ios").is_err());
    }

    #[test]
    fn typescript_aliases_are_refused() {
        let m = manifest(serde_json::json!({"typescript": {"aliases": {"@core": "../core"}}}));
        assert_eq!(
            m.placement("typescript", "ios"),
            Err("invalid typescript policy key: aliases".into())
        );
    }
}

#[cfg(test)]
mod document_extension_tests {
    use super::Manifest;

    #[test]
    fn rust_and_javascript_share_document_suffix_cases() {
        let cases: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/document-suffixes.json"
        )))
        .unwrap();
        let dir = std::env::temp_dir().join(format!(
            "exact-manifest-document-suffixes-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let result = std::panic::catch_unwind(|| {
            for case in cases.as_array().unwrap() {
                let declaration = case["declaration"].as_str().unwrap();
                std::fs::write(
                    dir.join("app.json"),
                    serde_json::json!({
                        "name": "Suffixes",
                        "app": {"id": "test.suffixes", "name": "Suffixes"},
                        "file_handlers": [{"action": "/", "accept": {"text/markdown": [declaration]}}]
                    })
                    .to_string(),
                )
                .unwrap();
                match case.get("canonical").and_then(serde_json::Value::as_str) {
                    Some(canonical) => assert_eq!(
                        Manifest::read(&dir).unwrap().json["file_handlers"][0]["accept"]
                            ["text/markdown"][0],
                        canonical,
                        "{declaration:?}"
                    ),
                    None => {
                        let error = Manifest::read(&dir).unwrap_err();
                        assert!(error.contains(case["error"].as_str().unwrap()), "{error}");
                    }
                }
            }
        });
        std::fs::remove_dir_all(&dir).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }

    #[test]
    fn a_selected_variant_has_canonical_document_handlers() {
        let dir = std::env::temp_dir().join(format!(
            "exact-manifest-variant-document-suffixes-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let result = std::panic::catch_unwind(|| {
            std::fs::write(
                dir.join("app.json"),
                serde_json::json!({
                    "name": "Reader",
                    "app": {"id": "test.reader", "name": "Reader"},
                    "variants": {"dev": {"idSuffix": ".dev", "name": "Reader Dev"}},
                    "file_handlers": [{"action": "/", "accept": {"text/markdown": ["MD"]}}]
                })
                .to_string(),
            )
            .unwrap();
            let manifest = Manifest::read_selected(&dir, Some("dev")).unwrap();
            assert_eq!(manifest.id, "test.reader.dev");
            assert_eq!(manifest.variant_base(), Some("test.reader"));
            assert_eq!(
                manifest.json["file_handlers"][0]["accept"]["text/markdown"][0],
                ".md"
            );
        });
        std::fs::remove_dir_all(&dir).unwrap();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
    }
}

#[cfg(test)]
mod appdrop_tests {
    use super::Manifest;

    fn manifest(json: serde_json::Value) -> Manifest {
        Manifest {
            json,
            id: "test.app".into(),
            name: "Test".into(),
            declared: true,
        }
    }

    /// "appdrop" names AppDrop's origin for the signing key; the build and
    /// `scripts/origin.mjs` must agree on the key id.
    #[test]
    fn appdrop_is_named_after_the_signing_key() {
        let m = manifest(serde_json::json!({"deploy": {
            "channels": {"dogfood": "appdrop"},
            "signing": {"key": "shelf", "keys": {"shelf": "/dF36/m50wjVH0iDogmQB4FTDF6bx+qwAyD0seHMV70="}},
        }}));
        let (channel, origin) = m.channel().unwrap();
        assert_eq!(channel, "dogfood");
        assert!(origin
            .unwrap()
            .ends_with("/u/832cad2da443ed7c9e8948f16e5b137d"));
        let unsigned =
            manifest(serde_json::json!({"deploy": {"channels": {"dogfood": "appdrop"}}}));
        assert!(unsigned
            .channel()
            .unwrap_err()
            .contains("deploy.signing.keys"));
    }
}

#[cfg(test)]
mod windows_tests {
    use super::Manifest;

    fn manifest(json: serde_json::Value) -> Manifest {
        Manifest {
            json,
            id: "test.app".into(),
            name: "Test".into(),
            declared: true,
        }
    }

    #[test]
    fn the_painter_is_opted_into_on_one_platform() {
        let mut named = manifest(serde_json::json!({"host": {"windows": {"painter": "ink"}}}));
        assert_eq!(named.painter("windows").unwrap(), Some("ink"));
        assert_eq!(named.painter("linux").unwrap(), None);
        assert_eq!(named.painter("android").unwrap(), None);
        for value in [
            serde_json::json!("gpu"),
            serde_json::json!(true),
            serde_json::Value::Null,
        ] {
            named.json["host"]["windows"]["painter"] = value;
            assert!(named
                .painter("windows")
                .unwrap_err()
                .contains("host.windows.painter"));
        }
    }

    #[test]
    fn the_window_names_what_app_json_gives_and_defaults_the_rest() {
        let named = manifest(serde_json::json!({"host": {"windows": {"window": {
            "width": 1100, "height": 760, "minWidth": 360,
        }}}}));
        assert_eq!(
            named.windows_window().unwrap(),
            "exact_windows::Window { width: 1100.0_f32, height: 760.0_f32, min_width: 360.0_f32, \
             ..exact_windows::Window::default() }"
        );
        let all = manifest(serde_json::json!({"host": {"windows": {"window": {
            "width": 1100, "height": 760, "minWidth": 360, "minHeight": 500, "keepRunning": true,
        }}}}));
        assert_eq!(
            all.windows_window().unwrap(),
            "exact_windows::Window { width: 1100.0_f32, height: 760.0_f32, min_width: 360.0_f32, \
             min_height: 500.0_f32, keep_running: true, }"
        );
        let kept =
            manifest(serde_json::json!({"host": {"windows": {"window": {"keepRunning": true}}}}));
        assert_eq!(
            kept.windows_window().unwrap(),
            "exact_windows::Window { keep_running: true, ..exact_windows::Window::default() }"
        );
        let unsaid = manifest(serde_json::json!({"host": {"windows": {}}}));
        assert_eq!(
            unsaid.windows_window().unwrap(),
            "exact_windows::Window::default()"
        );
        let wrong = manifest(serde_json::json!({"host": {"windows": {"window": {"width": 0}}}}));
        assert!(wrong.windows_window().unwrap_err().contains("window.width"));
        let wrong =
            manifest(serde_json::json!({"host": {"windows": {"window": {"keepRunning": 1}}}}));
        assert!(wrong.windows_window().unwrap_err().contains("keepRunning"));
    }
}

#[cfg(test)]
mod rust_data_tests {
    use super::{Manifest, RustData};

    fn manifest(data: serde_json::Value) -> Manifest {
        Manifest {
            json: serde_json::json!({"rust": {"mode": "off", "data": data}}),
            id: "test.app".into(),
            name: "Test".into(),
            declared: true,
        }
    }

    /// LLP 1153.003 D2: the source defaults to the type's `default()`, and is otherwise in the grammar the web
    /// build copies into a module of its own.
    #[test]
    fn rust_data_names_a_crate_a_type_and_a_constructor() {
        let data = |json| manifest(json).rust_data();
        assert_eq!(
            data(serde_json::json!({"crate": "pins-data", "type": "pins_data::Pins"})).unwrap(),
            Some(RustData {
                krate: "pins-data".into(),
                ty: "pins_data::Pins".into(),
                source: "pins_data::Pins::default()".into(),
            })
        );
        for source in [
            "pins_data::Pins",
            "pins_data::Pins::new()",
            "pins_data::Pins::new(3, \"a, b\", true, -1.5f32, 'x')",
        ] {
            let named = data(
                serde_json::json!({"crate": "pins-data", "type": "pins_data::Pins", "source": source}),
            );
            assert_eq!(named.unwrap().unwrap().source, source);
        }
        for (json, said) in [
            (serde_json::json!({"type": "pins_data::Pins"}), "`crate`"),
            (
                serde_json::json!({"crate": "pins-data", "type": "Pins"}),
                "rust.data.type",
            ),
            (
                serde_json::json!({"crate": "pins-data", "type": "pins_data::Pins", "source": "pins_data::Pins::new(path())"}),
                "rust.data.source",
            ),
            (
                serde_json::json!({"crate": "pins-data", "type": "pins_data::Pins", "source": "other::Pins"}),
                "rust.data.source",
            ),
            (
                serde_json::json!({"crate": "pins-data", "type": "pins_data::Pins", "mode": "off"}),
                "not mode",
            ),
        ] {
            let refused = data(json).unwrap_err();
            assert!(refused.contains(said), "{refused}");
        }
        // The policy reader validates it as it validates the rest of `rust`.
        assert!(manifest(serde_json::json!({"crate": "pins-data"}))
            .rust_mode("ios", true)
            .is_err());
        assert_eq!(
            manifest(serde_json::json!({"crate": "pins-data", "type": "pins_data::Pins"}))
                .rust_mode("ios", true)
                .unwrap(),
            "off"
        );
    }
}
