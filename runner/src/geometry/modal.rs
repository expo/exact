use super::*;

#[derive(Debug, Clone)]
pub(super) struct Backdrop {
    key: NodeKey,
    bounds: [f64; 4],
    excluding: Option<[f64; 4]>,
}

impl Scrolled {
    /// The front modal's native backdrop hit region in viewport points.
    /// The host omits it when no backdrop accepts input. It is independent
    /// of the dialog's content clip and authored pointer-event inheritance.
    pub fn top_layer_backdrop(&mut self, kernel: &Kernel, backdrop: Option<TopLayerBackdrop>) {
        self.backdrop = backdrop.and_then(|(view, bounds, excluding)| {
            kernel
                .arena()
                .key_of(view)
                .filter(|_| valid_rect(&bounds) && excluding.as_ref().is_none_or(valid_rect))
                .map(|key| Backdrop {
                    key,
                    bounds,
                    excluding,
                })
        });
    }
}

pub(super) fn backdrop_hit(
    kernel: &Kernel,
    scrolled: &Scrolled,
    live: Option<&[u32]>,
    x: f64,
    y: f64,
) -> Option<ViewId> {
    let backdrop = scrolled.backdrop.as_ref()?;
    let arena = kernel.arena();
    let slot = arena.resolve(backdrop.key)?;
    let contains = |[left, top, width, height]: [f64; 4]| {
        x >= left && x < left + width && y >= top && y < top + height
    };
    (live?.contains(&slot)
        && contains(backdrop.bounds)
        && !backdrop.excluding.is_some_and(contains))
    .then(|| arena.local_id(slot))
}

/// The live part of the top layer starts at the frontmost modal dialog or
/// the first box of the host's frontmost presentation (`top_layer_frames`):
/// whichever of its routes it shows, and what is open above it.
/// Everything else, below it in the top layer or in the document, is
/// behind it and inert to structural reads, as UIKit's dimming view and
/// the host's `inertBehind` make it to touch. None while no presentation
/// is open.
pub(super) fn modal_live(kernel: &Kernel, scrolled: &Scrolled) -> Option<Vec<u32>> {
    let arena = kernel.arena();
    let dialog = scrolled.top_layer.iter().rposition(|open| {
        arena
            .resolve(open.key)
            .is_some_and(|slot| modal_dialog(kernel, slot))
    });
    let from = scrolled.modal.into_iter().chain(dialog).max()?;
    Some(
        scrolled.top_layer[from.min(scrolled.top_layer.len())..]
            .iter()
            .filter_map(|open| arena.resolve(open.key))
            .collect(),
    )
}

/// Whether `slot` is behind an open presentation (`modal_live`).
pub(super) fn behind_modal(kernel: &Kernel, scrolled: &Scrolled, slot: u32) -> bool {
    let Some(live) = modal_live(kernel, scrolled) else {
        return false;
    };
    let arena = kernel.arena();
    !std::iter::successors(Some(slot), |&s| arena.parent(s)).any(|s| live.contains(&s))
}

pub(super) fn modal_dialog(kernel: &Kernel, slot: u32) -> bool {
    let props = kernel.arena().props(slot);
    props.str(PropId::SemanticTag) == Some("dialog") && props.str(PropId::Popover).is_none()
}
