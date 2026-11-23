//! An Android app's notification channels (LLP 1133.004.000 D1): the
//! declaration in `host.android.notificationChannels`, checked, with each
//! channel's name and description read from the strings tables, as a grant's
//! purpose is (LLP 1060).
//!
//! The result rides in `compat.reach` as `androidChannels`, present exactly
//! when the app grants `device.notifications`: the Android build writes the
//! channels' resources from it, and with none it writes nothing, so an app
//! that does not notify has no Notifications row in Settings. The
//! declaration's digest enters the id, since the channels are the APK's
//! resources.

use crate::compat::{canonical, compatibility_digest, Compat};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

/// The platform's `IMPORTANCE_*` names a channel may take: `IMPORTANCE_MAX`
/// is documented "Unused", and `none` is the person's to choose.
const IMPORTANCES: [&str; 4] = ["min", "low", "default", "high"];

/// The sound files `res/raw` carries that SystemUI plays.
const SOUNDS: [&str; 3] = ["ogg", "mp3", "wav"];

/// The channel a post without one takes, which the host makes undeclared.
const DEFAULT: &str = "default";

/// Fill `compat.reach.androidChannels` for an Android bake. Every refusal in
/// one run.
pub(crate) fn derive(app_dir: &Path, manifest: &Value, compat: &mut Compat) -> Result<(), String> {
    let declared = &manifest["host"]["android"]["notificationChannels"];
    let granted = compat.inputs["grantCeiling"]
        .as_str()
        .and_then(|c| exact_runner::device::device_grants(c).ok())
        .is_some_and(|g| g.iter().any(|g| g.device.name == "notifications"));
    let list = match declared {
        Value::Null => Vec::new(),
        Value::Array(list) => list.clone(),
        _ => {
            return Err(
                "android-channel: host.android.notificationChannels is a list of channels".into(),
            )
        }
    };
    // The activities Android shows as ongoing notifications (LLP 1126 D13):
    // each posts on a channel, so it needs the grant a channel does.
    let activities: Vec<Value> = (manifest["live_activities"].as_array().into_iter().flatten())
        .filter(|row| row.get("android").is_some())
        .cloned()
        .collect();
    if !granted {
        if let Some(row) = activities.first() {
            return Err(format!("android-activity-needs-notifications: live_activities \"{}\" has an `android` mapping, which posts a notification, and the app does not grant `device.notifications <strings key>`; grant the line or remove the mapping", row["kind"].as_str().unwrap_or("?")));
        }
        if list.is_empty() {
            return Ok(());
        }
        return Err("android-channels-need-notifications: host.android.notificationChannels declares channels, and the app does not grant `device.notifications <strings key>`; a channel carries only notifications, so grant the line or remove the declaration".into());
    }
    let strings = contract::strings_tables(app_dir)?;
    let mut errors = Vec::new();
    let mut ids: Vec<&str> = Vec::new();
    let mut channels = Vec::new();
    for (i, c) in list.iter().enumerate() {
        let at = format!("host.android.notificationChannels[{i}]");
        let Some(id) = c["id"].as_str() else {
            errors.push(format!("android-channel: {at} has no `id`"));
            continue;
        };
        let resource = id.chars().next().is_some_and(|c| c.is_ascii_lowercase())
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
        if !resource {
            errors.push(format!("android-channel: {at}'s id \"{id}\" is not an Android resource name (a lowercase letter, then lowercase letters, digits and `_`): its sound is named by it"));
        } else if ids.contains(&id) {
            errors.push(format!("android-channel: {at} declares \"{id}\" again"));
        }
        ids.push(id);
        let mut text = |field: &str, required: bool| -> Value {
            let Some(key) = c[field].as_str() else {
                if required || !c[field].is_null() {
                    errors.push(format!("android-channel: {at} names its {field} by strings key (LLP 1060), `\"{field}\": \"channel.{id}\"`"));
                }
                return Value::Null;
            };
            let base = strings
                .as_ref()
                .and_then(|s| s.base_table().get(key))
                .filter(|t| !t.trim().is_empty());
            let (Some(strings), Some(base)) = (strings.as_ref(), base) else {
                let table = strings.as_ref().map_or_else(
                    || "the app has no strings/ directory".to_string(),
                    |s| {
                        format!(
                            "strings/{}.json has no \"{key}\", or only a blank one",
                            s.base
                        )
                    },
                );
                errors.push(format!("android-channel: {at}'s {field} is the strings key \"{key}\", and {table}: add the text Settings shows"));
                return Value::Null;
            };
            let texts: serde_json::Map<String, Value> = (strings.tables.iter())
                .map(|(locale, table)| {
                    let t = table
                        .get(key)
                        .filter(|t| !t.trim().is_empty())
                        .unwrap_or(base);
                    (locale.clone(), Value::String(t.clone()))
                })
                .collect();
            Value::Object(texts)
        };
        let name = text("name", true);
        let description = text("description", false);
        let importance = c["importance"].as_str().unwrap_or("default");
        if !IMPORTANCES.contains(&importance) {
            errors.push(format!(
                "android-channel: {at}'s importance \"{importance}\" is not one of {}",
                IMPORTANCES.join(", ")
            ));
        }
        let sound = match &c["sound"] {
            Value::Null => Value::Null,
            Value::String(path) => {
                let extension = Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase);
                if !extension.as_deref().is_some_and(|e| SOUNDS.contains(&e)) {
                    errors.push(format!("android-channel: {at}'s sound \"{path}\" is not an .ogg, .mp3 or .wav file, which the channel plays"));
                } else if path.split('/').any(|p| p == "..")
                    || path.starts_with('/')
                    || !app_dir.join(path).is_file()
                {
                    errors.push(format!("android-channel: {at}'s sound \"{path}\" is not a file in the app's directory"));
                }
                Value::String(path.clone())
            }
            _ => {
                errors.push(format!(
                    "android-channel: {at}'s sound is an asset path, `\"assets/alert.wav\"`"
                ));
                Value::Null
            }
        };
        for key in c.as_object().into_iter().flat_map(|o| o.keys()) {
            if !["id", "name", "description", "importance", "sound"].contains(&key.as_str()) {
                errors.push(format!("android-channel: {at} gives \"{key}\"; a channel has id, name, description, importance and sound"));
            }
        }
        channels.push(json!({ "id": id, "name": name, "description": description, "importance": importance, "sound": sound }));
    }
    // A post's literal channel is checked here; a computed one is refused at
    // run time. Contract writes it `android.channelId="…"`, which no other
    // name spells, so the app's source text is read for it as `derives` reads
    // an option's literal values (`contract::native::derived`).
    for literal in literal_channels(&contract::native::app_text(app_dir)) {
        if literal != "default" && !ids.contains(&literal.as_str()) {
            errors.push(format!("android-channel-undeclared: a post names android.channelId=\"{literal}\", which host.android.notificationChannels does not declare; declare it there, or post on \"default\""));
        }
    }
    let mut rows = Vec::new();
    for row in &activities {
        let kind = row["kind"].as_str().unwrap_or("?");
        let channel = row["android"]["channel"].as_str().unwrap_or(DEFAULT);
        let declared = channels.iter().find(|c| c["id"] == channel);
        if channel != DEFAULT && declared.is_none() {
            errors.push(format!("android-activity-channel: live_activities \"{kind}\" posts on android.channel \"{channel}\", which host.android.notificationChannels does not declare; declare it there, or use \"default\""));
        } else if declared.is_some_and(|c| c["importance"] == "min") {
            // A promoted notification's channel is not IMPORTANCE_MIN
            // (developer.android.com/develop/ui/views/notifications/live-update).
            errors.push(format!("android-activity-channel: live_activities \"{kind}\" posts on \"{channel}\", whose importance is \"min\", which Android never promotes or shows in the status bar; give it \"low\" or higher, or post on another channel"));
        }
        let mut android = row["android"].clone();
        android["channel"] = Value::String(channel.to_owned());
        rows.push(json!({ "kind": kind, "android": android }));
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    if compat.reach.is_null() {
        compat.reach = json!({});
    }
    if !rows.is_empty() {
        let mut canon = String::new();
        canonical(&Value::Array(rows.clone()), &mut canon);
        let digest = Sha256::digest(canon.as_bytes());
        compat.inputs["androidActivities"] =
            Value::String(digest[..16].iter().map(|b| format!("{b:02x}")).collect());
        compat.id = compatibility_digest(&compat.inputs);
        compat.reach["androidActivities"] = Value::Array(rows);
    }
    compat.reach["androidChannels"] = Value::Array(channels);
    if !list.is_empty() {
        let mut canon = String::new();
        canonical(&compat.reach["androidChannels"], &mut canon);
        let digest = Sha256::digest(canon.as_bytes());
        compat.inputs["androidChannels"] =
            Value::String(digest[..16].iter().map(|b| format!("{b:02x}")).collect());
        compat.id = compatibility_digest(&compat.inputs);
    }
    Ok(())
}

/// Each string literal `text` gives `android.channelId`, in order.
fn literal_channels(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("android.channelId") {
        rest = &rest[at + "android.channelId".len()..];
        let Some(value) = rest.trim_start().strip_prefix('=').map(str::trim_start) else {
            continue;
        };
        let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            continue;
        };
        if let Some(end) = value[1..].find(quote) {
            let literal = value[1..1 + end].to_string();
            if !out.contains(&literal) {
                out.push(literal);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compat(grants: &str) -> Compat {
        Compat {
            id: "old".into(),
            inputs: json!({ "grantCeiling": grants }),
            channel: String::new(),
            origin: None,
            activate: "next-launch".into(),
            check_interval: None,
            target: "aarch64-linux-android".into(),
            embedded: Value::Null,
            reach: Value::Null,
            painter: None,
            game: false,
            host_work: false,
        }
    }

    struct Dir(std::path::PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn app(tables: &[(&str, &str)]) -> Dir {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("exact-bake-channels-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("strings")).unwrap();
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        std::fs::write(dir.join("assets/alert.wav"), b"RIFF").unwrap();
        for (locale, json) in tables {
            std::fs::write(dir.join(format!("strings/{locale}.json")), json).unwrap();
        }
        Dir(dir)
    }

    fn manifest(channels: Value) -> Value {
        json!({ "host": { "android": { "notificationChannels": channels } } })
    }

    const GRANT: &str = "device.notifications purpose.notify";
    const EN: &str = r#"{"purpose.notify": "Alerts.", "channel.urgent": "Urgent", "channel.urgent.about": "Panic alerts"}"#;

    #[test]
    fn a_declared_channel_carries_its_texts_in_every_locale_and_moves_the_id() {
        let dir = app(&[("en", EN), ("es", r#"{"channel.urgent": "Urgencias"}"#)]);
        let mut c = compat(GRANT);
        let declared = manifest(
            json!([{ "id": "urgent", "name": "channel.urgent", "description": "channel.urgent.about", "importance": "high", "sound": "assets/alert.wav" }]),
        );
        derive(&dir.0, &declared, &mut c).unwrap();
        assert_eq!(
            c.reach["androidChannels"],
            json!([{ "id": "urgent", "name": { "en": "Urgent", "es": "Urgencias" },
                "description": { "en": "Panic alerts", "es": "Panic alerts" }, "importance": "high", "sound": "assets/alert.wav" }])
        );
        assert_ne!(c.id, "old");
    }

    #[test]
    fn granted_with_no_declaration_is_an_empty_list_and_ungranted_is_nothing() {
        let dir = app(&[("en", EN)]);
        let mut c = compat(GRANT);
        derive(&dir.0, &json!({}), &mut c).unwrap();
        assert_eq!(c.reach["androidChannels"], json!([]));
        assert_eq!(c.id, "old");
        let mut c = compat("net.fetch https://example.com");
        derive(&dir.0, &json!({}), &mut c).unwrap();
        assert!(c.reach.is_null());
    }

    fn with_activity(mut manifest: Value, android: Value) -> Value {
        manifest["live_activities"] = json!([
            { "kind": "patrol", "component": "Patrol", "state": "State", "preview": {}, "android": android },
            { "kind": "ios", "component": "Ios", "state": "State", "preview": {} }
        ]);
        manifest
    }

    #[test]
    fn an_android_activity_rides_in_reach_with_its_channel_and_moves_the_id() {
        let dir = app(&[("en", EN)]);
        let declared =
            manifest(json!([{ "id": "urgent", "name": "channel.urgent", "importance": "high" }]));
        let mut c = compat(GRANT);
        derive(&dir.0, &declared, &mut c).unwrap();
        let before = c.id.clone();
        let mut c = compat(GRANT);
        let both = with_activity(
            declared,
            json!({ "channel": "urgent", "title": "title", "progress": "done" }),
        );
        derive(&dir.0, &both, &mut c).unwrap();
        assert_eq!(
            c.reach["androidActivities"],
            json!([{ "kind": "patrol", "android": { "channel": "urgent", "title": "title", "progress": "done" } }])
        );
        assert_ne!(c.id, before);
        // No channel: the default one, which the host makes.
        let mut c = compat(GRANT);
        derive(
            &dir.0,
            &with_activity(json!({}), json!({ "title": "title" })),
            &mut c,
        )
        .unwrap();
        assert_eq!(
            c.reach["androidActivities"][0]["android"]["channel"],
            "default"
        );
    }

    #[test]
    fn an_android_activity_needs_a_declared_channel_above_min_and_the_grant() {
        let dir = app(&[("en", EN)]);
        let declared =
            manifest(json!([{ "id": "quiet", "name": "channel.urgent", "importance": "min" }]));
        let error = derive(
            &dir.0,
            &with_activity(
                declared.clone(),
                json!({ "channel": "urgnet", "title": "t" }),
            ),
            &mut compat(GRANT),
        )
        .unwrap_err();
        assert!(error.starts_with("android-activity-channel: live_activities \"patrol\" posts on android.channel \"urgnet\""), "{error}");
        let error = derive(
            &dir.0,
            &with_activity(declared, json!({ "channel": "quiet", "title": "t" })),
            &mut compat(GRANT),
        )
        .unwrap_err();
        assert!(error.contains("whose importance is \"min\""), "{error}");
        let error = derive(
            &dir.0,
            &with_activity(json!({}), json!({ "title": "t" })),
            &mut compat(""),
        )
        .unwrap_err();
        assert!(
            error.starts_with("android-activity-needs-notifications:"),
            "{error}"
        );
        // Without a mapping, an iOS activity asks nothing of Android.
        let mut ios = with_activity(json!({}), json!({}));
        ios["live_activities"].as_array_mut().unwrap().remove(0);
        let mut c = compat("");
        derive(&dir.0, &ios, &mut c).unwrap();
        assert!(c.reach.is_null());
    }

    #[test]
    fn a_post_on_an_undeclared_literal_channel_is_refused() {
        let dir = app(&[("en", EN)]);
        std::fs::write(dir.0.join("app.contract"), "  action a\n    showNotification(title=\"t\", android.channelId=\"urgnet\")\n    showNotification(title=\"t\", android.channelId = \"urgent\")\n    showNotification(title=\"t\", android.channelId=\"default\")\n").unwrap();
        let declared = manifest(json!([{ "id": "urgent", "name": "channel.urgent" }]));
        let error = derive(&dir.0, &declared, &mut compat(GRANT)).unwrap_err();
        assert_eq!(error.lines().count(), 1, "{error}");
        assert!(
            error.starts_with(
                "android-channel-undeclared: a post names android.channelId=\"urgnet\""
            ),
            "{error}"
        );
    }

    #[test]
    fn channels_without_the_notifications_grant_are_refused() {
        let dir = app(&[("en", EN)]);
        let error = derive(
            &dir.0,
            &manifest(json!([{ "id": "urgent", "name": "channel.urgent" }])),
            &mut compat(""),
        )
        .unwrap_err();
        assert!(
            error.starts_with("android-channels-need-notifications: "),
            "{error}"
        );
    }

    #[test]
    fn a_missing_key_a_bad_sound_and_bad_ids_are_refused_in_one_run() {
        let dir = app(&[("en", EN)]);
        let declared = manifest(json!([
            { "id": "urgent", "name": "channel.missing" },
            { "id": "Loud-1", "name": "channel.urgent", "sound": "assets/alert.aiff" },
            { "id": "silent", "name": "channel.urgent", "sound": "assets/none.wav" },
            { "id": "urgent", "name": "channel.urgent", "importance": "max" },
        ]));
        let error = derive(&dir.0, &declared, &mut compat(GRANT)).unwrap_err();
        for want in [
            "strings/en.json has no \"channel.missing\"",
            "\"Loud-1\" is not an Android resource name",
            "is not an .ogg, .mp3 or .wav file",
            "\"assets/none.wav\" is not a file",
            "declares \"urgent\" again",
            "importance \"max\" is not one of",
        ] {
            assert!(error.contains(want), "{want}: {error}");
        }
        // `silent` is an id like any other: a silent post keeps its channel (LLP 1133.004.000 D2).
        assert!(!error.contains("id \"silent\""), "{error}");
        assert!(
            error.lines().all(|l| l.starts_with("android-channel: ")),
            "{error}"
        );
    }
}
