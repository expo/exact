//! Resolved animation targets are checked after all rows in a batch.
use super::*;
use crate::{StyleId, StyleProps};
use exact_motion::{Easing, Property};

pub(super) fn validate(
    arena: &NodeArena,
    staged: &Staged<'_>,
    ops: &[Op],
) -> Result<(), ApplyError> {
    let mut rows: IdMap<ViewId, (usize, StyleProps)> = IdMap::default();
    let mut kinds: IdMap<ViewId, NodeType> = IdMap::default();
    for (op_index, op) in ops.iter().enumerate() {
        match op {
            Op::CreateView { id, node_type } => {
                kinds.insert(*id, *node_type);
            }
            Op::DestroyView { id } => {
                rows.remove(id);
                kinds.remove(id);
            }
            Op::SetStyle { id, .. } | Op::ClearStyle { id, .. } => {
                let slot = (!matches!(staged.state(*id), State::Created(_)))
                    .then(|| arena.slot_of(*id))
                    .flatten();
                let kind = kinds
                    .get(id)
                    .copied()
                    .or_else(|| slot.map(|s| arena.node_type(s)));
                if !kind.is_some_and(NodeType::is_svg_element) {
                    continue;
                }
                let (_, style) = rows.entry(*id).or_insert_with(|| {
                    (
                        op_index,
                        slot.map(|s| arena.style(s).clone()).unwrap_or_default(),
                    )
                });
                match op {
                    Op::SetStyle { patch, .. } => style.apply_patch(patch),
                    Op::ClearStyle { mask, .. } => style.clear(*mask),
                    _ => unreachable!(),
                }
            }
            _ => {}
        }
    }
    for (id, (op_index, style)) in rows {
        if !matches!(staged.state(id), State::Live(_) | State::Created(_)) {
            continue;
        }
        let underlying = crate::motion::targets(&style)[2].1;
        for (row, animations) in [
            (StyleId::Animation, &*style.animation),
            (StyleId::ExitAnimation, &style.rare.exit_animation),
        ] {
            for animation in &animations.0 {
                if animation.keyframes.properties().contains(&Property::Rotate)
                    && animation
                        .keyframes
                        .track(Property::Rotate, underlying, false, &Easing::Linear)
                        .iter()
                        .any(|(_, _, v)| v.y != 0.0 || v.z != 0.0)
                {
                    return Err(ApplyError::SpatialSvgAnimation {
                        op_index,
                        id,
                        style: row,
                    });
                }
            }
        }
    }
    Ok(())
}
