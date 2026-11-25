//! The largest band an owner's host has reported for the owner's configuration (@ref LLP 1123
//! D9, "Moving edges"), which a reader under `-exact-safe-area-track: observed-max` reads, so its
//! layout settles against the band a collapsing title or a minimizing bar leaves at its largest
//! while its readouts stay live.

use super::area::Space;
use super::Kernel;
use crate::arena::NodeArena;
use crate::generated::SafeAreaTrack;

impl Kernel {
    /// Fold the band just set for the owner whose root is `slot`, or for the session where it is
    /// `None`, into the largest one of its `configuration`, which the host's identity of the
    /// owner's bars, title display mode, size classes, orientation and window size gives: a new
    /// configuration starts again from the band, and none or an unavailable band holds none.
    /// Returns whether a layout is owed: the largest band moved and some box reads a band.
    pub(super) fn track_band(&mut self, slot: Option<u32>, configuration: Option<u64>) -> bool {
        let observations = &mut self.arena.observations;
        let band = match slot {
            Some(s) => observations.band(s),
            None => Some(observations.session()),
        };
        let held = match slot {
            Some(s) => observations.max_bands.get(&s).copied(),
            None => observations.session_max,
        };
        let next = match (configuration, band) {
            (Some(c), Some(b)) => Some(match held {
                Some((h, m)) if h == c => (c, std::array::from_fn(|e| m[e].max(b[e]))),
                _ => (c, b),
            }),
            _ => None,
        };
        if next == held {
            return false;
        }
        match (slot, next) {
            (Some(s), Some(n)) => drop(observations.max_bands.insert(s, n)),
            (Some(s), None) => drop(observations.max_bands.remove(&s)),
            (None, n) => observations.session_max = n,
        }
        observations.invalidate_band(slot);
        self.band_read()
    }
}

/// The space `slot` measures its input in where it tracks the largest band: `space` with that
/// band where its owner's host, or the session's outside every owner, reported one for the
/// current configuration; `None` for a reader that reads the band live, or where there is none.
pub(super) fn tracked(arena: &NodeArena, slot: u32, space: &Space) -> Option<Space> {
    if arena.style(slot).rare.safe_area_track != SafeAreaTrack::ObservedMax {
        return None;
    }
    let observations = &arena.observations;
    let (_, max) = match space.owner {
        Some(owner) => observations.max_bands.get(&owner).copied(),
        None => observations.session_max,
    }?;
    // A top-layer box its host places has no band, and so no largest one
    space.band?;
    Some(Space {
        band: Some(max),
        ..*space
    })
}
