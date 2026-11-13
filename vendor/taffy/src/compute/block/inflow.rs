//! Final sizing and placement of in-flow block children.

use super::{resolve_stretch_height, BlockContext, BlockItem};
use crate::compute::common::sizing_keyword::{resolve_sizing_keyword, SizingKeywordResolution};
#[cfg(feature = "content_size")]
use crate::compute::common::scrollable_overflow::compute_scrollable_overflow_contribution;
use crate::geometry::{Line, Point, Rect, Size};
use crate::style::{AlignItemsKeyword, AvailableSpace, Direction, Overflow, Position};
use crate::tree::{CollapsibleMarginSet, Layout, LayoutInput, LayoutPartialTreeExt, RunMode, SizingMode};
use crate::util::sys::f32_max;
use crate::util::{MaybeMath, MaybeResolve};
use crate::{LayoutBlockContainer, RequestedAxis, TextAlign};

/// Compute each child's final size and position
#[inline]
#[allow(clippy::too_many_arguments)]
pub(super) fn perform_final_layout_on_in_flow_children(
    tree: &mut impl LayoutBlockContainer,
    run_mode: RunMode,
    items: &mut [BlockItem],
    container_outer_width: f32,
    container_percentage_resolution_height: Option<f32>,
    content_box_inset: Rect<f32>,
    resolved_content_box_inset: Rect<f32>,
    resolved_border: Rect<f32>,
    text_align: TextAlign,
    direction: Direction,
    own_margins_collapse_with_children: Line<bool>,
    #[cfg(feature = "content_size")] is_scroll_container: bool,
    block_ctx: &mut BlockContext<'_>,
) -> (Rect<f32>, f32, CollapsibleMarginSet, CollapsibleMarginSet, Option<f32>) {
    // Resolve container_inner_width for sizing child nodes using initial content_box_inset
    let container_inner_width = container_outer_width - resolved_content_box_inset.horizontal_axis_sum();
    let container_percentage_resolution_height =
        container_percentage_resolution_height.maybe_sub(resolved_content_box_inset.vertical_axis_sum());
    let parent_size = Size { width: Some(container_inner_width), height: container_percentage_resolution_height };
    // Vertical available space in block flow is indefinite, NOT a min-content
    // constraint: MaxContent is taffy's representation of "indefinite".
    // Passing MinContent here made every descendant grid believe it was being
    // sized under a min-content constraint, in which the maximize-tracks step
    // has zero free space — so auto rows containing only scroll-container
    // items (overflow != visible, automatic minimum size = 0) collapsed to
    // zero height. Browsers size such rows to the item's content.
    let available_space =
        Size { width: AvailableSpace::Definite(container_inner_width), height: AvailableSpace::MaxContent };

    // TODO: handle nested blocks with different widths
    #[cfg(feature = "float_layout")]
    if block_ctx.is_bfc_root() {
        block_ctx.set_width(container_outer_width);
        block_ctx.apply_content_box_inset([resolved_content_box_inset.left, resolved_content_box_inset.right]);
    }

    // If this block's top margin does not collapse with its children's then the position of its
    // top margin strut is resolved relative to it, and floats adjoining ancestor struts do not
    // adjoin this block's strut.
    #[cfg(feature = "float_layout")]
    if !own_margins_collapse_with_children.start {
        block_ctx.commit_strut();
    }

    #[cfg_attr(not(feature = "content_size"), allow(unused_mut))]
    let mut inflow_overflow_rect = Rect::ZERO;
    let mut committed_y_offset = resolved_content_box_inset.top;
    let mut y_offset_for_absolute = resolved_content_box_inset.top;
    let mut first_child_top_margin_set = CollapsibleMarginSet::ZERO;
    let mut active_collapsible_margin_set = CollapsibleMarginSet::ZERO;
    let mut is_collapsing_with_first_margin_set = true;
    let mut first_baseline: Option<f32> = None;
    // Whether the active margin set contains the margins of a self-collapsing element with
    // clearance. Such margins collapse with the margins of following siblings but the resulting
    // margin does not collapse with the bottom margin of the parent block.
    let mut active_margin_set_has_clearance = false;

    #[cfg(feature = "float_layout")]
    let mut has_active_floats = block_ctx.has_active_floats(committed_y_offset);
    #[cfg(not(feature = "float_layout"))]
    let has_active_floats = false;

    for item in items.iter_mut() {
        if item.position == Position::Absolute {
            let x = match direction {
                Direction::Ltr => resolved_content_box_inset.left,
                Direction::Rtl => container_outer_width - resolved_content_box_inset.right,
            };
            item.static_position = Point { x, y: y_offset_for_absolute }
        } else {
            let item_margin = item
                .margin
                .map(|margin| margin.resolve_to_option(container_inner_width, |val, basis| tree.calc(val, basis)));
            let item_non_auto_margin = item_margin.map(|m| m.unwrap_or(0.0));
            let item_non_auto_x_margin_sum = item_non_auto_margin.horizontal_axis_sum();

            let scrollbar_size = Size {
                width: if item.overflow.y == Overflow::Scroll { item.scrollbar_width } else { 0.0 },
                height: if item.overflow.x == Overflow::Scroll { item.scrollbar_width } else { 0.0 },
            };

            // Handle floated boxes
            #[cfg(feature = "float_layout")]
            if let Some(float_direction) = item.float.float_direction() {
                has_active_floats = true;

                // A float with `width: auto` is shrink-to-fit (fit-content) sized: the available
                // space clamped between its min-content and max-content sizes.
                let available_width = container_inner_width - item_non_auto_x_margin_sum;
                let (item_known_width, item_available_width) = match resolve_sizing_keyword(
                    item.size_style.width,
                    Some(available_width),
                    Some(container_inner_width),
                ) {
                    Some(SizingKeywordResolution::Measure(available)) => (None, available),
                    Some(SizingKeywordResolution::Exact(width)) => (Some(width), AvailableSpace::Definite(width)),
                    None => (None, AvailableSpace::Definite(available_width)),
                };
                let item_known_height = resolve_stretch_height(
                    item.size_style.height,
                    container_percentage_resolution_height,
                    item_non_auto_margin.vertical_axis_sum(),
                );
                let item_layout = tree.perform_child_layout(
                    item.node_id,
                    Size { width: item_known_width, height: item_known_height },
                    parent_size,
                    Size { width: item_available_width, height: AvailableSpace::MaxContent },
                    SizingMode::InherentSize,
                    // A float establishes a new block formatting context: its margins do not
                    // collapse with the margins of its children
                    Line::FALSE,
                );
                let margin_box = item_layout.size + item_non_auto_margin.sum_axes();

                // Floats that occur between collapsing margins are positioned as if they had an otherwise
                // empty anonymous block parent taking part in the flow, so the pending collapsible margins
                // contribute to the float's minimum y position (unless those margins collapse with the
                // container's own top margin, in which case they are applied outside the container).
                //
                // In the latter case the position of the float is not fully resolved: margins contributed
                // by later siblings can still collapse into the strut and move the container (and float).
                // Such floats force clearance on cleared elements whose margins adjoin the same strut.
                let adjoins_unresolved_strut =
                    is_collapsing_with_first_margin_set && own_margins_collapse_with_children.start;
                let y_offset_for_float = if adjoins_unresolved_strut {
                    committed_y_offset
                } else {
                    committed_y_offset + active_collapsible_margin_set.resolve()
                };

                let mut location = block_ctx.place_floated_box(
                    margin_box,
                    y_offset_for_float,
                    float_direction,
                    item.clear,
                    adjoins_unresolved_strut,
                );

                // Ensure that content that appears after a float does not get positioned before/above the float
                //
                // FIXME: this isn't quite right, because a second float at the same location
                // shouldn't cause content to push down to it's level
                // committed_y_offset = committed_y_offset.max(location.y);
                // y_offset_for_absolute = y_offset_for_absolute.max(location.y);

                // Convert the margin-box location returned by float placement into a border-box location
                // for the output Layout
                location.y += item_non_auto_margin.top;
                location.x += item_non_auto_margin.left;

                // println!("BLOCK FLOATED BOX ({:?}) {:?}", item.node_id, float_direction);
                // println!("w:{} h:{} x:{}, y:{}", margin_box.width, margin_box.height, location.x, location.y);

                // Deferred to the post-loop pass in `compute_inner` (like in-flow items) so that
                // `align-content` can shift `location.y` before the layout is committed.
                item.final_layout = Some(Layout {
                    order: item.order,
                    size: item_layout.size,
                    #[cfg(feature = "content_size")]
                    scrollable_overflow_rect: item_layout.scrollable_overflow_rect,
                    scrollbar_size,
                    location,
                    padding: item.padding,
                    border: item.border,
                    margin: item_non_auto_margin,
                });

                #[cfg(feature = "content_size")]
                {
                    // TODO: Should the overflow of floated boxes count as "inflow_overflow_rect"
                    // or should it be counted separately?
                    let contribution_location = if direction.is_rtl() {
                        Point {
                            x: container_outer_width - (location.x + item_layout.size.width) - resolved_border.right,
                            y: location.y - resolved_border.top,
                        }
                    } else {
                        Point { x: location.x - resolved_border.left, y: location.y - resolved_border.top }
                    };
                    inflow_overflow_rect = inflow_overflow_rect.union(compute_scrollable_overflow_contribution(
                        contribution_location,
                        item_layout.size,
                        item_layout.scrollable_overflow_rect,
                        item.overflow,
                        item.contain,
                        is_scroll_container,
                    ));
                }

                continue;
            }

            // Handle non-floated boxes

            let mut y_margin_offset: f32 = 0.0;
            #[cfg(feature = "float_layout")]
            let mut item_avoids_floats = false;
            #[cfg(feature = "float_layout")]
            let mut item_pushed_below_float = false;

            let (stretch_width, float_avoiding_position, float_avoiding_width) = if item.is_in_same_bfc {
                let stretch_width = container_inner_width - item_non_auto_x_margin_sum;
                let position = Point { x: 0.0, y: 0.0 };
                let width = 0.0;

                (stretch_width, position, width)
            } else {
                'block: {
                    // Set y_margin_offset (different bfc child)
                    if !is_collapsing_with_first_margin_set || !own_margins_collapse_with_children.start {
                        y_margin_offset =
                            active_collapsible_margin_set.collapse_with_margin(item_non_auto_margin.top).resolve();
                    };
                    let min_y = committed_y_offset + y_margin_offset;

                    // In addition to the running flag, check the float context directly:
                    // floats placed by the subtree of a preceding in-flow sibling (in the same
                    // BFC) are not reflected in the flag
                    #[cfg(feature = "float_layout")]
                    if has_active_floats || block_ctx.has_active_floats(min_y) {
                        let x_margins = [item_non_auto_margin.left, item_non_auto_margin.right];
                        // An auto width resolves to at least the negation of the margin sum
                        // (so that the margin box width is non-negative, per CSS2 §10.3.3)
                        let min_auto_width = -item_non_auto_x_margin_sum;

                        // Find the highest slot (at or below `min_y`) with enough horizontal space
                        // for the item's border box, which must not overlap any float
                        let mut slot_segment = None;
                        let slot = loop {
                            let slot = block_ctx.find_bfc_slot(min_y, x_margins, direction, item.clear, slot_segment);
                            let Some(segment_id) = slot.segment_id else { break slot };
                            let width = item
                                .size
                                .width
                                .unwrap_or(slot.stretch_width.max(min_auto_width))
                                .maybe_clamp(item.min_size.width, item.max_size.width);
                            if width <= slot.border_width + 0.001 {
                                break slot;
                            }
                            slot_segment = Some(segment_id);
                        };

                        // If the item had to move down to avoid floats then it "separates from the
                        // float": similarly to clearance, its top margin no longer collapses with
                        // the parent's margins.
                        if slot.y > min_y {
                            item_pushed_below_float = true;
                        }

                        has_active_floats = slot.segment_id.is_some();
                        item_avoids_floats = true;
                        let stretch_width = slot.stretch_width.max(min_auto_width);
                        break 'block (stretch_width, Point { x: slot.x, y: slot.y }, slot.border_width);
                    }

                    if !has_active_floats {
                        let stretch_width = container_inner_width - item_non_auto_x_margin_sum;
                        break 'block (
                            stretch_width,
                            Point { x: resolved_content_box_inset.left, y: min_y },
                            container_inner_width,
                        );
                    }

                    unreachable!("One of the above cases will always be hit");
                }
            };

            // EXACT PATCH 21: tables are shrink-to-fit rather than stretch-sized. Exact
            // also uses this marker for form controls (including a button whose
            // inner display is flex): clamp its preferred width between its
            // min-content width and the available block width.
            let known_dimensions = if item.is_table && item.size.width.is_none() {
                let mut measure = |width| {
                    tree.measure_child_size(
                        item.node_id,
                        Size::NONE,
                        Size { width: None, height: parent_size.height },
                        Size { width, height: AvailableSpace::MaxContent },
                        SizingMode::InherentSize,
                        crate::AbsoluteAxis::Horizontal,
                        Line::TRUE,
                    )
                };
                let min_content = measure(AvailableSpace::MinContent);
                let max_content = measure(AvailableSpace::MaxContent);
                let width = stretch_width
                    .max(min_content)
                    .min(max_content)
                    .maybe_clamp(item.min_size.width, item.max_size.width);
                Size { width: Some(width), height: item.size.height }
            // Replaced elements are not stretch-sized: an auto width resolves
            // to the intrinsic size.
            } else if item.is_table || item.is_replaced {
                Size::NONE
            } else {
                // Items with a sizing keyword width (min-content, max-content, fit-content,
                // fit-content(...), stretch) resolve their width either directly or by measuring
                // the item under the corresponding available space constraint
                let keyword_width =
                    resolve_sizing_keyword(item.size_style.width, Some(stretch_width), Some(container_inner_width))
                        .map(|resolution| match resolution {
                            SizingKeywordResolution::Exact(width) => width,
                            SizingKeywordResolution::Measure(item_available_width) => tree.measure_child_size(
                                item.node_id,
                                Size::NONE,
                                parent_size,
                                Size { width: item_available_width, height: AvailableSpace::MaxContent },
                                SizingMode::InherentSize,
                                crate::AbsoluteAxis::Horizontal,
                                Line::TRUE,
                            ),
                        });

                let keyword_height = resolve_stretch_height(
                    item.size_style.height,
                    container_percentage_resolution_height,
                    item_non_auto_margin.vertical_axis_sum(),
                );

                // EXACT PATCH 66: non-stretch alignment makes an auto width
                // fit-content, even when auto margins take over placement.
                let auto_width = if item.size.width.is_none()
                    && keyword_width.is_none()
                    && item.justify_self.is_some_and(|alignment| alignment.keyword() != AlignItemsKeyword::Stretch)
                {
                    let mut measure = |width| {
                        tree.measure_child_size(
                            item.node_id,
                            Size { width: None, height: item.size.height },
                            parent_size,
                            Size { width, height: AvailableSpace::MaxContent },
                            SizingMode::InherentSize,
                            crate::AbsoluteAxis::Horizontal,
                            Line::TRUE,
                        )
                    };
                    let min_content = measure(AvailableSpace::MinContent);
                    let max_content = measure(AvailableSpace::MaxContent);
                    stretch_width.max(min_content).min(max_content)
                } else {
                    stretch_width
                };

                item.size
                    .map_width(|width| {
                        Some(
                            width
                                .or(keyword_width)
                                .unwrap_or(auto_width)
                                .maybe_clamp(item.min_size.width, item.max_size.width),
                        )
                    })
                    .map_height(|height| height.or(keyword_height))
                    .maybe_clamp(item.min_size, item.max_size)
            };

            //

            let ratio_min = crate::compute::ratio::minimum_ratio_width(tree, item.node_id, parent_size, None);
            let known_dimensions = known_dimensions.map_width(|width| width.maybe_max(ratio_min));
            let inputs = LayoutInput {
                run_mode,
                sizing_mode: SizingMode::InherentSize,
                axis: RequestedAxis::Both,
                known_dimensions,
                known_dimensions_are_definite: Size { width: true, height: true },
                parent_size,
                available_space: available_space.map_width(|_| AvailableSpace::Definite(stretch_width)),
                vertical_margins_are_collapsible: if item.is_in_same_bfc { Line::TRUE } else { Line::FALSE },
            };

            #[cfg(feature = "float_layout")]
            let clear_threshold = block_ctx.cleared_threshold(item.clear);
            #[cfg(feature = "float_layout")]
            let clear_pos = clear_threshold.unwrap_or(f32::NEG_INFINITY);
            #[cfg(not(feature = "float_layout"))]
            let clear_pos = f32::NEG_INFINITY;

            let item_layout = if item.is_in_same_bfc {
                // Replaced elements may not have a known width (they are sized by their
                // measure function rather than stretch-sized)
                let width = known_dimensions.width.unwrap_or(stretch_width);

                // TODO: account for auto margins
                let inset_left = item_non_auto_margin.left + content_box_inset.left;
                let inset_right = container_outer_width - width - inset_left;
                let insets = [inset_left, inset_right];

                // Compute child layout
                let mut child_block_ctx =
                    block_ctx.sub_context((y_offset_for_absolute + item_non_auto_margin.top).max(clear_pos), insets);
                let output = tree.compute_block_child_layout(item.node_id, inputs, Some(&mut child_block_ctx));

                // Extract float contribution from child block context
                #[cfg(feature = "float_layout")]
                {
                    let child_contribution = child_block_ctx.floated_content_height_contribution();
                    let child_top_adjoining_floats = child_block_ctx.top_adjoining_floats();
                    block_ctx.add_child_floated_content_height_contribution(y_offset_for_absolute + child_contribution);
                    // Floats placed while the position of the child's top margin strut was unresolved
                    // also adjoin this block's current strut
                    block_ctx.merge_adjoining_floats(child_top_adjoining_floats);
                }

                output
            } else {
                tree.compute_child_layout(item.node_id, inputs)
            };
            let final_size = item_layout.size;

            let top_margin_set = item_layout.top_margin.collapse_with_margin(item_margin.top.unwrap_or(0.0));
            let bottom_margin_set = item_layout.bottom_margin.collapse_with_margin(item_margin.bottom.unwrap_or(0.0));

            // Expand auto margins to fill available space
            // Note: Vertical auto-margins for relatively positioned block items simply resolve to 0.
            // See: https://www.w3.org/TR/CSS21/visudet.html#abs-non-replaced-width
            let free_x_space = f32_max(0.0, stretch_width - final_size.width);
            let x_axis_auto_margin_size = {
                let auto_margin_count = item_margin.left.is_none() as u8 + item_margin.right.is_none() as u8;
                if auto_margin_count > 0 {
                    free_x_space / auto_margin_count as f32
                } else {
                    0.0
                }
            };
            let resolved_margin = Rect {
                left: item_margin.left.unwrap_or(x_axis_auto_margin_size),
                right: item_margin.right.unwrap_or(x_axis_auto_margin_size),
                top: top_margin_set.resolve(),
                bottom: bottom_margin_set.resolve(),
            };

            // Resolve item inset
            let inset_percentage_basis =
                Size { width: Some(container_inner_width), height: container_percentage_resolution_height };
            let inset = item
                .inset
                .zip_size(inset_percentage_basis, |p, s| p.maybe_resolve(s, |val, basis| tree.calc(val, basis)));
            // EXACT PATCH 20: a `Static` box's insets do nothing.
            let inset_offset = if item.position == Position::Static {
                Point::ZERO
            } else {
                Point {
                    x: if direction.is_rtl() {
                        inset.right.map(|x| -x).or(inset.left).unwrap_or(0.0)
                    } else {
                        inset.left.or(inset.right.map(|x| -x)).unwrap_or(0.0)
                    },
                    y: inset.top.or(inset.bottom.map(|x| -x)).unwrap_or(0.0),
                }
            };

            // Set y_margin_offset (same bfc child)
            if item.is_in_same_bfc
                && (!is_collapsing_with_first_margin_set || !own_margins_collapse_with_children.start)
            {
                y_margin_offset = active_collapsible_margin_set.collapse_with_set(top_margin_set).resolve()
            };

            // Compute clearance (CSS2.2 9.5.2). Clearance is introduced if the hypothetical position of the
            // item's top border edge (the position it would have with normal margin collapsing) is not past
            // the bottom of the relevant floats. When clearance is introduced the item's border edge is
            // placed at `max(float bottom, hypothetical position)`.
            #[cfg(feature = "float_layout")]
            let mut has_clearance = false;
            #[cfg(not(feature = "float_layout"))]
            let has_clearance = false;
            #[cfg(feature = "float_layout")]
            if item.is_in_same_bfc {
                if let Some(threshold) = clear_threshold {
                    // The hypothetical position always includes the item's collapsed top margin set, even
                    // when those margins collapse with the container's own top margin (and are thus applied
                    // outside the container): in that case they still move the container (and hence the item)
                    // relative to the floats.
                    let hypothetical_y =
                        committed_y_offset + active_collapsible_margin_set.collapse_with_set(top_margin_set).resolve();
                    // Clearance is forced (regardless of the hypothetical position) if a relevant float is
                    // adjoining the margin-collapse strut that the item's top margin would collapse into:
                    // if the margins were allowed to collapse they would pull the float down with the item,
                    // so clearance is inserted to separate the two, placing the item just below the float.
                    let forced_clearance = block_ctx.has_adjoining_float(item.clear);
                    if forced_clearance || hypothetical_y < threshold {
                        has_clearance = true;
                        // Clearance stops the item's top margin collapsing with preceding margins. If those
                        // preceding margins collapse with the container's own top margin they are applied
                        // outside the container (moving it down), so the item's cleared position within the
                        // container must be reduced by that amount to keep its absolute position correct.
                        let escaped_margin =
                            if is_collapsing_with_first_margin_set && own_margins_collapse_with_children.start {
                                active_collapsible_margin_set.resolve()
                            } else {
                                0.0
                            };
                        y_margin_offset = threshold - committed_y_offset - escaped_margin;
                    }
                }
            }

            item.computed_size = item_layout.size;
            item.can_be_collapsed_through = item_layout.margins_can_collapse_through && !has_clearance;
            item.static_position = if item.is_in_same_bfc {
                let uncleared_y = committed_y_offset + active_collapsible_margin_set.resolve();
                Point {
                    x: match direction {
                        Direction::Ltr => resolved_content_box_inset.left,
                        Direction::Rtl => container_outer_width - resolved_content_box_inset.right - final_size.width,
                    },
                    y: uncleared_y.max(clear_pos),
                }
            } else {
                // TODO: handle inset and margins
                Point {
                    x: match direction {
                        Direction::Ltr => float_avoiding_position.x,
                        Direction::Rtl => float_avoiding_position.x + float_avoiding_width - final_size.width,
                    },
                    y: float_avoiding_position.y,
                }
            };
            let mut location = if item.is_in_same_bfc {
                Point {
                    x: match direction {
                        Direction::Ltr => resolved_content_box_inset.left + inset_offset.x + resolved_margin.left,
                        Direction::Rtl => {
                            container_outer_width
                                - resolved_content_box_inset.right
                                - final_size.width
                                - resolved_margin.right
                                + inset_offset.x
                        }
                    },
                    y: committed_y_offset + y_margin_offset + inset_offset.y,
                }
            } else {
                // When the item avoids floats, its non-auto margins are already accounted for in the
                // slot's border-box position/width (margins may overlap floats), so only the auto
                // portion of the resolved margin is added here.
                #[cfg(feature = "float_layout")]
                let (extra_margin_left, extra_margin_right) = if item_avoids_floats {
                    (
                        resolved_margin.left - item_non_auto_margin.left,
                        resolved_margin.right - item_non_auto_margin.right,
                    )
                } else {
                    (resolved_margin.left, resolved_margin.right)
                };
                #[cfg(not(feature = "float_layout"))]
                let (extra_margin_left, extra_margin_right) = (resolved_margin.left, resolved_margin.right);

                // TODO: handle inset and margins
                Point {
                    x: match direction {
                        Direction::Ltr => float_avoiding_position.x + extra_margin_left + inset_offset.x,
                        Direction::Rtl => {
                            float_avoiding_position.x + float_avoiding_width - final_size.width - extra_margin_right
                                + inset_offset.x
                        }
                    },
                    y: float_avoiding_position.y + inset_offset.y,
                }
            };

            // Apply alignment
            let item_outer_width = item_layout.size.width + resolved_margin.horizontal_axis_sum();
            if let Some(alignment) = item.justify_self {
                // Auto margins win even when there is no positive free space.
                if item_margin.left.is_some() && item_margin.right.is_some() {
                    let free = container_inner_width - item_outer_width;
                    let offset = if alignment.is_safe() && free < 0.0 {
                        0.0
                    } else {
                        match alignment.keyword() {
                            AlignItemsKeyword::End | AlignItemsKeyword::FlexEnd => free,
                            AlignItemsKeyword::Center => free / 2.0,
                            _ => 0.0,
                        }
                    };
                    location.x += if direction.is_rtl() { -offset } else { offset };
                }
            } else if item_outer_width < container_inner_width {
                let free_x_space = container_inner_width - item_outer_width;
                match (text_align, direction) {
                    (TextAlign::Auto, _) => {
                        // Do nothing
                    }
                    (TextAlign::LegacyLeft, Direction::Ltr) => {
                        // Do nothing. Left aligned by default.
                    }
                    (TextAlign::LegacyLeft, Direction::Rtl) => location.x -= free_x_space,
                    (TextAlign::LegacyRight, Direction::Ltr) => location.x += free_x_space,
                    (TextAlign::LegacyRight, Direction::Rtl) => {
                        // Do nothing. Right aligned by default.
                    }
                    (TextAlign::LegacyCenter, Direction::Ltr) => location.x += free_x_space / 2.0,
                    (TextAlign::LegacyCenter, Direction::Rtl) => location.x -= free_x_space / 2.0,
                }
            }

            // A block container's first baseline is the first baseline of its first in-flow child
            // that has one.
            //
            // Scroll containers' baselines are determined from their content as if scrolled to the
            // initial position, but are additionally clamped to their border box. A scroll container
            // with no baseline synthesizes one from its border-box bottom edge.
            // See https://github.com/w3c/csswg-drafts/issues/7660
            if first_baseline.is_none() {
                let child_baseline = if item.overflow.y.is_scroll_container() {
                    Some(
                        item_layout
                            .baselines
                            .first
                            .unwrap_or(item_layout.size.height)
                            .min(item_layout.size.height)
                            .max(0.0),
                    )
                } else {
                    item_layout.baselines.first
                };
                first_baseline = child_baseline.map(|baseline| location.y + baseline);
            }

            // Defer `set_unrounded_layout` to the post-loop pass in `compute_inner` so that
            // `align-content` can shift `location.y` before the layout is committed to the tree.
            item.relative_offset = inset_offset;
            item.final_layout = Some(Layout {
                order: item.order,
                size: item_layout.size,
                #[cfg(feature = "content_size")]
                scrollable_overflow_rect: item_layout.scrollable_overflow_rect,
                scrollbar_size,
                location,
                padding: item.padding,
                border: item.border,
                margin: resolved_margin,
            });

            #[cfg(feature = "content_size")]
            {
                let contribution_location = if direction.is_rtl() {
                    Point {
                        x: container_outer_width - (location.x + final_size.width) - resolved_border.right,
                        y: location.y - resolved_border.top,
                    }
                } else {
                    Point { x: location.x - resolved_border.left, y: location.y - resolved_border.top }
                };
                inflow_overflow_rect = inflow_overflow_rect.union(compute_scrollable_overflow_contribution(
                    contribution_location,
                    final_size,
                    item_layout.scrollable_overflow_rect,
                    item.overflow,
                    item.contain,
                    is_scroll_container,
                ));
            }

            // Update first_child_top_margin_set
            //
            // The top margin of an item with clearance does not collapse with the container's top margin,
            // so clearance terminates collapsing without contributing the item's own margins.
            #[cfg(feature = "float_layout")]
            if is_collapsing_with_first_margin_set && item_pushed_below_float {
                // The item's top margin "separated from the float" and must not
                // propagate to the parent
                is_collapsing_with_first_margin_set = false;
            }
            if is_collapsing_with_first_margin_set && has_clearance {
                is_collapsing_with_first_margin_set = false;
            } else if is_collapsing_with_first_margin_set {
                if item.can_be_collapsed_through {
                    first_child_top_margin_set = first_child_top_margin_set
                        .collapse_with_set(top_margin_set)
                        .collapse_with_set(bottom_margin_set);
                } else {
                    first_child_top_margin_set = first_child_top_margin_set.collapse_with_set(top_margin_set);
                    is_collapsing_with_first_margin_set = false;
                }
            }

            // Update active_collapsible_margin_set
            if item.can_be_collapsed_through {
                active_collapsible_margin_set = active_collapsible_margin_set
                    .collapse_with_set(top_margin_set)
                    .collapse_with_set(bottom_margin_set);
                y_offset_for_absolute = committed_y_offset + item_layout.size.height + y_margin_offset;
            } else {
                committed_y_offset = location.y - inset_offset.y + item_layout.size.height;
                // A self-collapsing item with clearance is not collapsed through (its margins do not collapse
                // with margins of preceding siblings), but its top and bottom margins still collapse with each
                // other and with the margins of following siblings.
                if has_clearance && item_layout.margins_can_collapse_through {
                    // The element's border edge stays at the cleared position, but its collapsed margin
                    // extends below it: the border edge sits `top margin` inside the collapsed margin, so
                    // following content is offset by `collapsed margin - top margin` from the border edge.
                    committed_y_offset -= top_margin_set.resolve();
                    active_collapsible_margin_set = top_margin_set.collapse_with_set(bottom_margin_set);
                    active_margin_set_has_clearance = true;
                } else {
                    active_collapsible_margin_set = bottom_margin_set;
                    active_margin_set_has_clearance = false;
                }
                y_offset_for_absolute = committed_y_offset + active_collapsible_margin_set.resolve();
                // Committing in-flow content resolves the position of the current margin-collapse strut,
                // so floats placed before this point no longer force clearance
                #[cfg(feature = "float_layout")]
                block_ctx.commit_strut();
            }
        }
    }

    // The margins of a self-collapsing element with clearance do not collapse with the bottom
    // margin of the parent block: they extend the parent's content height instead of escaping it
    let last_child_bottom_margin_set =
        if active_margin_set_has_clearance { CollapsibleMarginSet::ZERO } else { active_collapsible_margin_set };
    let bottom_y_margin_offset = if active_margin_set_has_clearance {
        active_collapsible_margin_set.resolve()
    } else if own_margins_collapse_with_children.end {
        0.0
    } else {
        last_child_bottom_margin_set.resolve()
    };

    committed_y_offset += resolved_content_box_inset.bottom + bottom_y_margin_offset;
    let content_height = f32_max(0.0, committed_y_offset);
    (inflow_overflow_rect, content_height, first_child_top_margin_set, last_child_bottom_margin_set, first_baseline)
}
