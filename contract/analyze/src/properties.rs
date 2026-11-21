//! Root property reads closed over derive and function bodies (LLP 1151 D1).

use super::AnalyzeError;
use contract_syntax::inline::calls::shown;
use contract_syntax::{
    Component, Expanded, Expr, FnDecl, PropertyDecl, PropertyForm, PropertyKind,
};
use std::collections::BTreeSet;

/// Resolve authored action scopes against the same component instances used by lowering.
pub fn resolve_actions(expanded: &Expanded, properties: &mut [PropertyDecl]) -> Vec<AnalyzeError> {
    let mut errors = Vec::new();
    for p in properties {
        let written = match &p.kind {
            PropertyKind::After(action) => action,
            PropertyKind::NeverRefuses => &p.name,
            PropertyKind::Always | PropertyKind::Clock => continue,
        };
        let message = if let Some((component, action)) = written.split_once('.') {
            let mut available = BTreeSet::new();
            let mut matched = Vec::new();
            for (_, name) in expanded
                .component_actions()
                .filter(|(i, _)| i.component == component)
            {
                available.insert(shown(name));
                if shown(name) == action {
                    matched.push(name.to_owned());
                }
            }
            if !matched.is_empty() {
                p.actions = matched;
                continue;
            }
            if !expanded.instances.iter().any(|i| i.component == component) {
                format!("the app uses no component `{component}`")
            } else if available.is_empty() {
                format!("`{component}` declares no actions")
            } else {
                format!(
                    "`{component}` has no action `{action}`. Available actions are {}",
                    available.into_iter().collect::<Vec<_>>().join(", ")
                )
            }
        } else {
            // Existing tests may name an expanded action directly. A wholly unknown bare
            // name keeps its historical, unexercised scope.
            if expanded.root.actions.iter().any(|a| a.name == *written) {
                continue;
            }
            let owners: BTreeSet<_> = expanded
                .component_actions()
                .filter(|(_, name)| shown(name) == written)
                .map(|(i, _)| format!("`{}.{written}`", i.component))
                .collect();
            if owners.is_empty() {
                continue;
            }
            format!(
                "name the child action `{written}` as {}",
                owners.into_iter().collect::<Vec<_>>().join(" or ")
            )
        };
        errors.push(AnalyzeError {
            id: "analyze-property-action",
            message,
            span: p.span,
            related: Vec::new(),
        });
    }
    errors
}

/// Refuse unknown names and derives whose values depend on the oracle, clock or geometry.
/// An `always` line may read a derive over resources: it checks each printed
/// configuration, where both sides print the oracle's answer, so it never
/// pairs a value with a commit as `after` does. `root` is the expanded
/// component that the plan compiler uses.
pub fn check_properties(
    root: &Component,
    fns: &[FnDecl],
    properties: &[PropertyDecl],
) -> Vec<AnalyzeError> {
    let mut errors = Vec::new();
    for property in properties {
        for line in &property.lines {
            let names: Vec<&str> = match &line.form {
                PropertyForm::Hold { name, .. } => vec![name],
                PropertyForm::OnlyWrites(names) => names.iter().map(String::as_str).collect(),
                PropertyForm::Command { .. } => Vec::new(),
            };
            for name in names {
                let slot = root.states.iter().any(|s| s.name == name)
                    || root.mutations.iter().any(|m| m.name == name);
                if slot {
                    continue;
                }
                let mut cone = Cone {
                    root,
                    fns,
                    seen: BTreeSet::new(),
                    resources: property.kind == PropertyKind::Always,
                };
                let reason = if let Some(d) = root.derives.iter().find(|d| d.name == name) {
                    if matches!(line.form, PropertyForm::OnlyWrites(_)) {
                        Some(format!(
                            "`{name}` is a derive; `only writes` names root slots"
                        ))
                    } else {
                        cone.expr(&d.expr, &BTreeSet::new()).map(|chain| format!("`{name}` -> {chain}: its value would be the oracle's or would change on a step that commits nothing"))
                    }
                } else if root.resources.iter().any(|r| r.name == name) {
                    Some(format!("`{name}` is a resource: its value would be the oracle's or would change on a step that commits nothing"))
                } else {
                    Some(format!("`{name}` is not a root state or derive"))
                };
                if let Some(message) = reason {
                    errors.push(AnalyzeError {
                        id: "analyze-property-read",
                        message,
                        span: line.span,
                        related: Vec::new(),
                    });
                }
            }
        }
    }
    errors
}

struct Cone<'a> {
    root: &'a Component,
    fns: &'a [FnDecl],
    seen: BTreeSet<(&'static str, String)>,
    /// Whether a resource's value may be read.
    resources: bool,
}
impl Cone<'_> {
    fn base<'a>(e: &'a Expr, bound: &BTreeSet<String>) -> Option<&'a str> {
        match e {
            Expr::Ident(name, _) if !bound.contains(name) => Some(name),
            Expr::Member(base, _, _) => Self::base(base, bound),
            _ => None,
        }
    }

    fn expr(&mut self, e: &Expr, bound: &BTreeSet<String>) -> Option<String> {
        match e {
            Expr::Ident(name, _) if !bound.contains(name) => {
                if self.root.resources.iter().any(|r| r.name == *name) {
                    return (!self.resources).then(|| format!("resource `{name}`"));
                }
                if self.seen.insert(("derive", name.clone())) {
                    if let Some(d) = self.root.derives.iter().find(|d| d.name == *name) {
                        return self
                            .expr(&d.expr, &BTreeSet::new())
                            .map(|s| format!("`{name}` -> {s}"));
                    }
                }
                None
            }
            Expr::Call(name, args, _) => {
                if contract_syntax::scope::STATUS_CALLS.contains(&name.as_str()) {
                    if let Some(target) = args.first().and_then(|arg| Self::base(arg, bound)) {
                        let kind = if self.root.mutations.iter().any(|m| m.name == target) {
                            "mutation"
                        } else if self.root.resources.iter().any(|r| r.name == target) {
                            "resource"
                        } else {
                            "target"
                        };
                        return Some(format!("`{name}(…)` of {kind} `{target}`"));
                    }
                } else if let Some(f) = self.fns.iter().find(|f| f.name == *name) {
                    if self.seen.insert(("fn", name.clone())) {
                        let locals = f.params.iter().map(|p| p.name.clone()).collect();
                        if let Some(s) = self.expr(&f.body, &locals) {
                            return Some(format!("fn `{name}` -> {s}"));
                        }
                    }
                } else if matches!(name.as_str(), "performanceNow" | "frame" | "measure") {
                    // The roster's date formatters read their arguments; exactTime is a resource.
                    return Some(format!("`{name}(…)`"));
                }
                args.iter().find_map(|a| self.expr(a, bound))
            }
            _ => e.scoped_operands().find_map(|(x, binds)| {
                if binds.is_empty() {
                    return self.expr(x, bound);
                }
                let mut locals = bound.clone();
                locals.extend(binds.iter().map(String::from));
                self.expr(x, &locals)
            }),
        }
    }
}
