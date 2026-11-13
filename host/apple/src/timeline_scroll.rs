//! Scroll and view timelines on Apple (LLP 1057.003 D5). After each layout
//! the host places them (`Kernel::place_timelines`), marks each scroller
//! they read with the `scrollTimeline` prop (`page` on a root the page
//! scrolls) and gives the engine each scroller's last offset. The
//! presenter's scroll callback then calls [`Host::timeline_scrolled`] for a
//! marked scroller, in UIKit's or AppKit's scroll, on the main thread: the
//! consumers are presented in the frame the scroller moves.
//!
//! Credit: Janic Duplessis's fork (appandflow/exact a98b117) fed the
//! scroller's offset to the engine from this callback for view timelines.

use super::*;
use exact_kernel::timeline::TimelineScroller;

/// The scrollers the last placement reads, each with the offset its
/// presenter last gave.
#[derive(Default)]
pub(super) struct TimelineScrolls {
    scrollers: Vec<(TimelineScroller, [f64; 2])>,
}

impl<D: DataSource> Host<D> {
    /// Place the scroll and view timelines from the layout just run, mark
    /// the scrollers they read, and hold each consumer at its scroller's
    /// offset.
    pub(super) fn place_timelines(&mut self, batch: &mut Batch) {
        for (node, binding) in self.runner.kernel_mut().place_timelines() {
            self.engine.set_animation_timeline(node, binding);
        }
        let kernel = self.runner.kernel();
        let now = kernel.timeline_scrollers();
        let was = &self.timeline_scrolls.scrollers;
        if now.is_empty() && was.is_empty() {
            return;
        }
        let mut scrollers = Vec::with_capacity(now.len());
        for s in now {
            // A view this batch creates is a new scroller, at its start.
            let kept = (!batch.creates(s.view))
                .then(|| was.iter().find(|(w, _)| w == s))
                .flatten();
            if kept.is_none() {
                let mark = if s.page { "page" } else { "1" };
                batch.props(s.view, &[("scrollTimeline", mark.into())], &[]);
            }
            // A scroller marked anew is where the presenter last showed it,
            // which the runner notes for every scroll, marked or not; a
            // node created since has none noted, and is at its start.
            let offset = kept.map(|k| k.1).unwrap_or_else(|| {
                let (x, y) = self.runner.scroll_offset((!s.page).then_some(s.view));
                [x, y]
            });
            scrollers.push((*s, offset));
        }
        for (s, _) in was {
            if !now.iter().any(|n| n.view == s.view) && kernel.node(s.view).is_some() {
                batch.props(s.view, &[], &["scrollTimeline"]);
            }
        }
        for (s, [x, y]) in &scrollers {
            self.engine.set_scroll_offset(s.node, *x, *y);
        }
        self.timeline_scrolls.scrollers = scrollers;
    }

    /// The presenter's scroller `view` (the page for `None`) now stands at
    /// `(left, top)` CSS px: the timelines it drives, presented.
    pub fn timeline_scrolled(&mut self, view: Option<ViewId>, left: f64, top: f64) -> String {
        let mut batch = Batch::new();
        for (s, offset) in self.timeline_scrolls.scrollers.iter_mut() {
            let moved = match view {
                Some(v) => s.view == v,
                None => s.page,
            };
            if moved {
                *offset = [left, top];
                self.engine.set_scroll_offset(s.node, left, top);
            }
        }
        self.present(&mut batch, false);
        self.finish(batch, None)
    }
}
