//! Component uses, checked where they are written and before the expanded
//! root, so a misspelled or mistyped argument is reported at its call site
//! rather than inside the component it was substituted into.

use super::{checks, err, infer, ComponentTypes, Ref, Scope, Sink, Ty, TypeError, Types};
use contract_syntax::{Attr, Component, Expr, File, Node, Span};
use std::collections::BTreeSet;

/// Every use names only the used component's props (before expansion, which
/// would otherwise report the prop a misspelling left missing).
pub(crate) fn check_prop_names(
    nodes: &[Node],
    file: &File,
    sink: &mut Sink,
    explained: &mut Vec<contract_syntax::Span>,
) {
    for n in Node::walk(nodes) {
        let Node::Use {
            name, args, span, ..
        } = n
        else {
            continue;
        };
        let Some(target) = file.components.iter().find(|c| &c.name == name) else {
            continue;
        };
        if let Some(a) = args
            .iter()
            .find(|a| !target.props.iter().any(|p| p.name == a.name))
        {
            sink.push(checks::unknown_props(target, args, a.span));
            let unknown: Vec<_> = args
                .iter()
                .filter(|a| !target.props.iter().any(|p| p.name == a.name))
                .collect();
            let missing: Vec<&str> = target
                .props
                .iter()
                .filter(|p| !args.iter().any(|a| a.name == p.name))
                .map(|p| p.name.as_str())
                .collect();
            if let ([given], [meant]) = (unknown.as_slice(), missing.as_slice()) {
                if contract_syntax::suggestion(&given.name, [*meant]) == Some(*meant) {
                    explained.push(*span);
                }
            }
        }
    }
}

/// The root's scope as far as its declarations alone type it, for checking
/// its call sites before the expanded view: anything not yet known is `?`,
/// which unifies with every type, so this scope can hide an error from
/// [`check_root_uses`] but never invent one.
fn provisional_scope(c: &Component, types: &Types, routes: bool) -> Scope {
    let shapes = &types.shapes;
    let resolve = |t: Option<&contract_syntax::TypeExpr>| {
        t.and_then(|t| shapes.resolve(t).ok())
            .unwrap_or(Ty::Unknown)
    };
    let mut ct = ComponentTypes {
        name: c.name.clone(),
        props: c
            .props
            .iter()
            .chain(&c.injects)
            .map(|p| resolve(p.ty.as_ref()))
            .collect(),
        slots: vec![Ty::Unknown; c.states.len()],
        derives: vec![Ty::Unknown; c.derives.len()],
        resources: c
            .resources
            .iter()
            .map(|r| resolve(Some(&r.shape)))
            .collect(),
        mutations: c
            .mutations
            .iter()
            .map(|m| resolve(Some(&m.shape)))
            .collect(),
        actions: c
            .actions
            .iter()
            .map(|a| a.params.iter().map(|p| resolve(p.ty.as_ref())).collect())
            .collect(),
        sources: Default::default(),
    };
    // One scope, each name retyped as it is learned: building it again for
    // each state is quadratic in a large expanded root, and only a refused
    // build gets here.
    let mut scope = types.component_scope(c, &ct);
    let first = c.props.len() + c.injects.len();
    for i in 0..c.states.len() {
        ct.slots[i] = if i == 0 && routes {
            Ty::Record("Router".into())
        } else {
            infer(&c.states[i].expr, &scope, shapes).unwrap_or(Ty::Unknown)
        };
        scope.retype(first + i, ct.slots[i].clone());
    }
    let first = first + c.states.len();
    for _ in 0..c.derives.len() + 1 {
        let derives: Vec<Ty> = c
            .derives
            .iter()
            .map(|d| infer(&d.expr, &scope, shapes).unwrap_or(Ty::Unknown))
            .collect();
        if derives == ct.derives {
            break;
        }
        for (i, t) in derives.iter().enumerate() {
            scope.retype(first + i, t.clone());
        }
        ct.derives = derives;
    }
    #[cfg(test)]
    crate::scope_tests::same_declarations(&scope, &types.component_scope(c, &ct));
    scope
}

/// The root's own uses, typed against [`provisional_scope`]. A refusal that
/// only a `?` produced is left for the full check, as the derive fixpoint does.
pub(crate) fn check_root_uses(
    root: &Component,
    expanded: &Component,
    types: &Types,
    file: &File,
    sink: &mut Sink,
) {
    let scope = provisional_scope(expanded, types, file.routes.is_some());
    let mut found = Sink::default();
    walk_uses(
        &root.view,
        &scope,
        types,
        file,
        false,
        &mut UseErrors::new(&mut found),
    );
    for e in found.errors {
        if !e.message.contains("`?`") {
            sink.push(e);
        }
    }
}

pub(crate) fn check_uses(
    nodes: &[Node],
    scope: &Scope,
    types: &Types,
    file: &File,
    sink: &mut Sink,
) {
    walk_uses(nodes, scope, types, file, true, &mut UseErrors::new(sink));
}

/// Report child calls in their authored scopes and mark the data arguments
/// whose substitutions would repeat those errors in their consumers.
pub(crate) fn check_child_uses(file: &File, types: &Types, sink: &mut Sink) -> BTreeSet<Span> {
    let mut errors = UseErrors::new(sink);
    for (c, ct) in file.components.iter().zip(&types.components).skip(1) {
        walk_uses(
            &c.view,
            &types.component_scope(c, ct),
            types,
            file,
            true,
            &mut errors,
        );
    }
    errors.arguments
}

struct UseErrors<'a> {
    sink: &'a mut Sink,
    arguments: BTreeSet<Span>,
}

impl<'a> UseErrors<'a> {
    fn new(sink: &'a mut Sink) -> Self {
        Self {
            sink,
            arguments: BTreeSet::new(),
        }
    }
}

fn check_argument(
    arg: &Attr,
    expected: &Ty,
    scope: &Scope,
    types: &Types,
) -> Result<(), TypeError> {
    let given = infer(&arg.value, scope, &types.shapes)?;
    if checks::accepts_expr(expected, &arg.value, &given, scope, &types.shapes) {
        Ok(())
    } else {
        err(
            "type-prop",
            format!("`{}` expects `{expected}`, given `{given}`", arg.name),
            arg.span,
        )
    }
}

/// `actions: false` checks only data arguments. Action bindings are checked
/// by the full walk; their errors do not suppress independent child errors.
fn walk_uses(
    nodes: &[Node],
    scope: &Scope,
    types: &Types,
    file: &File,
    actions: bool,
    errors: &mut UseErrors,
) {
    for n in nodes {
        let checked = walk_use(n, scope, types, file, actions, errors);
        errors.sink.keep_unit(checked);
    }
}

/// One node's uses; a refusal ends only this node's walk.
fn walk_use(
    n: &Node,
    scope: &Scope,
    types: &Types,
    file: &File,
    actions: bool,
    errors: &mut UseErrors,
) -> Result<(), TypeError> {
    {
        match n {
            Node::Use {
                name,
                args,
                children,
                span,
            } => {
                walk_uses(children, scope, types, file, actions, errors);
                let Some(target) = file.components.iter().position(|c| &c.name == name) else {
                    return err(
                        "type-unknown-component",
                        file.unknown_component_message(name),
                        *span,
                    );
                };
                let target_c = &file.components[target];
                let target_t = &types.components[target];
                for (i, p) in target_c.props.iter().enumerate() {
                    let Some(arg) = args.iter().find(|a| a.name == p.name) else {
                        // At a use the root expands, a missing prop is expansion's refusal.
                        if actions {
                            errors.sink.push(TypeError {
                                id: "type-missing-prop",
                                message: target_c.missing_props_message(args),
                                span: *span,
                            });
                        }
                        continue;
                    };
                    if matches!(target_t.props[i], Ty::Action(_)) {
                        if !actions {
                            continue;
                        }
                        if let Err(error) = check_action_binding(
                            &p.name,
                            &arg.value,
                            arg.span,
                            computed_action(&arg.value, scope, types, file),
                            false,
                        ) {
                            errors.sink.push(error);
                            continue;
                        }
                    }
                    if let Err(error) = check_argument(arg, &target_t.props[i], scope, types) {
                        if !matches!(target_t.props[i], Ty::Action(_)) {
                            errors.arguments.insert(arg.span);
                        }
                        errors.sink.push(error);
                    }
                }
                for a in args {
                    if !target_c.props.iter().any(|p| p.name == a.name) {
                        return Err(checks::unknown_props(target_c, args, a.span));
                    }
                }
            }
            Node::Element { children, .. } => {
                walk_uses(children, scope, types, file, actions, errors)
            }
            Node::Children { .. } => {}
            Node::When {
                then, otherwise, ..
            } => {
                walk_uses(then, scope, types, file, actions, errors);
                walk_uses(otherwise, scope, types, file, actions, errors);
            }
            Node::Each {
                var,
                index,
                list,
                body,
                ..
            } => {
                let lt = infer(list, scope, &types.shapes)?;
                let Ty::List(item) = lt else {
                    return err(
                        "type-each-list",
                        format!("`for` needs a list, given `{lt}`"),
                        list.span(),
                    );
                };
                let mut inner = scope.clone();
                inner.push_each(var, index.as_deref(), *item);
                walk_uses(body, &inner, types, file, actions, errors);
            }
            Node::Match {
                subject,
                some,
                none,
                ..
            } => {
                let st = infer(subject, scope, &types.shapes)?;
                let Ty::Option(item) = st else {
                    return err(
                        "type-match-subject",
                        format!("`match` needs an option, given `{st}`"),
                        subject.span(),
                    );
                };
                let mut inner = scope.clone();
                inner.push_region(Some((some.0.clone(), Ref::Bound(0), *item)));
                walk_uses(&some.1, &inner, types, file, actions, errors);
                let mut none_scope = scope.clone();
                none_scope.push_region(None);
                walk_uses(none, &none_scope, types, file, actions, errors);
            }
        }
    }
    Ok(())
}

/// Props and injects substitute the supplied expression into a handler.
pub(super) fn check_action_binding(
    name: &str,
    mut value: &Expr,
    span: Span,
    computed: bool,
    provided: bool,
) -> Result<(), TypeError> {
    while let Expr::Typed(inner, ..) = value {
        value = inner;
    }
    if !computed && matches!(value, Expr::Ident(..) | Expr::Call(..)) {
        return Ok(());
    }
    let (id, kind) = if provided {
        ("type-action-provide-shape", "inject")
    } else {
        ("type-action-prop-shape", "prop")
    };
    let remedy = if computed || matches!(value, Expr::Ternary(..)) {
        "; write one action that chooses, and pass its name"
    } else {
        ""
    };
    err(
        id,
        format!(
            "`{name}` is an `action` {kind} and needs an action name or `action(args)`{remedy}"
        ),
        span,
    )
}

/// Child derives substitute their resolved expressions; root derives remain
/// values that lowering cannot bind as action targets.
pub(super) fn computed_action(mut value: &Expr, scope: &Scope, types: &Types, file: &File) -> bool {
    while let Expr::Typed(inner, ..) = value {
        value = inner;
    }
    match value {
        Expr::Ident(target, _)
            if matches!(scope.lookup(target), Some((Ref::Derive(_), Ty::Action(_)))) =>
        {
            let Some((i, c)) = file
                .components
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, c)| c.name == scope.component())
            else {
                return true;
            };
            let Ok(derives) = contract_syntax::inline::resolved_derives(c) else {
                return true;
            };
            let Some((_, resolved)) = derives.iter().find(|(d, _)| &d.name == target) else {
                return true;
            };
            let declared = types.component_scope(c, &types.components[i]);
            check_action_binding(
                target,
                resolved,
                value.span(),
                computed_action(resolved, &declared, types, file),
                false,
            )
            .is_err()
        }
        Expr::Ident(target, _) | Expr::Call(target, ..) => matches!(
            scope.lookup(target),
            Some((r, Ty::Action(_))) if !matches!(r, Ref::Action(_) | Ref::Prop(_))
        ),
        _ => false,
    }
}

/// Prefer refusals at bindings over consequences in the expanded view.
/// Misspelled props precede missing-prop errors; invalid action and data
/// bindings precede errors inside callees. Action-binding errors affect ordering
/// only; the data-argument and prop-name checks may suppress expanded child
/// errors. Standalone child checks, their uses and expansion errors stay.
/// Only refused builds pay for this walk.
pub(crate) fn prefer_call_sites(
    errors: Vec<TypeError>,
    independent: usize,
    file: &File,
    types: &Types,
    expanded: &contract_syntax::Expanded,
) -> Vec<TypeError> {
    let mut sites = Sink::default();
    // Uses whose missing prop is the one a misspelling there was meant as.
    let mut explained = Vec::new();
    for c in &file.components {
        check_prop_names(&c.view, file, &mut sites, &mut explained);
    }
    check_root_uses(&file.components[0], &expanded.root, types, file, &mut sites);
    let call_site_fault = !sites.errors.is_empty();
    for id in [
        "syntax-call-target",
        "type-action-prop-shape",
        "type-action-provide-shape",
    ] {
        for e in errors.iter().filter(|e| e.id == id) {
            sites.push(e.clone());
        }
    }
    if sites.errors.is_empty() {
        return errors;
    }
    for (i, e) in errors.into_iter().enumerate() {
        if i < independent || !call_site_fault || !in_child(file, e.span) {
            sites.push(e);
        }
    }
    sites.errors.retain(|e| {
        !(matches!(e.id, "syntax-missing-prop" | "type-missing-prop")
            && explained.contains(&e.span))
    });
    sites.errors
}

/// Whether `span` lies in a child component's own lines: each component runs
/// from its header to the next component's header in the same file.
fn in_child(file: &File, span: Span) -> bool {
    file.components
        .iter()
        .enumerate()
        .filter(|(_, c)| c.span.source_id == span.source_id && c.span.line <= span.line)
        .max_by_key(|(_, c)| c.span.line)
        .is_some_and(|(i, _)| i > 0)
}

/// An error about the `routes` name in a component other than the first: only the file's
/// first component is the root that owns it, so a helper written above the root leaves the
/// root's own uses unknown. Names the first component and guesses no root (LLP 1156.002 D9).
pub(crate) fn routes_owner(e: &mut TypeError, file: &File) {
    let (Some(routes), Some(first)) = (&file.routes, file.components.first()) else {
        return;
    };
    let slot = &routes.slot;
    let about_slot = match e.id {
        "type-unknown-name" => e.message.starts_with(&format!("unknown name `{slot}`")),
        "type-assign-not-state" => e.message.starts_with(&format!("`{slot}` is not a state")),
        _ => false,
    };
    // A component's span is its keyword: the error's component is the last one at or above it.
    let within = file
        .components
        .iter()
        .filter(|c| c.span.source_id == e.span.source_id && c.span.line <= e.span.line)
        .max_by_key(|c| c.span.line);
    if about_slot && within.is_some_and(|c| !std::ptr::eq(c, first)) {
        let first = &first.name;
        e.message.push_str(&format!(
            "; `routes {slot}` belongs to the file's first component, `{first}`: if `{first}` is a helper, move it below the component the app starts from"
        ));
    }
}
