//! Attribute templates checked against the same grammar as quoted text.
use super::*;
use std::borrow::Cow;

/// A template's text once its component arguments are substituted. The caller
/// evaluates numeric holes with the VM in the app's function and type scope.
fn template_text(e: &Expr, constant: &impl Fn(&Expr, bool) -> Option<String>) -> Option<String> {
    let Expr::Template(parts, _) = e else {
        return None;
    };
    let mut text = String::new();
    for part in parts {
        match part {
            TemplatePart::Text(s) | TemplatePart::Expr(Expr::Str(s, _)) => text.push_str(s),
            TemplatePart::Expr(Expr::Number(n, _)) => exact_num::push_js(*n, &mut text),
            TemplatePart::Expr(Expr::Ident(..)) => return None,
            TemplatePart::Expr(hole) => {
                text.push_str(&template_text(hole, constant).or_else(|| constant(hole, false))?)
            }
        }
    }
    Some(text)
}

/// Keep the literal-only admission for a platform colour (LLP 1095 D3), including one
/// whose function name spans several template parts.
pub(super) fn fold(e: &mut Expr, name: &str, constant: &impl Fn(&Expr, bool) -> Option<String>) {
    match e {
        Expr::Template(..) | Expr::Binary(contract_syntax::BinOp::Add, ..) | Expr::Call(..) => {
            let text = template_text(e, constant).or_else(|| {
                if !matches!(e, Expr::Template(..))
                    && e.walk().any(|v| matches!(v, Expr::Template(..)))
                {
                    constant(e, true)
                } else {
                    None
                }
            });
            if let Some(text) = text {
                if text.contains("-exact-platform-color(") && tags::style(name) {
                    *e = Expr::Template(vec![TemplatePart::Text(text)], e.span());
                    return;
                }
                // Font admission already folds these forms. Its emitter preserves shared
                // family choices in the plan (LLP 1172.013 step 4).
                if name != "font-family" || !crate::fonts::literal_template(e) {
                    *e = Expr::Str(text, e.span());
                }
            } else if let Expr::Template(parts, _) = e {
                for part in parts {
                    if let TemplatePart::Expr(hole @ Expr::Template(..)) = part {
                        fold(hole, name, constant);
                    }
                }
            }
        }
        Expr::Ternary(_, yes, no, _)
        | Expr::Match {
            some: yes,
            none: no,
            ..
        } => {
            fold(yes, name, constant);
            fold(no, name, constant);
        }
        Expr::Let { value, body, .. } => {
            fold(value, name, constant);
            fold(body, name, constant);
        }
        Expr::Typed(body, ..) => fold(body, name, constant),
        _ => {}
    }
}

pub(crate) fn fold_attr_template(a: &mut Attr, constant: &impl Fn(&Expr, bool) -> Option<String>) {
    fold(&mut a.value, &a.name, constant)
}

pub(crate) fn fold_attr_templates(
    attrs: &mut Cow<'_, [Attr]>,
    constant: &impl Fn(&Expr, bool) -> Option<String>,
) {
    for i in 0..attrs.len() {
        if attrs[i]
            .value
            .walk()
            .any(|e| matches!(e, Expr::Template(..)))
        {
            fold_attr_template(&mut attrs.to_mut()[i], constant);
        }
    }
}

/// Numeric interpolations with no text between them or unit beside them.
pub(crate) fn number_holes<'a>(
    parts: &'a [TemplatePart],
    scope: &Scope,
    shapes: &Shapes,
) -> Option<Vec<&'a Expr>> {
    fn collect<'a>(
        parts: &'a [TemplatePart],
        scope: &Scope,
        shapes: &Shapes,
        holes: &mut Vec<&'a Expr>,
        space: &mut bool,
    ) -> Option<()> {
        for part in parts {
            match part {
                TemplatePart::Text(s) | TemplatePart::Expr(Expr::Str(s, _)) => {
                    if !s.trim_matches([' ', '\t', '\r', '\n', '\x0c']).is_empty() {
                        return None;
                    }
                    *space |= !holes.is_empty() && !s.is_empty();
                }
                TemplatePart::Expr(Expr::Template(inner, _)) => {
                    collect(inner, scope, shapes, holes, space)?
                }
                TemplatePart::Expr(e)
                    if !*space && contract_types::infer(e, scope, shapes).ok()? == Ty::Number =>
                {
                    holes.push(e)
                }
                _ => return None,
            }
        }
        Some(())
    }
    let mut holes = Vec::new();
    collect(parts, scope, shapes, &mut holes, &mut false)?;
    (!holes.is_empty()).then_some(holes)
}

const HOLE_NOTE: &str = ", where `n` stands for the expression inside the hole";

fn single_edit(name: &str, hole: &str, px: bool) -> String {
    let mut edit = format!("write `{name}=({hole})`");
    if px {
        edit.push_str(&format!(", or give it a unit: `${{{hole}}}px`"));
    }
    edit
}

/// Expansion substitutes the caller's arguments. A repair points into the
/// component that wrote the template and must use the name in that source.
pub(crate) fn name_template_holes(checked: &contract_types::Checked, errors: &mut [LowerError]) {
    let mut repairs: std::collections::BTreeMap<_, _> = errors
        .iter_mut()
        .filter(|e| e.message.ends_with(HOLE_NOTE))
        .map(|e| (e.span, e))
        .collect();
    if repairs.is_empty() {
        return;
    }
    let file = checked.file;
    let checking = contract_checking::Checking::new(checked);
    let views = (0..file.components.len())
        .flat_map(|i| checking.authored(i).structure().walk())
        .flat_map(|visit| visit.expressions());
    let derives = file
        .components
        .iter()
        .flat_map(|c| c.derives.iter().map(|d| &d.expr));
    let styles = file
        .styles
        .iter()
        .flat_map(|s| s.attrs.iter().map(|a| &a.value));
    for expr in views.chain(derives).chain(styles).flat_map(Expr::walk) {
        let Expr::Template(parts, span) = expr else {
            continue;
        };
        let Some(error) = repairs.remove(span) else {
            continue;
        };
        let mut holes = parts.iter().filter_map(|p| {
            if let TemplatePart::Expr(e) = p {
                Some(e)
            } else {
                None
            }
        });
        if let (Some(Expr::Ident(name, _)), None) = (holes.next(), holes.next()) {
            if let Some(edit) = error.message.strip_suffix(HOLE_NOTE) {
                error.message = edit
                    .replace("=(n)", &format!("=({name})"))
                    .replace("${n}", &format!("${{{name}}}"));
            }
        }
    }
}

pub(super) fn numeric_template_error(
    a: &Attr,
    scope: &Scope,
    shapes: &Shapes,
    admits_px: impl FnOnce() -> Option<bool>,
) -> Option<LowerError> {
    if !a.value.walk().any(|v| matches!(v, Expr::Template(..))) {
        return None;
    }
    let px = admits_px()?;
    find_arm(
        &a.value,
        scope,
        shapes,
        &|v, scope| {
            let Expr::Template(parts, span) = v else {
                return None;
            };
            let holes = number_holes(parts, scope, shapes)?;
            let name = &a.name;
            let message = match holes.as_slice() {
                [_] => format!("{}{HOLE_NOTE}", single_edit(name, "n", px)),
                _ => {
                    let mut edit =
                        format!("write the number for `{name}` as a parenthesized expression");
                    if px {
                        edit.push_str(", or add `px` before the closing backtick");
                    }
                    edit
                }
            };
            Some(LowerError {
                id: "lower-attr-value",
                message,
                span: *span,
            })
        },
        |_| None,
        &[],
    )
}
pub(crate) fn check_numeric_prop(
    a: &Attr,
    prop: PropId,
    scope: &Scope,
    shapes: &Shapes,
) -> Result<(), LowerError> {
    if matches!(tags::prop_ty(prop), tags::PropTy::Int | tags::PropTy::Float) {
        if let Some(error) = numeric_template_error(a, scope, shapes, || Some(false)) {
            return Err(error);
        }
    }
    Ok(())
}
