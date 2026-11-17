//! Shared resolution logic for sizing keywords (`min-content`, `max-content`, `fit-content`,
//! `fit-content(...)`, and `stretch`) on the `width`/`height` style properties
use crate::geometry::{AbsoluteAxis, Line, Rect, Size};
use crate::style::{AvailableSpace, CoreStyle};
use crate::tree::{LayoutPartialTree, LayoutPartialTreeExt, NodeId, SizingMode};
use crate::util::sys::f32_max;
use crate::{CompactLength, Dimension};

/// How a sizing keyword resolves to a used size
#[derive(Clone, Copy, Debug)]
pub(crate) enum SizingKeywordResolution {
    /// The size is the result of measuring the item under the given available space constraint
    Measure(AvailableSpace),
    /// EXACT PATCH 77: `fit-content` against a stretch size. In the inline axis it is
    /// `max(min-content, min(max-content, stretch))` (CSS Sizing 3 §3.2), which needs two
    /// measurements: a wrapped text measurer offered the stretch size returns its widest
    /// line, not this width. In the block axis it is a measurement under the stretch size
    /// ([`Self::block_axis`]).
    Fit(f32),
    /// The size resolves to an exact value without measuring the item
    Exact(f32),
}

impl SizingKeywordResolution {
    /// This resolution in the block axis, where `fit-content` is the content size as
    /// `min-content` and `max-content` are, so measured under its stretch size.
    pub(crate) fn block_axis(self) -> Self {
        match self {
            Self::Fit(size) => Self::Measure(AvailableSpace::Definite(size)),
            other => other,
        }
    }

    /// The available space a measurement under this resolution takes.
    pub(crate) fn available(&self) -> AvailableSpace {
        match *self {
            Self::Measure(available) => available,
            Self::Fit(size) | Self::Exact(size) => AvailableSpace::Definite(size),
        }
    }
}

/// EXACT PATCH 77: the border-box width a keyword resolution gives the item `node`,
/// measured with `known_dimensions` known, against `parent_size`, its height offered
/// `available_height`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn measure_keyword_width(
    tree: &mut impl LayoutPartialTree,
    node: NodeId,
    resolution: SizingKeywordResolution,
    known_dimensions: Size<Option<f32>>,
    parent_size: Size<Option<f32>>,
    available_height: AvailableSpace,
    sizing_mode: SizingMode,
    vertical_margins_are_collapsible: Line<bool>,
) -> f32 {
    match resolution {
        SizingKeywordResolution::Exact(width) => width,
        SizingKeywordResolution::Measure(available_width) => tree.measure_child_size(
            node,
            known_dimensions,
            parent_size,
            Size { width: available_width, height: available_height },
            sizing_mode,
            AbsoluteAxis::Horizontal,
            vertical_margins_are_collapsible,
        ),
        SizingKeywordResolution::Fit(stretch) => crate::compute::common::fit_content_width(
            tree,
            node,
            known_dimensions,
            known_dimensions.map(|d| d.is_some()),
            parent_size,
            stretch,
            sizing_mode,
        ),
    }
}

/// Resolve an item's size style in one axis if it is a sizing keyword (`min-content`,
/// `max-content`, `fit-content`, `fit-content(...)`, or `stretch`).
///
/// - `stretch_size` is the size the item would take if stretched to fill the available space
///   (available space minus margins). Used by `fit-content` and `stretch`.
/// - `percent_resolution_basis` is the size that percentages resolve against in this axis.
///   Used by `fit-content(<percentage>)`.
///
/// Returns `None` if the size style is not a sizing keyword, or if it cannot
/// be resolved in the current context (in which case it behaves as `auto`).
#[inline]
pub(crate) fn resolve_sizing_keyword(
    style: Dimension,
    stretch_size: Option<f32>,
    percent_resolution_basis: Option<f32>,
) -> Option<SizingKeywordResolution> {
    match style.tag() {
        CompactLength::MIN_CONTENT_TAG => Some(SizingKeywordResolution::Measure(AvailableSpace::MinContent)),
        CompactLength::MAX_CONTENT_TAG => Some(SizingKeywordResolution::Measure(AvailableSpace::MaxContent)),
        CompactLength::FIT_CONTENT_PX_TAG => {
            Some(SizingKeywordResolution::Measure(AvailableSpace::Definite(style.value())))
        }
        CompactLength::FIT_CONTENT_PERCENT_TAG => percent_resolution_basis
            .map(|basis| SizingKeywordResolution::Measure(AvailableSpace::Definite(basis * style.value()))),
        CompactLength::FIT_CONTENT_KEYWORD_TAG => stretch_size.map(SizingKeywordResolution::Fit),
        CompactLength::STRETCH_TAG => stretch_size.map(SizingKeywordResolution::Exact),
        _ => None,
    }
}

/// Resolve the sizing keywords (`min-content`, `max-content`, `fit-content`, `fit-content(...)`,
/// and `stretch`) on the size styles of an absolutely positioned item, filling in the
/// corresponding `known_dimensions` axes.
///
/// - `area_size` is the size of the item's containing block (which insets and percentages
///   resolve against).
/// - The stretch size in each axis is the containing block minus the item's insets and margins
///   in that axis.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_absolute_sizing_keywords(
    tree: &mut impl LayoutPartialTree,
    node: NodeId,
    known_dimensions: &mut Size<Option<f32>>,
    size_style: Size<Dimension>,
    area_size: Size<f32>,
    inset: Rect<Option<f32>>,
    margin: Rect<Option<f32>>,
    sizing_mode: SizingMode,
) {
    let stretch_size = Size {
        width: f32_max(
            area_size.width
                - inset.left.unwrap_or(0.0)
                - inset.right.unwrap_or(0.0)
                - margin.left.unwrap_or(0.0)
                - margin.right.unwrap_or(0.0),
            0.0,
        ),
        height: f32_max(
            area_size.height
                - inset.top.unwrap_or(0.0)
                - inset.bottom.unwrap_or(0.0)
                - margin.top.unwrap_or(0.0)
                - margin.bottom.unwrap_or(0.0),
            0.0,
        ),
    };

    let keyword_width = if known_dimensions.width.is_none() {
        resolve_sizing_keyword(size_style.width, Some(stretch_size.width), Some(area_size.width))
    } else {
        None
    };
    let keyword_height = if known_dimensions.height.is_none() {
        resolve_sizing_keyword(size_style.height, Some(stretch_size.height), Some(area_size.height))
            .map(SizingKeywordResolution::block_axis)
    } else {
        None
    };

    match (keyword_width, keyword_height) {
        // If both axes need to be measured then resolve them with a single measure call
        (
            Some(SizingKeywordResolution::Measure(available_width)),
            Some(SizingKeywordResolution::Measure(available_height)),
        ) => {
            let measured_size = tree.measure_child_size_both(
                node,
                Size::NONE,
                area_size.map(Some),
                Size { width: available_width, height: available_height },
                sizing_mode,
                Line::FALSE,
            );
            *known_dimensions = measured_size.map(Some);
        }
        (keyword_width, keyword_height) => {
            if let Some(resolution) = keyword_width {
                known_dimensions.width = Some(measure_keyword_width(
                    tree,
                    node,
                    resolution,
                    *known_dimensions,
                    area_size.map(Some),
                    AvailableSpace::Definite(stretch_size.height),
                    sizing_mode,
                    Line::FALSE,
                ));
            }
            if let Some(resolution) = keyword_height {
                known_dimensions.height = Some(match resolution {
                    SizingKeywordResolution::Exact(height) | SizingKeywordResolution::Fit(height) => height,
                    SizingKeywordResolution::Measure(available_height) => tree.measure_child_size(
                        node,
                        *known_dimensions,
                        area_size.map(Some),
                        Size {
                            width: known_dimensions
                                .width
                                .map(AvailableSpace::Definite)
                                .unwrap_or(AvailableSpace::Definite(stretch_size.width)),
                            height: available_height,
                        },
                        sizing_mode,
                        AbsoluteAxis::Vertical,
                        Line::FALSE,
                    ),
                });
            }
        }
    }
}

/// Whether `bound` is a keyword [`keyword_bounds`] resolves.
#[inline]
fn is_content_keyword(bound: Dimension) -> bool {
    bound.is_sizing_keyword() && !bound.is_stretch()
}

/// Whether any of a box's minimum or maximum sizes is a content keyword.
#[inline]
pub(crate) fn has_keyword_bounds(min: Size<Dimension>, max: Size<Dimension>) -> bool {
    is_content_keyword(min.width)
        || is_content_keyword(min.height)
        || is_content_keyword(max.width)
        || is_content_keyword(max.height)
}

/// EXACT PATCH 78: a box's keyword minimum and maximum sizes (`min-content`, `max-content`,
/// `fit-content`; CSS Sizing 3 §3.2), each `None` where the bound is not a keyword. A
/// keyword resolves to `None` as a length, so a caller takes these with `.or()` after
/// resolving the lengths, and adds no `box-sizing` adjustment: each is a border-box size,
/// measured from the box's content in [`SizingMode::ContentSize`], which reads no keyword
/// bound, so a box that measures itself here does not recurse.
///
/// In the inline axis a keyword is the min-content or max-content width, or the
/// fit-content width against `stretch`, the inline space the box is offered less its
/// margins; under a min-content or max-content constraint that is the min-content or
/// max-content width, and with a definite `size.height` a ratio transfers it (CSS Sizing 3
/// §5.1). In the block axis it is the content height at `size.width`, the box's border-box
/// width, or else at the stretch size.
pub(crate) fn keyword_bounds(
    tree: &mut impl LayoutPartialTree,
    node: NodeId,
    parent_size: Size<Option<f32>>,
    size: Size<Option<f32>>,
    stretch: AvailableSpace,
) -> (Size<Option<f32>>, Size<Option<f32>>) {
    let width = size.width;
    let height_only = Size { width: None, height: size.height };
    let (style_min, style_max) = {
        let style = tree.get_core_container_style(node);
        (style.min_size(), style.max_size())
    };
    let (mut min, mut max) = (Size::NONE, Size::NONE);
    if !has_keyword_bounds(style_min, style_max) {
        return (min, max);
    }
    let inline = |tree: &mut _, bound: Dimension| -> f32 {
        let available = match (bound.tag(), stretch) {
            (CompactLength::MIN_CONTENT_TAG, _) => AvailableSpace::MinContent,
            (CompactLength::MAX_CONTENT_TAG, _) => AvailableSpace::MaxContent,
            (_, AvailableSpace::Definite(stretch)) => {
                return crate::compute::common::fit_content_width(
                    tree,
                    node,
                    height_only,
                    Size { width: false, height: true },
                    parent_size,
                    stretch,
                    SizingMode::ContentSize,
                )
            }
            (_, constraint) => constraint,
        };
        let space = Size { width: available, height: AvailableSpace::MaxContent };
        measure_content(tree, node, height_only, parent_size, space, AbsoluteAxis::Horizontal)
    };
    if is_content_keyword(style_min.width) {
        min.width = Some(inline(tree, style_min.width));
    }
    if is_content_keyword(style_max.width) {
        max.width = Some(inline(tree, style_max.width));
    }
    if is_content_keyword(style_min.height) || is_content_keyword(style_max.height) {
        let available_width = width.map_or(stretch, AvailableSpace::Definite);
        let space = Size { width: available_width, height: AvailableSpace::MaxContent };
        let height =
            measure_content(tree, node, Size { width, height: None }, parent_size, space, AbsoluteAxis::Vertical);
        if is_content_keyword(style_min.height) {
            min.height = Some(height);
        }
        if is_content_keyword(style_max.height) {
            max.height = Some(height);
        }
    }
    (min, max)
}

/// EXACT PATCH 78: a box's keyword bounds at its own layout entry. Sizing itself
/// ([`SizingMode::InherentSize`]) it resolves them through [`keyword_bounds`]. Measuring its
/// content ([`SizingMode::ContentSize`]) it reads them as unset, so the measurement does not
/// re-enter, and in an axis with a keyword bound it reads its length bound as unset too: the
/// content size there is what the keyword measures (CSS Sizing 3 §5.1 sizes content without
/// the box's own bounds), and whoever reads it applies the bounds.
pub(crate) fn own_keyword_bounds(
    tree: &mut impl LayoutPartialTree,
    node: NodeId,
    inputs: crate::tree::LayoutInput,
) -> OwnKeywordBounds {
    let (style_min, style_max) = {
        let style = tree.get_core_container_style(node);
        (style.min_size(), style.max_size())
    };
    if !has_keyword_bounds(style_min, style_max) {
        return OwnKeywordBounds::NONE;
    }
    match inputs.sizing_mode {
        SizingMode::InherentSize => {
            let (min, max) =
                keyword_bounds(tree, node, inputs.parent_size, inputs.known_dimensions, inputs.available_space.width);
            OwnKeywordBounds { min, max, unset: Size { width: false, height: false } }
        }
        SizingMode::ContentSize => OwnKeywordBounds {
            min: Size::NONE,
            max: Size::NONE,
            unset: Size {
                width: is_content_keyword(style_min.width) || is_content_keyword(style_max.width),
                height: is_content_keyword(style_min.height) || is_content_keyword(style_max.height),
            },
        },
    }
}

/// What [`own_keyword_bounds`] gives a box's own entry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OwnKeywordBounds {
    /// The keyword minimum in each axis, or `None`.
    pub(crate) min: Size<Option<f32>>,
    /// The keyword maximum in each axis, or `None`.
    pub(crate) max: Size<Option<f32>>,
    /// The axes whose bounds read as unset.
    unset: Size<bool>,
}

impl OwnKeywordBounds {
    /// No keyword bound.
    pub(crate) const NONE: Self =
        Self { min: Size::NONE, max: Size::NONE, unset: Size { width: false, height: false } };

    /// The minimum, given the resolved lengths.
    pub(crate) fn min(&self, lengths: Size<Option<f32>>) -> Size<Option<f32>> {
        self.apply(lengths, self.min)
    }

    /// The maximum, given the resolved lengths.
    pub(crate) fn max(&self, lengths: Size<Option<f32>>) -> Size<Option<f32>> {
        self.apply(lengths, self.max)
    }

    fn apply(&self, lengths: Size<Option<f32>>, keywords: Size<Option<f32>>) -> Size<Option<f32>> {
        Size {
            width: if self.unset.width { None } else { lengths.width.or(keywords.width) },
            height: if self.unset.height { None } else { lengths.height.or(keywords.height) },
        }
    }
}

fn measure_content(
    tree: &mut impl LayoutPartialTree,
    node: NodeId,
    known_dimensions: Size<Option<f32>>,
    parent_size: Size<Option<f32>>,
    available_space: Size<AvailableSpace>,
    axis: AbsoluteAxis,
) -> f32 {
    tree.measure_child_size(node, known_dimensions, parent_size, available_space, SizingMode::ContentSize, axis, Line::FALSE)
}
