//! Flex bases, line breaking and flexible main sizes.

use super::*;

/// Determine the flex base size and hypothetical main size of each item.
///
/// # [9.2. Line Length Determination](https://www.w3.org/TR/css-flexbox-1/#line-sizing)
///
/// - [**Determine the flex base size and hypothetical main size of each item:**](https://www.w3.org/TR/css-flexbox-1/#algo-main-item)
///
///     - A. If the item has a definite used flex basis, that’s the flex base size.
///
///     - B. If the flex item has ...
///
///         - an intrinsic aspect ratio,
///         - a used flex basis of content, and
///         - a definite cross size,
///
///       then the flex base size is calculated from its inner cross size and the flex item’s intrinsic aspect ratio.
///
///     - C. If the used flex basis is content or depends on its available space, and the flex container is being sized under a min-content
///       or max-content constraint (e.g. when performing automatic table layout \[CSS21\]), size the item under that constraint.
///       The flex base size is the item’s resulting main size.
///
///     - E. Otherwise, size the item into the available space using its used flex basis in place of its main size, treating a value of content as max-content.
///       If a cross size is needed to determine the main size (e.g. when the flex item’s main size is in its block axis) and the flex item’s cross size is auto and not definite,
///       in this calculation use fit-content as the flex item’s cross size. The flex base size is the item’s resulting main size.
///
///   When determining the flex base size, the item’s min and max main sizes are ignored (no clamping occurs).
///   Furthermore, the sizing calculations that floor the content box size at zero when applying box-sizing are also ignored.
///   (For example, an item with a specified size of zero, positive padding, and box-sizing: border-box will have an outer flex base size of zero—and hence a negative inner flex base size.)
#[inline]
pub(super) fn determine_flex_base_size(
    tree: &mut impl LayoutFlexboxContainer,
    constants: &AlgoConstants,
    available_space: Size<AvailableSpace>,
    flex_items: &mut [FlexItem],
) {
    let dir = constants.dir;

    for child in flex_items.iter_mut() {
        let child_style = tree.get_flexbox_child_style(child.node);
        let ratio = Ratio::of(&child_style, (child.padding + child.border).sum_axes());
        let replaced_ratio = ratio.filter(|_| child_style.is_compressible_replaced());

        // Parent size for child sizing
        let cross_axis_parent_size = constants.node_inner_size.cross(dir);
        let child_parent_size = Size::from_cross(dir, cross_axis_parent_size);

        // Available space for child sizing
        // Min/max sizes transferred through the aspect ratio are taken into account here
        // https://github.com/w3c/csswg-drafts/issues/10997
        let cross_axis_margin_sum = child.margin.cross_axis_sum(dir);
        // EXACT PATCH 12: never into a dimension the item sizes itself.
        let sized = child.size_style.map(|d| !d.is_auto());
        let transferred_min_size = transfer_into_unsized(child.min_size, ratio, sized);
        let transferred_max_size = transfer_into_unsized(child.max_size, ratio, sized);
        let child_min_cross = transferred_min_size.cross(dir).maybe_add(cross_axis_margin_sum);
        let child_max_cross = transferred_max_size.cross(dir).maybe_add(cross_axis_margin_sum);

        // Clamp available space by min- and max- size
        let cross_axis_available_space: AvailableSpace = match available_space.cross(dir) {
            AvailableSpace::Definite(val) => AvailableSpace::Definite(
                constants
                    .divided_cross_space(cross_axis_parent_size.unwrap_or(val))
                    .maybe_clamp(child_min_cross, child_max_cross),
            ),
            AvailableSpace::MinContent => match child_min_cross {
                Some(min) => AvailableSpace::Definite(min),
                None => AvailableSpace::MinContent,
            },
            AvailableSpace::MaxContent => match child_max_cross {
                Some(max) => AvailableSpace::Definite(max),
                None => AvailableSpace::MaxContent,
            },
        };

        // Known dimensions for child sizing
        let mut child_cross_size_is_definite = child.size.cross(dir).is_some();
        let child_known_dimensions = {
            let mut ckd = child.size.with_main(dir, None);
            // Clamp the definite cross size by the cross min/max sizes so that sizes
            // transferred through an intrinsic aspect ratio (e.g. for replaced elements)
            // are based on the used cross size.
            ckd.set_cross(
                dir,
                ckd.cross(dir).maybe_clamp(transferred_min_size.cross(dir), transferred_max_size.cross(dir)),
            );
            if child.align_self == AlignSelf::STRETCH
                && !child.margin_is_auto.cross_start(constants.dir)
                && !child.margin_is_auto.cross_end(constants.dir)
                && ckd.cross(dir).is_none()
            {
                ckd.set_cross(
                    dir,
                    cross_axis_available_space.into_option().maybe_sub(child.margin.cross_axis_sum(dir)),
                );
                // The cross size of a stretched item is definite if the container has a definite
                // cross size (https://www.w3.org/TR/css-flexbox-1/#definite-sizes)
                child_cross_size_is_definite =
                    !constants.is_wrap && constants.has_definite_cross_size && cross_axis_parent_size.is_some();
            }
            ckd
        };
        // EXACT PATCH 12: a stretched cross size that is not definite is not a
        // size to transfer through the item's ratio (CSS Box Sizing 4 §5.1):
        // the item is measured, and its automatic minimum found, without it.
        let provisional_cross_removed = if child.aspect_ratio.is_some() && !child_cross_size_is_definite {
            child_known_dimensions.with_cross(dir, child.size.cross(dir))
        } else {
            child_known_dimensions
        };

        let container_width = constants.node_inner_size.main(dir);
        let box_sizing_adjustment = if child_style.box_sizing() == BoxSizing::ContentBox {
            let padding = child_style.padding().resolve_or_zero(container_width, |val, basis| tree.calc(val, basis));
            let border = child_style.border().resolve_or_zero(container_width, |val, basis| tree.calc(val, basis));
            (padding + border).sum_axes()
        } else {
            Size::ZERO
        }
        .main(dir);
        // Percentage flex basis values resolve against the container's inner main size, but only
        // if that size is definite. A known main size which is derived from the container's own
        // content is treated as indefinite here.
        let percent_resolution_main_size =
            if constants.known_main_size_is_definite { constants.node_inner_size.main(dir) } else { None };
        let flex_basis_style = child_style.flex_basis();
        let flex_basis = flex_basis_style
            .maybe_resolve(percent_resolution_main_size, |val, basis| tree.calc(val, basis))
            .maybe_add(box_sizing_adjustment);
        // EXACT PATCH 51: a ratio-derived content basis is definite for layout,
        // but does not clamp the item's intrinsic contribution as a definite used basis does.
        child.used_flex_basis_is_definite = flex_basis.is_some()
            || (flex_basis_style.is_auto() && child.size_style.main(dir)
                .maybe_resolve(percent_resolution_main_size, |val, basis| tree.calc(val, basis)).is_some());

        drop(child_style);

        // EXACT PATCH 28: CSS Flexbox §9.2 3E measures an indefinite auto
        // cross size at fit-content before determining a column item's main
        // size. An available width alone can return a wrapped line's width
        // rather than the item's fit-content width, shrinking a ratio leaf's
        // derived height. A sizing keyword keeps its own constraint.
        // Resolve the width only when a probe below needs it.
        let fit_content_available = match cross_axis_available_space.maybe_sub(cross_axis_margin_sum) {
            AvailableSpace::Definite(available)
                if !dir.is_row()
                    && provisional_cross_removed.cross(dir).is_none()
                    && child.size_style.cross(dir).is_auto()
                    && !tree.get_flexbox_child_style(child.node).is_compressible_replaced() =>
            {
                Some(available)
            }
            _ => None,
        };
        let mut fit_content_known: Option<Size<Option<f32>>> = None;
        let fit_content_floor = (child.padding + child.border).cross_axis_sum(dir);
        let (fit_content_min, fit_content_max) = (child.min_size.cross(dir), child.max_size.cross(dir));
        let mut known_for_probe = |tree: &mut _| -> Size<Option<f32>> {
            let Some(available) = fit_content_available else { return provisional_cross_removed };
            *fit_content_known.get_or_insert_with(|| {
                // The final width's own helper and clamp
                // (determine_hypothetical_cross_size), so the two agree.
                let fit_content = crate::compute::common::fit_content_width(
                    tree,
                    child.node,
                    provisional_cross_removed,
                    Size { width: true, height: true },
                    child_parent_size,
                    available,
                    SizingMode::ContentSize,
                )
                .maybe_clamp(fit_content_min, fit_content_max)
                .max(fit_content_floor);
                provisional_cross_removed.with_cross(dir, Some(fit_content))
            })
        };

        child.flex_basis = 'flex_basis: {
            // A. If the item has a definite used flex basis, that’s the flex base size.

            // B. If the flex item has an intrinsic aspect ratio,
            //    a used flex basis of content, and a definite cross size,
            //    then the flex base size is calculated from its inner
            //    cross size and the flex item’s intrinsic aspect ratio.

            // Note: `child.size` has already been resolved against aspect_ratio in generate_anonymous_flex_items
            // So B will just work here by using main_size without special handling for aspect_ratio
            let main_size = child.size.main(dir);
            let main_stretch_size = percent_resolution_main_size.maybe_sub(child.margin.main_axis_sum(dir));

            // A flex basis that is a sizing keyword (min-content, max-content, fit-content,
            // fit-content(...), stretch) is used in place of the main size property: `stretch`
            // resolves to an exact (definite) size while the other keywords determine the
            // available space constraint the item is measured under. A keyword that cannot be
            // resolved in the current context behaves as `content`.
            let keyword_main_available_space = if flex_basis_style.is_content() {
                // A flex basis of `content` indicates an automatic size based on the item's
                // content: the item is measured (ignoring its main size property) under the
                // default constraint below
                None
            } else if flex_basis_style.is_sizing_keyword() {
                match resolve_sizing_keyword(flex_basis_style, main_stretch_size, percent_resolution_main_size) {
                    Some(SizingKeywordResolution::Exact(size)) => {
                        child.flex_basis_is_definite = true;
                        child.used_flex_basis_is_definite = true;
                        break 'flex_basis size;
                    }
                    Some(SizingKeywordResolution::Measure(available)) => Some(available),
                    None => None,
                }
            } else {
                if let Some(flex_basis) = flex_basis.or(main_size) {
                    child.flex_basis_is_definite = true;
                    break 'flex_basis flex_basis;
                };

                // A main size that is a sizing keyword either resolves to an exact size or
                // determines the available space constraint the item is measured under
                match resolve_sizing_keyword(
                    child.size_style.main(dir),
                    main_stretch_size,
                    percent_resolution_main_size,
                ) {
                    Some(SizingKeywordResolution::Exact(size)) => {
                        child.flex_basis_is_definite = true;
                        child.used_flex_basis_is_definite = flex_basis_style.is_auto();
                        break 'flex_basis size;
                    }
                    Some(SizingKeywordResolution::Measure(available)) => Some(available),
                    None => None,
                }
            };

            // C. If the used flex basis is content or depends on its available space,
            //    and the flex container is being sized under a min-content or max-content
            //    constraint (e.g. when performing automatic table layout [CSS21]),
            //    size the item under that constraint. The flex base size is the item’s
            //    resulting main size.

            // This is covered by the implementation of E below, which passes the available_space constraint
            // through to the child size computation. It may need a separate implementation if/when D is implemented.

            // D. Otherwise, if the used flex basis is content or depends on its
            //    available space, the available main size is infinite, and the flex item’s
            //    inline axis is parallel to the main axis, lay the item out using the rules
            //    for a box in an orthogonal flow [CSS3-WRITING-MODES]. The flex base size
            //    is the item’s max-content main size.

            // TODO if/when vertical writing modes are supported

            // If the item has an aspect ratio and a definite cross size then the flex base size
            // is calculated by transferring that cross size through the aspect ratio (case B
            // above), and is therefore definite.
            if child_cross_size_is_definite {
                if let (Some(ratio), Some(cross)) = (ratio, child_known_dimensions.cross(dir)) {
                    child.flex_basis_is_definite = true;
                    break 'flex_basis if dir.is_row() { ratio.width(cross) } else { ratio.height(cross) };
                }
            }

            // E. Otherwise, size the item into the available space using its used flex basis
            //    in place of its main size, treating a value of content as max-content.
            //    If a cross size is needed to determine the main size (e.g. when the
            //    flex item’s main size is in its block axis) and the flex item’s cross size
            //    is auto and not definite, in this calculation use fit-content as the
            //    flex item’s cross size. The flex base size is the item’s resulting main size.

            let child_available_space = Size::MAX_CONTENT
                .with_main(
                    dir,
                    keyword_main_available_space.unwrap_or(
                        // Map AvailableSpace::Definite to AvailableSpace::MaxContent
                        if available_space.main(dir) == AvailableSpace::MinContent {
                            AvailableSpace::MinContent
                        } else {
                            AvailableSpace::MaxContent
                        },
                    ),
                )
                .with_cross(dir, cross_axis_available_space.maybe_sub(cross_axis_margin_sum));

            debug_log!("COMPUTE CHILD BASE SIZE:");
            let known = known_for_probe(tree);
            break 'flex_basis tree.measure_child_size(
                child.node,
                known,
                child_parent_size,
                child_available_space,
                SizingMode::ContentSize,
                dir.main_axis(),
                Line::FALSE,
            );
        };

        // Floor flex-basis by the padding_border_sum (floors inner_flex_basis at zero)
        // This seems to be in violation of the spec which explicitly states that the content box should not be floored at zero
        // (like it usually is) when calculating the flex-basis. But including this matches both Chrome and Firefox's behaviour.
        //
        // TODO: resolve spec violation
        // Spec: https://www.w3.org/TR/css-flexbox-1/#intrinsic-item-contributions
        // Spec: https://www.w3.org/TR/css-flexbox-1/#change-2016-max-contribution
        let padding_border_sum = child.padding.main_axis_sum(constants.dir) + child.border.main_axis_sum(constants.dir);
        child.flex_basis = child.flex_basis.max(padding_border_sum);

        // The hypothetical main size is the item’s flex base size clamped according to its
        // used min and max main sizes (and flooring the content box size at zero).

        child.inner_flex_basis =
            child.flex_basis - child.padding.main_axis_sum(constants.dir) - child.border.main_axis_sum(constants.dir);

        let padding_border_axes_sums = (child.padding + child.border).sum_axes().map(Some);

        // Note that it is important that the `parent_size` parameter in the main axis is not set for this
        // function call as it used for resolving percentages, and percentage size in an axis should not contribute
        // to a min-content contribution in that same axis. However the `parent_size` and `available_space` *should*
        // be set to their usual values in the cross axis so that wrapping content can wrap correctly.
        //
        // See https://drafts.csswg.org/css-sizing-3/#min-percentage-contribution
        let style_min_main_size =
            child.min_size.or(child.overflow.map(Overflow::maybe_into_automatic_min_size).into()).main(dir);

        child.resolved_minimum_main_size = style_min_main_size.unwrap_or_else(|| {
            let min_content_main_size = {
                let child_available_space = Size::MIN_CONTENT
                    .with_cross(dir, cross_axis_available_space.maybe_sub(cross_axis_margin_sum));

                debug_log!("COMPUTE CHILD MIN SIZE:");
                // EXACT PATCH 37: measure the content suggestion independently
                // of the transferred suggestion; a replaced item takes their
                // smaller value (CSS Flexbox §4.5).
                // A definite stretch is already the used cross size, so it
                // still sizes the content probe (CSS Flexbox §9.8).
                let definite_stretch = child.size.cross(dir).is_none() && child_cross_size_is_definite;
                let known = if replaced_ratio.is_some() && !definite_stretch {
                    Size::NONE
                } else {
                    known_for_probe(tree)
                };
                // EXACT PATCH 38: ContentSize ignores the item's preferred
                // width, but percentage padding still needs the container's
                // inline size (CSS Flexbox §4.2).
                let parent = if replaced_ratio.is_some() {
                    Size { width: constants.node_inner_size.width, ..child_parent_size }
                } else {
                    child_parent_size
                };
                tree.measure_child_size(
                    child.node,
                    known,
                    parent,
                    child_available_space,
                    SizingMode::ContentSize,
                    dir.main_axis(),
                    Line::FALSE,
                )
            };

            let min_content_main_size = if let Some(ratio) = replaced_ratio {
                let transfer = |cross| if dir.is_row() { ratio.width(cross) } else { ratio.height(cross) };
                let content = min_content_main_size.maybe_clamp(
                    child.min_size.cross(dir).map(transfer),
                    child.max_size.cross(dir).map(transfer),
                );
                let transferred = child.size.cross(dir)
                    .maybe_clamp(child.min_size.cross(dir), child.max_size.cross(dir))
                    .map(transfer);
                content.maybe_min(transferred)
            } else {
                min_content_main_size
            };

            // 4.5. Automatic Minimum Size of Flex Items
            // https://www.w3.org/TR/css-flexbox-1/#min-size-auto
            // EXACT PATCH 57: Chrome 151 lets a non-replaced row item's wider
            // content minimum exceed the converted max-height when its height
            // is definite. Other rows retain the transferred cap; columns
            // use the cross-axis bounds below.
            let content_max = if dir.is_column() {
                // EXACT PATCH 59: a column's content size suggestion is
                // capped by max-width through the ratio, floored by min-width
                // before transfer. Authored max-height still caps the result
                // (CSS Flexbox §4.5).
                ratio.zip(child.max_size.width)
                    .map(|(ratio, width)| ratio.height(width.maybe_max(child.min_size.width)).maybe_min(child.max_size.height))
                    .or(child.max_size.height)
            } else if replaced_ratio.is_none() && child_cross_size_is_definite {
                child.max_size.main(dir)
            } else {
                transferred_max_size.main(dir)
            };
            let clamped_min_content_size =
                min_content_main_size.maybe_min(child.size.main(dir)).maybe_min(content_max);
            clamped_min_content_size.maybe_max(padding_border_axes_sums.main(dir))
        });

        // Sizes transferred through the aspect ratio clamp the hypothetical main size,
        // but do not participate in resolving flexible lengths or clamping the final size.
        // https://github.com/w3c/csswg-drafts/issues/10997
        let hypothetical_inner_min_main = child
            .resolved_minimum_main_size
            .maybe_max(transferred_min_size.main(constants.dir))
            .maybe_max(padding_border_axes_sums.main(constants.dir));
        let hypothetical_inner_size =
            child.flex_basis.maybe_clamp(Some(hypothetical_inner_min_main), transferred_max_size.main(constants.dir));
        let hypothetical_outer_size = hypothetical_inner_size + child.margin.main_axis_sum(constants.dir);

        child.hypothetical_inner_size.set_main(constants.dir, hypothetical_inner_size);
        child.hypothetical_outer_size.set_main(constants.dir, hypothetical_outer_size);
    }
}

/// Collect flex items into flex lines.
///
/// # [9.3. Main Size Determination](https://www.w3.org/TR/css-flexbox-1/#main-sizing)
///
/// - [**Collect flex items into flex lines**](https://www.w3.org/TR/css-flexbox-1/#algo-line-break):
///
///     - If the flex container is single-line, collect all the flex items into a single flex line.
///
///     - Otherwise, starting from the first uncollected item, collect consecutive items one by one until the first time that the next collected item would not fit into the flex container’s inner main size
///       (or until a forced break is encountered, see [§10 Fragmenting Flex Layout](https://www.w3.org/TR/css-flexbox-1/#pagination)).
///       If the very first uncollected item wouldn't fit, collect just it into the line.
///
///       For this step, the size of a flex item is its outer hypothetical main size. (**Note: This can be negative**.)
///
///       Repeat until all flex items have been collected into flex lines.
///
///       **Note that the "collect as many" line will collect zero-sized flex items onto the end of the previous line even if the last non-zero item exactly "filled up" the line**.
#[inline]
pub(super) fn collect_flex_lines<'a>(
    constants: &AlgoConstants,
    available_space: Size<AvailableSpace>,
    flex_items: &'a mut Vec<FlexItem>,
) -> Vec<FlexLine<'a>> {
    // Wrapping into multiple lines requires a definite main size. If the container's known main size
    // is derived from its own content (and is thus indefinite) then all items are collected into a
    // single flex line, matching how the container was sized under a min/max-content constraint.
    if !constants.is_wrap || !constants.known_main_size_is_definite {
        let mut lines = new_vec_with_capacity(1);
        lines.push(FlexLine { items: flex_items.as_mut_slice(), cross_size: 0.0, offset_cross: 0.0 });
        lines
    } else {
        let main_axis_available_space = match constants.max_size.main(constants.dir) {
            // EXACT PATCH 47: max-content row width is measured without wrapping;
            // compute_preliminary then wraps at the clamped width to size height
            // and place items, including when there is no later layout pass.
            // Columns still wrap at max-height: MaxContent also represents the
            // indefinite height offered by ordinary block layout.
            // https://www.w3.org/TR/css-flexbox-1/#intrinsic-main-sizes
            Some(_) if constants.is_row && available_space.width == AvailableSpace::MaxContent => {
                AvailableSpace::MaxContent
            }
            Some(max_size) => AvailableSpace::Definite({
                let available = available_space.main(constants.dir).into_option().unwrap_or(max_size);
                // If the container's main size is not definite then it is at most the max main size,
                // so the max size (and not the available space) is the limit that items wrap against.
                let available = if constants.has_definite_main_size { available } else { available.min(max_size) };
                available.maybe_max(constants.min_size.main(constants.dir))
            }),
            None => available_space.main(constants.dir),
        };

        match main_axis_available_space {
            // If we're sizing under a max-content constraint then the flex items will never wrap
            // (at least for now - future extensions to the CSS spec may add provisions for forced wrap points)
            AvailableSpace::MaxContent => {
                let mut lines = new_vec_with_capacity(1);
                lines.push(FlexLine { items: flex_items.as_mut_slice(), cross_size: 0.0, offset_cross: 0.0 });
                lines
            }
            // If flex-wrap is Wrap and we're sizing under a min-content constraint, then we take every possible wrapping opportunity
            // and place each item in it's own line
            AvailableSpace::MinContent => {
                let mut lines = new_vec_with_capacity(flex_items.len());
                let mut items = &mut flex_items[..];
                while !items.is_empty() {
                    let (line_items, rest) = items.split_at_mut(1);
                    lines.push(FlexLine { items: line_items, cross_size: 0.0, offset_cross: 0.0 });
                    items = rest;
                }
                lines
            }
            AvailableSpace::Definite(main_axis_available_space) => {
                let mut lines = new_vec_with_capacity(1);
                let mut flex_items = &mut flex_items[..];
                let main_axis_gap = constants.gap.main(constants.dir);

                while !flex_items.is_empty() {
                    // Find index of the first item in the next line
                    // (or the last item if all remaining items are in the current line)
                    let mut line_length = 0.0;
                    let index = flex_items
                        .iter()
                        .enumerate()
                        .find(|&(idx, child)| {
                            // EXACT PATCH 47: sum items and gaps separately, as intrinsic sizing does.
                            // Interleaving fractional gaps can round a fitting line above its limit.
                            line_length += child.hypothetical_outer_size.main(constants.dir);
                            let gap_sum = sum_axis_gaps(main_axis_gap, idx + 1);
                            // Allow 1/1024px of f32 roundoff at the fit boundary: sixteen times
                            // smaller than Chrome's 1/64px layout unit, independent of container size.
                            line_length + gap_sum - main_axis_available_space > 1.0 / 1024.0 && idx != 0
                        })
                        .map(|(idx, _)| idx)
                        .unwrap_or(flex_items.len());

                    let (items, rest) = flex_items.split_at_mut(index);
                    lines.push(FlexLine { items, cross_size: 0.0, offset_cross: 0.0 });
                    flex_items = rest;
                }
                lines
            }
        }
    }
}

/// Collect flex items into balanced flex lines (`flex-wrap: balance`), such that the largest
/// line is as small as possible.
///
/// See <https://drafts.csswg.org/css-flexbox-2/#balance-values>
#[cfg(feature = "flexbox_balance")]
pub(super) fn collect_balanced_flex_lines<'a>(
    constants: &AlgoConstants,
    available_space: Size<AvailableSpace>,
    flex_items: &'a mut [FlexItem],
) -> Vec<FlexLine<'a>> {
    if flex_items.is_empty() {
        return new_vec_with_capacity(0);
    }

    // If the container's known main size is derived from its own content (and is thus indefinite)
    // then items are balanced without a size limit, matching how the container was sized under a
    // min/max-content constraint.
    let main_axis_available_space = if constants.known_main_size_is_definite {
        match constants.max_size.main(constants.dir) {
            Some(max_size) => AvailableSpace::Definite({
                let available = available_space.main(constants.dir).into_option().unwrap_or(max_size);
                // If the container's main size is not definite then it is at most the max main size,
                // so the max size (and not the available space) is the limit that items wrap against.
                let available = if constants.has_definite_main_size { available } else { available.min(max_size) };
                available.maybe_max(constants.min_size.main(constants.dir))
            }),
            None => available_space.main(constants.dir),
        }
    } else {
        AvailableSpace::MaxContent
    };

    // If we're sizing under a min-content constraint then we take every possible wrapping
    // opportunity and place each item in its own line, the same as greedy wrapping (the
    // min-content main size of a multi-line container is the size of its largest item)
    if main_axis_available_space == AvailableSpace::MinContent {
        let mut lines = new_vec_with_capacity(flex_items.len());
        let mut items = &mut flex_items[..];
        while !items.is_empty() {
            let (line_items, rest) = items.split_at_mut(1);
            lines.push(FlexLine { items: line_items, cross_size: 0.0, offset_cross: 0.0 });
            items = rest;
        }
        return lines;
    }

    let line_break_size = main_axis_available_space.into_option().unwrap_or(f32::INFINITY);
    let min_line_count = constants.line_count.unwrap_or(1) as usize;
    let item_counts = balance::balanced_line_item_counts(
        flex_items.iter().map(|item| item.hypothetical_outer_size.main(constants.dir)),
        line_break_size,
        constants.gap.main(constants.dir),
        min_line_count,
    );

    let mut lines = new_vec_with_capacity(item_counts.len());
    let mut items = &mut flex_items[..];
    for count in item_counts {
        let (line_items, rest) = items.split_at_mut(count);
        lines.push(FlexLine { items: line_items, cross_size: 0.0, offset_cross: 0.0 });
        items = rest;
    }
    debug_assert!(items.is_empty());
    lines
}

/// Compute whether each of an item's known dimensions should be treated as definite when performing
/// layout on the item. An item's post-flexing main size is only treated as definite if the container
/// has a definite main size or the item's used flex basis is definite.
/// See <https://www.w3.org/TR/css-flexbox-1/#definite-sizes>
#[inline]
pub(super) fn item_known_dimension_definiteness(constants: &AlgoConstants, item: &FlexItem) -> Size<bool> {
    let dir = constants.dir;
    let main_is_definite = constants.has_definite_main_size || item.flex_basis_is_definite;

    // An item's cross size is definite if it is stretched (it stretches to the flex line, whose
    // size is known by the time the item's final layout is performed), or if its cross size style
    // resolved to a definite size. Additionally, a non-stretched item's cross *width* is fit-content
    // sized, and a size resolved by fit-content sizing against definite available space is itself
    // definite (<https://www.w3.org/TR/css-sizing-3/#definite>). The same does not apply to cross
    // heights, which are content-based and therefore indefinite when not stretched.

    let has_cross_auto_margins = item.margin_is_auto.cross_start(dir) || item.margin_is_auto.cross_end(dir);
    let cross_size = item.size_style.cross(dir);
    let is_stretched = !has_cross_auto_margins
        && (cross_size.is_stretch() || (item.align_self == AlignSelf::STRETCH && cross_size.is_auto()));
    let ratio_auto_height = dir.is_row() && item.aspect_ratio.is_some()
        && item.size_style.height.is_auto() && !constants.has_definite_cross_size;
    let cross_is_definite = !ratio_auto_height && (is_stretched
        || item.size.cross(dir).is_some()
        || (!dir.is_row() && constants.cross_axis_available_space_is_definite));

    Size { width: true, height: true }.with_main(dir, main_is_definite).with_cross(dir, cross_is_definite)
}

/// The widest item's outer min-content contribution in a wrapping row.
pub(super) fn wrapping_row_min_content(
    tree: &mut impl LayoutFlexboxContainer,
    lines: &[FlexLine<'_>],
    constants: &AlgoConstants,
    available_space: Size<AvailableSpace>,
) -> f32 {
    // EXACT PATCH 48: include contents when the automatic minimum is zero.
    // Chrome uses preferred width and min/max, but neither flex-basis clamp,
    // even for inflexible items or scroll containers (§9.9.1.3).
    // https://www.w3.org/TR/css-flexbox-1/#intrinsic-item-contributions
    let available_space = Size { width: AvailableSpace::MinContent, ..available_space };
    lines.iter().flat_map(|line| line.items.iter()).map(|item| {
        let width = tree.measure_child_size(
            item.node,
            Size::NONE,
            constants.node_inner_size,
            available_space.maybe_sub(item.margin.sum_axes()),
            SizingMode::InherentSize,
            constants.dir.main_axis(),
            Line::FALSE,
        );
        width.max(item.resolved_minimum_main_size) + item.margin.horizontal_axis_sum()
    }).fold(0.0, f32_max)
}

/// Determine the container's main size (if not already known)
pub(super) fn determine_container_main_size(
    tree: &mut impl LayoutFlexboxContainer,
    available_space: Size<AvailableSpace>,
    lines: &mut [FlexLine<'_>],
    constants: &mut AlgoConstants,
) {
    let dir = constants.dir;
    let main_content_box_inset = constants.content_box_inset.main_axis_sum(constants.dir);

    // EXACT PATCH 33: an item's length is its flex base size clamped by its
    // min *and* max main size, as an intrinsic contribution below is. Floored
    // by the minimum alone, a leaf's length was its whole content (a leaf's
    // base size is measured without its max-height), and a column in definite
    // space was as tall as that content. The clamp can now go below the
    // padding and border, so the inner size is floored there before the
    // margin is added (patch 14's order), or a margin was lost to the floor.
    let item_main_length = |child: &FlexItem| {
        let padding_border_sum = (child.padding + child.border).main_axis_sum(dir);
        // EXACT PATCH 46: use the minimum that flexing enforces, including
        // the content-based automatic minimum (CSS Flexbox §4.5).
        child.flex_basis
            .maybe_clamp(Some(child.resolved_minimum_main_size), child.max_size.main(dir))
            .max(padding_border_sum)
            + child.margin.main_axis_sum(dir)
    };

    let is_wrapping_row = constants.is_row && constants.is_wrap;
    #[cfg(feature = "flexbox_balance")]
    let is_wrapping_row = is_wrapping_row && !constants.is_balance;

    let outer_main_size: f32 = constants.node_outer_size.main(constants.dir).unwrap_or_else(|| {
        match available_space.main(dir) {
            AvailableSpace::Definite(main_axis_available_space) => {
                let main_axis_gap = constants.gap.main(constants.dir);
                let longest_line_length: f32 = lines
                    .iter()
                    .map(|line| {
                        let line_main_axis_gap = sum_axis_gaps(main_axis_gap, line.items.len());
                        let total_target_size = line.items.iter().map(item_main_length).sum::<f32>();
                        total_target_size + line_main_axis_gap
                    })
                    .max_by(|a, b| a.total_cmp(b))
                    .unwrap_or(0.0);
                let size = longest_line_length + main_content_box_inset;

                // A balanced container can produce multiple lines that all fit within the
                // available space (via `flex-line-count`), in which case fit-content sizing
                // uses its max-content size: the longest line when items are balanced across
                // the minimum line count without a size limit.
                #[cfg(feature = "flexbox_balance")]
                if constants.is_balance {
                    let min_line_count = constants.line_count.unwrap_or(1);
                    let item_count = lines.iter().map(|line| line.items.len()).sum::<usize>();
                    if item_count == 0 {
                        return size;
                    }
                    let mut item_lengths: Vec<f32> = new_vec_with_capacity(item_count);
                    for line in lines.iter() {
                        for child in line.items.iter() {
                            item_lengths.push(item_main_length(child));
                        }
                    }
                    let item_counts = balance::balanced_line_item_counts(
                        item_lengths.iter().copied(),
                        f32::INFINITY,
                        main_axis_gap,
                        min_line_count.max(1) as usize,
                    );
                    let mut widest_line_length: f32 = 0.0;
                    let mut index = 0;
                    for count in item_counts {
                        let line_length: f32 = item_lengths[index..index + count].iter().sum::<f32>()
                            + sum_axis_gaps(main_axis_gap, count);
                        widest_line_length = widest_line_length.max(line_length);
                        index += count;
                    }
                    let max_content_size = widest_line_length + main_content_box_inset;
                    return f32_max(size, f32_min(max_content_size, main_axis_available_space));
                }

                // EXACT PATCH 45: a column's auto height follows its longest line;
                // definite available space limits wrapping, not the used height.
                // Rows retain the available-width floor for fit-content sizing.
                // https://www.w3.org/TR/css-flexbox-1/#algo-main-container
                if constants.is_row && lines.len() > 1 {
                    f32_max(size, main_axis_available_space)
                } else {
                    size
                }
            }
            AvailableSpace::MinContent if is_wrapping_row => {
                wrapping_row_min_content(tree, lines, constants, available_space) + main_content_box_inset
            }
            AvailableSpace::MinContent if constants.is_wrap => {
                let longest_line_length: f32 = lines
                    .iter()
                    .map(|line| {
                        let line_main_axis_gap = sum_axis_gaps(constants.gap.main(constants.dir), line.items.len());
                        let total_target_size = line.items.iter().map(item_main_length).sum::<f32>();
                        total_target_size + line_main_axis_gap
                    })
                    .max_by(|a, b| a.total_cmp(b))
                    .unwrap_or(0.0);
                longest_line_length + main_content_box_inset
            }
            AvailableSpace::MinContent | AvailableSpace::MaxContent => {
                // Define a base main_size variable. This is mutated once for iteration over the outer
                // loop over the flex lines as:
                //   "The flex container’s max-content size is the largest sum of the afore-calculated sizes of all items within a single line."
                let mut main_size = 0.0;

                for line in lines.iter_mut() {
                    let mut item_main_size_sum = 0.0;
                    for item in line.items.iter_mut() {
                        let style_min = item.min_size.main(constants.dir);
                        let style_preferred = item.size.main(constants.dir);
                        let style_max = item.max_size.main(constants.dir);

                        // Columns retain the upstream basis clamps; rows apply Patch 51 below.
                        // The spec seems a bit unclear on this point (my initial reading was that the `.maybe_max(style_preferred)` should
                        // not be included here), however this matches both Chrome and Firefox as of 9th March 2023.
                        //
                        // Spec: https://www.w3.org/TR/css-flexbox-1/#intrinsic-item-contributions
                        // Spec modification: https://www.w3.org/TR/css-flexbox-1/#change-2016-max-contribution
                        // Issue: https://github.com/w3c/csswg-drafts/issues/1435
                        // Gentest: padding_border_overrides_size_flex_basis_0.html
                        let clamping_basis = Some(item.flex_basis).maybe_max(style_preferred).filter(|_| !constants.is_row);
                        let flex_basis_min = clamping_basis.filter(|_| item.flex_shrink == 0.0);
                        let flex_basis_max = clamping_basis.filter(|_| item.flex_grow == 0.0);

                        let min_main_size = style_min
                            .maybe_max(flex_basis_min)
                            .or(flex_basis_min)
                            .unwrap_or(item.resolved_minimum_main_size)
                            .max(item.resolved_minimum_main_size);
                        let max_main_size =
                            style_max.maybe_min(flex_basis_max).or(flex_basis_max).unwrap_or(f32::INFINITY);

                        // EXACT PATCH 33: the flex base size clamps a size first and the
                        // min and max main size last (§9.9.1), so an item that may not
                        // shrink below a base size past its max-height (`flex-shrink: 0`)
                        // contributes its max-height. Where the two ranges overlap this is
                        // the clamp to `min_main_size..max_main_size`; where they do not,
                        // that clamp gave the base size.
                        let clamp = |size: f32| {
                            size.maybe_min(flex_basis_max)
                                .maybe_max(flex_basis_min)
                                .maybe_min(style_max)
                                .max(item.resolved_minimum_main_size)
                        };

                        let content_contribution = match (min_main_size, style_preferred, max_main_size) {
                            // If the clamping values are such that max <= min, then we can avoid the expensive step of computing the content size
                            // as we know that the clamping values will override it anyway
                            (min, Some(pref), max) if max <= min || max <= pref => clamp(pref),
                            // EXACT PATCH 33: the clamps meet, so any size clamps to the answer.
                            (min, _, max) if max <= min => clamp(min),

                            // Else compute the min- or -max content size and apply the full formula for computing the
                            // min- or max- content contribution
                            // EXACT PATCH 33: clamped as the other contributions are, and
                            // floored by the padding and border the base size already was.
                            _ if !constants.is_row && item.is_scroll_container() => {
                                let item_pb_main = item.padding.main_axis_sum(constants.dir)
                                    + item.border.main_axis_sum(constants.dir);
                                item.flex_basis.maybe_clamp(style_min, style_max).max(item_pb_main)
                            }

                            // If the item has a definite preferred main size then that is its content
                            // contribution (an inherent-size measure of the item would return it), floored
                            // by the item's main-axis padding+border, so measuring the item can be skipped.
                            // Min/max clamping is applied in the same way as for measured contributions.
                            (_, Some(pref), _) => {
                                let item_pb_main = item.padding.main_axis_sum(constants.dir)
                                    + item.border.main_axis_sum(constants.dir);
                                // EXACT PATCH 14: clamp the inner size, then add the margin.
                                // Adding a (negative) margin first let `max(flex_basis)` and the
                                // min/max clamp, both inner sizes, erase it.
                                let inner = pref.max(item_pb_main);
                                if constants.is_row {
                                    inner.maybe_clamp(style_min, style_max)
                                } else {
                                    inner.max(item.flex_basis).maybe_clamp(style_min, style_max)
                                }
                            }

                            _ => {
                                // Parent size for child sizing
                                let cross_axis_parent_size = constants.node_inner_size.cross(dir);

                                // Available space for child sizing
                                let cross_axis_margin_sum = item.margin.cross_axis_sum(dir);
                                let child_min_cross = item.min_size.cross(dir).maybe_add(cross_axis_margin_sum);
                                let child_max_cross = item.max_size.cross(dir).maybe_add(cross_axis_margin_sum);
                                let cross_axis_available_space: AvailableSpace = available_space
                                    .cross(dir)
                                    .map_definite_value(|val| {
                                        constants.divided_cross_space(cross_axis_parent_size.unwrap_or(val))
                                    })
                                    .maybe_clamp(child_min_cross, child_max_cross);

                                let child_available_space = available_space.with_cross(
                                    dir, cross_axis_available_space.maybe_sub(cross_axis_margin_sum),
                                );

                                // Known dimensions for child sizing
                                let child_known_dimensions = {
                                    let mut ckd = item.size.with_main(dir, None);
                                    // EXACT PATCH 41: intrinsic contributions can transfer a stretch
                                    // only from a definite single-line cross size, without auto margins.
                                    if item.align_self == AlignSelf::STRETCH
                                        && !constants.is_wrap
                                        && constants.has_definite_cross_size
                                        && cross_axis_parent_size.is_some()
                                        && !item.margin_is_auto.cross_start(dir)
                                        && !item.margin_is_auto.cross_end(dir)
                                        && ckd.cross(dir).is_none()
                                    {
                                        ckd.set_cross(
                                            dir,
                                            cross_axis_available_space
                                                .into_option()
                                                .maybe_sub(item.margin.cross_axis_sum(dir)),
                                        );
                                    }
                                    ckd
                                };

                                // Either the min- or max- content size depending on which constraint we are sizing under.
                                // TODO: Optimise by using already computed values where available
                                debug_log!("COMPUTE CHILD BASE SIZE (for intrinsic main size):");
                                let measured_main_size = tree.measure_child_size(
                                    item.node,
                                    child_known_dimensions,
                                    constants.node_inner_size,
                                    child_available_space,
                                    SizingMode::ContentSize,
                                    dir.main_axis(),
                                    Line::FALSE,
                                );

                                // A known cross size is transferred through the item's aspect-ratio
                                // and floors the measured content size
                                let ratio = Ratio::of(
                                    &tree.get_flexbox_child_style(item.node),
                                    (item.padding + item.border).sum_axes(),
                                );
                                let transferred_main_size = ratio
                                    .zip(child_known_dimensions.cross(dir))
                                    .map(|(ratio, cross)| if constants.is_row { ratio.width(cross) } else { ratio.height(cross) });

                                // EXACT PATCH 14: the inner content size; the margin is added after
                                // the clamp below, as for any outer size.
                                let content_main_size = measured_main_size.maybe_max(transferred_main_size);

                                // This is somewhat bizarre in that it's asymmetrical depending whether the flex container is a column or a row.
                                //
                                // I *think* this might relate to https://drafts.csswg.org/css-flexbox-1/#algo-main-container:
                                //
                                //    "The automatic block size of a block-level flex container is its max-content size."
                                //
                                // Which could suggest that flex-basis defining a vertical size does not shrink because it is in the block axis, and the automatic size
                                // in the block axis is a MAX content size. Whereas a flex-basis defining a horizontal size does shrink because the automatic size in
                                // inline axis is MIN content size (although I don't have a reference for that).
                                //
                                // Ultimately, this was not found by reading the spec, but by trial and error fixing tests to align with Webkit/Firefox output.
                                // (see the `flex_basis_unconstraint_row` and `flex_basis_uncontraint_column` generated tests which demonstrate this)
                                if constants.is_row {
                                    content_main_size.maybe_clamp(style_min, style_max)
                                } else {
                                    content_main_size.max(item.flex_basis).maybe_clamp(style_min, style_max)
                                }
                            }
                        };
                        // EXACT PATCH 51: Chrome's row intrinsic sizing substitutes the
                        // hypothetical border-box size when a definite flex base cannot
                        // flex to the contribution. It already includes the automatic
                        // minimum and min/max bounds. Add margins only after this test.
                        // https://www.w3.org/TR/css-flexbox-1/#intrinsic-main-sizes
                        let cannot_flex = (content_contribution > item.flex_basis && item.flex_grow == 0.0)
                            || (content_contribution < item.flex_basis && item.flex_shrink == 0.0);
                        let content_contribution = if constants.is_row && item.used_flex_basis_is_definite && cannot_flex {
                            item.hypothetical_inner_size.main(dir)
                        } else {
                            content_contribution
                        } + item.margin.main_axis_sum(dir);
                        // EXACT PATCH 51: sum the outer contribution directly. No shared
                        // line fraction is applied; dividing and multiplying by a zero
                        // inner flex base discarded negative margins and contributions.
                        item.outer_target_size.set_main(dir, content_contribution);
                        item.target_size.set_main(dir, content_contribution);
                        item_main_size_sum += content_contribution;
                    }

                    let gap_sum = sum_axis_gaps(constants.gap.main(constants.dir), line.items.len());
                    main_size = f32_max(main_size, item_main_size_sum + gap_sum)
                }

                // EXACT PATCH 51: wrapping max-content cannot be narrower than
                // its min-content width, even without a fit-content caller.
                if is_wrapping_row {
                    main_size = main_size.max(wrapping_row_min_content(tree, lines, constants, available_space));
                }
                main_size + main_content_box_inset
            }
        }
    });

    let outer_main_size = outer_main_size
        .maybe_clamp(constants.min_size.main(constants.dir), constants.max_size.main(constants.dir))
        .max(main_content_box_inset - constants.scrollbar_gutter.main(constants.dir));

    // let outer_main_size = inner_main_size + constants.padding_border.main_axis_sum(constants.dir);
    let inner_main_size = f32_max(outer_main_size - main_content_box_inset, 0.0);
    constants.container_size.set_main(constants.dir, outer_main_size);
    constants.inner_container_size.set_main(constants.dir, inner_main_size);
    constants.node_inner_size.set_main(constants.dir, Some(inner_main_size));
}

/// Resolve the flexible lengths of the items within a flex line.
/// Sets the `main` component of each item's `target_size` and `outer_target_size`
///
/// # [9.7. Resolving Flexible Lengths](https://www.w3.org/TR/css-flexbox-1/#resolve-flexible-lengths)
#[inline]
pub(super) fn resolve_flexible_lengths(line: &mut FlexLine, constants: &AlgoConstants) {
    let total_main_axis_gap = sum_axis_gaps(constants.gap.main(constants.dir), line.items.len());

    // 1. Determine the used flex factor. Sum the outer hypothetical main sizes of all
    //    items on the line. If the sum is less than the flex container’s inner main size,
    //    use the flex grow factor for the rest of this algorithm; otherwise, use the
    //    flex shrink factor.

    let total_hypothetical_outer_main_size =
        line.items.iter().map(|child| child.hypothetical_outer_size.main(constants.dir)).sum::<f32>();
    let used_flex_factor: f32 = total_main_axis_gap + total_hypothetical_outer_main_size;
    let growing = used_flex_factor < constants.node_inner_size.main(constants.dir).unwrap_or(0.0);
    let shrinking = used_flex_factor > constants.node_inner_size.main(constants.dir).unwrap_or(0.0);
    let exactly_sized = !growing & !shrinking;

    // 2. Size inflexible items. Freeze, setting its target main size to its hypothetical main size
    //    - Any item that has a flex factor of zero
    //    - If using the flex grow factor: any item that has a flex base size
    //      greater than its hypothetical main size
    //    - If using the flex shrink factor: any item that has a flex base size
    //      smaller than its hypothetical main size

    for child in line.items.iter_mut() {
        let inner_target_size = child.hypothetical_inner_size.main(constants.dir);
        child.target_size.set_main(constants.dir, inner_target_size);

        if exactly_sized
            || (child.flex_grow == 0.0 && child.flex_shrink == 0.0)
            || (growing && child.flex_basis > child.hypothetical_inner_size.main(constants.dir))
            || (shrinking && child.flex_basis < child.hypothetical_inner_size.main(constants.dir))
        {
            child.frozen = true;
            let outer_target_size = inner_target_size + child.margin.main_axis_sum(constants.dir);
            child.outer_target_size.set_main(constants.dir, outer_target_size);
        }
    }

    if exactly_sized {
        return;
    }

    // 3. Calculate initial free space. Sum the outer sizes of all items on the line,
    //    and subtract this from the flex container’s inner main size. For frozen items,
    //    use their outer target main size; for other items, use their outer flex base size.

    let used_space: f32 = total_main_axis_gap
        + line
            .items
            .iter()
            .map(|child| {
                if child.frozen {
                    child.outer_target_size.main(constants.dir)
                } else {
                    child.flex_basis + child.margin.main_axis_sum(constants.dir)
                }
            })
            .sum::<f32>();

    let initial_free_space = constants.node_inner_size.main(constants.dir).maybe_sub(used_space).unwrap_or(0.0);

    // 4. Loop

    loop {
        // a. Check for flexible items. If all the flex items on the line are frozen,
        //    free space has been distributed; exit this loop.

        if line.items.iter().all(|child| child.frozen) {
            break;
        }

        // b. Calculate the remaining free space as for initial free space, above.
        //    If the sum of the unfrozen flex items’ flex factors is less than one,
        //    multiply the initial free space by this sum. If the magnitude of this
        //    value is less than the magnitude of the remaining free space, use this
        //    as the remaining free space.

        let used_space: f32 = total_main_axis_gap
            + line
                .items
                .iter()
                .map(|child| {
                    if child.frozen {
                        child.outer_target_size.main(constants.dir)
                    } else {
                        child.flex_basis + child.margin.main_axis_sum(constants.dir)
                    }
                })
                .sum::<f32>();

        // EXACT PATCH 3: iterate the unfrozen set without allocating it.

        let (sum_flex_grow, sum_flex_shrink): (f32, f32) =
            line.items.iter().filter(|child| !child.frozen).fold((0.0, 0.0), |(flex_grow, flex_shrink), item| {
                (flex_grow + item.flex_grow, flex_shrink + item.flex_shrink)
            });

        let free_space = if growing && sum_flex_grow < 1.0 {
            (initial_free_space * sum_flex_grow - total_main_axis_gap)
                .maybe_min(constants.node_inner_size.main(constants.dir).maybe_sub(used_space))
        } else if shrinking && sum_flex_shrink < 1.0 {
            (initial_free_space * sum_flex_shrink - total_main_axis_gap)
                .maybe_max(constants.node_inner_size.main(constants.dir).maybe_sub(used_space))
        } else {
            (constants.node_inner_size.main(constants.dir).maybe_sub(used_space))
                .unwrap_or(used_flex_factor - used_space)
        };

        // c. Distribute free space proportional to the flex factors.
        //    - If the remaining free space is zero
        //        Do Nothing
        //    - If using the flex grow factor
        //        Find the ratio of the item’s flex grow factor to the sum of the
        //        flex grow factors of all unfrozen items on the line. Set the item’s
        //        target main size to its flex base size plus a fraction of the remaining
        //        free space proportional to the ratio.
        //    - If using the flex shrink factor
        //        For every unfrozen item on the line, multiply its flex shrink factor by
        //        its inner flex base size, and note this as its scaled flex shrink factor.
        //        Find the ratio of the item’s scaled flex shrink factor to the sum of the
        //        scaled flex shrink factors of all unfrozen items on the line. Set the item’s
        //        target main size to its flex base size minus a fraction of the absolute value
        //        of the remaining free space proportional to the ratio. Note this may result
        //        in a negative inner main size; it will be corrected in the next step.
        //    - Otherwise
        //        Do Nothing

        if free_space.is_normal() {
            if growing && sum_flex_grow > 0.0 {
                for child in line.items.iter_mut().filter(|child| !child.frozen) {
                    child
                        .target_size
                        .set_main(constants.dir, child.flex_basis + free_space * (child.flex_grow / sum_flex_grow));
                }
            } else if shrinking && sum_flex_shrink > 0.0 {
                let sum_scaled_shrink_factor: f32 =
                    line.items.iter().filter(|child| !child.frozen).map(|child| child.inner_flex_basis * child.flex_shrink).sum();

                if sum_scaled_shrink_factor > 0.0 {
                    for child in line.items.iter_mut().filter(|child| !child.frozen) {
                        let scaled_shrink_factor = child.inner_flex_basis * child.flex_shrink;
                        child.target_size.set_main(
                            constants.dir,
                            child.flex_basis + free_space * (scaled_shrink_factor / sum_scaled_shrink_factor),
                        )
                    }
                }
            }
        }

        // d. Fix min/max violations. Clamp each non-frozen item’s target main size by its
        //    used min and max main sizes and floor its content-box size at zero. If the
        //    item’s target main size was made smaller by this, it’s a max violation.
        //    If the item’s target main size was made larger by this, it’s a min violation.

        let total_violation = line.items.iter_mut().filter(|child| !child.frozen).fold(0.0, |acc, child| -> f32 {
            let resolved_min_main: Option<f32> = child.resolved_minimum_main_size.into();
            let max_main = child.max_size.main(constants.dir);
            let clamped = child.target_size.main(constants.dir).maybe_clamp(resolved_min_main, max_main).max(0.0);
            child.violation = clamped - child.target_size.main(constants.dir);
            child.target_size.set_main(constants.dir, clamped);
            child.outer_target_size.set_main(
                constants.dir,
                child.target_size.main(constants.dir) + child.margin.main_axis_sum(constants.dir),
            );

            acc + child.violation
        });

        // e. Freeze over-flexed items. The total violation is the sum of the adjustments
        //    from the previous step ∑(clamped size - unclamped size). If the total violation is:
        //    - Zero
        //        Freeze all items.
        //    - Positive
        //        Freeze all the items with min violations.
        //    - Negative
        //        Freeze all the items with max violations.

        for child in line.items.iter_mut().filter(|child| !child.frozen) {
            match total_violation {
                v if v > 0.0 => child.frozen = child.violation > 0.0,
                v if v < 0.0 => child.frozen = child.violation < 0.0,
                _ => child.frozen = true,
            }
        }

        // f. Return to the start of this loop.
    }
}
