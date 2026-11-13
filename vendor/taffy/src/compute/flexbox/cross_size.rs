//! Cross sizes and alignment of flex items.

use super::*;

/// Determine the hypothetical cross size of each item.
///
/// # [9.4. Cross Size Determination](https://www.w3.org/TR/css-flexbox-1/#cross-sizing)
///
/// - [**Determine the hypothetical cross size of each item**](https://www.w3.org/TR/css-flexbox-1/#algo-cross-item)
///   by performing layout with the used main size and the available space, treating auto as fit-content.
#[inline]
pub(super) fn determine_hypothetical_cross_size(
    tree: &mut impl LayoutFlexboxContainer,
    line: &mut FlexLine,
    constants: &AlgoConstants,
    available_space: Size<AvailableSpace>,
) {
    for child in line.items.iter_mut() {
        // EXACT PATCH 54: transfer the used height (Flexbox §9.4). The
        // content probe keeps height bounds when the preferred height is auto.
        if constants.is_column {
            if let Some(width) = crate::compute::ratio::minimum_ratio_width(
                tree, child.node, constants.node_inner_size, Some(child.target_size.height),
            ) {
                child.min_size.width = Some(width).maybe_max(child.min_size.width);
            }
        }
        let padding_border_sum = (child.padding + child.border).cross_axis_sum(constants.dir);

        let child_known_main = constants.container_size.main(constants.dir).into();

        // Sizes transferred through the aspect ratio clamp the hypothetical cross size
        // https://github.com/w3c/csswg-drafts/issues/10997
        // EXACT PATCH 12: never into a dimension the item sizes itself.
        let sized = child.size_style.map(|d| !d.is_auto());
        let ratio = Ratio::of(&tree.get_flexbox_child_style(child.node), (child.padding + child.border).sum_axes());
        let transferred_min_cross =
            transfer_into_unsized(child.min_size, ratio, sized).cross(constants.dir);
        let transferred_max_cross =
            transfer_into_unsized(child.max_size, ratio, sized).cross(constants.dir);

        // EXACT PATCH 65: a single-line row's definite stretch also applies
        // during measurement (Flexbox §9.8). Overflow inside that item must
        // not enlarge the container's content-based ratio height minimum.
        let child_cross = if constants.is_row && !constants.is_wrap && constants.has_definite_cross_size
            && constants.node_outer_size.height.is_none()
            && child.align_self == AlignSelf::STRETCH && child.size_style.height.is_auto()
            && !child.margin_is_auto.top && !child.margin_is_auto.bottom
        {
            constants.node_inner_size.height
                .maybe_sub(child.margin.vertical_axis_sum())
                .maybe_clamp(child.min_size.height, child.max_size.height)
        } else if !constants.is_row && !constants.is_wrap && constants.node_outer_size.width.is_some()
            && child.align_self == AlignSelf::STRETCH && child.size_style.width.is_auto()
            && !child.margin_is_auto.left && !child.margin_is_auto.right
        {
            // EXACT PATCH 68: a single-line column of known width makes its
            // line that width (§9.4 step 8) and stretches this item to it
            // (step 11), which reads no hypothetical width. Measuring the item's
            // fit-content width would ask every box in it for its intrinsic
            // widths: a list's rows, all of them again when a row is inserted.
            constants.node_inner_size.width
                .maybe_sub(child.margin.horizontal_axis_sum())
                .maybe_clamp(child.min_size.width, child.max_size.width)
        } else {
            child.size.cross(constants.dir).maybe_clamp(transferred_min_cross, transferred_max_cross)
        }.maybe_max(padding_border_sum);

        let child_available_cross = available_space
            .cross(constants.dir)
            .map_definite_value(|val| constants.divided_cross_space(val))
            .maybe_sub(child.margin.cross_axis_sum(constants.dir))
            .maybe_clamp(transferred_min_cross, transferred_max_cross)
            .maybe_max(padding_border_sum);

        // A cross size that is a sizing keyword (min-content, max-content, fit-content,
        // fit-content(...)) determines the available space constraint the item is measured under.
        // The `stretch` keyword is not resolved here: it stretches to the flex line, which is
        // handled in `determine_used_cross_size`
        let cross_stretch_size = constants
            .node_inner_size
            .cross(constants.dir)
            .map(|val| constants.divided_cross_space(val))
            .maybe_sub(child.margin.cross_axis_sum(constants.dir));
        let child_available_cross = match resolve_sizing_keyword(
            child.size_style.cross(constants.dir),
            cross_stretch_size,
            constants.node_inner_size.cross(constants.dir),
        ) {
            Some(SizingKeywordResolution::Measure(available)) => available,
            _ => child_available_cross,
        };

        let child_cross = child_cross.or_else(|| {
            if !constants.is_row && child.size_style.width.is_auto() && child.aspect_ratio.is_none()
                && !tree.get_flexbox_child_style(child.node).is_compressible_replaced() {
                child_available_cross.into_option().map(|available_width| {
                    crate::compute::common::fit_content_width(
                        tree, child.node, Size { width: None, height: Some(child.target_size.height) },
                        item_known_dimension_definiteness(constants, child),
                        constants.node_inner_size, available_width, SizingMode::ContentSize,
                    ).maybe_clamp(transferred_min_cross, transferred_max_cross).max(padding_border_sum)
                })
            } else { None }
        });
        let child_inner_cross = child_cross.unwrap_or_else(|| {
            tree.compute_child_layout(
                child.node,
                LayoutInput {
                    run_mode: RunMode::ComputeSize,
                    sizing_mode: SizingMode::ContentSize,
                    axis: constants.dir.cross_axis().into(),
                    known_dimensions: Size {
                        width: if constants.is_row { child.target_size.width.into() } else { child_cross },
                        height: if constants.is_row { child_cross } else { child.target_size.height.into() },
                    },
                    known_dimensions_are_definite: item_known_dimension_definiteness(constants, child),
                    parent_size: constants.node_inner_size,
                    available_space: Size {
                        width: if constants.is_row { child_known_main } else { child_available_cross },
                        height: if constants.is_row { child_available_cross } else { child_known_main },
                    },
                    vertical_margins_are_collapsible: Line::FALSE,
                },
            )
            .size
            .get_abs(constants.dir.cross_axis())
            .maybe_clamp(transferred_min_cross, transferred_max_cross)
            .max(padding_border_sum)
        });
        let child_outer_cross = child_inner_cross + child.margin.cross_axis_sum(constants.dir);

        child.hypothetical_inner_size.set_cross(constants.dir, child_inner_cross);
        child.hypothetical_outer_size.set_cross(constants.dir, child_outer_cross);
    }
}

/// Calculate the base lines of the children.
#[inline]
pub(super) fn calculate_children_base_lines(
    tree: &mut impl LayoutFlexboxContainer,
    node_size: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    flex_lines: &mut [FlexLine],
    constants: &AlgoConstants,
) -> bool {
    // Only compute baselines for flex rows because we only support baseline alignment in the cross axis
    // where that axis is also the inline axis
    // TODO: this may need revisiting if/when we support vertical writing modes
    if !constants.is_row {
        return false;
    }

    let mut laid_out_children = false;

    for line in flex_lines {
        // If a flex line has one or zero items participating in baseline alignment then baseline alignment is a no-op so we skip
        let line_baseline_child_count =
            line.items.iter().filter(|child| child.participates_in_baseline_alignment(constants.dir)).count();
        if line_baseline_child_count <= 1 {
            continue;
        }

        for child in line.items.iter_mut() {
            // Only calculate baselines for children participating in baseline alignment
            if !child.participates_in_baseline_alignment(constants.dir) {
                continue;
            }

            laid_out_children = true;
            let measured_size_and_baselines = tree.compute_child_layout(
                child.node,
                LayoutInput {
                    run_mode: RunMode::PerformLayout,
                    sizing_mode: SizingMode::ContentSize,
                    axis: RequestedAxis::Both,
                    known_dimensions: Size {
                        width: if constants.is_row {
                            child.target_size.width.into()
                        } else {
                            child.hypothetical_inner_size.width.into()
                        },
                        height: if constants.is_row {
                            child.hypothetical_inner_size.height.into()
                        } else {
                            child.target_size.height.into()
                        },
                    },
                    known_dimensions_are_definite: item_known_dimension_definiteness(constants, child),
                    parent_size: constants.node_inner_size,
                    available_space: Size {
                        width: if constants.is_row {
                            constants.container_size.width.into()
                        } else {
                            available_space.width.maybe_set(node_size.width)
                        },
                        height: if constants.is_row {
                            available_space.height.maybe_set(node_size.height)
                        } else {
                            constants.container_size.height.into()
                        },
                    },
                    vertical_margins_are_collapsible: Line::FALSE,
                },
            );

            let baseline = measured_size_and_baselines.baselines.first;
            let height = measured_size_and_baselines.size.height;

            // Scroll containers' baselines are determined from their content as if scrolled to the
            // initial position, but are additionally clamped to their border box.
            // See https://github.com/w3c/csswg-drafts/issues/7660
            let baseline = if child.overflow.y.is_scroll_container() {
                baseline.unwrap_or(height).min(height).max(0.0)
            } else {
                baseline.unwrap_or(height)
            };

            child.baseline = baseline + child.margin.top;
        }
    }
    laid_out_children
}

/// Calculate the cross size of each flex line.
///
/// # [9.4. Cross Size Determination](https://www.w3.org/TR/css-flexbox-1/#cross-sizing)
///
/// - [**Calculate the cross size of each flex line**](https://www.w3.org/TR/css-flexbox-1/#algo-cross-line).
#[inline]
pub(super) fn calculate_cross_size(flex_lines: &mut [FlexLine], node_size: Size<Option<f32>>, constants: &AlgoConstants) {
    // If the flex container is single-line and has a definite cross size,
    // the cross size of the flex line is the flex container’s inner cross size.
    if !constants.is_wrap && node_size.cross(constants.dir).is_some() {
        let cross_axis_padding_border = constants.content_box_inset.cross_axis_sum(constants.dir);
        let cross_min_size = constants.min_size.cross(constants.dir);
        let cross_max_size = constants.max_size.cross(constants.dir);
        flex_lines[0].cross_size = node_size
            .cross(constants.dir)
            .maybe_clamp(cross_min_size, cross_max_size)
            .maybe_sub(cross_axis_padding_border)
            .maybe_max(0.0)
            .unwrap_or(0.0);
    } else {
        // Otherwise, for each flex line:
        //
        //    1. Collect all the flex items whose inline-axis is parallel to the main-axis, whose
        //       align-self is baseline, and whose cross-axis margins are both non-auto. Find the
        //       largest of the distances between each item’s baseline and its hypothetical outer
        //       cross-start edge, and the largest of the distances between each item’s baseline
        //       and its hypothetical outer cross-end edge, and sum these two values.

        //    2. Among all the items not collected by the previous step, find the largest
        //       outer hypothetical cross size.

        //    3. The used cross-size of the flex line is the largest of the numbers found in the
        //       previous two steps and zero.
        for line in flex_lines.iter_mut() {
            let max_baseline: f32 = line.items.iter().map(|child| child.baseline).fold(0.0, |acc, x| acc.max(x));
            line.cross_size = line
                .items
                .iter()
                .map(|child| {
                    if child.participates_in_baseline_alignment(constants.dir) {
                        max_baseline - child.baseline + child.hypothetical_outer_size.cross(constants.dir)
                    } else {
                        child.hypothetical_outer_size.cross(constants.dir)
                    }
                })
                .fold(0.0, |acc, x| acc.max(x));
        }

        // If the flex container is single-line, then clamp the line’s cross-size to be within the container’s computed min and max cross sizes.
        // Note that if CSS 2.1’s definition of min/max-width/height applied more generally, this behavior would fall out automatically.
        if !constants.is_wrap {
            let cross_axis_padding_border = constants.content_box_inset.cross_axis_sum(constants.dir);
            let cross_min_size = constants.min_size.cross(constants.dir);
            let cross_max_size = constants.max_size.cross(constants.dir);
            flex_lines[0].cross_size = flex_lines[0].cross_size.maybe_clamp(
                cross_min_size.maybe_sub(cross_axis_padding_border),
                cross_max_size.maybe_sub(cross_axis_padding_border),
            );
        }
    }
}

/// Handle 'align-content: stretch'.
///
/// # [9.4. Cross Size Determination](https://www.w3.org/TR/css-flexbox-1/#cross-sizing)
///
/// - [**Handle 'align-content: stretch'**](https://www.w3.org/TR/css-flexbox-1/#algo-line-stretch). If the flex container has a definite cross size, align-content is stretch,
///   and the sum of the flex lines' cross sizes is less than the flex container’s inner cross size,
///   increase the cross size of each flex line by equal amounts such that the sum of their cross sizes exactly equals the flex container’s inner cross size.
#[inline]
pub(super) fn handle_align_content_stretch(flex_lines: &mut [FlexLine], node_size: Size<Option<f32>>, constants: &AlgoConstants) {
    if constants.align_content == AlignContent::STRETCH {
        let cross_axis_padding_border = constants.content_box_inset.cross_axis_sum(constants.dir);
        let cross_min_size = constants.min_size.cross(constants.dir);
        let cross_max_size = constants.max_size.cross(constants.dir);
        let container_min_inner_cross = node_size
            .cross(constants.dir)
            .or(cross_min_size)
            .maybe_clamp(cross_min_size, cross_max_size)
            .maybe_sub(cross_axis_padding_border)
            .maybe_max(0.0)
            .unwrap_or(0.0);

        let total_cross_axis_gap = sum_axis_gaps(constants.gap.cross(constants.dir), flex_lines.len());
        let lines_total_cross: f32 = flex_lines.iter().map(|line| line.cross_size).sum::<f32>() + total_cross_axis_gap;

        if lines_total_cross < container_min_inner_cross {
            let remaining = container_min_inner_cross - lines_total_cross;
            let addition = remaining / flex_lines.len() as f32;
            flex_lines.iter_mut().for_each(|line| line.cross_size += addition);
        }
    }
}

/// Determine the used cross size of each flex item.
///
/// # [9.4. Cross Size Determination](https://www.w3.org/TR/css-flexbox-1/#cross-sizing)
///
/// - [**Determine the used cross size of each flex item**](https://www.w3.org/TR/css-flexbox-1/#algo-stretch). If a flex item has align-self: stretch, its computed cross size property is auto,
///   and neither of its cross-axis margins are auto, the used outer cross size is the used cross size of its flex line, clamped according to the item’s used min and max cross sizes.
///   Otherwise, the used cross size is the item’s hypothetical cross size.
///
///   If the flex item has align-self: stretch, redo layout for its contents, treating this used size as its definite cross size so that percentage-sized children can be resolved.
///
///   **Note that this step does not affect the main size of the flex item, even if it has an intrinsic aspect ratio**.
#[inline]
pub(super) fn determine_used_cross_size(
    tree: &impl LayoutFlexboxContainer,
    flex_lines: &mut [FlexLine],
    constants: &AlgoConstants,
) {
    for line in flex_lines {
        let line_cross_size = line.cross_size;

        for child in line.items.iter_mut() {
            let child_style = tree.get_flexbox_child_style(child.node);
            // A cross size of `stretch` stretches to the flex line like align-self: stretch
            // (but regardless of the alignment style)
            let cross_is_stretch = child.size_style.cross(constants.dir).is_stretch();
            child.target_size.set_cross(
                constants.dir,
                if !child.margin_is_auto.cross_start(constants.dir)
                    && !child.margin_is_auto.cross_end(constants.dir)
                    && (cross_is_stretch
                        || (child.align_self == AlignSelf::STRETCH
                            && child_style.size().cross(constants.dir).is_auto()))
                {
                    // For some reason this particular usage of max_width is an exception to the rule that max_width's transfer
                    // using the aspect_ratio (if set). Both Chrome and Firefox agree on this. And reading the spec, it seems like
                    // a reasonable interpretation. Although it seems to me that the spec *should* apply aspect_ratio here.
                    // EXACT PATCH 10: the inline size is the basis on every side.
                    let padding = child_style
                        .padding()
                        .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis));
                    let border = child_style
                        .border()
                        .resolve_or_zero(constants.node_inner_size.width, |val, basis| tree.calc(val, basis));
                    let pb_sum = (padding + border).sum_axes();
                    let box_sizing_adjustment =
                        if child_style.box_sizing() == BoxSizing::ContentBox { pb_sum } else { Size::ZERO };

                    let max_size_ignoring_aspect_ratio = child_style
                        .max_size()
                        .maybe_resolve(constants.node_inner_size, |val, basis| tree.calc(val, basis))
                        .maybe_add(box_sizing_adjustment);

                    // EXACT PATCH 50: keep the ratio's automatic inline minimum
                    // for intrinsic measurement, but a column's final stretched
                    // width only obeys authored width bounds (Flexbox §9.4).
                    let min_cross_size = if constants.is_column && child_style.min_size().width.is_auto() {
                        None
                    } else {
                        child.min_size.cross(constants.dir)
                    };
                    // EXACT PATCH 55: alignment needs the used border-box size,
                    // including its padding/border floor even when max-size is
                    // smaller. Deferring this floor to child layout loses overflow.
                    let min_cross_size = Some(min_cross_size.unwrap_or(0.0).max(pb_sum.cross(constants.dir)));
                    let cross_size = (line_cross_size - child.margin.cross_axis_sum(constants.dir)).maybe_clamp(
                        min_cross_size,
                        max_size_ignoring_aspect_ratio.cross(constants.dir),
                    );
                    // EXACT PATCH 49: compare before reversing inset and margin
                    // additions when the item's measured outer size fills the
                    // line. Otherwise that round trip can cross a layout-unit
                    // boundary. Item bounds still apply to the comparison.
                    let hypothetical_cross = child.hypothetical_inner_size.cross(constants.dir);
                    let inset = constants.content_box_inset.cross_axis_sum(constants.dir);
                    let comparison_cross = if line_cross_size + inset
                        == child.hypothetical_outer_size.cross(constants.dir) + inset
                    {
                        hypothetical_cross.maybe_clamp(
                            min_cross_size,
                            max_size_ignoring_aspect_ratio.cross(constants.dir),
                        )
                    } else {
                        cross_size
                    };
                    // Blink's LayoutUnit(float) truncates each positive size to
                    // 1/64px. Rounding their difference misclassifies bounds
                    // that resolve to the same unit as the natural height.
                    child.cross_size_was_stretched =
                        floor(comparison_cross * 64.0) != floor(hypothetical_cross * 64.0);
                    cross_size
                } else {
                    child.hypothetical_inner_size.cross(constants.dir)
                },
            );

            child.outer_target_size.set_cross(
                constants.dir,
                child.target_size.cross(constants.dir) + child.margin.cross_axis_sum(constants.dir),
            );
        }
    }
}

/// Distribute any remaining free space.
///
/// # [9.5. Main-Axis Alignment](https://www.w3.org/TR/css-flexbox-1/#main-alignment)
///
/// - [**Distribute any remaining free space**](https://www.w3.org/TR/css-flexbox-1/#algo-main-align). For each flex line:
///
///   1. If the remaining free space is positive and at least one main-axis margin on this line is `auto`, distribute the free space equally among these margins.
///      Otherwise, set all `auto` margins to zero.
///
///   2. Align the items along the main-axis per `justify-content`.
#[inline]
pub(super) fn distribute_remaining_free_space(flex_lines: &mut [FlexLine], constants: &AlgoConstants) {
    for line in flex_lines {
        let total_main_axis_gap = sum_axis_gaps(constants.gap.main(constants.dir), line.items.len());
        let used_space: f32 = total_main_axis_gap
            + line.items.iter().map(|child| child.outer_target_size.main(constants.dir)).sum::<f32>();
        let mut free_space = constants.inner_container_size.main(constants.dir) - used_space;
        let mut num_auto_margins = 0;

        for child in line.items.iter_mut() {
            if child.margin_is_auto.main_start(constants.dir) {
                num_auto_margins += 1;
            }
            if child.margin_is_auto.main_end(constants.dir) {
                num_auto_margins += 1;
            }
        }

        if free_space > 0.0 && num_auto_margins > 0 {
            let margin = free_space / num_auto_margins as f32;

            for child in line.items.iter_mut() {
                if child.margin_is_auto.main_start(constants.dir) {
                    if constants.is_row {
                        child.margin.left = margin;
                    } else {
                        child.margin.top = margin;
                    }
                }
                if child.margin_is_auto.main_end(constants.dir) {
                    if constants.is_row {
                        child.margin.right = margin;
                    } else {
                        child.margin.bottom = margin;
                    }
                }
            }

            // The auto margins have absorbed all of the free space, leaving none for `justify-content`
            free_space = 0.0;
        }

        let num_items = line.items.len();
        let layout_reverse = constants.dir.is_reverse();
        let gap = constants.gap.main(constants.dir);
        let raw_justify_content_mode = constants.justify_content.unwrap_or(JustifyContent::FLEX_START);
        let justify_content_mode = apply_alignment_fallback(free_space, num_items, raw_justify_content_mode);

        let justify_item = |(i, child): (usize, &mut FlexItem)| {
            child.offset_main =
                compute_alignment_offset(free_space, num_items, gap, justify_content_mode, layout_reverse, i == 0);
        };

        if layout_reverse {
            line.items.iter_mut().rev().enumerate().for_each(justify_item);
        } else {
            line.items.iter_mut().enumerate().for_each(justify_item);
        }
    }
}

/// Resolve cross-axis `auto` margins.
///
/// # [9.6. Cross-Axis Alignment](https://www.w3.org/TR/css-flexbox-1/#cross-alignment)
///
/// - [**Resolve cross-axis `auto` margins**](https://www.w3.org/TR/css-flexbox-1/#algo-cross-margins).
///   If a flex item has auto cross-axis margins:
///
///   - If its outer cross size (treating those auto margins as zero) is less than the cross size of its flex line,
///     distribute the difference in those sizes equally to the auto margins.
///
///   - Otherwise, if the block-start or inline-start margin (whichever is in the cross axis) is auto, set it to zero.
///     Set the opposite margin so that the outer cross size of the item equals the cross size of its flex line.
#[inline]
pub(super) fn resolve_cross_axis_auto_margins(flex_lines: &mut [FlexLine], constants: &AlgoConstants) {
    for line in flex_lines {
        let line_cross_size = line.cross_size;
        let max_baseline: f32 = line.items.iter_mut().map(|child| child.baseline).fold(0.0, |acc, x| acc.max(x));
        let max_baseline_to_bottom_distance: f32 = line
            .items
            .iter_mut()
            .filter(|child| child.participates_in_baseline_alignment(constants.dir))
            .map(|child| child.outer_target_size.cross(constants.dir) - child.baseline)
            .fold(0.0, |acc, x| acc.max(x));

        for child in line.items.iter_mut() {
            let free_space = line_cross_size - child.outer_target_size.cross(constants.dir);
            let auto_start = child.margin_is_auto.cross_start(constants.dir);
            let auto_end = child.margin_is_auto.cross_end(constants.dir);

            // EXACT PATCH 52: auto margins consume only positive space. Chrome places
            // overflowing items at inline/block start; in an RTL column that is the
            // right edge, independently of which margin is auto.
            let margin_space = free_space.max(0.0);
            if (auto_start || auto_end) && constants.is_column && constants.layout_direction.is_rtl() {
                child.offset_cross = free_space.min(0.0);
            }

            if auto_start && auto_end {
                if constants.is_row {
                    child.margin.top = margin_space / 2.0;
                    child.margin.bottom = margin_space / 2.0;
                } else {
                    child.margin.left = margin_space / 2.0;
                    child.margin.right = margin_space / 2.0;
                }
            } else if auto_start {
                if constants.is_row {
                    child.margin.top = margin_space;
                } else {
                    child.margin.left = margin_space;
                }
            } else if auto_end {
                if constants.is_row {
                    child.margin.bottom = margin_space;
                } else {
                    child.margin.right = margin_space;
                }
            } else {
                // 14. Align all flex items along the cross-axis.
                child.offset_cross = align_flex_items_along_cross_axis(
                    child,
                    free_space,
                    max_baseline,
                    max_baseline_to_bottom_distance,
                    constants,
                );
            }
        }
    }
}

/// Align all flex items along the cross-axis.
///
/// # [9.6. Cross-Axis Alignment](https://www.w3.org/TR/css-flexbox-1/#cross-alignment)
///
/// - [**Align all flex items along the cross-axis**](https://www.w3.org/TR/css-flexbox-1/#algo-cross-align) per `align-self`,
///   if neither of the item's cross-axis margins are `auto`.
#[inline]
pub(super) fn align_flex_items_along_cross_axis(
    child: &FlexItem,
    free_space: f32,
    max_baseline: f32,
    max_baseline_to_bottom_distance: f32,
    constants: &AlgoConstants,
) -> f32 {
    let cross_axis_should_reverse = constants.is_column && matches!(constants.layout_direction, Direction::Rtl);

    // If align-self uses a "safe" overflow-position keyword and the item would overflow its
    // line cross size, fall back to logical Start to avoid data loss. See CSS Box Alignment 3
    // §4.3 <https://www.w3.org/TR/css-align-3/#overflow-values>. Otherwise, drop the safety
    // field so the match below operates on a bare keyword and stays exhaustive.
    let align_keyword = if child.align_self.is_safe() && free_space < 0.0 {
        AlignItemsKeyword::Start
    } else {
        child.align_self.keyword
    };

    match align_keyword {
        AlignItemsKeyword::Start => {
            if cross_axis_should_reverse {
                free_space
            } else {
                0.0
            }
        }
        AlignItemsKeyword::FlexStart => {
            if constants.is_wrap_reverse ^ cross_axis_should_reverse {
                free_space
            } else {
                0.0
            }
        }
        AlignItemsKeyword::End => {
            if cross_axis_should_reverse {
                0.0
            } else {
                free_space
            }
        }
        AlignItemsKeyword::FlexEnd => {
            if constants.is_wrap_reverse ^ cross_axis_should_reverse {
                0.0
            } else {
                free_space
            }
        }
        AlignItemsKeyword::Center => free_space / 2.0,
        AlignItemsKeyword::Baseline => {
            if constants.is_row {
                if constants.is_wrap_reverse {
                    // In a wrap-reverse container the cross axis is flipped, so the baseline-aligned
                    // group of items is aligned to the cross-start edge, which is the bottom of the line.
                    let line_cross_size = free_space + child.outer_target_size.cross(constants.dir);
                    line_cross_size - max_baseline_to_bottom_distance - child.baseline
                } else {
                    max_baseline - child.baseline
                }
            } else {
                // Until we support vertical writing modes, baseline alignment only makes sense if
                // the constants.direction is row, so we treat it as flex-start alignment in columns.
                let baseline_column_should_reverse = cross_axis_should_reverse && !constants.is_wrap;
                if constants.is_wrap_reverse ^ baseline_column_should_reverse {
                    free_space
                } else {
                    0.0
                }
            }
        }
        AlignItemsKeyword::Stretch => {
            if constants.is_wrap_reverse ^ cross_axis_should_reverse {
                free_space
            } else {
                0.0
            }
        }
        // SelfStart/SelfEnd are resolved to Start/End against the item's own direction when
        // flex items are generated.
        AlignItemsKeyword::SelfStart | AlignItemsKeyword::SelfEnd
            | AlignItemsKeyword::Left | AlignItemsKeyword::Right => unreachable!(),
    }
}

/// Determine the flex container’s used cross size.
///
/// # [9.6. Cross-Axis Alignment](https://www.w3.org/TR/css-flexbox-1/#cross-alignment)
///
/// - [**Determine the flex container’s used cross size**](https://www.w3.org/TR/css-flexbox-1/#algo-cross-container):
///
///     - If the cross size property is a definite size, use that, clamped by the used min and max cross sizes of the flex container.
///
///     - Otherwise, use the sum of the flex lines' cross sizes, clamped by the used min and max cross sizes of the flex container.
#[inline]
#[must_use]
pub(super) fn determine_container_cross_size(
    flex_lines: &[FlexLine],
    node_size: Size<Option<f32>>,
    constants: &mut AlgoConstants,
) -> f32 {
    let total_cross_axis_gap = sum_axis_gaps(constants.gap.cross(constants.dir), flex_lines.len());
    let total_line_cross_size: f32 = flex_lines.iter().map(|line| line.cross_size).sum::<f32>();

    let padding_border_sum = constants.content_box_inset.cross_axis_sum(constants.dir);
    let cross_scrollbar_gutter = constants.scrollbar_gutter.cross(constants.dir);
    let min_cross_size = constants.min_size.cross(constants.dir);
    let max_cross_size = constants.max_size.cross(constants.dir);
    let outer_container_size = node_size
        .cross(constants.dir)
        .unwrap_or(total_line_cross_size + total_cross_axis_gap + padding_border_sum)
        .maybe_clamp(min_cross_size, max_cross_size)
        .max(padding_border_sum - cross_scrollbar_gutter);
    let inner_container_size = f32_max(outer_container_size - padding_border_sum, 0.0);

    constants.container_size.set_cross(constants.dir, outer_container_size);
    constants.inner_container_size.set_cross(constants.dir, inner_container_size);

    total_line_cross_size
}
