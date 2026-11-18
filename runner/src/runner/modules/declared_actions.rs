//! The app's declared actions (LLP 1133.004 stage 2): what its literals
//! declare of each module name, and whether a notification action's record
//! is one of them.

use super::super::DataSource;
use super::{Json, Runner};

impl<D: DataSource> Runner<D> {
    /// What the app's literals declare, by `<extension>.<name>`: each row's
    /// `declared` (LLP 1133.004 stage 2), `{option: [value, …]}`.
    pub(super) fn declared_values_linked(&self) -> Json {
        let mut out = serde_json::Map::new();
        for row in &self.plan.modules {
            let declared = &self.substitute(row)["declared"];
            if declared.is_object() {
                let key = format!(
                    "{}.{}",
                    self.plan.str(row.extension),
                    self.plan.str(row.declared)
                );
                out.insert(key, declared.clone());
            }
        }
        Json::Object(out)
    }

    /// Whether `action`, a notification action's record as its post carried
    /// it, is one this plan declares under `extension` ([`Self::declared_values`]):
    /// a host whose platform hands back the record rather than its own
    /// registration (Android's, LLP 1133.004.000 D5) raises the action only
    /// then, so a button an update removed or changed raises nothing.
    pub(super) fn declares_action_linked(&self, extension: &str, action: &Json) -> bool {
        declared_action(&self.declared_values(), extension, action)
    }
}

/// [`Runner::declares_action`] over `declared`, `declared_values`' answer:
/// each field is compared with its default filled in, as Apple's
/// `DeclaredAction` reads both.
fn declared_action(declared: &Json, extension: &str, action: &Json) -> bool {
    let normal = |a: &Json| {
        let name = a["action"].as_str().filter(|n| !n.is_empty())?;
        let flag = |key: &str, default: bool| a[key].as_bool().unwrap_or(default);
        Some((
            name.to_owned(),
            a["title"].as_str().unwrap_or(name).to_owned(),
            flag("foreground", true),
            flag("destructive", false),
            flag("authenticationRequired", false),
            a["placeholder"].as_str().map(str::to_owned),
        ))
    };
    let (Some(wanted), Some(rows)) = (normal(action), declared.as_object()) else {
        return false;
    };
    let prefix = format!("{extension}.");
    rows.iter()
        .filter(|(key, _)| key.starts_with(&prefix))
        .filter_map(|(_, row)| row["actions"].as_array())
        .flatten()
        .filter_map(Json::as_array)
        .flatten()
        .any(|a| normal(a).as_ref() == Some(&wanted))
}

#[cfg(test)]
mod tests {
    use super::declared_action;
    use serde_json::json;

    #[test]
    fn an_action_is_declared_only_while_the_plan_declares_its_record() {
        let declared = json!({"notifications.showNotification": {"actions": [[
            {"action": "open", "title": "Open", "foreground": true, "destructive": false,
             "authenticationRequired": false, "placeholder": null},
            {"action": "answer", "title": "Answer", "foreground": true, "destructive": false,
             "authenticationRequired": false, "placeholder": "Say"}
        ]]}});
        let declares = |extension, action| declared_action(&declared, extension, &action);
        // The record as the post carried it, absent fields at their defaults.
        assert!(declares(
            "notifications",
            json!({"action": "open", "title": "Open"})
        ));
        assert!(declares(
            "notifications",
            json!({"action": "answer", "title": "Answer", "placeholder": "Say"})
        ));
        // A changed title or placeholder, or a name no list has: an update changed or removed it.
        assert!(!declares(
            "notifications",
            json!({"action": "open", "title": "Show"})
        ));
        assert!(!declares(
            "notifications",
            json!({"action": "answer", "title": "Answer"})
        ));
        assert!(!declares(
            "notifications",
            json!({"action": "delete", "title": "Delete"})
        ));
        // Another extension's declarations, or no name at all.
        assert!(!declares(
            "alarms",
            json!({"action": "open", "title": "Open"})
        ));
        assert!(!declares("notifications", json!({"title": "Open"})));
    }
}
