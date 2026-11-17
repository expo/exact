//! An implementation's own declarations, sharing its interface's admission and runtime owner.

use super::{module, object};
use crate::resolve::{resolve, Origin};
use contract_syntax::{File, Span, UseDecl, UseName};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub(super) fn read(
    path: &Path,
    json: &Value,
    origin: &Origin,
    consulted: &mut Vec<PathBuf>,
    span: Span,
    file: &mut File,
) -> Result<(), String> {
    let names = json["names"].as_array().cloned().unwrap_or_default();
    if names.is_empty() {
        return Ok(());
    }
    let implements = json["implements"].as_str().unwrap_or_default();
    let (interface, range) = implements.rsplit_once('@').unwrap_or_default();
    if interface.is_empty() || range.is_empty() {
        return Err("an implementation's `implements` names an interface and version range".into());
    }
    let Origin::Package {
        name,
        root,
        manifest,
        ..
    } = origin
    else {
        return Err("an implementation belongs to a package".into());
    };
    let package: Value =
        serde_json::from_str(&std::fs::read_to_string(manifest).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut exports = vec![".".to_owned()];
    exports.extend(
        object(&package["exports"])
            .keys()
            .filter(|k| k.starts_with("./"))
            .cloned(),
    );
    let export = exports
        .into_iter()
        .find(|key| {
            crate::package_views::export(&package["exports"], key)
                .and_then(|p| root.join(p).canonicalize().ok())
                .as_deref()
                == Some(path)
        })
        .ok_or_else(|| "an implementation's exact.json is exported by its package".to_owned())?;
    let owner = format!("{name}{}", export.strip_prefix('.').unwrap_or_default());
    let dir = path.parent().unwrap_or(root);
    // The package's own interface is read by its path inside the package: a
    // linked package's directory has no `node_modules` above it that
    // installs the package again.
    let specifier = if interface == name {
        let entry = crate::package_views::export(&package["exports"], ".")
            .and_then(|e| e.strip_prefix("./"))
            .ok_or_else(|| format!("the package `{name}` exports no interface as `.`"))?;
        let depth = dir.strip_prefix(root).map_or(0, |d| d.components().count());
        match depth {
            0 => format!("./{entry}"),
            _ => format!("{}{entry}", "../".repeat(depth)),
        }
    } else {
        interface.to_owned()
    };
    let resolved =
        resolve(&specifier, dir, origin, root, consulted).map_err(|(_, message)| message)?;
    consulted.push(resolved.key.clone());
    let shared: Value =
        serde_json::from_str(&std::fs::read_to_string(&resolved.key).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if shared["kind"] != "interface" || shared["name"] != interface {
        return Err(format!(
            "`{implements}` does not resolve to interface `{interface}`"
        ));
    }
    let shared_shapes: Vec<UseName> = object(&shared["shapes"])
        .keys()
        .filter(|name| !object(&json["shapes"]).contains_key(*name))
        .map(|name| UseName {
            name: name.clone(),
            alias: None,
            span,
        })
        .collect();
    // Primitive interface sources can answer this implementation's substitutes too.
    file.uses.push(UseDecl {
        names: shared_shapes,
        path: specifier,
        span,
    });
    let targets = json["targets"]
        .as_array()
        .filter(|t| !t.is_empty() && t.iter().all(Value::is_string))
        .ok_or_else(|| {
            "an implementation's `targets` is a nonempty list of target names".to_owned()
        })?;
    let mut policy = shared;
    for (key, value) in object(json) {
        if key != "shapes" {
            policy[key] = value.clone();
        }
    }
    for (name, shape) in object(&json["shapes"]) {
        policy["shapes"][name] = shape.clone();
    }
    for entry in names {
        let covers = match entry.get("targets") {
            None => targets,
            Some(Value::Array(own)) if own.iter().all(|t| t.is_string() && targets.contains(t)) => {
                own
            }
            _ => {
                return Err(
                    "a name's `targets` is a list within its implementation's targets".into(),
                )
            }
        };
        let mut decl = module(&entry, interface, &policy, span)?;
        let mut substitute: Value =
            serde_json::from_str(&decl.substitute).map_err(|e| e.to_string())?;
        substitute["owner"] = owner.clone().into();
        substitute["implementationTargets"] = covers.clone().into();
        decl.substitute = substitute.to_string();
        // A shared type read alone retains none of this implementation's data-only calls.
        decl.view_only = true;
        file.modules.push(decl);
    }
    Ok(())
}

/// The implementations an interface's own package carries, by subpath, from
/// its `package.json` `exports` (LLP 1133.001 D1: `./apple`, `./android`).
pub(super) fn endorsed(dir: &Path) -> Result<Vec<(String, Value)>, String> {
    let manifest = dir.join("package.json");
    let text =
        std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| format!("{}: not JSON: {e}", manifest.display()))?;
    let mut out = Vec::new();
    for (key, target) in object(&json["exports"]) {
        let (Some(subpath), Some(target)) = (key.strip_prefix("./"), target.as_str()) else {
            continue;
        };
        if target != format!("./{subpath}/exact.json") {
            continue;
        }
        let at = dir.join(subpath).join("exact.json");
        let text = std::fs::read_to_string(&at).map_err(|e| format!("{}: {e}", at.display()))?;
        let implementation: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", at.display()))?;
        if implementation["kind"] == "implementation" {
            out.push((subpath.to_owned(), implementation));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
