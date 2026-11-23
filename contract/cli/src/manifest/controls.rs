//! Control rows (LLP 1126.004 D1): a button the system draws outside the
//! app, which runs one of its tools or opens one of its locations.
use super::Manifest;
use serde_json::{json, Value};
use std::collections::HashSet;

/// One `controls` row as `app.json` writes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Control {
    /// The placed control's stable identity, unique among the app's
    /// controls; generated names derive from it.
    pub kind: String,
    /// What the system labels the button with.
    pub title: String,
    /// An SF Symbol name; Android maps it to a Material glyph.
    pub symbol: String,
    /// What the controls gallery says about it.
    pub description: Option<String>,
    /// What a press does.
    pub action: ControlAction,
    /// Who may press a tool control on a locked device: `required`,
    /// `local` or `always` (LLP 1133.023 D10). `required` for a URL control,
    /// which opens the app.
    pub authentication: String,
}

/// A press runs a tool or opens a location.
#[derive(Debug, Clone, PartialEq)]
pub enum ControlAction {
    /// A declared tool, called with no arguments.
    Tool(String),
    /// A location at the navigation root.
    Url(String),
}

impl Control {
    /// The row with its defaults filled in: what the dispatch digest and
    /// each binary's compatibility id cover.
    pub fn json(&self) -> Value {
        let mut row = json!({
            "kind": self.kind,
            "title": self.title,
            "symbol": self.symbol,
            "description": self.description,
        });
        match &self.action {
            ControlAction::Tool(tool) => {
                row["tool"] = json!(tool);
                row["authentication"] = json!(self.authentication);
            }
            ControlAction::Url(url) => row["url"] = json!(url),
        }
        row
    }
}

const KEYS: [&str; 7] = [
    "kind",
    "title",
    "symbol",
    "description",
    "tool",
    "url",
    "authentication",
];

impl Manifest {
    /// Read `controls`. A kind is an entry identifier, unique without
    /// regard to case, since generated class names land on case-insensitive
    /// file systems.
    pub fn controls(&self) -> Result<Vec<Control>, String> {
        let Some(value) = self.json.get("controls") else {
            return Ok(Vec::new());
        };
        let rows = value.as_array().ok_or("controls must be an array")?;
        let mut kinds = HashSet::new();
        rows.iter().enumerate().map(|(i, row)| {
            let at = format!("controls[{i}]");
            let object = row.as_object().ok_or_else(|| format!("{at} must be an object"))?;
            if let Some(key) = object.keys().find(|k| !KEYS.contains(&k.as_str())) {
                return Err(format!("{at}.{key} is unknown; remove it"));
            }
            let text = |key: &str| object.get(key).map(|v| v.as_str()
                .filter(|s| !s.trim().is_empty() && !s.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')))
                .map(str::to_owned).ok_or_else(|| format!("{at}.{key} must be a non-empty string without control characters"))).transpose();
            let kind = object.get("kind").and_then(Value::as_str)
                .filter(|v| exact_plan::entry_identifier(v)).map(str::to_owned)
                .ok_or_else(|| format!("{at}.kind must be an identifier"))?;
            if !kinds.insert(kind.to_ascii_lowercase()) {
                return Err(format!("{at}.kind repeats {kind}; give each control a unique kind"));
            }
            let title = text("title")?.ok_or_else(|| format!("{at}.title is required"))?;
            let symbol = text("symbol")?.ok_or_else(|| format!("{at}.symbol is required: an SF Symbol name"))?;
            let action = match (text("tool")?, text("url")?) {
                (Some(tool), None) => ControlAction::Tool(tool),
                (None, Some(url)) if url.starts_with('/') && !url.starts_with("//") => ControlAction::Url(url),
                (None, Some(url)) => return Err(format!("{at}.url {url} must be an app path, starting with one /")),
                _ => return Err(format!("{at} names exactly one of `tool` and `url`")),
            };
            let authentication = match (object.get("authentication").map(|v| v.as_str()), &action) {
                (None, _) => "required".to_owned(),
                (Some(_), ControlAction::Url(_)) => return Err(format!("{at}.authentication applies to a tool control; a URL control opens the app")),
                (Some(Some(a @ ("required" | "local" | "always"))), _) => a.to_owned(),
                (Some(_), _) => return Err(format!("{at}.authentication is required, local or always")),
            };
            Ok(Control { kind, title, symbol, description: text("description")?, action, authentication })
        }).collect()
    }
}
