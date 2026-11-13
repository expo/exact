//! Each commit resolves the relative rows whose inputs changed (LLP 1069.000 D3).
//! A root font size the host set reaches the nodes that inherit it, and
//! affected rows receive their new pixels — parents before children, so an
//! `em` font size builds on its parent's resolved one, as CSS computes it.
//! A hairline row (LLP 1115.000
//! D10.3) is resolved with them, against the display's scale as the host
//! last set it. Hosts that snap borders resolve their used widths against
//! that scale before layout too, without changing the authored rows. A
//! `rem` inside a calculation or a grid track is no row of its own: it
//! resolves at layout against the environment's root font size, which a
//! change here writes, restyling the nodes that hold one.
//! Scalar viewport lengths resolve with these rows; a changed viewport basis
//! invalidates their owners through `Kernel::replace_env` before layout.
use super::*;
use crate::generated::StyleProps;
use crate::style::env::root_font_readers;
use crate::style::relative::MEDIUM;
use crate::StyleId;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Debug, Default, Clone)]
pub(crate) struct Slots {
    pub(crate) all: crate::sorted::SlotSet,
    pending: IdSet<u32>,
    // A sparse queue avoids scanning arena-sized bitsets for a small edit.
    queue: Vec<u32>,
}

impl Slots {
    pub(crate) fn update(&mut self, slot: u32, relative: bool) {
        if relative {
            self.all.insert(slot);
            self.invalidate(slot);
        } else {
            self.forget(slot);
        }
    }

    pub(crate) fn forget(&mut self, slot: u32) {
        self.all.remove(slot);
        self.pending.remove(&slot);
    }

    pub(crate) fn invalidate(&mut self, slot: u32) {
        if self.all.contains(slot) && self.pending.insert(slot) {
            self.queue.push(slot);
        }
    }

    fn invalidate_all(&mut self) {
        for slot in self.all.iter() {
            if self.pending.insert(slot) {
                self.queue.push(slot);
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(super) static RESOLVED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn before_prop(arena: &NodeArena, slot: u32) -> Option<f32> {
    matches!(
        arena.node_type(slot),
        NodeType::Control | NodeType::TextInput
    )
    .then(|| font_basis(arena, slot))
}

pub(super) fn after_prop(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    before: Option<f32>,
    touched: &mut Vec<NodeKey>,
    receipt: &mut CommitReceipt,
) {
    if before.is_some_and(|font| font != font_basis(arena, slot)) {
        arena.relative.invalidate(slot);
        if !arena.style(slot).mask.has(StyleId::FontSize) {
            propagate_inherited(
                arena,
                layout,
                slot,
                StyleMask::of(StyleId::FontSize),
                touched,
                receipt,
            );
        }
    }
}

fn font_basis(arena: &NodeArena, slot: u32) -> f32 {
    arena
        .control_text_start(slot)
        .filter(|s| s.mask.has(StyleId::FontSize))
        .map_or_else(
            || {
                arena
                    .parent(slot)
                    .map_or(arena.document_style.font_size, |s| {
                        arena.computed_source(s, StyleId::FontSize).font_size
                    })
            },
            |s| s.font_size,
        )
}

pub(crate) fn resolve(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    touched: &mut Vec<NodeKey>,
    receipt: &mut CommitReceipt,
) {
    if let Some(px) = arena.root_font_size_next.take() {
        if px != arena.document_style.font_size {
            arena.relative.invalidate_all();
            arena.document_style.font_size = px;
            if px == MEDIUM {
                arena.document_style.mask = arena
                    .document_style
                    .mask
                    .minus(StyleMask::of(StyleId::FontSize));
            } else {
                arena.document_style.mask.set(StyleId::FontSize);
            }
            let rows = StyleMask::of(StyleId::FontSize);
            let slots: Vec<u32> = arena.iter_live().collect();
            for slot in slots {
                // Top-layer and tab metrics use rem even when an authored local
                // font stops inheritance. Refresh their layout and host offers.
                if arena.neutral_tab_root(slot).is_some()
                    || arena.neutral_alert(slot)
                    || arena.neutral_content_popover(slot)
                {
                    control_env_changed(arena, layout, slot);
                    receipt.layout_invalidated = true;
                    touched.push(arena.key(slot));
                }
                if arena.inherited_source(slot, StyleId::FontSize).is_none()
                    && !arena
                        .control_text_start(slot)
                        .is_some_and(|s| s.mask.has(StyleId::FontSize))
                {
                    inherited_changed(arena, layout, slot, rows, receipt);
                    touched.push(arena.key(slot));
                }
            }
            // A `rem` in a calculation or a grid track resolves under the
            // environment, by the handle it was given: a replayed layout
            // would keep what the handle resolved to before
            let mut env = arena.env().clone();
            env.root_font_size = px;
            arena.set_env(env);
            let readers: Vec<(u32, StyleMask)> = (arena.iter_live())
                .map(|slot| (slot, root_font_readers(arena.style(slot))))
                .filter(|(_, rows)| !rows.is_empty())
                .collect();
            if !readers.is_empty() {
                if let Some(tree) = layout.tree() {
                    tree.forget_memo();
                }
            }
            for (slot, rows) in readers {
                style_changed(arena, layout, slot, rows, receipt);
                if let Some(node) = arena.taffy(slot) {
                    layout.mark_dirty(node);
                }
                touched.push(arena.key(slot));
            }
        }
    }
    if let Some(scale) = arena.display_scale_next.take() {
        if arena.display_scale.unwrap_or(1.0) != scale {
            arena.relative.invalidate_all();
        }
        arena.display_scale = Some(scale);
        if arena.env().border_scale.is_some_and(|old| old != scale) {
            let mut env = arena.env().clone();
            env.border_scale = Some(scale);
            arena.set_env(env);
            let slots: Vec<_> = arena
                .iter_live()
                .filter(|&slot| {
                    arena
                        .effective_style(slot)
                        .border_widths()
                        .iter()
                        .any(|w| *w > 0.0)
                })
                .collect();
            let rows = StyleMask::of(StyleId::BorderWidthTop)
                .union(StyleMask::of(StyleId::BorderWidthRight))
                .union(StyleMask::of(StyleId::BorderWidthBottom))
                .union(StyleMask::of(StyleId::BorderWidthLeft));
            for slot in slots {
                style_changed(arena, layout, slot, rows, receipt);
                touched.push(arena.key(slot));
            }
        }
    }
    // Bold Text (LLP 1115.000 D12.3): every text is measured and drawn again
    // at the weight it now means.
    if let Some(on) = arena
        .bold_text_next
        .take()
        .filter(|on| *on != arena.bold_text)
    {
        arena.bold_text = on;
        let rows = StyleMask::of(StyleId::FontWeight);
        let slots: Vec<u32> = arena.iter_live().collect();
        for slot in slots {
            inherited_changed(arena, layout, slot, rows, receipt);
            touched.push(arena.key(slot));
        }
    }
    if arena.relative.queue.is_empty() {
        return;
    }
    let device = crate::style::relative::device_pixel(arena.display_scale.unwrap_or(1.0));
    let depth = |arena: &NodeArena, mut slot: u32| {
        let mut depth = 0u32;
        while let Some(parent) = arena.parent(slot) {
            depth += 1;
            slot = parent;
        }
        depth
    };
    let mut slots = BinaryHeap::new();
    let root = arena.document_style.font_size;
    loop {
        // Resolving a font can invalidate more descendants. Order those
        // with the remaining work before resolving any child of theirs.
        while let Some(slot) = arena.relative.queue.pop() {
            if arena.relative.pending.contains(&slot) {
                slots.push(Reverse((depth(arena, slot), slot)));
            }
        }
        let Some(Reverse((_, slot))) = slots.pop() else {
            break;
        };
        if !arena.relative.pending.contains(&slot) {
            continue;
        }
        #[cfg(test)]
        RESOLVED.with(|count| count.set(count.get() + 1));
        let parent = font_basis(arena, slot);
        let mut next = StyleProps::clone(arena.style(slot));
        let mut changed = StyleMask::EMPTY;
        let relative: Vec<_> = next.relative.iter().collect();
        // `font-size` first: an `em` elsewhere on the node is its font size.
        let mut font = arena.computed_source(slot, StyleId::FontSize).font_size;
        for (id, unit, n) in relative.iter().copied() {
            if id != StyleId::FontSize {
                continue;
            }
            let px = unit.px(n, root, parent, device, arena.env());
            if next.set_resolved(id, px) {
                changed.set(id);
            }
        }
        if next.mask.has(StyleId::FontSize) {
            font = next.font_size;
        }
        for (id, unit, n) in relative {
            if id == StyleId::FontSize {
                continue;
            }
            let px = unit.px(n, root, font, device, arena.env());
            if next.set_resolved(id, px) {
                changed.set(id);
            }
        }
        if changed.is_empty() {
            arena.relative.pending.remove(&slot);
            continue;
        }
        let excluded = crate::flow::is_exclusion(arena, slot);
        arena.set_style(slot, next);
        arena.update_exclusion_count(slot, excluded);
        style_changed(arena, layout, slot, changed, receipt);
        touched.push(arena.key(slot));
        propagate_inherited(arena, layout, slot, changed, touched, receipt);
        arena.relative.pending.remove(&slot);
    }
}
