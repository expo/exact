//! Logical row snap areas, queried at release rather than copied into every frame.
use super::*;
use exact_kernel::style::snap::SnapAlign;
use exact_kernel::{Dimension, Display, PositionType, StyleId};

impl Tree {
    /// `geometry`: where the snapport's start is projected to rest along the
    /// list's axis, in the list's border-box coordinates; the snapport's
    /// length; the least and greatest positions of its start; and the
    /// distance of the nearest mounted target. The nearest unmounted row
    /// root's position, in the same coordinates.
    pub(crate) fn collection_snap(
        &self,
        u: &mut Update<'_>,
        kernel: &exact_kernel::Kernel,
        view: ViewId,
        geometry: [f64; 5],
    ) -> Result<Option<f64>, InstanceError> {
        let [proposed, port, minimum, maximum, mut distance] = geometry;
        if distance == 0.0 {
            return Ok(None);
        }
        // UIKit calls this synchronously at drag end. Search the index for a
        // viewport beyond each edge of the projected port, capped at 32 rows
        // before/including the projection's and 32 after it. This covers
        // ordinary card strips while tiny/zero-width rows cannot turn a
        // release into O(N).
        let window = [proposed - port, proposed + 2.0 * port];
        let areas = self.collection_snap_areas(
            u,
            kernel,
            view,
            (proposed, window, 32),
            [port, minimum, maximum],
        )?;
        let mut best: Option<f64> = None;
        for (first, last, _) in areas {
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

    /// The first position past `release` toward `rest` (in
    /// [`Tree::collection_snap`]'s coordinates) that an unmounted row root
    /// with `scroll-snap-stop: always` holds a snap at (CSS Scroll Snap 1
    /// §6.2): an area's first position going forward, its last going back.
    /// At most 256 rows past the release are read: a release-time read
    /// bounded however far the projection goes.
    pub(crate) fn collection_snap_stop(
        &self,
        u: &mut Update<'_>,
        kernel: &exact_kernel::Kernel,
        view: ViewId,
        [release, rest, port, minimum, maximum]: [f64; 5],
    ) -> Result<Option<f64>, InstanceError> {
        // A move under half a point is none, as the hosts' rule has it.
        let e = 0.5;
        if (rest - release).abs() <= e {
            return Ok(None);
        }
        let window = if rest > release {
            [release, rest + port]
        } else {
            [rest - port, release]
        };
        let areas = self.collection_snap_areas(
            u,
            kernel,
            view,
            (release, window, 256),
            [port, minimum, maximum],
        )?;
        let stops = areas.into_iter().filter(|a| a.2);
        Ok(if rest > release {
            stops
                .filter_map(|(from, to, _)| {
                    (from > release + e)
                        .then_some(from)
                        .or((to > release + e).then_some(to))
                })
                .min_by(f64::total_cmp)
        } else {
            stops
                .filter_map(|(from, to, _)| {
                    (to < release - e)
                        .then_some(to)
                        .or((from < release - e).then_some(from))
                })
                .max_by(f64::total_cmp)
        })
    }

    /// The positions each unmounted row root with a snap alignment along the
    /// list's axis rests at, first to last, and whether it says
    /// `scroll-snap-stop: always`, in [`Tree::collection_snap`]'s
    /// coordinates. `rows`: a position, a window of positions, and a cap:
    /// the rows the window covers, at most `cap` up to and including the
    /// position's row and `cap` after it. `snapport`: its length and the
    /// least and greatest positions of its start.
    pub(crate) fn collection_snap_areas(
        &self,
        u: &mut Update<'_>,
        kernel: &exact_kernel::Kernel,
        view: ViewId,
        (around, window, cap): (f64, [f64; 2], usize),
        [port, minimum, maximum]: [f64; 3],
    ) -> Result<Vec<(f64, f64, bool)>, InstanceError> {
        let mut frames = Vec::new();
        let Some((list, _)) = super::super::text::list_region(
            None,
            &self.children,
            view,
            &mut frames,
            crate::ListTextPolicy::Web,
        ) else {
            return Ok(Vec::new());
        };
        if list.index.len() == 0 {
            return Ok(Vec::new());
        }
        let vertical = list.axis == ListAxis::Vertical;
        let origin = kernel
            .node(view)
            .and_then(|n| {
                let (left, top, _, _) = kernel.resolved_padding(n.key)?;
                let (border_left, border_top, _, _) = kernel.resolved_border(n.key)?;
                Some(f64::from(if vertical {
                    top + border_top
                } else {
                    left + border_left
                }))
            })
            .unwrap_or(0.0);
        let row_at = |offset: f64| {
            list.index
                .row_at((offset - origin).max(0.0))
                .map(|p| p.unwrap_or(list.index.len() - 1))
                .map_err(index_error)
        };
        let at = row_at(around)?;
        let first_row = row_at(window[0])?.max((at + 1).saturating_sub(cap));
        let end_row = (row_at(window[1])? + 1).min(at.saturating_add(cap + 1));
        let mut areas = Vec::new();
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
            let size = list.index.height(position).unwrap() - list.index.item_gap();
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
            let Some(align) = style.scroll_snap_align.along(vertical) else {
                continue;
            };
            if root.is_none()
                || style.display == Display::None
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
            // Along the list's axis: the inset and margin before and after,
            // and `scroll-margin`, which grows the snap area (§4.3).
            let rows = if vertical {
                [
                    (StyleId::Top, &style.top),
                    (StyleId::Bottom, &style.bottom),
                    (StyleId::MarginTop, &style.margin_top),
                    (StyleId::MarginBottom, &style.margin_bottom),
                    (StyleId::ScrollMarginTop, &style.rare.scroll_margin_top),
                    (
                        StyleId::ScrollMarginBottom,
                        &style.rare.scroll_margin_bottom,
                    ),
                ]
            } else {
                [
                    (StyleId::Left, &style.left),
                    (StyleId::Right, &style.right),
                    (StyleId::MarginLeft, &style.margin_left),
                    (StyleId::MarginRight, &style.margin_right),
                    (StyleId::ScrollMarginLeft, &style.rare.scroll_margin_left),
                    (StyleId::ScrollMarginRight, &style.rare.scroll_margin_right),
                ]
            };
            let Some([inset_before, inset_after, leading, trailing, grow_before, grow_after]) =
                rows.iter()
                    .map(|(id, value)| absolute(*id, (*value).clone()))
                    .collect::<Option<Vec<_>>>()
                    .and_then(|v| <[f64; 6]>::try_from(v).ok())
            else {
                continue;
            };
            let shift = if style.position_type == PositionType::Relative {
                if *rows[0].1 == Dimension::Auto {
                    -inset_after
                } else {
                    inset_before
                }
            } else {
                0.0
            };
            // Only positive margins occupy the slot; negative ones shift the box.
            let border = (size - leading.max(0.0) - trailing.max(0.0)).max(0.0);
            let lo = start + leading + shift - grow_before;
            let hi = start + leading + shift + border + grow_after;
            let wide = hi - lo >= port;
            let first = match align {
                _ if wide => lo,
                SnapAlign::Start => lo,
                SnapAlign::Center => (lo + hi - port) / 2.0,
                SnapAlign::End => hi - port,
            };
            let maximum = maximum.max(minimum);
            let first = first.clamp(minimum, maximum);
            let last = if wide {
                (hi - port).clamp(first, maximum)
            } else {
                first
            };
            let stop = style.scroll_snap_stop == exact_kernel::ScrollSnapStop::Always;
            areas.push((first, last, stop));
        }
        Ok(areas)
    }
}
