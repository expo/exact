//! EXACT PATCH 81 (LLP 1123 D12): bars, a child whose `RareStyle::bar` names an edge, which its
//! parent measures before its algorithm, reserves as padding and places in a slot on that edge
//! after it, in one layout. `EXACT-PATCHES.md` records the rule.

use super::*;
use crate::geometry::Line;
use crate::style::AlignItemsKeyword as Keyword;
use crate::tree::{LayoutPartialTree, RequestedAxis, RunMode, SizingMode};

/// A bar as its parent measured it.
struct Bar {
    node: NodeId,
    /// Its edge of the parent's box: top 0, right 1, bottom 2, left 3.
    edge: usize,
    order: u32,
    /// Its border-box size as measured.
    size: Size<f32>,
    margin: Rect<f32>,
    /// Its slot's depth: its margin box across the edge.
    depth: f32,
    stretch: bool,
    /// Where `align-self`, or its `auto` margins along the edge, place it along the edge, from
    /// 0 at the slot's start to 1 at its end.
    align: f32,
    /// Its least and greatest border-box size along the edge, which a stretch keeps to.
    range: (f32, f32),
    /// Whether its depth is its authored size, definite, rather than its content's.
    authored_depth: bool,
}

/// The side of `r` on `edge`, top 0, right 1, bottom 2, left 3.
fn side(r: &mut Rect<f32>, edge: usize) -> &mut f32 {
    match edge {
        0 => &mut r.top,
        1 => &mut r.right,
        2 => &mut r.bottom,
        _ => &mut r.left,
    }
}

impl<NodeContext, MeasureFunction> TaffyView<'_, NodeContext, MeasureFunction>
where
    MeasureFunction: FnMut(LayoutInput, NodeId, Option<&mut NodeContext>, &Style) -> LayoutOutput,
{
    /// The layout of `node_id`, some of whose children are bars: its bars measured, then its
    /// `algorithm` run with the bars reserved, then, for a final layout, each bar placed in its
    /// slot.
    pub(super) fn barred_layout(
        &mut self,
        node_id: NodeId,
        inputs: LayoutInput,
        algorithm: impl FnOnce(&mut Self, LayoutInput) -> LayoutOutput,
    ) -> LayoutOutput {
        let key: DefaultKey = node_id.into();
        let placed_style = Rc::clone(&self.taffy.nodes[key].style);
        let basis = inputs.parent_size.width;
        let resolve = |tree: &Self, v: LengthPercentage, basis: Option<f32>| {
            v.resolve_or_zero(basis, |v, b| LayoutPartialTreeExt::calc(tree, v, b))
        };
        let border = placed_style.border.map(|v| resolve(self, v, basis));
        let padding = placed_style.padding.map(|v| resolve(self, v, basis));
        let inset = placed_style.rare.bar_inset;
        let rtl = placed_style.direction.is_rtl();
        let known = inputs.known_dimensions;
        let (bars, taken, contribution) = self.measure_bars(key, known, inputs.available_space, border, inset, rtl);

        // In flow, the bars reserve as padding
        let mut barred = (*placed_style).clone();
        let mut whole = padding;
        for edge in 0..4 {
            let (p, t) = (*side(&mut whole, edge), [taken.top, taken.right, taken.bottom, taken.left][edge]);
            if t > 0.0 {
                *side(&mut whole, edge) = p + t;
                let length = LengthPercentage::length(p + t);
                match edge {
                    0 => barred.padding.top = length,
                    1 => barred.padding.right = length,
                    2 => barred.padding.bottom = length,
                    _ => barred.padding.left = length,
                }
            }
        }
        let content = barred.box_sizing == BoxSizing::ContentBox;
        let sides = Size {
            width: whole.left + whole.right + border.left + border.right,
            height: whole.top + whole.bottom + border.top + border.bottom,
        };
        // A definite size, minimum and maximum keep their border box whatever `box-sizing`, as a
        // UIKit view keeps its frame: the bars take their room from its content
        let shrink = |d: Dimension, parent: Option<f32>, by: f32, tree: &Self| {
            match d.maybe_resolve(parent, |v, b| LayoutPartialTreeExt::calc(tree, v, b)) {
                Some(v) if content && by > 0.0 => Dimension::length((v - by).max(0.0)),
                _ => d,
            }
        };
        let (across, down) = (taken.left + taken.right, taken.top + taken.bottom);
        let parent = inputs.parent_size;
        let shrunk = |d: Size<Dimension>, tree: &Self| Size {
            width: shrink(d.width, parent.width, across, tree),
            height: shrink(d.height, parent.height, down, tree),
        };
        barred.size = shrunk(placed_style.size, self);
        barred.min_size = shrunk(placed_style.min_size, self);
        barred.max_size = shrunk(placed_style.max_size, self);
        // An unknown size along an edge is at least its widest bar's, past the shrunk minimum
        let least = |min: Dimension, parent: Option<f32>, bar: f32, sides: f32, tree: &Self| -> Dimension {
            let own = bar - if content { sides } else { 0.0 };
            match min.maybe_resolve(parent, |v, b| LayoutPartialTreeExt::calc(tree, v, b)) {
                Some(min) if min >= own => Dimension::length(min),
                _ => Dimension::length(own.max(0.0)),
            }
        };
        if known.width.is_none() && contribution.width > 0.0 {
            barred.min_size.width = least(barred.min_size.width, parent.width, contribution.width, sides.width, self);
        }
        if known.height.is_none() && contribution.height > 0.0 {
            barred.min_size.height = least(barred.min_size.height, parent.height, contribution.height, sides.height, self);
        }
        self.taffy.nodes[key].style = Rc::new(barred);
        #[allow(unused_mut)]
        let mut output = algorithm(self, inputs);
        self.taffy.nodes[key].style = Rc::clone(&placed_style);
        if inputs.run_mode == RunMode::PerformLayout {
            let scroller = placed_style.overflow.x.is_scroll_container() || placed_style.overflow.y.is_scroll_container();
            let _overflow = self.place_bars(&bars, output.size, border, inset, rtl, scroller);
            // The absolute pass leaves the bars out of the overflow; each adds its own at its slot
            #[cfg(feature = "content_size")]
            {
                output.scrollable_overflow_rect = output.scrollable_overflow_rect.union(_overflow);
            }
        }
        output
    }

    /// Each bar of `parent` in tree order, measured against its slot's extent along its edge where
    /// the parent's `known` size gives it, and otherwise in the space the parent is offered there,
    /// `available`, as an in-flow child is; what they take of each edge; and the least
    /// border-box size along each axis they give the parent.
    fn measure_bars(
        &mut self,
        parent: DefaultKey,
        known: Size<Option<f32>>,
        available: Size<AvailableSpace>,
        border: Rect<f32>,
        inset: Rect<f32>,
        rtl: bool,
    ) -> (Vec<Bar>, Rect<f32>, Size<f32>) {
        let children: Vec<NodeId> = self.taffy.children[parent].iter().copied().collect();
        let (mut bars, mut taken, mut contribution) = (Vec::new(), Rect::ZERO, Size::ZERO);
        for (order, child) in children.into_iter().enumerate() {
            let Some(edge) = self.taffy.nodes[child.into()].style.rare.bar else {
                continue;
            };
            // Skip a bar laid out `display: none`, one its host replaces whole, which takes no
            // room, margins included, and keeps no depth; its parent's algorithm hides it as any
            // such child
            if self.taffy.nodes[child.into()].style.display == Display::None {
                self.taffy.bar_depths.remove(child.into());
                continue;
            }
            let edge = edge as usize;
            let horizontal = edge % 2 == 0;
            let sides = match horizontal {
                true => border.left + border.right + inset.left + inset.right + taken.left + taken.right,
                false => border.top + border.bottom + inset.top + inset.bottom + taken.top + taken.bottom,
            };
            let length = if horizontal { known.width } else { known.height };
            let extent = length.map(|l| (l - sides).max(0.0));
            let style = Rc::clone(&self.taffy.nodes[child.into()].style);
            // Percentages in the bar resolve against the slot, its margins against the slot's width
            let width = if horizontal { extent } else { None };
            let margin = style.margin.map(|m| m.resolve_or_zero(width, |v, b| LayoutPartialTreeExt::calc(&*self, v, b)));
            let (along_margin, across_margin) = match horizontal {
                true => (margin.left + margin.right, margin.top + margin.bottom),
                false => (margin.top + margin.bottom, margin.left + margin.right),
            };
            let along_auto = if horizontal { style.size.width.is_auto() } else { style.size.height.is_auto() };
            let authored_depth = !if horizontal { style.size.height.is_auto() } else { style.size.width.is_auto() };
            let keyword = style.align_self.map(|a| a.keyword);
            // An `auto` margin along the edge takes the free room, as it does a flex item's
            let (start, end) = match horizontal {
                true => (style.margin.left.is_auto(), style.margin.right.is_auto()),
                false => (style.margin.top.is_auto(), style.margin.bottom.is_auto()),
            };
            let stretch = along_auto && !start && !end && matches!(keyword, None | Some(Keyword::Stretch));
            let (min, max) = match horizontal {
                true => (style.min_size.width, style.max_size.width),
                false => (style.min_size.height, style.max_size.height),
            };
            // A `content-box` bar's bounds hold no padding or border, which the border box adds
            let edges = match style.box_sizing {
                BoxSizing::ContentBox => {
                    let resolve = |v: LengthPercentage| v.resolve_or_zero(width, |v, b| LayoutPartialTreeExt::calc(&*self, v, b));
                    let (padding, border) = (style.padding.map(resolve), style.border.map(resolve));
                    match horizontal {
                        true => padding.left + padding.right + border.left + border.right,
                        false => padding.top + padding.bottom + border.top + border.bottom,
                    }
                }
                BoxSizing::BorderBox => 0.0,
            };
            let bound = |d: Dimension| d.maybe_resolve(extent, |v, b| LayoutPartialTreeExt::calc(&*self, v, b)).map(|v| v + edges);
            let range = (bound(min).unwrap_or(0.0), bound(max).unwrap_or(f32::INFINITY));
            let along = extent.filter(|_| stretch).map(|e| (e - along_margin).max(0.0).min(range.1).max(range.0));
            let offered = if horizontal { available.width } else { available.height };
            let room = match (extent, offered) {
                (Some(e), _) => AvailableSpace::Definite((e - along_margin).max(0.0)),
                (None, AvailableSpace::Definite(a)) => AvailableSpace::Definite((a - sides - along_margin).max(0.0)),
                (None, intrinsic) => intrinsic,
            };
            let (known_dimensions, parent_size, available_space) = match horizontal {
                true => (
                    Size { width: along, height: None },
                    Size { width: extent, height: None },
                    Size { width: room, height: AvailableSpace::MaxContent },
                ),
                false => (
                    Size { width: None, height: along },
                    Size { width: None, height: extent },
                    Size { width: AvailableSpace::MaxContent, height: room },
                ),
            };
            let input = LayoutInput {
                run_mode: RunMode::ComputeSize,
                sizing_mode: SizingMode::InherentSize,
                axis: RequestedAxis::Both,
                known_dimensions,
                known_dimensions_are_definite: Size { width: true, height: true },
                parent_size,
                available_space,
                vertical_margins_are_collapsible: Line::FALSE,
            };
            let size = LayoutPartialTree::compute_child_layout(self, child, input).size;
            let depth = if horizontal { size.height } else { size.width } + across_margin;
            let outer = if horizontal { size.width } else { size.height } + along_margin + sides;
            match horizontal {
                true => contribution.width = contribution.width.max(outer),
                false => contribution.height = contribution.height.max(outer),
            }
            *side(&mut taken, edge) += depth;
            // The slot's cross axis runs along the edge: its start is the inline start for a top
            // or bottom bar, the top for a side one; `left` and `right` are physical
            let (align, physical) = match keyword {
                Some(Keyword::Center) => (0.5, false),
                Some(Keyword::FlexEnd | Keyword::End | Keyword::SelfEnd) => (1.0, false),
                Some(Keyword::Right) => (1.0, true),
                Some(Keyword::Left) => (0.0, true),
                _ => (0.0, false),
            };
            let align = if horizontal && rtl && !physical { 1.0 - align } else { align };
            let align = match (start, end) {
                (true, true) => 0.5,
                (true, false) => 1.0,
                (false, true) => 0.0,
                (false, false) => align,
            };
            bars.push(Bar { node: child, edge, order: order as u32, size, margin, depth, stretch, align, range, authored_depth });
        }
        (bars, taken, contribution)
    }

    /// Lay each of `bars` out in its slot of a parent `size` big, in tree order, keeping each
    /// one's slot depth for publication; returns what they add to the parent's scrollable
    /// overflow, measured from its scroll origin (the padding box's inline-start corner).
    fn place_bars(
        &mut self,
        bars: &[Bar],
        size: Size<f32>,
        border: Rect<f32>,
        inset: Rect<f32>,
        rtl: bool,
        scroller: bool,
    ) -> Rect<f32> {
        let mut before = Rect::ZERO;
        let mut overflow = Rect { left: 0.0, right: 0.0, top: 0.0, bottom: 0.0 };
        for bar in bars {
            let horizontal = bar.edge % 2 == 0;
            let x0 = border.left + inset.left + before.left;
            let x1 = size.width - border.right - inset.right - before.right;
            let y0 = border.top + inset.top + before.top;
            let y1 = size.height - border.bottom - inset.bottom - before.bottom;
            let slot = match bar.edge {
                0 => Rect { left: x0, right: x1, top: y0, bottom: y0 + bar.depth },
                1 => Rect { left: x1 - bar.depth, right: x1, top: y0, bottom: y1 },
                2 => Rect { left: x0, right: x1, top: y1 - bar.depth, bottom: y1 },
                _ => Rect { left: x0, right: x0 + bar.depth, top: y0, bottom: y1 },
            };
            let slot_size = Size { width: (slot.right - slot.left).max(0.0), height: (slot.bottom - slot.top).max(0.0) };
            let (extent, along_margin) = match horizontal {
                true => (slot_size.width, bar.margin.left + bar.margin.right),
                false => (slot_size.height, bar.margin.top + bar.margin.bottom),
            };
            // Stretched to the slot's final extent; its depth stays what it was measured at
            let along = match bar.stretch {
                true => (extent - along_margin).max(0.0).min(bar.range.1).max(bar.range.0),
                false => if horizontal { bar.size.width } else { bar.size.height },
            };
            let known = match horizontal {
                true => Size { width: along, height: bar.size.height },
                false => Size { width: bar.size.width, height: along },
            };
            let input = LayoutInput {
                run_mode: RunMode::PerformLayout,
                sizing_mode: SizingMode::InherentSize,
                axis: RequestedAxis::Both,
                known_dimensions: known.map(Some),
                // A depth its content gives is no definite size for its children's percentages
                known_dimensions_are_definite: match horizontal {
                    true => Size { width: true, height: bar.authored_depth },
                    false => Size { width: bar.authored_depth, height: true },
                },
                parent_size: slot_size.map(Some),
                available_space: slot_size.map(AvailableSpace::Definite),
                vertical_margins_are_collapsible: Line::FALSE,
            };
            let out = LayoutPartialTree::compute_child_layout(self, bar.node, input);
            let offset = (extent - along - along_margin).max(0.0) * bar.align;
            let location = match horizontal {
                true => Point { x: slot.left + bar.margin.left + offset, y: slot.top + bar.margin.top },
                false => Point { x: slot.left + bar.margin.left, y: slot.top + bar.margin.top + offset },
            };
            let style = Rc::clone(&self.taffy.nodes[bar.node.into()].style);
            let width = Some(slot_size.width);
            let resolve = |v: LengthPercentage| v.resolve_or_zero(width, |v, b| LayoutPartialTreeExt::calc(&*self, v, b));
            let mut layout = Layout::with_order(bar.order);
            layout.location = location;
            layout.size = out.size;
            #[cfg(feature = "content_size")]
            {
                layout.scrollable_overflow_rect = out.scrollable_overflow_rect;
            }
            layout.border = style.border.map(resolve);
            layout.padding = style.padding.map(resolve);
            layout.margin = bar.margin;
            self.set_unrounded_layout(bar.node, &layout);
            self.taffy.bar_depths.insert(bar.node.into(), bar.depth);
            *side(&mut before, bar.edge) += bar.depth;
            let origin = match rtl {
                true => Point { x: size.width - (location.x + out.size.width) - border.right, y: location.y - border.top },
                false => Point { x: location.x - border.left, y: location.y - border.top },
            };
            overflow = overflow.union(crate::compute::common::scrollable_overflow::compute_scrollable_overflow_contribution(
                origin,
                out.size,
                out.scrollable_overflow_rect,
                style.overflow,
                style.contain,
                scroller,
            ));
        }
        overflow
    }
}
