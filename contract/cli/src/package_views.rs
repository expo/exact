//! The views packages serve (LLP 1133.001 D1, LLP 1133.008 D8 item 1). A
//! tag is declared, not imported: the bake finds it in the `exact.json` of
//! each package the app depends on directly, as it finds the app's own in
//! `app.json` `modules`. An interface declares each view, its props and
//! events, whether it hears that it is displayed (`"displayed": true`) and
//! which props bind a device (`"binds": true`); an implementation lists the
//! tags it serves, or declares a view of its own the same way.
//!
//! The app's direct dependencies are the `dependencies` of the nearest
//! `package.json` above it: its own for an app `exact new` made, the
//! repository's for an app in it.

use contract_syntax::{Attr, Expr, Span};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// One package view, as the plan's `views` table carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageView {
    pub tag: String,
    /// The extension whose implementation makes it in each session: the
    /// interface's name after its scope.
    pub extension: String,
    /// The full interface identity.
    pub package: String,
    pub displayed: bool,
    pub binds: Vec<String>,
    /// The interface's agent vocabulary, including interfaces with no names.
    pub agent: String,
    /// Its declared props' types and its events beyond the nine, which the
    /// lowering holds the tag's attributes to (LLP 1133.021 §9 items 1, 2).
    pub declared: contract_lower::DeclaredView,
}

/// Tags in the lowered program, including deferred branches but excluding
/// components never instantiated. Declarations alone do not link a view.
pub(crate) fn placed(plan: &exact_plan::Plan) -> BTreeSet<String> {
    use exact_plan::{BindingKind, Opcode, StrId};
    plan.nodes
        .iter()
        .flat_map(|n| n.bindings.iter())
        .filter_map(|id| {
            let binding = plan.binding(id);
            if binding.kind != BindingKind::Prop
                || binding.id != exact_kernel::PropId::NativeViewModuleName as u16
            {
                return None;
            }
            match exact_runner::vm::instructions(plan.read_code(plan.code(binding.expr)).0)
                .collect::<Result<Vec<_>, _>>()
                .as_deref()
            {
                Ok([value, end]) if value.op == Opcode::Str && end.op == Opcode::Return => {
                    Some(plan.str(StrId(value.args[0] as u32)).to_owned())
                }
                _ => None,
            }
        })
        .collect()
}

pub struct DeclarationError {
    pub id: &'static str,
    pub message: String,
}

/// Every view the app's direct dependencies declare, by tag. Layout prop
/// names and two packages declaring one tag are refused.
pub fn declared(app_root: &Path) -> Result<BTreeMap<String, PackageView>, DeclarationError> {
    let mut out: BTreeMap<String, PackageView> = BTreeMap::new();
    for name in dependencies(app_root) {
        let Some((root, json)) = located(app_root, &name) else {
            continue;
        };
        let mut found = Vec::new();
        match json["kind"].as_str() {
            Some("interface") => {
                let extension = extension_of(json["name"].as_str().unwrap_or(&name));
                found.extend(views(
                    &json,
                    &extension,
                    json["name"].as_str().unwrap_or(&name),
                    &json["agent"],
                )?);
                for implementation in carried(&root) {
                    found.extend(views(
                        &implementation,
                        &extension,
                        json["name"].as_str().unwrap_or(&name),
                        &json["agent"],
                    )?);
                }
            }
            Some("implementation") => {
                let implements = json["implements"].as_str().unwrap_or_default();
                let interface = implements.rsplit_once('@').map_or(implements, |(i, _)| i);
                let interface = if interface.is_empty() {
                    implements
                } else {
                    interface
                };
                let declaration = located(app_root, interface)
                    .map(|(_, json)| json)
                    .unwrap_or_default();
                let agent = &declaration["agent"];
                let extension = extension_of(interface);
                let shared = views(&declaration, &extension, interface, agent)?;
                for view in views(&json, &extension, interface, agent)? {
                    // A served tag takes its interface's declaration regardless
                    // of which dependency is read first.
                    found.push(
                        shared
                            .iter()
                            .find(|decl| decl.tag == view.tag)
                            .cloned()
                            .unwrap_or(view),
                    );
                }
            }
            _ => {}
        }
        for view in found {
            match out.get(&view.tag) {
                // An interface's tag its implementation lists is the one view.
                Some(seen) if seen.package == view.package => {}
                Some(seen) => {
                    return Err(DeclarationError {
                        id: "bake-view-twice",
                        message: format!(
                            "`{}` is declared by {} and by {}: a tag is one per document, so the app depends on one of them",
                            view.tag, seen.package, view.package
                        ),
                    })
                }
                None => {
                    out.insert(view.tag.clone(), view);
                }
            }
        }
    }
    Ok(out)
}

/// The tag a standard module serves, for the line `exact add` writes, and
/// that module's short name.
pub fn standard_with(tag: &str) -> Option<String> {
    let packages = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages");
    let mut names: Vec<_> = std::fs::read_dir(packages).ok()?.flatten().collect();
    names.sort_by_key(|e| e.file_name());
    names.into_iter().find_map(|entry| {
        let text = std::fs::read_to_string(entry.path().join("exact.json")).ok()?;
        let json: Value = serde_json::from_str(&text).ok()?;
        (json["kind"] == "interface" && json["views"].get(tag).is_some())
            .then(|| entry.file_name().to_string_lossy().into_owned())
    })
}

fn extension_of(package: &str) -> String {
    package.rsplit('/').next().unwrap_or(package).to_owned()
}

/// The views `json` declares: an object of declarations, or a list of the
/// tags it serves, which declare nothing of their own.
fn views(
    json: &Value,
    extension: &str,
    package: &str,
    agent: &Value,
) -> Result<Vec<PackageView>, DeclarationError> {
    let agent = crate::declaration::agent_words(agent).map_err(|message| DeclarationError {
        id: "contract-use-declaration",
        message: format!("{package}: {message}"),
    })?;
    let made = |tag: &str, decl: Option<&Map<String, Value>>| {
        Ok(PackageView {
            tag: tag.to_owned(),
            extension: extension.to_owned(),
            package: package.to_owned(),
            agent: agent.clone(),
            displayed: decl.is_some_and(|d| d.get("displayed") == Some(&Value::Bool(true))),
            binds: decl
                .and_then(|d| d.get("props"))
                .and_then(Value::as_object)
                .map(|props| {
                    (props.iter())
                        .filter(|(_, p)| p["binds"] == true)
                        .map(|(name, _)| name.clone())
                        .collect()
                })
                .unwrap_or_default(),
            declared: contract_lower::DeclaredView {
                extension: extension.to_owned(),
                ..decl
                    .map(|d| declared_view(d, tag, package))
                    .transpose()?
                    .unwrap_or_default()
            },
        })
    };
    match &json["views"] {
        Value::Object(map) => map
            .iter()
            .filter(|(tag, _)| contract_lower::is_module_tag(tag))
            .map(|(tag, decl)| made(tag, decl.as_object()))
            .collect(),
        Value::Array(tags) => (tags.iter())
            .filter_map(Value::as_str)
            .filter(|tag| contract_lower::is_module_tag(tag))
            .map(|tag| made(tag, None))
            .collect(),
        _ => Ok(Vec::new()),
    }
}

/// A view declaration's props, each `"type"` or `{"type": …}`, and its
/// events written `{"event": name, "args": [types]}`.
fn declared_view(
    decl: &Map<String, Value>,
    tag: &str,
    package: &str,
) -> Result<contract_lower::DeclaredView, DeclarationError> {
    let mut props = BTreeMap::new();
    for (name, p) in decl
        .get("props")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        // Resolve shorthands through the attribute table; layout membership
        // comes from the schema's generated StyleIds. A string selects CSS
        // where an attribute name can also name a handler (`resize`).
        let attr = Attr {
            name: name.clone(),
            value: Expr::Str(String::new(), Span::default()),
            span: Span::default(),
            written: true,
        };
        if contract_lower::tags::rows(&attr)
            .iter()
            .any(|row| row.affects_layout())
        {
            return Err(DeclarationError {
                id: "bake-view-prop-layout",
                message: format!(
                    "{package} declares prop `{name}` for view `{tag}`, but `{name}` is a layout attribute of the box; give the view prop another name"
                ),
            });
        }
        if let Some(ty) = p.as_str().or_else(|| p["type"].as_str()) {
            props.insert(name.clone(), ty.to_owned());
        }
    }
    let events = (decl
        .get("events")
        .and_then(Value::as_array)
        .into_iter()
        .flatten())
    .filter_map(|e| {
        let name = e["event"].as_str()?;
        let args = (e["args"].as_array().into_iter().flatten())
            .filter_map(|a| a.as_str().map(str::to_owned))
            .collect();
        Some((name.to_owned(), args))
    })
    .collect();
    Ok(contract_lower::DeclaredView {
        props,
        events,
        ..Default::default()
    })
}

/// The nearest `package.json` above `app_root`: its `dependencies`' names.
pub(crate) fn dependencies(app_root: &Path) -> Vec<String> {
    let Some(text) = app_root
        .ancestors()
        .find_map(|dir| std::fs::read_to_string(dir.join("package.json")).ok())
    else {
        return Vec::new();
    };
    let json: Value = serde_json::from_str(&text).unwrap_or_default();
    (json["dependencies"].as_object())
        .map(|deps| deps.keys().cloned().collect())
        .unwrap_or_default()
}

/// Package `name` installed above `app_root`: its directory and the
/// `exact.json` its `exports` `"."` names; `None` for a package that is not a
/// module (a Contract library, a JavaScript one).
pub(crate) fn located(app_root: &Path, name: &str) -> Option<(PathBuf, Value)> {
    let root = app_root
        .ancestors()
        .map(|a| a.join("node_modules").join(name))
        .find(|candidate| candidate.join("package.json").is_file())?;
    let manifest = read(&root.join("package.json"))?;
    let main = export(&manifest["exports"], ".")?;
    if !main.ends_with("exact.json") {
        return None;
    }
    Some((root.clone(), read(&root.join(main))?))
}

/// The implementations an interface's own package carries (its `exports`
/// subpaths that name an implementation's `exact.json`).
fn carried(root: &Path) -> Vec<Value> {
    let Some(manifest) = read(&root.join("package.json")) else {
        return Vec::new();
    };
    let Some(exports) = manifest["exports"].as_object() else {
        return Vec::new();
    };
    (exports.keys())
        .filter(|k| k.as_str() != ".")
        .filter_map(|k| export(&manifest["exports"], k))
        .filter(|at| at.ends_with("/exact.json"))
        .filter_map(|at| read(&root.join(at)))
        .filter(|json| json["kind"] == "implementation")
        .collect()
}

pub(crate) fn export<'a>(exports: &'a Value, key: &str) -> Option<&'a str> {
    let target = match exports {
        Value::String(s) if key == "." => return Some(s),
        Value::Object(map) => map.get(key)?,
        _ => return None,
    };
    match target {
        Value::String(s) => Some(s),
        Value::Object(t) => t.get("contract").or(t.get("default"))?.as_str(),
        _ => None,
    }
}

pub(crate) fn read(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}
