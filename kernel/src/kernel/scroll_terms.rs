//! A scroller's terms for its host's one writer (LLP 1123 D13, D14): on each edge, how far its
//! native view reaches past its laid-out box, and, measured from that reached edge, the depth
//! of what Exact reserves there and of the keyboard it takes, with whether it observes the
//! container region there and whether the band reaches it. The writer takes the larger of the
//! container and keyboard terms the scroller observes, the container's being `reserved` where
//! Exact reserves and elsewhere the scroller's native band where the band reaches it, and writes
//! it less the platform's adjustment, at least 0 where it observes the container region, Exact
//! reserves nothing and the band reaches it (D14). And a box's extension (D13): how far its
//! picture is mirrored past its border box, `auto` going as far as reach would.
use super::area::{self, Space};
use super::block::reservation;
use super::{block, ignore, keyboard};
use crate::arena::NodeArena;
use crate::generated::{SafeAreaEdges, SafeAreaExtend, SafeAreaIgnore, SafeAreaReach, StyleProps};
use crate::style::Dimension;
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

    /// How far the box `key`, no scroller, reaches past its laid-out border box on each edge, top
    /// right bottom left, in points in its layout's space (D13): on each edge
    /// `-exact-safe-area-reach` names, or under `auto` where it paints a plain colour fill, to the
    /// outer side of the area that reaches it where its border edge meets or lies inside that area,
    /// no further than reach's stops; 0 elsewhere, and while the band is unavailable. Its layout,
    /// its observations and its geometry keep its laid-out box. `None` for a scroller, whose
    /// terms carry its reach, and for a box not live, not laid out or where the kernel is not told.
    pub fn box_reach(&self, key: NodeKey) -> Option<[f32; 4]> {
        let arena = &self.arena;
        let slot = arena.resolve(key)?;
        if area::scroll_view(arena, slot) {
            return None;
        }
        self.laid_out_frame(key)?;
        let space = area::band_at(self, slot);
        if !space.placed() {
            return None;
        }
        let named = box_reached(arena, slot);
        let Some(band) = space.band.filter(|_| named.contains(&true)) else {
            return Some([0.0; 4]);
        };
        let sides = sides(arena, slot, &space, band, None).0;
        Some(reach_on(arena, slot, &space, &sides, named))
    }

    /// How far the box `key`'s picture is mirrored past its laid-out border box on each edge, top
    /// right bottom left (D13's extension), in points in its layout's space. On an edge it extends
    /// on, `auto` is the distance reach would have there were the box to reach that edge, whatever
    /// its `-exact-safe-area-reach`, and 0 while the band is unavailable; a length is taken as
    /// given and a calculation below 0 counts as 0. 0 on an edge it does not extend on. `None` when
    /// it extends on no edge or is not live or not laid out.
    pub fn extension(&self, key: NodeKey) -> Option<[f32; 4]> {
        let arena = &self.arena;
        let slot = arena.resolve(key)?;
        let named = extended(arena, slot);
        if !named.contains(&true) {
            return None;
        }
        self.laid_out_frame(key)?;
        let space = area::band_at(self, slot);
        let auto = match space.band.filter(|_| space.placed()) {
            Some(band) => reach_on(
                arena,
                slot,
                &space,
                &sides(arena, slot, &space, band, None).0,
                named,
            ),
            None => [0.0; 4],
        };
        let style = arena.style(slot);
        let env = super::owner::env_for(arena, slot, style);
        let rare = &style.rare;
        let bounds = [
            &rare.safe_area_extend_bounds_top,
            &rare.safe_area_extend_bounds_right,
            &rare.safe_area_extend_bounds_bottom,
            &rare.safe_area_extend_bounds_left,
        ];
        // A bound that resolves to `auto`, through a fallback too, is `auto`
        Some(std::array::from_fn(|e| {
            match (named[e], bounds[e].resolve(&env)) {
                (false, _) => 0.0,
                (true, Dimension::Auto) => auto[e],
                (true, d) => d.at_basis(&env, 0.0).max(0.0),
            }
        }))
    }
}

pub(super) fn terms(arena: &NodeArena, slot: u32, space: &Space) -> ScrollTerms {
    terms_holding(arena, slot, space, None).0
}

/// [`terms`], with whether the reservation of the bar's slot `bar` lies in `slot`'s area
/// (`area::area_holding`).
pub(super) fn terms_holding(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    bar: Option<area::BarSlot>,
) -> (ScrollTerms, bool) {
    let ignored = ignore::edges(arena, slot, SafeAreaIgnore::Container);
    let mut container = ignored.map(|ignores| !ignores);
    let Some(band) = space.band else {
        let keyboard = [0.0, 0.0, keyboard::taken(arena, slot, space, 0.0), 0.0];
        let terms = ScrollTerms {
            keyboard,
            container,
            ..Default::default()
        };
        return (terms, false);
    };
    let band_reaches = area::open_edges(arena, slot, space, band);
    let (sides, reserving, roots, held) = sides(arena, slot, space, band, bar);
    for e in (0..4).filter(|&e| roots[e]) {
        container[e] = true;
    }
    let reach = reach_on(arena, slot, space, &sides, reached(arena, slot));
    let edges = space.edges(arena, slot);
    let reserved = std::array::from_fn(|e| {
        reserving[e].then(|| (sides[e][1] - (edges[e] - reach[e])).max(0.0))
    });
    let keyboard = [0.0, 0.0, keyboard::taken(arena, slot, space, reach[2]), 0.0];
    let terms = ScrollTerms {
        reach,
        reserved,
        keyboard,
        band_reaches,
        container,
    };
    (terms, held)
}

/// The area that reaches `slot` on each edge, its outer and inner sides from the space's edges,
/// and whether Exact reserves there; for an owner's root that scrolls and reserves, what its host
/// clears, as a controller's view lies under its own additional insets, the third value naming
/// those edges. A root that does not scroll lies clear of its own reservations. The fourth is
/// whether the reservation of `bar`'s slot lies in the area.
fn sides(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    band: [f32; 4],
    bar: Option<area::BarSlot>,
) -> ([[f32; 2]; 4], [bool; 4], [bool; 4], bool) {
    let (mut sides, mut reserving, held) = area::area_holding(arena, slot, space, band, bar);
    let mut roots = [false; 4];
    if space.owner == Some(slot) && area::scroll_view(arena, slot) {
        let clear = block::clearance(arena, slot);
        if let Some(r) = reservation::reserved(arena, slot) {
            for e in (0..4).filter(|&e| r.added[e] != 0.0 || r.padding[e] != 0.0) {
                (sides[e], reserving[e], roots[e]) = ([0.0, clear[e]], true, true);
            }
        }
    }
    (sides, reserving, roots, held)
}

/// How far `slot` reaches past its border box on each edge `named` names (D13): to the outer side
/// of the area that reaches it, `sides`, where that area has depth and its border edge meets or
/// lies inside it within half a device pixel, no further than the ancestors' stops and, under
/// `resizes-content`, the keyboard's top, where the content root ends; 0 elsewhere.
fn reach_on(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
    sides: &[[f32; 2]; 4],
    named: [bool; 4],
) -> [f32; 4] {
    let tolerance = area::tolerance(arena);
    let edges = space.edges(arena, slot);
    let touches: [bool; 4] = std::array::from_fn(|e| {
        named[e] && sides[e][1] > sides[e][0] && edges[e] <= sides[e][1] + tolerance
    });
    // Walk the ancestors only where an edge would reach past the box
    let stops = match (0..4).any(|e| touches[e] && edges[e] > sides[e][0]) {
        true => stops(arena, slot, space),
        false => [0.0; 4],
    };
    let cover = space
        .owner
        .map_or([0.0; 4], |owner| keyboard::cover(arena, owner));
    std::array::from_fn(|e| match touches[e] {
        true => (edges[e] - sides[e][0].max(cover[e]).max(stops[e])).max(0.0),
        false => 0.0,
    })
}

/// The edges `slot`'s `-exact-safe-area-extend` extends on, top right bottom left, `start` and
/// `end` resolved against its `direction` (D13).
fn extended(arena: &NodeArena, slot: u32) -> [bool; 4] {
    let bits = arena.style(slot).rare.safe_area_extend;
    let has = |v: SafeAreaExtend| bits & v.keyword_bits() != 0;
    let (right, left) = match super::owner::rtl(arena, slot) {
        false => (SafeAreaExtend::End, SafeAreaExtend::Start),
        true => (SafeAreaExtend::Start, SafeAreaExtend::End),
    };
    [
        has(SafeAreaExtend::Top),
        has(SafeAreaExtend::Right) || has(right),
        has(SafeAreaExtend::Bottom),
        has(SafeAreaExtend::Left) || has(left),
    ]
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
        let whole = drawn_through(arena.style(s));
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

/// Whether `style` draws its box through a transform, a translation, a scale, a rotation, a press
/// scale or translation, a perspective, a filter, a backdrop filter, a clip path or a mask (D13).
fn drawn_through(style: &StyleProps) -> bool {
    let m = &style.mask;
    (m.has(StyleId::Transform) && !style.transform.0.is_empty())
        || m.has(StyleId::Translate)
        || m.has(StyleId::Scale)
        || m.has(StyleId::Rotate)
        || (m.has(StyleId::PressScale) && style.press_scale != 1.0)
        || m.has(StyleId::PressTranslate)
        || (m.has(StyleId::Perspective) && style.perspective > 0.0)
        || (m.has(StyleId::Filter) && !style.filter.is_none())
        || (m.has(StyleId::BackdropFilter) && !style.backdrop_filter.is_none())
        || (m.has(StyleId::ClipPath) && style.rare.clip_path != crate::clip::ClipPath::default())
        || (m.has(StyleId::MaskImage)
            && style.rare.mask_image != crate::gradient::BackgroundImage::default())
}

/// The edges a non-scrolling box reaches on (D13): those `-exact-safe-area-reach` names, or under
/// `auto` every edge where it paints a plain fill. None for a box whose edge is its host's drawing
/// (a text, a replaced box, a control), one that clips its overflow or one drawn through what stops
/// reach, which show no strip of an extension either.
fn box_reached(arena: &NodeArena, slot: u32) -> [bool; 4] {
    let (x, y) = area::overflow(arena, slot);
    let style = arena.style(slot);
    if arena.node_type(slot) != crate::NodeType::View
        || x != Overflow::Visible
        || y != Overflow::Visible
        || drawn_through(style)
    {
        return [false; 4];
    }
    // An edge it extends on has its strip drawn by the extension's mirror (D13)
    let extended = extended(arena, slot);
    let named = match style.rare.safe_area_reach == SafeAreaReach::Auto {
        true => [plain_fill(arena, slot); 4],
        false => reached(arena, slot),
    };
    std::array::from_fn(|e| named[e] && !extended[e])
}

/// Whether `slot` paints a plain colour fill (D13): a background colour that paints in either
/// appearance and fills a rectangle undecorated by a background image, a border, a radius, a
/// shadow or a material, as a SwiftUI colour's background ignores the safe area and a UIKit view's
/// background colour fills its bounds.
fn plain_fill(arena: &NodeArena, slot: u32) -> bool {
    use crate::style::ColorValue;
    let s = arena.style(slot);
    // A colour paints in either appearance, which the host picks: `currentColor`, a role and a
    // platform colour do
    let coloured = match s.background_color {
        Some(ColorValue::Fixed(c)) => c.a() > 0,
        Some(ColorValue::LightDark(a, b)) => a.a() > 0 && b.a() > 0,
        Some(ColorValue::Wide(id)) => crate::style::wide::wide(id)
            .is_some_and(|w| w.half(false).alpha > 0.0 && w.half(true).alpha > 0.0),
        _ => true,
    };
    // It fills the border box, which has no border here, unless clipped to the content or text
    let fills = matches!(
        s.background_clip,
        crate::BackgroundClip::BorderBox | crate::BackgroundClip::PaddingBox
    );
    let radius = [
        &s.border_radius_top_left,
        &s.border_radius_top_right,
        &s.border_radius_bottom_right,
        &s.border_radius_bottom_left,
    ];
    let material = arena
        .props(slot)
        .str(crate::PropId::BackgroundMaterial)
        .is_some();
    coloured
        && fills
        && !material
        && s.border_widths().iter().all(|&w| w == 0.0)
        && radius
            .iter()
            .all(|r| matches!(r, Dimension::Points(p) if *p == 0.0))
        && s.background_image == Default::default()
        && s.box_shadow.0.is_empty()
}
