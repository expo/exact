//! The root's viewport: the page's `<meta name="viewport">`, and the root's
//! fit to the visual viewport.

use super::Em;
use exact_web::host::template::Parts;
use std::fmt::Write as _;

/// Fits the root to the visual viewport from before the first layout under
/// `resizes-content`, which Safari ignores (`fitViewport`, document.js).
pub(super) fn fit(em: &mut Em<'_>, root: usize, body: &mut String) {
    let widget = em.parts[root]
        .as_ref()
        .and_then(|p| p.props.get("interactiveWidget"));
    if widget.is_some_and(|w| w == "resizes-content") {
        let _ = write!(body, "{}();", em.uses.rt("fitViewport"));
    }
}

/// The `<meta name="viewport">` content, where the root says any of it.
pub(super) fn meta(root: &Parts) -> Option<String> {
    let fit = root.props.get("viewportFit");
    let widget = root.props.get("interactiveWidget");
    (fit.is_some() || widget.is_some()).then(|| {
        let mut m = String::from("width=device-width, initial-scale=1");
        if let Some(f) = fit {
            let _ = write!(m, ", viewport-fit={f}");
        }
        if let Some(w) = widget {
            let _ = write!(m, ", interactive-widget={w}");
        }
        m
    })
}
