//! The watch entry: the app's own live plan for Apple Watch, rooted at a component the author
//! writes for the wrist, in its own file. Not a widget: it keeps state, resources, timers and
//! handlers. @ref LLP 1126.002 D4, LLP 1126 D10.
use super::Manifest;
use serde_json::Value;

/// Its key in the envelope's `plans` map and its file, `plans/watch.plan`.
pub const WATCH_KIND: &str = "watch";

/// A watch entry's declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct WatchEntry {
    /// The entry's Contract file, beside `app.contract`.
    pub source: String,
    /// Root component of the watch plan: the first component of `source`.
    pub component: String,
    /// Names replicated from the companion phone.
    pub sync: WatchSync,
}

/// Explicit phone-owned replicas; an empty selection has no transport.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WatchSync {
    /// Root persisted states, with matching types on both entries.
    pub state: Vec<String>,
    /// Ordinary secret.keep names; presence policies are never exported.
    pub secrets: Vec<String>,
}

impl WatchSync {
    /// Whether the binary needs WatchConnectivity.
    pub fn selected(&self) -> bool {
        !self.state.is_empty() || !self.secrets.is_empty()
    }

    /// Binary identity and transport allowlist.
    pub fn json(&self) -> Value {
        let mut state = self.state.clone();
        let mut secrets = self.secrets.clone();
        state.sort();
        secrets.sort();
        serde_json::json!({"state": state, "secrets": secrets})
    }

    fn read(value: Option<&Value>) -> Result<Self, String> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let object = value.as_object().ok_or("watch.sync must be an object")?;
        if object
            .keys()
            .any(|k| !["state", "secrets"].contains(&k.as_str()))
        {
            return Err("watch.sync contains an unknown key".into());
        }
        let list = |key| -> Result<Vec<String>, String> {
            let Some(v) = object.get(key) else {
                return Ok(Vec::new());
            };
            v.as_array()
                .ok_or_else(|| "watch.sync needs arrays of names".to_owned())?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "watch.sync needs string names".into())
                })
                .collect()
        };
        let sync = Self {
            state: list("state")?,
            secrets: list("secrets")?,
        };
        for (kind, names) in [("state", &sync.state), ("secrets", &sync.secrets)] {
            let mut seen = std::collections::HashSet::new();
            for name in names {
                if name.is_empty()
                    || name.len() > 64
                    || name.starts_with("exact.")
                    || !name
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
                    || !seen.insert(name)
                {
                    return Err(format!(
                        "watch.sync.{kind} needs unique explicit names, without internal prefixes"
                    ));
                }
            }
        }
        Ok(sync)
    }
}

impl Manifest {
    /// Read `watch`, whose kind is unique with every widget and activity.
    pub fn watch(&self) -> Result<Option<WatchEntry>, String> {
        let Some(value) = self.json.get("watch") else {
            return Ok(None);
        };
        let object = value.as_object().ok_or("watch must be an object")?;
        if let Some(key) = object
            .keys()
            .find(|k| !["source", "component", "sync"].contains(&k.as_str()))
        {
            return Err(format!("watch.{key} is unknown; remove it"));
        }
        let component = object
            .get("component")
            .and_then(Value::as_str)
            .filter(|v| exact_plan::entry_identifier(v))
            .ok_or("watch.component must be an identifier")?
            .to_owned();
        let source = object
            .get("source")
            .and_then(Value::as_str)
            .filter(|v| {
                v.strip_suffix(".contract").is_some_and(|stem| {
                    !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                })
            })
            .ok_or("watch.source must name a `.contract` file beside app.contract, such as `watch.contract`")?
            .to_owned();
        let taken = self
            .widgets()?
            .iter()
            .map(|w| w.kind.clone())
            .chain(self.live_activities()?.iter().map(|a| a.kind.clone()))
            .any(|kind| kind.eq_ignore_ascii_case(WATCH_KIND));
        if taken {
            return Err(format!("a widget or activity has the kind `{WATCH_KIND}`, which names the watch entry's plan; rename it"));
        }
        Ok(Some(WatchEntry {
            source,
            component,
            sync: WatchSync::read(object.get("sync"))?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(json: Value) -> Manifest {
        Manifest {
            json,
            id: "com.example.app".into(),
            name: "App".into(),
            declared: true,
        }
    }

    #[test]
    fn sync_identity_normalizes_order_and_an_absent_selection() {
        let left = WatchSync::read(Some(&json!({"secrets":["b","a"]}))).unwrap();
        let right = WatchSync::read(Some(&json!({"state":[],"secrets":["a","b"]}))).unwrap();
        assert_eq!(left.json(), right.json());
        assert_eq!(
            WatchSync::read(None).unwrap().json(),
            json!({"state":[],"secrets":[]})
        );
    }

    #[test]
    fn reads_the_entry() {
        let entry =
            manifest(json!({"watch": {"source": "watch.contract", "component": "AppWatch"}}))
                .watch();
        assert_eq!(
            entry,
            Ok(Some(WatchEntry {
                source: "watch.contract".into(),
                component: "AppWatch".into(),
                sync: WatchSync::default()
            }))
        );
        assert_eq!(manifest(json!({})).watch(), Ok(None));
    }

    #[test]
    fn refuses_what_it_cannot_compile() {
        for (watch, said) in [
            (
                json!({"source": "watch.contract", "component": "AppWatch", "families": []}),
                "watch.families is unknown",
            ),
            (
                json!({"source": "watch.contract"}),
                "watch.component must be an identifier",
            ),
            (
                json!({"source": "../watch.contract", "component": "AppWatch"}),
                "watch.source must name",
            ),
            (
                json!({"source": "watch.txt", "component": "AppWatch"}),
                "watch.source must name",
            ),
            (json!(["watch.contract"]), "watch must be an object"),
        ] {
            let err = manifest(json!({ "watch": watch })).watch().unwrap_err();
            assert!(err.contains(said), "{err}");
        }
    }

    #[test]
    fn its_kind_is_unique_with_widgets_and_activities() {
        let app = json!({
            "watch": {"source": "watch.contract", "component": "AppWatch"},
            "live_activities": [{"kind": "Watch", "component": "A", "state": "S", "preview": {}}],
        });
        assert!(manifest(app)
            .watch()
            .unwrap_err()
            .contains("names the watch entry's plan"));
    }
}
