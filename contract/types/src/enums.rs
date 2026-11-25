//! Finite strings: assignment admission differs from expression unification.
use super::{checks, err, infer, Ref, Scope, Shapes, Ty, TypeError};
use contract_syntax::Expr;

pub(super) fn contains(ty: &Ty) -> bool {
    match ty {
        Ty::Enum(_) => true,
        Ty::Option(t) | Ty::List(t) => contains(t),
        Ty::Action(params) => params.iter().any(contains),
        _ => false,
    }
}

/// Check literal membership recursively, retaining declared constraints.
/// Each child is inferred in its own context: mixed enum/literal lists may
/// have widened during expression inference without becoming unsafe assignments.
pub(super) fn accepts_expr(
    want: &Ty,
    expr: &Expr,
    have: &Ty,
    scope: &Scope,
    shapes: &Shapes,
) -> bool {
    let child = |want: &Ty, expr: &Expr, scope: &Scope| {
        infer(expr, scope, shapes).is_ok_and(|have| accepts_expr(want, expr, &have, scope, shapes))
    };
    match (want, expr) {
        (Ty::Enum(values), Expr::Str(value, _)) => values.contains(value),
        (Ty::Option(inner), Expr::Some(value, _)) => child(inner, value, scope),
        (Ty::List(inner), Expr::List(values, _)) => values.iter().all(|v| child(inner, v, scope)),
        (_, Expr::Ternary(_, a, b, _)) if contains(want) => {
            child(want, a, scope) && child(want, b, scope)
        }
        (
            _,
            Expr::Match {
                subject,
                var,
                some,
                none,
                ..
            },
        ) if contains(want) => {
            let Ok(Ty::Option(inner)) = infer(subject, scope, shapes) else {
                return false;
            };
            let mut inner_scope = scope.clone();
            inner_scope.push(vec![(var.clone(), Ref::Local(0), *inner)]);
            child(want, some, &inner_scope) && child(want, none, scope)
        }
        (
            _,
            Expr::Let {
                name, value, body, ..
            },
        ) if contains(want) => {
            let Ok(t) = infer(value, scope, shapes) else {
                return false;
            };
            let mut inner_scope = scope.clone();
            inner_scope.push(vec![(name.clone(), Ref::Local(0), t)]);
            child(want, body, &inner_scope)
        }
        _ => checks::can_unify(want, have),
    }
}

/// Infer a comparison operand before a list can widen its individual items.
pub(super) fn operand(
    e: &Expr,
    opposite: &Expr,
    scope: &Scope,
    shapes: &Shapes,
) -> Result<Ty, TypeError> {
    let ty = match (e, opposite) {
        (Expr::List(items, _), Expr::List(others, _)) => {
            super::lists::literal(items, |i, item| match others.get(i) {
                Some(other) => operand(item, other, scope, shapes),
                None => infer(item, scope, shapes),
            })?
        }
        (Expr::Some(value, _), Expr::Some(other, _)) => {
            let item = operand(value, other, scope, shapes)?;
            super::held::value(&item, value, "`some(…)`")?;
            Ty::Option(Box::new(item))
        }
        _ => {
            let ty = infer(e, scope, shapes)?;
            compare_literal(&ty, opposite, shapes)?;
            return Ok(ty);
        }
    };
    shapes.bounded(&ty, e.span())?;
    Ok(ty)
}

/// Equality with a dynamic string is valid even if its type is wider than the enum.
pub(super) fn compare_literal(ty: &Ty, expr: &Expr, shapes: &Shapes) -> Result<(), TypeError> {
    match (ty, expr) {
        (Ty::Option(item), Expr::Some(value, _)) => compare_literal(item, value, shapes),
        (Ty::List(item), Expr::List(values, _)) if item.contains_enum() => values
            .iter()
            .try_for_each(|value| compare_literal(item, value, shapes)),
        (Ty::Enum(members), Expr::Str(value, span)) if !members.contains(value) => {
            if let Some(candidate) = &shapes.expanded_comparison {
                candidate.store(true, std::sync::atomic::Ordering::Relaxed);
                return Ok(());
            }
            let nearest = members
                .iter()
                .min_by_key(|member| crate::literals::edit_distance(value, member))
                .expect("an enum has at least one member");
            err(
                "type-enum-comparison",
                format!("{value:?} is not a declared member of `{ty}`; did you mean {nearest:?}?"),
                *span,
            )
        }
        _ => Ok(()),
    }
}

/// A child's body was checked with its declared props. Substitution can turn
/// a string read into a literal, or narrow it to an enum. If that produced a
/// candidate, check the root as written, with the handler types its uses inferred.
pub(super) fn check_authored_root(
    file: &contract_syntax::File,
    expanded: &contract_syntax::Expanded,
    types: &mut super::Types,
    sink: &mut super::Sink,
) {
    let Some(candidate) = types.shapes.expanded_comparison.take() else {
        return;
    };
    if candidate.load(std::sync::atomic::Ordering::Relaxed) {
        let mut root = file.components[0].clone();
        if file.routes.is_some() {
            root.states.insert(0, expanded.root.states[0].clone());
        }
        let mut source = super::Sink::default();
        super::component::check_component(
            &root,
            types,
            expanded.owners.get(..root.states.len()),
            &[],
            &[],
            &types.components[0].actions,
            &mut source,
        );
        let scope = types.component_scope(&expanded.root, &types.components[0]);
        super::uses::check_uses(&root.view, &scope, types, file, &mut source);
        for b in &root.provides {
            source.keep(infer(&b.expr, &scope, &types.shapes));
        }
        for error in source.errors {
            if error.id == "type-enum-comparison" {
                sink.push(error);
            }
        }
    }
    types.shapes.expanded_comparison = Some(candidate);
}

/// An assignment diagnostic keeps the rejected literal visible through wrappers.
pub(super) fn assignment_error(target: &str, want: &Ty, expr: &Expr, have: &Ty) -> String {
    let mut message = format!("`{target}` is `{want}`, cannot assign `{have}`");
    if let Some(value) = rejected(want, expr) {
        message.push_str(&format!("; {value:?} is not a declared member"));
    }
    message
}

/// A state's initializer that does not fit its declared type, worded as an
/// assignment's refusal is.
pub(super) fn initializer_error(state: &str, want: &Ty, expr: &Expr, have: &Ty) -> String {
    let mut message =
        format!("state `{state}` is declared `{want}`, and its initializer is `{have}`");
    if let Some(value) = rejected(want, expr) {
        message.push_str(&format!("; {value:?} is not a declared member"));
    }
    message.push_str(": start it from a value of the declared type, or declare the type it holds");
    message
}

fn rejected<'a>(want: &Ty, expr: &'a Expr) -> Option<&'a str> {
    match (want, expr) {
        (Ty::Enum(values), Expr::Str(value, _)) if !values.contains(value) => Some(value),
        (Ty::Option(inner), Expr::Some(value, _)) => rejected(inner, value),
        (Ty::List(inner), Expr::List(values, _)) => values.iter().find_map(|v| rejected(inner, v)),
        (_, Expr::Ternary(_, a, b, _)) => rejected(want, a).or_else(|| rejected(want, b)),
        (_, Expr::Match { some, none, .. }) => {
            rejected(want, some).or_else(|| rejected(want, none))
        }
        (_, Expr::Let { body, .. }) => rejected(want, body),
        _ => None,
    }
}
