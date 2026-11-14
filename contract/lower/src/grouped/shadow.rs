//! A section's shadow is painted by its card (LLP 1084 D7).
use contract_syntax::{state::state_of, Attr, Expr};
use exact_motion::{Property, TransitionProperty, Transitions};

const SECTION: &str = "$grouped-shadow-section";
const CARD: &str = "$grouped-shadow-card";

/// Keep `class` on both boxes so its rows retain their source and edit scope.
/// The marks select that class's rows before ordinary attribute lowering.
pub(crate) fn part(attrs: &[Attr]) -> Option<bool> {
    attrs.iter().find_map(
        |a| match a.name.strip_prefix(super::MARK).unwrap_or(&a.name) {
            SECTION => Some(false),
            CARD => Some(true),
            _ => None,
        },
    )
}

pub(super) fn mark(card: bool, span: contract_syntax::Span) -> Attr {
    super::s(if card { CARD } else { SECTION }, "", span)
}

pub(crate) fn class_rows(rows: &mut Vec<Attr>, card: bool) {
    rows.retain(|a| a.name == "transition" || moves_to_card(a) == card);
    for a in rows.iter_mut().filter(|a| a.name == "transition") {
        *a = transition(a, card);
    }
}

pub(super) fn is_shadow(a: &Attr) -> bool {
    a.name == "box-shadow" || state_of(&a.name).is_some_and(|(_, row)| row == "box-shadow")
}

pub(super) fn moves_to_card(a: &Attr) -> bool {
    is_shadow(a) && !state_of(&a.name).is_some_and(|(s, _)| matches!(s, "hover" | "focus-visible"))
}

/// Runner-known predicates follow their shadow for the ordinary state fold.
/// Host-known states stay on the section; hosts project only their shadow.
pub(super) fn card(attrs: &[Attr], authored: &[Attr]) -> Vec<Attr> {
    let has_state = |state| {
        authored
            .iter()
            .any(|a| state_of(&a.name).is_some_and(|(s, row)| s == state && row == "box-shadow"))
    };
    let disabled = has_state("disabled");
    let checked = has_state("checked");
    attrs
        .iter()
        .filter_map(|a| {
            if a.name == "transition" {
                return Some(transition(a, true));
            }
            let predicate = (disabled && a.name == "disabled")
                || (checked
                    && matches!(
                        a.name.as_str(),
                        "checked" | "aria-checked" | "aria-selected" | "aria-pressed"
                    ));
            (moves_to_card(a) || predicate || a.name == "class").then(|| a.clone())
        })
        .collect()
}

pub(super) fn transition(a: &Attr, card: bool) -> Attr {
    Attr {
        value: select(&a.value, card),
        ..a.clone()
    }
}

fn select(value: &Expr, card: bool) -> Expr {
    let mut out = value.clone();
    match &mut out {
        Expr::Str(text, _) => {
            let Ok(parsed) = Transitions::parse(text) else {
                return out;
            };
            let mut depth = 0;
            let entries = text.split(|c| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                c == ',' && depth == 0
            });
            let kept: Vec<_> = entries
                .zip(parsed.0)
                .filter_map(|(entry, t)| {
                    if card {
                        if !t.property.covers(Property::BoxShadow) {
                            return None;
                        }
                        if t.property == TransitionProperty::All {
                            let timing = entry
                                .split_whitespace()
                                .filter(|word| !word.eq_ignore_ascii_case("all"))
                                .collect::<Vec<_>>()
                                .join(" ");
                            return Some(format!("box-shadow {timing}"));
                        }
                    } else if t.property == TransitionProperty::Property(Property::BoxShadow) {
                        return None;
                    }
                    Some(entry.trim().to_string())
                })
                .collect();
            *text = if kept.is_empty() {
                "none".into()
            } else {
                kept.join(", ")
            };
        }
        Expr::Ternary(_, yes, no, _) => {
            **yes = select(yes, card);
            **no = select(no, card);
        }
        Expr::Match { some, none, .. } => {
            **some = select(some, card);
            **none = select(none, card);
        }
        Expr::Let { body, .. } => **body = select(body, card),
        // A runtime string keeps its full list on both boxes. Only the
        // section's shadow moves; the card has no other authored targets.
        _ => {}
    }
    out
}

/// A shadow track needs the card's underlying value and paint target on every host.
pub(crate) fn check_animations(
    attrs: &[Attr],
    keyframes: &std::collections::BTreeMap<String, Vec<Property>>,
) -> Result<(), crate::LowerError> {
    let shadows = |name: &str| {
        keyframes
            .get(name)
            .is_some_and(|rows| rows.contains(&Property::BoxShadow))
    };
    if !keyframes.keys().any(|name| shadows(name)) {
        return Ok(());
    }
    fn allowed(
        value: &Expr,
        names: bool,
        absent: Option<&Expr>,
        shadows: &impl Fn(&str) -> bool,
    ) -> bool {
        if let Some(text) = crate::keyframes::constant(value, &[]) {
            if names {
                return text.split(',').all(|name| !shadows(name.trim()));
            }
            return exact_motion::Animations::parse(&text)
                .map_or(true, |list| list.0.iter().all(|a| !shadows(&a.name)));
        }
        match value {
            Expr::Ternary(_, yes, no, _)
            | Expr::Match {
                some: yes,
                none: no,
                ..
            } => allowed(yes, names, absent, shadows) && allowed(no, names, absent, shadows),
            Expr::Typed(inner, ..) => allowed(inner, names, absent, shadows),
            Expr::None(_) => absent.is_none_or(|base| allowed(base, false, None, shadows)),
            _ => false,
        }
    }
    let short = attrs.iter().find(|a| a.name == "animation");
    let names = attrs.iter().find(|a| a.name == "animation-name");
    let exit = attrs.iter().find(|a| a.name == "-exact-exit-animation");
    // Longhands replace the shorthand's names, independently of its timing.
    // An absent longhand arm uses the shorthand's name, as composition does.
    for a in names.or(short).into_iter().chain(exit) {
        let names = a.name == "animation-name";
        let absent = names.then_some(short).flatten().map(|a| &a.value);
        if !allowed(&a.value, names, absent, &shadows) {
            return crate::err(
                "lower-grouped-shadow-animation",
                "a grouped section's shadow belongs to the card; keyframe shadow animation on a grouped section is not supported yet. Use `transition` for shadow changes. Name other animations with literals or choices of literals so their keyframes can be checked",
                a.span,
            );
        }
    }
    Ok(())
}
