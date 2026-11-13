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
