//! What `app.json` adds to a compiled plan: the app's identity, its hatch
//! words, and the system's ways into the app (LLP 1133.023): the manifest's
//! `shortcuts` and the packages that publish shortcuts and content open a
//! URL at the navigation root, so an app with any of them and no
//! `navigate=` root is refused (`quick-actions-root`, `search-index-root`).
//! The manifest's `intents` faces (D8) are checked against the tools they
//! name and stored on the tools' rows.

use crate::{native, CompileError, Manifest};
use contract_syntax::{File, Node, Span};
use exact_plan::Plan;
use serde_json::{json, Value};
use std::path::Path;

/// Read `app.json` beside the app, where there is one, into `plan`.
pub(crate) fn from_manifest(
    path: &Path,
    app_root: &Path,
    file: &File,
    plan: &mut Plan,
) -> Result<(), Vec<CompileError>> {
    if !app_root.join("app.json").is_file() && !app_root.join(".shells/app.json").is_file() {
        return Ok(());
    }
    let refused = |id: &str, message: String| {
        vec![CompileError {
            pass: "app",
            id: id.into(),
            message,
            span: Span::default(),
            file: Some(path.into()),
            related: Box::new([]),
        }]
    };
    let manifest = Manifest::read(app_root).map_err(|m| refused("app-manifest", m))?;
    if !plan.app_id.is_empty() && plan.app_id != manifest.id {
        return Err(refused(
            "app-identity",
            format!(
                "the plan names app {}, but app.json names {}",
                plan.app_id, manifest.id
            ),
        ));
    }
    // @ref LLP 1075.003.000.001 §4.3 — each hatch word with the platforms
    // that handle it, under the plan's signature: a host calls a word only
    // where the plan says its platform handles it.
    let rows = native::hatch_rows(&manifest).map_err(|m| refused("app-manifest", m))?;
    for (word, platforms) in rows {
        plan.add_hatch(&word, platforms);
    }
    let shortcuts = (manifest.json.get("shortcuts").and_then(|s| s.as_array()))
        .filter(|s| !s.is_empty())
        .cloned();
    let uses = |extension: &str| (plan.modules.iter()).any(|m| plan.str(m.extension) == extension);
    let root = navigation_root(file);
    if !root && (shortcuts.is_some() || uses("quick-actions")) {
        return Err(refused(
            "quick-actions-root",
            "a quick action opens its url at the navigation root, and the root component's outermost node has no `navigate=` (LLP 1133.023 D1; docs/recipes/navigation-stack.md)".into(),
        ));
    }
    if !root && uses("search-index") {
        return Err(refused(
            "search-index-root",
            "a search result opens its entry's url at the navigation root, and the root component's outermost node has no `navigate=` (LLP 1133.023 D1; docs/recipes/navigation-stack.md)".into(),
        ));
    }
    // The agent's substitute shows the manifest's items before the app's
    // (LLP 1133.023 §10), as the platform does.
    if let Some(shortcuts) = shortcuts {
        for i in 0..plan.modules.len() {
            let row = &plan.modules[i];
            if plan.str(row.extension) != "quick-actions" {
                continue;
            }
            let mut sub: serde_json::Value =
                serde_json::from_str(plan.str(row.substitute)).unwrap_or_default();
            if sub["shortcuts"].is_null() {
                continue;
            }
            sub["manifest"] = serde_json::Value::Array(shortcuts.clone());
            plan.set_module_substitute(i, &sub.to_string());
        }
    }
    // The synthetic store's catalogue (LLP 1133.015 §9): the products the
    // agent's purchases sell, which each name's substitute carries and the
    // runner hands the implementation bound under the agent.
    if let Some(at) = manifest.json.pointer("/purchases/catalog") {
        let catalog = catalog(app_root, at).map_err(|m| refused("purchases-catalog", m))?;
        for i in 0..plan.modules.len() {
            if plan.str(plan.modules[i].extension) != "purchases" {
                continue;
            }
            let mut sub: serde_json::Value =
                serde_json::from_str(plan.str(plan.modules[i].substitute)).unwrap_or_default();
            sub["catalog"] = catalog.clone();
            plan.set_module_substitute(i, &sub.to_string());
        }
    }
    if let Some(intents) = manifest.json.get("intents") {
        faces(intents, plan).map_err(|(id, m)| refused(id, m))?;
    }
    plan.app_id = manifest.id;
    Ok(())
}

/// `purchases.catalog`'s file, beside `app.json`: a list of `ItemDetails`,
/// each with its `itemId`, `title`, `type` and `price`, ids unique.
fn catalog(app_root: &Path, at: &Value) -> Result<Value, String> {
    let file = at
        .as_str()
        .ok_or("`purchases.catalog` names a JSON file beside app.json")?;
    let text = std::fs::read_to_string(app_root.join(file))
        .map_err(|e| format!("`purchases.catalog`: {file}: {e}"))?;
    let items: Value =
        serde_json::from_str(&text).map_err(|e| format!("`purchases.catalog`: {file}: {e}"))?;
    let list = items
        .as_array()
        .ok_or_else(|| format!("`purchases.catalog`: {file} is a list of ItemDetails"))?;
    let mut ids = std::collections::BTreeSet::new();
    for item in list {
        let id = item["itemId"].as_str().unwrap_or_default();
        let typed = matches!(item["type"].as_str(), Some("product" | "subscription"));
        let priced = item["price"]["currency"].is_string() && item["price"]["value"].is_string();
        if id.is_empty() || !item["title"].is_string() || !typed || !priced || !ids.insert(id) {
            return Err(format!("`purchases.catalog`: {file}: each item has a unique `itemId`, a `title`, a `type` (product or subscription) and a `price` {{currency, value}}; `{item}` does not"));
        }
    }
    Ok(items)
}

/// The doors a face reaches when it names none (§9's cells).
const DOORS: [&str; 3] = ["apple", "android", "web"];

/// Every door a face may name: the defaults, and the local agents' door on
/// the Mac, offered only where named (LLP 1111.000 D1).
const NAMED: [&str; 4] = ["apple", "android", "web", "mcp"];

/// Whether type text `ty` (`faces::type_text`) is a string, number or bool.
fn scalar(ty: &str) -> bool {
    matches!(ty, "string" | "number" | "bool") || ty.starts_with("enum<")
}

/// Check each of `intents.tools`' faces against its tool and store it on
/// the tool's row, with the doors it reaches spelled out and its digest.
fn faces(intents: &Value, plan: &mut Plan) -> Result<(), (&'static str, String)> {
    let kinds = intents.get("entities").and_then(Value::as_object);
    let Some(tools) = intents.get("tools").and_then(Value::as_object) else {
        return Ok(());
    };
    for (name, face) in tools {
        let Some(i) = (plan.tools.iter()).position(|t| plan.str(t.name) == name) else {
            return Err(("intents-tool", format!("intents.tools names `{name}`, and the app has no `tool {name}` line (LLP 1133.023 D8)")));
        };
        let signature = crate::faces::tool_signature(plan, i);
        let destroys = signature["consequence"] == "destroys";
        let doors: Vec<String> = match face.get("doors").and_then(Value::as_array) {
            Some(doors) => doors
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            None if destroys => vec!["apple".into()],
            None => DOORS.map(str::to_owned).to_vec(),
        };
        if let Some(door) = doors.iter().find(|d| !NAMED.contains(&d.as_str())) {
            return Err(("intents-door", format!("tool `{name}`'s face names door `{door}`; the doors are apple, android, web and mcp")));
        }
        // The Mac draws the mcp door's confirmation (LLP 1111.000 D5).
        if let Some(door) = doors
            .iter()
            .find(|d| destroys && !["apple", "mcp"].contains(&d.as_str()))
        {
            return Err(("intents-destroys-door", format!("tool `{name}` destroys, and the {door} door has no confirmation the person answers, so its face may not name it (LLP 1133.023 D10)")));
        }
        // A face reaching only local agents keeps the plan's types; App
        // Intents, AppFunctions and WebMCP's faces take scalars (LLP 1111.000 D1).
        let scalars = doors.iter().any(|d| DOORS.contains(&d.as_str()));
        // WebMCP's limits (https://webmachinelearning.github.io/webmcp/);
        // App Intents takes an empty `IntentDescription` and any name. A
        // `tool` line's name is already within WebMCP's characters.
        if doors.iter().any(|d| d == "web") {
            let description = signature["description"].as_str().unwrap_or_default();
            if description.trim().is_empty() {
                return Err(("intents-web-description", format!("tool `{name}`'s face reaches the web door, and its description is blank: write what the tool does between its `tool` line's quotes, which an agent reads to choose it (WebMCP §4.2 requires a description), or leave web out of the face's `doors`")));
            }
            let length = name.chars().count();
            if length > 128 {
                return Err(("intents-web-name", format!("tool `{name}`'s face reaches the web door, and its name is {length} characters: a WebMCP tool name is at most 128 (WebMCP §3, tool definition), so shorten the `tool` line's name, or leave web out of the face's `doors`")));
            }
        }
        let declared = face.get("parameters").and_then(Value::as_object);
        let params = signature["parameters"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for p in &params {
            let (pname, ty) = (
                p[0].as_str().unwrap_or_default(),
                p[1].as_str().unwrap_or_default(),
            );
            let said = declared.and_then(|d| d.get(pname));
            let entity = said.and_then(|s| s.get("entity")).and_then(Value::as_str);
            let face_type = said.and_then(|s| s.get("type")).and_then(Value::as_str);
            let wrong = match (ty, entity, face_type) {
                ("string", None, Some("file")) => None,
                (_, _, Some("file")) => {
                    Some("`type: file` needs a string parameter with no entity")
                }
                (_, _, Some(t)) if t != "integer" || ty != "number" => {
                    Some("`type` is `integer` on a number, or `file` on a string")
                }
                ("string", Some(kind), _) if !kinds.is_some_and(|k| k.contains_key(kind)) => {
                    Some("its entity kind is not in intents.entities")
                }
                (_, Some(_), _) if ty != "string" => Some("only a string may name an entity"),
                _ if scalars && !scalar(ty) => Some("a face that names the apple, android or web door takes a string, number or bool; one that names only mcp takes any type"),
                _ => None,
            };
            if let Some(why) = wrong {
                return Err((
                    "intents-face-type",
                    format!("tool `{name}`'s parameter `{pname}` ({ty}): {why} (LLP 1133.023 D5)"),
                ));
            }
        }
        for p in declared.into_iter().flat_map(|d| d.values()) {
            if !p.is_object() {
                return Err((
                    "intents-face-type",
                    format!("tool `{name}`: a face parameter must be an object"),
                ));
            }
            if p.get("type").and_then(Value::as_str) == Some("file") {
                let types: Value =
                    serde_json::from_str(include_str!("../../../scripts/document-utis.json"))
                        .expect("document UTI table");
                let accepts = p["accept"].as_array();
                if !accepts.is_some_and(|a| {
                    !a.is_empty()
                        && a.iter().all(|v| {
                            v.as_str()
                                .is_some_and(|m| m != "inode/directory" && types.get(m).is_some())
                        })
                }) {
                    return Err(("intents-file-accept", format!("tool `{name}`: a file parameter needs a non-empty `accept` list of MIME types in scripts/document-utis.json; directories are not files")));
                }
                if let Some(door) = doors.iter().find(|d| *d != "apple") {
                    return Err(("intents-file-door", format!("tool `{name}`: the {door} door has no file handoff (AppFunctions is not built; WebMCP takes JSON arguments); set this face's `doors` to [\"apple\"]")));
                }
            } else if p.get("accept").is_some() {
                return Err((
                    "intents-file-accept",
                    format!("tool `{name}`: `accept` belongs only to a `type: file` parameter"),
                ));
            }
        }
        if let Some(extra) =
            declared.and_then(|d| d.keys().find(|k| !params.iter().any(|p| p[0] == **k)))
        {
            return Err(("intents-face-type", format!("tool `{name}`'s face names parameter `{extra}`, which its action does not take")));
        }
        if let Some(ty) = signature["returns"]
            .as_str()
            .filter(|t| scalars && !scalar(t))
        {
            return Err(("intents-face-type", format!("tool `{name}` returns {ty}; the tool of a face that names the apple, android or web door returns a string, number or bool (LLP 1133.023 D5), and one that names only mcp may return any type (LLP 1111.000 D1)")));
        }
        let mut face = face.clone();
        face["doors"] = json!(doors);
        face["digest"] = json!(crate::faces::face_digest(plan, i, &face));
        plan.set_tool_face(i, &face.to_string());
    }
    Ok(())
}

/// Whether the root component's outermost node hears `navigate`.
fn navigation_root(file: &File) -> bool {
    let Some(root) = file.components.first() else {
        return false;
    };
    (root.view.iter()).any(|n| match n {
        Node::Element { attrs, .. } => attrs.iter().any(|a| a.name == "navigate"),
        _ => false,
    })
}
