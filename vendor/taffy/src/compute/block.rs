//! Computes the CSS block layout algorithm in the case that the block container being laid out contains only block-level boxes
mod inflow;

use inflow::perform_final_layout_on_in_flow_children;
use crate::geometry::{Line, Point, Rect, Size};
use crate::style::{AlignItemsKeyword, AlignmentSafety, AvailableSpace, CoreStyle, JustifyItems, JustifySelf, LengthPercentageAuto, Overflow, Position};
use crate::style_helpers::TaffyMaxContent;
use crate::tree::{
    Baselines, CollapsibleMarginSet, Layout, LayoutInput, LayoutOutput, RunMode, SizingMode, StaticAlignment, StaticPosition,
};
use crate::tree::{LayoutPartialTree, LayoutPartialTreeExt, NodeId};
use crate::util::debug::debug_log;
use crate::compute::common::absolute::AbsolutePass;
use crate::compute::ratio::{container_sizes_through_ratio, sizes_through_ratio};
use crate::util::sys::f32_max;
use crate::util::sys::Vec;
use crate::util::MaybeMath;
use crate::util::{MaybeResolve, ResolveOrZero};
use crate::{
    BlockContainerStyle, BlockItemStyle, BoxGenerationMode, BoxSizing, Contain, Dimension, Direction,
    LayoutBlockContainer, RequestedAxis,
};

#[cfg(feature = "float_layout")]
use super::float::{BfcSlot, ContentSlot, FloatContext, FloatIntrinsicWidthCalculator};
#[cfg(feature = "float_layout")]
use crate::{Clear, Float, FloatDirection};

/// Context for positioning Block and Float boxes within a Block Formatting Context
pub struct BlockFormattingContext {
    /// The float positioning context that handles positioning floats within this Block Formatting Context
    #[cfg(feature = "float_layout")]
    float_context: FloatContext,
}

impl Default for BlockFormattingContext {
    fn default() -> Self {
        Self {
            #[cfg(feature = "float_layout")]
            float_context: FloatContext::new(),
        }
    }
}

impl BlockFormattingContext {
    /// Create a new `BlockFormattingContext` with the specified width constraint
    pub fn new() -> Self {
        Default::default()
    }

    /// Create an initial `BlockContext` for this `BlockFormattingContext`
    pub fn root_block_context(&mut self) -> BlockContext<'_> {
        BlockContext {
            bfc: self,
            y_offset: 0.0,
            insets: [0.0, 0.0],
            content_box_insets: [0.0, 0.0],
            float_content_contribution: f32::NEG_INFINITY,
            is_root: true,
            #[cfg(feature = "float_layout")]
            adjoining_floats: [false, false],
            #[cfg(feature = "float_layout")]
            top_adjoining_floats: None,
        }
    }
}

/// Context for each individual Block within a Block Formatting Context
///
/// Contains a mutable reference to the BlockFormattingContext + block-specific data
pub struct BlockContext<'bfc> {
    /// A mutable reference to the root BlockFormatttingContext that this BlockContext belongs to
    bfc: &'bfc mut BlockFormattingContext,
    /// The y-offset of the border-top of the block node, relative to the to the border-top of the
    /// root node of the Block Formatting Context it belongs to.
    y_offset: f32,
    /// The x-inset of the border-box in from each side of the block node, relative to the root node of the Block Formatting Context it belongs to.
    insets: [f32; 2],
    /// The x-insets of the content box
    content_box_insets: [f32; 2],
    /// The height that floats take up in the element (`f32::NEG_INFINITY` if the element's
    /// subtree does not contain any floats)
    float_content_contribution: f32,
    /// Whether the node is the root of the Block Formatting Context is belongs to.
    is_root: bool,
    /// Whether a float has been placed (on each side) whose position adjoins the current
    /// margin-collapse strut of this block (i.e. whose final position can still be moved by
    /// margins that collapse into that strut). Such floats force clearance on cleared elements
    /// whose margins adjoin the same strut.
    #[cfg(feature = "float_layout")]
    adjoining_floats: [bool; 2],
    /// The value of `adjoining_floats` frozen at the first point at which in-flow content was
    /// committed within this block (resolving the position of the block's top margin strut).
    /// `None` if no in-flow content has been committed yet.
    #[cfg(feature = "float_layout")]
    top_adjoining_floats: Option<[bool; 2]>,
}

impl BlockContext<'_> {
    /// Create a sub-`BlockContext` for a child block node
    pub fn sub_context(&mut self, additional_y_offset: f32, insets: [f32; 2]) -> BlockContext<'_> {
        let insets = [self.insets[0] + insets[0], self.insets[1] + insets[1]];
        BlockContext {
            bfc: self.bfc,
            y_offset: self.y_offset + additional_y_offset,
            insets,
            content_box_insets: insets,
            float_content_contribution: f32::NEG_INFINITY,
            is_root: false,
            // Floats adjoining the parent's current strut also adjoin this block's top strut
            // (if this block's top margin collapses with its first child's, which is checked separately)
            #[cfg(feature = "float_layout")]
            adjoining_floats: self.adjoining_floats,
            #[cfg(feature = "float_layout")]
            top_adjoining_floats: None,
        }
    }

    /// Returns whether this block is the root block of it's Block Formatting Context
    pub fn is_bfc_root(&self) -> bool {
        self.is_root
    }
}

#[cfg(feature = "float_layout")]
impl BlockContext<'_> {
    /// Set the width of the overall Block Formatting Context. This is used to resolve positions
    /// that are relative to the right of the context such as right-floated boxes.
    ///
    /// Sub-blocks within a Block Formatting Context should use the `Self::sub_context` method to create
    /// a sub-`BlockContext` with `insets` instead.
    pub fn set_width(&mut self, available_width: f32) {
        self.bfc.float_context.set_width(available_width);
    }

    /// Set the x-axis content-box insets of the `BlockContext`. These are the difference between the border-box
    /// and the content-box of the box (padding + border + scrollbar_gutter).
    pub fn apply_content_box_inset(&mut self, content_box_x_insets: [f32; 2]) {
        self.content_box_insets[0] = self.insets[0] + content_box_x_insets[0];
        self.content_box_insets[1] = self.insets[1] + content_box_x_insets[1];
    }

    /// Whether the float context contains any floats
    #[inline(always)]
    pub fn has_floats(&self) -> bool {
        self.bfc.float_context.has_floats()
    }

    /// Whether the float context contains any floats that extend to or below min_y
    #[inline(always)]
    pub fn has_active_floats(&self, min_y: f32) -> bool {
        self.bfc.float_context.has_active_floats(min_y + self.y_offset)
    }

    /// Position a floated box with the context
    pub fn place_floated_box(
        &mut self,
        floated_box: Size<f32>,
        min_y: f32,
        direction: FloatDirection,
        clear: Clear,
        adjoins_unresolved_strut: bool,
    ) -> Point<f32> {
        if adjoins_unresolved_strut {
            self.adjoining_floats[direction as usize] = true;
        }
        let mut pos = self.bfc.float_context.place_floated_box(
            floated_box,
            min_y + self.y_offset,
            self.content_box_insets,
            direction,
            clear,
        );
        pos.y -= self.y_offset;
        pos.x -= self.insets[0];

        self.float_content_contribution = self.float_content_contribution.max(pos.y + floated_box.height);

        pos
    }

    /// Search a space suitable for laying out non-floated content into
    pub fn find_content_slot(&self, min_y: f32, clear: Clear, after: Option<usize>) -> ContentSlot {
        let mut slot =
            self.bfc.float_context.find_content_slot(min_y + self.y_offset, self.content_box_insets, clear, after);
        slot.y -= self.y_offset;
        slot.x -= self.insets[0];
        slot
    }

    /// Search for a space suitable for laying out a box that establishes an independent
    /// formatting context (whose border box must not overlap floats)
    pub fn find_bfc_slot(
        &self,
        min_y: f32,
        margins: [f32; 2],
        direction: Direction,
        clear: Clear,
        after: Option<usize>,
    ) -> BfcSlot {
        let mut slot = self.bfc.float_context.find_bfc_slot(
            min_y + self.y_offset,
            self.content_box_insets,
            margins,
            direction,
            clear,
            after,
        );
        slot.y -= self.y_offset;
        slot.x -= self.insets[0];
        slot
    }

    /// Get the bottom of lowest relevant float for the specific clear property
    pub fn cleared_threshold(&self, clear: Clear) -> Option<f32> {
        self.bfc.float_context.cleared_threshold(clear).map(|threshold| threshold - self.y_offset)
    }

    /// Whether a float that is adjoining the current margin-collapse strut has been placed
    /// on the side(s) relevant to the passed clear property
    pub fn has_adjoining_float(&self, clear: Clear) -> bool {
        match clear {
            Clear::Left => self.adjoining_floats[0],
            Clear::Right => self.adjoining_floats[1],
            Clear::Both => self.adjoining_floats[0] || self.adjoining_floats[1],
            Clear::None => false,
        }
    }

    /// Merge adjoining float flags propagated from a child block into this block's flags
    fn merge_adjoining_floats(&mut self, flags: [bool; 2]) {
        self.adjoining_floats[0] |= flags[0];
        self.adjoining_floats[1] |= flags[1];
    }

    /// Record that in-flow content has been committed within this block, resolving the position of
    /// the current margin-collapse strut. Floats placed before this point no longer adjoin the
    /// current strut. The flags for the block's top strut are frozen at the first commit.
    fn commit_strut(&mut self) {
        if self.top_adjoining_floats.is_none() {
            self.top_adjoining_floats = Some(self.adjoining_floats);
        }
        self.adjoining_floats = [false, false];
    }

    /// The adjoining float flags for this block's top margin strut: floats placed while the
    /// position of the block's top strut was still unresolved
    fn top_adjoining_floats(&self) -> [bool; 2] {
        self.top_adjoining_floats.unwrap_or(self.adjoining_floats)
    }

    /// Update the height that descendent floats with the height that floats consume
    /// within a particular child
    fn add_child_floated_content_height_contribution(&mut self, child_contribution: f32) {
        self.float_content_contribution = self.float_content_contribution.max(child_contribution);
    }

    /// Returns the height that descendent floats consume
    pub fn floated_content_height_contribution(&self) -> f32 {
        self.float_content_contribution
    }
}

#[cfg(not(feature = "float_layout"))]
impl BlockContext<'_> {
    #[inline(always)]
    /// Returns the height that descendent floats consume (always 0.0 when the float feature is disabled)
    fn float_content_contribution(&self) -> f32 {
        0.0
    }
}

use super::common::alignment::{apply_alignment_fallback, compute_alignment_offset};
#[cfg(feature = "content_size")]
use super::common::scrollable_overflow::{
    compute_scrollable_overflow_contribution, margin_box_contribution, with_end_padding,
};
use super::common::sizing_keyword::{
    resolve_sizing_keyword, SizingKeywordResolution,
};

/// Per-child data that is accumulated and modified over the course of the layout algorithm
struct BlockItem {
    /// The identifier for the associated node
    node_id: NodeId,

    /// The "source order" of the item. This is the index of the item within the children iterator,
    /// and controls the order in which the nodes are placed
    order: u32,

    /// The child's alignment with `auto` and self-relative keywords resolved.
    justify_self: Option<JustifySelf>,

    /// Items that are tables don't have stretch sizing applied to them
    is_table: bool,

    /// Items that are replaced elements resolve an auto width to their intrinsic size
    /// rather than being stretch-sized
    /// <https://www.w3.org/TR/CSS22/visudet.html#block-replaced-width>
    is_replaced: bool,

    /// Whether the child is a non-independent block or inline node
    is_in_same_bfc: bool,

    #[cfg(feature = "float_layout")]
    /// The `float` style of the node
    float: Float,
    #[cfg(feature = "float_layout")]
    /// The `clear` style of the node
    clear: Clear,

    /// The size style of this item. Used to detect and resolve sizing keywords
    /// (`min-content`, `max-content`, `fit-content`, `fit-content(...)`, and `stretch`).
    /// In the height (block) axis only `stretch` requires resolution: the intrinsic sizing
    /// keywords are all equal to the content size, which is what an auto height already
    /// resolves to.
    size_style: Size<Dimension>,

    /// The base size of this item
    size: Size<Option<f32>>,
    /// The minimum allowable size of this item
    min_size: Size<Option<f32>>,
    /// The maximum allowable size of this item
    max_size: Size<Option<f32>>,

    /// The overflow style of the item
    overflow: Point<Overflow>,
    /// The contain style of the item
    contain: Contain,
    /// The width of the item's scrollbars (if it has scrollbars)
    scrollbar_width: f32,

    /// The position style of the item
    position: Position,
    /// The final offset of this item
    inset: Rect<LengthPercentageAuto>,
    /// The margin of this item
    margin: Rect<LengthPercentageAuto>,
    /// The margin of this item
    padding: Rect<f32>,
    /// The margin of this item
    border: Rect<f32>,
    /// The sum of padding and border for this item
    padding_border_sum: Size<f32>,

    /// The computed border box size of this item
    computed_size: Size<f32>,
    /// The computed "static position" of this item. The static position is the position
    /// taking into account padding, border, margins, and scrollbar_gutters but not inset
    static_position: Point<f32>,
    /// The relative offset in this item's final layout (EXACT PATCH 67: its margin box counts
    /// toward a scroll container's end padding where it would be without it)
    relative_offset: Point<f32>,
    /// Whether margins can be collapsed through this item
    can_be_collapsed_through: bool,

    /// Pending layout for in-flow non-floated items. Held back from `set_unrounded_layout` so the
    /// post-loop `align-content` pass in `compute_inner` can shift `location.y` before commit.
    final_layout: Option<Layout>,
}

/// Computes the layout of [`LayoutPartialTree`] according to the block layout algorithm
pub fn compute_block_layout(
    tree: &mut impl LayoutBlockContainer,
    node_id: NodeId,
    inputs: LayoutInput,
    block_ctx: Option<&mut BlockContext<'_>>,
) -> LayoutOutput {
    let LayoutInput { known_dimensions, parent_size, run_mode, .. } = inputs;
    let style = tree.get_block_container_style(node_id);

    // Pull these out earlier to avoid borrowing issues
    let overflow = style.overflow();
    let is_scroll_container = overflow.x.is_scroll_container() || overflow.y.is_scroll_container();
    let contain = style.contain();
    // css-align-3 §5.1.1: a non-`normal` `align-content` makes a block container establish an
    // independent formatting context. <https://drafts.csswg.org/css-align-3/#distribution-block>
    // Layout and paint containment also establish an independent formatting context.
    // <https://drafts.csswg.org/css-contain-2/#containment-layout>
    // EXACT PATCH 27: a multi-column container establishes an independent
    // formatting context (CSS Multi-column Layout 1 §2), so it shares no
    // floats with its parent.
    let establishes_new_bfc = is_scroll_container
        || style.align_content().is_some()
        || contain.establishes_independent_formatting_context()
        || style.multicol().is_some();
    let padding = style.padding().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let border = style.border().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let padding_border_size = (padding + border).sum_axes();
    let box_sizing_adjustment =
        if style.box_sizing() == BoxSizing::ContentBox { padding_border_size } else { Size::ZERO };

    // EXACT PATCH 12: the size styles through the ratio (`compute::ratio`).
    let raw_max_size =
        style.max_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment);
    let (style_size, min_size, mut max_size) = sizes_through_ratio(
        &style,
        padding_border_size,
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

    drop(style);

    // If both min and max in a given axis are set and max <= min then this determines the size in that axis
    let min_max_definite_size = min_size.zip_map(max_size, |min, max| match (min, max) {
        (Some(min), Some(max)) if max <= min => Some(min),
        _ => None,
    });

    let styled_based_known_dimensions =
        known_dimensions.or(min_max_definite_size).or(clamped_style_size).maybe_max(padding_border_size);

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

    // Unwrap the block formatting context if one was passed, or else create a new one
    debug_log!("BLOCK");
    let mut output = match block_ctx {
        Some(inherited_bfc) if !establishes_new_bfc => compute_inner(
            tree,
            node_id,
            LayoutInput { known_dimensions: styled_based_known_dimensions, ..inputs },
            inherited_bfc,
        ),
        _ => {
            let mut root_bfc = BlockFormattingContext::new();
            let mut root_ctx = root_bfc.root_block_context();
            compute_inner(
                tree,
                node_id,
                LayoutInput { known_dimensions: styled_based_known_dimensions, ..inputs },
                &mut root_ctx,
            )
        }
    };

    // Layout containment suppresses the box's baseline for baseline-alignment purposes
    if contain.suppresses_baseline() {
        output.baselines = Baselines::NONE;
    }

    output
}

/// Computes the layout of [`LayoutBlockContainer`] according to the block layout algorithm
fn compute_inner(
    tree: &mut impl LayoutBlockContainer,
    node_id: NodeId,
    inputs: LayoutInput,
    #[allow(unused_mut)] mut block_ctx: &mut BlockContext<'_>,
) -> LayoutOutput {
    let LayoutInput {
        known_dimensions, parent_size, available_space, run_mode, vertical_margins_are_collapsible, ..
    } = inputs;

    let style = tree.get_block_container_style(node_id);
    let raw_padding = style.padding();
    let raw_border = style.border();
    let raw_margin = style.margin();
    let padding = raw_padding.resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let border = raw_border.resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let direction = style.direction();

    // Scrollbar gutters are reserved when the `overflow` property is set to `Overflow::Scroll`.
    // However, the axis are switched (transposed) because a node that scrolls vertically needs
    // *horizontal* space to be reserved for a scrollbar
    let scrollbar_gutter = {
        let offsets = style.overflow().transpose().map(|overflow| match overflow {
            Overflow::Scroll => style.scrollbar_width(),
            _ => 0.0,
        });
        match direction {
            Direction::Ltr => Rect { top: 0.0, left: 0.0, right: offsets.x, bottom: offsets.y },
            Direction::Rtl => Rect { top: 0.0, left: offsets.x, right: 0.0, bottom: offsets.y },
        }
    };
    let padding_border = padding + border;
    let padding_border_size = padding_border.sum_axes();
    let content_box_inset = padding_border + scrollbar_gutter;

    // Apply content box inset
    #[cfg(feature = "float_layout")]
    block_ctx.apply_content_box_inset([content_box_inset.left, content_box_inset.right]);

    let box_sizing_adjustment =
        if style.box_sizing() == BoxSizing::ContentBox { padding_border_size } else { Size::ZERO };
    // EXACT PATCH 12: css-sizing-4 — a size given in one axis transfers through
    // `aspect-ratio` to the other (`compute::ratio`); a derived height with
    // `min-height: auto` is a floor the content can pass. The style's own
    // sizes decide collapsing; `known_dimensions` (the parent's resolution,
    // which fills an axis only when it is a real constraint) decides the
    // transfer, and an incoming known size is left as the parent resolved it.
    let raw_min_size =
        style.min_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment);
    let raw_max_size =
        style.max_size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment);
    let (size, _, _) = sizes_through_ratio(
        &style,
        padding_border_size,
        style.size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment),
        raw_min_size,
        raw_max_size,
    );
    let (known_dimensions, min_size, max_size) = container_sizes_through_ratio(
        &style, padding_border_size, known_dimensions, raw_min_size, raw_max_size, inputs.sizing_mode,
    );
    let percentage_basis_dimensions = Size {
        width: known_dimensions.width,
        height: known_dimensions.height.filter(|_| inputs.known_dimensions_are_definite.height)
            .or_else(|| crate::compute::ratio::percentage_height(&style, padding_border_size, known_dimensions.width)),
    };
    let mut container_content_box_size = percentage_basis_dimensions.maybe_sub(content_box_inset.sum_axes());

    let overflow = style.overflow();
    let is_scroll_container = overflow.x.is_scroll_container() || overflow.y.is_scroll_container();
    // EXACT PATCH 27: and its first child's top margin stays inside it.
    let multicol = style.multicol();
    let establishes_new_bfc = is_scroll_container
        || style.align_content().is_some()
        || style.contain().establishes_independent_formatting_context()
        || multicol.is_some();

    // Determine margin collapsing behaviour
    let own_margins_collapse_with_children = Line {
        start: vertical_margins_are_collapsible.start
            && !establishes_new_bfc
            && style.position() != Position::Absolute
            && padding.top == 0.0
            && border.top == 0.0,
        end: vertical_margins_are_collapsible.end
            && !establishes_new_bfc
            && style.position() != Position::Absolute
            && padding.bottom == 0.0
            && border.bottom == 0.0
            && size.height.is_none(),
    };
    let has_styles_preventing_being_collapsed_through = !style.is_block()
        || block_ctx.is_bfc_root()
        || establishes_new_bfc
        || style.position() == Position::Absolute
        || padding.top > 0.0
        || padding.bottom > 0.0
        || border.top > 0.0
        || border.bottom > 0.0
        || matches!(size.height, Some(h) if h > 0.0)
        || matches!(min_size.height, Some(h) if h > 0.0);

    let text_align = style.text_align();
    let justify_items = style.justify_items();
    let align_content = style.align_content();
    drop(style);
    // EXACT PATCH 30: the height bounds the caller cuts the columns by
    if multicol.is_some() && run_mode == RunMode::PerformLayout {
        tree.set_multicol_bounds(node_id, min_size.height, max_size.height);
    }
    // EXACT PATCH 27: a multi-column container's children resolve percentage
    // widths against the column width. A height its columns size (the
    // caller's `used_height`) is no basis for percentage heights, even where
    // a parent hands it back as known (an absolute box's solver, a stretched
    // flex item): it was not definite before the flow thread was laid out.
    let columns_size_height = multicol.is_some_and(|m| m.used_height.is_some());
    if let (Some(m), Some(u)) = (multicol, container_content_box_size.width) {
        container_content_box_size.width = Some(m.columns(u.max(0.0)).1);
    }
    if columns_size_height {
        container_content_box_size.height = None;
    }

    // 1. Generate items
    let mut items = generate_item_list(tree, node_id, container_content_box_size, justify_items, direction);

    // 2. Compute container width
    let container_outer_width = known_dimensions.width.unwrap_or_else(|| {
        let available_width = available_space.width.maybe_sub(content_box_inset.horizontal_axis_sum());
        let intrinsic_width = determine_content_based_container_width(tree, &items, available_width)
            + content_box_inset.horizontal_axis_sum();
        intrinsic_width.maybe_clamp(min_size.width, max_size.width).maybe_max(Some(padding_border_size.width))
    });

    // Short-circuit if computing size and both dimensions known
    if let (RunMode::ComputeSize, Some(container_outer_height)) = (run_mode, known_dimensions.height) {
        return LayoutOutput::from_outer_size(Size { width: container_outer_width, height: container_outer_height });
    }

    // We can also short-circuit if the width is known and only the width has been requested.
    if run_mode == RunMode::ComputeSize && inputs.axis == RequestedAxis::Horizontal {
        return LayoutOutput::from_outer_size(Size { width: container_outer_width, height: 0.0 });
    }

    let container_percentage_resolution_height = match columns_size_height {
        true => None,
        false => percentage_basis_dimensions.height.or(size.height.maybe_max(min_size.height)),
    };

    // 3. Perform final item layout and return content height
    //
    // Percentage padding and borders resolve against the *containing block's* width
    // (`parent_size`), not the box's own width. These only differ when the box has a
    // non-stretch width. Fall back to the box's own width when the parent size is
    // unknown (e.g. at the root of the layout tree).
    let percentage_resolution_width = parent_size.width.unwrap_or(container_outer_width);
    let resolved_padding =
        raw_padding.resolve_or_zero(Some(percentage_resolution_width), |val, basis| tree.calc(val, basis));
    let resolved_border =
        raw_border.resolve_or_zero(Some(percentage_resolution_width), |val, basis| tree.calc(val, basis));
    let resolved_content_box_inset = resolved_padding + resolved_border + scrollbar_gutter;
    // EXACT PATCH 27: a multi-column container lays its children out as one
    // column of the used column width W (CSS Multi-column Layout 1 §3.4 over
    // its content-box width U), the flow thread, which is also their basis
    // for percentage widths. A content-sized U is the content's width as one
    // column (Exact declares that), which the columns then divide.
    let flow_outer_width = match multicol {
        Some(m) => {
            let inset = resolved_content_box_inset.horizontal_axis_sum();
            m.columns((container_outer_width - inset).max(0.0)).1 + inset
        }
        None => container_outer_width,
    };
    #[cfg_attr(not(feature = "content_size"), allow(unused_mut))]
    let (
        mut inflow_overflow_rect,
        mut intrinsic_outer_height,
        first_child_top_margin_set,
        last_child_bottom_margin_set,
        mut first_baseline,
    ) = perform_final_layout_on_in_flow_children(
        tree,
        run_mode,
        &mut items,
        flow_outer_width,
        container_percentage_resolution_height,
        content_box_inset,
        resolved_content_box_inset,
        resolved_border,
        text_align,
        direction,
        own_margins_collapse_with_children,
        #[cfg(feature = "content_size")]
        is_scroll_container,
        block_ctx,
    );

    // Root BFCs contain floats
    #[cfg(feature = "float_layout")]
    if block_ctx.is_bfc_root() || establishes_new_bfc {
        intrinsic_outer_height = intrinsic_outer_height.max(block_ctx.floated_content_height_contribution());
    }

    // EXACT PATCH 27: once the caller has cut the flow thread, an automatic
    // height is the columns' height plus the box's own vertical inset,
    // clamped as any automatic height is.
    let intrinsic_or_columns = match multicol.and_then(|m| m.used_height) {
        Some(columns) => columns + resolved_content_box_inset.vertical_axis_sum(),
        None => intrinsic_outer_height,
    };
    let container_outer_height = known_dimensions
        .height
        .unwrap_or(intrinsic_or_columns.maybe_clamp(min_size.height, max_size.height))
        .maybe_max(Some(padding_border_size.height));
    let final_outer_size = Size { width: container_outer_width, height: container_outer_height };

    // CSS2 §8.3.1: the bottom margin of a block with `height: auto` collapses with its last
    // in-flow child's bottom margin only if the box's `min-height` is less than the box's
    // used height. When `min-height` determines the used height, the last child's bottom
    // margin no longer adjoins the box's bottom edge, so it stays inside the box instead of
    // collapsing with the box's own bottom margin. (`max-height` has no such effect.)
    let height_constrained_by_min_height = matches!(min_size.height, Some(h) if h > 0.0 && h >= container_outer_height);
    let own_bottom_margin_collapses_with_children =
        own_margins_collapse_with_children.end && !height_constrained_by_min_height;

    // Apply `align-content` to in-flow non-floated items if requested. The per-item layouts were
    // held back in `item.final_layout` so that this step can shift `location.y` before tree commit.
    //
    // For block layout the entire stack of in-flow children is treated as a single alignment
    // subject. That means distribution keywords (`space-between`, `space-around`,
    // `space-evenly`, `stretch`) must invoke the single-subject fallback unconditionally —
    // which is what passing `num_items = 1` to `apply_alignment_fallback` does. The whole
    // group then shifts by one offset, with zero inter-item gap.
    if let Some(align_content) = align_content {
        let container_inner_height = container_outer_height - resolved_content_box_inset.vertical_axis_sum();
        let inflow_content_height = intrinsic_outer_height - resolved_content_box_inset.vertical_axis_sum();
        let free_space = container_inner_height - inflow_content_height;
        let any_in_flow = items.iter().any(|item| item.final_layout.is_some());
        let keyword = apply_alignment_fallback(free_space, 1, align_content);
        let group_offset = compute_alignment_offset(free_space, 1, 0.0, keyword, false, true);
        // EXACT PATCH 26: an absolutely positioned child's static position
        // moves with the group, which is empty when nothing is in flow
        // (Chrome 154: the static position of a box after a centred 20px
        // child in 100px is 60, and of a lone one 50).
        for item in items.iter_mut().filter(|item| item.position == Position::Absolute) {
            item.static_position.y += group_offset;
        }
        if any_in_flow {
            first_baseline = first_baseline.map(|baseline| baseline + group_offset);
            for item in items.iter_mut() {
                if let Some(layout) = item.final_layout.as_mut() {
                    layout.location.y += group_offset;
                }
            }

            #[cfg(feature = "content_size")]
            {
                inflow_overflow_rect = Rect::ZERO;
                for item in items.iter() {
                    if let Some(layout) = item.final_layout.as_ref() {
                        let contribution_location = if direction.is_rtl() {
                            Point {
                                x: container_outer_width
                                    - (layout.location.x + layout.size.width)
                                    - resolved_border.right,
                                y: layout.location.y - resolved_border.top,
                            }
                        } else {
                            Point {
                                x: layout.location.x - resolved_border.left,
                                y: layout.location.y - resolved_border.top,
                            }
                        };
                        inflow_overflow_rect = inflow_overflow_rect.union(compute_scrollable_overflow_contribution(
                            contribution_location,
                            layout.size,
                            layout.scrollable_overflow_rect,
                            item.overflow,
                            item.contain,
                            is_scroll_container,
                        ));
                    }
                }
            }
        }
    }

    // Determine whether this node can be collapsed through
    let all_in_flow_children_can_be_collapsed_through = items.iter().all(|item| {
        #[cfg(feature = "float_layout")]
        if item.float.is_floated() {
            return true;
        }
        item.position == Position::Absolute || item.can_be_collapsed_through
    });
    let can_be_collapsed_through =
        !has_styles_preventing_being_collapsed_through && all_in_flow_children_can_be_collapsed_through;

    let mut output = LayoutOutput {
        size: final_outer_size,
        #[cfg(feature = "content_size")]
        scrollable_overflow_rect: Rect::ZERO,
        baselines: Baselines::from_first(first_baseline),
        top_margin: if own_margins_collapse_with_children.start {
            first_child_top_margin_set
        } else {
            let margin_top = raw_margin.top.resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
            CollapsibleMarginSet::from_margin(margin_top)
        },
        bottom_margin: if own_bottom_margin_collapses_with_children {
            last_child_bottom_margin_set
        } else {
            let margin_bottom =
                raw_margin.bottom.resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
            CollapsibleMarginSet::from_margin(margin_bottom)
        },
        margins_can_collapse_through: can_be_collapsed_through,
    };

    // Short-circuit if computing size.
    //
    // Note: it is important that we return the margin-collapsing related outputs here as Parent block containers
    // rely on the `top_margin`/`bottom_margin` of their children to compute their own intrinsic height.
    if run_mode == RunMode::ComputeSize {
        return output;
    }

    // Commit deferred child layouts to the tree.
    for item in items.iter() {
        if let Some(layout) = item.final_layout.as_ref() {
            tree.set_unrounded_layout(item.node_id, layout);
        }
    }

    // 4. Layout absolutely positioned children
    let absolute_position_inset = resolved_border + scrollbar_gutter;
    let absolute_position_area = final_outer_size - absolute_position_inset.sum_axes();
    let absolute_position_offset = Point { x: absolute_position_inset.left, y: absolute_position_inset.top };
    let clearance = tree.get_core_container_style(node_id).absolute_clearance();
    let absolute_overflow_rect = perform_absolute_layout_on_absolute_children(
        tree,
        node_id,
        &items,
        AbsolutePass {
            area_size: absolute_position_area,
            area_offset: absolute_position_offset,
            clearance,
            direction,
            sizing_mode: SizingMode::ContentSize,
            #[cfg(feature = "content_size")]
            is_scroll_container,
        },
        Line {
            start: resolved_content_box_inset.left,
            end: container_outer_width - resolved_content_box_inset.right,
        },
    );

    #[cfg(feature = "content_size")]
    {
        // A scroll container's own padding at the end of its children is part of its scrollable
        // overflow region. Boxes that are not scroll containers do not extend their overflow
        // region by their own padding.
        if is_scroll_container {
            let mut margin_box_rect = Rect::ZERO;
            for item in items.iter() {
                if let Some(layout) = item.final_layout.as_ref() {
                    let x = layout.location.x - item.relative_offset.x;
                    let location = Point {
                        x: if direction.is_rtl() {
                            container_outer_width - (x + layout.size.width) - resolved_border.right
                        } else {
                            x - resolved_border.left
                        },
                        y: layout.location.y - item.relative_offset.y - resolved_border.top,
                    };
                    margin_box_rect = margin_box_rect.union(margin_box_contribution(
                        location,
                        layout.size,
                        layout.margin,
                        direction.is_rtl(),
                    ));
                }
            }
            let end_padding = Size {
                width: if direction.is_rtl() { resolved_padding.left } else { resolved_padding.right },
                height: resolved_padding.bottom,
            };
            inflow_overflow_rect = with_end_padding(inflow_overflow_rect, margin_box_rect, end_padding);
        }
        output.scrollable_overflow_rect = inflow_overflow_rect.union(absolute_overflow_rect);
        // EXACT PATCH 27: the columns' overflow, not the flow thread's.
        if let Some(overflow) = multicol.and_then(|m| m.overflow) {
            output.scrollable_overflow_rect = overflow;
        }
    }

    // 5. Perform hidden layout on hidden children
    let len = tree.child_count(node_id);
    for order in 0..len {
        let child = tree.get_child_id(node_id, order);
        let child_style = tree.get_block_child_style(child);
        if child_style.box_generation_mode() == BoxGenerationMode::None {
            drop(child_style);
            tree.set_unrounded_layout(child, &Layout::with_order(order as u32));
            tree.perform_child_layout(
                child,
                Size::NONE,
                Size::NONE,
                Size::MAX_CONTENT,
                SizingMode::InherentSize,
                Line::FALSE,
            );
        }
    }

    output
}

/// Create a `Vec` of `BlockItem` structs where each item in the `Vec` represents a child of the current node
#[inline]
fn generate_item_list(
    tree: &impl LayoutBlockContainer,
    node: NodeId,
    node_inner_size: Size<Option<f32>>,
    justify_items: Option<JustifyItems>,
    direction: Direction,
) -> Vec<BlockItem> {
    tree.child_ids(node)
        .map(|child_node_id| (child_node_id, tree.get_block_child_style(child_node_id)))
        .filter(|(_, style)| style.box_generation_mode() != BoxGenerationMode::None)
        .enumerate()
        .map(|(order, (child_node_id, child_style))| {
            // EXACT PATCH 10: percentage padding and border widths refer to the
            // containing block's inline size on every side (CSS Box Model 3 §4),
            // as the item's own layout and the flex and grid algorithms resolve them.
            let padding =
                child_style.padding().resolve_or_zero(node_inner_size.width, |val, basis| tree.calc(val, basis));
            let border =
                child_style.border().resolve_or_zero(node_inner_size.width, |val, basis| tree.calc(val, basis));
            let pb_sum = (padding + border).sum_axes();
            let box_sizing_adjustment =
                if child_style.box_sizing() == BoxSizing::ContentBox { pb_sum } else { Size::ZERO };
            // EXACT PATCH 12: the size styles through the ratio (`compute::ratio`).
            let resolve = |s: Size<Option<f32>>| s.maybe_add(box_sizing_adjustment);
            let (size, min_size, max_size) = sizes_through_ratio(
                &child_style,
                pb_sum,
                resolve(child_style.size().maybe_resolve(node_inner_size, |val, basis| tree.calc(val, basis))),
                resolve(child_style.min_size().maybe_resolve(node_inner_size, |val, basis| tree.calc(val, basis))),
                resolve(child_style.max_size().maybe_resolve(node_inner_size, |val, basis| tree.calc(val, basis))),
            );

            let position = child_style.position();
            let overflow = child_style.overflow();

            #[cfg(feature = "float_layout")]
            let float = child_style.float();
            #[cfg(feature = "float_layout")]
            let is_not_floated = float == Float::None;

            #[cfg(not(feature = "float_layout"))]
            let is_not_floated = true;

            let is_block = child_style.is_block();
            let is_table = child_style.is_table();
            let is_replaced = child_style.is_compressible_replaced();
            let is_scroll_container = overflow.x.is_scroll_container() || overflow.y.is_scroll_container();
            let contain = child_style.contain();

            let is_in_same_bfc: bool = is_block
                && !is_table
                && position != Position::Absolute
                && is_not_floated
                && !is_scroll_container
                && !contain.establishes_independent_formatting_context();

            BlockItem {
                node_id: child_node_id,
                order: order as u32,
                // Patch 66. The web hosts write `justify-self: start` over a canvas and an
                // indeterminate progress except where this alignment applies
                // (host/web/src/sized_css.rs `aligns`; host/web-js/src/sized.rs, its static CSS
                // and bound writers): a change to which `justify-items` values align a block
                // child changes them too.
                justify_self: child_style.justify_self()
                    .or_else(|| justify_items.map(|mut alignment| {
                        // Chrome applies overflow safety only from the absolute child's own alignment.
                        if position == Position::Absolute {
                            alignment.safety = AlignmentSafety::Unsafe;
                        }
                        alignment
                    }))
                    .map(|mut alignment| {
                        // Chrome's block static-position path treats left/right as inline start/end.
                        if position == Position::Absolute {
                            alignment.keyword = match alignment.keyword {
                                AlignItemsKeyword::Left => AlignItemsKeyword::Start,
                                AlignItemsKeyword::Right => AlignItemsKeyword::End,
                                keyword => keyword,
                            };
                        }
                        alignment.resolve_self_relative(child_style.direction(), direction, true)
                    }),
                is_table,
                is_replaced,
                is_in_same_bfc,
                #[cfg(feature = "float_layout")]
                float,
                #[cfg(feature = "float_layout")]
                clear: child_style.clear(),
                size_style: child_style.size(),
                size,
                min_size,
                max_size,
                overflow,
                contain,
                scrollbar_width: child_style.scrollbar_width(),
                position,
                inset: child_style.inset(),
                margin: child_style.margin(),
                padding,
                border,
                padding_border_sum: pb_sum,

                // Fields to be computed later (for now we initialise with dummy values)
                computed_size: Size::zero(),
                static_position: Point::zero(),
                relative_offset: Point::zero(),
                can_be_collapsed_through: false,
                final_layout: None,
            }
        })
        .collect()
}

/// Resolve the `stretch` sizing keyword for an item's height style. In the block axis the
/// intrinsic sizing keywords (`min-content`, `max-content`, `fit-content`, `fit-content(...)`)
/// are all equal to the content size, which is what an auto height already resolves to, so
/// only `stretch` requires explicit resolution.
#[inline]
fn resolve_stretch_height(
    height_style: Dimension,
    container_inner_height: Option<f32>,
    item_y_margin_sum: f32,
) -> Option<f32> {
    match resolve_sizing_keyword(
        height_style,
        container_inner_height.maybe_sub(item_y_margin_sum),
        container_inner_height,
    ) {
        Some(SizingKeywordResolution::Exact(height)) => Some(height),
        _ => None,
    }
}

/// Compute the content-based width in the case that the width of the container is not known
#[inline]
fn determine_content_based_container_width(
    tree: &mut impl LayoutBlockContainer,
    items: &[BlockItem],
    available_width: AvailableSpace,
) -> f32 {
    let available_space = Size { width: available_width, height: AvailableSpace::MinContent };

    let mut max_child_width = 0.0;
    #[cfg(feature = "float_layout")]
    let mut float_contribution = FloatIntrinsicWidthCalculator::new(available_width);
    for item in items.iter().filter(|item| item.position != Position::Absolute) {
        let known_dimensions = item.size.maybe_clamp(item.min_size, item.max_size);

        let item_x_margin_sum = item
            .margin
            .resolve_or_zero(available_space.width.into_option(), |val, basis| tree.calc(val, basis))
            .horizontal_axis_sum();
        // EXACT PATCH 36: a cyclic percentage width or max-width compresses a
        // replaced child's min-content contribution (CSS Sizing 3 §5.2.1).
        // Chrome 151 takes zero even for calc(), floored by the authored inline
        // minimum, not a size transferred from height. This is a contribution
        // rule: the child's own content size and its final layout stay intact.
        let compressed = if item.is_replaced && available_width == AvailableSpace::MinContent {
            let style = tree.get_block_child_style(item.node_id);
            let cyclic = style.size().width.into_raw().uses_percentage()
                || style.max_size().width.into_raw().uses_percentage();
            cyclic.then(|| {
                // Resolve cyclic percentages in the minimum against zero, preserving its length terms.
                let min = style.min_size().width.maybe_resolve(Some(0.0), |val, basis| tree.calc(val, basis));
                let inset = if style.box_sizing() == BoxSizing::ContentBox { item.padding_border_sum.width } else { 0.0 };
                min.unwrap_or(0.0) + inset
            })
        } else {
            None
        };
        let width = compressed.or(known_dimensions.width).unwrap_or_else(|| {
            let item_available_width = match resolve_sizing_keyword(item.size_style.width, None, None) {
                Some(SizingKeywordResolution::Measure(available_width)) => available_width,
                Some(SizingKeywordResolution::Exact(width)) => AvailableSpace::Definite(width),
                None => available_space.width.maybe_sub(item_x_margin_sum),
            };
            tree.measure_child_size(
                item.node_id,
                known_dimensions,
                Size::NONE,
                Size { width: item_available_width, height: available_space.height },
                SizingMode::InherentSize,
                crate::AbsoluteAxis::Horizontal,
                Line::TRUE,
            )
        });

        let width = f32_max(width, item.padding_border_sum.width) + item_x_margin_sum;

        #[cfg(feature = "float_layout")]
        if let Some(direction) = item.float.float_direction() {
            float_contribution.add_float(width, direction, item.clear);
            continue;
        }

        max_child_width = f32_max(max_child_width, width);
    }

    #[cfg(feature = "float_layout")]
    {
        max_child_width = max_child_width.max(float_contribution.result());
    }

    max_child_width
}

/// Perform absolute layout on the absolutely positioned boxes this container is the
/// containing block of.
///
/// EXACT PATCH 18, 20: sized and placed by the shared solver (`common::absolute`). A
/// `Static` container keeps each absolute child's static position for the box that contains it.
#[inline]
fn perform_absolute_layout_on_absolute_children(
    tree: &mut impl LayoutBlockContainer,
    node: NodeId,
    items: &[BlockItem],
    pass: AbsolutePass,
    inline_bounds: Line<f32>,
) -> Rect<f32> {
    let contains = tree.get_core_container_style(node).position() != Position::Static;
    let mut absolute_overflow_rect = Rect::ZERO;
    for item in items.iter().filter(|item| item.position == Position::Absolute) {
        let rtl = pass.direction.is_rtl();
        let mut position = StaticPosition::point(item.static_position, rtl, item.order);
        if let Some(alignment) = item.justify_self {
            let start = if rtl { StaticAlignment::End } else { StaticAlignment::Start };
            let end = if rtl { StaticAlignment::Start } else { StaticAlignment::End };
            position.rect.left = inline_bounds.start;
            position.rect.right = inline_bounds.end;
            position.align.x = match alignment.keyword() {
                AlignItemsKeyword::End | AlignItemsKeyword::FlexEnd => end,
                AlignItemsKeyword::Center => StaticAlignment::Center,
                _ => start,
            };
            position.safe.x = alignment.is_safe().then_some(start);
        }
        if contains {
            absolute_overflow_rect =
                absolute_overflow_rect.union(pass.place(tree, item.node_id, position, None, Point::ZERO));
        } else {
            tree.set_static_position(item.node_id, position);
        }
    }
    absolute_overflow_rect = absolute_overflow_rect.union(pass.place_hoisted(tree, node));
    absolute_overflow_rect
}
