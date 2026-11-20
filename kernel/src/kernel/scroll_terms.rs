//! A scroller's terms for its host's one writer (LLP 1123 D13, D14): on each edge, how far its
//! native view reaches past its laid-out box, and, measured from that reached edge, the depth
//! of what Exact reserves there and of the keyboard it takes, with whether it observes the
//! container region there and whether the band reaches it. The writer takes the larger of the
//! container and keyboard terms the scroller observes, the container's being `reserved` where
//! Exact reserves and elsewhere the scroller's native band where the band reaches it, and writes
//! it less the platform's adjustment, at least 0 where it observes the container region, Exact
//! reserves nothing and the band reaches it (D14).
use super::area::{self, Space};
use super::block::reservation;
use super::{block, ignore, keyboard};
use crate::arena::NodeArena;
use crate::generated::{SafeAreaEdges, SafeAreaIgnore, SafeAreaReach};
use crate::{Kernel, NodeKey, Overflow, StyleId};

/// A scroller's terms, each top right bottom left, in points in its layout's space: its owner's,
/// or the viewport outside every owner.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ScrollTerms {
    /// How far its native view reaches past its laid-out box on each edge (D13): to the outer
    /// side of the area that reaches it, where its border edge meets or lies inside that area
    /// and its reach names the edge, no further than the furthest in of the padding edges of
    /// the ancestors that stop reach there, and under `resizes-content` no further down than the
    /// keyboard's top, where the content root ends.
    pub reach: [f32; 4],
    /// D14's `reserved` term where Exact reserves on that edge, its ancestors' additional insets
    /// and safe-area padding (D11), the bar slots stacked there (D12), or an owner's root's own:
    /// from the reached edge to the inner side of what is reserved, the band it lies past
    /// included. It replaces the native band there, as it holds Exact's writes and a negative
    /// inset's reduction, which UIKit's band may not; `None` where nothing is reserved.
    pub reserved: [Option<f32>; 4],
    /// D14's `keyboard` term: under `resizes-visual`, the docked keyboard's overlap with the
    /// reached box on the bottom edge, where the scroller takes it (D7); 0 elsewhere.
    pub keyboard: [f32; 4],
    /// Whether the band of its space, its owner's or the session's, reaches it on each edge past
    /// every scroller between it and its layout's root, the root included (D9). A scroller above
    /// takes the band on each edge except one it ignores where its box reaches the area, as UIKit
    /// gives a scroll view's content none of an edge it adjusts. Where it is false the band is no
    /// term, and the writer cancels the platform's adjustment there. False on every edge while
    /// the band is unavailable.
    pub band_reaches: [bool; 4],
    /// Whether it observes the container region on each edge, which with `reserved` picks D14's
    /// equation there; an owner's root observes its own reservations on an edge it takes, as D4
    /// applies them after its ignore.
    pub container: [bool; 4],
}

impl Kernel {
    /// The terms of the scroller `key` as last laid out (D13, D14). `None` when it is not a
    /// scroller, not live or not laid out, or lies where the kernel is not told (a painted
    /// `position-area` popover). A band or a keyboard can move them with no box moving, and a
    /// layout with no commit or report can, so the writer reads them after each layout, and after
    /// each commit and each owner or session report that owes none. While the band is
    /// unavailable, out of a window, no scroller reaches or has a `reserved` term, an owner's
    /// root that scrolls included.
    pub fn scroll_terms(&self, key: NodeKey) -> Option<ScrollTerms> {
        let arena = &self.arena;
        let slot = arena.resolve(key)?;
        if !area::scroll_view(arena, slot) {
            return None;
        }
        self.laid_out_frame(key)?;
        let space = area::band_at(self, slot);
        space.placed().then(|| terms(arena, slot, &space))
    }
}

fn terms(arena: &NodeArena, slot: u32, space: &Space) -> ScrollTerms {
    let ignored = ignore::edges(arena, slot, SafeAreaIgnore::Container);
    let mut container = ignored.map(|ignores| !ignores);
    let Some(band) = space.band else {
        let keyboard = [0.0, 0.0, keyboard::taken(arena, slot, space, 0.0), 0.0];
        return ScrollTerms {
            keyboard,
            container,
            ..Default::default()
        };
    };
    let band_reaches = area::open_edges(arena, slot, space, band);
    let (mut sides, mut reserving) = area::area_reserving(arena, slot, space, band);
    // Take what its host clears as the area of an owner's root that reserves, as a controller's
    // view lies under its own additional insets
    if space.owner == Some(slot) {
        let clear = block::clearance(arena, slot);
        if let Some(r) = reservation::reserved(arena, slot) {
            for e in (0..4).filter(|&e| r.added[e] != 0.0 || r.padding[e] != 0.0) {
                (sides[e], reserving[e], container[e]) = ([0.0, clear[e]], true, true);
            }
        }
    }
    let (edges, tolerance) = (space.edges(arena, slot), arena.observations.tolerance());
    let named = reached(arena, slot);
    let touches: [bool; 4] =
        std::array::from_fn(|e| named[e] && edges[e] <= sides[e][1] + tolerance);
    // Walk the ancestors only where an edge would reach past the scroller
    let stops = match (0..4).any(|e| touches[e] && edges[e] > sides[e][0]) {
        true => stops(arena, slot, space),
        false => [0.0; 4],
    };
    // Stop reach at the keyboard's top under `resizes-content`, where the content root ends
    let cover = space
        .owner
        .map_or([0.0; 4], |owner| keyboard::cover(arena, owner));
    let reach: [f32; 4] = std::array::from_fn(|e| match touches[e] {
        true => (edges[e] - sides[e][0].max(cover[e]).max(stops[e])).max(0.0),
        false => 0.0,
    });
    let reserved = std::array::from_fn(|e| {
        reserving[e].then(|| (sides[e][1] - (edges[e] - reach[e])).max(0.0))
    });
    let keyboard = [0.0, 0.0, keyboard::taken(arena, slot, space, reach[2]), 0.0];
    ScrollTerms {
        reach,
        reserved,
        keyboard,
        band_reaches,
        container,
    }
}

/// The edges `slot`'s `-exact-safe-area-reach` reaches on, top right bottom left: every edge
/// under `auto`, a scroller's default, else the edges it names, `start` and `end` resolved
/// against its `direction` (D13).
fn reached(arena: &NodeArena, slot: u32) -> [bool; 4] {
    let style = arena.style(slot);
    if style.rare.safe_area_reach == SafeAreaReach::Auto {
        return [true; 4];
    }
    let bits = style.rare.safe_area_reach_edges;
    let has = |v: SafeAreaEdges| bits & v.keyword_bits() != 0;
    let (right, left) = match super::owner::rtl(arena, slot) {
        false => (SafeAreaEdges::End, SafeAreaEdges::Start),
        true => (SafeAreaEdges::Start, SafeAreaEdges::End),
    };
    [
        has(SafeAreaEdges::Top),
        has(SafeAreaEdges::Right) || has(right),
        has(SafeAreaEdges::Bottom),
        has(SafeAreaEdges::Left) || has(left),
    ]
}

/// How far in from the space's edges reach may go at most, top right bottom left: the furthest
/// in of the padding edges of the ancestors between `slot` and its layout's root, the root
/// included, that clip on that edge's axis, have a border on that edge, or are drawn through a
/// transform, a press scale or translation, a perspective, a filter, a clip path or a mask (D13).
fn stops(arena: &NodeArena, slot: u32, space: &Space) -> [f32; 4] {
    let mut limit = [0.0f32; 4];
    let mut above = (space.root != Some(slot))
        .then(|| arena.parent(slot))
        .flatten();
    while let Some(s) = above {
        let (style, m) = (arena.style(s), &arena.style(s).mask);
        let whole = (m.has(StyleId::Transform) && !style.transform.0.is_empty())
            || m.has(StyleId::Translate)
            || m.has(StyleId::Scale)
            || m.has(StyleId::Rotate)
            || (m.has(StyleId::PressScale) && style.press_scale != 1.0)
            || m.has(StyleId::PressTranslate)
            || (m.has(StyleId::Perspective) && style.perspective > 0.0)
            || (m.has(StyleId::Filter) && !style.filter.is_none())
            || (m.has(StyleId::BackdropFilter) && !style.backdrop_filter.is_none())
            || (m.has(StyleId::ClipPath)
                && style.rare.clip_path != crate::clip::ClipPath::default())
            || (m.has(StyleId::MaskImage)
                && style.rare.mask_image != crate::gradient::BackgroundImage::default());
        let (x, y) = area::overflow(arena, s);
        let (clips_x, clips_y) = (x != Overflow::Visible, y != Overflow::Visible);
        let border = area::border(arena, s);
        let stop: [bool; 4] = std::array::from_fn(|e| {
            whole || border[e] > 0.0 || if e % 2 == 0 { clips_y } else { clips_x }
        });
        if stop.contains(&true) {
            let padding = space.padding_edges(arena, s);
            for e in (0..4).filter(|&e| stop[e]) {
                limit[e] = limit[e].max(padding[e]);
            }
        }
        above = (space.root != Some(s)).then(|| arena.parent(s)).flatten();
    }
    limit
}
