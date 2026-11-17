//! A route's own entrance and exit for UIKit and AppKit (LLP 1013.001): the
//! part it declares, sampled into the poses a keyframe animation plays.
use super::*;
use exact_kernel::StyleId;

/// Poses a second, at least. Core Animation interpolates linearly between them.
const RATE: f64 = 60.0;

impl<D: DataSource> Host<D> {
    /// The `::view-transition-new` (`exit` false) or `::view-transition-old`
    /// part route `view` declares, posed for a box `size` points large: JSON
    /// `{"seconds": s, "poses": [[opacity, x, y, scale, degrees], …]}` at
    /// even steps from its start to its end, the translation in points and
    /// each pose about the box's centre, with no pose for `none`. `null` where the route declares no part or may play
    /// none (D2: a sheet, or a route that zooms from its `navigationSource`):
    /// the platform's transition plays.
    pub fn route_part(&self, view: ViewId, exit: bool, size: (f64, f64)) -> String {
        let Some(node) = self.runner.kernel().node(view) else {
            return "null".into();
        };
        let (row, animations) = match exit {
            false => (
                StyleId::ViewTransitionNew,
                &node.style.rare.view_transition_new,
            ),
            true => (
                StyleId::ViewTransitionOld,
                &node.style.rare.view_transition_old,
            ),
        };
        let sheet = node.props.str(PropId::NavigationPresentation) == Some("modal");
        let zoom = (node.props.str(PropId::NavigationSource)).is_some_and(|s| !s.is_empty());
        if !node.style.mask.has(row) || sheet || zoom {
            return "null".into();
        }
        let seconds = animations.end_time();
        let mut out = format!("{{\"seconds\":{seconds},\"poses\":[");
        if seconds > 0.0 && seconds.is_finite() {
            let steps = (seconds * RATE).ceil() as usize;
            for i in 0..=steps {
                let p = animations.picture(seconds * i as f64 / steps as f64, size);
                let comma = if i == 0 { "" } else { "," };
                let (x, y) = p.translate;
                let pose = format!("[{},{x},{y},{},{}]", p.opacity, p.scale, p.rotate);
                out.push_str(comma);
                out.push_str(&pose);
            }
        }
        out.push_str("]}");
        out
    }
}
