//! @ref LLP 1013.001 D1, D4 — a route's entrance and exit: CSS's
//! `::view-transition-new { animation }` and `::view-transition-old`'s,
//! carried as their rows (`style_names.rs`). Each is checked here, where the
//! element's place and its other attributes are known: only a route pushed
//! or presented full screen takes them, and each value is literal, ends and
//! names keyframes that move a picture.

use crate::routes::NavPlace;
use crate::{err, LowerError, Lowerer};
use contract_syntax::{Attr, Expr, VIEW_TRANSITION_NEW, VIEW_TRANSITION_OLD};
use exact_motion::{Animations, Property};

/// The part an attribute is a declaration of, if it is one of the two.
fn part_of(name: &str) -> Option<&'static str> {
    ["::view-transition-new", "::view-transition-old"]
        .into_iter()
        .find(|part| {
            name.strip_prefix(part)
                .is_some_and(|rest| rest.starts_with(' '))
        })
}

/// What to write for a declaration the parts do not take, or for a name an
/// author guesses for them.
pub(crate) fn hint(name: &str) -> Option<String> {
    if let Some(part) = part_of(name) {
        let property = &name[part.len() + 1..];
        return Some(if property.starts_with("animation-") {
            format!("; `{part}` takes the `animation` shorthand alone, which resets every longhand to CSS's initial value: write `animation=\"fade-in 180ms ease-out both\"` (LLP 1013.001 D1)")
        } else {
            format!("; `{part}` admits only `animation`, naming `keyframes` that move `opacity`, `translate`, `scale` and `rotate` (LLP 1013.001 D1)")
        });
    }
    matches!(name, "view-transition-name" | "view-transition-class").then(|| {
        "; Exact names what a transition moves: an element that moves between states takes `sharedElement` (LLP 1013.000), and a route's entrance and exit are its `::view-transition-new` and `::view-transition-old` parts, each with `animation` (LLP 1013.001)".into()
    })
}

impl Lowerer<'_> {
    /// D4's placement and D1's value rules for each part `attrs` declares,
    /// on a `tag` at `place`.
    pub(crate) fn check_route_parts(
        &self,
        place: &NavPlace,
        tag: &str,
        attrs: &[Attr],
    ) -> Result<(), LowerError> {
        let literal = |name: &str| {
            attrs
                .iter()
                .rev()
                .find(|a| a.name == name)
                .and_then(|a| match &a.value {
                    Expr::Str(s, _) => Some(s.as_str()),
                    _ => None,
                })
        };
        for a in attrs
            .iter()
            .filter(|a| a.name == VIEW_TRANSITION_NEW || a.name == VIEW_TRANSITION_OLD)
        {
            let part = part_of(&a.name).expect("one of the two");
            if !place.is_route(attrs) {
                return err(
                    "lower-route-part",
                    format!("`{part}` is a route's entrance or exit, and this `{tag}` is no route (an element with a `navigationKey`, a child of the navigation root or of a linked `role=\"tabpanel\"` in it); an element that moves between states takes `sharedElement` (LLP 1013.000)"),
                    a.span,
                );
            }
            if literal("navigationPresentation") == Some("modal") {
                return err(
                    "lower-route-part",
                    format!("`{part}` is on a route presented as a sheet (`navigationPresentation=\"modal\"`), whose motion is the platform's: its detents, its drag and its dismissal, which keyframes cannot follow. A route that is pushed, or presented `fullscreen`, takes the parts (LLP 1013.001 D4)"),
                    a.span,
                );
            }
            if literal("navigationSource").is_some_and(|s| !s.is_empty()) {
                return err(
                    "lower-route-part",
                    format!("`{part}` is on a route with `navigationSource`, which zooms from its source as the platform does; a route asks for the zoom or for its own transition, not both (LLP 1013.001 D4)"),
                    a.span,
                );
            }
            self.check_part_value(part, &a.value)?;
        }
        Ok(())
    }

    /// Each value a part may take is a literal `animation` (or no
    /// declaration, a class choice's unset side) that ends and names
    /// keyframes moving only what moves a picture.
    fn check_part_value(&self, part: &str, value: &Expr) -> Result<(), LowerError> {
        leaves(value, &mut Vec::new(), &mut |leaf| {
            let (text, span) = match leaf {
                Expr::Str(text, span) => (text, *span),
                Expr::None(_) => return Ok(()),
                other => {
                    return err(
                        "lower-route-part",
                        format!("`{part}`'s `animation` is a literal, or a choice of literals (`still ? \"none\" : \"fade-in 180ms\"`), each checked when the app compiles; a string built at run time is not taken (LLP 1013.001 D1)"),
                        other.span(),
                    )
                }
            };
            // A value the row cannot parse is named by the row's own check.
            let Ok(list) = Animations::parse(text) else {
                return Ok(());
            };
            if list.validate_ending().is_err() {
                return err(
                    "lower-route-part",
                    format!("`{part}`'s `animation` must end: the route's transition lasts until it does, and the route takes no input until then, so it cannot be `infinite` or `paused` (LLP 1013.001 D1)"),
                    span,
                );
            }
            for a in &list.0 {
                let Some(frames) = self.keyframes.get(&a.name) else {
                    return err(
                        "lower-animation-name",
                        format!(
                            "`{part}` names `{}`, and no `keyframes {}` is declared",
                            a.name, a.name
                        ),
                        span,
                    );
                };
                if frames.spatial_rotation() {
                    return err(
                        "lower-route-part",
                        format!("`keyframes {}` rotates outside the screen plane, which a route-transition picture cannot draw on Apple or painted hosts (Linux, Android, Windows); animate a box inside the route instead", a.name),
                        span,
                    );
                }
                let properties = frames.properties();
                let picture = |p: &Property| {
                    matches!(
                        p,
                        Property::Opacity
                            | Property::Translate
                            | Property::Scale
                            | Property::Rotate
                    )
                };
                if let Some(p) = properties.iter().find(|p| !picture(p)) {
                    return err(
                        "lower-route-part",
                        format!("`keyframes {}` animates `{}`, and `{part}` moves a picture of the route: its keyframes take `opacity`, `translate`, `scale` and `rotate` alone (LLP 1013.001 D1)", a.name, p.name()),
                        span,
                    );
                }
            }
            Ok(())
        })
    }
}

/// Visit each value `value` can take: through a choice's or a `match`'s
/// arms, a type ascription and a `let`'s body, where a read of the bound
/// name is its value. Anything else is a leaf, literal or not.
fn leaves<'a>(
    value: &'a Expr,
    bindings: &mut Vec<(&'a str, Option<&'a Expr>)>,
    visit: &mut dyn FnMut(&Expr) -> Result<(), LowerError>,
) -> Result<(), LowerError> {
    match value {
        Expr::Ternary(_, yes, no, _) => {
            leaves(yes, bindings, visit)?;
            leaves(no, bindings, visit)
        }
        Expr::Match {
            var, some, none, ..
        } => {
            bindings.push((var, None));
            let some = leaves(some, bindings, visit);
            bindings.pop();
            some?;
            leaves(none, bindings, visit)
        }
        Expr::Typed(inner, ..) => leaves(inner, bindings, visit),
        Expr::Let {
            name,
            value: bound,
            body,
            ..
        } => {
            bindings.push((name, Some(bound)));
            let body = leaves(body, bindings, visit);
            bindings.pop();
            body
        }
        Expr::Ident(name, _) => match bindings.iter().rposition(|(bound, _)| bound == name) {
            Some(i) if bindings[i].1.is_some() => {
                let bound = bindings[i].1.expect("checked");
                leaves(bound, &mut bindings[..i].to_vec(), visit)
            }
            _ => visit(value),
        },
        _ => visit(value),
    }
}
