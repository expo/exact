//! Authored inset placement beside the shared region placement (LLP 1021 D2).
pub(in crate::presenter) use exact_kernel::position_try::area::{origin, place, History};
use exact_kernel::{Dimension, Env, StyleProps};

pub(in crate::presenter) fn at_point(
    point: (f32, f32),
    size: (f32, f32),
    viewport: (f32, f32),
) -> (f32, f32) {
    (
        point.0.min(viewport.0 - size.0).max(0.),
        point.1.min(viewport.1 - size.1).max(0.),
    )
}

/// Where a popover's authored `left` and `top` put its border box in a
/// viewport `viewport` big (LLP 1021 D2): past its margin, as CSS places a
/// fixed box by its insets; an axis with none at its margin. `None` when it
/// writes neither, and its invoker places it; `None` too under an authored
/// `position-area`, which CSS lets win. A host presents it as it presents a
/// context menu at that point.
pub(in crate::presenter) fn inset_point(
    style: &StyleProps,
    env: &Env,
    viewport: (f32, f32),
) -> Option<(f32, f32)> {
    if style.position_area.name() != "none" {
        return None;
    }
    let written = |d: Dimension| (d.resolve(env) != Dimension::Auto).then_some(d);
    let (left, top) = (written(style.left.clone()), written(style.top.clone()));
    if left.is_none() && top.is_none() {
        return None;
    }
    let at = |inset: Option<Dimension>, margin: Dimension, basis: f32| {
        inset.map_or(0., |d| d.at_basis(env, basis)) + margin.at_basis(env, viewport.0)
    };
    Some((
        at(left, style.margin_left.clone(), viewport.0),
        at(top, style.margin_top.clone(), viewport.1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_point_stays_inside_the_viewport() {
        assert_eq!(
            at_point((280., 190.), (80., 50.), (300., 200.)),
            (220., 150.)
        );
    }
}
