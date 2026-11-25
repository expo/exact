//! Reserved regions (@ref LLP 1123 D16): the regions UIKit reserves in an owner's view, a fold's
//! division or an occlusion, each with its identity, its frame including its margins, its margins
//! and whether it is active, as the owner's host reports them in its update and the session's in
//! its own. D6's `exactGeometry` answers them; no layout reads them, the fold reaching layout
//! through the owner's segments.

use super::area::{holder, Holder};
use super::Kernel;
use crate::id::ViewId;
use std::sync::Arc;

/// One reserved region in its owner's block space, or the document's for the session's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReservedRegion {
    /// Its identity, the host's for the region's life.
    pub id: u64,
    /// What reserves it.
    pub kind: RegionKind,
    /// Its rectangle including its margins, x y width height.
    pub frame: [f32; 4],
    /// The margins its frame includes around the reserved rectangle for interactive content, top
    /// right bottom left.
    pub margins: [f32; 4],
    /// Whether it is active now, as a fold's division is while the hinge is partly open.
    pub active: bool,
}

/// UIKit's `UIViewReservedRegionKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    /// Where content should divide in two, as at a fold.
    Division,
    /// What an element occludes.
    Occlusion,
}

impl RegionKind {
    /// Every kind, in wire order.
    pub const ALL: [RegionKind; 2] = [RegionKind::Division, RegionKind::Occlusion];

    /// Its name in D6's `regions`.
    pub fn name(self) -> &'static str {
        match self {
            RegionKind::Division => "division",
            RegionKind::Occlusion => "occlusion",
        }
    }
}

/// Why a host's regions are refused, if they are: a nonfinite number, a frame of negative size or
/// negative margins.
pub(super) fn refusal(regions: Option<&[ReservedRegion]>) -> Option<&'static str> {
    let bad = |r: &ReservedRegion| {
        let finite = r.frame.iter().chain(&r.margins).all(|v| v.is_finite());
        !finite || r.frame[2] < 0.0 || r.frame[3] < 0.0 || r.margins.iter().any(|&m| m < 0.0)
    };
    regions?.iter().any(bad).then_some(
        "a reserved region with a nonfinite number, a frame of negative size or negative margins; report each as UIKit gives it",
    )
}

impl Kernel {
    /// The reserved regions of the owner holding the box `view`, or the document's outside every
    /// owner, a document top-layer box's content included, as its host last reported them;
    /// `None` where it reported none, as a host without reserved regions does.
    pub fn reserved_regions(&self, view: ViewId) -> Option<Arc<[ReservedRegion]>> {
        let slot = self.arena.slot_of(view)?;
        let observations = &self.arena.observations;
        match holder(&self.arena, slot) {
            Holder::Owner(owner) => observations.regions.get(&owner).cloned(),
            Holder::TopLayer(_) | Holder::Session => observations.session_regions.clone(),
        }
    }

    /// Hold `regions` for the owner whose root is `slot`, or for the session where it is `None`.
    pub(super) fn set_regions(&mut self, slot: Option<u32>, regions: Option<Vec<ReservedRegion>>) {
        let observations = &mut self.arena.observations;
        let regions: Option<Arc<[ReservedRegion]>> = regions.map(Into::into);
        match (slot, regions) {
            (Some(s), Some(r)) => drop(observations.regions.insert(s, r)),
            (Some(s), None) => drop(observations.regions.remove(&s)),
            (None, r) => observations.session_regions = r,
        }
    }
}
