//! A list's snapshot for its host ([`CollectionSnapshot`]).
use super::*;

impl Collection {
    pub(super) fn snapshot(&self) -> CollectionSnapshot {
        self.snapshot_rows(usize::MAX)
    }
    /// [`Collection::snapshot`] with at most `rows` mounted rows (the first).
    pub(super) fn snapshot_rows(&self, rows: usize) -> CollectionSnapshot {
        CollectionSnapshot {
            view: self.view,
            axis: self.axis,
            parent: self.parent,
            restored: self.restored,
            seeking: self.target.is_some() && !self.request_waits(),
            waiting: self.request_waits(),
            request: self.request,
            revision: self.revision,
            scroll_sequence: self.geometry.as_ref().map_or(0, |g| g.scroll_sequence),
            count: self.index.len(),
            fixed: self.fixed_snapshot(),
            total_extent: self.index.total_height(),
            rows: self
                .mounted
                .iter()
                .take(rows)
                .map(|row| CollectionRow {
                    view: row.wrapper,
                    root: super::super::first_root(&row.row.roots).expect("a row has a root"),
                    index: row.position,
                    start: self.index.prefix(row.position).unwrap(),
                    size: (self.index.height(row.position).unwrap() - self.index.item_gap())
                        .max(0.0),
                    epoch: row.epoch,
                    measured: self.index.is_measured_at(row.position),
                })
                .collect(),
            correction: self.correction,
            pending: self.asks_report(),
            window: self.built_window(),
        }
    }
}
