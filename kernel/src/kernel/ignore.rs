//! Taking a region's depth (`-exact-safe-area-ignore`, LLP 1123 D4). On an
//! owner's root its host leaves the edge uncleared (D3). Inside the root, a
//! box whose margin box touches the area its parent passes it, on an edge it
//! names, takes that area to its outer side there. The area fills its
//! padding there first, and its parent places it with what is left of the
//! padding, otherwise as though it did not touch; where filling the padding
//! would move it clear of the area, as where its own size places that edge,
//! the padding stays whole. Its subtree is laid out in its box outset by
//! what it takes (the engine's `outset`), or, with `align`, it keeps its
//! size and is moved within that. The solve measures what each box takes
//! from the frames, as it measures readers (`observe.rs`).
use crate::arena::NodeArena;
use crate::generated::{SafeAreaIgnore, SafeAreaIgnoreAlign, StyleProps};
use crate::Direction;

/// What a box that ignores takes past the box its parent places, as the
/// solve last measured it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Taken {
    /// How far past its place its margin box extends on each edge, top
    /// right bottom left.
    pub(crate) by: [f32; 4],
    /// With `align`, how far it moves to its place within that; `None`
    /// extends it instead.
    pub(crate) offset: Option<(f32, f32)>,
    /// The edges where its padding stays whole: there the band's filling
    /// it moved the box clear of the area, as where the box's own size
    /// places the edge it takes, so that it took the band, gave it back and
    /// touched again in one solve.
    pub(crate) whole: [bool; 4],
    /// Its margin edges as its parent placed it when it was measured: an edge
    /// kept whole stays so only while its place holds.
    pub(crate) at: [f32; 4],
}

impl Taken {
    /// What `slot` takes, `by`, placed as its `align` says.
    pub(crate) fn of(
        arena: &NodeArena,
        slot: u32,
        by: [f32; 4],
        whole: [bool; 4],
        at: [f32; 4],
    ) -> Self {
        let [top, right, bottom, left] = by;
        let offset = alignment(arena, slot)
            .map(|(x, y)| (x * (left + right) - left, y * (top + bottom) - top));
        Taken {
            by,
            offset,
            whole,
            at,
        }
    }

    /// What it adds to the box's frame, x y width height, and to the
    /// origin its children are placed from, x y.
    pub(crate) fn frame(&self) -> [f32; 4] {
        let [top, right, bottom, left] = self.by;
        match self.offset {
            Some((x, y)) => [x, y, 0.0, 0.0],
            None => [-left, -top, left + right, top + bottom],
        }
    }

    /// How far an extended box's scroll origin, its padding box's start,
    /// moved out, across and down: the engine measures its content from
    /// where its parent places it (Taffy patch 80), and its own extent
    /// counts from its extended box.
    pub(crate) fn origin(&self, arena: &NodeArena, slot: u32) -> (f32, f32) {
        let [top, right, _, left] = self.by;
        match (self.offset, rtl(arena, slot)) {
            (Some(_), _) => (0.0, 0.0),
            (None, false) => (left, top),
            (None, true) => (right, top),
        }
    }
}

/// Whether `style` takes the container region on some edge.
pub(crate) fn ignores(style: &StyleProps) -> bool {
    let bits = style.rare.safe_area_ignore;
    let regions = bit(SafeAreaIgnore::Container) | bit(SafeAreaIgnore::Keyboard);
    bits != 0 && (bits & regions == 0 || bits & bit(SafeAreaIgnore::Container) != 0)
}

/// `slot`'s part of its engine style inside the root. A box that ignores the
/// container region is laid out as `display: flow-root` and gives its parent
/// no baseline, so that its children's margins stay inside the box it may
/// extend and a parent aligning by baselines does not place it by content
/// the extension moves. An extended box carries what it takes, and its
/// padding there is what the band leaves of it, except on an edge that keeps
/// it whole: its subtree is laid out with the whole padding (Taffy patch 80),
/// its place with the rest.
pub(crate) fn lay_out(arena: &NodeArena, slot: u32, s: &mut taffy::Style) {
    if !arena.ignoring.contains(slot) || super::block::owns(arena, slot) {
        return;
    }
    if s.display == taffy::Display::Block {
        s.display = taffy::Display::FlowRoot;
    }
    s.rare.no_baseline = true;
    let taken = arena.observations.taken(slot);
    if let Some((x, y)) = taken.and_then(|t| t.offset) {
        s.rare.shift = taffy::geometry::Point { x, y };
    }
    if let Some(Taken {
        by,
        offset: None,
        whole,
        ..
    }) = taken
    {
        let [top, right, bottom, left] = by;
        s.rare.outset = taffy::geometry::Rect {
            top,
            right,
            bottom,
            left,
        };
        s.rare.outset_padding = s.padding;
        let less = |side, by: f32, whole: bool| match by > 0.0 && !whole {
            true => crate::style::less(side, by),
            false => side,
        };
        let (p, [t, r, b, l]) = (s.padding, whole);
        s.padding = taffy::geometry::Rect {
            top: less(p.top, top, t),
            right: less(p.right, right, r),
            bottom: less(p.bottom, bottom, b),
            left: less(p.left, left, l),
        };
    }
}

/// `inset`, that of `slot`, which ignores, as its parent placed it, with
/// the padding its subtree was laid out with where it extends: what its own
/// content sits inside.
pub(crate) fn inset(
    arena: &NodeArena,
    tree: &crate::layout::LayoutTree,
    slot: u32,
    node: taffy::NodeId,
    inset: crate::id::BoxInset,
) -> crate::id::BoxInset {
    let extends = (arena.observations.taken(slot)).is_some_and(|t| t.offset.is_none());
    match extends.then(|| tree.outset_padding(node)).flatten() {
        Some(p) => {
            let (padding, cover) = ([p.left, p.top, p.right, p.bottom], inset.cover);
            crate::id::BoxInset {
                padding: std::array::from_fn(|i| (padding[i] - cover[i]).max(0.0)),
                ..inset
            }
        }
        None => inset,
    }
}

/// `frame` and `content`, `slot`'s as its layout placed it, where what it
/// takes puts them, and how far that moves the origin its children are
/// placed from.
pub(crate) fn place(
    arena: &NodeArena,
    slot: u32,
    frame: crate::Frame,
    content: (f32, f32),
) -> (crate::Frame, (f32, f32), (f32, f32)) {
    let Some(taken) = arena.observations.taken(slot) else {
        return (frame, (0.0, 0.0), content);
    };
    let [x, y, width, height] = taken.frame();
    let (across, down) = taken.origin(arena, slot);
    let frame = crate::Frame {
        x: frame.x + x,
        y: frame.y + y,
        width: frame.width + width,
        height: frame.height + height,
    };
    (frame, (x, y), (content.0 + across, content.1 + down))
}

/// The edges, top right bottom left, on which `slot` takes `region`'s
/// depth: naming no region takes both, naming no edge takes every edge, and
/// `start` and `end` are the box's inline start and end under its
/// `direction`.
pub(crate) fn edges(arena: &NodeArena, slot: u32, region: SafeAreaIgnore) -> [bool; 4] {
    let bits = arena.style(slot).rare.safe_area_ignore;
    let has = |v: SafeAreaIgnore| bits & bit(v) != 0;
    let named = [SafeAreaIgnore::Container, SafeAreaIgnore::Keyboard];
    if bits == 0 || (named.into_iter().any(has) && !has(region)) {
        return [false; 4];
    }
    // Read the direction only for a logical edge
    let logical = has(SafeAreaIgnore::Start) || has(SafeAreaIgnore::End);
    let (right, left) = match logical && rtl(arena, slot) {
        false => (SafeAreaIgnore::End, SafeAreaIgnore::Start),
        true => (SafeAreaIgnore::Start, SafeAreaIgnore::End),
    };
    let edges = [
        has(SafeAreaIgnore::Top),
        has(SafeAreaIgnore::Right) || has(right),
        has(SafeAreaIgnore::Bottom),
        has(SafeAreaIgnore::Left) || has(left),
    ];
    match edges.contains(&true) {
        true => edges,
        false => [true; 4],
    }
}

/// Where `slot`'s `align` puts its margin box within what it takes, as
/// fractions of the room across and down, or `None` without one. SwiftUI's
/// `Alignment`: `top` is centred across, `leading` centred down, and
/// `leading` and `trailing` follow the box's `direction`.
fn alignment(arena: &NodeArena, slot: u32) -> Option<(f32, f32)> {
    use SafeAreaIgnoreAlign as A;
    let align = arena.style(slot).rare.safe_area_ignore_align;
    let across = |trailing: bool| match trailing != rtl(arena, slot) {
        true => 1.0,
        false => 0.0,
    };
    Some(match align {
        A::None => return None,
        A::Center => (0.5, 0.5),
        A::Top => (0.5, 0.0),
        A::Bottom => (0.5, 1.0),
        A::Leading => (across(false), 0.5),
        A::Trailing => (across(true), 0.5),
        A::TopLeading => (across(false), 0.0),
        A::TopTrailing => (across(true), 0.0),
        A::BottomLeading => (across(false), 1.0),
        A::BottomTrailing => (across(true), 1.0),
    })
}

fn rtl(arena: &NodeArena, slot: u32) -> bool {
    let style = arena.computed_source(slot, crate::StyleId::Direction);
    style.direction == Direction::Rtl
}

/// `v`'s bit in the `safe_area_ignore` row: one for each value after
/// `none`, in order (`kernel/build/codec.rs`), without the name lookups of
/// `keyword_bits`, since a layout reads it for every block.
fn bit(v: SafeAreaIgnore) -> u8 {
    let bit = 1 << (v as u8 - 1);
    debug_assert_eq!(bit, v.keyword_bits());
    bit
}
