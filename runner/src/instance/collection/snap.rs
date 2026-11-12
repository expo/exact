//! Logical row snap areas, queried at release rather than copied into every frame.
use super::*;
use exact_kernel::{Dimension, Display, PositionType, ScrollSnapAlign, StyleId};

impl Tree {
    pub(crate) fn collection_snap(
        &self,
        u: &mut Update<'_>,
        kernel: &exact_kernel::Kernel,
        view: ViewId,
        geometry: [f64; 4],
    ) -> Result<Option<f64>, InstanceError> {
        let [proposed, port, maximum, mut distance] = geometry;
        if distance == 0.0 {
            return Ok(None);
        }
        let mut frames = Vec::new();
        let Some((list, _)) = super::super::text::list_region(
            None,
            &self.children,
            view,
            &mut frames,
            crate::ListTextPolicy::Web,
        ) else {
            return Ok(None);
        };
        if list.axis != ListAxis::Horizontal || list.index.len() == 0 {
            return Ok(None);
        }
        let origin = kernel
            .node(view)
            .and_then(|n| {
                let (padding, _, _, _) = kernel.resolved_padding(n.key)?;
                let (border, _, _, _) = kernel.resolved_border(n.key)?;
                Some(f64::from(padding + border))
            })
            .unwrap_or(0.0);
        let row_at = |offset: f64| {
            list.index
                .row_at((offset - origin).max(0.0))
                .map(|p| p.unwrap_or(list.index.len() - 1))
                .map_err(index_error)
        };
        let at = row_at(proposed)?;
        // UIKit calls this synchronously at drag end. Search the index for a
        // viewport beyond each edge of the projected port, capped at 32 rows
        // before/including `at` and 32 after it. This covers ordinary card
        // strips while tiny/zero-width rows cannot turn a release into O(N).
        let first_row = row_at(proposed - port)?.max(at.saturating_sub(31));
        let end_row = (row_at(proposed + 2.0 * port)? + 1).min(at.saturating_add(33));
        let mut best: Option<f64> = None;
        for position in first_row..end_row {
            // Mounted views supply their actual border boxes to the presenter;
            // their first measurement may not have reached the index yet.
            if list
                .mounted
                .binary_search_by_key(&position, |r| r.position)
                .is_ok()
            {
                continue;
            }
            let start = origin + list.index.prefix(position).unwrap();
            let size = list.index.height(position).unwrap();
            // Only the row root contributes an unmounted target. Descendant
            // snap areas and layout-dependent displacement require row layout,
            // which this bounded read does not perform.
            let row = list.logical_row(u, position, &frames);
            let root = row.as_ref().ok().and_then(|row| first_root(&row.roots));
            let mut style = StyleProps::default();
            for op in &u.ops {
                if let Op::SetStyle { id, patch } = op {
                    if Some(*id) == root {
                        style.apply_patch(patch);
                    }
                }
            }
            // A failed row may have emitted partial operations. Discard them
            // too, preserving any candidate found in another row.
            u.ops.clear();
            *u.ids = Ids::default();
            u.surfaces.clear();
            u.notes.clear();
            u.shown = Default::default();
            u.renewed.clear();
            if root.is_none()
                || style.display == Display::None
                || style.scroll_snap_align == ScrollSnapAlign::None
                || [
                    StyleId::Transform,
                    StyleId::Translate,
                    StyleId::Rotate,
                    StyleId::Scale,
                ]
                .iter()
                .any(|&id| style.mask.has(id))
            {
                continue;
            }
            // Raw font-relative values still use the provisional font basis.
            // Only absolute lengths are known without laying out the row.
            let absolute = |id, value| {
                if style.relative.get(id).is_some() {
                    return None;
                }
                match value {
                    Dimension::Points(v) => Some(f64::from(v)),
                    Dimension::Auto => Some(0.0),
                    _ => None,
                }
            };
            let (Some(left), Some(right), Some(leading), Some(trailing)) = (
                absolute(StyleId::Left, style.left.clone()),
                absolute(StyleId::Right, style.right.clone()),
                absolute(StyleId::MarginLeft, style.margin_left.clone()),
                absolute(StyleId::MarginRight, style.margin_right.clone()),
            ) else {
                continue;
            };
            let shift = if style.position_type == PositionType::Relative {
                if style.left == Dimension::Auto {
                    -right
                } else {
                    left
                }
            } else {
                0.0
            };
            // Only positive margins occupy the slot; negative ones shift the box.
            let border = (size - leading.max(0.0) - trailing.max(0.0)).max(0.0);
            let lo = start + leading + shift;
            let hi = lo + border;
            let wide = hi - lo >= port;
            let first = if style.scroll_snap_align == ScrollSnapAlign::End && !wide {
                hi - port
            } else {
                lo
            };
            let first = first.clamp(0.0, maximum);
            let last = if wide {
                (hi - port).clamp(first, maximum)
            } else {
                first
            };
            let candidate = proposed.clamp(first, last);
            // Negative margins and relative insets can move the snap outside
            // its indexed slot. Compare only after reading the root's style.
            let next_distance = (candidate - proposed).abs();
            if next_distance <= distance
                && (next_distance < distance || best.is_none_or(|b| candidate < b))
            {
                best = Some(candidate);
                distance = next_distance;
            }
        }
        Ok(best)
    }
}
