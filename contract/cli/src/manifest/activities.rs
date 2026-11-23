//! Activity entries share the app's cohort, never a generated Swift state type.
//! @ref LLP 1126.001 WP8, LLP 1126 D9.
use super::Manifest;
use serde_json::Value;
use std::collections::HashSet;

/// A Live Activity's declaration. Placements belong to its component's view.
#[derive(Debug, Clone, PartialEq)]
pub struct Activity {
    /// Stable entry identifier, unique across widgets and activities.
    pub kind: String,
    /// Literal platform label; absence is left to the platform host.
    pub title: Option<String>,
    /// Literal platform description; absence is left to the platform host.
    pub description: Option<String>,
    /// Root component, whose props match the state shape.
    pub component: String,
    /// Contract shape for the JSON state.
    pub state: String,
    /// Placeholder state, validated at bake.
    pub preview: Value,
    /// The app's backend advances this activity through APNs.
    pub push: bool,
    /// Request ActivityKit's frequent-update budget.
    pub frequent: bool,
    /// Android's ongoing notification for this activity, which asks to be
    /// promoted to a Live Update (LLP 1126 D13). Absent, Android refuses the
    /// activity's sends.
    pub android: Option<AndroidActivity>,
}

/// The state fields an Android activity's notification shows, by name.
#[derive(Debug, Clone, PartialEq)]
pub struct AndroidActivity {
    /// A channel `host.android.notificationChannels` declares, or `default`.
    pub channel: String,
    /// A string field: the notification's title.
    pub title: String,
    /// A string field: its text.
    pub text: Option<String>,
    /// A number field from 0 to 1: its progress bar.
    pub progress: Option<String>,
    /// A string field: the status-bar chip's text.
    pub chip: Option<String>,
    /// A number field, Unix milliseconds: the notification's time, counted
    /// down in the chip while ahead and up while past.
    pub when: Option<String>,
}

const ANDROID_KEYS: [&str; 6] = ["channel", "title", "text", "progress", "chip", "when"];

fn android(at: &str, value: &Value) -> Result<AndroidActivity, String> {
    let at = format!("{at}.android");
    let object = value
        .as_object()
        .ok_or_else(|| format!("{at} must be an object naming state fields"))?;
    for key in object.keys() {
        if !ANDROID_KEYS.contains(&key.as_str()) {
            return Err(format!(
                "{at}.{key} is unknown; use {}",
                ANDROID_KEYS.join(", ")
            ));
        }
    }
    let field = |key: &str| {
        object
            .get(key)
            .map(|v| {
                v.as_str()
                    .filter(|v| !v.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{at}.{key} must name a field of the state shape"))
            })
            .transpose()
    };
    let channel = match object.get("channel") {
        None => "default".to_owned(),
        Some(v) => v
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("{at}.channel must be a channel id"))?,
    };
    Ok(AndroidActivity {
        channel,
        title: field("title")?.ok_or_else(|| {
            format!("{at}.title is required: the state field the notification's title shows")
        })?,
        text: field("text")?,
        progress: field("progress")?,
        chip: field("chip")?,
        when: field("when")?,
    })
}

impl Manifest {
    /// Read activities with the same identifier and literal-label rules as widgets.
    pub fn live_activities(&self) -> Result<Vec<Activity>, String> {
        let Some(value) = self.json.get("live_activities") else {
            return Ok(Vec::new());
        };
        let rows = value.as_array().ok_or("live_activities must be an array")?;
        let mut kinds: HashSet<_> = self
            .widgets()?
            .iter()
            .map(|w| w.kind.to_ascii_lowercase())
            .collect();
        rows.iter().enumerate().map(|(i, row)| {
            let at = format!("live_activities[{i}]");
            let object = row.as_object().ok_or_else(|| format!("{at} must be an object"))?;
            for key in object.keys() {
                if !["kind", "component", "state", "preview", "title", "description", "push", "frequent", "android"].contains(&key.as_str()) {
                    return Err(format!("{at}.{key} is unknown; remove it"));
                }
            }
            let name = |key: &str| object.get(key).and_then(Value::as_str)
                .filter(|v| exact_plan::entry_identifier(v)).map(str::to_owned)
                .ok_or_else(|| format!("{at}.{key} must be an identifier"));
            let label = |key: &str| object.get(key).map(|v| v.as_str()
                .filter(|s| !s.trim().is_empty() && !s.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')))
                .map(str::to_owned).ok_or_else(|| format!("{at}.{key} must be a non-empty string without control characters"))).transpose();
            let flag = |key: &str| object.get(key).map(|v| v.as_bool()
                .ok_or_else(|| format!("{at}.{key} must be a Boolean"))).transpose().map(|v| v.unwrap_or(false));
            let kind = name("kind")?;
            if !kinds.insert(kind.to_ascii_lowercase()) {
                return Err(format!("{at}.kind repeats {kind}; give each widget and activity a unique kind"));
            }
            Ok(Activity {
                kind, title: label("title")?, description: label("description")?,
                component: name("component")?, state: name("state")?,
                preview: object.get("preview").filter(|v| v.is_object())
                    .ok_or_else(|| format!("{at}.preview must be an object of state fields"))?.clone(),
                push: flag("push")?, frequent: flag("frequent")?,
                android: object.get("android").map(|v| android(&at, v)).transpose()?,
            })
        }).collect()
    }
}
