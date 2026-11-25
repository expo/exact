//! Compile an entry's reachable component tree with typed, replaceable data. No request, timer or handler enters this plan.

use super::{check, referenced_styles, refusal};
use crate::{
    manifest::{Activity, Configuration, WatchEntry, Widget, WATCH_KIND},
    CompileError, Manifest,
};
use contract_syntax::{Attr, Binding, Expr, File, Node, Span, TypeExpr};
use exact_plan::Plan;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

/// A compiled entry and the manifest that supplies its placeholder and refresh.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The author declaration.
    pub declaration: Declaration,
    /// Pure render plan, with a typed `snapshot_props` or `snapshot_state` slot.
    pub plan: Plan,
}

/// The entry kind determines whether a timeline refresh exists.
#[derive(Debug, Clone)]
pub enum Declaration {
    /// A timeline-driven widget.
    Widget(Widget),
    /// An activity updated by its app or backend.
    Activity(Activity),
    /// The app's watch entry: a live plan, not a snapshot (LLP 1126.002 D4).
    Watch(WatchEntry),
}

pub(super) struct RenderEntry<'a> {
    pub(super) kind: &'a str,
    pub(super) component: &'a str,
    pub(super) props: &'a str,
    pub(super) preview: &'a Value,
    pub(super) families: &'a [String],
    pub(super) refresh: Option<&'a str>,
    pub(super) configuration: Option<&'a Configuration>,
}

/// Compile every entry without changing or compiling the running app's plan.
pub fn compile_entries(
    app: &Path,
    manifest: &Manifest,
) -> Result<BTreeMap<String, Entry>, Vec<CompileError>> {
    let widgets = manifest
        .widgets()
        .map_err(|m| refusal("snapshot-manifest", m, Span::default()))?;
    let activities = manifest
        .live_activities()
        .map_err(|m| refusal("snapshot-manifest", m, Span::default()))?;
    let watch = manifest
        .watch()
        .map_err(|m| refusal("watch-manifest", m, Span::default()))?;
    if widgets.is_empty() && activities.is_empty() && watch.is_none() {
        return Ok(BTreeMap::new());
    }
    let app = app
        .canonicalize()
        .map_err(|e| refusal("snapshot-app", e.to_string(), Span::default()))?;
    let app = app.as_path();
    let path = app.join("app.contract");
    let src = crate::read_source(&path)?;
    let (file, sources) = crate::sources::load(&path, &src, app)?;
    let mut entries = BTreeMap::new();
    let mut errors = Vec::new();
    for widget in widgets {
        match compile_widget(file.clone(), &widget, Some(app)) {
            Ok(mut plan) => {
                plan.app_id.clone_from(&manifest.id);
                entries.insert(
                    widget.kind.clone(),
                    Entry {
                        declaration: Declaration::Widget(widget),
                        plan,
                    },
                );
            }
            Err(all) => errors.extend(all.into_iter().map(|e| sources.resolve(e))),
        }
    }
    for activity in activities {
        match super::activities::compile_file(file.clone(), &activity, Some(app)) {
            Ok(mut plan) => {
                plan.app_id.clone_from(&manifest.id);
                entries.insert(
                    activity.kind.clone(),
                    Entry {
                        declaration: Declaration::Activity(activity),
                        plan,
                    },
                );
            }
            Err(all) => errors.extend(all.into_iter().map(|e| sources.resolve(e))),
        }
    }
    if let Some(watch) = watch {
        match crate::compile_watch(app, &watch) {
            Ok(mut plan) => {
                if watch.sync.selected() {
                    let phone = crate::compile_path(&path).map_err(|e| vec![e])?;
                    crate::watch_sync::plans(&watch.sync, &phone, &plan)
                        .map_err(|m| refusal("watch-sync", m, Span::default()))?;
                }
                plan.app_id.clone_from(&manifest.id);
                entries.insert(
                    WATCH_KIND.to_owned(),
                    Entry {
                        declaration: Declaration::Watch(watch),
                        plan,
                    },
                );
            }
            Err(all) => errors.extend(all),
        }
    }
    if errors.is_empty() {
        Ok(entries)
    } else {
        Err(errors)
    }
}

/// Write compiled entries beside the app plan, replacing the prior entry tree.
pub fn write_entries(entries: &BTreeMap<String, Entry>, out: &Path) -> Result<(), String> {
    let directory = out.join("plans");
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|e| e.to_string())?;
    }
    if !entries.is_empty() {
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let registry: Vec<_> = entries
            .values()
            .filter_map(|e| match &e.declaration {
                Declaration::Widget(w) => Some(
                    serde_json::json!({"kind":w.kind, "families":w.families, "watchFamilies":w.watch_families, "requiresSecrets":w.requires_secrets, "preview":w.preview, "configuration":w.configuration.as_ref().map(Configuration::json)}),
                ),
                Declaration::Activity(_) | Declaration::Watch(_) => None,
            })
            .collect();
        if !registry.is_empty() {
            std::fs::write(
                directory.join("widgets.json"),
                serde_json::to_vec(&registry).unwrap(),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    super::activities::write(entries, out)?;
    for (kind, entry) in entries {
        std::fs::write(directory.join(format!("{kind}.plan")), entry.plan.encode())
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// The synthesized `exactViewport()` resource is checked as a written one
/// is at bake: every field of the inject's shape is one the viewport fact
/// supplies, with its type (`runner::viewport::FIELDS`).
fn viewport_shape(shape: &TypeExpr, file: &File, at: Span) -> Result<(), Vec<CompileError>> {
    let fail = |m: String| vec![refusal("snapshot-viewport", m, at)];
    let TypeExpr::Named(name, _) = shape else {
        return Err(fail(format!(
            "`{}` takes a shape of the viewport's fields (`width`, `height`, …), not `{}`",
            super::VIEWPORT_SLOT,
            crate::modules::written(shape)
        )));
    };
    let Some(declared) = file.shapes.iter().find(|s| &s.name == name) else {
        return Err(fail(format!("`{}`: declare shape `{name}` with the viewport's fields it reads (`width: number`, `height: number`)", super::VIEWPORT_SLOT)));
    };
    for field in &declared.fields {
        let expected = match field.name.as_str() {
            "width"
            | "height"
            | "horizontalViewportSegments"
            | "verticalViewportSegments"
            | "orientationAngle"
            | "keyboardHeight" => "number",
            "prefersReducedMotion" | "prefersReducedTransparency" | "accessibilityTextSize" => {
                "bool"
            }
            other if exact_runner::viewport::FIELDS.contains(&other) => "string",
            _ => {
                return Err(fail(format!("`{}`: `{name}` has a field `{}` the viewport fact does not supply; its fields are {}", super::VIEWPORT_SLOT, field.name, exact_runner::viewport::FIELDS.join(", "))));
            }
        };
        let written = match &field.ty {
            TypeExpr::Named(n, _) => n.as_str(),
            _ => "",
        };
        if written != expected {
            return Err(fail(format!("`{}`: `{name}.{}` is `{expected}` in the viewport fact; declare it `{}: {expected}`", super::VIEWPORT_SLOT, field.name, field.name)));
        }
    }
    Ok(())
}

/// A component that injects `snapshot_configuration` reads the widget's
/// configuration: the wrapper holds it in a slot of that name, initially each
/// parameter's default, which the host replaces with the person's choices. The
/// inject and the manifest's `configuration` come together, and the shape has
/// one field per parameter: a `string` or an enum of exactly its values, or
/// for an entity parameter an `option<string>`, the chosen entry's id.
fn configuration(
    file: &File,
    widget: &RenderEntry<'_>,
    wrapper: &mut contract_syntax::Component,
) -> Result<(), Vec<CompileError>> {
    let slot = super::CONFIGURATION_SLOT;
    let fail = |m: String, at: Span| vec![refusal("snapshot-configuration", m, at)];
    let mut injected: Option<(TypeExpr, Span)> = None;
    for c in &file.components {
        for inject in c.injects.iter().filter(|p| p.name == slot) {
            let Some(ty) = &inject.ty else {
                return Err(fail(format!("`{slot}` needs its shape: `inject` with `{slot}: <shape>`, one field per configuration parameter"), inject.span));
            };
            match &injected {
                Some((first, _)) if !same_type(first, ty) => {
                    return Err(fail(format!("`{slot}` is declared with another shape in `{}`; every component reads the configuration through one shape", c.name), inject.span));
                }
                Some(_) => {}
                None => injected = Some((ty.clone(), inject.span)),
            }
        }
    }
    let ((ty, at), configuration) = match (injected, widget.configuration) {
        (None, None) => return Ok(()),
        (Some((_, at)), None) => {
            return Err(fail(
                if widget.refresh.is_some() {
                    format!("entry {}: `{slot}` reads a configuration the widget does not declare; add `configuration` to its app.json entry or remove the inject", widget.kind)
                } else {
                    format!("entry {}: a Live Activity has no configuration; remove the `{slot}` inject", widget.kind)
                },
                at,
            ))
        }
        (None, Some(_)) => {
            return Err(fail(format!("entry {}: its `configuration` is read by no component; inject `{slot}: <shape>` in `{}` (or one it uses) or remove the configuration", widget.kind, widget.component), Span::default()))
        }
        (Some(found), Some(configuration)) => (found, configuration),
    };
    let parameters = configuration.parameters.as_slice();
    let names = |parameters: &[crate::Parameter]| {
        parameters
            .iter()
            .map(|p| format!("`{}`", p.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let shape = match &ty {
        TypeExpr::Named(name, _) => file.shapes.iter().find(|s| &s.name == name),
        _ => None,
    };
    let Some(declared) = shape else {
        return Err(fail(
            format!(
                "`{slot}` takes a shape declared with one field per configuration parameter: {}",
                names(parameters)
            ),
            at,
        ));
    };
    let name = &declared.name;
    if declared.fields.len() != parameters.len() {
        return Err(fail(
            format!(
                "`{slot}`: `{name}` must have exactly the configuration's parameters as fields: {}",
                names(parameters)
            ),
            at,
        ));
    }
    let mut args = Vec::new();
    for parameter in parameters {
        let Some(field) = declared.fields.iter().find(|f| f.name == parameter.name) else {
            return Err(fail(format!("`{slot}`: `{name}` has no field `{}`; its fields must be the configuration's parameters: {}", parameter.name, names(parameters)), at));
        };
        let initial = match &parameter.choices {
            crate::Choices::Fixed { values, default } => {
                let fits = match &field.ty {
                    TypeExpr::Named(n, _) => n == "string",
                    TypeExpr::Enum(written, _) => {
                        written.len() == values.len()
                            && values.iter().all(|(v, _)| written.contains(v))
                    }
                    _ => false,
                };
                if !fits {
                    let values = values
                        .iter()
                        .map(|(v, _)| format!("\"{v}\""))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(fail(format!("`{slot}`: declare `{name}.{}` as `string` or `enum<{values}>`, the parameter's values", parameter.name), at));
                }
                Expr::Str(default.clone(), at)
            }
            crate::Choices::Entity { kind, .. } => {
                let fits = matches!(&field.ty, TypeExpr::Option(inner, _)
                    if matches!(inner.as_ref(), TypeExpr::Named(n, _) if n == "string"));
                if !fits {
                    return Err(fail(format!("`{slot}`: declare `{name}.{}` as `option<string>`: the id of the `{kind}` entry the person chose, and `none` until they choose", parameter.name), at));
                }
                Expr::None(at)
            }
        };
        args.push(Expr::NamedArg(
            parameter.name.clone(),
            Box::new(initial),
            at,
        ));
    }
    // The shape's field order, which the record constructor follows.
    args.sort_by_key(|arg| match arg {
        Expr::NamedArg(field, _, _) => declared.fields.iter().position(|f| &f.name == field),
        _ => None,
    });
    wrapper.states.push(Binding {
        name: slot.into(),
        expr: Expr::Call(name.clone(), args, at),
        span: at,
    });
    wrapper.provides.push(Binding {
        name: slot.into(),
        expr: Expr::Ident(slot.into(), at),
        span: at,
    });
    Ok(())
}

/// Compile a source-text fixture's entry under the same profile as the bake.
pub fn compile(src: &str, widget: &Widget) -> Result<Plan, Vec<CompileError>> {
    let file = contract_syntax::parse(src).map_err(CompileError::from)?;
    crate::sources::standalone_members(&file)?;
    compile_widget(file, widget, None)
}

fn compile_widget(
    file: File,
    widget: &Widget,
    app: Option<&Path>,
) -> Result<Plan, Vec<CompileError>> {
    let admission_families: Vec<_> = widget
        .families
        .iter()
        .chain(&widget.watch_families)
        .cloned()
        .collect();
    compile_file(
        file,
        &RenderEntry {
            kind: &widget.kind,
            component: &widget.component,
            props: &widget.props,
            preview: &widget.preview,
            families: &admission_families,
            refresh: Some(&widget.refresh),
            configuration: widget.configuration.as_ref(),
        },
        app,
    )
}

pub(super) fn compile_file(
    mut file: File,
    widget: &RenderEntry<'_>,
    app: Option<&Path>,
) -> Result<Plan, Vec<CompileError>> {
    let fail = |m: String| refusal("snapshot-entry", m, Span::default());
    let index = file
        .components
        .iter()
        .position(|c| c.name == widget.component)
        .ok_or_else(|| {
            fail(format!(
                "entry {}: {}; name its component in app.contract",
                widget.kind,
                file.unknown_component_message(widget.component)
            ))
        })?;
    let root = file.components[index].clone();
    let shape = file
        .shapes
        .iter()
        .find(|s| s.name == widget.props)
        .ok_or_else(|| {
            fail(format!(
                "entry {}: declare props shape `{}` in app.contract",
                widget.kind, widget.props
            ))
        })?;
    if shape.fields.len() != root.props.len()
        || shape.fields.iter().any(|f| {
            !root
                .props
                .iter()
                .any(|p| p.name == f.name && p.ty.as_ref().is_some_and(|ty| same_type(ty, &f.ty)))
        })
    {
        return Err(vec![fail(format!("entry {}: `{}` props must match every field and type of `{}`; edit the component props or manifest shape", widget.kind, root.name, widget.props))]);
    }
    let slot_name = if widget.refresh.is_some() {
        super::PROPS_SLOT
    } else {
        super::STATE_SLOT
    };
    let span = root.span;
    let preview = preview(
        widget.preview,
        &TypeExpr::Named(widget.props.to_string(), span),
        &file,
        0,
    )
    .map_err(|m| fail(format!("entry {} preview: {m}", widget.kind)))?;
    file.retain_component_tree(index);
    let routes = file.routes.take();
    file.tests.clear();
    file.launch.clear();
    file.sounds.clear();
    file.keyframes.clear();
    file.timelines.clear();
    // Styles and functions may be shared with the app; only declarations the
    // entry reaches are checked, so an unrelated app animation is harmless.
    let (styles, mut errors) = referenced_styles(&file);
    if let Err(refused) = check(&file, &styles) {
        errors.extend(refused);
    }
    let mut families = Vec::new();
    let mut font_attrs = |attrs: &[Attr]| {
        for a in attrs.iter().filter(|a| a.name == "font-family") {
            literal_fonts(&a.value, &mut families);
        }
    };
    for c in &file.components {
        Node::walk(&c.view).for_each(|n| {
            if let Node::Element { attrs, .. } = n {
                font_attrs(attrs);
            }
        });
    }
    for s in file.styles.iter().filter(|s| styles.contains(&s.name)) {
        font_attrs(&s.attrs);
    }
    // The system decodes and draws the entry in its own process, which has
    // only installed fonts: a bundled face fails the render on iOS and Android.
    // The refusal is at the use, since the app's own pages may keep the font.
    for (family, span) in families {
        let bundled = file
            .fonts
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(&family));
        if let Some(font) = bundled {
            errors.push(refusal(
                "snapshot-font-bundled",
                format!(
                    "the system draws a widget or Live Activity in a process without the app's fonts; replace `{}` with an installed or generic family such as `system-ui`",
                    font.name
                ),
                span,
            ));
        }
    }
    file.fonts.clear();
    if !errors.is_empty() {
        return Err(errors);
    }
    file.styles.retain(|s| styles.contains(&s.name));
    // No handler is admitted. Its unreferenced body must not retain helpers.
    for c in &mut file.components {
        c.actions.clear();
    }
    let functions = std::mem::take(&mut file.fns);
    loop {
        let calls = contract_syntax::scope::calls(&mut file);
        let before = file.fns.len();
        for f in &functions {
            if calls.contains(&f.name) && !file.fns.iter().any(|used| used.name == f.name) {
                file.fns.push(f.clone());
            }
        }
        if file.fns.len() == before {
            break;
        }
    }
    let shapes = std::mem::take(&mut file.shapes);
    loop {
        let mut used = contract_syntax::scope::type_names(&mut file);
        used.extend(contract_syntax::scope::calls(&mut file));
        used.insert(widget.props.to_string());
        let before = file.shapes.len();
        for s in &shapes {
            if used.contains(&s.name) && !file.shapes.iter().any(|kept| kept.name == s.name) {
                file.shapes.push(s.clone());
            }
        }
        if file.shapes.len() == before {
            break;
        }
    }
    if contract_syntax::scope::calls(&mut file).contains("path") {
        if routes.as_ref().is_some_and(|r| r.slot == slot_name) {
            return Err(vec![refusal("snapshot-props-name", format!("rename the `{slot_name}` router; the snapshot reader reserves that binding for entry data"), routes.as_ref().unwrap().span)]);
        }
        // A URL shares the app's checked patterns, never its page data sources.
        file.routes = routes.map(|mut routes| {
            for row in &mut routes.rows {
                row.fields.clear();
            }
            routes
        });
    }
    let mut wrapper = contract_syntax::parse("component Snapshot\n  view\n    view\n")
        .expect("wrapper")
        .components
        .remove(0);
    wrapper.name = format!("__snapshot_{}", widget.kind);
    wrapper.states.push(Binding {
        name: slot_name.into(),
        expr: preview,
        span,
    });
    wrapper.view = vec![Node::Use {
        name: root.name,
        args: root
            .props
            .iter()
            .map(|p| Attr {
                name: p.name.clone(),
                value: Expr::Member(
                    Box::new(Expr::Ident(slot_name.into(), span)),
                    p.name.clone(),
                    span,
                ),
                span,
                written: true,
            })
            .collect(),
        children: vec![],
        span,
    }];
    // A component that injects `snapshot_viewport` reads the family's content
    // size: the wrapper declares the fact and provides it (LLP 1126 D3). The
    // shape is the injecting components' own, and must be one shape.
    let mut viewport: Option<(TypeExpr, Span)> = None;
    for c in &file.components {
        for inject in c.injects.iter().filter(|p| p.name == super::VIEWPORT_SLOT) {
            let Some(ty) = &inject.ty else {
                return Err(vec![refusal(
                    "snapshot-viewport",
                    format!(
                        "`{}` needs its shape: `inject` with `{}: <shape>` of `width` and `height`",
                        super::VIEWPORT_SLOT,
                        super::VIEWPORT_SLOT
                    ),
                    inject.span,
                )]);
            };
            match &viewport {
                Some((first, _)) if !same_type(first, ty) => {
                    return Err(vec![refusal("snapshot-viewport", format!("`{}` is declared with another shape in `{}`; every component reads the family's size through one shape", super::VIEWPORT_SLOT, c.name), inject.span)]);
                }
                Some(_) => {}
                None => viewport = Some((ty.clone(), inject.span)),
            }
        }
    }
    if let Some((shape, at)) = viewport {
        viewport_shape(&shape, &file, at)?;
        wrapper.resources.push(contract_syntax::ResourceDecl {
            name: super::VIEWPORT_SLOT.into(),
            source: exact_runner::viewport::SOURCE.into(),
            args: vec![],
            identity: None,
            shape,
            placeholder: None,
            each: None,
            transient: false,
            keep: false,
            gate: None,
            span: at,
        });
        wrapper.provides.push(Binding {
            name: super::VIEWPORT_SLOT.into(),
            expr: Expr::Ident(super::VIEWPORT_SLOT.into(), at),
            span: at,
        });
    }
    configuration(&file, widget, &mut wrapper)?;
    file.components.insert(0, wrapper);
    super::paragraphs::check(&file, widget.families)?;
    let strings = match app {
        Some(app) => crate::strings::load(app, &app.join("app.contract"))?,
        None => None,
    };
    let (plan, _) =
        crate::compile_file_output(&file, app, strings, false, contract_lower::Profile::Web)?;
    let props = plan
        .slots
        .iter()
        .find(|slot| plan.str(slot.name) == slot_name)
        .expect("props slot")
        .ty;
    let mut plan = plan;
    let mut configuration_ty = None;
    if let Some(configuration) = widget.configuration {
        // LLP 1126: the boot slot keeps the fixed vocabulary even when the inject
        // reads strings. Its first enum member is the current fallback for a saved choice.
        // An entity field holds any entry's id, so it has no vocabulary.
        let slot = plan
            .slots
            .iter()
            .position(|s| plan.str(s.name) == super::CONFIGURATION_SLOT)
            .unwrap();
        let fields = plan
            .type_(plan.slots[slot].ty)
            .fields
            .iter()
            .map(|id| plan.str(plan.field(id).name).to_owned())
            .collect::<Vec<_>>();
        let mut builder = exact_plan::builder::PlanBuilder::from_plan(plan);
        let fields = fields
            .iter()
            .map(|name| {
                let parameter = configuration
                    .parameters
                    .iter()
                    .find(|p| &p.name == name)
                    .unwrap();
                let ty = match &parameter.choices {
                    crate::Choices::Fixed { values, default } => {
                        let mut ordered = vec![default.clone()];
                        ordered.extend(
                            values
                                .iter()
                                .map(|(v, _)| v)
                                .filter(|v| *v != default)
                                .cloned(),
                        );
                        builder.string_enum(&ordered)
                    }
                    crate::Choices::Entity { .. } => {
                        let id = builder.primitive(exact_plan::TypeKind::String);
                        builder.option(id)
                    }
                };
                (name.as_str(), ty)
            })
            .collect::<Vec<_>>();
        // Unnamed, so the declarations of two widgets with different
        // configurations cannot print one shape under a shared name.
        let ty = builder.record("", &fields);
        plan = builder
            .finish()
            .map_err(|e| vec![fail(format!("invalid configuration: {e:?}"))])?;
        plan.slots[slot].ty = ty;
        configuration_ty = Some(ty);
    }
    if let Some(refresh) = widget.refresh {
        let mut builder = exact_plan::builder::PlanBuilder::from_plan(plan);
        let number = builder.primitive(exact_plan::TypeKind::Number);
        let entry = builder.record(
            "SnapshotTimelineEntry",
            &[("date", number), ("props", props)],
        );
        let timeline = builder.list(entry);
        // A configured widget's refresh also receives its placement's choices.
        let params: Vec<_> = [number].into_iter().chain(configuration_ty).collect();
        let refresh = builder.source(refresh, &params, timeline);
        plan = builder
            .finish()
            .map_err(|e| vec![fail(format!("invalid refresh signature: {e:?}"))])?;
        plan.snapshot_refresh = Some(refresh);
    } else {
        super::activities::placements(&mut plan)?;
    }
    plan.snapshot_entry = widget.kind.to_owned();
    // The snapshot reader exercises the same validating decoder as its host.
    let plan = Plan::decode_snapshot(&plan.encode(), widget.kind)
        .map_err(|e| vec![fail(format!("invalid snapshot plan: {e:?}"))])?;
    super::inline::check(&plan, widget.families, span)?;
    Ok(plan)
}

fn same_type(a: &TypeExpr, b: &TypeExpr) -> bool {
    match (a, b) {
        (TypeExpr::Named(a, _), TypeExpr::Named(b, _)) => a == b,
        (TypeExpr::Enum(a, _), TypeExpr::Enum(b, _)) => a == b,
        (TypeExpr::List(a, _), TypeExpr::List(b, _))
        | (TypeExpr::Option(a, _), TypeExpr::Option(b, _)) => same_type(a, b),
        _ => false,
    }
}

fn preview(v: &Value, ty: &TypeExpr, file: &File, depth: usize) -> Result<Expr, String> {
    if depth > 64 {
        return Err("props nest more than 64 levels".into());
    }
    let span = ty.span();
    let bad = || {
        format!(
            "{v} does not match {}; supply preview props of the declared shape",
            crate::modules::written(ty)
        )
    };
    Ok(match ty {
        TypeExpr::Named(name, _) => match name.as_str() {
            "number" => Expr::Number(v.as_f64().ok_or_else(bad)?, span),
            "string" => Expr::Str(v.as_str().ok_or_else(bad)?.into(), span),
            "bool" => Expr::Bool(v.as_bool().ok_or_else(bad)?, span),
            _ => {
                let shape = file
                    .shapes
                    .iter()
                    .find(|s| s.name == *name)
                    .ok_or_else(bad)?;
                let object = v.as_object().ok_or_else(bad)?;
                let args = shape
                    .fields
                    .iter()
                    .map(|f| {
                        let value = object
                            .get(&f.name)
                            .ok_or_else(|| format!("missing `{}`; add it to preview", f.name))?;
                        Ok(Expr::NamedArg(
                            f.name.clone(),
                            Box::new(
                                preview(value, &f.ty, file, depth + 1)
                                    .map_err(|message| format!("`{}`: {message}", f.name))?,
                            ),
                            span,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                if object.len() != shape.fields.len() {
                    return Err(bad());
                }
                Expr::Call(name.clone(), args, span)
            }
        },
        TypeExpr::Enum(values, _) => Expr::Str(
            v.as_str()
                .filter(|s| values.iter().any(|v| v == s))
                .ok_or_else(bad)?
                .into(),
            span,
        ),
        TypeExpr::List(inner, _) => Expr::Typed(
            Box::new(Expr::List(
                v.as_array()
                    .ok_or_else(bad)?
                    .iter()
                    .map(|v| preview(v, inner, file, depth + 1))
                    .collect::<Result<_, _>>()?,
                span,
            )),
            ty.clone(),
            span,
        ),
        TypeExpr::Option(inner, _) => Expr::Typed(
            Box::new(if v.is_null() {
                Expr::None(span)
            } else {
                Expr::Some(Box::new(preview(v, inner, file, depth + 1)?), span)
            }),
            ty.clone(),
            span,
        ),
    })
}

fn literal_fonts(e: &Expr, fonts: &mut Vec<(String, Span)>) {
    match e {
        Expr::Str(stack, span) => fonts.extend(
            stack
                .split(',')
                .map(|n| (n.trim().trim_matches(['"', '\'']).to_string(), *span)),
        ),
        Expr::Ternary(_, yes, no, _) => {
            literal_fonts(yes, fonts);
            literal_fonts(no, fonts);
        }
        Expr::Match { some, none, .. } => {
            literal_fonts(some, fonts);
            literal_fonts(none, fonts);
        }
        Expr::Let { body, .. } => literal_fonts(body, fonts),
        _ => {}
    }
}
