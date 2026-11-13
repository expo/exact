//! Computes the [flexbox](https://css-tricks.com/snippets/css/a-guide-to-flexbox/) layout algorithm on [`TaffyTree`](crate::TaffyTree) according to the [spec](https://www.w3.org/TR/css-flexbox-1/)
mod main_size;
mod cross_size;
use main_size::*;
use cross_size::*;

use crate::compute::common::alignment::compute_alignment_offset;
use crate::geometry::{Line, Point, Rect, Size};
use crate::style::{
    AlignContent, AlignContentKeyword, AlignItems, AlignItemsKeyword, AlignSelf, AlignmentSafety, AvailableSpace,
    Contain, JustifyContent, LengthPercentageAuto, Overflow, Position,
};
use crate::style::{CoreStyle, FlexDirection, FlexboxContainerStyle, FlexboxItemStyle};
use crate::style_helpers::{TaffyMaxContent, TaffyMinContent};
use crate::tree::{
    Baselines, Layout, LayoutInput, LayoutOutput, RunMode, SizingMode, StaticAlignment, StaticPosition,
};
use crate::tree::{LayoutFlexboxContainer, LayoutPartialTreeExt, NodeId};
use crate::compute::common::absolute::AbsolutePass;
use crate::compute::ratio::{container_sizes_through_ratio, sizes_through_ratio, transfer_into_unsized, Ratio};
use crate::util::debug::debug_log;
use crate::util::sys::{f32_max, f32_min, floor, new_vec_with_capacity, Vec};
use crate::util::MaybeMath;
use crate::util::{MaybeResolve, ResolveOrZero};
use crate::{BoxGenerationMode, BoxSizing, Dimension, Direction, RequestedAxis};

use super::common::alignment::apply_alignment_fallback;
#[cfg(feature = "content_size")]
use super::common::scrollable_overflow::{
    compute_scrollable_overflow_contribution, margin_box_contribution, with_end_padding,
};
use super::common::sizing_keyword::{
    resolve_sizing_keyword, SizingKeywordResolution,
};

/// The intermediate results of a flexbox calculation for a single item
struct FlexItem {
    /// The identifier for the associated node
    node: NodeId,

    /// The order of the node relative to it's siblings
    order: u32,

    /// The base size of this item
    size: Size<Option<f32>>,
    /// The raw size style of this item. Used to detect and resolve sizing
    /// keywords (`min-content`, `max-content`, `fit-content`, `fit-content(...)`, and `stretch`)
    size_style: Size<Dimension>,
    /// The minimum allowable size of this item
    min_size: Size<Option<f32>>,
    /// The maximum allowable size of this item
    max_size: Size<Option<f32>>,
    /// The aspect ratio of this item
    aspect_ratio: Option<f32>,
    /// The cross-alignment of this item
    align_self: AlignSelf,

    /// The overflow style of the item
    overflow: Point<Overflow>,
    /// The contain style of the item
    contain: Contain,
    /// The width of the scrollbars (if it has any)
    scrollbar_width: f32,
    /// The flex shrink style of the item
    flex_shrink: f32,
    /// The flex grow style of the item
    flex_grow: f32,
    /// Whether the computed flex base size is definite, including a size transferred through a ratio.
    flex_basis_is_definite: bool,
    /// Whether flex-basis (or the main size for auto) resolves without a content calculation.
    used_flex_basis_is_definite: bool,

    /// The minimum size of the item. This differs from min_size above because it also
    /// takes into account content based automatic minimum sizes
    resolved_minimum_main_size: f32,

    /// The final offset of this item
    inset: Rect<Option<f32>>,
    /// The margin of this item
    margin: Rect<f32>,
    /// Whether each margin is an auto margin or not
    margin_is_auto: Rect<bool>,
    /// The padding of this item
    padding: Rect<f32>,
    /// The border of this item
    border: Rect<f32>,

    /// The default size of this item
    flex_basis: f32,
    /// The default size of this item, minus padding and border
    inner_flex_basis: f32,
    /// The amount by which this item has deviated from its target size
    violation: f32,
    /// Is the size of this item locked
    frozen: bool,

    /// The proposed inner size of this item
    hypothetical_inner_size: Size<f32>,
    /// The proposed outer size of this item
    hypothetical_outer_size: Size<f32>,
    /// The size that this item wants to be
    target_size: Size<f32>,
    /// The size that this item wants to be, plus any padding and border
    outer_target_size: Size<f32>,
    /// Whether stretching changed the cross size at browser layout-unit precision
    cross_size_was_stretched: bool,

    /// The position of the bottom edge of this item
    baseline: f32,

    /// A temporary value for the main offset
    ///
    /// Offset is the relative position from the item's natural flow position based on
    /// relative position values, alignment, and justification. Does not include margin/padding/border.
    offset_main: f32,
    /// A temporary value for the cross offset
    ///
    /// Offset is the relative position from the item's natural flow position based on
    /// relative position values, alignment, and justification. Does not include margin/padding/border.
    offset_cross: f32,
}

impl FlexItem {
    /// Returns true if the item is a <https://www.w3.org/TR/css-overflow-3/#scroll-container>
    fn is_scroll_container(&self) -> bool {
        self.overflow.x.is_scroll_container() | self.overflow.y.is_scroll_container()
    }

    /// Returns true if the item participates in baseline alignment: it has `align-self: baseline`
    /// and neither of its cross-axis margins are `auto`.
    /// See <https://www.w3.org/TR/css-flexbox-1/#baseline-participation>
    fn participates_in_baseline_alignment(&self, dir: FlexDirection) -> bool {
        self.align_self == AlignSelf::BASELINE
            && !self.margin_is_auto.cross_start(dir)
            && !self.margin_is_auto.cross_end(dir)
    }
}

/// A line of [`FlexItem`] used for intermediate computation
struct FlexLine<'a> {
    /// The slice of items to iterate over during computation of this line
    items: &'a mut [FlexItem],
    /// The dimensions of the cross-axis
    cross_size: f32,
    /// The relative offset of the cross-axis
    offset_cross: f32,
}

/// Values that can be cached during the flexbox algorithm
struct AlgoConstants {
    /// The direction of the current segment being laid out
    dir: FlexDirection,
    /// The layout direction of the current segment being laid out
    layout_direction: Direction,
    /// Is this segment a row
    is_row: bool,
    /// Is this segment a column
    is_column: bool,
    /// Is wrapping enabled (in either direction)
    is_wrap: bool,
    /// Is the wrap direction inverted
    is_wrap_reverse: bool,
    /// Are items balanced across lines (`flex-wrap: balance`)?
    #[cfg(feature = "flexbox_balance")]
    is_balance: bool,
    /// The requested minimum number of lines (`flex-line-count`). `Some` for every
    /// multi-line container, `None` for `nowrap`.
    #[cfg(feature = "flexbox_balance")]
    line_count: Option<u16>,

    /// The item's min_size style
    min_size: Size<Option<f32>>,
    /// The item's max_size style
    max_size: Size<Option<f32>>,
    /// The border of this section
    border: Rect<f32>,
    /// The space between the content box and the border box.
    /// This consists of padding + border + scrollbar_gutter.
    content_box_inset: Rect<f32>,
    /// The size reserved for scrollbar gutters in each axis
    scrollbar_gutter: Point<f32>,
    /// Whether the node being laid out is a scroll container
    #[cfg(feature = "content_size")]
    is_scroll_container: bool,
    /// The gap of this section
    gap: Size<f32>,
    /// The align_items property of this node
    align_items: AlignItems,
    /// The align_content property of this node
    align_content: AlignContent,
    /// The justify_content property of this node
    justify_content: Option<JustifyContent>,

    /// The border-box size of the node being laid out (if known)
    node_outer_size: Size<Option<f32>>,
    /// The content-box size of the node being laid out (if known)
    node_inner_size: Size<Option<f32>>,
    /// Whether the known main size of the node (if any) is definite. This is `false` when a parent
    /// imposes a main size on this node that is derived from the node's own content, in which case
    /// it is indefinite for the purposes of resolving percentage sizes of items and collecting items
    /// into flex lines. See <https://www.w3.org/TR/css-flexbox-1/#definite-sizes>.
    known_main_size_is_definite: bool,
    /// Whether the node has a known main size which is definite (as of the start of layout,
    /// before the main size is determined from the node's contents)
    has_definite_main_size: bool,
    /// Whether the node has a known cross size which is definite
    has_definite_cross_size: bool,
    /// Whether the cross-axis space that non-stretched items are fit-content sized into is
    /// definite (either the node's cross size is definite, or the cross-axis available space is).
    /// A size resolved by fit-content sizing against definite available space is itself definite
    /// (see <https://www.w3.org/TR/css-sizing-3/#definite>).
    cross_axis_available_space_is_definite: bool,

    /// The size of the virtual container containing the flex items.
    container_size: Size<f32>,
    /// The size of the internal container
    inner_container_size: Size<f32>,
}

impl AlgoConstants {
    /// When a multi-line container requests a minimum number of lines (`flex-line-count`),
    /// definite cross-axis available space for measuring items is divided between the requested
    /// number of lines (after subtracting the cross-axis gaps between them).
    /// See <https://github.com/w3c/csswg-drafts/issues/13414>
    #[inline]
    fn divided_cross_space(&self, cross_available_space: f32) -> f32 {
        #[cfg(feature = "flexbox_balance")]
        if let Some(line_count) = self.line_count {
            if line_count > 1 {
                let line_count = line_count as f32;
                return (cross_available_space - (line_count - 1.0) * self.gap.cross(self.dir)) / line_count;
            }
        }
        cross_available_space
    }
}

/// Computes the layout of a box according to the flexbox algorithm
pub fn compute_flexbox_layout(
    tree: &mut impl LayoutFlexboxContainer,
    node: NodeId,
    inputs: LayoutInput,
) -> LayoutOutput {
    let LayoutInput { known_dimensions, parent_size, run_mode, .. } = inputs;
    let style = tree.get_flexbox_container_style(node);

    // Pull these out earlier to avoid borrowing issues
    let contain = style.contain();
    let padding = style.padding().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let border = style.border().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let padding_border_sum = padding.sum_axes() + border.sum_axes();
    let box_sizing_adjustment =
        if style.box_sizing() == BoxSizing::ContentBox { padding_border_sum } else { Size::ZERO };

    // EXACT PATCH 12: the container's own size styles through the ratio (`compute::ratio`).
    let raw_max_size =
        style.max_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment);
    let (style_size, min_size, mut max_size) = sizes_through_ratio(
        &style,
        padding_border_sum,
        style.size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment),
        style.min_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment),
        raw_max_size,
    );
    // EXACT PATCH 57: with a supplied height, transferred bounds must not short-circuit the inline content probe.
    if inputs.sizing_mode == SizingMode::ContentSize && !style.is_compressible_replaced() && known_dimensions.height.is_some() {
        max_size.width = raw_max_size.width;
    }
    let clamped_style_size = if inputs.sizing_mode == SizingMode::InherentSize {
        style_size.maybe_clamp(min_size, max_size)
    } else {
        Size::NONE
    };

    // EXACT PATCH 43: height bounds must not size an unresolved intrinsic width.
    // Width bounds still size height, including in a flex column's content probe.
    // A block parent's intrinsic width probe uses InherentSize for its child.
    let mut min_max_definite_size = min_size.zip_map(max_size, |min, max| match (min, max) {
        (Some(min), Some(max)) if max <= min => Some(min),
        _ => None,
    });
    if known_dimensions.or(clamped_style_size).or(min_max_definite_size).width.is_none()
        && (inputs.sizing_mode == SizingMode::ContentSize || !inputs.available_space.width.is_definite())
    {
        min_max_definite_size.height = None;
    }

    // The size of the container should be floored by the padding and border
    let styled_based_known_dimensions =
        known_dimensions.or(min_max_definite_size.or(clamped_style_size).maybe_max(padding_border_sum));

    // Short-circuit layout if the container's size is fully determined by the container's size and the run mode
    // is ComputeSize (and thus the container's size is all that we're interested in)
    if run_mode == RunMode::ComputeSize {
        if let Size { width: Some(width), height: Some(height) } = styled_based_known_dimensions {
            return LayoutOutput::from_outer_size(Size { width, height });
        }

        // We can also short-circuit if the width is known and only the width has been requested.
        if inputs.axis == RequestedAxis::Horizontal {
            if let Some(width) = styled_based_known_dimensions.width {
                return LayoutOutput::from_outer_size(Size { width, height: 0.0 });
            }
        }
    }

    // Short-circuit layout if the container's size is fully determined by the container's size and the run mode
    // is ComputeSize (and thus the container's size is all that we're interested in)
    if run_mode == RunMode::ComputeSize {
        if let Size { width: Some(width), height: Some(height) } = styled_based_known_dimensions {
            return LayoutOutput::from_outer_size(Size { width, height });
        }
    }

    debug_log!("FLEX:", dbg:style.flex_direction());

    // Parent flags apply only to supplied dimensions. Resolved preferred sizes are definite.
    let mut known_dimensions_are_definite = inputs
        .known_dimensions_are_definite
        .zip_map(known_dimensions, |is_definite, known_dimension| is_definite || known_dimension.is_none());
    // EXACT PATCH 44: height bounds and an absolute row's measured, capped
    // auto height supply a used cross size, not a definite flex basis input.
    // Authored sizes, ratios and opposing insets still supply definite heights.
    let measured_absolute_height = style.position() == Position::Absolute
        && style_size.height.is_none()
        && style.aspect_ratio().is_none()
        && max_size.height.is_some()
        && (style.inset().top.is_auto() || style.inset().bottom.is_auto());
    if style.flex_direction().is_row()
        && (measured_absolute_height || (known_dimensions.height.is_none() && clamped_style_size.height.is_none()))
    {
        known_dimensions_are_definite.height = false;
    }
    drop(style);

    let mut output = compute_preliminary(
        tree,
        node,
        LayoutInput { known_dimensions: styled_based_known_dimensions, known_dimensions_are_definite, ..inputs },
    );

    // Layout containment suppresses the box's baseline for baseline-alignment purposes
    if contain.suppresses_baseline() {
        output.baselines = Baselines::NONE;
    }

    output
}

/// Compute a preliminary size for an item
fn compute_preliminary(tree: &mut impl LayoutFlexboxContainer, node: NodeId, inputs: LayoutInput) -> LayoutOutput {
    let LayoutInput { known_dimensions, parent_size, available_space, run_mode, .. } = inputs;

    // Define some general constants we will need for the remainder of the algorithm.
    let mut constants = compute_constants(
        tree,
        tree.get_flexbox_container_style(node),
        known_dimensions,
        inputs.known_dimensions_are_definite,
        parent_size,
        available_space,
        inputs.sizing_mode,
    );
    let known_dimensions = constants.node_outer_size;

    // 9. Flex Layout Algorithm

    // 9.1. Initial Setup

    // 1. Generate anonymous flex items as described in §4 Flex Items.
    debug_log!("generate_anonymous_flex_items");
    // EXACT PATCH 4: retain capacity, never results, across flex layouts.
    let mut flex_items = flex_item_scratch::Scratch::take();
    generate_anonymous_flex_items(tree, node, &constants, &mut flex_items);
    // EXACT PATCH 54: a column's automatic inline minimum depends on the
    // flexed height. Resolve it during cross sizing, not before main sizing.
    for child in flex_items.iter_mut().filter(|_| constants.is_row) {
        if let Some(width) = crate::compute::ratio::minimum_ratio_width(tree, child.node, constants.node_inner_size, None) {
            child.min_size.width = Some(width).maybe_max(child.min_size.width);
        }
    }

    // 9.2. Line Length Determination

    // 2. Determine the available main and cross space for the flex items
    debug_log!("determine_available_space");
    let available_space = determine_available_space(known_dimensions, available_space, &constants);

    // 3. Determine the flex base size and hypothetical main size of each item.
    debug_log!("determine_flex_base_size");
    determine_flex_base_size(tree, &constants, available_space, &mut flex_items);

    #[cfg(feature = "debug")]
    for item in flex_items.iter() {
        debug_log!("item.flex_basis", item.flex_basis);
        debug_log!("item.inner_flex_basis", item.inner_flex_basis);
        debug_log!("item.hypothetical_outer_size", dbg:item.hypothetical_outer_size);
        debug_log!("item.hypothetical_inner_size", dbg:item.hypothetical_inner_size);
        debug_log!("item.resolved_minimum_main_size", dbg:item.resolved_minimum_main_size);
    }

    // 4. Determine the main size of the flex container
    // This has already been done as part of compute_constants. The inner size is exposed as constants.node_inner_size.

    // 9.3. Main Size Determination

    // 5. Collect flex items into flex lines.
    debug_log!("collect_flex_lines");
    #[cfg(feature = "flexbox_balance")]
    let mut flex_lines = if constants.is_balance {
        collect_balanced_flex_lines(&constants, available_space, &mut flex_items)
    } else {
        collect_flex_lines(&constants, available_space, &mut flex_items)
    };
    #[cfg(not(feature = "flexbox_balance"))]
    let mut flex_lines = collect_flex_lines(&constants, available_space, &mut flex_items);

    // If container size is undefined, determine the container's main size
    // and then re-resolve gaps based on newly determined size
    debug_log!("determine_container_main_size");
    // EXACT PATCH 59: a ratio height in node_inner_size is a percentage
    // basis, not a used automatic height; content can exceed its floor.
    if let Some(outer_main_size) = constants.node_outer_size.main(constants.dir) {
        let inner_main_size = f32_max(outer_main_size - constants.content_box_inset.main_axis_sum(constants.dir), 0.0);
        constants.inner_container_size.set_main(constants.dir, inner_main_size);
        constants.container_size.set_main(constants.dir, outer_main_size);
    } else {
        // Sets constants.container_size and constants.outer_container_size
        determine_container_main_size(tree, available_space, &mut flex_lines, &mut constants);
        constants.node_inner_size.set_main(constants.dir, Some(constants.inner_container_size.main(constants.dir)));
        constants.node_outer_size.set_main(constants.dir, Some(constants.container_size.main(constants.dir)));

        // EXACT PATCH 61: repeat item generation once the intrinsic width
        // supplies the ratio's percentage-height basis. Pass only the width:
        // the automatic height can grow beyond that basis with the content.
        // The known width prevents another repeat; retain the input definiteness.
        // https://www.w3.org/TR/css-sizing-4/#aspect-ratio-automatic
        if constants.is_row && known_dimensions.height.is_none()
            && tree.get_flexbox_container_style(node).aspect_ratio().is_some()
        {
            let known_dimensions = known_dimensions.with_main(constants.dir, Some(constants.container_size.width));
            drop(flex_lines);
            drop(flex_items);
            return compute_preliminary(tree, node, LayoutInput { known_dimensions, ..inputs });
        }

        debug_log!("constants.node_outer_size", dbg:constants.node_outer_size);
        debug_log!("constants.node_inner_size", dbg:constants.node_inner_size);

        // Re-resolve percentage gaps
        let style = tree.get_flexbox_container_style(node);
        let inner_container_size = constants.inner_container_size.main(constants.dir);
        let new_gap = style
            .gap()
            .main(constants.dir)
            .maybe_resolve(inner_container_size, |val, basis| tree.calc(val, basis))
            .unwrap_or(0.0);
        drop(style);
        constants.gap.set_main(constants.dir, new_gap);

        // EXACT PATCH 47: max-content width uses the unwrapped sum, but its
        // clamped inner width governs line breaks for both measured height and
        // final frames. A max-content root need not receive another layout pass.
        // https://www.w3.org/TR/css-flexbox-1/#main-sizing
        // EXACT PATCH 48: min-content width can fit several flex bases, or be
        // narrower than the maximum used for initial line collection. It also
        // needs fresh flex bases and line breaks at the resolved width before
        // measuring height. A base measured under MinContent can be smaller
        // than its content-based size once the container width is known.
        let rewrap = constants.is_row
            && constants.is_wrap
            && (available_space.width == AvailableSpace::MinContent
                || (available_space.width == AvailableSpace::MaxContent && constants.max_size.width.is_some()));
        #[cfg(feature = "flexbox_balance")]
        let rewrap = rewrap && !constants.is_balance;
        if rewrap {
            drop(flex_lines);
            if available_space.width == AvailableSpace::MinContent {
                let known_dimensions = known_dimensions.with_main(
                    constants.dir, Some(constants.container_size.main(constants.dir)),
                );
                drop(flex_items);
                return compute_preliminary(tree, node, LayoutInput { known_dimensions, ..inputs });
            }
            flex_lines = collect_flex_lines(
                &constants,
                Size { width: AvailableSpace::Definite(inner_container_size), ..available_space },
                &mut flex_items,
            );
        }
    }

    // 6. Resolve the flexible lengths of all the flex items to find their used main size.
    debug_log!("resolve_flexible_lengths");
    for line in &mut flex_lines {
        resolve_flexible_lengths(line, &constants);
    }

    // 9.4. Cross Size Determination

    // 7. Determine the hypothetical cross size of each item.
    debug_log!("determine_hypothetical_cross_size");
    for line in &mut flex_lines {
        determine_hypothetical_cross_size(tree, line, &constants, available_space);
    }

    // Calculate child baselines. This function is internally smart and only computes child baselines
    // if they are necessary.
    debug_log!("calculate_children_base_lines");
    let laid_out_children = calculate_children_base_lines(tree, known_dimensions, available_space, &mut flex_lines, &constants);
    // EXACT PATCH 72: the cached final layout no longer describes these children.
    if run_mode == RunMode::ComputeSize && laid_out_children {
        tree.invalidate_final_layout(node);
    }

    // 8. Calculate the cross size of each flex line.
    debug_log!("calculate_cross_size");
    calculate_cross_size(&mut flex_lines, known_dimensions, &constants);

    // 9. Handle 'align-content: stretch'.
    debug_log!("handle_align_content_stretch");
    handle_align_content_stretch(&mut flex_lines, known_dimensions, &constants);

    // 10. Collapse visibility:collapse items. If any flex items have visibility: collapse,
    //     note the cross size of the line they’re in as the item’s strut size, and restart
    //     layout from the beginning.
    //
    //     In this second layout round, when collecting items into lines, treat the collapsed
    //     items as having zero main size. For the rest of the algorithm following that step,
    //     ignore the collapsed items entirely (as if they were display:none) except that after
    //     calculating the cross size of the lines, if any line’s cross size is less than the
    //     largest strut size among all the collapsed items in the line, set its cross size to
    //     that strut size.
    //
    //     Skip this step in the second layout round.

    // TODO implement once (if ever) we support visibility:collapse

    // 11. Determine the used cross size of each flex item.
    debug_log!("determine_used_cross_size");
    determine_used_cross_size(tree, &mut flex_lines, &constants);

    // 9.5. Main-Axis Alignment

    // 12. Distribute any remaining free space.
    debug_log!("distribute_remaining_free_space");
    distribute_remaining_free_space(&mut flex_lines, &constants);

    // 9.6. Cross-Axis Alignment

    // 13. Resolve cross-axis auto margins (also includes 14).
    debug_log!("resolve_cross_axis_auto_margins");
    resolve_cross_axis_auto_margins(&mut flex_lines, &constants);

    // 15. Determine the flex container’s used cross size.
    debug_log!("determine_container_cross_size");
    let total_line_cross_size = determine_container_cross_size(&flex_lines, known_dimensions, &mut constants);

    // EXACT PATCH 61: a column learns its intrinsic width during cross sizing.
    // Regenerate items against that width's ratio height, retaining the input
    // definiteness and allowing content to exceed the percentage basis.
    // The known width bounds the repeat even when the height remains automatic.
    if constants.is_column && known_dimensions.width.is_none() && known_dimensions.height.is_none()
        && tree.get_flexbox_container_style(node).aspect_ratio().is_some()
    {
        let known_dimensions = Size { width: Some(constants.container_size.width), ..known_dimensions };
        drop(flex_lines);
        drop(flex_items);
        return compute_preliminary(tree, node, LayoutInput { known_dimensions, ..inputs });
    }

    // We have the container size.
    // If our caller does not care about performing layout we are done now.
    if run_mode == RunMode::ComputeSize {
        return LayoutOutput::from_outer_size(constants.container_size);
    }

    // 16. Align all flex lines per align-content.
    debug_log!("align_flex_lines_per_align_content");
    align_flex_lines_per_align_content(&mut flex_lines, &constants, total_line_cross_size);

    // Do a final layout pass and gather the resulting layouts
    debug_log!("final_layout_pass");
    let inflow_overflow_rect = final_layout_pass(tree, &mut flex_lines, &constants);

    // Before returning we perform absolute layout on all absolutely positioned children
    debug_log!("perform_absolute_layout_on_absolute_children");
    let absolute_overflow_rect = perform_absolute_layout_on_absolute_children(tree, node, &constants);

    debug_log!("hidden_layout");
    let len = tree.child_count(node);
    for order in 0..len {
        let child = tree.get_child_id(node, order);
        if tree.get_flexbox_child_style(child).box_generation_mode() == BoxGenerationMode::None {
            tree.set_unrounded_layout(child, &Layout::with_order(order as u32));
            tree.perform_child_layout(
                child,
                Size::NONE,
                Size::NONE,
                Size::MAX_CONTENT,
                SizingMode::ContentSize,
                Line::FALSE,
            );
        }
    }

    // 8.5. Flex Container Baselines: calculate the flex container's first baseline
    // See https://www.w3.org/TR/css-flexbox-1/#flex-baselines
    // The baselines are generated from the startmost flex line, where "startmost" refers to the
    // line's visual position: for wrap-reverse containers the cross axis is flipped, so the
    // startmost line is the last line in flex-line order rather than the first.
    let first_line = if constants.is_wrap_reverse { flex_lines.last() } else { flex_lines.first() };
    let first_vertical_baseline = first_line.and_then(|line| {
        if constants.is_column {
            // For column containers the baseline is generated from the startmost item in the line,
            // which for reverse-direction containers is the last item in flex order.
            let item = if constants.dir.is_reverse() { line.items.last() } else { line.items.first() };
            item.map(|child| child.baseline)
        } else {
            line.items
                .iter()
                .find(|item| item.participates_in_baseline_alignment(constants.dir))
                .or_else(|| line.items.iter().next())
                .map(|child| child.baseline)
        }
    });

    LayoutOutput::from_sizes_and_baselines(
        constants.container_size,
        inflow_overflow_rect.union(absolute_overflow_rect),
        Baselines::from_first(first_vertical_baseline),
    )
}

/// Compute constants that can be reused during the flexbox algorithm.
#[inline]
fn compute_constants(
    tree: &impl LayoutFlexboxContainer,
    style: impl FlexboxContainerStyle,
    known_dimensions: Size<Option<f32>>,
    known_dimensions_are_definite: Size<bool>,
    parent_size: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    sizing_mode: SizingMode,
) -> AlgoConstants {
    let dir = style.flex_direction();
    let is_row = dir.is_row();
    let is_column = dir.is_column();
    let flex_wrap = style.flex_wrap();
    let is_wrap = flex_wrap.is_multi_line();
    let is_wrap_reverse = flex_wrap.is_reverse();
    #[cfg(feature = "flexbox_balance")]
    let is_balance = flex_wrap.is_balance();
    #[cfg(feature = "flexbox_balance")]
    let line_count = if is_wrap { Some(style.flex_line_count().max(1)) } else { None };

    let padding = style.padding().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let border = style.border().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let padding_border_sum = padding.sum_axes() + border.sum_axes();
    let box_sizing_adjustment =
        if style.box_sizing() == BoxSizing::ContentBox { padding_border_sum } else { Size::ZERO };

    let align_items = style.align_items().unwrap_or(AlignItems::STRETCH);
    let align_content = style.align_content().unwrap_or(AlignContent::STRETCH);
    let justify_content = style.justify_content();
    let layout_direction = style.direction();

    // Scrollbar gutters are reserved when the `overflow` property is set to `Overflow::Scroll`.
    // However, the axis are switched (transposed) because a node that scrolls vertically needs
    // *horizontal* space to be reserved for a scrollbar
    let scrollbar_gutter = style.overflow().transpose().map(|overflow| match overflow {
        Overflow::Scroll => style.scrollbar_width(),
        _ => 0.0,
    });
    #[cfg(feature = "content_size")]
    let is_scroll_container = {
        let overflow = style.overflow();
        overflow.x.is_scroll_container() || overflow.y.is_scroll_container()
    };
    let mut content_box_inset = padding + border;
    content_box_inset.bottom += scrollbar_gutter.y;

    match layout_direction {
        Direction::Ltr => content_box_inset.right += scrollbar_gutter.x,
        Direction::Rtl => content_box_inset.left += scrollbar_gutter.x,
    };

    // EXACT PATCH 56: transfer the incoming dimensions during measurement.
    // ContentSize omits preferred sizes; final dimensions must not regain a
    // ratio floor derived from an authored size that flexing has replaced.
    let ratio_height = if known_dimensions.height.is_none() {
        crate::compute::ratio::percentage_height(&style, padding_border_sum, known_dimensions.width)
    } else {
        None
    };
    let (known_dimensions, min_size, max_size) = container_sizes_through_ratio(
        &style,
        padding_border_sum,
        known_dimensions,
        style.min_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment),
        style.max_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment),
        sizing_mode,
    );
    let node_outer_size = known_dimensions;
    let node_inner_size = node_outer_size.map_height(|height| height.or(ratio_height))
        .maybe_sub(content_box_inset.sum_axes());
    let known_main_size_is_definite = known_dimensions_are_definite.main(dir);
    let has_definite_main_size = known_main_size_is_definite && known_dimensions.main(dir).is_some();
    // EXACT PATCH 56: a ratio height from a definite width is definite for
    // item sizing even when content can exceed its floor (CSS Sizing 4 §4.2).
    let has_definite_cross_size = (known_dimensions_are_definite.cross(dir) && known_dimensions.cross(dir).is_some())
        || (is_row && known_dimensions_are_definite.width && ratio_height.is_some());
    let cross_axis_available_space_is_definite =
        has_definite_cross_size || matches!(available_space.cross(dir), AvailableSpace::Definite(_));
    let gap = style.gap().resolve_or_zero(node_inner_size.or(Size::zero()), |val, basis| tree.calc(val, basis));

    let container_size = Size::zero();
    let inner_container_size = Size::zero();

    AlgoConstants {
        dir,
        layout_direction,
        is_row,
        is_column,
        is_wrap,
        is_wrap_reverse,
        #[cfg(feature = "flexbox_balance")]
        is_balance,
        #[cfg(feature = "flexbox_balance")]
        line_count,
        min_size,
        max_size,
        border,
        gap,
        content_box_inset,
        scrollbar_gutter,
        #[cfg(feature = "content_size")]
        is_scroll_container,
        align_items,
        align_content,
        justify_content,
        node_outer_size,
        node_inner_size,
        known_main_size_is_definite,
        has_definite_main_size,
        has_definite_cross_size,
        cross_axis_available_space_is_definite,
        container_size,
        inner_container_size,
    }
}

// EXACT PATCH (RFC 0491 WS-A allocation axes): a thread-local pool of
// `Vec<FlexItem>` scratch buffers.
//
// `compute_preliminary` builds one `Vec<FlexItem>` per flex container per
// layout pass and drops it on the way out. On Exact's pinned 1,000-node
// fixture that was 600 allocations and 537,600 bytes per full relayout — 74%
// of all bytes allocated by a layout pass, and the single largest allocation
// site in the engine. The buffer's *contents* are fully overwritten on every
// use, so retaining its capacity between passes cannot change a computed
// value; only the allocator traffic changes.
//
// The pool is a stack of buffers rather than a single buffer because flexbox
// recurses: a container measures its children, which may themselves be flex
// containers holding a live borrow of their own buffer. Depth is therefore the
// tree's flex nesting depth, and buffers are handed back on unwind-safe Drop.
#[cfg(feature = "std")]
/// Thread-local pool of reusable `FlexItem` scratch buffers.
mod flex_item_scratch {
    use super::FlexItem;
    use crate::util::sys::Vec;
    use core::cell::RefCell;
    use core::ops::{Deref, DerefMut};

    /// Cap on retained buffers, so a pathologically deep tree cannot pin an
    /// unbounded amount of memory for the lifetime of the thread.
    const MAX_POOLED_BUFFERS: usize = 32;

    std::thread_local! {
        static POOL: RefCell<Vec<Vec<FlexItem>>> = const { RefCell::new(Vec::new()) };
    }

    /// A checked-out scratch buffer that returns itself to the pool on drop.
    pub(super) struct Scratch(Option<Vec<FlexItem>>);

    impl Scratch {
        /// Check a buffer out of the pool, allocating only if the pool is empty.
        pub(super) fn take() -> Self {
            let buffer = POOL.with(|pool| pool.borrow_mut().pop()).unwrap_or_default();
            Self(Some(buffer))
        }
    }

    impl Deref for Scratch {
        type Target = Vec<FlexItem>;
        #[inline]
        fn deref(&self) -> &Self::Target {
            self.0.as_ref().expect("flex item scratch buffer is live until drop")
        }
    }

    impl DerefMut for Scratch {
        #[inline]
        fn deref_mut(&mut self) -> &mut Self::Target {
            self.0.as_mut().expect("flex item scratch buffer is live until drop")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Some(mut buffer) = self.0.take() {
                buffer.clear();
                POOL.with(|pool| {
                    let mut pool = pool.borrow_mut();
                    if pool.len() < MAX_POOLED_BUFFERS {
                        pool.push(buffer);
                    }
                });
            }
        }
    }
}

// Without `std` there is no thread-local storage, so the scratch buffer
// degrades to the incumbent allocate-per-container behavior.
#[cfg(not(feature = "std"))]
/// `no_std` shim: allocates a fresh buffer per container, as upstream does.
mod flex_item_scratch {
    use super::FlexItem;
    use crate::util::sys::Vec;
    use core::ops::{Deref, DerefMut};

    pub(super) struct Scratch(Vec<FlexItem>);

    impl Scratch {
        /// Allocate a fresh buffer; there is no pool without thread-local storage.
        pub(super) fn take() -> Self {
            Self(Vec::new())
        }
    }

    impl Deref for Scratch {
        type Target = Vec<FlexItem>;
        #[inline]
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl DerefMut for Scratch {
        #[inline]
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
}

/// Generate anonymous flex items.
///
/// # [9.1. Initial Setup](https://www.w3.org/TR/css-flexbox-1/#box-manip)
///
/// - [**Generate anonymous flex items**](https://www.w3.org/TR/css-flexbox-1/#algo-anon-box) as described in [§4 Flex Items](https://www.w3.org/TR/css-flexbox-1/#flex-items).
#[inline]
fn generate_anonymous_flex_items(
    tree: &impl LayoutFlexboxContainer,
    node: NodeId,
    constants: &AlgoConstants,
    out: &mut Vec<FlexItem>,
) {
    // Percentage sizes of items resolve against the container's inner size, but only if that size
    // is definite. A known main size which is derived from the container's own content is treated
    // as indefinite here.
    let percent_resolution_size = if constants.known_main_size_is_definite {
        constants.node_inner_size
    } else {
        constants.node_inner_size.with_main(constants.dir, None)
    };

    out.clear();
    out.extend(
    tree.child_ids(node)
        .enumerate()
        .map(|(index, child)| (index, child, tree.get_flexbox_child_style(child)))
        .filter(|(_, _, style)| style.position() != Position::Absolute)
        .filter(|(_, _, style)| style.box_generation_mode() != BoxGenerationMode::None)
        .map(|(index, child, child_style)| {
            let aspect_ratio = child_style.aspect_ratio();
            let padding = child_style
                .padding()
                .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis));
            let border = child_style
                .border()
                .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis));
            let pb_sum = (padding + border).sum_axes();
            let box_sizing_adjustment =
                if child_style.box_sizing() == BoxSizing::ContentBox { pb_sum } else { Size::ZERO };
            // EXACT PATCH 12: the size through the ratio (`compute::ratio`); the
            // algorithm transfers min/max itself, so they stay as authored.
            let min_size = child_style
                .min_size()
                .maybe_resolve(percent_resolution_size, |val, basis| tree.calc(val, basis))
                .maybe_add(box_sizing_adjustment);
            let max_size = child_style
                .max_size()
                .maybe_resolve(percent_resolution_size, |val, basis| tree.calc(val, basis))
                .maybe_add(box_sizing_adjustment);
            let given = child_style
                .size()
                .maybe_resolve(percent_resolution_size, |val, basis| tree.calc(val, basis))
                .maybe_add(box_sizing_adjustment);
            let (mut size, _, _) = sizes_through_ratio(&child_style, pb_sum, given, min_size, max_size);
            // EXACT PATCH 37 + 54: a replaced item's cross size, or a column
            // item's width, follows its flexed main size (§9.4). The preferred
            // main size must not feed back into its automatic minimum (§4.5).
            if child_style.is_compressible_replaced() || constants.is_column {
                size.set_cross(constants.dir, given.cross(constants.dir));
            }
            FlexItem {
                node: child,
                order: index as u32,
                size,
                size_style: child_style.size(),
                min_size,
                max_size,
                aspect_ratio,

                // EXACT PATCH 20: a `Static` box's insets do nothing.
                inset: if child_style.position() == Position::Static {
                    Rect { left: None, right: None, top: None, bottom: None }
                } else {
                    child_style.inset().zip_size(constants.node_inner_size, |p, s| {
                        p.maybe_resolve(s, |val, basis| tree.calc(val, basis))
                    })
                },
                margin: child_style
                    .margin()
                    .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis)),
                margin_is_auto: child_style.margin().map(LengthPercentageAuto::is_auto),
                padding: child_style
                    .padding()
                    .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis)),
                border: child_style
                    .border()
                    .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis)),
                align_self: child_style.align_self().unwrap_or(constants.align_items).resolve_self_relative(
                    child_style.direction(),
                    constants.layout_direction,
                    constants.is_column,
                ),
                overflow: child_style.overflow(),
                contain: child_style.contain(),
                scrollbar_width: child_style.scrollbar_width(),
                flex_grow: child_style.flex_grow(),
                flex_shrink: child_style.flex_shrink(),
                flex_basis_is_definite: false,
                used_flex_basis_is_definite: false,
                flex_basis: 0.0,
                inner_flex_basis: 0.0,
                violation: 0.0,
                frozen: false,

                resolved_minimum_main_size: 0.0,
                hypothetical_inner_size: Size::zero(),
                hypothetical_outer_size: Size::zero(),
                target_size: Size::zero(),
                outer_target_size: Size::zero(),
                cross_size_was_stretched: false,

                baseline: 0.0,

                offset_main: 0.0,
                offset_cross: 0.0,
            }
        })
    );
}

/// Determine the available main and cross space for the flex items.
///
/// # [9.2. Line Length Determination](https://www.w3.org/TR/css-flexbox-1/#line-sizing)
///
/// - [**Determine the available main and cross space for the flex items**](https://www.w3.org/TR/css-flexbox-1/#algo-available).
///
/// For each dimension, if that dimension of the flex container’s content box is a definite size, use that;
/// if that dimension of the flex container is being sized under a min or max-content constraint, the available space in that dimension is that constraint;
/// otherwise, subtract border and padding from the border-box space supplied by the parent.
/// EXACT PATCH 25: the parent has already subtracted this container's margins.
/// **This might result in an infinite value**.
#[inline]
#[must_use]
fn determine_available_space(
    known_dimensions: Size<Option<f32>>,
    outer_available_space: Size<AvailableSpace>,
    constants: &AlgoConstants,
) -> Size<AvailableSpace> {
    // Note: min/max/preferred size styles have already been applied to known_dimensions in the `compute` function above
    let width = match known_dimensions.width {
        Some(node_width) => AvailableSpace::Definite(node_width - constants.content_box_inset.horizontal_axis_sum()),
        None => outer_available_space
            .width
            .maybe_sub(constants.content_box_inset.horizontal_axis_sum()),
    };

    let height = match known_dimensions.height {
        Some(node_height) => AvailableSpace::Definite(node_height - constants.content_box_inset.vertical_axis_sum()),
        None => outer_available_space
            .height
            .maybe_sub(constants.content_box_inset.vertical_axis_sum()),
    };

    let height = if constants.is_row && constants.has_definite_cross_size {
        constants.node_inner_size.height.map(AvailableSpace::Definite).unwrap_or(height)
    } else {
        height
    };

    Size { width, height }
}

/// Align all flex lines per `align-content`.
///
/// # [9.6. Cross-Axis Alignment](https://www.w3.org/TR/css-flexbox-1/#cross-alignment)
///
/// - [**Align all flex lines**](https://www.w3.org/TR/css-flexbox-1/#algo-line-align) per `align-content`.
#[inline]
fn align_flex_lines_per_align_content(flex_lines: &mut [FlexLine], constants: &AlgoConstants, total_cross_size: f32) {
    let num_lines = flex_lines.len();
    let gap = constants.gap.cross(constants.dir);
    let total_cross_axis_gap = sum_axis_gaps(gap, num_lines);
    let free_space = constants.inner_container_size.cross(constants.dir) - total_cross_size - total_cross_axis_gap;

    let align_content_mode = apply_alignment_fallback(free_space, num_lines, constants.align_content);

    let align_line = |(i, line): (usize, &mut FlexLine)| {
        line.offset_cross =
            compute_alignment_offset(free_space, num_lines, gap, align_content_mode, constants.is_wrap_reverse, i == 0);
    };

    if constants.is_wrap_reverse {
        flex_lines.iter_mut().rev().enumerate().for_each(align_line);
    } else {
        flex_lines.iter_mut().enumerate().for_each(align_line);
    }
}

/// Calculates the layout for a flex-item
#[allow(clippy::too_many_arguments)]
fn calculate_flex_item(
    tree: &mut impl LayoutFlexboxContainer,
    item: &mut FlexItem,
    total_offset_main: &mut f32,
    total_offset_cross: f32,
    line_offset_cross: f32,
    #[cfg(feature = "content_size")] total_overflow_rect: &mut Rect<f32>,
    #[cfg(feature = "content_size")] margin_box_rect: &mut Rect<f32>,
    #[cfg(feature = "content_size")] border: Rect<f32>,
    constants: &AlgoConstants,
) {
    let container_size = constants.container_size;
    let node_inner_size = constants.node_inner_size;
    let direction = constants.dir;
    let layout_direction = constants.layout_direction;
    let mut item_known_dimension_definiteness = item_known_dimension_definiteness(constants, item);
    // EXACT PATCH 49: a changed stretch size is definite for percentages
    // (Flexbox §9.4). An unchanged content-grown ratio height keeps patch 12's
    // preferred percentage basis.
    if item.cross_size_was_stretched {
        item_known_dimension_definiteness.set_cross(direction, true);
    }
    let layout_output = tree.compute_child_layout(
        item.node,
        LayoutInput {
            run_mode: RunMode::PerformLayout,
            sizing_mode: SizingMode::ContentSize,
            axis: RequestedAxis::Both,
            known_dimensions: item.target_size.map(|s| s.into()),
            known_dimensions_are_definite: item_known_dimension_definiteness,
            parent_size: node_inner_size,
            available_space: container_size.map(|s| s.into()),
            vertical_margins_are_collapsible: Line::FALSE,
        },
    );
    let LayoutOutput {
        size,
        #[cfg(feature = "content_size")]
        scrollable_overflow_rect,
        ..
    } = layout_output;

    let is_rtl_row = direction.is_row() && layout_direction.is_rtl();
    let is_rtl_column = direction.is_column() && layout_direction.is_rtl();
    let main_relative_inset = if is_rtl_row {
        item.inset.main_end(direction).or(item.inset.main_start(direction).map(|pos| -pos)).unwrap_or(0.0)
    } else {
        item.inset.main_start(direction).or(item.inset.main_end(direction).map(|pos| -pos)).unwrap_or(0.0)
    };
    let cross_relative_inset = if is_rtl_column {
        item.inset.cross_end(direction).map(|pos| -pos).or(item.inset.cross_start(direction)).unwrap_or(0.0)
    } else {
        item.inset.cross_start(direction).or(item.inset.cross_end(direction).map(|pos| -pos)).unwrap_or(0.0)
    };
    let effective_line_offset_cross = if is_rtl_column { 0.0 } else { line_offset_cross };

    let offset_main = if is_rtl_row {
        *total_offset_main - item.offset_main - item.margin.main_end(direction) - main_relative_inset - size.width
    } else {
        *total_offset_main + item.offset_main + item.margin.main_start(direction) + main_relative_inset
    };

    let offset_cross = total_offset_cross
        + item.offset_cross
        + effective_line_offset_cross
        + item.margin.cross_start(direction)
        + cross_relative_inset;

    if direction.is_row() {
        let baseline_offset_cross =
            total_offset_cross + item.offset_cross + effective_line_offset_cross + item.margin.cross_start(direction);
        // Scroll containers' baselines are determined from their content as if scrolled to the initial
        // position, but are additionally clamped to their border box.
        // See https://github.com/w3c/csswg-drafts/issues/7660
        let inner_baseline = {
            let baseline = layout_output.baselines.first.unwrap_or(size.height);
            if item.overflow.y.is_scroll_container() {
                baseline.min(size.height).max(0.0)
            } else {
                baseline
            }
        };
        item.baseline = baseline_offset_cross + inner_baseline;
    } else {
        let baseline_offset_main = *total_offset_main + item.offset_main + item.margin.main_start(direction);
        let inner_baseline = layout_output.baselines.first.unwrap_or(size.height);
        item.baseline = baseline_offset_main + inner_baseline;
    }

    let location = if direction.is_row() {
        Point { x: offset_main, y: offset_cross }
    } else {
        Point { x: offset_cross, y: offset_main }
    };
    let scrollbar_size = Size {
        width: if item.overflow.y == Overflow::Scroll { item.scrollbar_width } else { 0.0 },
        height: if item.overflow.x == Overflow::Scroll { item.scrollbar_width } else { 0.0 },
    };

    tree.set_unrounded_layout(
        item.node,
        &Layout {
            order: item.order,
            size,
            #[cfg(feature = "content_size")]
            scrollable_overflow_rect,
            scrollbar_size,
            location,
            padding: item.padding,
            border: item.border,
            margin: item.margin,
        },
    );

    if is_rtl_row {
        *total_offset_main -= item.offset_main + item.margin.main_axis_sum(direction) + size.main(direction);
    } else {
        *total_offset_main += item.offset_main + item.margin.main_axis_sum(direction) + size.main(direction);
    }

    #[cfg(feature = "content_size")]
    {
        let contribution_location = if layout_direction.is_rtl() {
            Point { x: container_size.width - (location.x + size.width) - border.right, y: location.y - border.top }
        } else {
            Point { x: location.x - border.left, y: location.y - border.top }
        };
        *total_overflow_rect = total_overflow_rect.union(compute_scrollable_overflow_contribution(
            contribution_location,
            size,
            scrollable_overflow_rect,
            item.overflow,
            item.contain,
            constants.is_scroll_container,
        ));
        // EXACT PATCH 67: the margin box where the item sits before its relative offset.
        let main_shift = if is_rtl_row { -main_relative_inset } else { main_relative_inset };
        let shift = if direction.is_row() {
            Point { x: main_shift, y: cross_relative_inset }
        } else {
            Point { x: cross_relative_inset, y: main_shift }
        };
        let static_location = if layout_direction.is_rtl() {
            Point { x: contribution_location.x + shift.x, y: contribution_location.y - shift.y }
        } else {
            Point { x: contribution_location.x - shift.x, y: contribution_location.y - shift.y }
        };
        *margin_box_rect = margin_box_rect.union(margin_box_contribution(
            static_location,
            size,
            item.margin,
            layout_direction.is_rtl(),
        ));
    }
}

/// Calculates the layout line
#[allow(clippy::too_many_arguments)]
fn calculate_layout_line(
    tree: &mut impl LayoutFlexboxContainer,
    line: &mut FlexLine,
    total_offset_cross: &mut f32,
    #[cfg(feature = "content_size")] overflow_rect: &mut Rect<f32>,
    #[cfg(feature = "content_size")] margin_box_rect: &mut Rect<f32>,
    #[cfg(feature = "content_size")] border: Rect<f32>,
    constants: &AlgoConstants,
) {
    let container_size = constants.container_size;
    let padding_border = constants.content_box_inset;
    let direction = constants.dir;
    let layout_direction = constants.layout_direction;
    let mut total_offset_main = if layout_direction.is_rtl() && direction.is_row() {
        container_size.width - padding_border.main_end(direction)
    } else {
        padding_border.main_start(direction)
    };
    let line_offset_cross = line.offset_cross;

    let is_rtl_column = layout_direction.is_rtl() && direction.is_column();
    if is_rtl_column {
        *total_offset_cross -= line_offset_cross + line.cross_size;
    }

    if direction.is_reverse() {
        for item in line.items.iter_mut().rev() {
            calculate_flex_item(
                tree,
                item,
                &mut total_offset_main,
                *total_offset_cross,
                line_offset_cross,
                #[cfg(feature = "content_size")]
                overflow_rect,
                #[cfg(feature = "content_size")]
                margin_box_rect,
                #[cfg(feature = "content_size")]
                border,
                constants,
            );
        }
    } else {
        for item in line.items.iter_mut() {
            calculate_flex_item(
                tree,
                item,
                &mut total_offset_main,
                *total_offset_cross,
                line_offset_cross,
                #[cfg(feature = "content_size")]
                overflow_rect,
                #[cfg(feature = "content_size")]
                margin_box_rect,
                #[cfg(feature = "content_size")]
                border,
                constants,
            );
        }
    }

    if !is_rtl_column {
        *total_offset_cross += line_offset_cross + line.cross_size;
    }
}

/// Do a final layout pass and collect the resulting layouts.
#[inline]
fn final_layout_pass(
    tree: &mut impl LayoutFlexboxContainer,
    flex_lines: &mut [FlexLine],
    constants: &AlgoConstants,
) -> Rect<f32> {
    let mut total_offset_cross = if constants.is_column && constants.layout_direction.is_rtl() {
        constants.container_size.width - constants.content_box_inset.cross_end(constants.dir)
    } else {
        constants.content_box_inset.cross_start(constants.dir)
    };

    #[cfg_attr(not(feature = "content_size"), allow(unused_mut))]
    let mut overflow_rect = Rect::ZERO;
    #[cfg(feature = "content_size")]
    let mut margin_box_rect = Rect::ZERO;

    if constants.is_wrap_reverse {
        for line in flex_lines.iter_mut().rev() {
            calculate_layout_line(
                tree,
                line,
                &mut total_offset_cross,
                #[cfg(feature = "content_size")]
                &mut overflow_rect,
                #[cfg(feature = "content_size")]
                &mut margin_box_rect,
                #[cfg(feature = "content_size")]
                constants.border,
                constants,
            );
        }
    } else {
        for line in flex_lines.iter_mut() {
            calculate_layout_line(
                tree,
                line,
                &mut total_offset_cross,
                #[cfg(feature = "content_size")]
                &mut overflow_rect,
                #[cfg(feature = "content_size")]
                &mut margin_box_rect,
                #[cfg(feature = "content_size")]
                constants.border,
                constants,
            );
        }
    }

    // A scroll container's own padding at the end of its items is part of its scrollable
    // overflow region. Boxes that are not scroll containers do not extend their overflow region
    // by their own padding.
    #[cfg(feature = "content_size")]
    if constants.is_scroll_container {
        let end_padding = Size {
            width: if constants.layout_direction.is_rtl() {
                constants.content_box_inset.left - constants.border.left - constants.scrollbar_gutter.x
            } else {
                constants.content_box_inset.right - constants.border.right - constants.scrollbar_gutter.x
            },
            height: constants.content_box_inset.bottom - constants.border.bottom - constants.scrollbar_gutter.y,
        };
        overflow_rect = with_end_padding(overflow_rect, margin_box_rect, end_padding);
    }

    overflow_rect
}

/// Perform absolute layout on the absolutely positioned boxes this container is the
/// containing block of.
///
/// EXACT PATCH 18, 20: sized and placed by the shared solver (`common::absolute`). A box
/// with no inset on an axis sits where it would as the container's only item (CSS Flexbox
/// §4.1): aligned in the content box by `justify-content` and its `align-self`. A `Static`
/// container keeps that static position for the box that contains its absolute children.
#[inline]
fn perform_absolute_layout_on_absolute_children(
    tree: &mut impl LayoutFlexboxContainer,
    node: NodeId,
    constants: &AlgoConstants,
) -> Rect<f32> {
    let contains = tree.get_core_container_style(node).position() != Position::Static;
    let pass = AbsolutePass {
        area_size: constants.container_size - constants.border.sum_axes() - constants.scrollbar_gutter.into(),
        area_offset: Point {
            x: constants.border.left + if constants.layout_direction.is_rtl() { constants.scrollbar_gutter.x } else { 0.0 },
            y: constants.border.top,
        },
        direction: constants.layout_direction,
        sizing_mode: SizingMode::ContentSize,
        #[cfg(feature = "content_size")]
        is_scroll_container: constants.is_scroll_container,
    };
    let content_box = Rect {
        left: constants.content_box_inset.left,
        right: constants.container_size.width - constants.content_box_inset.right,
        top: constants.content_box_inset.top,
        bottom: constants.container_size.height - constants.content_box_inset.bottom,
    };
    let main_is_rtl = constants.is_row && constants.layout_direction.is_rtl();
    let cross_is_rtl = !constants.is_row && constants.layout_direction.is_rtl();
    let main_axis_flex_start_reversed = constants.dir.is_reverse() ^ main_is_rtl;
    let cross_axis_flex_start_reversed = constants.is_wrap_reverse ^ cross_is_rtl;
    let edge = |at_start: bool| if at_start { StaticAlignment::Start } else { StaticAlignment::End };

    // Stretch is an invalid value for justify_content in the flexbox algorithm, so we
    // treat it as if it wasn't set (and thus we default to FlexStart behaviour).
    //
    // The `safe` overflow-position keyword is intentionally NOT applied here, even when
    // the abs-positioned item would overflow the main axis: Chrome does not apply safe
    // fallback to `justify-content` on absolutely-positioned flex items (only the
    // cross-axis `align-self` does so).
    // `start`/`end` are writing-mode relative (they flip for RTL but not for
    // reversed flex-directions), whereas `flex-start`/`flex-end` and the
    // distributed keywords' fallbacks are flex-relative.
    let main = match constants.justify_content.unwrap_or(JustifyContent::FLEX_START).keyword() {
        AlignContentKeyword::Start => edge(!main_is_rtl),
        AlignContentKeyword::End => edge(main_is_rtl),
        AlignContentKeyword::SpaceBetween | AlignContentKeyword::Stretch | AlignContentKeyword::FlexStart => {
            edge(!main_axis_flex_start_reversed)
        }
        AlignContentKeyword::FlexEnd => edge(main_axis_flex_start_reversed),
        AlignContentKeyword::SpaceEvenly | AlignContentKeyword::SpaceAround | AlignContentKeyword::Center => {
            StaticAlignment::Center
        }
    };
    // `start`/`end` (and `baseline`, whose static-position fallback is `start`) are
    // writing-mode relative: they flip for RTL but not for `wrap-reverse`.
    // `flex-start`/`flex-end` and the `stretch` fallback are flex-relative. Stretch alignment
    // does not apply to absolutely positioned items: see "Example 3" at
    // https://www.w3.org/TR/css-flexbox-1/#abspos-items
    let cross_of = |keyword: AlignItemsKeyword| match keyword {
        AlignItemsKeyword::Start | AlignItemsKeyword::Baseline => edge(!cross_is_rtl),
        AlignItemsKeyword::End => edge(cross_is_rtl),
        AlignItemsKeyword::Stretch | AlignItemsKeyword::FlexStart => edge(!cross_axis_flex_start_reversed),
        AlignItemsKeyword::FlexEnd => edge(cross_axis_flex_start_reversed),
        AlignItemsKeyword::Center => StaticAlignment::Center,
        // SelfStart/SelfEnd are resolved to Start/End against the item's own direction below.
        AlignItemsKeyword::SelfStart | AlignItemsKeyword::SelfEnd
        | AlignItemsKeyword::Left | AlignItemsKeyword::Right => unreachable!(),
    };

    let mut overflow_rect = Rect::ZERO;
    for order in 0..tree.child_count(node) {
        let child = tree.get_child_id(node, order);
        let child_style = tree.get_flexbox_child_style(child);

        // Skip items that are display:none or are not position:absolute
        if child_style.box_generation_mode() == BoxGenerationMode::None || child_style.position() != Position::Absolute
        {
            continue;
        }
        let align_self = child_style.align_self().unwrap_or(constants.align_items).resolve_self_relative(
            child_style.direction(),
            constants.layout_direction,
            constants.is_column,
        );
        drop(child_style);
        let cross = cross_of(align_self.keyword);
        let cross_safe =
            matches!(align_self.safety, AlignmentSafety::Safe).then(|| cross_of(AlignItemsKeyword::Start));
        let position = StaticPosition {
            rect: content_box,
            align: if constants.is_row { Point { x: main, y: cross } } else { Point { x: cross, y: main } },
            safe: if constants.is_row { Point { x: None, y: cross_safe } } else { Point { x: cross_safe, y: None } },
            order: order as u32,
        };
        if contains {
            overflow_rect = overflow_rect.union(pass.place(tree, child, position, None, Point::ZERO));
        } else {
            tree.set_static_position(child, position);
        }
    }
    overflow_rect = overflow_rect.union(pass.place_hoisted(tree, node));
    overflow_rect
}

/// Computes the total space taken up by gaps in an axis given:
///   - The size of each gap
///   - The number of items (children or flex-lines) between which there are gaps
#[inline(always)]
fn sum_axis_gaps(gap: f32, num_items: usize) -> f32 {
    // Gaps only exist between items, so...
    if num_items <= 1 {
        // ...if there are less than 2 items then there are no gaps
        0.0
    } else {
        // ...otherwise there are (num_items - 1) gaps
        gap * (num_items - 1) as f32
    }
}

#[cfg(feature = "flexbox_balance")]
mod balance;

#[cfg(all(test, feature = "taffy_tree"))]
#[path = "flexbox_cross_margins_tests.rs"]
mod cross_margins_tests;

#[cfg(all(test, feature = "taffy_tree", feature = "grid"))]
mod intrinsic_tests;
