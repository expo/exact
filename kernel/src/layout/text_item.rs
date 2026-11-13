//! Placement of anonymous text items (CSS Flexbox §4, Grid §6).
//! Layout baselines, column cuts and hosts use the same answer.

use crate::generated::{
    AlignContent, AlignItems, Display, FlexDirection, FlexWrap, JustifyContent,
};
use crate::{Frame, StyleProps};
use taffy::geometry::{Rect, Size};
use taffy::tree::{LayoutInput, LayoutOutput};

pub(super) fn compute(
    arena: &crate::arena::NodeArena,
    slot: Option<u32>,
    inputs: LayoutInput,
    style: &taffy::Style,
    mut measure: impl FnMut(Size<Option<f32>>, Size<taffy::AvailableSpace>) -> Size<f32>,
) -> (LayoutOutput, Option<Frame>) {
    if !slot.is_some_and(|s| {
        arena.node_type(s) == crate::NodeType::Text && arena.style(s).display == Display::Grid
    }) {
        return (
            taffy::compute_leaf_layout(inputs, style, crate::style::resolve_calc, measure),
            None,
        );
    }
    // The grid algorithm sizes and places the anonymous item; the arena
    // still owns one paragraph, with no authored child box.
    let mut tree = taffy::TaffyTree::<()>::new();
    tree.disable_rounding();
    tree.set_calc_resolver(crate::style::resolve_calc);
    let item = tree.new_leaf(taffy::Style::default()).unwrap();
    let root = tree.new_with_children(style.clone(), &[item]).unwrap();
    // Its output keeps the grid's own overflow, which holds the item where
    // the grid placed it, so a scrolling ancestor reaches its lines.
    let output = tree.compute_boundary_with_measure(root, inputs, |input, _, _, style| {
        taffy::compute_leaf_layout(input, style, crate::style::resolve_calc, &mut measure)
    });
    use taffy::util::ResolveOrZero;
    let padding = style
        .padding
        .resolve_or_zero(inputs.parent_size.width, crate::style::resolve_calc);
    let border = style
        .border
        .resolve_or_zero(inputs.parent_size.width, crate::style::resolve_calc);
    let item = tree.layout(item).unwrap();
    let placement = Frame {
        x: item.location.x - padding.left - border.left,
        y: item.location.y - padding.top - border.top,
        width: item.size.width,
        height: item.size.height,
    };
    (output, Some(placement))
}

fn offset(style: &StyleProps, size: Size<f32>, inset: Rect<f32>, lines: f32) -> f32 {
    let room = size.height - inset.top - inset.bottom;
    let free = (room - lines).max(0.0);
    if style.display != Display::Flex {
        return 0.0;
    }
    let reverse = style.flex_wrap == FlexWrap::WrapReverse;
    let (flex_start, flex_end) = if reverse { (free, 0.0) } else { (0.0, free) };
    match style.flex_direction {
        FlexDirection::Row | FlexDirection::RowReverse => {
            // A wrapping container's one line is placed by `align-content`,
            // the item in it (as tall as it) by nothing more.
            if style.flex_wrap != FlexWrap::Nowrap {
                match style.align_content {
                    AlignContent::Normal | AlignContent::Stretch => {}
                    AlignContent::Center
                    | AlignContent::SpaceAround
                    | AlignContent::SpaceEvenly => return free / 2.0,
                    AlignContent::End => return free,
                    AlignContent::FlexEnd => return flex_end,
                    AlignContent::FlexStart | AlignContent::SpaceBetween => return flex_start,
                    AlignContent::Start => return 0.0,
                }
            }
            match style.align_items {
                AlignItems::Center => free / 2.0,
                AlignItems::End | AlignItems::SelfEnd => free,
                AlignItems::FlexEnd => flex_end,
                AlignItems::FlexStart => flex_start,
                _ => 0.0,
            }
        }
        direction => {
            let reversed = direction == FlexDirection::ColumnReverse;
            match style.justify_content {
                JustifyContent::Center
                | JustifyContent::SpaceAround
                | JustifyContent::SpaceEvenly => free / 2.0,
                JustifyContent::End => free,
                JustifyContent::FlexEnd if !reversed => free,
                JustifyContent::FlexStart
                | JustifyContent::Normal
                | JustifyContent::SpaceBetween
                    if reversed =>
                {
                    free
                }
                _ => 0.0,
            }
        }
    }
}

pub(super) fn baseline_offset(
    arena: &crate::arena::NodeArena,
    slot: Option<u32>,
    inset: Rect<f32>,
    lines: f32,
    output: &taffy::tree::LayoutOutput,
) -> f32 {
    let Some(slot) = slot else { return 0.0 };
    if arena.is_native_text_control(slot)
        && crate::FieldKind::from_props(arena.props(slot)) != crate::FieldKind::Textarea
    {
        return (output.size.height - inset.top - inset.bottom - lines) / 2.0;
    }
    if arena.node_type(slot) != crate::NodeType::Text {
        return 0.0;
    }
    offset(&arena.effective_style(slot), output.size, inset, lines)
}

pub(crate) fn from_layout(
    arena: &crate::arena::NodeArena,
    slot: u32,
    layout: taffy::tree::Layout,
) -> f32 {
    if arena.node_type(slot) != crate::NodeType::Text {
        return 0.0;
    }
    let style = arena.effective_style(slot);
    let scrolls = matches!(
        style.overflow_x,
        crate::Overflow::Hidden | crate::Overflow::Scroll | crate::Overflow::Auto
    ) || matches!(
        style.overflow_y,
        crate::Overflow::Hidden | crate::Overflow::Scroll | crate::Overflow::Auto
    );
    let lines = layout.scrollable_overflow_rect.bottom
        - layout.padding.top
        - if scrolls { layout.padding.bottom } else { 0.0 };
    offset(&style, layout.size, layout.padding + layout.border, lines)
}

impl super::LayoutTree {
    pub(crate) fn text_item(&self, node: taffy::NodeId) -> Option<Frame> {
        self.taffy.get_node_context(node)?.text_item
    }
}
