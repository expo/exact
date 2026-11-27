//! @ref LLP 1123 D16 — the window's aspect-fit guide, which a host gives the session's first root:
//! the root's border box is the guide's rectangle, its size filled as a resizable pane's block fills
//! it (`BlockAxis::Fill`) and its origin where publication starts the root, while the document's
//! viewport and every fact of it stay the offer's.

use super::Kernel;
use crate::error::{KernelError, LayoutError};
use crate::id::{NodeKey, Offer, ViewId};
use crate::kernel::{BlockAxis, HostBlock};

/// The guide's rectangle the host fits a root to, `[x, y, width, height]` in the document's space,
/// and the origin that root was last published at, each by the root's key, so a root that replaced
/// it takes neither.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RootFits {
    fit: Option<(NodeKey, [f32; 4])>,
    published: Option<(NodeKey, (f32, f32))>,
}

impl RootFits {
    /// The block that fills the root `key`'s border box with the guide, where a host fits it.
    pub(crate) fn block(&self, key: NodeKey) -> Option<HostBlock> {
        match self.fit {
            Some((k, [_, _, width, height])) if k == key => Some(HostBlock {
                tactic: Default::default(),
                width: BlockAxis::Fill(width),
                height: BlockAxis::Fill(height),
            }),
            _ => None,
        }
    }

    /// Where publication starts the root `key`, the guide's origin or none, and whether that moved
    /// since it was last published there, which republishes its whole subtree.
    pub(crate) fn publish(&mut self, key: NodeKey) -> ((f32, f32), bool) {
        let origin = match self.fit {
            Some((k, [x, y, _, _])) if k == key => (x, y),
            _ => (0.0, 0.0),
        };
        let before = match self.published {
            Some((k, at)) if k == key => at,
            _ => (0.0, 0.0),
        };
        self.published = (origin != (0.0, 0.0)).then_some((key, origin));
        (origin, origin != before)
    }
}

impl Kernel {
    /// Fit the root `root` to `rect`, the window's aspect-fit guide in the document's space,
    /// `[x, y, width, height]`, or to the offer again (`None`). Its size is the root's border box
    /// and offer, and its origin where the root is published; the viewport stays the offer's. Refuses a rectangle
    /// that is not finite or not of positive size, and any view but a root. Returns whether it
    /// changed, which owes a layout.
    pub fn set_root_fit(
        &mut self,
        root: ViewId,
        rect: Option<[f32; 4]>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(root)
            .ok_or(LayoutError::UnknownView(root))?;
        if let Some(r) = rect {
            if !r.iter().all(|v| v.is_finite()) || r[2] <= 0.0 || r[3] <= 0.0 {
                return Err(LayoutError::InvalidOffer.into());
            }
            if !self.arena.is_root(slot) {
                return Err(LayoutError::HostBlock(
                    "only a root takes the window's aspect-fit guide",
                )
                .into());
            }
        }
        let next = rect.map(|r| (self.arena.key(slot), r));
        let fits = self.arena.blocks.root_fits_mut();
        if fits.fit == next {
            return Ok(false);
        }
        fits.fit = next;
        // Its engine style fills the guide, or is its own again
        if let (Some(node), Some(layout)) = (self.arena.taffy(slot), self.layout.as_deref_mut()) {
            layout.restyle(&self.arena, slot, node);
            layout.mark_dirty(node);
        }
        Ok(true)
    }

    /// The offer the root `slot` is laid out in: the guide's size where a host fits it, else `offer`.
    pub(super) fn root_offer(&self, slot: u32, offer: Offer) -> Offer {
        match self.arena.blocks.root_fits().fit {
            Some((key, [_, _, width, height])) if key == self.arena.key(slot) => {
                Offer::definite(width, height)
            }
            _ => offer,
        }
    }
}
