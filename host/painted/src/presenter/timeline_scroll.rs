//! Scroll and view timelines (LLP 1057.003 D5): before each paint and in
//! each scroll the engine is given the offset of every scroller a placed
//! timeline reads, so a scroll moves the consumers in its own frame, with
//! no action in the path. Placement itself follows each layout
//! (`Host::layout`).
//!
//! Credit: Janic Duplessis's fork (appandflow/exact 85d2bb0) gave the engine
//! view timelines' progress from this presenter's scroll path.

use super::*;

impl<D: DataSource> Presenter<D> {
    /// Give the engine each timeline scroller's offset as this presenter
    /// shows it; whether a consumer's presentation changed.
    pub(super) fn scroll_timelines(&mut self) -> bool {
        let offsets: Vec<(u64, f64, f64)> = (self.host.kernel().timeline_scrollers().iter())
            .map(|s| {
                let (x, y) = if s.page {
                    self.page
                } else {
                    self.scroll_of(s.view)
                };
                (s.node, f64::from(x), f64::from(y))
            })
            .collect();
        !offsets.is_empty() && self.host.scroll_timelines(&offsets)
    }
}
