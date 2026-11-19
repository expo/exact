use super::{BakeError, CompileError};
use contract_syntax::Span;
use exact_plan::Plan;
use std::{collections::BTreeMap, path::Path};

pub(super) fn arguments(
    app_root: &Path,
) -> Result<Option<BTreeMap<String, Vec<String>>>, CompileError> {
    let path = app_root.join(".shells/surfaces.json");
    let refusal = |message| CompileError {
        pass: "analyze",
        id: "analyze-surface-declaration".into(),
        message,
        span: Span::default(),
        file: Some(path.as_path().into()),
        related: Box::new([]),
    };
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(refusal(e.to_string())),
    };
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| refusal(e.to_string()))?;
    let declarations = json.as_object().and_then(|surfaces| {
        surfaces
            .iter()
            .map(|(name, args)| {
                let fields: Option<Vec<_>> = args
                    .as_array()?
                    .iter()
                    .map(|arg| Some(arg.get("name")?.as_str()?.to_owned()))
                    .collect();
                Some((name.clone(), fields?))
            })
            .collect()
    });
    declarations.map(Some).ok_or_else(|| {
        refusal(
            "expected surface names mapped to argument declarations with a `name` string".into(),
        )
    })
}

/// The keys each surface's `Actions` bind (`.shells/keys.json`, written by
/// the game's last GPU build beside `surfaces.json`), each with the actions
/// it drives; empty without one. It feeds a warning only, so an unreadable
/// file is none.
pub(super) fn keys(app_root: &Path) -> BTreeMap<String, BTreeMap<String, Vec<String>>> {
    std::fs::read(app_root.join(".shells/keys.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// The game Rust newer than `.shells/surfaces.json`, when some is: the
/// declaration is the last GPU build's, so it may predate an `Options` edit
/// (the platformer's diary, R4). Content-stable writes keep an unchanged
/// declaration's time, so newer Rust only says it may be stale.
pub(super) fn newer_rust(app_root: &Path) -> Option<std::path::PathBuf> {
    newer_rust_than(app_root, ".shells/surfaces.json")
}

/// The game Rust newer than the declaration at `declaration` (relative to
/// the app's root), when some is.
pub(super) fn newer_rust_than(app_root: &Path, declaration: &str) -> Option<std::path::PathBuf> {
    fn newest(dir: &Path, best: &mut Option<(std::time::SystemTime, std::path::PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                newest(&path, best);
            } else if path.extension().is_some_and(|e| e == "rs") || path.ends_with("Cargo.toml") {
                if let Ok(at) = entry.metadata().and_then(|m| m.modified()) {
                    if best.as_ref().is_none_or(|(t, _)| at > *t) {
                        *best = Some((at, path));
                    }
                }
            }
        }
    }
    let declared = std::fs::metadata(app_root.join(declaration))
        .and_then(|m| m.modified())
        .ok()?;
    let mut best = None;
    newest(&app_root.join("logic"), &mut best);
    best.filter(|(at, _)| *at > declared).map(|(_, path)| path)
}

/// Surface records are runner-owned and default before the module loads.
pub(super) fn shape(plan: &Plan) -> Result<(), BakeError> {
    use exact_runner::surface_record::{surface_name, SOURCE};
    for row in plan.resources.iter().filter(|r| {
        matches!(
            plan.str(r.source),
            SOURCE | exact_runner::canvas_performance::SOURCE
        )
    }) {
        let name = plan.str(row.name);
        let source = plan.str(row.source);
        let fail = |message: String| BakeError::Lint {
            site: None,
            id: "bake-surface-record",
            message: format!("resource `{name}`: {message}"),
        };
        let surface = surface_name(plan, row).ok_or_else(|| {
            fail(format!(
                "{source} takes exactly one string-literal surface name"
            ))
        })?;
        if plan.type_(row.ty).kind != exact_plan::TypeKind::Record {
            return Err(fail(format!("{source} must be `as shape` a record")));
        }
        if source == exact_runner::canvas_performance::SOURCE {
            exact_runner::canvas_performance::validate(plan, row.ty).map_err(fail)?;
        }
        if !plan.surfaces.iter().any(|s| plan.str(s.name) == surface) {
            return Err(fail(format!("surface `{surface}` is not used by a canvas")));
        }
    }
    Ok(())
}

/// The runner's own sources (LLP 1030 D7 delivery; LLP 1039 D1 viewport;
/// LLP 1069.000 D2 page) are answered by the runner, not by the data crate,
/// so the fields each can fill are a closed set — a declared field it does
/// not know would refuse every boot on every device, which is a build-time
/// refusal here instead, naming the field.
pub(super) fn fact_shape(
    plan: &Plan,
    source: &str,
    fields: &[&str],
    id: &'static str,
) -> Result<(), BakeError> {
    for row in plan.resources.iter() {
        if plan.str(row.source) != source {
            continue;
        }
        let name = plan.str(row.name);
        let ty = plan.type_(row.ty);
        if ty.kind != exact_plan::TypeKind::Record {
            return Err(BakeError::Lint {
                site: None,
                id,
                message: format!(
                    "`resource {name} = {source}()` must be `as shape` a record of {}",
                    fields.join(", ")
                ),
            });
        }
        for f in ty.fields.iter() {
            let field = plan.str(plan.field(f).name);
            if !fields.contains(&field) {
                return Err(BakeError::Lint {
                    site: None,
                    id,
                    message: format!(
                        "`{name}` declares `{field}`, which {source} does not answer; it answers {}",
                        fields.join(", ")
                    ),
                });
            }
        }
    }
    Ok(())
}

/// The geometry facts' declared shapes (LLP 1123 D6): each field one the source answers, and
/// every answer it can give fillable, so a nested field it lacks or a quantity it may not have
/// in a field that is not an `option` is refused here, naming it, not dropped at run time.
pub(super) fn geometry_shapes(plan: &Plan) -> Result<(), BakeError> {
    use exact_runner::native_geometry as geometry;
    for (source, fields) in geometry::SOURCES {
        fact_shape(plan, source, fields, "bake-geometry-field")?;
        let templates = geometry::templates(source);
        for row in plan
            .resources
            .iter()
            .filter(|r| plan.str(r.source) == source)
        {
            for template in &templates {
                if let Err(e) = geometry::project(plan, row.ty, template, source) {
                    return Err(BakeError::Lint {
                        site: None,
                        id: "bake-geometry-field",
                        message: format!("`resource {}`'s shape: {e}", plan.str(row.name)),
                    });
                }
            }
        }
    }
    Ok(())
}
