//! Derive inference and diagnostics over their free value dependencies.

use super::first_free;
use crate::{infer, ComponentTypes, Ref, Scope, Shapes, Sink, Ty, TypeError, Types};
use contract_syntax::{Component, Expr, Span, TemplatePart};
use std::collections::BTreeMap;

pub(super) struct Dependencies {
    reads: Vec<Vec<usize>>,
    order: Vec<usize>,
    cyclic: Vec<bool>,
}

impl Dependencies {
    pub(super) fn new(c: &Component) -> Self {
        let index: BTreeMap<_, _> = c
            .derives
            .iter()
            .enumerate()
            .map(|(i, d)| (d.name.as_str(), i))
            .collect();
        let reads: Vec<Vec<usize>> = c
            .derives
            .iter()
            .map(|d| {
                let mut out = Vec::new();
                first_free(&d.expr, &|_| false, &mut |name| {
                    if let Some(&i) = index.get(name) {
                        out.push(i);
                    }
                    false
                });
                out
            })
            .collect();
        let n = reads.len();
        let (mut seen, mut order) = (vec![false; n], Vec::new());
        for i in 0..n {
            visit(i, &reads, &mut seen, &mut order);
        }
        // Reverse traversal groups strongly connected components. Only their
        // members depend on themselves; a reader of a cycle does not.
        let mut reverse = vec![Vec::new(); n];
        for (i, deps) in reads.iter().enumerate() {
            for &d in deps {
                reverse[d].push(i);
            }
        }
        seen.fill(false);
        let mut cyclic = vec![false; n];
        for &i in order.iter().rev() {
            let mut group = Vec::new();
            visit(i, &reverse, &mut seen, &mut group);
            if group.len() > 1 || reads[i].contains(&i) {
                for d in group {
                    cyclic[d] = true;
                }
            }
        }
        Self {
            reads,
            order,
            cyclic,
        }
    }

    /// Derives to a fixpoint, so order does not matter and `?` fills. Each is
    /// inferred after the derives it reads, and its type enters the scope at
    /// once, so a set without a cycle settles in one round (and one to confirm)
    /// rather than one round per link of a chain.
    pub(super) fn settle(&self, c: &Component, ct: &mut ComponentTypes, types: &Types) {
        let first_derive = c.props.len() + c.injects.len() + c.states.len();
        let mut scope = types.component_scope(c, ct);
        for _round in 0..(c.derives.len() + 2) {
            let mut changed = false;
            for &i in &self.order {
                // An expression over a derive not yet typed (`current.ok` while
                // `current` is still `?`) waits for a later round; the strict
                // pass reports what never types.
                if let Ok(t) = infer(&c.derives[i].expr, &scope, &types.shapes) {
                    if t != ct.derives[i] {
                        scope.retype(first_derive + i, t.clone());
                        ct.derives[i] = t;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    pub(super) fn check(
        &self,
        c: &Component,
        ct: &mut ComponentTypes,
        types: &Types,
        sink: &mut Sink,
    ) {
        let scope = types.component_scope(c, ct);
        let mut results: Vec<_> = c
            .derives
            .iter()
            .map(|d| infer(&d.expr, &scope, &types.shapes))
            .collect();
        let mut refused: Vec<_> = results
            .iter()
            .enumerate()
            .map(|(i, r)| r.is_err() || (self.cyclic[i] && matches!(r, Ok(t) if !t.is_complete())))
            .collect();
        for &i in &self.order {
            if !matches!(&results[i], Ok(t) if !t.is_complete()) || self.cyclic[i] {
                continue;
            }
            let untyped_state = first_free(&c.derives[i].expr, &|_| false, &mut |name| {
                matches!(scope.lookup(name), Some((Ref::Slot(_), t)) if !t.is_complete())
            }).is_some();
            if untyped_state || self.reads[i].iter().any(|&d| refused[d]) {
                refused[i] = true;
                continue;
            }
            let d = &c.derives[i];
            let (message, span) = match untyped_literal_in(&d.expr, &scope, &types.shapes) {
                Some((literal, context, span)) => (format!(
                    "cannot infer what `{literal}` holds in `{}`: pair it with a typed arm (`match`/`?:`) or write it where {context} is declared",
                    d.name
                ), span),
                None => (format!("cannot infer the type of `{}`: its expression needs a typed value", d.name), d.span),
            };
            results[i] = Err(TypeError {
                id: "type-cannot-infer",
                message,
                span,
            });
            refused[i] = true;
        }
        // Report dependencies first so the sink drops consequential errors.
        for &i in &self.order {
            let d = &c.derives[i];
            match &results[i] {
                Err(e) => sink.push(e.clone()),
                Ok(t) if !t.is_complete() && self.cyclic[i] => sink.push(TypeError {
                    id: "type-derive-cycle",
                    message: format!(
                        "cannot infer the type of `{}`: it depends on itself through other derives",
                        d.name
                    ),
                    span: d.span,
                }),
                Ok(t) if !t.is_complete() && refused[i] => {}
                Ok(t) => ct.derives[i] = t.clone(),
            }
        }
    }
}

fn visit(start: usize, reads: &[Vec<usize>], seen: &mut [bool], order: &mut Vec<usize>) {
    if std::mem::replace(&mut seen[start], true) {
        return;
    }
    let mut stack = vec![(start, 0usize)];
    while let Some((node, next)) = stack.last_mut() {
        if let Some(&dep) = reads[*node].get(*next) {
            *next += 1;
            if !std::mem::replace(&mut seen[dep], true) {
                stack.push((dep, 0));
            }
        } else {
            let node = *node;
            stack.pop();
            order.push(node);
        }
    }
}

fn untyped_literal_in(
    e: &Expr,
    scope: &Scope,
    shapes: &Shapes,
) -> Option<(&'static str, &'static str, Span)> {
    if infer(e, scope, shapes).is_ok_and(|t| t.is_complete()) {
        return None;
    }
    let part = |x: &Expr| untyped_literal_in(x, scope, shapes);
    match e {
        Expr::None(span) => Some(("none", "an `option<T>`", *span)),
        Expr::List(items, span) if items.is_empty() => Some(("[]", "a `T[]`", *span)),
        Expr::List(items, _) => items.iter().find_map(part),
        Expr::Some(x, _)
        | Expr::Member(x, _, _)
        | Expr::NamedArg(_, x, _)
        | Expr::Typed(x, _, _)
        | Expr::Unary(_, x, _) => part(x),
        // A condition cannot supply the result's missing type.
        Expr::Ternary(_, a, b, _) => part(a).or_else(|| part(b)),
        Expr::Match {
            subject,
            var,
            some,
            none,
            ..
        } => {
            let Ty::Option(item) = infer(subject, scope, shapes).ok()? else {
                return None;
            };
            let mut inner = scope.clone();
            inner.push(vec![(var.clone(), Ref::Local(0), *item)]);
            part(subject)
                .or_else(|| untyped_literal_in(some, &inner, shapes))
                .or_else(|| part(none))
        }
        Expr::Let {
            name, value, body, ..
        } => {
            let ty = infer(value, scope, shapes).ok()?;
            let mut inner = scope.clone();
            inner.push(vec![(name.clone(), Ref::Local(0), ty)]);
            part(value).or_else(|| untyped_literal_in(body, &inner, shapes))
        }
        Expr::Call(name, args, _) => {
            for arg in args {
                let found = if matches!(arg, Expr::Arrow { .. }) {
                    let Ty::List(item) = infer(args.first()?, scope, shapes).ok()? else {
                        return None;
                    };
                    let (body, inner) = crate::lists::callback(name, arg, *item, scope).ok()?;
                    untyped_literal_in(body, &inner, shapes)
                } else {
                    part(arg)
                };
                if found.is_some() {
                    return found;
                }
            }
            None
        }
        Expr::Binary(_, a, b, _) => part(a).or_else(|| part(b)),
        Expr::Template(parts, _) => parts.iter().find_map(|p| match p {
            TemplatePart::Expr(x) => part(x),
            _ => None,
        }),
        Expr::Arrow { .. }
        | Expr::Ident(..)
        | Expr::Number(..)
        | Expr::Str(..)
        | Expr::Bool(..) => None,
    }
}
