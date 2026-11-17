//! App variants (LLP 1030.008): the app under another identity, which
//! `EXACT_VARIANT` selects (`--variant` sets it) for the app a verb resolves. Resolved as
//! `scripts/variants.mjs` `resolveVariant` resolves it, so the JavaScript and
//! the Rust readers agree on the id every derived identifier starts with;
//! `host/apple/build.mjs` `assertAppleIdentity` holds them to it.

use serde_json::{json, Value};
use std::path::Path;

/// The variant selected for the app in `app_dir`: `EXACT_VARIANT`, when
/// `EXACT_VARIANT_APP` names this directory (the verb that resolved the app
/// set it, `scripts/variants.mjs` `claimVariant`). Any other app's manifest is
/// read as it is. `EXACT_VARIANT` without `EXACT_VARIANT_APP` is refused: no
/// verb resolved an app, and the build would be the base app's. A build
/// script that reads the manifest reruns when either changes.
pub(super) fn selected(app_dir: &Path) -> Result<Option<String>, String> {
    if std::env::var_os("OUT_DIR").is_some() {
        println!("cargo:rerun-if-env-changed=EXACT_VARIANT");
        println!("cargo:rerun-if-env-changed=EXACT_VARIANT_APP");
    }
    let Some(name) = std::env::var("EXACT_VARIANT")
        .ok()
        .filter(|n| !n.is_empty())
    else {
        return Ok(None);
    };
    let Some(claimed) = std::env::var_os("EXACT_VARIANT_APP") else {
        return Err(format!(
            "EXACT_VARIANT={name} names no app: build a variant through a verb \
             (bun exact.mjs <verb> --variant {name}), or unset EXACT_VARIANT"
        ));
    };
    let canonical = |path: &Path| std::fs::canonicalize(path).ok();
    Ok(
        (canonical(app_dir).is_some() && canonical(app_dir) == canonical(Path::new(&claimed)))
            .then_some(name),
    )
}

/// The base id of the selected variant whose resolved id is `id`, read from
/// `EXACT_VARIANT_APP`'s manifest.
pub(super) fn base_of(id: &str) -> Option<String> {
    let dir = std::env::var_os("EXACT_VARIANT_APP")?;
    let manifest = super::Manifest::read(Path::new(&dir)).ok()?;
    (manifest.id == id)
        .then(|| manifest.variant_base().map(str::to_owned))
        .flatten()
}

/// `json` as the variant `name` has it: `app.id` and `host.android.id` with
/// its `idSuffix`, `app.name` its `name`, an App Clip id under the new id,
/// its `icons` in place of every icon source, and `variant: {name, base}`.
pub(super) fn resolve(json: &mut Value, name: &str) -> Result<(), String> {
    if json.get("variant").is_some() {
        return Ok(()); // resolved already, and never twice
    }
    let declared = json.get("variants").and_then(Value::as_object);
    let Some(variant) = declared.and_then(|variants| variants.get(name)).cloned() else {
        let known: Vec<&str> = declared
            .map(|v| v.keys().map(String::as_str).collect())
            .unwrap_or_default();
        let declares = if known.is_empty() {
            "it declares none".to_owned()
        } else {
            format!("it declares {}", known.join(", "))
        };
        return Err(format!(
            "variant {name}: app.json has no variants.{name} ({declares})"
        ));
    };
    let field = |key: &str| {
        variant
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("variants.{name}.{key} is required"))
    };
    let (suffix, label) = (field("idSuffix")?, field("name")?);
    let base = json
        .pointer("/app/id")
        .and_then(Value::as_str)
        .ok_or("`app.id` is required")?
        .to_owned();
    let id = format!("{base}{suffix}");
    json["app"]["id"] = id.clone().into();
    json["app"]["name"] = label.into();
    if let Some(android) = json.pointer_mut("/host/android/id") {
        let package = format!("{}{suffix}", android.as_str().unwrap_or_default());
        *android = package.into();
    }
    if let Some(clip) = json.pointer_mut("/host/ios/appClip/id") {
        let rest = clip
            .as_str()
            .and_then(|clip| clip.strip_prefix(&format!("{base}.")))
            .ok_or_else(|| format!("variants.{name}: host.ios.appClip.id does not begin with {base}., so it cannot be this variant's App Clip"))?;
        *clip = format!("{id}.{rest}").into();
    }
    if let Some(icons) = variant.get("icons") {
        json["icons"] = icons.clone();
        for path in ["/color_scheme_dark", "/host/ios", "/host/macos"] {
            if let Some(Value::Object(section)) = json.pointer_mut(path) {
                section.remove(if path == "/color_scheme_dark" {
                    "icons"
                } else {
                    "icon"
                });
            }
        }
    }
    json["variant"] = json!({ "name": name, "base": base });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use serde_json::json;

    // The same fixture as `scripts/variants.test.mjs`, so the two readers are held to one answer.
    fn manifest() -> serde_json::Value {
        json!({
            "app": { "id": "com.example.v", "name": "V" },
            "icons": [{ "src": "assets/icon.png" }],
            "color_scheme_dark": { "background_color": "#000000", "icons": [{ "src": "assets/dark.png" }] },
            "host": { "android": { "id": "com.example.v_android" }, "ios": { "appClip": { "id": "com.example.v.Clip" }, "icon": "assets/v.icon" } },
            "variants": {
                "dev": { "idSuffix": ".dev", "name": "V Dev", "icons": [{ "src": "assets/dev.png" }] },
                "plain": { "idSuffix": ".qa.two", "name": "V QA" }
            }
        })
    }

    #[test]
    fn a_variant_suffixes_every_identity_and_replaces_every_icon_source() {
        let mut m = manifest();
        resolve(&mut m, "dev").unwrap();
        assert_eq!(
            m["app"],
            json!({ "id": "com.example.v.dev", "name": "V Dev" })
        );
        assert_eq!(m["host"]["android"]["id"], "com.example.v_android.dev");
        assert_eq!(
            m["host"]["ios"],
            json!({ "appClip": { "id": "com.example.v.dev.Clip" } })
        );
        assert_eq!(m["icons"], json!([{ "src": "assets/dev.png" }]));
        assert_eq!(
            m["color_scheme_dark"],
            json!({ "background_color": "#000000" })
        );
        assert_eq!(
            m["variant"],
            json!({ "name": "dev", "base": "com.example.v" })
        );
    }

    #[test]
    fn a_variant_without_icons_keeps_the_apps() {
        let mut m = manifest();
        resolve(&mut m, "plain").unwrap();
        assert_eq!(m["app"]["id"], "com.example.v.qa.two");
        assert_eq!(m["icons"], manifest()["icons"]);
        assert_eq!(m["host"]["ios"]["icon"], "assets/v.icon");
    }

    #[test]
    fn an_undeclared_variant_or_a_foreign_clip_id_is_refused() {
        let error = resolve(&mut manifest(), "preview").unwrap_err();
        assert_eq!(
            error,
            "variant preview: app.json has no variants.preview (it declares dev, plain)"
        );
        let mut m = manifest();
        m["host"]["ios"]["appClip"]["id"] = "com.other.Clip".into();
        assert!(resolve(&mut m, "dev")
            .unwrap_err()
            .contains("does not begin with com.example.v."));
        let error =
            resolve(&mut json!({ "app": { "id": "a.b", "name": "A" } }), "dev").unwrap_err();
        assert!(error.ends_with("(it declares none)"), "{error}");
    }
}
