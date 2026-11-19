//! Taking a region's depth (`-exact-safe-area-ignore`, LLP 1123 D4). On an
//! owner's root its host leaves the edge uncleared (D3). Inside the root, a
//! box whose margin box touches the area its parent passes it, on an edge it
//! names, takes that area to its outer side there. The area fills its
//! padding there first, and its parent places it with what is left of the
//! padding, otherwise as though it did not touch; where filling the padding
//! would move it clear of the area, as where its own size places that edge,
//! the padding stays whole. Its subtree is laid out in its box outset by
//! what it takes (the engine's `outset`), or, with `align`, it keeps its
//! size and is moved within that. The readers' solve (`observe.rs`) measures
//! what each box takes from the frames in its rounds, through the part here:
//! which boxes are due (`takers_due`), what each takes (`taking`), and the
//! layout again of those that took more or less (`check_taken`).
use super::area::{area, band_at};
use super::observe::{in_branches, within};
use super::Kernel;
use crate::arena::NodeArena;
use crate::generated::{SafeAreaIgnore, SafeAreaIgnoreAlign, StyleProps};
use crate::id::{IdMap, IdSet};
use crate::layout::{LayoutMirror, LayoutReceipt};

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
        match (self.offset, super::owner::rtl(arena, slot)) {
            (Some(_), _) => (0.0, 0.0),
            (None, false) => (left, top),
            (None, true) => (right, top),
        }
    }
}

/// What the boxes that ignore take, and what the solve needs to measure it
/// again only where it may have moved (D4).
#[derive(Debug, Clone, Default)]
pub(crate) struct Takings {
    /// What each box that ignores and touches takes past its place.
    taken: IdMap<u32, Taken>,
    /// Each box that ignores's used margins as its layout last placed it, an
    /// `auto` one 0, from whichever tree laid it out.
    margins: IdMap<u32, [f32; 4]>,
    /// The edges each box that ignores gave the band back on in this solve.
    pub(super) released: IdMap<u32, [bool; 4]>,
    /// The boxes that ignore whose taking is current: their frames, their
    /// styles and the area above them held since they were measured, so the
    /// solve measures them again only when one of those moves.
    pub(super) measured: IdSet<u32>,
}

impl Takings {
    pub(super) fn taken(&self, slot: u32) -> Option<Taken> {
        match self.taken.is_empty() {
            true => None,
            false => self.taken.get(&slot).copied(),
        }
    }

    pub(super) fn forget(&mut self, slot: u32) {
        self.taken.remove(&slot);
        self.margins.remove(&slot);
        self.measured.remove(&slot);
    }

    pub(super) fn set_margins(&mut self, slot: u32, margins: [f32; 4]) {
        // Measure it again where its margins moved and its frame need not
        if self.margins.insert(slot, margins) != Some(margins) {
            self.measured.remove(&slot);
        }
    }
}

impl Kernel {
    /// The boxes under `root` that ignore whose taking may have moved since
    /// they were measured: those whose frames `receipt`'s layout moved and
    /// those not measured since what they take from changed. In a content
    /// region, whose passes lay its branches out apart from the receipt,
    /// every one.
    pub(super) fn takers_due(&self, root: u32, receipt: &LayoutReceipt) -> Vec<u32> {
        let arena = &self.arena;
        if arena.ignoring.is_empty() {
            return Vec::new();
        }
        let measured = &arena.observations.takings.measured;
        let mut due: Vec<u32> = match self.region.is_some() {
            true => arena.ignoring.iter().collect(),
            false => (receipt.changed.iter())
                .map(|k| k.index)
                .filter(|&s| arena.ignoring.contains(s))
                .collect(),
        };
        // Skip the walk over every box that ignores while each is measured
        if measured.len() < arena.ignoring.len() {
            due.extend(arena.ignoring.iter().filter(|s| !measured.contains(s)));
        }
        due.sort_unstable();
        due.dedup();
        due.retain(|&s| within(arena, s, root));
        due
    }

    /// Measure what each box in `due` takes (D4), keep each measurement
    /// where `keep`, and return the boxes whose frames it moves by more than
    /// half a device pixel.
    pub(super) fn check_taken(&mut self, due: &[u32], keep: bool) -> Vec<u32> {
        if due.is_empty() {
            return Vec::new();
        }
        let arena = &self.arena;
        let tolerance = arena.observations.tolerance();
        // Two agree where both extend or both move the box, by frames within tolerance
        let near = |a: Option<Taken>, b: Option<Taken>| match (a, b) {
            (Some(a), Some(b)) => {
                let (x, y) = (a.frame(), b.frame());
                a.offset.is_some() == b.offset.is_some()
                    && (0..4).all(|e| (x[e] - y[e]).abs() <= tolerance)
            }
            (a, b) => a == b,
        };
        let measured: Vec<(u32, Option<Taken>, bool)> = (due.iter())
            .map(|&s| {
                let (t, held) = taking(self, s);
                (s, t, held)
            })
            .collect();
        // One within tolerance of what it took keeps that, so it does not drift
        let changed: Vec<(u32, Option<Taken>)> = (measured.iter())
            .filter(|&&(s, t, _)| !near(arena.observations.taken(s), t))
            .map(|&(s, t, _)| (s, t))
            .collect();
        let observations = &mut self.arena.observations;
        for &(s, _, held) in &measured {
            match held && keep {
                true => observations.takings.measured.insert(s),
                false => observations.takings.measured.remove(&s),
            };
        }
        if keep {
            for &(s, t) in &changed {
                if let Some(was) = observations.takings.taken.get(&s) {
                    let gave: [bool; 4] = std::array::from_fn(|e| {
                        was.by[e] > tolerance && t.is_none_or(|t| t.by[e] <= tolerance)
                    });
                    let released = observations.takings.released.entry(s).or_default();
                    *released = std::array::from_fn(|e| released[e] || gave[e]);
                }
                match t {
                    Some(t) => observations.takings.taken.insert(s, t),
                    None => observations.takings.taken.remove(&s),
                };
            }
        }
        changed.into_iter().map(|(s, _)| s).collect()
    }

    /// Let the band fill again the padding each box kept whole where its
    /// parent is to be laid out anew, since what moved the box clear may have
    /// changed, so that the layout reaches what a fresh one of the tree does.
    /// A content region's branches are laid out by trees of their own, which
    /// it builds afresh, so a box in one is tested again only where its place
    /// moves (`taking`).
    pub(super) fn reopen_whole(&mut self) {
        let taken = &self.arena.observations.takings.taken;
        if !taken.values().any(|t| t.whole.contains(&true)) {
            return;
        }
        let Some(tree) = self.layout.as_deref().and_then(LayoutMirror::tree_ref) else {
            return;
        };
        let arena = &self.arena;
        let branches =
            (self.region.as_ref()).map(|r| [r.binding.content.index, r.binding.pending.index]);
        let laid_anew = |slot: u32| {
            if branches.is_some_and(|b| in_branches(arena, b, slot)) {
                return false;
            }
            let mut at = arena.parent(slot);
            while let Some(p) = at {
                if let Some(node) = arena.taffy(p) {
                    return tree.is_dirty(node);
                }
                at = arena.parent(p);
            }
            true
        };
        let reopened: Vec<u32> = (taken.iter())
            .filter(|(_, t)| t.whole.contains(&true))
            .map(|(&s, _)| s)
            .filter(|&s| laid_anew(s))
            .collect();
        for &s in &reopened {
            if let Some(t) = self.arena.observations.takings.taken.get_mut(&s) {
                t.whole = [false; 4];
            }
        }
        self.restyle_measured(&reopened);
    }

    /// Lay out and publish again each box whose engine style carries a
    /// measurement that moved: what a box that ignores takes, which
    /// publication places where it moves the box, or what a parent of bars
    /// observes past which their slots begin (D12).
    pub(super) fn restyle_measured(&mut self, boxes: &[u32]) {
        if boxes.is_empty() {
            return;
        }
        if let Some(r) = &mut self.region {
            r.invalidate();
        }
        let mut unmirrored = crate::layout::Unmirrored;
        let layout = self.layout.as_deref_mut().unwrap_or(&mut unmirrored);
        for &s in boxes {
            self.arena.layout_dirty.insert(s);
            if let Some(node) = self.arena.taffy(s) {
                layout.restyle(&self.arena, s, node);
            }
        }
    }
}

/// What `slot`, which ignores, takes past its place on the layout just made
/// (D4): on each edge it names, where its margin box, an `auto` margin
/// counting 0, meets the inner side of the area its parent passes it or
/// lies inside the area, that area to its outer side, `None` where it
/// takes nothing; and whether that holds while its frame, its style and the
/// area above it do, which it does not in a top-layer box, whose block can
/// move in the viewport.
fn taking(kernel: &Kernel, slot: u32) -> (Option<Taken>, bool) {
    let arena = &kernel.arena;
    let hidden = arena.flags(slot).has(crate::id::NodeFlags::HIDDEN);
    if hidden
        || arena.taffy(slot).is_none()
        || arena.is_inline_run(slot)
        || super::block::owns(arena, slot)
    {
        return (None, true);
    }
    let space = band_at(kernel, slot);
    // One in a top-layer box can move in the viewport without its frame
    let held = space.owner.is_some() || space.root.is_none();
    let Some(band) = space.band else {
        return (None, held);
    };
    let sides = area(arena, slot, &space, band);
    let named = edges(arena, slot, SafeAreaIgnore::Container);
    let layer = super::keyboard::layer_at(arena, slot, &space);
    let keyboard = layer > 0.0 && edges(arena, slot, SafeAreaIgnore::Keyboard)[2];
    // Its edges where its parent placed it: its frame less what it took
    let stored = arena.observations.taken(slot);
    let [top, right, bottom, left] = space.edges(arena, slot);
    let [x, y, width, height] = stored.map_or([0.0; 4], |t| t.frame());
    let placed = [top - y, right + x + width, bottom + y + height, left - x];
    let margins = (arena.observations.takings.margins.get(&slot)).map_or([0.0; 4], |m| *m);
    let tolerance = arena.observations.tolerance();
    let at: [f32; 4] = std::array::from_fn(|e| placed[e] - margins[e]);
    let by: [f32; 4] = std::array::from_fn(|e| {
        let [outer, inner] = sides[e];
        // The keyboard layer lies on the bottom edge alone
        let reach = match e == 2 && layer > 0.0 {
            true => past_keyboard(at[e], sides[e], layer, [named[e], keyboard], tolerance),
            false if named[e] && at[e] <= inner + tolerance => outer,
            false => at[e],
        };
        (at[e] - reach).max(0.0)
    });
    // An edge it took, gave back and touches again in one solve keeps its
    // padding whole, since filling it moved the box clear, until its place
    // there moves
    let released = (arena.observations.takings.released.get(&slot)).map_or([false; 4], |r| *r);
    let whole: [bool; 4] = std::array::from_fn(|e| {
        let kept = stored.is_some_and(|t| t.whole[e] && (t.at[e] - at[e]).abs() <= tolerance);
        by[e] > tolerance && (released[e] || kept)
    });
    let taken = (by.iter().any(|&b| b > tolerance)).then(|| Taken::of(arena, slot, by, whole, at));
    (taken, held)
}

/// How far in from the space's edge a bottom edge `at` in reaches over the
/// keyboard layer, which runs `layer` in from the allocation's edge (D4's
/// keyboard region, D9). Going outward, the edge passes the container area
/// `[outer, inner]` where it touches it and the box ignores the container
/// region, and otherwise stops there. Touching the layer, it then stops at
/// the keyboard's top where it observes the keyboard, and otherwise passes
/// it, to the container base's inner side where it observes the container
/// region and to the allocation's edge where it ignores both. It never moves
/// inward of `at`: an ignore only extends a box.
fn past_keyboard(
    at: f32,
    [outer, inner]: [f32; 2],
    layer: f32,
    [container, keyboard]: [bool; 2],
    tolerance: f32,
) -> f32 {
    let touches = at <= inner + tolerance;
    if touches && !container {
        return at;
    }
    let reach = if touches { outer.min(at) } else { at };
    if reach > layer + tolerance {
        return reach;
    }
    match (keyboard, container) {
        (false, _) => at.min(reach.max(layer)),
        (true, true) => 0.0,
        (true, false) => reach.min(inner),
    }
}

/// Whether `style` takes a region on some edge (D4): the container region,
/// or the keyboard's, whose layer a box takes under `resizes-content`.
pub(crate) fn ignores(style: &StyleProps) -> bool {
    style.rare.safe_area_ignore != 0
}

/// `slot`'s part of its engine style inside the root. A box that ignores a
/// region is laid out as `display: flow-root` and gives its parent
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
    let (right, left) = match logical && super::owner::rtl(arena, slot) {
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
    let across = |trailing: bool| match trailing != super::owner::rtl(arena, slot) {
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

/// `v`'s bit in the `safe_area_ignore` row: one for each value after
/// `none`, in order (`kernel/build/codec.rs`), without the name lookups of
/// `keyword_bits`, since a layout reads it for every block.
fn bit(v: SafeAreaIgnore) -> u8 {
    let bit = 1 << (v as u8 - 1);
    debug_assert_eq!(bit, v.keyword_bits());
    bit
}
