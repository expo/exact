//! A bar's edge effect (LLP 1123 D12): the scroller an `edge-effect` bar registers with, which
//! the host gives the bar's view on the bar's physical edge (UIKit's
//! `UIScrollEdgeElementContainerInteraction`, one scroller to a view). It is the first scroller
//! in tree order in the bar parent's content whose reached edge lies past the slot's inner side
//! toward the outside, whose reached frame overlaps the slot along the edge, and in whose D14
//! `reserved` term the slot lies.
use super::area::{self, Space};
use super::bar::Slot;
use crate::generated::SafeAreaBar;
use crate::{Edge, Kernel, NodeFlags, NodeKey};

/// An `edge-effect` bar's registration: its physical edge, `start` and `end` resolved against
/// its parent's direction, and the scroller beneath its slot it registers with, if one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarEdgeEffect {
    /// The physical edge the bar lies on.
    pub edge: Edge,
    /// The scroller it registers with: the first beneath its slot, `None` where none is.
    pub scroller: Option<NodeKey>,
}

impl Kernel {
    /// The edge effect of the bar `key` as last laid out (D12). `None` when it is no bar, names
    /// no `edge-effect`, or is not live or not laid out in a slot. A result with no scroller is a
    /// bar with none beneath it, a bar in a space the kernel does not place (a painted
    /// `position-area` popover's) or one whose band is unavailable among them. Its scrollers keep
    /// their own reach and `reserved` terms whatever it registers with.
    pub fn bar_edge_effect(&self, key: NodeKey) -> Option<BarEdgeEffect> {
        let arena = &self.arena;
        let bar = arena.resolve(key)?;
        let e = super::bar::edge(arena, bar)?;
        if arena.style(bar).rare.safe_area_bar & SafeAreaBar::EdgeEffect.keyword_bits() == 0 {
            return None;
        }
        self.laid_out_frame(key)?;
        let slot = *arena.observations.bars.slots.get(&bar)?;
        let parent = arena.parent(bar)?;
        let space = area::band_at(self, parent);
        let scroller = (space.band.filter(|_| space.placed()))
            .and_then(|_| self.beneath(parent, e, slot, &space))
            .map(|s| arena.key(s));
        Some(BarEdgeEffect {
            edge: Edge::ALL[e],
            scroller,
        })
    }

    /// The first scroller in tree order in `parent`'s content, its bars' branches left out, beneath
    /// its bar's `slot` on the edge `e` (D12). Another owner's or a top-layer box's layout is no
    /// part of the content, nor a box not laid out or under `display: none` when last laid out, nor
    /// a scroller's content.
    fn beneath(&self, parent: u32, e: usize, slot: Slot, space: &Space) -> Option<u32> {
        let arena = &self.arena;
        let ((width, height), (ox, oy)) = (space.size, space.origin);
        let [x, y, w, h] = [
            slot.rect[0] + ox,
            slot.rect[1] + oy,
            slot.rect[2],
            slot.rect[3],
        ];
        // The slot's inner side, in from the space's edge, and its sides along the edge
        let inner = match e {
            0 => y + h,
            1 => width - x,
            2 => height - y,
            _ => x + w,
        };
        let along = match e % 2 {
            0 => [x, x + w],
            _ => [y, y + h],
        };
        let bar = area::BarSlot {
            parent,
            edge: e,
            depth: slot.depth,
            inner,
        };
        let content = |s: u32| {
            let children = arena.children(s).iter().rev().copied();
            children.filter(move |&c| s != parent || super::bar::edge(arena, c).is_none())
        };
        let mut stack: Vec<u32> = content(parent).collect();
        while let Some(s) = stack.pop() {
            let flags = arena.flags(s);
            let shown = !flags.has(NodeFlags::HIDDEN) && !flags.has(NodeFlags::CREATED);
            if !shown || arena.is_inline_run(s) || area::holds(arena, s).is_some() {
                continue;
            }
            // A scroller in another's content is beneath no bar outside that one, which clips it at
            // its padding edge (D12, D13)
            if area::scroll_view(arena, s) {
                if self.under_slot(s, bar, along, space) {
                    return Some(s);
                }
                continue;
            }
            stack.extend(content(s));
        }
        None
    }

    /// Whether the scroller `s` lies beneath the bar's slot `bar`, whose sides along its edge are
    /// `along`: the slot's reservation in its D14 `reserved` term there (`area::area_holding`);
    /// its reached edge past the slot's inner side toward the outside by more than D9's
    /// tolerance; and its reached frame overlapping the slot along the edge.
    fn under_slot(
        &self,
        s: u32,
        bar: area::BarSlot,
        [start, end]: [f32; 2],
        space: &Space,
    ) -> bool {
        let arena = &self.arena;
        let e = bar.edge;
        let (terms, held) = super::scroll_terms::terms_holding(arena, s, space, Some(bar));
        if terms.reserved[e].is_none() || !held {
            return false;
        }
        let reached = space.edges(arena, s)[e] - terms.reach[e];
        let (f, r, (ox, oy)) = (arena.frame(s), terms.reach, space.origin);
        let (fx, fy) = (f.x + ox, f.y + oy);
        let [from, to] = match e % 2 {
            0 => [fx - r[3], fx + f.width + r[1]],
            _ => [fy - r[0], fy + f.height + r[2]],
        };
        reached < bar.inner - area::tolerance(arena) && to.min(end) > from.max(start)
    }
}
