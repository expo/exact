//! Package GPU declarations (LLP 1153.003 D5): the same names feed the
//! generated registry, the canvas arity check and the host's module router.
use crate::package_views::{dependencies, located};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// The hosts that draw GPU surfaces (LLP 1153.003 §3): what a package's
/// `targets` may name, and its default. The watch draws none.
const GPU_PLATFORMS: [&str; 7] = ["web", "macos", "ios", "tvos", "android", "linux", "windows"];

struct PackageGpu {
    package: String,
    name: String,
    surfaces: BTreeMap<String, u64>,
    targets: Option<Vec<String>>,
}

fn declared(app: &Path, manifest: &Value) -> Result<Vec<PackageGpu>, String> {
    let mut modules = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for (name, surfaces) in manifest["gpu"]["modules"].as_object().into_iter().flatten() {
        modules.insert(name.clone(), format!("gpu.modules.{name}"));
        for surface in surfaces
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            owners.insert(surface.to_owned(), format!("gpu.modules.{name}"));
        }
    }
    let mut out = Vec::new();
    for package in dependencies(app) {
        let Some((_, declaration)) = located(app, &package) else {
            continue;
        };
        let Some(gpu) = declaration.get("gpu") else {
            continue;
        };
        let fail = |why: &str| format!("{package}: exact.json gpu {why}");
        crate::gpu_requirements::check(gpu).map_err(|why| fail(&why))?;
        let krate = gpu["crate"]
            .as_str()
            .ok_or_else(|| fail("requires a crate directory"))?;
        if krate.is_empty()
            || Path::new(krate)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(fail("crate must be a directory inside the package"));
        }
        if gpu["exactGpu"].as_str().is_none_or(str::is_empty) {
            return Err(fail("requires an exactGpu version requirement"));
        }
        let name = package
            .trim_start_matches('@')
            .replace(['/', '_', '.'], "-");
        if !name.split('-').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        }) {
            return Err(fail("package name must form a lowercase module slug"));
        }
        if let Some(other) = modules.insert(name.clone(), package.clone()) {
            return Err(format!(
                "GPU module {name} is claimed by both {other} and {package}"
            ));
        }
        let surfaces = gpu["surfaces"]
            .as_object()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| fail("surfaces must declare at least one surface"))?;
        let mut arities = BTreeMap::new();
        for (surface, spec) in surfaces {
            let identifier = |s: &str| {
                !s.is_empty()
                    && s.bytes().enumerate().all(|(i, b)| {
                        b == b'_' || b.is_ascii_alphabetic() || i > 0 && b.is_ascii_digit()
                    })
            };
            if !identifier(surface) {
                return Err(fail("surface names must be identifiers"));
            }
            let factory = spec["factory"]
                .as_str()
                .filter(|s| s.split("::").count() >= 2 && s.split("::").all(identifier));
            if factory.is_none() {
                return Err(fail(
                    "each surface needs a Rust factory path, crate::function",
                ));
            }
            let arity = spec["arity"]
                .as_u64()
                .filter(|n| *n <= u32::MAX as u64)
                .ok_or_else(|| fail("each surface needs a nonnegative integer arity"))?;
            if let Some(other) = owners.insert(surface.clone(), package.clone()) {
                return Err(format!(
                    "GPU surface {surface} is claimed by both {other} and {package}"
                ));
            }
            arities.insert(surface.clone(), arity);
        }
        let targets = gpu
            .get("targets")
            .map(|v| {
                v.as_array()
                    .ok_or_else(|| fail("targets must be an array"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .filter(|s| GPU_PLATFORMS.contains(s))
                            .map(str::to_owned)
                            .ok_or_else(|| {
                                fail(&format!(
                                    "targets may name only {}",
                                    GPU_PLATFORMS.join(", ")
                                ))
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        out.push(PackageGpu {
            package,
            name,
            surfaces: arities,
            targets,
        });
    }
    Ok(out)
}

/// The surface ownership table for a host, combining app and package modules.
pub fn modules(app: &Path, manifest: &Value, platform: &str) -> Result<Value, String> {
    if !GPU_PLATFORMS.contains(&platform) {
        return Ok(json!({}));
    }
    let mut modules = manifest["gpu"]["modules"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for gpu in declared(app, manifest)? {
        if gpu
            .targets
            .as_ref()
            .is_none_or(|t| t.iter().any(|p| p == platform))
        {
            modules.insert(gpu.name, json!(gpu.surfaces.keys().collect::<Vec<_>>()));
        }
    }
    Ok(Value::Object(modules))
}

pub(crate) fn check(app: &Path, plan: &exact_plan::Plan) -> Result<(), crate::CompileError> {
    let manifest = crate::package_views::read(&app.join("app.json")).unwrap_or_default();
    let error = |id: &str, message: String| crate::CompileError {
        pass: "analyze",
        id: id.into(),
        message,
        span: Default::default(),
        file: Some(app.join("app.contract").into()),
        related: Box::new([]),
    };
    for gpu in declared(app, &manifest).map_err(|m| error("bake-gpu-declaration", m))? {
        for surface in &plan.surfaces {
            let name = plan.str(surface.name);
            if let Some(arity) = gpu.surfaces.get(name) {
                if surface.mode == exact_plan::SurfaceArgsMode::Positional
                    && u64::from(surface.args.len) > *arity
                {
                    return Err(error("analyze-surface-arity", format!("surface `{name}` from {} accepts at most {arity} positional arguments, got {}", gpu.package, surface.args.len)));
                }
            }
        }
    }
    Ok(())
}
