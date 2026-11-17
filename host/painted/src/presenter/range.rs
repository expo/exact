//! A range under the pointer and the keys (LLP 1069.001 D7): its thumb
//! follows a drag, with HTML's `input` as the value changes and `change`
//! when the contact lifts, and a focused range steps on the arrows, Home,
//! End, PageUp and PageDown, as Chromium's `RangeInputType` steps one and
//! the WAI-ARIA Authoring Practices' slider pattern gives the keys.
use super::*;
use exact_kernel::NodeKey;

/// A contact that went down on a range. A mouse's moves the thumb from the
/// down; a finger's once it has moved past the slop along the track, as
/// Android's `AbsSeekBar` waits in a scrolling container, so a finger that
/// scrolls the page sets nothing, and a tap sets the value where it lifts.
pub(super) struct Drag {
    key: NodeKey,
    /// The value at the down: `change` fires on the lift if it moved.
    from: f64,
    origin: (f32, f32),
    following: bool,
}

impl<D: DataSource> Presenter<D> {
    /// The value the range `id` shows: its choice since its bound value
    /// changed, else its `value`, sanitized.
    pub(super) fn range_value(&self, id: ViewId) -> Option<f64> {
        let node = self.host.kernel().node(id)?;
        let range = exact_kernel::Range::of(node.props);
        let chosen = crate::paint::control::choice(&node, self.chosen.get(&id));
        Some(
            chosen
                .and_then(|c| c.parse::<f64>().ok())
                .map_or_else(|| range.shown(node.props), |v| range.sanitize(v)),
        )
    }

    /// The value of the range `id` at viewport `x`: the point under the
    /// thumb's centre, as the painter places it, stepped as HTML sanitizes a
    /// range's value. `None` when `id` is no range or has no box.
    pub(super) fn range_at(&self, id: ViewId, x: f32) -> Option<f64> {
        let node = self.host.kernel().node(id)?;
        if exact_kernel::ControlKind::of(node.node_type, node.props)
            != Some(exact_kernel::ControlKind::Range)
        {
            return None;
        }
        let range = exact_kernel::Range::of(node.props);
        let (bx, _, bw, bh) = self.boxes.iter().find(|b| b.id == id)?.rect;
        let d = crate::paint::control::range_thumb(&node, bw, bh);
        let t = if bw > d {
            ((x - bx - d / 2.0) / (bw - d)).clamp(0.0, 1.0) as f64
        } else {
            0.5
        };
        let t = match crate::paint::control::range_mirrored(&node) {
            true => 1.0 - t,
            false => t,
        };
        Some(range.sanitize(range.min + t * (range.max - range.min)))
    }

    /// A press on a range moves its thumb to the pointer (D7): the value
    /// there, `input` then `change`; a drag's lift, nothing more, since the
    /// drag set it. False when `id` is not a range.
    pub(crate) fn press_range(&mut self, id: ViewId, x: f32) -> bool {
        let Some(node) = self.host.kernel().node(id) else {
            return false;
        };
        if exact_kernel::ControlKind::of(node.node_type, node.props)
            != Some(exact_kernel::ControlKind::Range)
        {
            return false;
        }
        if self.range_dragged(id) {
            return true;
        }
        if let Some(value) = self.range_at(id, x) {
            if let Err(e) = self.set_control_value(id, &value.to_string()) {
                self.host.log(format!("range: {e}"));
            }
        }
        true
    }

    /// A contact went down on `hit`: if it is an enabled range, it holds
    /// the contact until the lift, and a mouse's sets the value under it.
    pub(super) fn begin_range_drag(&mut self, hit: NodeKey, (x, y): (f32, f32)) {
        self.range_drag = None;
        let Some(id) = self.host.kernel().node_by_key(hit).map(|n| n.id) else {
            return;
        };
        if self.menu.is_some() || !self.input_live(hit) {
            return;
        }
        let Some(from) = self
            .range_value(id)
            .filter(|_| self.range_at(id, x).is_some())
        else {
            return;
        };
        self.range_drag = Some(Drag {
            key: hit,
            from,
            origin: (x, y),
            following: false,
        });
        if !self.contact_is_direct() {
            self.follow_range(x);
        }
    }

    /// A move while a range holds the contact: true when it is the range's.
    pub(super) fn drag_range(&mut self, (x, y): (f32, f32)) -> bool {
        let Some(drag) = &self.range_drag else {
            return false;
        };
        if !drag.following {
            let (dx, dy) = (x - drag.origin.0, y - drag.origin.1);
            let slop = self.slop as f32;
            if dx.abs() <= slop && dy.abs() <= slop {
                return true;
            }
            // A finger that leaves along the other axis is the page's.
            if dx.abs() < dy.abs() {
                self.range_drag = None;
                return false;
            }
        }
        self.follow_range(x);
        true
    }

    /// The thumb to the pointer: an `input` where the value changes.
    fn follow_range(&mut self, x: f32) {
        let Some(drag) = self.range_drag.as_mut() else {
            return;
        };
        drag.following = true;
        let key = drag.key;
        let Some(id) = self
            .host
            .kernel()
            .node_by_key(key)
            .map(|n| n.id)
            .filter(|_| self.input_live(key))
        else {
            self.range_drag = None;
            return;
        };
        let (Some(to), Some(now)) = (self.range_at(id, x), self.range_value(id)) else {
            return;
        };
        if to != now {
            if let Err(e) = self.commit_control_value(id, &to.to_string(), &[EventKind::Input]) {
                self.host.log(format!("range: {e}"));
            }
        }
    }

    /// The contact lifted or was cancelled: `change` if the value moved
    /// since the down, as HTML commits a range when the person lets go.
    /// The drag stays until [`Self::forget_range_drag`], so that the lift's
    /// press does not set the value again.
    pub(super) fn end_range_drag(&mut self) {
        let Some(drag) = self.range_drag.as_ref().filter(|d| d.following) else {
            return;
        };
        let (key, from) = (drag.key, drag.from);
        let Some(id) = self.host.kernel().node_by_key(key).map(|n| n.id) else {
            return;
        };
        if let Some(now) = self.range_value(id).filter(|now| *now != from) {
            if let Err(e) = self.commit_control_value(id, &now.to_string(), &[EventKind::Change]) {
                self.host.log(format!("range: {e}"));
            }
        }
    }

    pub(super) fn forget_range_drag(&mut self) {
        self.range_drag = None;
    }

    /// Whether the press on `id` is a drag's, which set its value already.
    pub(super) fn range_dragged(&self, id: ViewId) -> bool {
        self.range_drag.as_ref().is_some_and(|d| {
            d.following && self.host.kernel().node(id).is_some_and(|n| n.key == d.key)
        })
    }

    /// A key at a focused range: Chromium's `RangeInputType` steps by
    /// `step` (by a hundredth of the span under `step="any"`), PageUp and
    /// PageDown by the larger of that and a tenth of the span, Home and End
    /// to the ends; the horizontal arrows follow the inline direction.
    /// HTML fires `input` then `change`. False when `id` is no range or the
    /// key is none of these.
    pub(super) fn range_key(&mut self, id: ViewId, name: &str) -> bool {
        let Some(node) = self.host.kernel().node(id) else {
            return false;
        };
        if exact_kernel::ControlKind::of(node.node_type, node.props)
            != Some(exact_kernel::ControlKind::Range)
        {
            return false;
        }
        let range = exact_kernel::Range::of(node.props);
        let rtl = crate::paint::control::range_mirrored(&node);
        let span = range.max - range.min;
        let step = range.step.unwrap_or(span / 100.0);
        let big = step.max(span / 10.0);
        let Some(now) = self.range_value(id) else {
            return false;
        };
        let to = match name {
            "ArrowUp" => now + step,
            "ArrowDown" => now - step,
            "ArrowRight" if rtl => now - step,
            "ArrowRight" => now + step,
            "ArrowLeft" if rtl => now + step,
            "ArrowLeft" => now - step,
            "PageUp" => now + big,
            "PageDown" => now - big,
            "Home" => range.min,
            "End" => range.max,
            _ => return false,
        };
        let to = range.sanitize(to);
        if to != now {
            if let Err(e) = self.set_control_value(id, &to.to_string()) {
                self.host.log(format!("range: {e}"));
            }
        }
        true
    }
}
