//! The Android notification list (LLP 1133.004 D3, AZ2). NotificationManager
//! closes a posted summary's children with it. A future alarm is still pending.

use super::{Json, Listed};
use exact_plan::{ModulesRow, Plan};
use serde_json::json;

pub(super) fn invalid(
    plan: &Plan,
    row: &ModulesRow,
    words: &Json,
    from_data: bool,
) -> Option<String> {
    let words = words.as_object()?;
    // A Contract record encodes an omitted word as null. Data calls carry
    // only written keys and can bypass the generated binding's types.
    let given = |key| words.get(key).filter(|v| from_data || !v.is_null());
    let group = given("group");
    let summary = given("groupSummary");
    if group.is_some_and(|v| v.as_str().is_none_or(str::is_empty)) {
        return Some("android.group must be a non-empty string".into());
    }
    if summary.is_some_and(|v| !v.is_boolean()) {
        return Some("android.groupSummary must be a boolean".into());
    }
    if summary.is_some() && group.is_none() {
        return Some("android.groupSummary requires android.group".into());
    }
    if from_data {
        let source = &plan.sources[row.source.0 as usize];
        let options = plan.source_params.get(source.params.start as usize)?.ty;
        let platform = plan
            .type_(options)
            .fields
            .iter()
            .map(|i| plan.field(i))
            .find(|f| plan.str(f.name) == "android")?;
        let ty = plan.type_(platform.ty).elem?;
        if let Some(key) = words.keys().find(|k| plan.field_index(ty, k).is_none()) {
            return Some(format!("showNotification has no option android.{key}"));
        }
    }
    None
}

pub(super) fn listed(args: &Json) -> Json {
    let mut words = args["android"].as_object().cloned().unwrap_or_default();
    if args["silent"] == true && args["android"]["group"].is_null() {
        words.insert("group".into(), json!("silent"));
    }
    words.insert(
        "groupSummary".into(),
        json!(args["android"]["groupSummary"] == true),
    );
    Json::Object(words)
}

impl Listed {
    fn posted(&self) -> bool {
        self.fields.get("delivered") == Some(&Json::Bool(true))
    }

    fn posted_group(&self, summary: bool) -> Option<&str> {
        let words = self.fields.get("android")?;
        if self.posted() && words["groupSummary"] == summary {
            words["group"].as_str()
        } else {
            None
        }
    }

    fn tag(&self) -> Option<&str> {
        self.fields.get("tag")?.as_str().filter(|s| !s.is_empty())
    }
}

/// Remove a delivered post, including a summary's posted children unless the
/// replacement is still the summary of that group. Alarms remain armed.
pub(super) fn take(entries: &mut Vec<Listed>, index: usize, new_summary: Option<&str>) -> Listed {
    let old = entries.remove(index);
    if let Some(group) = old.posted_group(true).filter(|g| Some(*g) != new_summary) {
        entries.retain(|e| e.posted_group(false) != Some(group));
    }
    old
}

/// Posting now replaces the system notification. Scheduling replaces only
/// the alarm. Both withdraw any earlier alarm for the same non-empty tag.
pub(super) fn post(entries: &mut Vec<Listed>, entry: Listed) -> Option<u64> {
    let mut cancelled = None;
    if let Some(tag) = entry.tag() {
        if let Some(i) = entries
            .iter()
            .position(|e| !e.posted() && e.tag() == Some(tag))
        {
            cancelled = Some(entries.remove(i).reference);
        }
        if entry.posted() {
            if let Some(i) = entries.iter().position(|e| e.tag() == Some(tag)) {
                take(entries, i, entry.posted_group(true));
            }
        }
    }
    entries.push(entry);
    cancelled
}

pub(super) fn close(entries: &mut Vec<Listed>, value: &Json) -> Vec<u64> {
    let mut removed = Vec::new();
    if let Some(tag) = value.as_str().filter(|s| !s.is_empty()) {
        while let Some(i) = entries.iter().position(|e| e.tag() == Some(tag)) {
            removed.push(take(entries, i, None).reference);
        }
    }
    removed
}
