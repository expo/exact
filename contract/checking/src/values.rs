use contract_syntax::Expr;

/// The possible result expressions, before a value admission checks them:
/// choices become their two arms, ascriptions their value, and a `let`
/// read its initializer in the scope where it was declared. An option
/// match's binding shadows a `let` and resolves a direct `some(value)`
/// payload in the subject's scope; other subjects remain unknown here.
/// Conditions and unused initializers are not result values. Type checks
/// still check them, including every ascription's constraint.
/// Remaining names and computed expressions are opaque to literal checks;
/// a leaf must not be inferred in the caller's scope.
///
/// Each initializer is visited at most once, even if several arms read
/// it. Work is O(source size × lexical depth), storage O(source size),
/// and traversal uses no Rust recursion or expression substitution.
/// @ref LLP 1172.013 E1, E6, §4 step 4.
pub fn value_leaves(value: &Expr) -> impl Iterator<Item = &Expr> {
    struct Binding<'a> {
        name: &'a str,
        value: Option<&'a Expr>,
        outer: Option<usize>,
        read: bool,
    }
    let mut bindings: Vec<Binding<'_>> = Vec::new();
    let mut pending = vec![(value, None)];
    std::iter::from_fn(move || {
        while let Some((value, scope)) = pending.pop() {
            match value {
                Expr::Ternary(_, yes, no, _) => {
                    pending.push((no, scope));
                    pending.push((yes, scope));
                }
                Expr::Match {
                    subject,
                    var,
                    some,
                    none,
                    ..
                } => {
                    pending.push((none, scope));
                    let inner = bindings.len();
                    bindings.push(Binding {
                        name: var,
                        value: match subject.as_ref() {
                            Expr::Some(payload, _) => Some(payload.as_ref()),
                            _ => None,
                        },
                        outer: scope,
                        read: false,
                    });
                    pending.push((some, Some(inner)));
                }
                Expr::Typed(inner, ..) => pending.push((inner, scope)),
                Expr::Let {
                    name, value, body, ..
                } => {
                    let inner = bindings.len();
                    bindings.push(Binding {
                        name,
                        value: Some(value),
                        outer: scope,
                        read: false,
                    });
                    pending.push((body, Some(inner)));
                }
                Expr::Ident(name, _) => {
                    let mut found = scope;
                    while let Some(index) = found {
                        if bindings[index].name == name {
                            break;
                        }
                        found = bindings[index].outer;
                    }
                    if let Some(binding) = found.map(|index| &mut bindings[index]) {
                        if let Some(initializer) = binding.value {
                            if !binding.read {
                                binding.read = true;
                                pending.push((initializer, binding.outer));
                            }
                            continue;
                        }
                    }
                    return Some(value);
                }
                _ => return Some(value),
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn value_choices_resolve_initializers_without_capturing_inner_names() {
        use contract_syntax::{Span, TypeExpr};
        let span = Span::point(1, 1);
        let name = || Expr::Ident("x".into(), span);
        let bind = |value, body| Expr::Let {
            name: "x".into(),
            value: Box::new(value),
            body: Box::new(body),
            span,
        };
        let matched = Expr::Match {
            subject: Box::new(Expr::Ident("option".into(), span)),
            var: "x".into(),
            some: Box::new(name()),
            none: Box::new(name()),
            span,
        };
        let typed = Expr::Typed(
            Box::new(matched),
            TypeExpr::Named("string".into(), span),
            span,
        );
        // The inner initializer reads the outer x; the some arm's x is
        // unknown, while the none arm reads that initializer.
        let value = bind(Expr::Str("outer".into(), span), bind(name(), typed));
        assert_eq!(
            value_leaves(&value).cloned().collect::<Vec<_>>(),
            [name(), Expr::Str("outer".into(), span),]
        );
        let unused = bind(
            Expr::Str("unused".into(), span),
            Expr::Str("result".into(), span),
        );
        assert_eq!(
            value_leaves(&unused).cloned().collect::<Vec<_>>(),
            [Expr::Str("result".into(), span)]
        );
    }

    #[test]
    fn some_payload_is_read_once_in_the_subject_scope() {
        use contract_syntax::Span;
        let span = Span::point(1, 1);
        let name = || Expr::Ident("x".into(), span);
        let leaf = Expr::Str("0 0 8px".into(), span);
        let fallback = Expr::Str("4px 4px".into(), span);
        let value = Expr::Let {
            name: "x".into(),
            value: Box::new(leaf.clone()),
            body: Box::new(Expr::Match {
                subject: Box::new(Expr::Some(Box::new(name()), span)),
                var: "x".into(),
                some: Box::new(Expr::Ternary(
                    Box::new(Expr::Bool(true, span)),
                    Box::new(name()),
                    Box::new(name()),
                    span,
                )),
                none: Box::new(fallback.clone()),
                span,
            }),
            span,
        };
        assert_eq!(
            value_leaves(&value).cloned().collect::<Vec<_>>(),
            [leaf, fallback]
        );
    }

    #[test]
    fn shared_choice_initializers_are_read_once() {
        use contract_syntax::Span;
        let span = Span::point(1, 1);
        let mut value = Expr::Ident("a11".into(), span);
        for depth in (0..12).rev() {
            let initializer = if depth == 0 {
                Expr::Str("leaf".into(), span)
            } else {
                let prior = Expr::Ident(format!("a{}", depth - 1), span);
                Expr::Ternary(
                    Box::new(Expr::Bool(true, span)),
                    Box::new(prior.clone()),
                    Box::new(prior),
                    span,
                )
            };
            value = Expr::Let {
                name: format!("a{depth}"),
                value: Box::new(initializer),
                body: Box::new(value),
                span,
            };
        }
        assert_eq!(
            value_leaves(&value).cloned().collect::<Vec<_>>(),
            [Expr::Str("leaf".into(), span)]
        );
    }
}
