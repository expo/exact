//! @ref LLP 1123 D5 — a node's runtime custom properties are one style row:
//! each `--name` attribute, its classes' among them, a declaration in the
//! order they stand, so a later one of a name replaces an earlier one as
//! CSS's cascade does.

use crate::LowerError;
use contract_syntax::{Attr, Expr, Span, TemplatePart};
use contract_types::{Ref, Scope, Shapes, Ty};
use exact_kernel::style::safe_area_properties::{is_author_name, SafeAreaProperties};

/// The one attribute that writes `attrs`' custom properties: each
/// declaration `--name: value; `, none where its value is `none`, so a class
/// choice or an absent option drops one declaration and keeps the rest.
/// Each literal value that is no one length is refused at its own attribute
/// and left out.
pub(crate) fn merged(
    attrs: &[Attr],
    scope: &Scope,
    shapes: &Shapes,
) -> (Option<Attr>, Vec<LowerError>) {
    let mut errors = Vec::new();
    let mut parts: Vec<TemplatePart> = Vec::new();
    let mut first: Option<&Attr> = None;
    for a in attrs.iter().filter(|a| is_author_name(&a.name)) {
        let mut value = a.value.clone();
        let mut varying = None;
        crate::values::call_tokens(&mut value, &mut varying);
        if let Some(token) = varying {
            errors.push(LowerError {
                id: "lower-attr-value",
                message: format!("`{}`: the design token `{token}` differs between the app's modes, and a custom property's value is read at run time, where nothing binds a mode; read the token where the length is", a.name),
                span: a.span,
            });
            continue;
        }
        if let Some((literal, span)) = refused(&value) {
            errors.push(LowerError {
                id: "lower-attr-value",
                message: format!(
                    "`{}=\"{literal}\"`: a custom property's value is one length: px, rem, a percentage, a viewport unit, env() without a fallback, var(), or calc(), min(), max() or clamp() of these; a container unit (cqw, cqh, cqi, cqb, cqmin, cqmax) is not built in one yet",
                    a.name
                ),
                span,
            });
            continue;
        }
        first.get_or_insert(a);
        match fragment(&a.name, value, scope, shapes) {
            Expr::Str(t, _) => text(&mut parts, &t),
            e => parts.push(TemplatePart::Expr(e)),
        }
    }
    let Some(first) = first else {
        return (None, errors);
    };
    let value = match parts.as_slice() {
        [] => Expr::Str(String::new(), first.span),
        [TemplatePart::Text(t)] => Expr::Str(t.clone(), first.span),
        _ => Expr::Template(parts, first.span),
    };
    let attr = Attr {
        name: first.name.clone(),
        value,
        span: first.span,
        written: first.written,
    };
    (Some(attr), errors)
}

/// The first literal of `value`, in any arm of a choice, that is no one
/// length, and where.
fn refused(value: &Expr) -> Option<(String, Span)> {
    match value {
        Expr::Str(s, span) => SafeAreaProperties::value(s)
            .is_none()
            .then(|| (s.clone(), *span)),
        Expr::Ternary(_, yes, no, _)
        | Expr::Match {
            some: yes,
            none: no,
            ..
        } => refused(yes).or_else(|| refused(no)),
        Expr::Let { body, .. } | Expr::Typed(body, ..) => refused(body),
        _ => None,
    }
}

/// The declaration `name: value; ` as an expression, through a choice's
/// arms and a binding's body, each typed in the scope it is in, and nothing
/// where `value` is `none`.
fn fragment(name: &str, value: Expr, scope: &Scope, shapes: &Shapes) -> Expr {
    let declared = |e: Expr| {
        let span = e.span();
        Expr::Template(
            vec![
                TemplatePart::Text(format!("{name}: ")),
                TemplatePart::Expr(e),
                TemplatePart::Text("; ".into()),
            ],
            span,
        )
    };
    let within = |bound: &str, ty: Ty| {
        let mut inner = scope.clone();
        inner.push(vec![(bound.to_string(), Ref::Local(0), ty)]);
        inner
    };
    match value {
        Expr::Str(s, span) => Expr::Str(format!("{name}: {s}; "), span),
        Expr::None(span) => Expr::Str(String::new(), span),
        Expr::Ternary(cond, yes, no, span) => Expr::Ternary(
            cond,
            Box::new(fragment(name, *yes, scope, shapes)),
            Box::new(fragment(name, *no, scope, shapes)),
            span,
        ),
        Expr::Match {
            subject,
            var,
            some,
            none,
            span,
        } => {
            let ty = match contract_types::infer(&subject, scope, shapes) {
                Ok(Ty::Option(inner)) => *inner,
                _ => Ty::Unknown,
            };
            Expr::Match {
                some: Box::new(fragment(name, *some, &within(&var, ty), shapes)),
                none: Box::new(fragment(name, *none, scope, shapes)),
                subject,
                var,
                span,
            }
        }
        Expr::Let {
            name: bound,
            value,
            body,
            span,
        } => {
            let ty = contract_types::infer(&value, scope, shapes).unwrap_or(Ty::Unknown);
            Expr::Let {
                body: Box::new(fragment(name, *body, &within(&bound, ty), shapes)),
                name: bound,
                value,
                span,
            }
        }
        // A literal a component's argument carries is the literal
        Expr::Typed(inner, ..) if matches!(*inner, Expr::None(_) | Expr::Str(..)) => {
            fragment(name, *inner, scope, shapes)
        }
        // An option's value is declared where it is present
        e if matches!(contract_types::infer(&e, scope, shapes), Ok(Ty::Option(_))) => {
            let span = e.span();
            Expr::Match {
                subject: Box::new(e),
                var: VALUE.into(),
                some: Box::new(declared(Expr::Ident(VALUE.into(), span))),
                none: Box::new(Expr::Str(String::new(), span)),
                span,
            }
        }
        e => declared(e),
    }
}

/// The name an option's present value is bound to in its declaration: one
/// no author can write.
const VALUE: &str = "declared$";

/// Append `t` to the template's text, joining it to the text before.
fn text(parts: &mut Vec<TemplatePart>, t: &str) {
    match parts.last_mut() {
        Some(TemplatePart::Text(last)) => last.push_str(t),
        _ => parts.push(TemplatePart::Text(t.to_string())),
    }
}
