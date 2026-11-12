//! A scroller's drag, as the reader that scrolls by itself tells it
//! (`scrollers.rs`; Charlie, 2026-11-10,
//! `issues/20261021-scroll-drag-events-and-hit-slop.md`): `scrolldragstart`
//! when a finger begins dragging a scroller, `scrolldragend` when it lets go,
//! UIKit's `scrollViewWillBeginDragging` and
//! `scrollViewWillEndDragging(_:withVelocity:targetContentOffset:)`. The
//! presenter recognizes no scroll; a wheel, a scrollbar's thumb, a key or the
//! app's own scroll sends neither.
use super::*;
use exact_runner::runner::ScrollDrag;

impl<D: DataSource> Presenter<D> {
    /// Deterministic reader-scroll samples for the Android agent, not native
    /// touch recognition or a fling. The regular pointer path owns neither.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn agent_scroll_drag(&mut self, r: &serde_json::Value) -> Result<String, String> {
        let pair = |name: &str| -> Result<(f32, f32), String> {
            let v = r[name]
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or_else(|| format!("scroll drag: {name} needs two finite numbers"))?;
            let n = |v: &serde_json::Value| v.as_f64().map(|n| n as f32).filter(|n| n.is_finite());
            n(&v[0])
                .zip(n(&v[1]))
                .ok_or_else(|| format!("scroll drag: {name} needs two finite numbers"))
        };
        let id = r["id"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .ok_or("scroll drag: id needs a live view")?;
        let (dx, dy) = pair("scrollDrag")?;
        let ms = r["ms"]
            .as_f64()
            .filter(|ms| ms.is_finite() && (0.0..=2000.0).contains(ms))
            .ok_or("scroll drag: duration must be 0–2000 ms")?;
        if self.contact_position().is_some() {
            return Err("scroll drag: another contact is down".into());
        }
        let (x, y, w, h) = self
            .rect_of(id)
            .ok_or("scroll drag: target has no painted box")?;
        let start = if r.get("from").is_some() {
            pair("from")?
        } else {
            (x + w / 2., y + h / 2.)
        };
        let hit = self
            .hit(start.0, start.1)
            .ok_or("scroll drag: start is not visible")?;
        if !self.is_inside(hit, id) || !self.alert_allows(id) || self.host.route_visibility(id).1 {
            return Err("scroll drag: target is hidden, inert or covered".into());
        }
        let limits = self.collection_scroll_limits();
        let scroller = self
            .scroller(id, &limits)
            .ok_or("scroll drag: target is not a scroller")?;
        let under = self.scrollers_at(start.0, start.1);
        let primary_claimed = if dx.abs() > dy.abs() {
            under.claims.0
        } else {
            under.claims.1
        };
        if primary_claimed && r["probe"] == true {
            return Ok(serde_json::json!({"scrollDrag": "presenter"}).to_string());
        }
        if (dx != 0. && under.claims.0) || (dy != 0. && under.claims.1) {
            return Err("scroll drag: a gesture owns this axis; use native input".into());
        }
        if under.field.is_some() {
            return Err("scroll drag: a native text field owns the start; use native input".into());
        }
        if scroller.axes.0 || scroller.axes.1 {
            let target = under
                .chain
                .iter()
                .find(|s| s.id == id)
                .ok_or("scroll drag: target is outside the scroll chain")?;
            if (dx != 0. && scroller.axes.0 && !target.axes.0)
                || (dy != 0. && scroller.axes.1 && !target.axes.1)
            {
                return Err("scroll drag: a nested scroller contains this axis".into());
            }
        }
        if r["probe"] == true {
            return Ok(serde_json::json!({"scrollDrag": "reader"}).to_string());
        }
        if scroller.axes.0 || scroller.axes.1 {
            self.reader_under(start.0, start.1, 1.);
            if let Some(error) = self.reader_drag_began(id) {
                return Err(error);
            }
            let steps = ((ms / 16.).ceil() as usize).clamp(1, 128);
            for _ in 0..steps {
                self.reader_wheel(id, -dx / steps as f32, -dy / steps as f32)?;
            }
            if let Some(error) = self.reader_drag_ended(id, (0., 0.), None) {
                return Err(error);
            }
        }
        let after = self
            .scroller(id, &self.collection_scroll_limits())
            .ok_or("scroll drag: target was removed while scrolling")?
            .offset;
        Ok(serde_json::json!({
            "tapped": id, "at": [start.0, start.1],
            "scrolledBy": [after.0 - scroller.offset.0, after.1 - scroller.offset.1],
            "delivery": "presenter", "native": "Presenter.reader_*",
            "timing": "deterministic scroll samples; native touch and fling unobserved"
        })
        .to_string())
    }

    /// The reader's finger began dragging scroller `id` (one it was told
    /// of): its `scrolldragstart`. Errors are the action's.
    pub fn reader_drag_began(&mut self, id: ViewId) -> Option<String> {
        self.scroll_drag(id, Event::Scrolldragstart)
    }

    /// The finger let go of scroller `id`. `velocity` is the scroller's, in
    /// points per second, positive as its offset grows: the platform's, which
    /// the reader's fling starts at. `rest` is where that fling will stop,
    /// when the reader knows before it runs (Android's `OverScroller`), clamped
    /// to the scroll range here. The drag's last move already sent its
    /// `scroll` (`reader_scrolled` dispatches it at once), as UIKit's
    /// `scrollViewDidScroll` precedes its `scrollViewWillEndDragging`; the
    /// fling's own `scroll`s and its `scrollend` follow.
    pub fn reader_drag_ended(
        &mut self,
        id: ViewId,
        velocity: (f64, f64),
        rest: Option<(f32, f32)>,
    ) -> Option<String> {
        if !(velocity.0.is_finite() && velocity.1.is_finite())
            || rest.is_some_and(|r| !(r.0.is_finite() && r.1.is_finite()))
        {
            return Some(format!("scroll drag {id}: refused, not finite"));
        }
        let limits = self.collection_scroll_limits();
        let max = self.scroller(id, &limits).map(|s| s.max);
        let target = rest.zip(max).map(|(r, max)| {
            (
                r.0.clamp(0., max.0.max(0.)) as f64,
                r.1.clamp(0., max.1.max(0.)) as f64,
            )
        });
        self.scroll_drag(
            id,
            Event::Scrolldragend(ScrollDrag {
                velocity_x: velocity.0 / 1000.,
                velocity_y: velocity.1 / 1000.,
                target,
            }),
        )
    }

    /// Dispatch a drag's event at `id`, a scroller the reader knows that
    /// hears it and is shown.
    fn scroll_drag(&mut self, id: ViewId, event: Event) -> Option<String> {
        let kind = match event {
            Event::Scrolldragstart => EventKind::Scrolldragstart,
            _ => EventKind::Scrolldragend,
        };
        if id == PAGE
            || !self.reader_knows(id)
            || self.host.route_visibility(id).1
            || !self.host.runner().handlers_of(id).contains(&kind)
        {
            return None;
        }
        let error = self.host.dispatch_at(id, event, self.host.now());
        error.or(self.after_commit())
    }
}

#[cfg(test)]
#[path = "scroll_drag_tests.rs"]
mod tests;
