//! What each `controls` row becomes in this binary (LLP 1126.004 D3, D5):
//! the conformances its tool's effects give the generated intent, and the
//! tiers the host offers or the reason it omits the control. Each row, its
//! tool's signature, effects, digest and placement join the compatibility
//! id, and the Apple generator reads them from `compat.inputs.controls`.
//!
//! Offered in this build: URL controls on iOS and iPadOS 18 and later, and
//! tool controls whose reach starts, updates or ends a Live Activity, as a
//! `LiveActivityIntent`, which Apple runs in the app's process. A tool
//! control with no activity in reach needs app-process placement that no
//! device has qualified (`control-process`), and a recording control the
//! locked-iPhone qualification (`control-recording-unqualified`). The Mac,
//! the watch and Android have no control generator yet, and tvOS,
//! visionOS, Linux and Windows have no controls (`control-host`).

use crate::compat::{compatibility_digest, Compat};
use contract::{ControlAction, Manifest};
use serde_json::{json, Value};

/// iOS's first release with WidgetKit controls.
const FIRST_IOS: &str = "18.0";

/// Derive each control's placement on `platform` and `target` into the
/// compatibility inputs. A recording control on iOS without a Live
/// Activity it starts, or without `device.microphone-background`, refuses
/// the bake: iOS stops such a recording.
pub(crate) fn derive(
    compat: &mut Compat,
    plan: &exact_plan::Plan,
    manifest: &Manifest,
    platform: &str,
    target: &str,
) -> Result<(), String> {
    let rows = manifest.controls()?;
    if rows.is_empty() || platform == "web" {
        return Ok(());
    }
    let iphone = platform == "ios"
        && !["tvos", "watchos", "xros", "visionos"]
            .iter()
            .any(|t| target.contains(t));
    // The Apple build names the extensions it packages (host/apple/build.mjs),
    // and a publisher replays that choice from the build's receipt.
    let packaged = std::env::var("EXACT_APPLE_EXTENSIONS")
        .map_or(true, |kinds| kinds.split(',').any(|k| k == "controls"));
    if std::env::var_os("OUT_DIR").is_some() {
        println!("cargo:rerun-if-env-changed=EXACT_APPLE_EXTENSIONS");
    }
    let declared: Vec<String> = manifest
        .live_activities()?
        .into_iter()
        .map(|a| a.kind)
        .collect();
    let ceiling = compat.inputs["grantCeiling"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let mut out = Vec::new();
    for control in &rows {
        let Some(planned) = (plan.controls.iter()).find(|c| plan.str(c.kind) == control.kind)
        else {
            return Err(format!(
                "control `{}` is in app.json and not in the plan; rebuild the plan",
                control.kind
            ));
        };
        let effects: Value = serde_json::from_str(plan.str(planned.effects)).map_err(|e| {
            format!(
                "control `{}`: the plan's effects do not parse ({e}); rebuild the plan",
                control.kind
            )
        })?;
        let signature = planned
            .tool
            .map(|t| contract::faces::tool_signature(plan, t.0 as usize));
        let recording =
            effects["audio"].as_str().is_some_and(|a| a != "none") || effects["recorder"] == true;
        // Speech recognition on the microphone is capture no Apple
        // conformance qualifies (D3's table).
        let unsupported = ["camera", "display", "speech"]
            .iter()
            .any(|k| effects[k] == "known");
        if iphone && recording && !unsupported {
            prerequisites(&control.kind, &effects, &declared, &ceiling)?;
        }
        // The generated intent's protocols on iOS and iPadOS (D3's table).
        let mut conformances = Vec::new();
        if iphone
            && effects["activity"]
                .as_array()
                .is_some_and(|a| !a.is_empty())
        {
            conformances.push("LiveActivityIntent");
        }
        if iphone && recording {
            conformances.push("AudioRecordingIntent");
        }
        let placement = match () {
            _ if !iphone => omitted("control-host", "this host offers no controls in this build"),
            _ if !packaged => omitted("control-metadata", "this build packages no control extension: the portable Apple builder cannot extract a control's App Intents metadata, or `--extensions` left `controls` out"),
            _ => match &control.action {
                ControlAction::Url(_) => json!({ "offered": [{ "from": FIRST_IOS, "intent": "open" }] }),
                ControlAction::Tool(_) if unsupported => {
                    omitted("control-capture", "its tool starts a camera, screen or speech capture, which a control may not")
                }
                ControlAction::Tool(_) if recording => omitted("control-recording-unqualified", "starting a recording from a locked iPhone awaits its device qualification (LLP 1126.004 §7 piece 0)"),
                ControlAction::Tool(_) if !conformances.is_empty() => {
                    json!({ "offered": [{ "from": FIRST_IOS, "intent": "tool" }] })
                }
                ControlAction::Tool(_) => omitted("control-process", "its tool starts no Live Activity, and running such an intent in the app's process is not yet qualified on a device"),
            },
        };
        out.push(json!({
            "control": control.json(),
            "digest": plan.str(planned.digest),
            "signature": signature,
            "effects": effects,
            "conformances": conformances,
            "placement": placement,
        }));
    }
    // By kind, so reordering the rows leaves the id as it was.
    out.sort_by(|a, b| {
        a["control"]["kind"]
            .as_str()
            .cmp(&b["control"]["kind"].as_str())
    });
    compat.inputs["controls"] = json!(out);
    compat.id = compatibility_digest(&compat.inputs);
    Ok(())
}

fn omitted(reason: &str, why: &str) -> Value {
    json!({ "omitted": reason, "why": why })
}

/// A recording control's iOS requirements (D3): its reach starts a declared
/// Live Activity, and the grant ceiling keeps capture running in the
/// background.
fn prerequisites(
    kind: &str,
    effects: &Value,
    declared: &[String],
    ceiling: &str,
) -> Result<(), String> {
    let kinds = effects["activityKinds"].as_array().into_iter().flatten();
    let starts = kinds
        .filter_map(Value::as_str)
        .any(|k| declared.iter().any(|d| d == k))
        || (effects["dynamicKind"] == true && !declared.is_empty());
    if !starts {
        return Err(format!("control-recording-activity: control `{kind}` records, and its tool starts no Live Activity that app.json's `live_activities` declares; iOS stops a recording whose intent starts none, so start one before the recording (LLP 1126.004 D3)"));
    }
    let background = ceiling
        .lines()
        .any(|l| l.split_whitespace().next() == Some("device.microphone-background"));
    if !background {
        return Err(format!("grant-needs: control `{kind}` records while the app is not in front, which needs a `device.microphone-background <purposeKey>` grant line beside `device.microphone` (LLP 1126.004 D3, LLP 1133.033 D3)"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "controls_tests.rs"]
mod tests;
