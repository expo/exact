//! This module is a partial implementation of the CSS Grid Level 1 specification
//! <https://www.w3.org/TR/css-grid-1>
use crate::compute::common::absolute::{AbsoluteArea, AbsolutePass};
#[cfg(feature = "content_size")]
use super::common::scrollable_overflow::{ScrollableOverflowContainer, finalize_scrollable_overflow};
use crate::compute::common::sizing_keyword::{OwnKeywordBounds, own_keyword_bounds};
use crate::compute::ratio::container_sizes_through_ratio;
use crate::geometry::{AbsoluteAxis, AbstractAxis, InBothAbsAxis};
use crate::geometry::{Line, Point, Rect, Size};
use crate::style::{AlignContent, AlignItems, AvailableSpace, Contain, Overflow, Position};
use crate::style_helpers::TaffyMaxContent;
use crate::tree::{Baselines, Layout, LayoutInput, LayoutOutput, LayoutPartialTreeExt, NodeId, RunMode, SizingMode};
use crate::util::MaybeMath;
use crate::util::debug::debug_log;
use crate::util::sys::{GridTrackVec, Vec, f32_max, f32_min};
use crate::util::{MaybeResolve, ResolveOrZero};
use crate::{
    BoxGenerationMode, BoxSizing, CoreStyle, Direction, GridContainerStyle, GridItemStyle, LayoutGridContainer,
    RequestedAxis,
};
use alignment::{align_and_position_item, align_tracks, static_position};
use explicit_grid::{AutoRepeatStrategy, compute_explicit_grid_size_in_axis, initialize_grid_tracks};
use implicit_grid::compute_grid_size_estimate;
use placement::{ItemPlacement, place_grid_items};
use track_sizing::{
    determine_if_item_crosses_flexible_or_intrinsic_tracks, resolve_item_track_indexes, track_sizing_algorithm,
};
use types::{CellOccupancyMatrix, GridAreasMap, GridTrack, NamedLineResolver};

#[cfg(feature = "detailed_layout_info")]
use crate::sys::{DefaultCheapStr, String};
#[cfg(feature = "detailed_layout_info")]
use crate::{CheapCloneStr, GridAreaAxis, GridPlacement};
#[cfg(feature = "detailed_layout_info")]
use core::ops::Range;
#[cfg(feature = "detailed_layout_info")]
use types::{GridTrackKind, TrackCounts};
use types::GridItem;

pub(crate) use types::{GridCoordinate, GridLine, LineLimit, MAX_GRID_TRACKS, OriginZeroLine};

pub use types::{GridLineNames, GridLineNamesIter};

mod alignment;
mod explicit_grid;
mod implicit_grid;
mod placement;
mod track_sizing;
mod types;
mod util;

/// The grid container's resolved box, size constraints, alignment styles and available space,
/// computed once from the container's style and the layout inputs before the grid is laid out
#[derive(Clone, Copy)]
struct GridContainerConstants {
    /// The container's `direction` style
    direction: Direction,
    /// The container's `contain` style
    contain: Contain,
    /// The container's resolved padding
    padding: Rect<f32>,
    /// The container's resolved border
    border: Rect<f32>,
    /// The sum of padding and border in each axis
    padding_border_size: Size<f32>,
    /// The container's border-box min size, if definite
    min_size: Size<Option<f32>>,
    /// The container's border-box max size, if definite
    max_size: Size<Option<f32>>,
    /// The container's border-box preferred size, if definite
    preferred_size: Size<Option<f32>>,
    /// The space reserved for scrollbars in each axis
    scrollbar_gutter: Point<f32>,
    /// Whether the container's overflow makes it a scroll container
    #[cfg(feature = "content_size")]
    is_scroll_container: bool,
    /// Padding, border and scrollbar gutter combined
    content_box_inset: Rect<f32>,
    /// The container's `align-content` style
    align_content: AlignContent,
    /// The container's `justify-content` style
    justify_content: AlignContent,
    /// The container's `align-items` style
    align_items: AlignItems,
    /// The container's `justify-items` style
    justify_items: AlignItems,
    /// The space available to size the grid tracks in each axis
    available_grid_space: Size<AvailableSpace>,
    /// The container's border-box size, if definite
    outer_node_size: Size<Option<f32>>,
    /// The container's content-box min size, if definite
    inner_min_size: Size<Option<f32>>,
    /// The container's content-box max size, if definite
    inner_max_size: Size<Option<f32>>,
    /// The container's content-box size, if definite
    inner_node_size: Size<Option<f32>>,
}

/// Resolve the grid container's style against the layout inputs (step 1 of `compute_grid_layout`)
fn compute_container_constants<Tree: LayoutGridContainer>(
    tree: &Tree,
    style: &Tree::GridContainerStyle<'_>,
    own: OwnKeywordBounds,
    inputs: LayoutInput,
) -> GridContainerConstants {
    let LayoutInput { known_dimensions, parent_size, available_space, .. } = inputs;

    let direction = style.direction();
    let contain = style.contain();

    // 1. Compute "available grid space"
    // https://www.w3.org/TR/css-grid-1/#available-grid-space
    let padding = style.padding().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let border = style.border().resolve_or_zero(parent_size.width, |val, basis| tree.calc(val, basis));
    let padding_border = padding + border;
    let padding_border_size = padding_border.sum_axes();
    let box_sizing_adjustment =
        if style.box_sizing() == BoxSizing::ContentBox { padding_border_size } else { Size::ZERO };

    // EXACT PATCH 56: in ContentSize, omit preferred sizes before ratio
    // resolution so an incoming dimension can transfer into the other axis.
    let preferred_size = if inputs.sizing_mode == SizingMode::InherentSize {
        style.size().maybe_resolve(parent_size, |val, basis| tree.calc(val, basis)).maybe_add(box_sizing_adjustment)
    } else {
        Size::NONE
    };
    // EXACT PATCH 12: the container's own size styles through the ratio (`compute::ratio`).
    // A definite width assigned by the parent (including grid-item stretch during track
    // measurement) supplies the ratio's automatic height floor just as an authored width does.
    let (preferred_size, min_size, max_size) = container_sizes_through_ratio(
        style,
        padding_border_size,
        known_dimensions.or(preferred_size),
        own.min(
            style
                .min_size()
                .maybe_resolve(parent_size, |val, basis| tree.calc(val, basis))
                .maybe_add(box_sizing_adjustment),
        ),
        own.max(
            style
                .max_size()
                .maybe_resolve(parent_size, |val, basis| tree.calc(val, basis))
                .maybe_add(box_sizing_adjustment),
        ),
        inputs.sizing_mode,
    );

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
    let mut content_box_inset = padding_border;
    content_box_inset.bottom += scrollbar_gutter.y;

    match direction {
        Direction::Ltr => content_box_inset.right += scrollbar_gutter.x,
        Direction::Rtl => content_box_inset.left += scrollbar_gutter.x,
    };

    // `normal` behaves as `stretch` for grid containers
    let align_content = style.grid_align_content(AbstractAxis::Block);
    let justify_content = style.grid_align_content(AbstractAxis::Inline);
    let align_items = style.align_items();
    let justify_items = style.justify_items();
    let constrained_available_space = known_dimensions
        .or(preferred_size)
        .map(|size| size.map(AvailableSpace::Definite))
        .unwrap_or(available_space)
        .maybe_clamp(min_size, max_size)
        .maybe_max(padding_border_size);

    let available_grid_space = Size {
        width: constrained_available_space
            .width
            .map_definite_value(|space| space - content_box_inset.horizontal_axis_sum()),
        height: constrained_available_space
            .height
            .map_definite_value(|space| space - content_box_inset.vertical_axis_sum()),
    };

    let outer_node_size =
        known_dimensions.or(preferred_size).maybe_clamp(min_size, max_size).maybe_max(padding_border_size);

    // The track sizing algorithm operates on the grid container's content box, so the min/max sizes
    // (which are border-box sizes) need converting to content-box sizes before being passed to it
    let inner_min_size = min_size.maybe_sub(content_box_inset.sum_axes());
    let inner_max_size = max_size.maybe_sub(content_box_inset.sum_axes());
    let inner_node_size = Size {
        width: outer_node_size.width.map(|space| space - content_box_inset.horizontal_axis_sum()),
        height: outer_node_size.height.map(|space| space - content_box_inset.vertical_axis_sum()),
    };

    GridContainerConstants {
        direction,
        contain,
        padding,
        border,
        padding_border_size,
        min_size,
        max_size,
        preferred_size,
        scrollbar_gutter,
        #[cfg(feature = "content_size")]
        is_scroll_container,
        content_box_inset,
        align_content,
        justify_content,
        align_items,
        justify_items,
        available_grid_space,
        outer_node_size,
        inner_min_size,
        inner_max_size,
        inner_node_size,
    }
}

/// Grid layout algorithm
/// This consists of a few phases:
///   - Resolving the explicit grid
///   - Placing items (which also resolves the implicit grid)
///   - Track (row/column) sizing
///   - Alignment & Final item placement
pub fn compute_grid_layout<Tree: LayoutGridContainer>(
    tree: &mut Tree,
    node: NodeId,
    inputs: LayoutInput,
) -> LayoutOutput {
    let LayoutInput { known_dimensions, run_mode, .. } = inputs;

    // EXACT PATCH 78: the container's own keyword bounds.
    let own = own_keyword_bounds(tree, node, inputs);
    let style = tree.get_grid_container_style(node);
    let constants = compute_container_constants(tree, &style, own, inputs);
    let GridContainerConstants {
        direction,
        contain,
        padding,
        border,
        padding_border_size,
        min_size,
        max_size,
        preferred_size,
        scrollbar_gutter,
        #[cfg(feature = "content_size")]
        is_scroll_container,
        content_box_inset,
        align_content,
        justify_content,
        align_items,
        justify_items,
        available_grid_space,
        outer_node_size,
        inner_min_size,
        inner_max_size,
        mut inner_node_size,
    } = constants;

    // Note: we avoid accessing the grid rows/columns methods more than once as this can
    // cause an expensive-ish computation
    let grid_template_columns = style.grid_template_columns();
    let grid_template_rows = style.grid_template_rows();
    let grid_auto_columns = style.grid_auto_columns();
    let grid_auto_rows = style.grid_auto_rows();

    debug_log!("parent_size", dbg:inputs.parent_size);
    debug_log!("outer_node_size", dbg:outer_node_size);
    debug_log!("inner_node_size", dbg:inner_node_size);

    // Short-circuit layout if the container's size is fully determined by the container's size and the run mode
    // is ComputeSize (and thus the container's size is all that we're interested in)
    if run_mode == RunMode::ComputeSize {
        if let Size { width: Some(width), height: Some(height) } = outer_node_size {
            return LayoutOutput::from_outer_size(Size { width, height });
        }

        // We can also short-circuit if the width is known and only the width has been requested.
        if inputs.axis == RequestedAxis::Horizontal {
            if let Some(width) = outer_node_size.width {
                return LayoutOutput::from_outer_size(Size { width, height: 0.0 });
            }
        }
    }

    // 2. Resolve the explicit grid

    // This is very similar to the inner_node_size except if the inner_node_size is not definite but the node
    // has a min- or max- size style then that will be used in it's place.
    let auto_fit_container_size = outer_node_size
        .or(max_size)
        .or(min_size)
        .maybe_clamp(min_size, max_size)
        .maybe_max(padding_border_size)
        .maybe_sub(content_box_inset.sum_axes());

    // If the grid container has a definite size or max size in the relevant axis:
    //   - then the number of repetitions is the largest possible positive integer that does not cause the grid to overflow the content
    //     box of its grid container.
    // Otherwise, if the grid container has a definite min size in the relevant axis:
    //   - then the number of repetitions is the smallest possible positive integer that fulfills that minimum requirement
    // Otherwise, the specified track list repeats only once.
    let auto_repeat_fit_strategy = outer_node_size.or(max_size).map(|val| match val {
        Some(_) => AutoRepeatStrategy::MaxRepetitionsThatDoNotOverflow,
        None => AutoRepeatStrategy::MinRepetitionsThatDoOverflow,
    });

    // Compute the number of rows and columns in the explicit grid *template*
    // (explicit tracks from grid_areas are computed separately below)
    let (col_auto_repetition_count, grid_template_col_count) = compute_explicit_grid_size_in_axis(
        &style,
        auto_fit_container_size.width,
        auto_repeat_fit_strategy.width,
        |val, basis| tree.calc(val, basis),
        AbsoluteAxis::Horizontal,
    );
    let (row_auto_repetition_count, grid_template_row_count) = compute_explicit_grid_size_in_axis(
        &style,
        auto_fit_container_size.height,
        auto_repeat_fit_strategy.height,
        |val, basis| tree.calc(val, basis),
        AbsoluteAxis::Vertical,
    );

    // type CustomIdent<'a> = <<Tree as LayoutPartialTree>::CoreContainerStyle<'_> as CoreStyle>::CustomIdent;
    let mut name_resolver = NamedLineResolver::new(&style, col_auto_repetition_count, row_auto_repetition_count);

    // EXACT PATCH 70: the lines this grid places items between
    let line_limit = LineLimit::with_max_line(style.grid_line_limit());

    // Clamp the explicit grid to MAX_GRID_TRACKS tracks in each axis
    // https://www.w3.org/TR/css-grid-1/#overlarge-grids
    let explicit_col_count = grid_template_col_count.max(name_resolver.area_column_count()).min(MAX_GRID_TRACKS);
    let explicit_row_count = grid_template_row_count.max(name_resolver.area_row_count()).min(MAX_GRID_TRACKS);

    name_resolver.set_explicit_column_count(explicit_col_count);
    name_resolver.set_explicit_row_count(explicit_row_count);

    // Build the per-line names of the explicit grid from the name resolver's collected pairs
    #[cfg(feature = "detailed_layout_info")]
    let mut detailed_column_line_names = name_resolver.detailed_line_names(AbsoluteAxis::Horizontal);
    #[cfg(feature = "detailed_layout_info")]
    let mut detailed_row_line_names = name_resolver.detailed_line_names(AbsoluteAxis::Vertical);

    // 3. Create the grid items (in document order) and resolve the named lines in their `grid-row`/`grid-column` styles
    // Absolutely positioned children do not take part in grid placement and do not create implicit tracks,
    // so they are excluded here. Each child's style is read and its named lines resolved exactly once; the
    // results drive both the grid size estimate and item placement below.
    let child_count = tree.child_count(node);
    let mut items: Vec<GridItem> = Vec::with_capacity(child_count);
    let mut placements: Vec<ItemPlacement> = Vec::with_capacity(child_count);
    for (index, child_node) in tree.child_ids(node).enumerate() {
        let child_style = tree.get_grid_child_style(child_node);
        if child_style.box_generation_mode() == BoxGenerationMode::None || child_style.position().is_out_of_flow() {
            continue;
        }
        placements.push(InBothAbsAxis {
            horizontal: name_resolver
                .resolve_column_names(&child_style.grid_column())
                .into_origin_zero(explicit_col_count, line_limit),
            vertical: name_resolver
                .resolve_row_names(&child_style.grid_row())
                .into_origin_zero(explicit_row_count, line_limit),
        });
        items.push(GridItem::new_with_style_and_order(
            child_node,
            child_style,
            align_items,
            justify_items,
            index as u16,
        ));
    }

    // 4. Implicit Grid: Estimate Track Counts
    // Estimate the number of rows and columns in the implicit grid (= the entire grid)
    // This is necessary as part of placement. Doing it early here is a perf optimisation to reduce allocations.
    let (est_col_counts, est_row_counts) =
        compute_grid_size_estimate(explicit_col_count, explicit_row_count, line_limit, &placements);

    // 5. Grid Item Placement
    // Match items (children) to a definite grid position (row start/end and column start/end position)
    let mut cell_occupancy_matrix = CellOccupancyMatrix::with_track_counts(est_col_counts, est_row_counts);
    // `items` is in document order from here on (placement only fills in each item's grid area). The track
    // sizing and baseline passes sort references to the items rather than the items themselves.
    place_grid_items(&mut cell_occupancy_matrix, &mut items, &placements, style.grid_auto_flow(), line_limit);

    // Extract track counts from previous step (auto-placement can expand the number of tracks)
    let final_col_counts = *cell_occupancy_matrix.track_counts(AbsoluteAxis::Horizontal);
    let final_row_counts = *cell_occupancy_matrix.track_counts(AbsoluteAxis::Vertical);

    // 5. Initialize Tracks
    // Initialize (explicit and implicit) grid tracks (and gutters)
    // This resolves the min and max track sizing functions for all tracks and gutters
    let mut columns = GridTrackVec::new();
    let mut rows = GridTrackVec::new();
    initialize_grid_tracks(
        &mut columns,
        final_col_counts,
        &style,
        AbsoluteAxis::Horizontal,
        col_auto_repetition_count,
        |column_index| cell_occupancy_matrix.column_is_occupied(column_index),
    );
    initialize_grid_tracks(
        &mut rows,
        final_row_counts,
        &style,
        AbsoluteAxis::Vertical,
        row_auto_repetition_count,
        |row_index| cell_occupancy_matrix.row_is_occupied(row_index),
    );

    drop(grid_template_rows);
    drop(grid_template_columns);
    drop(grid_auto_rows);
    drop(grid_auto_columns);
    drop(style);

    // 6. Track Sizing

    // Convert grid placements in origin-zero coordinates to indexes into the GridTrack (rows and columns) vectors
    // This computation is relatively trivial, but it requires the final number of negative (implicit) tracks in
    // each axis, and doing it up-front here means we don't have to keep repeating that calculation
    resolve_item_track_indexes(&mut items, final_col_counts, final_row_counts);
    // For each item, and in each axis, determine whether the item crosses any flexible (fr) tracks
    // Record this as a boolean (per-axis) on each item for later use in the track-sizing algorithm
    determine_if_item_crosses_flexible_or_intrinsic_tracks(&mut items, &columns, &rows);

    // Determine if the grid has any baseline aligned items
    let has_baseline_aligned_item = items.iter().any(|item| item.participates_in_baseline_alignment());

    // Run track sizing algorithm for Inline axis
    track_sizing_algorithm(
        tree,
        AbstractAxis::Inline,
        inner_min_size.get(AbstractAxis::Inline),
        inner_max_size.get(AbstractAxis::Inline),
        justify_content,
        align_content,
        available_grid_space,
        inner_node_size,
        &mut columns,
        &mut rows,
        &mut items,
        |track: &GridTrack, parent_size: Option<f32>, tree: &Tree| {
            track.max_track_sizing_function.definite_value(parent_size, |val, basis| tree.calc(val, basis))
        },
        has_baseline_aligned_item,
        (run_mode == RunMode::ComputeSize).then_some(node),
    );
    let initial_column_sum = columns.iter().map(|track| track.base_size).sum::<f32>();
    inner_node_size.width = inner_node_size.width.or_else(|| initial_column_sum.into());

    // If only the container's width has been requested then we can skip sizing the rows entirely (which avoids
    // measuring the height of every item), as column sizing is not re-run when only computing the container's size.
    // Note: the short-circuit above has already handled the case where the container's width is known.
    if run_mode == RunMode::ComputeSize && inputs.axis == RequestedAxis::Horizontal {
        let width = (initial_column_sum + content_box_inset.horizontal_axis_sum())
            .maybe_clamp(min_size.width, max_size.width)
            .max(padding_border_size.width);
        return LayoutOutput::from_outer_size(Size { width, height: 0.0 });
    }

    items.iter_mut().for_each(|item| item.grid_area_size_cache = None);

    // Run track sizing algorithm for Block axis
    track_sizing_algorithm(
        tree,
        AbstractAxis::Block,
        inner_min_size.get(AbstractAxis::Block),
        inner_max_size.get(AbstractAxis::Block),
        align_content,
        justify_content,
        available_grid_space,
        inner_node_size,
        &mut rows,
        &mut columns,
        &mut items,
        |track: &GridTrack, _, _| Some(track.base_size),
        false, // TODO: Support baseline alignment in the vertical axis
        None,
    );
    let initial_row_sum = rows.iter().map(|track| track.base_size).sum::<f32>();

    debug_log!("initial_column_sum", dbg:initial_column_sum);
    debug_log!(dbg: columns.iter().map(|track| track.base_size).collect::<Vec<_>>());
    debug_log!("initial_row_sum", dbg:initial_row_sum);
    debug_log!(dbg: rows.iter().map(|track| track.base_size).collect::<Vec<_>>());

    // 6. Compute container size
    let resolved_style_size = known_dimensions.or(preferred_size);
    let mut container_border_box = Size {
        width: resolved_style_size
            .get(AbstractAxis::Inline)
            .unwrap_or_else(|| initial_column_sum + content_box_inset.horizontal_axis_sum())
            .maybe_clamp(min_size.width, max_size.width)
            .max(padding_border_size.width),
        height: resolved_style_size
            .get(AbstractAxis::Block)
            .unwrap_or_else(|| initial_row_sum + content_box_inset.vertical_axis_sum())
            .maybe_clamp(min_size.height, max_size.height)
            .max(padding_border_size.height),
    };
    let mut container_content_box = Size {
        width: f32_max(0.0, container_border_box.width - content_box_inset.horizontal_axis_sum()),
        height: f32_max(0.0, container_border_box.height - content_box_inset.vertical_axis_sum()),
    };

    // If only the container's size has been requested
    if run_mode == RunMode::ComputeSize {
        return LayoutOutput::from_outer_size(container_border_box);
    }

    // The container's size is now determined, so percentages resolve against its content box when re-running track sizing.
    // This may be a no-op if the container's size was known upfront (in which case it already equals `inner_node_size`).
    inner_node_size = container_content_box.map(Some);

    // Column sizing must be re-run (once) if:
    //   - The grid container's width was initially indefinite and there are any columns with percentage track sizing functions
    //   - Any grid item crossing an intrinsically sized track's min content contribution width has changed
    // TODO: Only rerun sizing for tracks that actually require it rather than for all tracks if any need it.
    let mut rerun_column_sizing;
    let mut intrinsic_column_contribution_changed = false;

    let has_percentage_column = columns.iter().any(|track| track.uses_percentage());
    let has_percentage_row = rows.iter().any(|track| track.uses_percentage());
    let parent_width_indefinite = outer_node_size.width.is_none();
    rerun_column_sizing = parent_width_indefinite && has_percentage_column;

    if !rerun_column_sizing {
        // Note: every item must be visited (no short-circuiting) as the closure updates each item's caches
        intrinsic_column_contribution_changed =
            items.iter_mut().filter(|item| item.crosses_intrinsic_column).fold(false, |any_changed, item| {
                let grid_area_size = item.grid_area_size(
                    AbstractAxis::Inline,
                    &columns,
                    &rows,
                    inner_node_size,
                    |track: &GridTrack, _| Some(track.base_size),
                    &|val, basis| tree.calc(val, basis),
                );
                let available_space = grid_area_size.with(AbstractAxis::Inline, None);
                let new_min_content_contribution =
                    item.min_content_contribution(AbstractAxis::Inline, tree, grid_area_size, available_space);

                let has_changed = Some(new_min_content_contribution) != item.min_content_contribution_cache.width;

                item.grid_area_size_cache = Some(grid_area_size);
                item.min_content_contribution_cache.width = Some(new_min_content_contribution);
                item.max_content_contribution_cache.width = None;
                item.minimum_contribution_cache.width = None;

                any_changed | has_changed
            });
        rerun_column_sizing = intrinsic_column_contribution_changed;
    } else {
        // Clear intrinsic width caches
        items.iter_mut().for_each(|item| {
            item.grid_area_size_cache = None;
            item.min_content_contribution_cache.width = None;
            item.max_content_contribution_cache.width = None;
            item.minimum_contribution_cache.width = None;
        });
    }

    // Row sizing must be re-run (once) if:
    //   - The grid container's height was initially indefinite and there are any rows with percentage track sizing functions
    //   - Column sizing was re-run and any grid item crossing an intrinsically sized track's min content contribution height has changed
    // TODO: Only rerun sizing for tracks that actually require it rather than for all tracks if any need it.
    let parent_height_indefinite = outer_node_size.height.is_none();
    let mut rerun_row_sizing = parent_height_indefinite && has_percentage_row;
    let mut intrinsic_row_contribution_changed = false;

    if rerun_row_sizing {
        // Clear intrinsic height caches
        items.iter_mut().for_each(|item| {
            item.grid_area_size_cache = None;
            item.min_content_contribution_cache.height = None;
            item.max_content_contribution_cache.height = None;
            item.minimum_contribution_cache.height = None;
        });
    }

    if rerun_column_sizing {
        // Re-run track sizing algorithm for Inline axis
        track_sizing_algorithm(
            tree,
            AbstractAxis::Inline,
            inner_min_size.get(AbstractAxis::Inline),
            inner_max_size.get(AbstractAxis::Inline),
            justify_content,
            align_content,
            available_grid_space,
            inner_node_size,
            &mut columns,
            &mut rows,
            &mut items,
            |track: &GridTrack, _, _| Some(track.base_size),
            has_baseline_aligned_item,
            (run_mode == RunMode::ComputeSize).then_some(node),
        );

        // The column widths may have changed, so the items' intrinsic height contributions need to be recomputed
        // (unless row sizing is being re-run anyway, in which case the caches have already been cleared).
        if !rerun_row_sizing {
            // Note: every item must be visited (no short-circuiting) as the closure updates each item's caches
            intrinsic_row_contribution_changed =
                items.iter_mut().filter(|item| item.crosses_intrinsic_row).fold(false, |any_changed, item| {
                    let grid_area_size = item.grid_area_size(
                        AbstractAxis::Block,
                        &rows,
                        &columns,
                        inner_node_size,
                        |track: &GridTrack, _| Some(track.base_size),
                        &|val, basis| tree.calc(val, basis),
                    );
                    let available_space = grid_area_size.with(AbstractAxis::Block, None);
                    let new_min_content_contribution =
                        item.min_content_contribution(AbstractAxis::Block, tree, grid_area_size, available_space);

                    let has_changed = Some(new_min_content_contribution) != item.min_content_contribution_cache.height;

                    item.grid_area_size_cache = Some(grid_area_size);
                    item.min_content_contribution_cache.height = Some(new_min_content_contribution);
                    item.max_content_contribution_cache.height = None;
                    item.minimum_contribution_cache.height = None;

                    any_changed | has_changed
                });
            rerun_row_sizing = intrinsic_row_contribution_changed;
        }
    }

    if rerun_row_sizing {
        // Re-run track sizing algorithm for Block axis
        track_sizing_algorithm(
            tree,
            AbstractAxis::Block,
            inner_min_size.get(AbstractAxis::Block),
            inner_max_size.get(AbstractAxis::Block),
            align_content,
            justify_content,
            available_grid_space,
            inner_node_size,
            &mut rows,
            &mut columns,
            &mut items,
            |track: &GridTrack, _, _| Some(track.base_size),
            false, // TODO: Support baseline alignment in the vertical axis
            None,
        );
    }

    if (intrinsic_column_contribution_changed && !has_percentage_column)
        || (intrinsic_row_contribution_changed && !has_percentage_row)
    {
        let final_column_sum = columns.iter().map(|track| track.base_size).sum::<f32>();
        let final_row_sum = rows.iter().map(|track| track.base_size).sum::<f32>();

        if intrinsic_column_contribution_changed && !has_percentage_column {
            container_border_box.width = resolved_style_size
                .get(AbstractAxis::Inline)
                .unwrap_or_else(|| final_column_sum + content_box_inset.horizontal_axis_sum())
                .maybe_clamp(min_size.width, max_size.width)
                .max(padding_border_size.width);
            container_content_box.width =
                f32_max(0.0, container_border_box.width - content_box_inset.horizontal_axis_sum());
        }

        if intrinsic_row_contribution_changed && !has_percentage_row {
            container_border_box.height = resolved_style_size
                .get(AbstractAxis::Block)
                .unwrap_or_else(|| final_row_sum + content_box_inset.vertical_axis_sum())
                .maybe_clamp(min_size.height, max_size.height)
                .max(padding_border_size.height);
            container_content_box.height =
                f32_max(0.0, container_border_box.height - content_box_inset.vertical_axis_sum());
        }
    }

    // If only the container's size has been requested
    if run_mode == RunMode::ComputeSize {
        return LayoutOutput::from_outer_size(container_border_box);
    }

    // 8. Track Alignment

    // Align columns
    let inline_size_without_scrollbar = f32_max(container_border_box.width - padding_border_size.width, 0.0);
    let inline_scrollbar_gutter_for_alignment = f32_min(scrollbar_gutter.x, inline_size_without_scrollbar);
    align_tracks(
        container_content_box.get(AbstractAxis::Inline),
        Line {
            start: padding.left + if direction.is_rtl() { inline_scrollbar_gutter_for_alignment } else { 0.0 },
            end: padding.right + if direction.is_rtl() { 0.0 } else { inline_scrollbar_gutter_for_alignment },
        },
        Line { start: border.left, end: border.right },
        &mut columns,
        justify_content,
        direction.is_rtl(),
    );
    // Align rows
    align_tracks(
        container_content_box.get(AbstractAxis::Block),
        Line { start: padding.top, end: padding.bottom },
        Line { start: border.top, end: border.bottom },
        &mut rows,
        align_content,
        false,
    );

    // 9. Size, Align, and Position Grid Items

    #[cfg_attr(not(feature = "content_size"), allow(unused_mut))]
    let mut item_overflow_rect = Rect::ZERO;
    #[cfg_attr(not(feature = "content_size"), allow(unused_mut, unused))]
    let mut absolute_overflow_rect = Rect::ZERO;

    let container_alignment_styles = InBothAbsAxis { horizontal: justify_items, vertical: align_items };

    let padding_box_inset = border
        + Rect {
            left: if direction.is_rtl() { scrollbar_gutter.x } else { 0.0 },
            right: if direction.is_rtl() { 0.0 } else { scrollbar_gutter.x },
            top: 0.0,
            bottom: scrollbar_gutter.y,
        };
    #[cfg(feature = "content_size")]
    let overflow_container = ScrollableOverflowContainer {
        parent_is_scroll_container: is_scroll_container,
        parent_size: Size { width: container_border_box.width - padding_box_inset.horizontal_axis_sum(), height: 0.0 },
        reversed: Point { x: direction.is_rtl(), y: false },
        padding,
    };

    // Position in-flow children (stored in items vector)
    for (index, item) in items.iter_mut().enumerate() {
        // Tracks are stored in logical order. In RTL the physical offsets are assigned
        // right-to-left, so an item's physical left edge is derived from its logical end
        // line and its physical right edge from its logical start line.
        let grid_area = Rect {
            top: rows[item.row_indexes.start as usize + 1].offset,
            bottom: rows[item.row_indexes.end as usize].offset,
            left: if direction.is_rtl() {
                columns[item.column_indexes.end as usize - 1].offset
            } else {
                columns[item.column_indexes.start as usize + 1].offset
            },
            right: if direction.is_rtl() {
                columns[item.column_indexes.start as usize].offset
            } else {
                columns[item.column_indexes.end as usize].offset
            },
        };
        #[cfg_attr(not(feature = "content_size"), allow(unused_variables))]
        let (overflow_contribution, y_position, height, baseline) = align_and_position_item(
            tree,
            item.node,
            index as u32,
            grid_area,
            Size { width: grid_area.right - grid_area.left, height: grid_area.bottom - grid_area.top },
            container_alignment_styles,
            item.baseline_shim,
            direction,
            #[cfg(feature = "content_size")]
            padding_box_inset,
            #[cfg(feature = "content_size")]
            &overflow_container,
        );
        item.y_position = y_position;
        item.height = height;
        item.baseline = baseline;

        #[cfg(feature = "content_size")]
        {
            item_overflow_rect = item_overflow_rect.union(overflow_contribution);
        }
    }

    // Position hidden and absolutely positioned children
    let contains_absolutes = tree.get_core_container_style(node).position() != Position::Static;
    let absolute_pass = AbsolutePass {
        clearance: tree.get_core_container_style(node).absolute_clearance(),
        area_size: Size {
            width: f32_max(container_border_box.width - border.horizontal_axis_sum() - scrollbar_gutter.x, 0.0),
            height: f32_max(container_border_box.height - border.vertical_axis_sum() - scrollbar_gutter.y, 0.0),
        },
        area_offset: Point { x: padding_box_inset.left, y: padding_box_inset.top },
        direction,
        sizing_mode: SizingMode::InherentSize,
        #[cfg(feature = "content_size")]
        is_scroll_container,
        #[cfg(feature = "content_size")]
        reversed: Point { x: direction.is_rtl(), y: false },
    };
    let AbsoluteArea { size: absolute_area, offset: absolute_offset, .. } = absolute_pass.inner_area();
    let mut order = items.len() as u32;
    (0..tree.child_count(node)).for_each(|index| {
        let child = tree.get_child_id(node, index);
        let child_style = tree.get_grid_child_style(child);

        // Position hidden child
        if child_style.box_generation_mode() == BoxGenerationMode::None {
            drop(child_style);
            tree.set_unrounded_layout(child, &Layout::with_order(order));
            tree.perform_child_layout(
                child,
                Size::NONE,
                Size::NONE,
                Size::MAX_CONTENT,
                SizingMode::InherentSize,
                Line::FALSE,
            );
            order += 1;
            return;
        }

        // Position absolutely positioned child
        if child_style.position().is_out_of_flow() {
            // Convert grid-col-{start/end} into Option's of indexes into the columns vector
            // The Option is None if the style property is Auto and an unresolvable Span
            let maybe_col_indexes = name_resolver
                .resolve_column_names(&child_style.grid_column())
                .into_origin_zero(final_col_counts.explicit, line_limit)
                .resolve_absolutely_positioned_grid_tracks()
                .map(|maybe_grid_line| {
                    maybe_grid_line.and_then(|line: OriginZeroLine| line.try_into_track_vec_index(final_col_counts))
                });
            // Convert grid-row-{start/end} into Option's of indexes into the row vector
            // The Option is None if the style property is Auto and an unresolvable Span
            let maybe_row_indexes = name_resolver
                .resolve_row_names(&child_style.grid_row())
                .into_origin_zero(final_row_counts.explicit, line_limit)
                .resolve_absolutely_positioned_grid_tracks()
                .map(|maybe_grid_line| {
                    maybe_grid_line.and_then(|line: OriginZeroLine| line.try_into_track_vec_index(final_row_counts))
                });

            /// Resolve a grid line (by track vector index) used as a start edge to a position
            fn line_as_start_edge(tracks: &[GridTrack], index: usize) -> f32 {
                tracks.get(index + 1).unwrap_or(&tracks[index]).offset
            }
            /// Resolve a grid line (by track vector index) used as an end edge to a position
            fn line_as_end_edge(tracks: &[GridTrack], index: usize) -> f32 {
                if index == 0 { tracks.get(1).unwrap_or(&tracks[0]).offset } else { tracks[index].offset }
            }
            /// Resolve a grid line used as an inline-start edge to a physical right x-position (RTL)
            fn rtl_line_as_start_edge(tracks: &[GridTrack], index: usize) -> f32 {
                if tracks.len() > index + 1 {
                    tracks[index].offset
                } else if index == 0 {
                    tracks[0].offset
                } else {
                    tracks[index - 1].offset
                }
            }
            /// Resolve a grid line used as an inline-end edge to a physical left x-position (RTL)
            fn rtl_line_as_end_edge(tracks: &[GridTrack], index: usize) -> f32 {
                if index == 0 { tracks[0].offset } else { tracks[index - 1].offset }
            }

            // In RTL the item's physical left edge derives from its logical end line and its
            // physical right edge from its logical start line.
            let (grid_area_left, grid_area_right) = if direction.is_rtl() {
                (
                    maybe_col_indexes
                        .end
                        .map(|index| rtl_line_as_end_edge(&columns, index))
                        .unwrap_or(absolute_offset.x),
                    maybe_col_indexes
                        .start
                        .map(|index| rtl_line_as_start_edge(&columns, index))
                        .unwrap_or(absolute_offset.x + absolute_area.width),
                )
            } else {
                (
                    maybe_col_indexes.start.map(|index| line_as_start_edge(&columns, index)).unwrap_or(absolute_offset.x),
                    maybe_col_indexes
                        .end
                        .map(|index| line_as_end_edge(&columns, index))
                        .unwrap_or(absolute_offset.x + absolute_area.width),
                )
            };

            let grid_area = Rect {
                top: maybe_row_indexes.start.map(|index| line_as_start_edge(&rows, index)).unwrap_or(absolute_offset.y),
                bottom: maybe_row_indexes
                    .end
                    .map(|index| line_as_end_edge(&rows, index))
                    .unwrap_or(absolute_offset.y + absolute_area.height),
                left: grid_area_left,
                right: grid_area_right,
            };
            // EXACT PATCH 18, 20: sized and placed by the shared solver (`common::absolute`). The
            // static position is aligned in the area when the grid is the containing block, and
            // in the grid's content box when it is not (CSS Grid 1 §9.2, as Chrome places them): a
            // `Static` grid keeps it for the box that contains the child.
            let static_rect = if contains_absolutes {
                grid_area
            } else {
                Rect {
                    left: content_box_inset.left,
                    right: container_border_box.width - content_box_inset.right,
                    top: content_box_inset.top,
                    bottom: container_border_box.height - content_box_inset.bottom,
                }
            };
            let position = static_position(&child_style, container_alignment_styles, direction, static_rect, order);
            drop(child_style);
            // EXACT PATCH 69: the area, for a caller that places what the child holds outside it,
            // whichever box contains the child.
            tree.set_grid_origin(child, Point { x: grid_area.left, y: grid_area.top });

            if contains_absolutes {
                let (left_index, right_index) = if direction.is_rtl() {
                    (maybe_col_indexes.end, maybe_col_indexes.start)
                } else {
                    (maybe_col_indexes.start, maybe_col_indexes.end)
                };
                let area = AbsoluteArea {
                    size: Size {
                        width: f32_max(grid_area.right - grid_area.left, 0.0),
                        height: f32_max(grid_area.bottom - grid_area.top, 0.0),
                    },
                    offset: Point { x: grid_area.left, y: grid_area.top },
                    clearance: Rect {
                        top: maybe_row_indexes.start.map_or(absolute_pass.clearance.top, |_| 0.0),
                        right: right_index.map_or(absolute_pass.clearance.right, |_| 0.0),
                        bottom: maybe_row_indexes.end.map_or(absolute_pass.clearance.bottom, |_| 0.0),
                        left: left_index.map_or(absolute_pass.clearance.left, |_| 0.0),
                    },
                };
                #[cfg_attr(not(feature = "content_size"), allow(unused_variables))]
                let overflow_contribution = absolute_pass.place(tree, child, position, Some(area), Point::ZERO);
                #[cfg(feature = "content_size")]
                {
                    absolute_overflow_rect = absolute_overflow_rect.union(overflow_contribution);
                }
            } else {
                tree.set_static_position(child, position);
            }

            order += 1;
        }
    });
    {
        #[cfg_attr(not(feature = "content_size"), allow(unused_variables))]
        let hoisted_overflow = absolute_pass.place_hoisted(tree, node);
        #[cfg(feature = "content_size")]
        {
            absolute_overflow_rect = absolute_overflow_rect.union(hoisted_overflow);
        }
    }

    #[cfg(feature = "detailed_layout_info")]
    {
        let areas = name_resolver
            .populate_detailed_line_resolvers(&mut detailed_row_line_names, &mut detailed_column_line_names);
        tree.set_detailed_grid_info(
            node,
            DetailedGridInfo {
                rows: DetailedGridTracksInfo::from_grid_tracks_and_track_count(
                    final_row_counts,
                    rows,
                    detailed_row_line_names,
                ),
                columns: DetailedGridTracksInfo::from_grid_tracks_and_track_count(
                    final_col_counts,
                    columns,
                    detailed_column_line_names,
                ),
                items: items.iter().map(DetailedGridItemsInfo::from_grid_item).collect(),
                areas,
            },
        );
    }

    #[cfg(feature = "content_size")]
    let scrollable_overflow_rect = {
        let client_width = (container_border_box.width - border.horizontal_axis_sum() - scrollbar_gutter.x).max(0.0);
        let overflow_rect = if is_scroll_container {
            item_overflow_rect.union(Rect {
                left: if direction.is_rtl() { (client_width - padding.left).min(0.0) } else { 0.0 },
                right: if direction.is_rtl() { client_width } else { padding.right },
                top: 0.0,
                bottom: padding.bottom,
            })
        } else {
            item_overflow_rect.union(Rect { right: if direction.is_rtl() { client_width } else { 0.0 }, ..Rect::ZERO })
        };
        finalize_scrollable_overflow(
            overflow_rect.union(absolute_overflow_rect),
            container_border_box - border.sum_axes() - Size { width: scrollbar_gutter.x, height: scrollbar_gutter.y },
            Point { x: direction.is_rtl(), y: false },
            is_scroll_container,
        )
    };
    #[cfg(not(feature = "content_size"))]
    let scrollable_overflow_rect = crate::tree::ScrollableOverflow::ZERO;

    // If there are no in-flow items then return the container size and the overflow (no baseline)
    if items.is_empty() {
        return LayoutOutput::from_sizes(container_border_box, scrollable_overflow_rect);
    }

    // Determine the grid container baseline(s) (currently we only compute the first baseline)
    // Layout containment suppresses the box's baseline for baseline-alignment purposes
    let grid_container_baseline: Option<f32> = if contain.suppresses_baseline() {
        None
    } else {
        // Get the row index of the first row containing items
        let first_row = items.iter().map(|item| item.row_indexes.start).min().unwrap();

        // Check if any items in *this row* participate in baseline alignment
        // (items with an auto block-axis margin do not participate: https://www.w3.org/TR/css-align-3/#baseline-align-self)
        let mut first_row_items = items.iter().filter(|item| item.row_indexes.start == first_row);
        let first_item = first_row_items.next().unwrap();
        let item = if first_item.participates_in_baseline_alignment() {
            first_item
        } else {
            first_row_items.find(|item| item.participates_in_baseline_alignment()).unwrap_or(first_item)
        };

        Some(item.y_position + item.baseline.unwrap_or(item.height))
    };

    LayoutOutput::from_sizes_and_baselines(
        container_border_box,
        scrollable_overflow_rect,
        Baselines::from_first(grid_container_baseline),
    )
}

/// Information from the computation of grid
#[cfg(feature = "detailed_layout_info")]
#[derive(Debug, Clone, PartialEq)]
pub struct DetailedGridInfo<S: CheapCloneStr = DefaultCheapStr> {
    /// <https://drafts.csswg.org/css-grid-1/#grid-row>
    pub rows: DetailedGridTracksInfo<S>,
    /// <https://drafts.csswg.org/css-grid-1/#grid-column>
    pub columns: DetailedGridTracksInfo<S>,
    /// <https://drafts.csswg.org/css-grid-1/#grid-items>
    pub items: Vec<DetailedGridItemsInfo>,
    /// Grid area edge lookup used to resolve the grid placements of absolutely positioned children
    areas: GridAreasMap<S>,
}

#[cfg(feature = "detailed_layout_info")]
impl<S: CheapCloneStr> DetailedGridTracksInfo<S> {
    /// Resolve an absolute placement in this axis to physical start and end coordinates
    fn resolve_absolute_grid_axis(
        &self,
        placement: Line<GridPlacement<S>>,
        areas: &GridAreasMap<S>,
        axis: GridAreaAxis,
        padding_start: f32,
        padding_end: f32,
        is_reversed: bool,
    ) -> Line<f32> {
        let track_counts = TrackCounts {
            negative_implicit: self.negative_implicit_tracks,
            explicit: self.explicit_tracks,
            positive_implicit: self.positive_implicit_tracks,
        };
        let min_line = -(track_counts.negative_implicit as i16);
        let max_line = (track_counts.explicit + track_counts.positive_implicit) as i16;
        let placement = self
            .line_names
            .resolve_line_names(&placement, areas, axis, self.explicit_tracks)
            .into_origin_zero(self.explicit_tracks, LineLimit::with_max_line(max_line as u16))
            .resolve_absolutely_positioned_grid_tracks()
            .map(|line| line.filter(|line| line.0 >= min_line && line.0 <= max_line))
            .map(|line| line.and_then(|line| line.try_into_track_vec_index(track_counts).map(|index| index / 2)));
        let start_position = placement
            .start
            .and_then(|line| {
                (line < self.positions.len())
                    .then(|| self.absolute_track_edge(line, true, is_reversed))
                    .or_else(|| self.positions.last().map(|track| if is_reversed { track.start } else { track.end }))
                    .or(self.empty_axis_line)
            })
            .unwrap_or(if is_reversed { padding_end } else { padding_start });
        let end_position = placement
            .end
            .and_then(|line| {
                line.checked_sub(1)
                    .filter(|&line| line < self.positions.len())
                    .map(|line| self.absolute_track_edge(line, false, is_reversed))
                    .or_else(|| self.positions.first().map(|track| if is_reversed { track.end } else { track.start }))
                    .or(self.empty_axis_line)
            })
            .unwrap_or(if is_reversed { padding_start } else { padding_end });

        if is_reversed {
            Line { start: f32_min(start_position, end_position), end: start_position }
        } else {
            Line { start: start_position, end: f32_max(start_position, end_position) }
        }
    }

    /// Resolve a track edge, excluding gutters that overlap across a collapsed track run.
    fn absolute_track_edge(&self, index: usize, is_start: bool, is_reversed: bool) -> f32 {
        let range_index = self.collapsed_tracks.partition_point(|range| range.end <= index);
        let range = self.collapsed_tracks.get(range_index).filter(|range| range.contains(&index));
        let track = range
            .and_then(|range| {
                let adjacent = if is_start { Some(range.end) } else { range.start.checked_sub(1) };
                adjacent.and_then(|index| self.positions.get(index))
            })
            .unwrap_or(&self.positions[index]);
        if is_start != is_reversed { track.start } else { track.end }
    }
}

#[cfg(feature = "detailed_layout_info")]
impl<S: CheapCloneStr> DetailedGridInfo<S> {
    /// Write the used row track sizes and line names to the passed writer in the resolved value
    /// format of the `grid-template-rows` property
    pub fn write_grid_template_rows(&self, out: &mut impl core::fmt::Write, zoom: f32) -> core::fmt::Result {
        self.rows.write_track_list(out, zoom)
    }

    /// Write the used column track sizes and line names to the passed writer in the resolved value
    /// format of the `grid-template-columns` property
    pub fn write_grid_template_columns(&self, out: &mut impl core::fmt::Write, zoom: f32) -> core::fmt::Result {
        self.columns.write_track_list(out, zoom)
    }

    /// Serialize the used row track sizes and line names in the resolved value format of the
    /// `grid-template-rows` property
    pub fn grid_template_rows(&self, zoom: f32) -> String {
        self.rows.to_track_list_string(zoom)
    }

    /// Serialize the used column track sizes and line names in the resolved value format of the
    /// `grid-template-columns` property
    pub fn grid_template_columns(&self, zoom: f32) -> String {
        self.columns.to_track_list_string(zoom)
    }

    /// Resolve the physical grid area for an absolutely positioned box from its grid placement.
    pub fn resolve_absolute_grid_area(
        &self,
        grid_row: Line<GridPlacement<S>>,
        grid_column: Line<GridPlacement<S>>,
        direction: Direction,
        padding_box: Rect<f32>,
    ) -> Rect<f32> {
        let columns = self.columns.resolve_absolute_grid_axis(
            grid_column,
            &self.areas,
            GridAreaAxis::Column,
            padding_box.left,
            padding_box.right,
            direction.is_rtl(),
        );
        let rows = self.rows.resolve_absolute_grid_axis(
            grid_row,
            &self.areas,
            GridAreaAxis::Row,
            padding_box.top,
            padding_box.bottom,
            false,
        );
        Rect { left: columns.start, right: columns.end, top: rows.start, bottom: rows.end }
    }

    /// Compute the location and size of the grid area occupied by the item at `item_index`.
    pub fn item_grid_area(&self, item_index: usize) -> Option<(Point<f32>, Size<f32>)> {
        let item = self.items.get(item_index)?;
        let start_col = self.columns.positions[item.column_start as usize - 1];
        let end_col = self.columns.positions[item.column_end as usize - 2];
        let left = f32_min(start_col.start, end_col.start);
        let right = f32_max(start_col.end, end_col.end);
        let top = self.rows.positions[item.row_start as usize - 1].start;
        let bottom = self.rows.positions[item.row_end as usize - 2].end;
        Some((Point { x: left, y: top }, Size { width: right - left, height: bottom - top }))
    }
}

/// Information from the computation of grids tracks
#[cfg(feature = "detailed_layout_info")]
#[derive(Debug, Clone, PartialEq)]
pub struct DetailedGridTracksInfo<S: CheapCloneStr = DefaultCheapStr> {
    /// Number of leading implicit grid tracks
    pub negative_implicit_tracks: u16,
    /// Number of explicit grid tracks
    pub explicit_tracks: u16,
    /// Number of trailing implicit grid tracks
    pub positive_implicit_tracks: u16,
    /// The start and end position of each track relative to the grid container's border box.
    pub positions: Vec<Line<f32>>,
    /// Sorted, disjoint ranges of collapsed `auto-fit` tracks (indices into `positions`).
    pub collapsed_tracks: Vec<Range<usize>>,
    /// The position of the axis' single grid line relative to the grid container's border box
    /// when the axis has no tracks.
    pub empty_axis_line: Option<f32>,
    /// The names of each *explicit* grid line.
    pub line_names: GridLineNames<S>,
}

#[cfg(feature = "detailed_layout_info")]
impl<S: CheapCloneStr> DetailedGridTracksInfo<S> {
    fn positions_from_grid_track_layout(grid_tracks: &[GridTrack]) -> (Vec<Line<f32>>, Vec<Range<usize>>) {
        let mut positions = Vec::with_capacity(grid_tracks.len() / 2);
        let mut collapsed_tracks: Vec<Range<usize>> = Vec::new();
        for track in grid_tracks.iter().filter(|track| track.kind == GridTrackKind::Track) {
            if track.is_collapsed {
                let index = positions.len();
                match collapsed_tracks.last_mut() {
                    Some(range) if range.end == index => range.end += 1,
                    _ => collapsed_tracks.push(index..index + 1),
                }
            }
            positions.push(Line { start: track.offset, end: track.offset + track.base_size });
        }
        (positions, collapsed_tracks)
    }

    fn from_grid_tracks_and_track_count(
        track_count: TrackCounts,
        grid_tracks: Vec<GridTrack>,
        line_names: GridLineNames<S>,
    ) -> Self {
        let (positions, collapsed_tracks) = DetailedGridTracksInfo::<S>::positions_from_grid_track_layout(&grid_tracks);
        let empty_axis_line = if positions.is_empty() { grid_tracks.first().map(|track| track.offset) } else { None };
        DetailedGridTracksInfo {
            negative_implicit_tracks: track_count.negative_implicit,
            explicit_tracks: track_count.explicit,
            positive_implicit_tracks: track_count.positive_implicit,
            positions,
            collapsed_tracks,
            empty_axis_line,
            line_names,
        }
    }

    /// The names of the grid line with the passed 0-indexed line index.
    pub fn names_for_line(&self, line_index: usize) -> &[S] {
        match line_index.checked_sub(self.negative_implicit_tracks as usize) {
            Some(stored_index) => self.line_names.line(stored_index),
            None => &[],
        }
    }

    /// Iterate over the name group (`&[S]`) of each grid line of the full grid in line order.
    pub fn iter_line_names(&self) -> GridLineNamesIter<'_, S> {
        if self.line_names.is_empty() {
            return self.line_names.iter();
        }
        let total_line_count = self.positions.len() + 1;
        let leading_empty = self.negative_implicit_tracks as usize;
        let trailing_empty = total_line_count.saturating_sub(leading_empty + self.line_names.line_count());
        self.line_names.iter_padded(leading_empty, trailing_empty)
    }

    /// Write the used track sizes and line names of this axis to the passed writer.
    pub fn write_track_list(&self, out: &mut impl core::fmt::Write, zoom: f32) -> core::fmt::Result {
        fn write_line_names<S: CheapCloneStr>(out: &mut impl core::fmt::Write, names: &[S]) -> core::fmt::Result {
            out.write_char('[')?;
            for (i, name) in names.iter().enumerate() {
                if i != 0 {
                    out.write_char(' ')?;
                }
                out.write_str(name.as_ref())?;
            }
            out.write_char(']')
        }

        if self.positions.is_empty() {
            return out.write_str("none");
        }

        let mut needs_space = false;
        for (track_index, position) in self.positions.iter().enumerate() {
            let names = self.names_for_line(track_index);
            if !names.is_empty() {
                if needs_space {
                    out.write_char(' ')?;
                }
                write_line_names(out, names)?;
                needs_space = true;
            }
            if needs_space {
                out.write_char(' ')?;
            }
            write!(out, "{}px", (position.end - position.start) / zoom)?;
            needs_space = true;
        }
        let trailing_names = self.names_for_line(self.positions.len());
        if !trailing_names.is_empty() {
            out.write_char(' ')?;
            write_line_names(out, trailing_names)?;
        }
        Ok(())
    }

    /// Serialize the used track sizes and line names of this axis.
    pub fn to_track_list_string(&self, zoom: f32) -> String {
        let mut out = String::new();
        self.write_track_list(&mut out, zoom).expect("writing to a String cannot fail");
        out
    }
}

/// Grid area information from the placement algorithm
#[cfg(feature = "detailed_layout_info")]
#[derive(Debug, Clone, PartialEq)]
pub struct DetailedGridItemsInfo {
    /// row-start with 1-indexed grid line numbers
    pub row_start: u16,
    /// row-end with 1-indexed grid line numbers
    pub row_end: u16,
    /// column-start with 1-indexed grid line numbers
    pub column_start: u16,
    /// column-end with 1-indexed grid line numbers
    pub column_end: u16,
}

#[cfg(feature = "detailed_layout_info")]
impl DetailedGridItemsInfo {
    #[inline(always)]
    fn from_grid_item(grid_item: &GridItem) -> Self {
        #[inline(always)]
        fn to_one_indexed_grid_line(grid_track_index: u16) -> u16 {
            grid_track_index / 2 + 1
        }

        DetailedGridItemsInfo {
            row_start: to_one_indexed_grid_line(grid_item.row_indexes.start),
            row_end: to_one_indexed_grid_line(grid_item.row_indexes.end),
            column_start: to_one_indexed_grid_line(grid_item.column_indexes.start),
            column_end: to_one_indexed_grid_line(grid_item.column_indexes.end),
        }
    }
}
