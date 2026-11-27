use super::refusal;
use crate::{CompileError, Manifest};
use contract_syntax::{Expr, File, TypeExpr};
use std::path::Path;

pub(crate) fn check(file: &File, app_root: &Path) -> Result<(), Vec<CompileError>> {
    let sources: Vec<(&str, String)> = (file.modules.iter())
        .filter_map(|m| {
            let sub: serde_json::Value = serde_json::from_str(&m.substitute).ok()?;
            Some((m.name.as_str(), sub["answersWidget"].as_str()?.to_owned()))
        })
        .collect();
    let resources: Vec<_> = (file.components.iter())
        .flat_map(|c| &c.resources)
        .filter_map(|r| {
            let (_, option) = sources.iter().find(|(name, _)| *name == r.source)?;
            Some((r, option.as_str()))
        })
        .collect();
    if resources.is_empty() {
        return Ok(());
    }
    let widgets = if app_root.join("app.json").is_file() {
        Manifest::read(app_root)
            .and_then(|manifest| manifest.widgets())
            .map_err(|m| vec![refusal("app-manifest", m, Default::default())])?
    } else {
        Vec::new()
    };
    let configured: Vec<&str> = (widgets.iter())
        .filter(|w| w.configuration.is_some())
        .map(|w| w.kind.as_str())
        .collect();
    let mut errors = Vec::new();
    for (resource, option) in resources {
        let fail = |message: String| refusal("snapshot-placed-widgets", message, resource.span);
        let source = &resource.source;
        let given = resource.args.iter().flat_map(|a| match a {
            Expr::Call(_, fields, _) => fields.as_slice(),
            other => std::slice::from_ref(other),
        });
        let kind = given.into_iter().find_map(|a| match a {
            Expr::NamedArg(name, value, _) if name == option => match value.as_ref() {
                Expr::Str(kind, _) => Some(kind.as_str()),
                _ => None,
            },
            _ => None,
        });
        let Some(kind) = kind else {
            errors.push(fail(format!("`{}` reads `{source}` with `{option}` a string literal: the kind of a widget of app.json", resource.name)));
            continue;
        };
        let Some(widget) = widgets.iter().find(|w| w.kind == kind) else {
            let named = match configured.as_slice() {
                [] => "app.json declares no widget with a `configuration`".to_owned(),
                kinds => format!("name one of: {}", kinds.join(", ")),
            };
            errors.push(fail(format!(
                "`{source}({option}=\"{kind}\")` names no widget of app.json; {named}"
            )));
            continue;
        };
        if widget.configuration.is_none() {
            errors.push(fail(format!("widget `{kind}` has no configuration, so its placements carry nothing to read; read a widget with a `configuration`")));
            continue;
        }
        let Some(injected) = injected(file, &widget.component) else {
            errors.push(fail(format!("widget `{kind}` reads its configuration in no component, so `{}` has no shape to answer; inject `{}: <shape>` in `{}` or one it uses", resource.name, super::CONFIGURATION_SLOT, widget.component)));
            continue;
        };
        let answer = match &resource.shape {
            TypeExpr::Named(name, _) => file.shapes.iter().find(|s| &s.name == name),
            _ => None,
        };
        let fits = answer.is_some_and(|shape| {
            shape.fields.len() == 2
                && shape.fields.iter().all(|f| match (f.name.as_str(), &f.ty) {
                    ("ready", TypeExpr::Named(t, _)) => t == "bool",
                    ("placements", TypeExpr::List(item, _)) => {
                        matches!(item.as_ref(), TypeExpr::Named(t, _) if *t == injected)
                    }
                    _ => false,
                })
        });
        if !fits {
            errors.push(fail(format!("`{}` answers `ready: bool` and `placements: {injected}[]`, the shape widget `{kind}` injects as its configuration", resource.name)));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn injected(file: &File, component: &str) -> Option<String> {
    let index = file.components.iter().position(|c| c.name == component)?;
    let mut tree = File {
        components: file.components.clone(),
        ..File::default()
    };
    tree.retain_component_tree(index);
    tree.components
        .iter()
        .flat_map(|c| &c.injects)
        .filter(|p| p.name == super::CONFIGURATION_SLOT)
        .find_map(|p| match &p.ty {
            Some(TypeExpr::Named(name, _)) => Some(name.clone()),
            _ => None,
        })
}
