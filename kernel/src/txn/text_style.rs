//! A platform text style's weight and leading move with what names it (LLP
//! 1115.000 D12.2): its node's size, and whether the `font` shorthand named
//! it. Either change reaches every node that inherits them, a child with a
//! size of its own included.
use super::*;
use crate::StyleId;

/// `changed` with the rows a text style gives, when the size of `slot`
/// moved and it named a style before (`named`) or does now.
pub(super) fn with_style_rows(
    arena: &NodeArena,
    slot: u32,
    changed: StyleMask,
    named: bool,
) -> StyleMask {
    if changed.has(StyleId::FontSize) && (named || arena.named_text_style(slot).is_some()) {
        changed
            .union(StyleMask::of(StyleId::FontWeight))
            .union(StyleMask::of(StyleId::LineHeight))
    } else {
        changed
    }
}

/// `prop` set or cleared on `slot`: when it is `fontShorthand`, its weight
/// and leading move for it and what inherits them.
pub(super) fn shorthand_changed(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    prop: PropId,
    touched: &mut Vec<NodeKey>,
    receipt: &mut CommitReceipt,
) {
    if prop != PropId::FontShorthand {
        return;
    }
    let rows = StyleMask::of(StyleId::FontWeight).union(StyleMask::of(StyleId::LineHeight));
    style_changed(arena, layout, slot, rows, receipt);
    propagate_inherited(arena, layout, slot, rows, touched, receipt);
}

/// Moving the layer of an independent paragraph leaves its text source intact.
pub(super) fn only_translates(arena: &NodeArena, slot: u32, mask: StyleMask) -> bool {
    arena.node_type(slot) == NodeType::Text
        && !arena.is_inline_run(slot)
        && mask.minus(StyleMask::LAYOUT).iter().all(|row| {
            matches!(
                row,
                StyleId::Translate | StyleId::TranslatePercent | StyleId::TranslateZ
            )
        })
}

/// Whether `slot` is effectively out of flow (absolute, fixed, or non-sticky popover).
pub(super) fn is_out_of_flow(arena: &NodeArena, slot: u32) -> bool {
    crate::style::top_layer::layout_position(arena, slot) == crate::PositionType::Absolute
}

/// If `slot` flipped between in-flow and out-of-flow, revise any affected
/// paragraph paint sources under a decorated in-flow ancestor.
pub(super) fn flow_changed(arena: &mut NodeArena, slot: u32, was_out_of_flow: bool) {
    if was_out_of_flow != is_out_of_flow(arena, slot) {
        position_decoration_changed(arena, slot);
    }
}

/// Switching `position` between in-flow and out-of-flow changes whether an
/// ancestor's `text-decoration-line` reaches this paragraph or its in-flow
/// descendant paragraphs (CSS Text Decoration 3 §2.1).
pub(super) fn position_decoration_changed(arena: &mut NodeArena, slot: u32) {
    let mut up = arena.parent(slot);
    let mut decorated = false;
    while let Some(p) = up {
        if arena.style(p).text_decoration_line != crate::TextDecorationLine::None {
            decorated = true;
            break;
        }
        if crate::style::top_layer::layout_position(arena, p) == crate::PositionType::Absolute {
            break;
        }
        up = arena.parent(p);
    }
    if !decorated {
        return;
    }
    if matches!(arena.node_type(slot), NodeType::Text | NodeType::TextInput)
        && !arena.is_inline_run(slot)
    {
        arena.revise_text(slot, false);
        return;
    }
    let mut stack: Vec<u32> = arena.children(slot).to_vec();
    while let Some(child) = stack.pop() {
        if crate::style::top_layer::layout_position(arena, child) == crate::PositionType::Absolute {
            continue;
        }
        if matches!(arena.node_type(child), NodeType::Text | NodeType::TextInput)
            && !arena.is_inline_run(child)
        {
            arena.revise_text(child, false);
        } else {
            stack.extend_from_slice(arena.children(child));
        }
    }
}
