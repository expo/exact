//! Inherited rows invalidate only the descendants they reach.
use super::*;

/// Of `slots`, read before they move, those whose base a `direction`
/// written on them or above decides. Natural text turns on it, and a move
/// can change it while the value stays (LLP 1060.000 D3); without natural
/// text, none, and nothing is allocated.
pub(super) fn written_direction(arena: &NodeArena, slots: impl Iterator<Item = u32>) -> Vec<u32> {
    if !arena.natural_text {
        return Vec::new();
    }
    let written = |s: &u32| {
        arena
            .inherited_source(*s, crate::StyleId::Direction)
            .is_some()
    };
    slots.filter(written).collect()
}

/// CSS inheritance: an inherited row that changed on `slot` changes the
/// computed value of every logical descendant that does not set the row
/// itself. Mark and touch them, stopping under a descendant that overrides
/// every changed row, so the receipt names what changed and no host has to
/// re-derive descendants per frame (LLP 1035.000 D4).
pub(super) fn propagate_inherited(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    changed: StyleMask,
    touched: &mut Vec<NodeKey>,
    receipt: &mut CommitReceipt,
) {
    let changed = changed.intersect(StyleMask::INHERITED);
    if changed.is_empty() {
        return;
    }
    let mut stack: Vec<(u32, StyleMask)> =
        arena.children(slot).iter().map(|c| (*c, changed)).collect();
    while let Some((s, rows)) = stack.pop() {
        let stopped = arena
            .style_starts(s)
            .fold(StyleMask::EMPTY, |rows, start| rows.union(start.mask))
            .minus(arena.style(s).written);
        if rows.has(crate::StyleId::FontSize)
            && matches!(
                arena.style(s).relative.get(crate::StyleId::FontSize),
                Some((crate::style::relative::Unit::Em, _))
            )
        {
            arena.relative.invalidate(s);
        }
        let pass = rows.minus(arena.style(s).mask).minus(stopped);
        if pass.is_empty() {
            continue;
        }
        inherited_changed(arena, layout, s, pass, receipt);
        touched.push(arena.key(s));
        stack.extend(arena.children(s).iter().map(|c| (*c, pass)));
    }
}

/// What a changed inherited value does at a node: text rows remeasure its
/// paragraph (the nearest measure owner); layout rows such as direction
/// also rederive the engine style of containers.
pub(super) fn inherited_changed(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    rows: StyleMask,
    receipt: &mut CommitReceipt,
) {
    if rows.has(crate::StyleId::FontSize) {
        arena.relative.invalidate(slot);
    }
    if rows.intersects(StyleMask::LAYOUT) {
        if let Some(node) = arena.taffy(slot) {
            layout.restyle(arena, slot, node);
        }
        receipt.layout_invalidated = true;
    }
    if rows.intersects(StyleMask::TEXT)
        && matches!(arena.node_type(slot), NodeType::Text | NodeType::TextInput)
    {
        invalidate_text(arena, layout, slot);
        receipt.layout_invalidated = true;
    }
    symbol_font_changed(arena, slot, rows, receipt);
    if !rows.minus(StyleMask::TEXT).is_empty() {
        arena.mark_export_dirty(slot, NodeFlags::PAINT_DIRTY);
        if !rows.intersects(StyleMask::TEXT) {
            arena.revise_text(slot, false);
        }
    }
}
