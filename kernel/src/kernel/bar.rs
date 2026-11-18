//! Bars (`-exact-safe-area-bar`, LLP 1123 D12): a child that names an edge is a bar of its
//! parent, laid out in a slot on that edge, out of the parent's flow (Taffy patch 81, which
//! measures and places it). Its slot lies past the parent's own reservations and the earlier
//! bars, and the bar reserves its slot's depth: the parent lays its in-flow children out clear of
//! it, and its descendants observe it (D9, `area.rs`), the bar itself only the bars before it.
use crate::arena::NodeArena;
use crate::generated::SafeAreaBar;
use crate::id::IdMap;
use crate::sorted::SlotSet;
use crate::{Display, PositionType};

/// The bars a document has, which D9's area and the solve read.
#[derive(Debug, Clone, Default)]
pub(crate) struct Bars {
    /// The boxes whose style names an edge to be a bar on: none, in most
    /// documents, lets layout skip asking each box's children.
    pub(crate) styled: SlotSet,
    /// Each bar's slot depth on its parent's last layout (Taffy patch 81).
    pub(crate) depths: IdMap<u32, f32>,
}

impl Bars {
    pub(crate) fn forget(&mut self, slot: u32) {
        self.styled.remove(slot);
        self.depths.remove(&slot);
    }
}

/// The edge `slot` is a bar of its parent on, top 0, right 1, bottom 2, left 3, or `None`: its
/// `-exact-safe-area-bar` names exactly one edge, `start` and `end` resolved against the
/// parent's `direction`, and it is an in-flow child. An owner's authored root, any box its host
/// gives a block, an absolutely positioned box (a top-layer one included), a box with
/// `display: none`, and a child of a scroller or of a replaced box are no bars.
pub(crate) fn edge(arena: &NodeArena, slot: u32) -> Option<usize> {
    if !arena.observations.bars.styled.contains(slot) {
        return None;
    }
    let style = arena.style(slot);
    let parent = arena.parent(slot)?;
    let position = crate::style::top_layer::layout_position(arena, slot);
    let out_of_flow = position == PositionType::Absolute || style.display == Display::None;
    // A scroller's bar, on its scrollport, is not built yet (D14), and a
    // replaced box that holds children lays out none
    let holder = super::area::scroll_view(arena, parent) || arena.node_type(parent).is_replaced();
    if out_of_flow || holder || arena.blocks.holds(slot) {
        return None;
    }
    let bits = style.rare.safe_area_bar;
    let has = |v: SafeAreaBar| bits & v.keyword_bits() != 0;
    let rtl = arena
        .computed_source(parent, crate::StyleId::Direction)
        .direction
        == crate::Direction::Rtl;
    let (start, end) = if rtl { (1, 3) } else { (3, 1) };
    let named = [
        (SafeAreaBar::Top, 0),
        (SafeAreaBar::Right, 1),
        (SafeAreaBar::Bottom, 2),
        (SafeAreaBar::Left, 3),
        (SafeAreaBar::Start, start),
        (SafeAreaBar::End, end),
    ];
    let mut edges = named.into_iter().filter(|(v, _)| has(*v)).map(|(_, e)| e);
    match (edges.next(), edges.next()) {
        (Some(e), None) => Some(e),
        _ => None,
    }
}

/// Whether some child of `slot` is a bar: a walk of the boxes styled as bars, few, rather
/// than of `slot`'s children.
pub(crate) fn has_bars(arena: &NodeArena, slot: u32) -> bool {
    let styled = &arena.observations.bars.styled;
    styled
        .iter()
        .any(|b| arena.parent(b) == Some(slot) && edge(arena, b).is_some())
}

/// `slot`'s part of its engine style: a bar is laid out `position: absolute`, out of its
/// parent's flow, and so is the containing block of its own absolutely positioned descendants;
/// a parent with bars has its slots begin past `reserved`, what its own reservations add to its
/// padding (Taffy patch 81).
pub(crate) fn lay_out(arena: &NodeArena, slot: u32, reserved: [f32; 4], s: &mut taffy::Style) {
    if arena.observations.bars.styled.is_empty() {
        return;
    }
    if let Some(e) = edge(arena, slot) {
        s.position = taffy::style::Position::Absolute;
        s.rare.bar = Some(e as u8);
    }
    if has_bars(arena, slot) {
        let [top, right, bottom, left] = reserved;
        s.rare.bars = true;
        s.rare.bar_inset = taffy::geometry::Rect {
            top,
            right,
            bottom,
            left,
        };
    }
}

/// Restyle what a change to `slot`'s rows `mask` moves of the bars: its parent, where the
/// change can make `slot` a bar or stop it being one, and its own bar children, whose edges
/// follow its `direction` and which are no bars while it scrolls. Returns the boxes whose
/// child lists to sync again, where a child whose bar status may have changed has an
/// `order`, which moves no bar.
pub(crate) fn style_changed(
    arena: &NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    slot: u32,
    mask: &crate::generated::StyleMask,
) -> [Option<u32>; 2] {
    use crate::StyleId;
    let styled = &arena.observations.bars.styled;
    let mut resync = [None; 2];
    if styled.is_empty() && !mask.has(StyleId::SafeAreaBar) {
        return resync;
    }
    let ordered = |s: u32| arena.style(s).order != 0;
    // A bar's edge follows its parent's direction, and a scroller's child is no bar yet
    let children = [StyleId::Direction, StyleId::OverflowX, StyleId::OverflowY];
    if children.iter().any(|&r| mask.has(r)) {
        for &child in arena.children(slot).iter().filter(|&&c| styled.contains(c)) {
            if let Some(n) = arena.taffy(child) {
                layout.restyle(arena, child, n);
            }
            if ordered(child) {
                resync[0] = Some(slot);
            }
        }
    }
    let status = mask.has(StyleId::SafeAreaBar)
        || styled.contains(slot) && (mask.has(StyleId::PositionType) || mask.has(StyleId::Display));
    if let Some((parent, node)) = (arena.parent(slot)).and_then(|p| Some((p, arena.taffy(p)?))) {
        if status {
            layout.restyle(arena, parent, node);
            resync[1] = ordered(slot).then_some(parent);
        }
    }
    resync
}

/// Restyle `parent`, whose children changed, and each of its children styled as a bar, which
/// is one only under a parent, where it has bars now or its engine style says it had.
pub(crate) fn children_changed(
    arena: &NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    parent: u32,
    node: taffy::NodeId,
) {
    let styled = &arena.observations.bars.styled;
    let has = !styled.is_empty() && arena.children(parent).iter().any(|&c| styled.contains(c));
    if !has && !(layout.tree_ref()).is_some_and(|t| t.has_bars(node)) {
        return;
    }
    for &child in arena
        .children(parent)
        .iter()
        .filter(|&&c| styled.contains(c))
    {
        if let Some(n) = arena.taffy(child) {
            layout.restyle(arena, child, n);
        }
    }
    layout.restyle(arena, parent, node);
}

/// How deep `parent`'s bars reserve on each edge, top right bottom left, for a descendant
/// through its child `via`: every bar where `via` is none, else only the bars before it, as a
/// bar observes the area the earlier bars make and not its own or a later one's (D9). Each is
/// its slot's depth on the last layout.
pub(crate) fn reserved(arena: &NodeArena, parent: u32, via: Option<u32>) -> [f32; 4] {
    let mut depths = [0.0; 4];
    // Only a parent of some bar walks its children
    if !has_bars(arena, parent) {
        return depths;
    }
    for &child in arena.children(parent) {
        if Some(child) == via && edge(arena, child).is_some() {
            break;
        }
        if let (Some(e), Some(d)) = (
            edge(arena, child),
            arena.observations.bars.depths.get(&child),
        ) {
            depths[e] += d;
        }
    }
    depths
}

/// The parents of the bars, each once, which the solve checks as it checks the boxes that
/// reserve.
pub(crate) fn parents(arena: &NodeArena) -> Vec<u32> {
    let mut parents: Vec<u32> = (arena.observations.bars.styled.iter())
        .filter(|&b| edge(arena, b).is_some())
        .filter_map(|b| arena.parent(b))
        .collect();
    parents.sort_unstable();
    parents.dedup();
    parents
}
