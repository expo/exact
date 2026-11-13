//! Widget entry declarations (LLP 1126 D2). The compiler checks the named
//! component and props; refresh names a function below the data seam.

use super::Manifest;
use serde_json::Value;
use std::collections::HashSet;

/// A WidgetKit entry, independent of any one Apple host.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    /// Stable identifier, also the key of the envelope's `plans` map.
    pub kind: String,
    /// Gallery display name; defaults to the humanised kind.
    pub title: Option<String>,
    /// Gallery description; defaults to the humanised kind.
    pub description: Option<String>,
    /// iOS and macOS WidgetKit family names.
    pub families: Vec<String>,
    /// watchOS families, defaulting to the accessory families above.
    pub watch_families: Vec<String>,
    /// Component declared or imported by `app.contract`.
    pub component: String,
    /// Contract shape whose fields are the component's props.
    pub props: String,
    /// Data-source function: `(epoch_ms: number) -> {date: number, props: Props}[]`.
    pub refresh: String,
    /// Refresh reads account data; an unprovisioned Mac shows the placeholder.
    pub requires_secrets: bool,
    /// Placeholder props, checked against the shape at bake.
    pub preview: Value,
    /// The person's choices in the Edit Widget sheet, when the entry has any.
    pub configuration: Option<Configuration>,
}

/// A widget's configuration: WidgetKit's `AppIntentConfiguration`, whose
/// intent has one parameter per row. A render reads the chosen values through
/// the reserved `snapshot_configuration` inject.
#[derive(Debug, Clone, PartialEq)]
pub struct Configuration {
    /// The intent's title.
    pub title: String,
    /// The intent's description; the generated intent falls back to the title.
    pub description: Option<String>,
    /// In the sheet's order.
    pub parameters: Vec<Parameter>,
}

impl Configuration {
    /// The declaration as `app.json` writes it, every default filled in: what
    /// the gallery's registry carries and what the binary's cohort hashes, the
    /// intent being generated Swift.
    pub fn json(&self) -> Value {
        serde_json::json!({
            "title": self.title,
            "description": self.description,
            "parameters": self.parameters.iter().map(|p| serde_json::json!({
                "name": p.name,
                "type": "enum",
                "title": p.title,
                "values": p.values.iter().map(|(value, title)| serde_json::json!({"value": value, "title": title})).collect::<Vec<_>>(),
                "default": p.default,
            })).collect::<Vec<_>>(),
        })
    }
}

/// One choice in the Edit Widget sheet: an enum of fixed values.
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// The field of the configuration shape that carries the chosen value.
    pub name: String,
    /// The row's label in the sheet.
    pub title: String,
    /// `(value, title)` pairs: the value the render reads and the label the sheet shows.
    pub values: Vec<(String, String)>,
    /// The value before the person chooses, and in the gallery's default preview.
    pub default: String,
}

use exact_plan::entry_identifier;

impl Manifest {
    /// Read widget entries, refusing malformed declarations.
    pub fn widgets(&self) -> Result<Vec<Widget>, String> {
        let Some(value) = self.json.get("widgets") else {
            return Ok(Vec::new());
        };
        let rows = value.as_array().ok_or("widgets must be an array")?;
        let mut kinds = HashSet::new();
        rows.iter()
            .enumerate()
            .map(|(i, row)| {
                let at = format!("widgets[{i}]");
                let object = row
                    .as_object()
                    .ok_or_else(|| format!("{at} must be an object"))?;
                for key in object.keys() {
                    if ![
                        "kind",
                        "title",
                        "description",
                        "families",
                        "watchFamilies",
                        "component",
                        "props",
                        "refresh",
                        "requiresSecrets",
                        "preview",
                        "configuration",
                    ]
                    .contains(&key.as_str())
                    {
                        return Err(format!("{at}.{key} is unknown; remove it"));
                    }
                }
                let name = |key: &str| -> Result<String, String> {
                    object
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|v| entry_identifier(v))
                        .map(str::to_owned)
                        .ok_or_else(|| format!("{at}.{key} must be an identifier"))
                };
                let label = |key: &str| -> Result<Option<String>, String> {
                    object
                        .get(key)
                        .map(|value| swift_label(value).ok_or_else(|| format!("{at}.{key} must be a non-empty string without control characters")))
                        .transpose()
                };
                let kind = name("kind")?;
                if !kinds.insert(kind.to_ascii_lowercase()) {
                    return Err(format!(
                        "{at}.kind repeats {kind}; give each widget a unique kind"
                    ));
                }
                let families = object
                    .get("families")
                    .and_then(Value::as_array)
                    .filter(|a| !a.is_empty())
                    .ok_or_else(|| format!("{at}.families must be a nonempty array"))?;
                let mut seen = HashSet::new();
                let families = families
                    .iter()
                    .map(|v| {
                        let family = v
                            .as_str()
                            .filter(|f| {
                                matches!(
                                    *f,
                                    "systemSmall"
                                        | "systemMedium"
                                        | "systemLarge"
                                        | "systemExtraLarge"
                                        | "accessoryCircular"
                                        | "accessoryRectangular"
                                        | "accessoryInline"
                                        | "accessoryCorner"
                                )
                            })
                            .ok_or_else(|| format!("{at}.families: use a WidgetKit family name"))?;
                        if !seen.insert(family) {
                            return Err(format!(
                                "{at}.families repeats {family}; remove the duplicate"
                            ));
                        }
                        Ok(family.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let mut watch_families = Vec::new();
                if let Some(watch) = object.get("watchFamilies") {
                    let watch = watch.as_array().filter(|a| !a.is_empty())
                        .ok_or_else(|| format!("{at}.watchFamilies must be a nonempty array"))?;
                    let mut seen = HashSet::new();
                    for value in watch {
                        let family = value.as_str().filter(|f| matches!(*f,
                            "accessoryCircular" | "accessoryRectangular" | "accessoryInline" | "accessoryCorner"))
                            .ok_or_else(|| format!("{at}.watchFamilies: use an accessory family name"))?;
                        if !seen.insert(family) { return Err(format!("{at}.watchFamilies repeats {family}; remove the duplicate")); }
                        watch_families.push(family.into());
                    }
                }
                else {
                    watch_families.extend(families.iter().filter(|f| f.starts_with("accessory")).cloned());
                }
                let preview = object
                    .get("preview")
                    .filter(|v| v.is_object())
                    .ok_or_else(|| format!("{at}.preview must be an object of props"))?
                    .clone();
                let configuration = object
                    .get("configuration")
                    .map(|value| configuration(value, &format!("{at}.configuration")))
                    .transpose()?;
                if !watch_families.is_empty() && configuration.as_ref().is_some_and(|c| {
                    c.parameters.iter().fold(1usize, |count, p| count.saturating_mul(p.values.len())) > 64
                }) {
                    return Err(format!("{at}.configuration allows at most 64 watch recommendations (the product of parameter value counts); reduce the values or parameters"));
                }
                Ok(Widget {
                    kind,
                    title: label("title")?,
                    description: label("description")?,
                    families,
                    watch_families,
                    component: name("component")?,
                    props: name("props")?,
                    refresh: name("refresh")?,
                    requires_secrets: match object.get("requiresSecrets") {
                        None => false,
                        Some(value) => value.as_bool().ok_or_else(|| format!("{at}.requiresSecrets must be a boolean"))?,
                    },
                    preview,
                    configuration,
                })
            })
            .collect()
    }
}

/// A label is a Swift string literal in the generated widget bundle.
fn swift_label(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| {
            !s.trim().is_empty()
                && !s
                    .chars()
                    .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        })
        .map(str::to_owned)
}

fn configuration(value: &Value, at: &str) -> Result<Configuration, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{at} must be an object with `title` and `parameters`"))?;
    for key in object.keys() {
        if !["title", "description", "parameters"].contains(&key.as_str()) {
            return Err(format!("{at}.{key} is unknown; remove it"));
        }
    }
    let label = |key: &str, object: &serde_json::Map<String, Value>, at: &str| {
        object.get(key).map(|v| {
            swift_label(v).ok_or_else(|| {
                format!("{at}.{key} must be a non-empty string without control characters")
            })
        })
    };
    let title = label("title", object, at)
        .ok_or_else(|| format!("{at}.title is required: the intent's title"))??;
    let description = label("description", object, at).transpose()?;
    let rows = object
        .get("parameters")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| format!("{at}.parameters must be a nonempty array"))?;
    let mut names = HashSet::new();
    let parameters = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let at = format!("{at}.parameters[{i}]");
            let row = row
                .as_object()
                .ok_or_else(|| format!("{at} must be an object"))?;
            for key in row.keys() {
                if !["name", "title", "type", "values", "default"].contains(&key.as_str()) {
                    return Err(format!("{at}.{key} is unknown; remove it"));
                }
            }
            if row.get("type").and_then(Value::as_str) != Some("enum") {
                return Err(format!("{at}.type must be `enum`: a fixed list of values"));
            }
            let name = row
                .get("name")
                .and_then(Value::as_str)
                .filter(|n| {
                    let mut chars = n.chars();
                    chars.next().is_some_and(|c| c.is_ascii_lowercase())
                        && chars.all(|c| c.is_ascii_alphanumeric())
                })
                .ok_or_else(|| {
                    format!("{at}.name must be a lowerCamelCase identifier, a field of the configuration shape")
                })?
                .to_owned();
            if !names.insert(name.clone()) {
                return Err(format!(
                    "{at}.name repeats {name}; give each parameter a unique name"
                ));
            }
            let title = label("title", row, &at)
                .ok_or_else(|| format!("{at}.title is required: the row's label in the sheet"))??;
            let values = row
                .get("values")
                .and_then(Value::as_array)
                .filter(|a| !a.is_empty())
                .ok_or_else(|| {
                    format!("{at}.values must be a nonempty array of {{\"value\", \"title\"}}")
                })?;
            let mut seen = HashSet::new();
            let values = values
                .iter()
                .enumerate()
                .map(|(j, choice)| {
                    let at = format!("{at}.values[{j}]");
                    let choice = choice
                        .as_object()
                        .filter(|o| o.keys().all(|k| k == "value" || k == "title"))
                        .ok_or_else(|| format!("{at} must be {{\"value\", \"title\"}}"))?;
                    let value = choice
                        .get("value")
                        .and_then(swift_label)
                        .ok_or_else(|| format!("{at}.value must be a non-empty string without control characters"))?;
                    if !seen.insert(value.clone()) {
                        return Err(format!("{at}.value repeats {value}; remove the duplicate"));
                    }
                    let title = label("title", choice, &at)
                        .ok_or_else(|| format!("{at}.title is required: the choice's label in the sheet"))??;
                    Ok((value, title))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let default = match row.get("default") {
                None => values[0].0.clone(),
                Some(v) => v
                    .as_str()
                    .filter(|d| values.iter().any(|(value, _)| value == d))
                    .ok_or_else(|| format!("{at}.default must be one of its values"))?
                    .to_owned(),
            };
            Ok(Parameter {
                name,
                title,
                values,
                default,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Configuration {
        title,
        description,
        parameters,
    })
}
