//! A containing block a host gives a subtree (@ref LLP 1115.002 D8.2).
//!
//! A host shows some subtrees of one page in views of their own: a route in
//! a split's pane, a modal route in a sheet. Each is laid out in a box that
//! is not the window, and the host says which: a width and a height, each
//! the size (a pane reports its own), a maximum, or a content-fit limit
//! that an authored size may exceed. The kernel learns nothing else of panes or
//! sheets, and where the host puts the result is the host's.
//!
//! While a node has a block it is out of its parent's layout, as a box in
//! CSS's top layer is out of flow, and is laid out as a root is, in the
//! block instead of the viewport: it is the engine's root for its subtree,
//! so a block that changes lays out that subtree and nothing else. Its
//! frames are in the block's own space. A content owner's root is laid out
//! in its content root, the block less what its host clears of it, and
//! placed there (`clearance`, @ref LLP 1123 D3). Under a maximum it is
//! sized as a modal `dialog` is (`apply`). Viewport units, `env()`, `rem`
//! and a hairline's device pixel stay the window's: no block reaches the
//! environment, the root font size or the display's scale, which are
//! resolved into rows at a commit while a block never touches a row. Only
//! container units read the block, through the content root it sizes
//! (`owner::container_at`, LLP 1123 D16): what is derived from them is
//! derived again when its fixed lengths move (`clear_block`).
//! Authored rows never change, and a host that gives no block lays out
//! exactly as before.
//!
//! The kernel implies one block itself: the viewport, for each box in
//! HTML's top layer (a popover; `Kernel::imply_blocks`), fitting its
//! content as the UA sheet's `fit-content` does. A host's block for the
//! same box wins, and taking it back leaves the viewport.

use super::{engine, Kernel};
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::generated::SafeAreaIgnore;
use crate::id::{NodeFlags, Offer, ViewId};
use crate::layout::{self, LayoutMirror, LayoutReceipt};
use crate::style::{calc_has_percent, resolve_calc};
use taffy::style::{BoxSizing, CompactLength, LengthPercentageAuto as Lpa, Position};
use taffy::util::{MaybeResolve, ResolveOrZero};

#[path = "reservation.rs"]
pub(crate) mod reservation;

/// One axis of a host's containing block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlockAxis {
    /// The block is this long: a pane's side.
    Is(f32),
    /// The host sizes the root's border box to this allocation, as a resizable pane does.
    Fill(f32),
    /// The block is at most this long, and as long as its content: a sheet's.
    AtMost(f32),
    /// An automatic size fits its content under this limit. An authored
    /// size or minimum may exceed it; percentages still use this basis.
    Fit(f32),
    /// The region a box is placed in, CSS's inset-modified containing block
    /// for a `position-area` (a popover's option, LLP 1021 D2): across, an
    /// automatic size is `fit-content` in it, its content's min-content
    /// winning where that is wider; down, its content's height, which may
    /// overflow it (whether it does is what picks the option). Percentages
    /// use it, and an authored size is the author's.
    Region(f32),
    /// The axis a host sizes to the box's content (an arranged pane's, LLP
    /// 1123 D7), measured as `Region` is. The size this answers is the basis
    /// of the box's own percentages on the axis, so they are cyclic (CSS
    /// Sizing 3 §5.2.1): `auto` for a preferred size and `none` for a maximum
    /// that holds one, and a minimum's percentage terms resolved against
    /// zero; the size the host then gives is their basis.
    Content(f32),
}

impl BlockAxis {
    /// The length the host gave, the size or the maximum.
    pub fn length(self) -> f32 {
        match self {
            Self::Is(v)
            | Self::Fill(v)
            | Self::AtMost(v)
            | Self::Fit(v)
            | Self::Region(v)
            | Self::Content(v) => v,
        }
    }

    /// This axis less `by`, at least 0.
    fn less(self, by: f32) -> Self {
        let v = (self.length() - by).max(0.0);
        match self {
            BlockAxis::Is(_) => BlockAxis::Is(v),
            BlockAxis::Fill(_) => BlockAxis::Fill(v),
            BlockAxis::AtMost(_) => BlockAxis::AtMost(v),
            BlockAxis::Fit(_) => BlockAxis::Fit(v),
            BlockAxis::Region(_) => BlockAxis::Region(v),
            BlockAxis::Content(_) => BlockAxis::Content(v),
        }
    }
}

/// A containing block a host gives one node, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HostBlock {
    /// The selected position-try transformation, applied before sizing.
    pub tactic: crate::position_try::BlockTactic,
    /// Its width.
    pub width: BlockAxis,
    /// Its height.
    pub height: BlockAxis,
}

/// How a host shows a block's route against the route its stack has on top
/// (LLP 1127 D8), which the runner's liveness reads and layout does not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlockShown {
    /// As its stack shows it: live when it is the top.
    #[default]
    InStack,
    /// Beside the top, live: a split's list column beside its detail.
    Beside,
    /// Under a pane shown over it, not live though shown: a split's detail
    /// under the list that overlays it, and a split's columns under a sheet.
    Under,
}

impl HostBlock {
    /// This block less `clear`, top right bottom left: the content root a
    /// content owner's root is laid out in (@ref LLP 1123 D3).
    fn less(self, [top, right, bottom, left]: [f32; 4]) -> Self {
        HostBlock {
            width: self.width.less(left + right),
            height: self.height.less(top + bottom),
            ..self
        }
    }
}

/// What the host clears of `slot`'s allocation for its content root, top
/// right bottom left (@ref LLP 1123 D3): a content owner's incoming band,
/// none of it on each edge whose container region the root takes (D4), and
/// the root's reservations (D11), with the keyboard's occupancy under
/// `resizes-content` covering the band and the additional inset where it is
/// deeper and the padding stacked on it (D7). No band while it is
/// unavailable, as UIKit gives a view outside a window zero safe-area
/// insets; and nothing for a box that is no content owner.
pub(crate) fn clearance(arena: &NodeArena, slot: u32) -> [f32; 4] {
    arena
        .blocks
        .held(slot)
        .map_or([0.0; 4], |b| cleared(arena, b))
}

/// [`clearance`] for `held`'s root, with no search of the blocks.
fn cleared(arena: &NodeArena, held: &Held) -> [f32; 4] {
    cleared_taking(arena, held, true)
}

/// How far past its content root's edge `held`'s root begins its bars'
/// slots (LLP 1123 D12), given its clearance `clear`: on an edge it takes,
/// past what its host would clear there had it not taken it, as a bar
/// pinned to the controller view's safe-area guide lies past the band its
/// content reaches under; 0 on every other edge, and for a root with no
/// bars.
fn bar_slots(arena: &NodeArena, held: &Held, clear: [f32; 4]) -> [f32; 4] {
    if !is_owner(arena, held) || !super::bar::has_bars(arena, held.slot) {
        return [0.0; 4];
    }
    let untaken = cleared_taking(arena, held, false);
    std::array::from_fn(|e| (untaken[e] - clear[e]).max(0.0))
}

/// [`cleared`] with the edges its root takes of the container region taken,
/// where `taking`, or none of them.
fn cleared_taking(arena: &NodeArena, held: &Held, taking: bool) -> [f32; 4] {
    if !is_owner(arena, held) {
        return [0.0; 4];
    }
    let band = arena.observations.band(held.slot).unwrap_or([0.0; 4]);
    let taken = match taking {
        true => super::ignore::edges(arena, held.slot, SafeAreaIgnore::Container),
        false => [false; 4],
    };
    let band = std::array::from_fn(|e| if taken[e] { 0.0 } else { band[e] });
    let cover = super::keyboard::cover(arena, held.slot);
    match reservation::reserved(arena, held.slot) {
        Some(r) => r.inside_covered(band, cover),
        None => std::array::from_fn(|e| band[e].max(cover[e])),
    }
}

/// `held`'s content root's width and height where its host fixes them, the
/// block less `clear`, which its band, the keyboard, the root's reservations
/// and its ignore size (LLP 1123 D16); neither for a box that is no owner,
/// whose block fixes no container (D2).
fn fixed_lengths(arena: &NodeArena, held: &Held, clear: [f32; 4]) -> [Option<f32>; 2] {
    let given = held.given.filter(|_| is_owner(arena, held));
    let fixed = |axis: Option<BlockAxis>, by: f32| match axis {
        Some(BlockAxis::Is(length) | BlockAxis::Fill(length)) => Some((length - by).max(0.0)),
        _ => None,
    };
    [
        fixed(given.map(|b| b.width), clear[1] + clear[3]),
        fixed(given.map(|b| b.height), clear[0] + clear[2]),
    ]
}

/// [`fixed_lengths`] of `slot`'s block as it stands, for a length in a
/// container unit (`owner::container_at`); a read of lengths other than those
/// the block was last laid out with marks it, so that its next layout derives
/// the readers again whatever the lengths are by then.
pub(crate) fn container_lengths(arena: &NodeArena, slot: u32) -> Option<[Option<f32>; 2]> {
    let held = arena.blocks.held(slot)?;
    let lengths = fixed_lengths(arena, held, cleared(arena, held));
    if lengths != held.content {
        held.read_off.set(true);
    }
    Some(lengths)
}

/// Whether `slot` is a content owner's root: a box its host gave a block
/// that is no top-layer box, which is no owner whatever block it has (D2).
pub(crate) fn owns(arena: &NodeArena, slot: u32) -> bool {
    arena.blocks.held(slot).is_some_and(|b| is_owner(arena, b))
}

fn is_owner(arena: &NodeArena, held: &Held) -> bool {
    held.given.is_some() && !arena.top_layer_slots.contains(held.slot)
}

/// The blocks: a few routes and open popovers at most, so a list by slot,
/// as the covers are. And what an owner carries beside its block, which a
/// host may give a route with no block of its own: its local insets (LLP
/// 1123 D2, D9; `owner.rs`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Blocks(
    Vec<Held>,
    Vec<(u32, [f32; 4])>,
    /// The owners in `.1`, for a lookup that finds no owner at once.
    crate::sorted::SlotSet,
);

/// One node's block: the one in force, the host's or else the one the
/// kernel implies (a top-layer box's, the viewport: HTML's top layer), its
/// used size, and whether an ancestor hid it when it was last published.
#[derive(Clone, Debug)]
pub(crate) struct Held {
    slot: u32,
    block: HostBlock,
    used: (f32, f32),
    hidden: bool,
    given: Option<HostBlock>,
    implied: Option<HostBlock>,
    /// Whether a box above it was replaced whole when it was last styled
    /// (`apply` makes it `display: none` then).
    buried: bool,
    /// How its host shows it against its stack's top.
    shown: BlockShown,
    /// What the host cleared of it for its content root when it was last
    /// laid out (`clearance`).
    cleared: [f32; 4],
    /// How far past its content root's edge its bars' slots began when it
    /// was last laid out (`bar_slots`).
    slots: [f32; 4],
    /// Its content root's width and height where its host fixes them, as
    /// they were when it was last laid out: what container units below read
    /// (`owner::container_at`, LLP 1123 D16).
    content: [Option<f32>; 2],
    /// Whether a length was derived since its last layout against a content
    /// root other than `content`, which its next layout derives again.
    read_off: std::cell::Cell<bool>,
}

impl Blocks {
    /// The block `slot` has, its host's or the kernel's.
    pub(crate) fn get(&self, slot: u32) -> Option<HostBlock> {
        self.held(slot).map(|b| b.block)
    }

    /// The block `slot`'s host gave it: a content owner's (LLP 1123 D2).
    pub(crate) fn given(&self, slot: u32) -> Option<HostBlock> {
        self.held(slot).and_then(|b| b.given)
    }

    /// How far past its content root's edge `slot`'s bars' slots begin, as
    /// it was last laid out (LLP 1123 D12): 0 but on an edge an owner's root
    /// takes.
    pub(crate) fn bar_slots(&self, slot: u32) -> [f32; 4] {
        self.held(slot).map_or([0.0; 4], |b| b.slots)
    }

    /// Whether `slot`'s bars' slots would begin elsewhere than at its last
    /// layout (`bar_slots`): a band's change on an edge an owner's root takes
    /// moves them though its clearance stays.
    pub(crate) fn slots_moved(arena: &NodeArena, slot: u32) -> bool {
        (arena.blocks.held(slot)).is_some_and(|b| b.slots != bar_slots(arena, b, cleared(arena, b)))
    }

    /// Whether `slot` is the root of a layout of its own.
    pub(crate) fn holds(&self, slot: u32) -> bool {
        !self.0.is_empty() && self.held(slot).is_some()
    }

    /// The roots of the blocks.
    pub(crate) fn roots(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.iter().map(|b| b.slot)
    }

    /// Whether any node has one.
    pub(crate) fn any(&self) -> bool {
        !self.0.is_empty()
    }

    /// Whether a host gave any block.
    pub(crate) fn any_given(&self) -> bool {
        self.0.iter().any(|b| b.given.is_some())
    }

    /// The block a host gives `slot` (`None`: takes it back, leaving the
    /// implied one, if any).
    pub(crate) fn set(&mut self, slot: u32, block: Option<HostBlock>) {
        let implied = self.held(slot).and_then(|b| b.implied);
        self.put(slot, block, implied);
    }

    /// The block the kernel implies for `slot` (`None`: none).
    pub(crate) fn imply(&mut self, slot: u32, block: Option<HostBlock>) {
        let given = self.held(slot).and_then(|b| b.given);
        self.put(slot, given, block);
    }

    /// The block the kernel implies for `slot`.
    pub(crate) fn implied(&self, slot: u32) -> Option<HostBlock> {
        self.held(slot).and_then(|b| b.implied)
    }

    /// Every implied block: the slots that hold one.
    pub(crate) fn implied_slots(&self) -> Vec<u32> {
        let implied = self.0.iter().filter(|b| b.implied.is_some());
        implied.map(|b| b.slot).collect()
    }

    /// How `slot`'s host shows it against its stack's top (`Held::shown`).
    pub(crate) fn shown(&self, slot: u32) -> BlockShown {
        self.held(slot).map_or(BlockShown::InStack, |b| b.shown)
    }

    /// Say how `slot`'s block is shown against its stack's top; whether that
    /// changed. A slot with no block is shown in the page, and keeps none.
    pub(crate) fn set_shown(&mut self, slot: u32, shown: BlockShown) -> bool {
        match self.held_mut(slot) {
            Some(held) if held.shown != shown => {
                held.shown = shown;
                true
            }
            _ => false,
        }
    }

    /// `slot` gone: neither block stands, nor its local insets.
    pub(crate) fn remove(&mut self, slot: u32) {
        self.0.retain(|b| b.slot != slot);
        self.set_local(slot, None);
    }

    /// The local insets a host gave the owner whose root is `slot`.
    pub(crate) fn local(&self, slot: u32) -> Option<[f32; 4]> {
        if !self.2.contains(slot) {
            return None;
        }
        self.1.iter().find(|(s, _)| *s == slot).map(|(_, l)| *l)
    }

    pub(crate) fn set_local(&mut self, slot: u32, local: Option<[f32; 4]>) {
        self.1.retain(|(s, _)| *s != slot);
        self.1.extend(local.map(|l| (slot, l)));
        match local {
            Some(_) => self.2.insert(slot),
            None => self.2.remove(slot),
        };
    }

    /// The owners whose hosts set local insets.
    pub(crate) fn local_slots(&self) -> &crate::sorted::SlotSet {
        &self.2
    }

    fn held(&self, slot: u32) -> Option<&Held> {
        self.0.iter().find(|b| b.slot == slot)
    }

    fn held_mut(&mut self, slot: u32) -> Option<&mut Held> {
        self.0.iter_mut().find(|b| b.slot == slot)
    }

    fn put(&mut self, slot: u32, given: Option<HostBlock>, implied: Option<HostBlock>) {
        match (self.held_mut(slot), given.or(implied)) {
            (Some(held), Some(block)) => {
                (held.block, held.given, held.implied) = (block, given, implied)
            }
            (Some(_), None) => self.0.retain(|b| b.slot != slot),
            (None, Some(block)) => self.0.push(Held {
                slot,
                block,
                used: (0.0, 0.0),
                hidden: false,
                given,
                implied,
                buried: false,
                shown: BlockShown::InStack,
                cleared: [0.0; 4],
                slots: [0.0; 4],
                content: [None; 2],
                read_off: Default::default(),
            }),
            (None, None) => {}
        }
    }
}

/// Whether `slot` is hidden from above (LLP 1141.004 D2): under a box its
/// host replaced whole, which hides every block in it (a tab not yet built,
/// LLP 1075.003 §9.12); under a box a bar replaced with no block its host
/// gave on the way down to `slot`, `slot`'s own included, since the bar shows
/// such a block in a view of its own; or, with `authored`, under a box
/// authored `display: none`. `itself`: `slot`'s own cover counts too.
fn hidden_by(arena: &NodeArena, slot: u32, itself: bool, authored: bool) -> bool {
    let mut given = false;
    let mut at = Some(slot);
    while let Some(s) = at {
        let above = s != slot;
        if above || itself {
            match arena.cover(s) {
                Some(super::HostCover::Whole) => return true,
                Some(super::HostCover::Bar) if !given => return true,
                _ => {}
            }
        }
        if authored && above && arena.style(s).display == crate::Display::None {
            return true;
        }
        given |= arena.blocks.given(s).is_some();
        at = arena.parent(s);
    }
    false
}

/// Whether an ancestor of `slot` is `display: none`, as authored or as a box
/// a host replaced (`hidden_by`).
fn hidden_above(arena: &NodeArena, slot: u32) -> bool {
    hidden_by(arena, slot, false, true)
}

/// Whether `slot` is not laid out because it is, or is under, a box a host
/// replaced (`hidden_by`).
pub(crate) fn replaced_over(arena: &NodeArena, slot: u32) -> bool {
    hidden_by(arena, slot, true, false)
}

/// Whether an ancestor of `slot` is a box a host replaced that hides it (`hidden_by`).
fn replaced_above(arena: &NodeArena, slot: u32) -> bool {
    hidden_by(arena, slot, false, false)
}

/// What publication starts a block's root with: whether an ancestor hides
/// it, whether its whole subtree is published (it took or lost a block, or
/// what hides it or its content root's place changed), and where its
/// content root is in the block's space. None of them for a root without a
/// block.
pub(crate) fn publication_start(arena: &mut NodeArena, root: u32) -> (bool, bool, (f32, f32)) {
    let hidden =
        hidden_above(arena, root) || arena.cover(root).is_some_and(super::HostCover::replaces);
    let whole = arena.flags(root).has(NodeFlags::ATTACHED);
    let Some(held) = arena.blocks.held_mut(root) else {
        return (false, false, (0.0, 0.0));
    };
    let was = std::mem::replace(&mut held.hidden, hidden);
    let [top, _, _, left] = held.cleared;
    (hidden, whole || was != hidden, (left, top))
}

/// A block's part of its root's engine style. A root holds the absolutely
/// positioned boxes no positioned box inside it holds, so a `static` one is
/// `relative`. On an axis where the block is a maximum the root is sized as
/// a modal `dialog` is in its viewport: an automatic size is `fit-content`,
/// its border box is at most the maximum, and its insets and automatic
/// margins, which would place it against the edge being asked for, do not
/// apply. `fit-content` is what CSS gives an absolutely positioned box with
/// no inset, and the engine sizes one before it lays its children out, so
/// their percentages have their basis: under a maximum width a root that
/// is in flow is laid out as such a box, at its static position. `Fit`
/// uses the same placement, but supplies a ceiling only for an automatic
/// size; CSS's own minimum can exceed that ceiling.
///
/// A block in a box a host replaced whole (a tab not yet built, LLP
/// 1075.003 §9.12) is `display: none` as that box is: none of its subtree
/// is measured, and it is published hidden until the box is given back. A
/// box a bar replaced buries only the blocks its host did not give.
pub(crate) fn apply(
    arena: &NodeArena,
    slot: u32,
    block: Option<HostBlock>,
    s: &mut taffy::style::Style,
) {
    let Some(block) = block else { return };
    if replaced_above(arena, slot) {
        s.display = taffy::style::Display::None;
        return;
    }
    let block = block.less(clearance(arena, slot));
    block.tactic.apply(s);
    let _env = crate::style::LayoutEnv::set(arena.env());
    cyclic(block, s);
    let in_flow = s.position != Position::Absolute;
    if s.position == Position::Static {
        s.position = Position::Relative;
    }
    let basis = Some(block.width.length());
    let edges = s.padding.resolve_or_zero(basis, resolve_calc)
        + s.border.resolve_or_zero(basis, resolve_calc);
    let content_box = s.box_sizing == BoxSizing::ContentBox;
    let fill = |length: f32, edges: f32| {
        taffy::Dimension::length((length - if content_box { edges } else { 0.0 }).max(0.0))
    };
    if let BlockAxis::Fill(width) = block.width {
        s.size.width = fill(width, edges.horizontal_axis_sum());
        s.min_size.width = s.size.width;
        s.max_size.width = s.size.width;
    }
    if let BlockAxis::Fill(height) = block.height {
        s.size.height = fill(height, edges.vertical_axis_sum());
        s.min_size.height = s.size.height;
        s.max_size.height = s.size.height;
    }
    let ceiling = |most: f32, edges: f32, own: taffy::Dimension, basis: f32| {
        let most = (most - if content_box { edges } else { 0.0 }).max(0.0);
        let own = own.maybe_resolve(Some(basis), resolve_calc);
        taffy::Dimension::length(own.map_or(most, |own: f32| own.min(most)))
    };
    let unplaced = |inset: &mut Lpa, margin: &mut Lpa| {
        *inset = Lpa::auto();
        if margin.is_auto() {
            *margin = Lpa::length(0.0);
        }
    };
    // A region sets no ceiling: the engine sizes an absolutely positioned
    // box with no inset as `fit-content` in its available width, which the
    // block's length is, min-content winning, and down as its content.
    let capped = |axis: BlockAxis, auto: bool| match axis {
        BlockAxis::AtMost(_) => true,
        BlockAxis::Fit(_) => auto,
        BlockAxis::Is(_) | BlockAxis::Fill(_) | BlockAxis::Region(_) | BlockAxis::Content(_) => {
            false
        }
    };
    if let BlockAxis::AtMost(most)
    | BlockAxis::Fit(most)
    | BlockAxis::Region(most)
    | BlockAxis::Content(most) = block.width
    {
        unplaced(&mut s.inset.left, &mut s.margin.left);
        unplaced(&mut s.inset.right, &mut s.margin.right);
        if in_flow {
            s.position = Position::Absolute;
            (s.inset.top, s.inset.bottom) = (Lpa::auto(), Lpa::auto());
        }
        if capped(block.width, s.size.width.is_auto()) {
            s.max_size.width = ceiling(most, edges.horizontal_axis_sum(), s.max_size.width, most);
        }
    }
    if let BlockAxis::AtMost(most)
    | BlockAxis::Fit(most)
    | BlockAxis::Region(most)
    | BlockAxis::Content(most) = block.height
    {
        unplaced(&mut s.inset.top, &mut s.margin.top);
        unplaced(&mut s.inset.bottom, &mut s.margin.bottom);
        if capped(block.height, s.size.height.is_auto()) {
            s.max_size.height = ceiling(most, edges.vertical_axis_sum(), s.max_size.height, most);
        }
    }
}

/// A `Content` axis's root percentages, which its answer would decide, as
/// CSS takes a cyclic percentage (CSS Sizing 3 §5.2.1): `auto` for a
/// preferred size and `none` for a maximum that holds one, and a minimum's
/// percentage terms resolved against zero. A length and a sizing keyword
/// keep theirs.
fn cyclic(block: HostBlock, s: &mut taffy::style::Style) {
    let cyclic = |d: taffy::Dimension| {
        let raw = d.into_raw();
        raw.tag() == CompactLength::PERCENT_TAG
            || (raw.is_calc() && calc_has_percent(raw.calc_value()))
    };
    let clear =
        |size: &mut taffy::Dimension, min: &mut taffy::Dimension, max: &mut taffy::Dimension| {
            if cyclic(*size) {
                *size = taffy::Dimension::auto();
            }
            if cyclic(*min) {
                // A length, zero included: `auto` would let an aspect ratio's automatic minimum grow the box to its content
                let floor = min.maybe_resolve(Some(0.0), resolve_calc).unwrap_or(0.0);
                *min = taffy::Dimension::length(floor.max(0.0));
            }
            if cyclic(*max) {
                *max = taffy::Dimension::auto();
            }
        };
    if let BlockAxis::Content(_) = block.width {
        clear(
            &mut s.size.width,
            &mut s.min_size.width,
            &mut s.max_size.width,
        );
    }
    if let BlockAxis::Content(_) = block.height {
        clear(
            &mut s.size.height,
            &mut s.min_size.height,
            &mut s.max_size.height,
        );
    }
}

impl Kernel {
    /// Give `view` a containing block, or take it back (`None`): at the next
    /// layout its subtree is laid out in the block, or in its parent again
    /// (in the viewport for a top-layer box, the block the kernel implies).
    /// Nonfinite or negative lengths are refused, as is a block for a root
    /// (the offer is its), for an inline run, and for an ordinary box while
    /// a content region is registered. Top-layer subtrees remain independent.
    pub fn set_host_block(
        &mut self,
        view: ViewId,
        block: Option<HostBlock>,
    ) -> Result<(), KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if let Some(b) = block {
            let lengths = [b.width.length(), b.height.length()];
            if !lengths.iter().all(|l| l.is_finite() && *l >= 0.0) {
                return Err(LayoutError::InvalidOffer.into());
            }
            let refused = if self.arena.is_root(slot) {
                Some("a root is laid out in its offer")
            } else if self.arena.in_paragraph(slot) {
                Some("an inline run or image has no box of its own")
            } else if self.region.is_some() && !self.arena.top_layer_slots.contains(slot) {
                Some("only a top-layer block while a content region is registered")
            } else {
                None
            };
            if let Some(why) = refused {
                return Err(LayoutError::HostBlock(why).into());
            }
        }
        let before = self.arena.blocks.get(slot);
        // Measure the readers under a box that gains or gives back a block again, which
        // changes the band they observe; a reader elsewhere that this moves has a frame
        // that moved
        let placed = self.arena.blocks.given(slot).is_some() != block.is_some();
        if placed {
            self.arena.observations.moved(slot);
        }
        self.arena.blocks.set(slot, block);
        self.block_changed(slot, before);
        // Derive the owner segments' readers below a box whose host starts or stops placing
        // it while its implied block stands
        if placed && before.is_some() && self.arena.blocks.get(slot).is_some() {
            self.segments_moved(slot);
        }
        Ok(())
    }

    /// Each top-layer box's block made the viewport, and a box that left the
    /// top layer (or became a root or an inline run, or its host replaced
    /// it whole) its own again: HTML's
    /// top layer, where `width: 50%` is half the viewport whatever box the
    /// popover was written in. Before each layout, after the viewport is
    /// the offer's; until a definite offer has given a viewport there is
    /// none, and a top-layer box stays in its positioned ancestor.
    pub(super) fn imply_blocks(&mut self) {
        let (wants, moved) = self.implied_changes();
        for (slot, want) in wants {
            let before = self.arena.blocks.get(slot);
            self.arena.blocks.imply(slot, want);
            self.block_changed(slot, before);
        }
        // A block moved under a box replaced whole, or out from under one
        // (its ancestors reparented), is styled again: `apply` reads that.
        for slot in moved {
            self.restyle_block(slot);
        }
        // Derive the raw readers and the owner segments' readers again below
        // each box that joined or left the top layer
        for slot in std::mem::take(&mut self.arena.observations.targets.moved) {
            if self.arena.is_live(slot) {
                self.targets_moved(slot);
            }
        }
    }

    /// What `imply_blocks` would change: each box whose implied block is
    /// to be another, and each block root to be styled again.
    fn implied_changes(&self) -> (Vec<(u32, Option<HostBlock>)>, Vec<u32>) {
        let arena = &self.arena;
        if arena.top_layer_slots.is_empty() && !arena.blocks.any() {
            return (Vec::new(), Vec::new());
        }
        let (env, lifted) = (arena.env(), &arena.top_layer_slots);
        let (width, height) = (env.viewport_width, env.viewport_height);
        let viewport = (width > 0.0 && height > 0.0).then_some(HostBlock {
            tactic: Default::default(),
            width: BlockAxis::Fit(width),
            height: BlockAxis::Fit(height),
        });
        let mut slots = arena.blocks.implied_slots();
        slots.extend(lifted.iter().filter(|s| arena.blocks.implied(*s).is_none()));
        let wants: Vec<_> = slots
            .into_iter()
            .map(|slot| {
                let boxed = !arena.is_root(slot)
                    && !arena.in_paragraph(slot)
                    && arena.cover(slot) != Some(super::HostCover::Whole);
                (slot, viewport.filter(|_| lifted.contains(slot) && boxed))
            })
            .filter(|&(slot, want)| arena.blocks.implied(slot) != want)
            .collect();
        let moved: Vec<u32> = (arena.blocks.0.iter())
            .filter(|b| b.buried != replaced_above(arena, b.slot))
            .map(|b| b.slot)
            .collect();
        (wants, moved)
    }

    /// Whether `view` is a top-layer box under its current rows, including
    /// while its host covers it whole because it is closed. An inline run
    /// that says `popover` is not: it stays in its paragraph.
    pub fn is_top_layer(&self, view: ViewId) -> bool {
        self.arena
            .slot_of(view)
            .is_some_and(|s| crate::style::top_layer::is_box(&self.arena, s))
    }

    /// Derive again the lengths in container units below `slot`, a block's
    /// root whose content root, which they read, may have resized, or whose
    /// box joined or left the top layer, where it is no owner (LLP 1123 D16).
    fn resize_content_root(&mut self, slot: u32) {
        let sized = super::readers::container_readers(&self.arena, slot);
        self.restyle_readers(&sized);
    }

    /// `slot`'s block root styled again, as its block or what is above it
    /// now says.
    fn restyle_block(&mut self, slot: u32) {
        let buried = replaced_above(&self.arena, slot);
        if let Some(held) = self.arena.blocks.held_mut(slot) {
            held.buried = buried;
        }
        if let (Some(node), Some(layout)) = (self.arena.taffy(slot), self.layout.as_deref_mut()) {
            layout.restyle(&self.arena, slot, node);
            layout.mark_dirty(node);
        }
    }

    /// `slot`'s block was `before`: restyled for the one it has now, and if
    /// it gained or lost one, moved out of its parent's layout or into it.
    fn block_changed(&mut self, slot: u32, before: Option<HostBlock>) {
        let block = self.arena.blocks.get(slot);
        if before == block {
            return;
        }
        self.restyle_block(slot);
        // A block taken back is laid out no more, so its readers are derived here, and
        // a given one's when it is laid out (`clear_block`)
        if block.is_none() {
            self.resize_content_root(slot);
        }
        if before.is_some() == block.is_some() {
            return;
        }
        self.targets_moved(slot);
        if let Some(region) = &mut self.region {
            if !self.arena.top_layer_slots.contains(slot)
                && self.arena.is_ancestor(region.binding.content.index, slot)
            {
                region.invalidate();
            }
        }
        // It leaves its parent's layout, or returns to it: published whole
        // in the space it is now in, and its parent's children again.
        self.arena.flags_mut(slot).insert(NodeFlags::ATTACHED);
        if let Some(parent) = self.arena.parent(slot) {
            self.arena
                .flags_mut(parent)
                .insert(NodeFlags::CHILDREN_DIRTY);
            if let (Some(node), Some(layout)) =
                (self.arena.taffy(parent), self.layout.as_deref_mut())
            {
                layout.sync_children(&self.arena, parent, node);
                layout.mark_dirty(node);
            }
        }
    }

    /// The containing block `view` is laid out in: its host's, or for a
    /// top-layer box the viewport, as of the last layout.
    pub fn host_block(&self, view: ViewId) -> Option<HostBlock> {
        self.arena.blocks.get(self.arena.slot_of(view)?)
    }

    /// How `view`'s host shows its block against the route its stack has on
    /// top (LLP 1127): beside it as a split's list column, under it as a
    /// split's detail under the list that overlays it, or in its stack.
    pub fn host_block_shown(&self, view: ViewId) -> BlockShown {
        self.arena
            .slot_of(view)
            .map_or(BlockShown::InStack, |slot| self.arena.blocks.shown(slot))
    }

    /// Say how `view`'s block is shown against its stack's top; whether that
    /// changed. Layout does not read it: the runner's liveness does.
    pub fn set_host_block_shown(&mut self, view: ViewId, shown: BlockShown) -> bool {
        match self.arena.slot_of(view) {
            Some(slot) => self.arena.blocks.set_shown(slot, shown),
            None => false,
        }
    }

    /// Where a top-layer box laid out in the viewport's block sits in a
    /// viewport `width` by `height`, as a fixed box does by its authored
    /// insets (the block unset them, so its frame is at its margins): a
    /// side's inset from that side, its margin past it; with both insets an
    /// `auto` margin takes the room left, two sharing it; no inset on an
    /// axis, at its margin. Its border box's origin; `None` when the kernel implies
    /// no block for `view`. What a host that draws the top layer itself
    /// places it by (the terminal), and where the runner reads one no host
    /// has said it shows.
    pub fn top_layer_origin(
        &self,
        view: ViewId,
        (width, height): (f32, f32),
    ) -> Option<(f32, f32)> {
        if !self.host_block_implied(view) {
            return None;
        }
        let node = self.node(view)?;
        let env = self.arena.env_of(node.key.index);
        let (s, f, env) = (node.style, node.frame, &*env);
        let used = self.host_block_size(view)?;
        // The end margins as laid out: the used size clamps a negative one.
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref);
        let laid = self.arena.taffy(node.key.index).zip(tree);
        let ends = laid.map_or(
            (used.0 - f.x - f.width, used.1 - f.y - f.height),
            |(n, t)| {
                let margin = t.layout(n).margin;
                (margin.right, margin.bottom)
            },
        );
        let length = |d: crate::Dimension, basis: f32| {
            (d.resolve(env) != crate::Dimension::Auto).then(|| d.at_basis(env, basis))
        };
        let auto = |m: crate::Dimension| m == crate::Dimension::Auto;
        // The containing block's direction, the root's: which inset wins
        // when both are written and neither margin is `auto`, and which
        // margin is zero when two `auto` ones would be negative.
        let mut root = node.key.index;
        while let Some(up) = self.arena.parent(root) {
            root = up;
        }
        let rtl = self
            .arena
            .computed_source(root, crate::StyleId::Direction)
            .direction
            == crate::generated::Direction::Rtl;
        // CSS 2 §10.3.7 and §10.6.4: with both insets, an `auto` margin
        // takes the room left and two share it; across, a negative share
        // leaves the start margin zero (the end one in RTL), down it stays.
        let axis = |inset: [crate::Dimension; 2],
                    margin: [crate::Dimension; 2],
                    at: f32,
                    size: f32,
                    end_margin: f32,
                    room: f32,
                    across: bool| {
            let from_end = |end: f32| room - end - end_margin - size;
            match (
                length(inset[0].clone(), room),
                length(inset[1].clone(), room),
            ) {
                (Some(start), Some(end)) => {
                    match (auto(margin[0].clone()), auto(margin[1].clone())) {
                        (true, true) => {
                            let share = (room - start - end - size) / 2.0;
                            match (across && share < 0.0, rtl) {
                                (true, false) => start,
                                (true, true) => room - end - size,
                                (false, _) => start + share,
                            }
                        }
                        (true, false) => from_end(end),
                        (false, true) => start + at,
                        (false, false) if across && rtl => from_end(end),
                        (false, false) => start + at,
                    }
                }
                (Some(start), None) => start + at,
                (None, Some(end)) => from_end(end),
                (None, None) => at,
            }
        };
        let x = axis(
            [s.left.clone(), s.right.clone()],
            [s.margin_left.clone(), s.margin_right.clone()],
            f.x,
            f.width,
            ends.0,
            width,
            true,
        );
        let y = axis(
            [s.top.clone(), s.bottom.clone()],
            [s.margin_top.clone(), s.margin_bottom.clone()],
            f.y,
            f.height,
            ends.1,
            height,
            false,
        );
        Some((x, y))
    }

    /// `view`'s node, unless it is laid out in a block of its own (a
    /// host's, or a popover's viewport): its frame is in that block's space,
    /// not its parent's.
    pub fn node_unblocked(&self, view: ViewId) -> Option<crate::NodeRef<'_>> {
        self.node(view).filter(|_| self.host_block(view).is_none())
    }

    /// Whether `view`'s block is the one the kernel implies (a top-layer
    /// box's, the viewport), no host having given it one.
    pub fn host_block_implied(&self, view: ViewId) -> bool {
        let Some(slot) = self.arena.slot_of(view) else {
            return false;
        };
        let held = self.arena.blocks.held(slot);
        held.is_some_and(|b| b.given.is_none())
    }

    /// The used size of `view`'s block as last laid out: the given length
    /// where the block is the size, and where it is a maximum the root's
    /// border box and margins with what the host clears around its content
    /// root, clamped only for `AtMost`. Zero while the root is hidden;
    /// `None` when `view` has no block.
    pub fn host_block_size(&self, view: ViewId) -> Option<(f32, f32)> {
        let slot = self.arena.slot_of(view)?;
        self.arena.blocks.held(slot).map(|b| b.used)
    }

    /// The blocks under `root`, each with its offer: the lengths the host
    /// gave less what it clears for a content root, which a maximum's root
    /// is sized inside (`apply`). A block whose clearance or bars' slots
    /// changed since its last layout, by a band or what its root takes of it,
    /// is restyled, and published whole where its content root's corner
    /// moved, which moves every frame in it (@ref LLP 1123 D3).
    fn blocks_under(&mut self, root: u32) -> Vec<(u32, Offer)> {
        let arena = &self.arena;
        let under = |b: &&Held| b.slot != root && arena.is_ancestor(root, b.slot);
        let clears: Vec<_> = (arena.blocks.0.iter().enumerate())
            .filter(|(_, b)| under(b))
            .map(|(i, b)| (i, cleared(arena, b)))
            .collect();
        let clears = clears.into_iter();
        clears
            .map(|(i, clear)| self.clear_block(i, clear))
            .collect()
    }

    /// Give the `i`th block its clearance `clear`, and return its root and
    /// offer: the block less the clearance. A clearance or bars' slots that
    /// changed restyle the block, and a clearance that moved the content
    /// root's corner publishes it whole.
    fn clear_block(&mut self, i: usize, clear: [f32; 4]) -> (u32, Offer) {
        let content = fixed_lengths(&self.arena, &self.arena.blocks.0[i], clear);
        let slots = bar_slots(&self.arena, &self.arena.blocks.0[i], clear);
        let held = &mut self.arena.blocks.0[i];
        let (slot, inner) = (held.slot, held.block.less(clear));
        let was = std::mem::replace(&mut held.cleared, clear);
        let slotted = std::mem::replace(&mut held.slots, slots) != slots;
        // Derive the container readers again where the content root's fixed lengths
        // moved, or where one was derived against others since the last layout
        let moved = std::mem::replace(&mut held.content, content) != content;
        let resized = held.read_off.replace(false) || moved;
        if was != clear || slotted {
            self.restyle_block(slot);
        }
        if resized {
            self.resize_content_root(slot);
        }
        if (was[0], was[3]) != (clear[0], clear[3]) {
            self.arena.flags_mut(slot).insert(NodeFlags::ATTACHED);
        }
        (
            slot,
            Offer::definite(inner.width.length(), inner.height.length()),
        )
    }

    /// Lay `root` out under `offer`, then each subtree with a block under
    /// it in its block, and publish them: one receipt, the page's nodes
    /// first and each block's after.
    pub(super) fn lay_out_roots(
        &mut self,
        root: u32,
        offer: Offer,
    ) -> Result<LayoutReceipt, LayoutError> {
        let mut receipt = layout::compute(
            &mut self.arena,
            engine(&mut self.layout),
            self.measurer.as_mut(),
            root,
            offer,
        )?;
        self.lay_out_blocks(root, &mut receipt)?;
        Ok(receipt)
    }

    /// Lay each subtree with a block under `root` out in its block and
    /// publish it, into `receipt`: after the page's nodes, or a content
    /// region's (a top-layer box leaves the region's trial).
    pub(super) fn lay_out_blocks(
        &mut self,
        root: u32,
        receipt: &mut LayoutReceipt,
    ) -> Result<(), LayoutError> {
        if !self.arena.blocks.any() {
            return Ok(());
        }
        for (slot, offer) in self.blocks_under(root) {
            let tree = engine(&mut self.layout);
            let provisional = tree.provisional_chrome();
            let part = layout::compute(&mut self.arena, tree, self.measurer.as_mut(), slot, offer);
            // One answer for the whole layout: a stand-in measure anywhere.
            tree.keep_provisional_chrome(provisional);
            let part = part?;
            let used = self.used_size(slot);
            let held = self.arena.blocks.held_mut(slot);
            let moved = held.is_some_and(|b| std::mem::replace(&mut b.used, used) != used);
            receipt.changed.extend(part.changed);
            receipt.updated.extend(part.updated);
            let key = self.arena.key(slot);
            if moved && !receipt.updated.contains(&key) {
                receipt.updated.push(key);
            }
            receipt.flow_changed.extend(part.flow_changed);
            receipt.flow_skipped.extend(part.flow_skipped);
            receipt.fragment_skipped = part.fragment_skipped;
            receipt.flow_passes += part.flow_passes;
            receipt.flow_comparisons += part.flow_comparisons;
        }
        Ok(())
    }

    /// Lay each subtree with a block under `root` out in the engine alone,
    /// publishing nothing: what a measurement of authored heights reads.
    pub(super) fn lay_out_blocks_unpublished(&mut self, root: u32) -> Result<(), LayoutError> {
        if !self.arena.blocks.any() {
            return Ok(());
        }
        for (slot, offer) in self.blocks_under(root) {
            if let Some(node) = self.arena.taffy(slot) {
                engine(&mut self.layout).compute(
                    node,
                    offer,
                    &self.arena,
                    self.measurer.as_mut(),
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn used_size(&self, slot: u32) -> (f32, f32) {
        let shown = !self.arena.flags(slot).has(NodeFlags::HIDDEN);
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref);
        let (Some(held), Some(node), Some(tree)) = (
            self.arena.blocks.held(slot),
            self.arena.taffy(slot).filter(|_| shown),
            tree,
        ) else {
            return (0.0, 0.0);
        };
        used_in(held.block, held.cleared, &tree.layout(node))
    }

    /// The used size `view`'s subtree would have in `block`, had its host
    /// given that block instead of the one it holds: what `host_block_size`
    /// would read after `set_host_block` and a layout, with the margins that
    /// layout resolved (a percentage is of the content root). A host weighing
    /// several blocks for one box (a popover's position-area options, each
    /// its own region) measures each, then gives the one it chose.
    ///
    /// The subtree is laid out as its block's root is, by the same path
    /// (`layout::compute`: the engine, flow and fragment settling, fitting,
    /// publication, then the solve of the observations read under it), in a
    /// copy of the arena and engine tree that is dropped after: no block,
    /// frame, observation or engine record the kernel holds changes,
    /// and the next layout does what it would have done. Only the text
    /// measurer, whose answers are a pure function of their inputs, may
    /// remember what it was asked. `view` must hold a block (its host's, or
    /// a top-layer box's viewport); zero where that layout would hide it,
    /// as `host_block_size` reads then.
    pub fn measure_block(
        &mut self,
        view: ViewId,
        block: HostBlock,
    ) -> Result<BlockMeasure, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        let lengths = [block.width.length(), block.height.length()];
        if !lengths.iter().all(|l| l.is_finite() && *l >= 0.0) {
            return Err(LayoutError::InvalidOffer.into());
        }
        if !self.arena.blocks.holds(slot) {
            return Err(
                LayoutError::HostBlock("only a block's root is measured in a block").into(),
            );
        }
        let tree = self.layout.as_deref().and_then(LayoutMirror::tree_ref);
        let (Some(node), Some(tree)) = (self.arena.taffy(slot), tree) else {
            return Err(LayoutError::HostBlock("not laid out yet").into());
        };
        // What only the kernel's next layout prepares, before it lays out,
        // is not a trial's to do: a block to imply or restyle (one whose
        // band moved its content root), a reader to measure (one whose band
        // changed), a symbol to size; nor is a change under the root since
        // the last layout (a text edit), which a measurer holding what that
        // layout showed would take as the text it shows. Until that layout
        // the host places by the last one.
        let (wants, moved) = self.implied_changes();
        let clear = clearance(&self.arena, slot);
        let restyle = (self.arena.blocks.held(slot)).is_some_and(|b| b.cleared != clear)
            || Blocks::slots_moved(&self.arena, slot);
        if restyle
            || !self.readers_measured(slot)
            || !wants.is_empty()
            || !moved.is_empty()
            || self.symbols_pending()
            || tree.is_dirty(node)
        {
            return Err(LayoutError::HostBlock("a layout is due first").into());
        }
        let placing = self.arena.blocks.given(slot).is_none();
        let (mut arena, mut trial) = (self.arena.trial_copy(), tree.clone());
        arena.blocks.set(slot, Some(block));
        trial.restyle(&arena, slot, node);
        trial.mark_dirty(node);
        // Lay the copies out in place of the kernel's own, so that the
        // readers under the root solve as a layout solves them (LLP 1123
        // D9), and with no content region, whose accepted content a
        // reader's restyle would invalidate
        let kept = (
            std::mem::replace(&mut self.arena, arena),
            self.layout.replace(Box::new(trial)),
            self.region.take(),
        );
        // Derive in the copies the owner segments' readers below a box the trial starts
        // placing, as giving it the block does
        if placing {
            self.segments_moved(slot);
        }
        let measured = self.measure_trial(slot, node, block);
        let trial = std::mem::replace(&mut self.arena, kept.0);
        (self.layout, self.region) = (kept.1, kept.2);
        // A measurer may hold what the trial measured under the text
        // revisions it gave, so no later revision may give one again
        self.arena.spend_text_revisions(&trial);
        measured
    }

    /// `measure_block`'s trial, on the copies in place of the kernel's own:
    /// the root `slot` laid out in `block` as a layout lays out each block,
    /// with its readers solved.
    fn measure_trial(
        &mut self,
        slot: u32,
        node: taffy::NodeId,
        block: HostBlock,
    ) -> Result<BlockMeasure, KernelError> {
        let key = self.arena.key(slot);
        // Each pass clears what the root's reservations then say, which can
        // read the observations the solve moves
        let relayout = &mut |k: &mut Kernel| -> Result<LayoutReceipt, KernelError> {
            let i = (k.arena.blocks.0.iter()).position(|b| b.slot == slot);
            let i = i.expect("the measured root holds its block");
            let clear = cleared(&k.arena, &k.arena.blocks.0[i]);
            let (_, offer) = k.clear_block(i, clear);
            let tree = engine(&mut k.layout);
            let mut receipt =
                layout::compute(&mut k.arena, tree, k.measurer.as_mut(), slot, offer)?;
            // Measure the readers again where the block's used size moved
            // while its root's frame is unchanged
            if !receipt.updated.contains(&key) {
                receipt.updated.push(key);
            }
            Ok(receipt)
        };
        let mut receipt = relayout(self)?;
        self.solve_observations(slot, relayout, &[], &mut receipt)?;
        if self.arena.flags(slot).has(NodeFlags::HIDDEN) {
            return Ok(BlockMeasure::default());
        }
        let l = engine(&mut self.layout).layout(node);
        let clear = self.arena.blocks.held(slot).map_or([0.0; 4], |b| b.cleared);
        Ok(BlockMeasure {
            used: used_in(block, clear, &l),
            border: (l.size.width, l.size.height),
            margin: [l.margin.top, l.margin.right, l.margin.bottom, l.margin.left],
        })
    }
}

/// What `Kernel::measure_block` answers: the used size, the border box, and
/// the margins, top, right, bottom and left, as the layout resolved them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BlockMeasure {
    /// The used size, as `host_block_size` reads it.
    pub used: (f32, f32),
    /// The border box's width and height.
    pub border: (f32, f32),
    /// The margins, top, right, bottom and left.
    pub margin: [f32; 4],
}

/// A block root's used size from its engine layout: the given length where
/// the block is the size, and where it is a maximum or a fit its border box
/// and margins with what the host cleared around its content root, `clear`,
/// clamped only for `AtMost`.
fn used_in(block: HostBlock, clear: [f32; 4], l: &taffy::tree::Layout) -> (f32, f32) {
    let [top, right, bottom, left] = clear;
    let axis = |axis: BlockAxis, end: f32| match axis {
        BlockAxis::Is(length) | BlockAxis::Fill(length) => length,
        BlockAxis::AtMost(most) => end.clamp(0.0, most),
        BlockAxis::Fit(_) | BlockAxis::Region(_) | BlockAxis::Content(_) => end.max(0.0),
    };
    (
        axis(
            block.width,
            left + l.location.x + l.size.width + l.margin.right + right,
        ),
        axis(
            block.height,
            top + l.location.y + l.size.height + l.margin.bottom + bottom,
        ),
    )
}
