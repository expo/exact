//! What a reorder lifts above the page (LLP 1041 §8.5, LLP 1094 D6): a row
//! carried within its own list, painted after its siblings inside that
//! list's clip; and a grouped drag's ghost, the row's subtree painted last,
//! over everything, under no ancestor's clip, at the contact.
//!
//! The ghost is the row itself, painted a second time where the contact
//! holds it: the runner hides the row in its list while the ghost stands
//! for it (`visibility: hidden` on its wrapper), so the ghost's walk shows
//! what that hides. Its boxes are not hit: a ghost is the web's
//! `pointer-events: none` popover.
use super::shadow::ShadowPaint;
use super::{PaintOffset, Painter, Presented, Rect4, Shape, Walk};
use exact_kernel::Kernel;
use exact_kernel::{NodeKey, NodeRef, NodeType, PropId, ViewId};
use tiny_skia::Transform;

/// The lifted rows the presenter asks the painter for.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct Lift {
    /// Within one list: the list and the carried row's wrapper.
    pub(crate) arrange: Option<(NodeKey, NodeKey)>,
    /// A grouped drag's ghost.
    pub(crate) ghost: Option<Ghost>,
}

/// A grouped drag's ghost (LLP 1094 D6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ghost {
    /// The wrapper whose subtree it paints: the row wherever it is now.
    pub(crate) wrapper: NodeKey,
    /// Its border box's top left, viewport points.
    pub(crate) at: (f32, f32),
    /// 1.03 while lifted (1 under reduced motion), easing to 1 as it lands.
    pub(crate) scale: f32,
    /// 1, or fading.
    pub(crate) opacity: f32,
    /// The lifted shadow's strength, 0 to 1, easing out as it lands.
    pub(crate) shadow: f32,
}

impl Painter {
    /// A node's children in paint order: a canvas's placed children first,
    /// then the rest by rank and tree order, and the row its list carries
    /// last, inside the list's clip.
    pub(super) fn children(
        &mut self,
        walk: &mut Walk<'_, '_>,
        node: &NodeRef<'_>,
        ts: Transform,
        child_offset: PaintOffset,
        child_rect: Option<Rect4>,
    ) {
        let lift = self.lifted_child(walk, node);
        // A native button's children are its face, painted above (LLP 1069.011 D5).
        let native =
            node.node_type == NodeType::Control && node.props.str(PropId::Type) == Some("button");
        let mut children: Vec<_> = node
            .children()
            .into_iter()
            .filter(|id| Some(*id) != lift && !native)
            .collect();
        // @ref LLP 1083.000 D5 — a canvas's placed children first, by
        // projective depth, as the web gives them negative indices by
        // depth; then every other child by its rank, then tree order.
        // All in flow and none placed (most parents): tree order as it is.
        self.order_children(&mut children, &walk.ranks);
        // A surface that composites its children takes them in this order.
        if self.composites.canvases.contains(&node.id) {
            self.composite(walk, node, &children, ts, child_offset, child_rect);
            return;
        }
        // A scroller's rows are its children; a scroller holding one
        // container (a column of settings) keeps that container's children
        // apart instead, so one of them changing records only itself.
        let ts = self.zoom_children(walk.scene.kernel, node, ts, child_offset);
        let r = self.drawn_rect(node, child_offset);
        let space = ts.pre_translate(r.0, r.1);
        self.kept.push_space(node.id, space);
        let rows = self.has_rows(walk, node);
        let group = rows && !self.rows_wrapper(node.key);
        let wrap = group && self.wraps_rows(walk, &children);
        let split_sections = rows && self.backend.independent_section_rows();
        let sections = (split_sections && !wrap).then(|| self.row_sections(walk.scene.kernel));
        let group = group
            && (!self.pixel_grid || self.grid().is_some())
            && self.group_can_move(walk.scene.kernel, node.id);
        if group {
            self.group_begin(walk, node.id);
        }
        let kept = self.kept_places(walk, Some(node), &children);
        let (last, count) = (self.stage_after(&children), children.len());
        for (at, child) in children.into_iter().enumerate() {
            self.replay_kept(walk, &kept, at, space);
            if wrap || Self::is_row_section(walk, sections.as_deref(), child) {
                self.wrapped(walk, child, ts, child_offset, child_rect);
            } else if rows {
                self.row(walk, child, ts, child_offset, child_rect);
            } else {
                self.node(walk, child, ts, child_offset, child_rect);
            }
            self.paint_stage(walk, child, Some(child) == last);
        }
        self.replay_kept(walk, &kept, count, space);
        if group {
            self.group_end(walk);
        }
        if let Some(child) = lift {
            self.node(walk, child, ts, child_offset, child_rect);
        }
        self.kept.pop_space();
    }

    /// The row a list carries within itself, painted after its siblings.
    fn lifted_child(&self, walk: &Walk<'_, '_>, list: &NodeRef<'_>) -> Option<ViewId> {
        let (owner, row) = self.lift.arrange?;
        let source = walk.scene.kernel.node_by_key(row)?;
        (owner == list.key && source.parent == Some(list.id)).then_some(source.id)
    }

    /// The ghost, over everything painted so far.
    pub(super) fn paint_ghost(&mut self, walk: &mut Walk<'_, '_>) {
        let Some(ghost) = self.lift.ghost else {
            return;
        };
        let Some(node) = walk.scene.kernel.node_by_key(ghost.wrapper) else {
            return;
        };
        let (w, h) = (node.frame.width, node.frame.height);
        let (cx, cy) = (ghost.at.0 + w / 2., ghost.at.1 + h / 2.);
        let ts = Transform::from_translate(cx, cy)
            .pre_scale(ghost.scale, ghost.scale)
            .pre_translate(-cx, -cy);
        let opacity = ghost.opacity.clamp(0., 1.);
        if opacity < 1. {
            self.backend.push_opacity(opacity);
        }
        // D6's look, the same on every host: `0 8px 24px` at 25% black.
        let rect: Rect4 = (ghost.at.0, ghost.at.1, w, h);
        if ghost.shadow > 0. {
            let shadow = ShadowPaint::lifted(ghost.shadow);
            let outer = Shape::rect(rect);
            self.backend.box_shadow(
                shadow.blurred(&outer),
                &outer,
                &|| shadow.fills(&outer, [0.; 4]),
                ts,
            );
        }
        let boxes = walk.boxes.len();
        let at = self.without_grid(super::grid::changes_axes(ts), |painter| {
            painter.drawn_offset((node.frame.x, node.frame.y))
        });
        let (reveal, page) = (
            walk.reveal,
            (at.0 - f64::from(rect.0), at.1 - f64::from(rect.1)),
        );
        walk.reveal = Some(node.id);
        // Painted after the walk, out of its ancestors, in their scheme.
        let previous = self.dark;
        self.dark = scheme_of(walk.scene.kernel, walk.scene.presented, node.id).unwrap_or(previous);
        self.node(walk, node.id, ts, page, None);
        self.dark = previous;
        walk.reveal = reveal;
        walk.boxes.truncate(boxes);
        if opacity < 1. {
            self.backend.pop_opacity();
        }
    }
}

/// The scheme `id` paints in (LLP 1034 §8): the nearest presented one (a
/// node's `color-scheme`, else a report for its view) through its logical
/// parents, as the walk carries it, for what is painted out of its
/// ancestors; `None` is the session's.
pub(crate) fn scheme_of(
    kernel: &Kernel,
    presented: &dyn Fn(ViewId) -> Presented,
    id: ViewId,
) -> Option<bool> {
    let mut at = Some(id);
    while let Some(id) = at {
        if let Some(dark) = presented(id).dark {
            return Some(dark);
        }
        at = kernel.node(id)?.parent;
    }
    None
}
