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
        let target = r["id"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .ok_or("scroll drag: id needs a live view")?;
        let (dx, dy) = pair("scrollDrag")?;
        let ms = r["ms"]
            .as_f64()
            .filter(|ms| ms.is_finite() && (0.0..=2000.0).contains(ms))
            .ok_or("scroll drag: duration must be 0–2000 ms")?;
        let press = r
            .get("press")
            .map_or(Some(0.), |v| v.as_f64())
            .filter(|v| v.is_finite() && *v >= 0.)
            .ok_or("invalid press duration")?;
        let hold = r
            .get("hold")
            .map_or(Some(0.), |v| v.as_f64())
            .filter(|hold| hold.is_finite() && *hold >= 0. && press + ms + hold <= 10000.)
            .ok_or("scroll drag: move and hold must fit in 10000 ms")?;
        if self.contact_position().is_some() {
            return Err("scroll drag: another contact is down".into());
        }
        let (x, y, w, h) = self
            .rect_of(target)
            .ok_or("scroll drag: target has no painted box")?;
        let start = if r.get("from").is_some() {
            pair("from")?
        } else {
            (x + w / 2., y + h / 2.)
        };
        let hit = self
            .hit(start.0, start.1)
            .ok_or("scroll drag: start is not visible")?;
        if !self.alert_allows(target) || self.host.route_visibility(target).1 {
            return Err("scroll drag: target is hidden or inert".into());
        }
        let limits = self.collection_scroll_limits();
        let under = self.scrollers_at(start.0, start.1);
        let named = self.scroller(target, &limits);
        // An explicit scroller receives its own samples. Content in a sheet
        // follows the sheet first when this direction can move its detent.
        let sheet = named.is_none()
            && if dy < 0. {
                under.sheet_directions.0
            } else {
                under.sheet_directions.1
            };
        let claims = (under.claims.0, (under.claims.1 && !under.sheet) || sheet);
        let across = dx.abs() > dy.abs();
        let primary_claimed = if across { claims.0 } else { claims.1 };
        // Scrolling.decide gives the other axis a turn if the primary one has
        // no scroller and the movement on the other has crossed the touch slop.
        let available = |under: &Under, horizontal| {
            under
                .chain
                .iter()
                .copied()
                .find(|s| if horizontal { s.axes.0 } else { s.axes.1 })
        };
        let slop = self.slop as f32;
        let other_allowed = if across {
            dy.abs() > slop && !claims.1
        } else {
            dx.abs() > slop && !claims.0
        };
        let horizontal = across ^ (available(&under, across).is_none() && other_allowed);
        let owner = named
            .filter(|s| if horizontal { s.axes.0 } else { s.axes.1 })
            .or_else(|| available(&under, horizontal))
            .or(named);
        if hit != target && !self.is_inside(hit, target) {
            // A target-relative offset can start beside a text line. Both points must
            // reach the same scroll owner, and the named content must still be exposed.
            let outside = start.0 < x || start.1 < y || start.0 >= x + w || start.1 >= y + h;
            let center = (x + w / 2., y + h / 2.);
            if named.is_some()
                || !outside
                || owner.is_none()
                || !self
                    .hit(center.0, center.1)
                    .is_some_and(|h| h == target || self.is_inside(h, target))
                || available(&self.scrollers_at(center.0, center.1), horizontal).map(|s| s.id)
                    != owner.map(|s| s.id)
            {
                return Err("scroll drag: target is hidden, inert or covered".into());
            }
        }
        let distance = dx.abs().max(dy.abs()) as f64;
        let (dx, dy) = match (named, horizontal) {
            (None, true) => (dx, 0.),
            (None, false) => (0., dy),
            _ => (dx, dy),
        };
        let reorder_grip = (r["probe"] == true || press > 0.)
            && under.field.is_none()
            && self.arrange_hold_at(start.0, start.1).is_some();
        if reorder_grip && r["probe"] == true {
            if let Some(deadline) = r["longPressMs"].as_f64() {
                let travel = (deadline - press).max(0.);
                if deadline <= press
                    || (ms > 0. && travel <= ms && distance * travel / ms <= self.slop)
                {
                    return Ok(
                        serde_json::json!({"scrollDrag": "presenter", "liftAt": deadline})
                            .to_string(),
                    );
                }
            }
        }
        if r["probe"] == true
            && (primary_claimed
                || owner.is_none()
                || named.is_none()
                    && (dx.abs().max(dy.abs()) <= slop
                        || under.field.is_some()
                        || self.canvas_contact_target(hit).is_some()))
        {
            return Ok(serde_json::json!({"scrollDrag": "presenter"}).to_string());
        }
        if (dx != 0. && claims.0) || (dy != 0. && claims.1) {
            return Err("scroll drag: a gesture owns this axis; use native input".into());
        }
        if under.field.is_some() {
            return Err("scroll drag: a native text field owns the start; use native input".into());
        }
        let scroller = owner.ok_or("scroll drag: no scroller owns this axis")?;
        let id = scroller.id;
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
        let seek = self.live_time().is_none();
        if !seek && (hold > 0. || press > 0.) {
            return Err("scroll drag: a timed hold needs the agent clock or native input".into());
        }
        if r["probe"] == true {
            return Ok(
                serde_json::json!({"scrollDrag": "reader", "reorderGrip": reorder_grip})
                    .to_string(),
            );
        }
        if press > 0. {
            if !reorder_grip {
                return Err("scroll drag: the press no longer starts on a reorder grip".into());
            }
            if let Some(error) = crate::agent::hold_framed(self, self.host.now() + press).1 {
                return Err(error);
            }
            // Recheck the hit and the scroll chain after timers have run.
            let mut move_only = r.clone();
            move_only.as_object_mut().unwrap().remove("press");
            return self.agent_scroll_drag(&move_only);
        }
        let travels = scroller.axes.0 || scroller.axes.1;
        let began = self.host.now();
        if travels {
            self.reader_under(start.0, start.1, 1.);
            if let Some(error) = self.reader_drag_began(id) {
                self.scroll_end_release(id);
                return Err(error);
            }
            let advance = |p: &mut Self, elapsed| {
                if seek {
                    if let Some(error) = crate::agent::hold_framed(p, began + elapsed).1 {
                        return Err(error);
                    }
                }
                Ok(())
            };
            let sample = |p: &mut Self| -> Result<(), String> {
                let steps = ((ms / 16.).ceil() as usize).clamp(1, 128);
                for step in 1..=steps {
                    p.reader_wheel(id, -dx / steps as f32, -dy / steps as f32)?;
                    advance(p, ms * step as f64 / steps as f64)?;
                }
                if hold > 0. {
                    advance(p, ms + hold)?;
                }
                Ok(())
            };
            if let Err(error) = sample(self) {
                self.scroll_end_release(id);
                return Err(error);
            }
            if let Some(error) = self.reader_drag_ended(id, (0., 0.), None, false) {
                return Err(error);
            }
        } else if seek {
            if let Some(error) = crate::agent::hold_framed(self, began + ms + hold).1 {
                return Err(error);
            }
        }
        let after = self
            .scroller(id, &self.collection_scroll_limits())
            .ok_or("scroll drag: target was removed while scrolling")?
            .offset;
        Ok(serde_json::json!({
            "tapped": target, "at": [start.0, start.1],
            "scrolledBy": [after.0 - scroller.offset.0, after.1 - scroller.offset.1],
            "delivery": "presenter", "native": "Presenter.reader_*",
            "timing": "deterministic scroll samples; native touch and fling unobserved"
        })
        .to_string())
    }

    /// The reader's finger began dragging scroller `id` (one it was told
    /// of): its `scrolldragstart`. From now until the drag, or the fling it
    /// throws, is over, the scroller's moves owe no `scrollend`
    /// (`scroll_end.rs`). Errors are the action's.
    pub fn reader_drag_began(&mut self, id: ViewId) -> Option<String> {
        if self.reader_knows(id) {
            self.scroll_end_hold(id);
        }
        self.scroll_drag(id, Event::Scrolldragstart)
    }

    /// The finger let go of scroller `id`. `velocity` is the scroller's, in
    /// points per second, positive as its offset grows: the platform's, which
    /// the reader's fling starts at. `rest` is where that fling will stop,
    /// when the reader knows before it runs (Android's `OverScroller`), clamped
    /// to the scroll range here. The drag's last move already sent its
    /// `scroll` (`reader_scrolled` dispatches it at once), as UIKit's
    /// `scrollViewDidScroll` precedes its `scrollViewWillEndDragging`; the
    /// fling's own `scroll`s and its `scrollend` follow. With nothing thrown
    /// (`flings` false) the scroll is over and its `scrollend` is due now;
    /// a fling's comes when the reader says it stopped
    /// ([`Presenter::reader_fling_ended`]).
    pub fn reader_drag_ended(
        &mut self,
        id: ViewId,
        velocity: (f64, f64),
        rest: Option<(f32, f32)>,
        flings: bool,
    ) -> Option<String> {
        if !(velocity.0.is_finite() && velocity.1.is_finite())
            || rest.is_some_and(|r| !(r.0.is_finite() && r.1.is_finite()))
        {
            return Some(format!("scroll drag {id}: refused, not finite"));
        }
        if !flings {
            self.scroll_end_release(id);
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

    /// The fling the reader threw at the end of its drag of scroller `id`
    /// stopped: it came to rest, a finger caught it, or the app scrolled
    /// the scroller. Its `scrollend` is due now, as UIKit's deceleration ends.
    pub fn reader_fling_ended(&mut self, id: ViewId) {
        self.scroll_end_release(id);
    }

    /// The reader's pull to refresh of scroller `id` let go past its
    /// threshold: its `refresh`, as UIKit's refresh control sends it. The app's
    /// `refreshing` keeps the indicator turning (`reader_scrollers`' bit 5).
    /// Errors are the action's.
    pub fn reader_refresh(&mut self, id: ViewId) -> Option<String> {
        if id == PAGE
            || !self.reader_knows(id)
            || self.host.route_visibility(id).1
            || !self
                .host
                .runner()
                .handlers_of(id)
                .contains(&EventKind::Refresh)
        {
            return None;
        }
        let error = self.host.dispatch_at(id, Event::Refresh, self.host.now());
        error.or(self.after_commit())
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
