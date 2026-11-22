//! Package GPU declarations (LLP 1153.003 D5): the same names feed the
//! generated registry, the canvas arity check and the host's module router.
use crate::package_views::{dependencies, located};
use contract_syntax::{Expr, Node, Span};
use exact_gpu_reflect::naga::{self, AddressSpace, Binding, ShaderStage, TypeInner};
use exact_shader_uniforms::{self as uniforms, Layout};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Component, Path};

/// The hosts that draw GPU surfaces (LLP 1153.003 §3): what a package's
/// `targets` may name, and its default. The watch draws none.
const GPU_PLATFORMS: [&str; 7] = ["web", "macos", "ios", "tvos", "android", "linux", "windows"];

struct PackageGpu {
    package: String,
    name: String,
    surfaces: BTreeMap<String, Surface>,
    targets: Option<Vec<String>>,
}

struct Surface {
    arity: u64,
    wgsl: Option<String>,
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
            let wgsl = spec
                .get("wgsl")
                .map(|v| {
                    v.as_str()
                        .filter(|s| identifier(s))
                        .map(str::to_owned)
                        .ok_or_else(|| fail("a surface's wgsl must name an argument"))
                })
                .transpose()?;
            arities.insert(surface.clone(), Surface { arity, wgsl });
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

pub(crate) fn check(app: &Path, nodes: &[Node]) -> Result<(), Vec<crate::CompileError>> {
    let manifest = crate::package_views::read(&app.join("app.json")).unwrap_or_default();
    let error = |id: &str, message: String| crate::CompileError {
        pass: "analyze",
        id: id.into(),
        message,
        span: Default::default(),
        file: None,
        related: Box::new([]),
    };
    let mut errors = Vec::new();
    for gpu in declared(app, &manifest).map_err(|m| vec![error("bake-gpu-declaration", m)])? {
        for node in Node::walk(nodes) {
            let Node::Element {
                tag, attrs, span, ..
            } = node
            else {
                continue;
            };
            for attr in attrs
                .iter()
                .filter(|a| tag == "canvas" && a.name == "surface")
            {
                let Expr::Call(name, args, _) = &attr.value else {
                    continue;
                };
                let Some(surface) = gpu.surfaces.get(name) else {
                    continue;
                };
                let named = args.iter().any(|a| matches!(a, Expr::NamedArg(..)));
                if !named && args.len() as u64 > surface.arity {
                    let mut e = error("analyze-surface-arity", format!("surface `{name}` from {} accepts at most {} positional arguments, got {}", gpu.package, surface.arity, args.len()));
                    e.span = *span;
                    errors.push(e);
                }
                if let Some(argument) = &surface.wgsl {
                    let src = if named {
                        args.iter().find_map(|a| match a {
                            Expr::NamedArg(n, value, _) if n == argument => Some(&**value),
                            _ => None,
                        })
                    } else {
                        args.first()
                    };
                    if let Some(Expr::Str(src, _)) = src {
                        match check_shader(app, src, argument, args, *span) {
                            Ok(findings) => errors.extend(findings),
                            Err(e) => errors.push(e),
                        }
                    }
                }
            }
        }
    }
    errors.truncate(crate::MAX_DIAGNOSTICS);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn shader_error(path: Option<&Path>, span: Span, message: String) -> crate::CompileError {
    crate::CompileError {
        pass: "analyze",
        id: "analyze-surface-shader".into(),
        message,
        span,
        file: path.map(Into::into),
        related: Box::new([]),
    }
}

fn wgsl_span(source: &str, offset: usize) -> Span {
    let prefix = &source[..offset];
    Span::point(
        prefix.bytes().filter(|b| *b == b'\n').count() as u32 + 1,
        (offset - prefix.rfind('\n').map_or(0, |i| i + 1)) as u32 + 1,
    )
}

// Naga retains spans for types and globals, but not entry-point declarations.
fn entry_span(source: &str, name: &str) -> Span {
    let word = |c: char| {
        c == '_'
            || (!c.is_ascii_punctuation()
                && !c.is_whitespace()
                && !matches!(c, '\u{200e}' | '\u{200f}'))
    };
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < source.len() {
        if source[i..].starts_with("//") {
            i += source[i..].find('\n').unwrap_or(source.len() - i);
        } else if source[i..].starts_with("/*") {
            i += 2;
            let mut depth = 1;
            while depth > 0 && i < source.len() {
                if source[i..].starts_with("/*") {
                    depth += 1;
                    i += 2;
                } else if source[i..].starts_with("*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += source[i..].chars().next().unwrap().len_utf8();
                }
            }
        } else {
            let start = i;
            let c = source[i..].chars().next().unwrap();
            i += c.len_utf8();
            if word(c) {
                while i < source.len() {
                    let c = source[i..].chars().next().unwrap();
                    if !word(c) {
                        break;
                    }
                    i += c.len_utf8();
                }
                tokens.push((&source[start..i], start));
            }
        }
    }
    tokens
        .windows(2)
        .find_map(|pair| {
            (pair[0].0 == "fn" && pair[1].0 == name).then(|| wgsl_span(source, pair[1].1))
        })
        .unwrap_or(Span::point(1, 1))
}

fn check_shader(
    app: &Path,
    src: &str,
    source_argument: &str,
    args: &[Expr],
    canvas: Span,
) -> Result<Vec<crate::CompileError>, crate::CompileError> {
    let fail = |message| shader_error(None, canvas, message);
    let Some(asset) = src.strip_prefix("assets/").filter(|s| {
        !s.is_empty()
            && Path::new(s)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
    }) else {
        return Err(fail(format!(
            "shader src `{src}` must name a file under assets/"
        )));
    };
    let path = app.join("assets").join(asset);
    let source =
        std::fs::read_to_string(&path).map_err(|e| fail(format!("shader src `{src}`: {e}")))?;
    let layout = uniforms::parse(&source).map_err(|e| {
        let at = e
            .location
            .map_or(Span::point(1, 1), |at| wgsl_span(&source, at.offset));
        shader_error(Some(&path), at, e.message)
    });
    let mut module = exact_gpu_reflect::inspect(&source).map_err(|e| {
        if e.validation {
            if let Err(member) = &layout {
                return member.clone();
            }
        }
        let at = e.location.map_or(Span::point(1, 1), |at| {
            let mut span = Span::point(at.line_number, at.line_position);
            span.end_col += at.length;
            span
        });
        shader_error(Some(&path), at, e.message)
    })?;
    let layout = layout?;
    check_interface(&module, &source)
        .map_err(|(span, message)| shader_error(Some(&path), span, message))?;
    check_layout(&path, &source, &module, &layout)?;
    if module.overrides.iter().any(|(_, o)| o.init.is_none()) {
        // The surface supplies no pipeline constants. Only overrides the fragment uses need defaults.
        naga::compact::compact(&mut module, naga::compact::KeepUnused::No);
        if let Some((handle, missing)) = module.overrides.iter().find(|(_, o)| o.init.is_none()) {
            let at = module.overrides.get_span(handle).location(&source);
            let name = missing.name.as_deref().unwrap_or("<unnamed>");
            return Err(shader_error(
                Some(&path),
                Span::point(at.line_number, at.line_position),
                format!("shader override `{name}` needs a default value"),
            ));
        }
    }
    let mut findings = Vec::new();
    for arg in args {
        let Expr::NamedArg(name, value, _) = arg else {
            continue;
        };
        if name == source_argument || uniforms::ARGUMENTS.contains(&name.as_str()) {
            continue;
        }
        if !layout.members.iter().any(|m| m.name == *name) {
            let members = layout
                .members
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            findings.push(fail(format!(
                "unknown uniform argument `{name}`; members: [{members}]"
            )));
            continue;
        }
        if let Some(value) = literal(value) {
            if let Err(message) = check_value(&layout, name, &value) {
                findings.push(fail(message));
            }
        }
    }
    Ok(findings)
}

fn check_interface(module: &naga::Module, source: &str) -> Result<(), (Span, String)> {
    let fail = |name: &str, message| (entry_span(source, name), message);
    for entry in &module.entry_points {
        if entry.stage != ShaderStage::Fragment {
            return Err(fail(
                &entry.name,
                format!(
                    "entry `{}` is {:?}; expected one @fragment entry only",
                    entry.name, entry.stage
                ),
            ));
        }
    }
    if module.entry_points.len() != 1 {
        let name = module.entry_points.get(1).map_or("", |e| e.name.as_str());
        let mut message = format!(
            "expected one @fragment entry; found {}",
            module.entry_points.len()
        );
        if !name.is_empty() {
            let extra = module.entry_points[1..]
                .iter()
                .map(|entry| format!("`{}`", entry.name))
                .collect::<Vec<_>>()
                .join(", ");
            message.push_str(&format!("; extra entries: {extra}"));
        }
        return Err(fail(name, message));
    }
    let entry = &module.entry_points[0];
    let fields = |ty, binding: Option<Binding>| match &module.types[ty].inner {
        TypeInner::Struct { members, .. } => {
            members.iter().map(|m| (m.ty, m.binding.clone())).collect()
        }
        _ => vec![(ty, binding)],
    };
    for argument in &entry.function.arguments {
        if fields(argument.ty, argument.binding.clone())
            .iter()
            .any(|(_, binding)| !matches!(binding, Some(Binding::BuiltIn(_))))
        {
            return Err(fail(&entry.name, format!("entry `{}` input `{}` must use built-in values; @location inputs require a GPU module", entry.name, argument.name.as_deref().unwrap_or("<unnamed>"))));
        }
    }
    let output = entry
        .function
        .result
        .as_ref()
        .map(|r| fields(r.ty, r.binding.clone()))
        .unwrap_or_default();
    if !matches!(output.as_slice(), [(ty, Some(Binding::Location { location: 0, .. }))] if matches!(module.types[*ty].inner, TypeInner::Vector { size: naga::VectorSize::Quad, scalar: naga::Scalar { kind: naga::ScalarKind::Float, width: 4 } }))
    {
        return Err(fail(
            &entry.name,
            format!("entry `{}` must return one @location(0) vec4f", entry.name),
        ));
    }
    let mut uniform = false;
    for (handle, global) in module.global_variables.iter() {
        if global.binding.is_none() && global.space != AddressSpace::Uniform {
            continue;
        }
        let binding = global.binding.as_ref();
        if uniform
            || global.space != AddressSpace::Uniform
            || !binding.is_some_and(|b| b.group == 0 && b.binding == 0)
            || !matches!(module.types[global.ty].inner, TypeInner::Struct { .. })
        {
            let span = module.global_variables.get_span(handle).location(source);
            return Err((Span::point(span.line_number, span.line_position), format!("binding `{}` must be the only resource: a uniform struct at @group(0) @binding(0)", global.name.as_deref().unwrap_or("<unnamed>"))));
        }
        uniform = true;
    }
    Ok(())
}

fn compare_layout(module: &naga::Module, layout: &Layout) -> Result<(), (usize, String)> {
    let Some((_, uniform)) = module
        .global_variables
        .iter()
        .find(|(_, g)| g.space == AddressSpace::Uniform)
    else {
        return Ok(());
    };
    let TypeInner::Struct { members, span } = &module.types[uniform.ty].inner else {
        return Ok(());
    };
    for (i, member) in layout.members.iter().enumerate() {
        if members.get(i).is_none_or(|m| {
            m.name.as_deref() != Some(&member.name) || m.offset as usize != member.offset
        }) {
            return Err((
                member.location.offset,
                format!("uniform member `{}` layout differs from naga", member.name),
            ));
        }
    }
    if layout.members.len() != members.len() || layout.size != *span as usize {
        let offset = layout.members.last().map_or_else(
            || {
                module
                    .types
                    .get_span(uniform.ty)
                    .to_range()
                    .map_or(0, |s| s.start)
            },
            |m| m.location.offset,
        );
        let name = layout
            .members
            .last()
            .map(|m| m.name.as_str())
            .or_else(|| members.last().and_then(|m| m.name.as_deref()))
            .unwrap_or("<unnamed>");
        return Err((offset, format!("uniform block `{}` size after member `{name}` differs from naga: reader {}, naga {span}", layout.name.as_deref().unwrap_or(""), layout.size)));
    }
    Ok(())
}

fn check_layout(
    path: &Path,
    source: &str,
    module: &naga::Module,
    layout: &Layout,
) -> Result<(), crate::CompileError> {
    compare_layout(module, layout)
        .map_err(|(offset, message)| shader_error(Some(path), wgsl_span(source, offset), message))
}

fn literal(expr: &Expr) -> Option<exact_plan::Value> {
    use exact_plan::Value;
    Some(match expr {
        Expr::Number(n, _) => Value::Number(*n),
        Expr::Bool(b, _) => Value::Bool(*b),
        Expr::Str(s, _) => Value::str(s),
        Expr::None(_) => Value::Unit,
        Expr::List(items, _) => Value::List(items.iter().map(literal).collect::<Option<_>>()?),
        Expr::Unary(contract_syntax::UnOp::Neg, value, _) => match literal(value)? {
            Value::Number(n) => Value::Number(-n),
            _ => return None,
        },
        _ => return None,
    })
}

fn check_value(layout: &Layout, name: &str, value: &exact_plan::Value) -> Result<(), String> {
    use exact_plan::Value;
    let scalar = |value: &Value| match value {
        Value::Number(n) => uniforms::Value::Number(*n),
        Value::Bool(b) => uniforms::Value::Bool(*b),
        _ => uniforms::Value::Null,
    };
    let items = match value {
        Value::List(items) => items.iter().map(scalar).collect::<Vec<_>>(),
        _ => Vec::new(),
    };
    let value = match (value, value.as_str()) {
        (Value::List(_), _) => uniforms::Value::List(&items),
        (_, Some(s))
            if layout
                .members
                .iter()
                .any(|m| m.name == name && m.ty == uniforms::UniformType::Vec4f) =>
        {
            let color = match exact_color::parse(s) {
                Some(exact_color::Parsed::Legacy(_, c) | exact_color::Parsed::Wide(c)) => c,
                _ => {
                    return Err(format!(
                    "uniform argument `{name}`: expected a CSS colour; `{s}` is not a fixed colour"
                ))
                }
            };
            let [r, g, b] = color
                .linear_srgb()
                .map(|c| exact_color::linear_to_srgb(c).clamp(0.0, 1.0));
            uniforms::Value::Color([r, g, b, color.alpha])
        }
        (_, Some(s)) => uniforms::Value::String(s),
        _ => scalar(value),
    };
    layout
        .pack(&[(name, value)], uniforms::Builtins::default())
        .map(|_| ())
        .map_err(|e| e.message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reader_disagreement_reports_the_shader_member() {
        let source = include_str!("../tests/shader/plain.wgsl");
        let module = exact_gpu_reflect::inspect(source).unwrap();
        let layout = uniforms::parse(source).unwrap();
        let path = Path::new("assets/plain.wgsl");
        check_layout(path, source, &module, &layout).unwrap();
        for size in [false, true] {
            let mut bad = layout.clone();
            if size {
                bad.size += 16;
            } else {
                bad.members[1].offset += 4;
            }
            let error = check_layout(path, source, &module, &bad).unwrap_err();
            assert_eq!(error.id, "analyze-surface-shader");
            assert_eq!(error.file.as_deref(), Some(path));
            assert_eq!((error.span.line, error.span.col), (3, 3));
            assert!(error.message.contains("tint"), "{error}");
            assert!(error.message.contains("differs from naga"), "{error}");
        }
    }
}
