//! The keyboard (LLP 1123 D7): what the host reports of it for each content
//! owner, the depth it occupies there, and what an owner's root clears of
//! it, which the first root's `interactive-widget` decides (LLP 1008 §9).
//!
//! The host reads the keyboard from guides of its own in each window and
//! converts their frame into each owner's coordinates (D7, "What the host
//! reports"), so nothing here is a measurement of the kernel's: a frame, a
//! docked state and a policy, each as reported.
use super::area::{scroll_view, Space};
use super::block::BlockAxis;
use super::{ignore, Kernel};
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::generated::SafeAreaIgnore;
use crate::id::{IdMap, ViewId};

/// What the host reports of the keyboard for one content owner, in its
/// block's space (D2): a docked keyboard's frame, or an undocked keyboard's
/// rectangle, which occupies nothing.
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
}

impl Keyboards {
    /// What a reset keeps: the policy, which is the session's. Each owner's
    /// keyboard goes with the tree, as its band does.
    pub(crate) fn after_reset(&self) -> Self {
        Keyboards {
            policy: self.policy,
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
    let keyboards = &arena.observations.keyboards;
    let Some(&(keyboard, (width, height))) = keyboards.owners.get(&slot) else {
        return 0.0;
    };
    let [x, y, w, h] = keyboard.frame;
    let overlaps = x < width && x + w > 0.0 && y < height && y + h > 0.0;
    match keyboard.docked && overlaps {
        true => height - y.max(0.0),
        false => 0.0,
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
/// past every scroller between them that passes it, one that ignores the
/// keyboard region on the bottom edge and whose box reaches the layer, as a
/// scroller takes the area that reaches it (D9). Content at rest below a
/// scroller's fold lies in the layer only by its scroll offset at zero, so
/// a scroller that does not pass the layer gives its content none.
pub(crate) fn layer_at(arena: &NodeArena, slot: u32, space: &Space) -> f32 {
    let layer = space.owner.map_or(0.0, |owner| layer(arena, owner));
    if layer <= 0.0 {
        return 0.0;
    }
    let tolerance = arena.observations.tolerance();
    let mut above = (space.root != Some(slot))
        .then(|| arena.parent(slot))
        .flatten();
    while let Some(s) = above {
        if scroll_view(arena, s) {
            let ignores = ignore::edges(arena, s, SafeAreaIgnore::Keyboard)[2];
            if !ignores || space.edges(arena, s)[2] > layer + tolerance {
                return 0.0;
            }
        }
        above = (space.root != Some(s)).then(|| arena.parent(s)).flatten();
    }
    layer
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
    /// (D4, D9). A frame that is not finite, or whose size is negative, is
    /// refused, keeping the last.
    pub fn set_owner_keyboard(
        &mut self,
        view: ViewId,
        keyboard: Option<Keyboard>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        let valid = |k: &Keyboard| {
            let [.., w, h] = k.frame;
            k.frame.iter().all(|v| v.is_finite()) && w >= 0.0 && h >= 0.0
        };
        if keyboard.as_ref().is_some_and(|k| !valid(k)) {
            return Err(LayoutError::Keyboard(
                "the frame is not finite or has a negative size: report a docked keyboard's \
                 frame, or an undocked one's rectangle, in the owner's block space, and none \
                 while it is dismissed",
            )
            .into());
        }
        let (cleared, laid) = (super::block::clearance(&self.arena, slot), self.laid(slot));
        let bounds = self.view_bounds(slot);
        let owners = &mut self.arena.observations.keyboards.owners;
        match keyboard {
            Some(k) => owners.insert(slot, (k, bounds)),
            None => owners.remove(&slot),
        };
        Ok(self.laid_again(slot, cleared, laid))
    }

    /// Set the keyboard policy, the first root's `interactive-widget`, or
    /// `None` where the host reports none (LLP 1008 §9, LLP 1123 D7).
    /// Returns whether a layout is owed, as [`Kernel::set_owner_keyboard`]
    /// does, for any owner.
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
        let mut owed = false;
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
