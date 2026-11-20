//! The keyboard (LLP 1123 D7): what the host reports of it for each content
//! owner and for the session, the depth it occupies there, what an owner's
//! root clears of it, which the first root's `interactive-widget` decides
//! (LLP 1008 §9), and what each box reads of it.
//!
//! The host reads the keyboard from guides of its own in each window and
//! converts their frame into each owner's coordinates (D7, "What the host
//! reports"), so nothing here is a measurement of the kernel's: a frame, a
//! docked state and a policy, each as reported.
use super::area::{scroll_axes, scroll_view, Space};
use super::block::BlockAxis;
use super::{ignore, Kernel};
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::generated::SafeAreaIgnore;
use crate::id::{IdMap, ViewId};

/// What the host reports of the keyboard for one content owner, in its
/// block's space (D2), or for the session, in the document's: a docked
/// keyboard's frame, or an undocked keyboard's rectangle, which occupies
/// nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyboard {
    /// Its x, y, width and height.
    pub frame: [f32; 4],
    /// Whether UIKit docks it: the host's guide that does not follow
    /// undocked keyboards has height (D7).
    pub docked: bool,
}

/// What layout does with the keyboard: the first root's
/// `interactive-widget` (LLP 1008 §9), which the host reads and reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardPolicy {
    /// Each owner's content root ends at what its root clears on the bottom
    /// edge, the keyboard's occupancy included (D3).
    ResizesContent,
    /// Layout keeps its size, and scrollers take the keyboard as a content
    /// inset (D14).
    ResizesVisual,
    /// Nothing is cleared; a box places itself by its observation.
    OverlaysContent,
}

/// The keyboard as the host last reported it.
#[derive(Debug, Clone, Default)]
pub(crate) struct Keyboards {
    /// Each owner root's keyboard, absent while it is dismissed, with the
    /// width and height of the owner view it was converted into.
    owners: IdMap<u32, (Keyboard, (f32, f32))>,
    /// The policy; `None` until the host reports one, when the keyboard lays
    /// nothing out, as a host that gives no block lays out as before.
    policy: Option<KeyboardPolicy>,
    /// The session's keyboard, in the document's space, which the boxes
    /// outside every owner read (D7).
    session: Option<Keyboard>,
}

impl Keyboards {
    /// What a reset keeps: the policy and the session's keyboard, which are
    /// the session's. Each owner's keyboard goes with the tree, as its band
    /// does.
    pub(crate) fn after_reset(&self) -> Self {
        Keyboards {
            policy: self.policy,
            session: self.session,
            ..Default::default()
        }
    }

    /// Forget a freed owner's keyboard, whose slot a new node may take.
    pub(crate) fn forget(&mut self, slot: u32) {
        self.owners.remove(&slot);
    }
}

/// The depth the keyboard occupies on `slot`'s bottom edge: a docked
/// keyboard's overlap with the owner's allocation, the owner view's bounds
/// its frame was converted into, from their bottom up to the keyboard's top
/// (D7). Nothing for an undocked keyboard, for none, or for a box with no
/// allocation. The bounds are those when the host reported it, so a trial
/// that weighs another block keeps the occupancy the owner has.
pub(crate) fn occupancy(arena: &NodeArena, slot: u32) -> f32 {
    match arena.observations.keyboards.owners.get(&slot) {
        Some((keyboard, bounds)) => overlap(keyboard, *bounds),
        None => 0.0,
    }
}

/// The depth a docked `keyboard` occupies of bounds `(width, height)` in its
/// space: its overlap with them, from their bottom up to its top; nothing
/// for an undocked keyboard or one clear of them.
fn overlap(keyboard: &Keyboard, (width, height): (f32, f32)) -> f32 {
    let [x, y, w, h] = keyboard.frame;
    let overlaps = x < width && x + w > 0.0 && y < height && y + h > 0.0;
    match keyboard.docked && overlaps {
        true => height - y.max(0.0),
        false => 0.0,
    }
}

/// How deep the docked keyboard lies in `space`, up from its bottom: its
/// owner's occupancy, or outside every owner the session's keyboard's
/// overlap with the viewport (D7).
fn docked_depth(arena: &NodeArena, space: &Space) -> f32 {
    match space.owner {
        Some(owner) => occupancy(arena, owner),
        None => (arena.observations.keyboards.session).map_or(0.0, |k| overlap(&k, space.size)),
    }
}

/// What `slot`'s host clears of the keyboard around its content root, top
/// right bottom left: on the bottom edge its occupancy under
/// `resizes-content`, unless the root takes the keyboard region (D3, D4).
/// It covers the band and the root's additional inset where it is deeper,
/// and the root's safe-area padding stacks on it ([`super::block`]).
pub(crate) fn cover(arena: &NodeArena, slot: u32) -> [f32; 4] {
    let keyboards = &arena.observations.keyboards;
    let resizes = keyboards.policy == Some(KeyboardPolicy::ResizesContent);
    if !resizes || keyboards.owners.is_empty() {
        return [0.0; 4];
    }
    match ignore::edges(arena, slot, SafeAreaIgnore::Keyboard)[2] {
        true => [0.0; 4],
        false => [0.0, 0.0, occupancy(arena, slot), 0.0],
    }
}

/// How far in from the bottom of `owner`'s allocation its keyboard layer
/// reaches, the layer that every box inside the root observes or takes on
/// that edge (D4, D9): the occupancy under `resizes-content`, whether or not
/// the root itself takes the keyboard region, as every UIKit view has a
/// keyboard guide of its own whatever its superviews do; nothing under the
/// other policies, where the keyboard region lays nothing out.
pub(crate) fn layer(arena: &NodeArena, owner: u32) -> f32 {
    match arena.observations.keyboards.policy {
        Some(KeyboardPolicy::ResizesContent) => occupancy(arena, owner),
        _ => 0.0,
    }
}

/// The keyboard layer that reaches `slot` in `space`: its owner's ([`layer`]),
/// past every scroller between them that passes it ([`past_scrollers`]), as a
/// scroller takes the area that reaches it (D9). Content at rest below a
/// scroller's fold lies in the layer only by its scroll offset at zero, so
/// a scroller that does not pass the layer gives its content none.
pub(crate) fn layer_at(arena: &NodeArena, slot: u32, space: &Space) -> f32 {
    let layer = space.owner.map_or(0.0, |owner| layer(arena, owner));
    match layer > 0.0 && past_scrollers(arena, slot, space, layer) {
        true => layer,
        false => 0.0,
    }
}

/// Whether the docked keyboard, `depth` deep in `space`, reaches `slot` past
/// the scrollers between it and its layout's root, each of which passes it
/// ([`passes`]).
fn past_scrollers(arena: &NodeArena, slot: u32, space: &Space, depth: f32) -> bool {
    let policy = arena.observations.keyboards.policy;
    if !matches!(
        policy,
        Some(KeyboardPolicy::ResizesContent | KeyboardPolicy::ResizesVisual)
    ) {
        return true;
    }
    let mut above = (space.root != Some(slot))
        .then(|| arena.parent(slot))
        .flatten();
    while let Some(s) = above {
        if scroll_view(arena, s) && !passes(arena, s, space, depth) {
            return false;
        }
        above = (space.root != Some(s)).then(|| arena.parent(s)).flatten();
    }
    true
}

/// Whether the scroller `s` passes the docked keyboard, `depth` deep in
/// `space`, to its content rather than taking it (D7, D9). Under
/// `resizes-content` it passes it where it ignores the keyboard region on
/// the bottom edge and its box reaches the keyboard's top; under
/// `resizes-visual` a vertical scroller that observes the keyboard region
/// takes it as its content inset (D14), and one that ignores it, or one that
/// scrolls only sideways, passes it; under the other policies none takes it.
pub(super) fn passes(arena: &NodeArena, s: u32, space: &Space, depth: f32) -> bool {
    let ignores = || ignore::edges(arena, s, SafeAreaIgnore::Keyboard)[2];
    match arena.observations.keyboards.policy {
        Some(KeyboardPolicy::ResizesContent) => {
            let tolerance = arena.observations.tolerance();
            ignores() && space.edges(arena, s)[2] <= depth + tolerance
        }
        Some(KeyboardPolicy::ResizesVisual) => ignores() || !scroll_axes(arena, s).1,
        _ => true,
    }
}

/// D14's keyboard term for the scroller `s`, whose bottom edge its reach
/// moves `reach` points down: under `resizes-visual`, where it takes the
/// docked keyboard that reaches it, the keyboard's overlap with its reached
/// box, measured up from the space's bottom as the occupancy is (D7); else 0.
pub(super) fn taken(arena: &NodeArena, s: u32, space: &Space, reach: f32) -> f32 {
    if arena.observations.keyboards.policy != Some(KeyboardPolicy::ResizesVisual) {
        return 0.0;
    }
    let depth = docked_depth(arena, space);
    if depth <= 0.0 || passes(arena, s, space, depth) || !past_scrollers(arena, s, space, depth) {
        return 0.0;
    }
    super::area::depth([0.0, depth], space.edges(arena, s)[2] - reach)
}

/// Whether the scroller `s` passes a docked keyboard in `space` to its
/// content, which the solve keeps so as to measure that content again when
/// it changes (`check_scrollers`).
pub(super) fn passing(arena: &NodeArena, s: u32, space: &Space) -> bool {
    let depth = docked_depth(arena, space);
    depth > 0.0 && passes(arena, s, space, depth)
}

/// What `slot` reads of the keyboard in `space` (D7), each on every edge:
/// the docked keyboard's guide (`--exact-keyboard-inset-*`), then the guide
/// that follows an undocked keyboard too (`--exact-keyboard-undocked-inset-*`);
/// and the most each can be, the keyboard's depth across each edge. As a
/// view's guide is the keyboard's frame in its coordinates, not clipped to
/// it, a reading is not limited by the box's size. The keyboard is its
/// owner's, or for a box outside every owner the session's; the docked one
/// is measured up from the space's bottom, as its occupancy is, and reaches
/// no box past a scroller that takes it. `None` where the kernel is not told
/// where the box lies ([`Space::placed`]).
pub(crate) fn readings(
    arena: &NodeArena,
    slot: u32,
    space: &Space,
) -> Option<([[f32; 4]; 2], [f32; 4])> {
    if !space.placed() {
        return None;
    }
    let keyboards = &arena.observations.keyboards;
    let keyboard = match space.owner {
        Some(owner) => keyboards.owners.get(&owner).map(|(k, _)| *k),
        None => keyboards.session,
    };
    let Some(keyboard) = keyboard else {
        return Some(([[0.0; 4]; 2], [0.0; 4]));
    };
    let f = arena.frame(slot);
    let (x, y) = (f.x + space.origin.0, f.y + space.origin.1);
    let [kx, ky, kw, kh] = keyboard.frame;
    if !keyboard.docked {
        let followed = crossing(
            [x, y, x + f.width, y + f.height],
            [kx, ky, kx + kw, ky + kh],
        );
        return Some(([[0.0; 4], followed], [kh, kw, kh, kw]));
    }
    let depth = docked_depth(arena, space);
    let reach = match depth > 0.0 && past_scrollers(arena, slot, space, depth) {
        true => super::area::depth([0.0, depth], space.edges(arena, slot)[2]),
        false => 0.0,
    };
    let docked = [0.0, 0.0, reach, 0.0];
    Some(([docked, docked], [0.0, 0.0, depth, 0.0]))
}

/// How far an undocked keyboard `k` reaches into a box `b`, each `[left,
/// top, right, bottom]` in one space, top right bottom left: on each edge it
/// crosses or meets, from that edge to the keyboard's far side, inside the
/// box; 0 on an edge it does not reach, on both edges
/// of an axis across which it spans the box, as a keyboard as wide as the box
/// enters it from neither side, and on every edge where it lies wholly inside
/// the box or clear of it.
fn crossing(b: [f32; 4], k: [f32; 4]) -> [f32; 4] {
    let [bx0, by0, bx1, by1] = b;
    let [kx0, ky0, kx1, ky1] = k;
    if kx0 >= bx1 || kx1 <= bx0 || ky0 >= by1 || ky1 <= by0 {
        return [0.0; 4];
    }
    let (spans_x, spans_y) = (kx0 <= bx0 && kx1 >= bx1, ky0 <= by0 && ky1 >= by1);
    let across = |crosses: bool, depth: f32| if crosses { depth } else { 0.0 };
    [
        across(!spans_y && ky0 <= by0, ky1 - by0),
        across(!spans_x && kx1 >= bx1, bx1 - kx0),
        across(!spans_y && ky1 >= by1, by1 - ky0),
        across(!spans_x && kx0 <= bx0, kx1 - bx0),
    ]
}

impl Kernel {
    /// Set the keyboard of the content owner whose root is `view`, in its
    /// block's space, or `None` while it is dismissed (LLP 1123 D2's owner
    /// update, D7). The frame is in the owner view as its bounds are now: on
    /// each axis the block's length where the block is the size, and its last
    /// used size where it is a maximum or fits. The host sets a new block
    /// before it reports the keyboard in it, and reports the keyboard again
    /// whenever those bounds change. Returns whether a layout is owed: what
    /// the root clears of it, or the layer its boxes take, changed, and that
    /// moved the content root, or some box reads an observation or ignores
    /// (D4, D9); or a box in the owner reads the keyboard (D7). A frame
    /// that is not finite, or whose size is negative, is refused, keeping
    /// the last.
    /// A host reports this field with the owner's others through
    /// [`Kernel::update_owner`], which applies them whole or not at all.
    pub fn set_owner_keyboard(
        &mut self,
        view: ViewId,
        keyboard: Option<Keyboard>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if let Some(k) = &keyboard {
            check(k)?;
        }
        let (cleared, laid) = (super::block::clearance(&self.arena, slot), self.laid(slot));
        let bounds = self.view_bounds(slot);
        let owners = &mut self.arena.observations.keyboards.owners;
        let reported = keyboard.map(|k| (k, bounds));
        if owners.get(&slot).copied() == reported {
            return Ok(false);
        }
        match reported {
            Some(r) => owners.insert(slot, r),
            None => owners.remove(&slot),
        };
        let observations = &mut self.arena.observations;
        let read = observations
            .forget_keyboard_readings(&self.arena.readers.observed, |o| o == Some(slot));
        Ok(self.laid_again(slot, cleared, laid) || read)
    }

    /// Set the session's keyboard, in the document's space, or `None` while
    /// it is dismissed, which the boxes outside every owner read (LLP 1123
    /// D7). Returns whether a layout is owed: one of those boxes reads the
    /// keyboard. A frame that is not finite, or whose size is negative, is
    /// refused, keeping the last.
    pub fn set_session_keyboard(
        &mut self,
        keyboard: Option<Keyboard>,
    ) -> Result<bool, KernelError> {
        if let Some(k) = &keyboard {
            check(k)?;
        }
        let keyboards = &mut self.arena.observations.keyboards;
        if keyboards.session == keyboard {
            return Ok(false);
        }
        keyboards.session = keyboard;
        let observations = &mut self.arena.observations;
        Ok(observations.forget_keyboard_readings(&self.arena.readers.observed, |o| o.is_none()))
    }

    /// Set the keyboard policy, the first root's `interactive-widget`, or
    /// `None` where the host reports none (LLP 1008 §9, LLP 1123 D7).
    /// Returns whether a layout is owed, as [`Kernel::set_owner_keyboard`]
    /// does, for any owner, or some box reads the keyboard, whose scrollers
    /// may take it now or no longer.
    pub fn set_keyboard_policy(&mut self, policy: Option<KeyboardPolicy>) -> bool {
        if self.arena.observations.keyboards.policy == policy {
            return false;
        }
        let owners: Vec<u32> = (self.arena.observations.keyboards.owners.keys())
            .copied()
            .collect();
        let before: Vec<_> = (owners.iter())
            .map(|&s| (super::block::clearance(&self.arena, s), self.laid(s)))
            .collect();
        self.arena.observations.keyboards.policy = policy;
        let observations = &mut self.arena.observations;
        let mut owed =
            observations.forget_keyboard_readings(&self.arena.readers.observed, |_| true);
        for (&slot, (cleared, laid)) in owners.iter().zip(before) {
            owed |= self.laid_again(slot, cleared, laid);
        }
        owed
    }

    /// The width and height of `slot`'s owner view: on each axis the length
    /// its host gave where the block is the size, and otherwise its used size
    /// from the last layout, which the host made the view; 0 for no block.
    fn view_bounds(&self, slot: u32) -> (f32, f32) {
        let Some(block) = self.arena.blocks.given(slot) else {
            return (0.0, 0.0);
        };
        let used = self.used_size(slot);
        let length = |axis: BlockAxis, used: f32| match axis {
            BlockAxis::Is(length) => length,
            _ => used,
        };
        (length(block.width, used.0), length(block.height, used.1))
    }

    /// What `slot`'s keyboard lays out: what its root clears of it, and the
    /// layer its boxes take.
    fn laid(&self, slot: u32) -> ([f32; 4], f32) {
        (cover(&self.arena, slot), layer(&self.arena, slot))
    }

    /// Whether `slot`'s keyboard laying out something other than `laid`, its
    /// clearance having been `cleared`, owes a layout: it moved the content
    /// root, or some box reads an observation or ignores, whose area or
    /// layer moved. Its readers and takers are measured again where it did.
    fn laid_again(&mut self, slot: u32, cleared: [f32; 4], laid: ([f32; 4], f32)) -> bool {
        if self.laid(slot) == laid {
            return false;
        }
        self.arena.observations.invalidate_band(Some(slot));
        super::block::clearance(&self.arena, slot) != cleared || self.band_read()
    }
}

/// Refuse a frame that is not finite or has a negative size.
pub(super) fn check(keyboard: &Keyboard) -> Result<(), KernelError> {
    let [.., w, h] = keyboard.frame;
    match keyboard.frame.iter().all(|v| v.is_finite()) && w >= 0.0 && h >= 0.0 {
        true => Ok(()),
        false => Err(LayoutError::Keyboard(
            "the frame is not finite or has a negative size: report a docked keyboard's frame, \
             or an undocked one's rectangle, in the owner's block space or the document's, and \
             none while it is dismissed",
        )
        .into()),
    }
}

/// Whether a reader's `components` read the keyboard.
pub(crate) fn reads_keyboard(components: crate::style::calc::Components) -> bool {
    use crate::style::calc::Family;
    components & (Family::Keyboard.bits() | Family::KeyboardUndocked.bits()) != 0
}
