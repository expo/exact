//! A positive authored border width with no authored line style.
//! Keep shorthand omissions and class branches distinct from explicit resets.

use crate::LowerError;
use contract_checking::View;
use contract_syntax::{Attr, Expr, Span, StyleDecl, VisitSpans};
use exact_kernel::{StyleId, StyleProps, StyleValue};
use std::collections::{BTreeMap, BTreeSet};

const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];
const ROWS: [[StyleId; 3]; 4] = [
    [
        StyleId::BorderWidthTop,
        StyleId::BorderStyleTop,
        StyleId::BorderColorTop,
    ],
    [
        StyleId::BorderWidthRight,
        StyleId::BorderStyleRight,
        StyleId::BorderColorRight,
    ],
    [
        StyleId::BorderWidthBottom,
        StyleId::BorderStyleBottom,
        StyleId::BorderColorBottom,
    ],
    [
        StyleId::BorderWidthLeft,
        StyleId::BorderStyleLeft,
        StyleId::BorderColorLeft,
    ],
];

#[derive(Clone, Default)]
struct Side {
    width: bool,
    styled: bool,
    class: Option<String>,
}

#[derive(Clone, Default)]
struct Case {
    sides: [Side; 4],
    conditions: Vec<(Expr, bool)>,
}

type Repairs = BTreeMap<Option<String>, BTreeSet<usize>>;

fn select(cases: &[Case], condition: &Expr, truth: bool) -> Vec<Case> {
    let mut condition = condition.clone();
    condition.visit_spans(&mut |span| *span = Span::default());
    cases
        .iter()
        .filter_map(|case| {
            if let Some((_, known)) = case.conditions.iter().find(|(e, _)| *e == condition) {
                return (*known == truth).then(|| case.clone());
            }
            let mut case = case.clone();
            case.conditions.push((condition.clone(), truth));
            Some(case)
        })
        .collect()
}

fn nonzero(value: &Expr) -> bool {
    let v = match value {
        Expr::None(_) => return true,
        Expr::Number(n, _) => StyleValue::Number(*n),
        Expr::Str(s, _) => {
            if StyleValue::text_unsets(s, StyleId::BorderWidthTop) {
                return true;
            }
            StyleValue::Text(s.as_str().into())
        }
        _ => return false,
    };
    let mut props = StyleProps::default();
    if props.set_dynamic(StyleId::BorderWidthTop, &v).is_err() {
        return false;
    }
    match props.relative.get(StyleId::BorderWidthTop) {
        None => props.border_width_top > 0.0,
        Some((unit, n)) => {
            use exact_kernel::style::relative::Unit;
            matches!(
                unit,
                Unit::Hairline | Unit::Ramp(_) | Unit::Rem | Unit::Em | Unit::Viewport(_)
            ) && n > 0.0
        }
    }
}

/// Keep the condition with each case across declarations, including class arms.
fn apply(
    cases: &mut Vec<Case>,
    name: &str,
    value: &Expr,
    written: bool,
    class: Option<&str>,
) -> Option<()> {
    match value {
        Expr::Ternary(condition, yes, no, _) => {
            let mut yes_cases = select(cases, condition, true);
            let mut no_cases = select(cases, condition, false);
            apply(&mut yes_cases, name, yes, written, class)?;
            apply(&mut no_cases, name, no, written, class)?;
            yes_cases.extend(no_cases);
            // A warning is optional; do not expand an unbounded product of choices.
            if yes_cases.len() > 32 {
                return None;
            }
            *cases = yes_cases;
            return Some(());
        }
        // These introduce bindings whose equivalence is not established here.
        Expr::Match { .. } | Expr::Let { .. } => return None,
        _ => {}
    }
    let shorthand = matches!(
        name,
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left"
    );
    let present = if shorthand {
        match value {
            Expr::Str(text, span) => crate::shorthands::border_parts(name, text, *span)
                .ok()?
                .map(|part| part.is_some()),
            // An explicit reset writes the width and the style.
            Expr::None(_) => [true; 3],
            _ => return None,
        }
    } else {
        [true; 3]
    };
    for (id, value) in crate::shorthands::style_values(name, value).ok()? {
        for case in cases.iter_mut() {
            for (side, ids) in case.sides.iter_mut().zip(ROWS) {
                if id == ids[0] {
                    side.width = written && nonzero(&value);
                    side.class = class.map(str::to_owned);
                } else if id == ids[1] {
                    // Any written style, including none/hidden or an unknown
                    // value, is deliberate. An omitted shorthand style resets it.
                    side.styled = present[1];
                }
            }
        }
    }
    Some(())
}

fn affected(attrs: &[Attr], styles: &[StyleDecl]) -> Option<Repairs> {
    inspect(attrs, styles, vec![Case::default()])
}

fn inspect(attrs: &[Attr], styles: &[StyleDecl], mut cases: Vec<Case>) -> Option<Repairs> {
    // Inspect class arms before class::rows erases shorthand provenance and
    // inserts absent-row resets. An absent arm never authored a width.
    if let Some((i, Expr::Ternary(condition, yes, no, _))) = attrs
        .iter()
        .enumerate()
        .find(|(_, a)| a.name == "class")
        .map(|(i, a)| (i, &a.value))
    {
        let mut branch = attrs.to_vec();
        branch[i].value = *yes.clone();
        let mut repairs = inspect(&branch, styles, select(&cases, condition, true))?;
        branch[i].value = *no.clone();
        for (class, sides) in inspect(&branch, styles, select(&cases, condition, false))? {
            repairs.entry(class).or_default().extend(sides);
        }
        return Some(repairs);
    }
    let class = crate::class::rows(
        attrs,
        &|name| {
            styles
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.attrs.as_slice())
        },
        &[],
    )
    .ok()?;
    let mut layers: BTreeMap<&str, Vec<(&Attr, Option<&str>)>> = BTreeMap::new();
    let class_rows = class
        .iter()
        .flat_map(|(name, rows)| rows.iter().map(move |a| (a, Some(name.as_str()))));
    for (a, class) in class_rows
        .chain(attrs.iter().map(|a| (a, None)))
        .filter(|(a, _)| a.name != "class")
    {
        let (state, name) =
            contract_syntax::state::state_of(&a.name).unwrap_or(("", a.name.as_str()));
        if name.starts_with("border") {
            layers.entry(state).or_default().push((a, class));
        }
    }
    if let Some(rows) = layers.remove("") {
        for (a, class) in rows {
            apply(&mut cases, &a.name, &a.value, a.written, class)?;
        }
    }
    // Pseudo-classes currently admit border colours only. If a rejected
    // style row is present, do not suggest a competing style beside its error.
    for rows in layers.values() {
        for (a, class) in rows {
            let (_, name) = contract_syntax::state::state_of(&a.name)?;
            apply(&mut cases, name, &a.value, a.written, *class)?;
        }
    }
    let mut repairs = Repairs::new();
    for case in cases {
        for (i, side) in case.sides.into_iter().enumerate() {
            if side.width && !side.styled {
                repairs.entry(side.class).or_default().insert(i);
            }
        }
    }
    Some(repairs)
}

pub(super) fn check(
    view: &View<'_>,
    styles: &[StyleDecl],
    lowered: &super::TextStyles,
    out: &mut Vec<LowerError>,
) {
    let mut seen = BTreeSet::new();
    for node in view.walk() {
        let Some((tag, _, attrs)) = node.element() else {
            continue;
        };
        // Use the appearance decision already made by lowering (LLP 1104 D2).
        let bare_control = matches!(tag, "button" | "input" | "textarea")
            && lowered
                .get(&(node.span(), node.instance().unwrap(), tag.to_owned()))
                .and_then(Option::as_ref)
                .is_some_and(|style| style.native == Some(false));
        // Bars and grouped sections may have a platform edge.
        if !bare_control
            && !matches!(
                tag,
                "view"
                    | "box"
                    | "column"
                    | "row"
                    | "footer"
                    | "article"
                    | "aside"
                    | "main"
                    | "scroll"
                    | "link"
                    | "text"
            )
        {
            continue;
        }
        let Some(repairs) = affected(attrs, styles).filter(|s| !s.is_empty()) else {
            continue;
        };
        let span = node.span();
        if !seen.insert((span.source_id, span.line, span.col, span.end_col)) {
            continue;
        }
        let sides: BTreeSet<_> = repairs.values().flatten().copied().collect();
        let names = sides
            .iter()
            .map(|&i| SIDES[i])
            .collect::<Vec<_>>()
            .join(", ");
        let repair = repairs
            .iter()
            .map(|(class, sides)| {
                let rows = if sides.len() == 4 {
                    "`border-style=\"solid\"`".to_string()
                } else {
                    sides
                        .iter()
                        .map(|&i| format!("`border-{}-style=\"solid\"`", SIDES[i]))
                        .collect::<Vec<_>>()
                        .join(" and ")
                };
                match class {
                    Some(class) => format!("write {rows} inside `style {class}`"),
                    None => format!("write {rows}"),
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        out.push(LowerError {
            id: "lower-border-unstyled",
            message: format!("The border on this `{tag}` ({names}) has a positive written width without a written line style on at least one path, so CSS draws no line there (React Native defaults to solid); {repair}"),
            span,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contract_syntax::Span;

    #[test]
    fn a_state_style_can_supply_the_line_even_before_the_base_width() {
        let attr = |name: &str, value: &str| Attr {
            name: name.into(),
            value: Expr::Str(value.into(), Span::default()),
            span: Span::default(),
            written: true,
        };
        // Style rows are not yet admitted in pseudo-classes. The warning
        // still gives them the benefit of the doubt beside that refusal.
        for state in contract_syntax::state::STATES {
            let name = format!(":{state} border-top-style");
            for style in ["solid", "none"] {
                let attrs = [attr(&name, style), attr("border-top-width", "1px")];
                assert!(affected(&attrs, &[]).unwrap().is_empty());
            }
        }
        assert!(affected(
            &[attr("class", "unknown"), attr("border-width", "1px")],
            &[]
        )
        .is_none());
    }
}
