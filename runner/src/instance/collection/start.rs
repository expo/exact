//! `scroll-start: end` (LLP 1010 §6.5): a virtualized list that opens at its
//! end — a transcript — builds its last rows first, tells the host to move
//! there before it paints, and anchors the end through measurement until the
//! window there is measured or the reader moves it.
use super::*;

/// What a list that opens at its end holds before an update
/// ([`Collection::bottom_anchor`]).
pub(super) struct BottomAnchor {
    anchor: index::Anchor,
    /// The first row the port showed, and where it was.
    first: Option<(usize, Rc<str>)>,
    /// Where the last row it showed was.
    last: usize,
}

impl Collection {
    /// Whether the window anchors the end: the list follows it, or it is
    /// still opening there. Not while a `scrollIntoView` is under way: the
    /// request overrides the list's kept position (LLP 1070.000 §2.5).
    pub(super) fn follows(&self) -> bool {
        (self.follow_end || self.at_end) && self.preview.is_none() && self.target.is_none()
    }
    /// Before any host report: the window is the last rows, and the host is
    /// told to start at the end (it clamps the offset to its extent).
    pub(super) fn start_at_end(&mut self) {
        if !self.at_end
            || self.geometry.is_some()
            || (self.index.len() == 0 && self.fixed.is_none())
        {
            return;
        }
        let offset = self.index.scroll_extent();
        self.start_offset = offset;
        self.correction = Some(AnchorCorrection {
            scroll_sequence: 0,
            offset,
            from: None,
            smooth: false,
        });
    }
    /// The first row of the window before any host report.
    pub(super) fn bootstrap_first(&self) -> Result<usize, InstanceError> {
        if self.at_end {
            return Ok(self.index.len().saturating_sub(self.bootstrap_rows));
        }
        if let Some(nest::ReadingPoint::Row(key)) = self.restored_at.as_ref().map(|p| &p.point) {
            if let Some(row) = self.index.position(key) {
                return Ok(row);
            }
        }
        Ok(self
            .index
            .row_at(self.start_offset)
            .map_err(index_error)?
            .unwrap_or(0))
    }
    /// The reader's own motion ends the opening: travel a host samples from
    /// a drag, a fling or a wheel, in two reports running. Not a report
    /// short of the end: a host clamps its port to rows it laid out at their
    /// real size before the runner measured them, and one may report before
    /// it applies the correction.
    pub(super) fn leave_end_if_moved(&mut self, velocity: f64) {
        if !self.at_end {
            return;
        }
        self.end_travel = if velocity != 0.0 {
            self.end_travel + 1
        } else {
            0
        };
        if self.end_travel >= 2 {
            self.at_end = false;
        }
    }
    /// @ref LLP 1168.000 D1.2 — before an update to a list that opens at its
    /// end (`scroll-start: end`, `opens_at_end`), the anchor at its last
    /// visible row ([`SizeIndex::capture_bottom_anchor`]); none while the
    /// list follows its end, which wins.
    pub(super) fn bottom_anchor(&self) -> Result<Option<BottomAnchor>, InstanceError> {
        let Some(g) = self.geometry.as_ref().filter(|_| self.opens_at_end) else {
            return Ok(None);
        };
        let offset = self.anchor_offset(g.offset, g.port_main);
        let anchor = self
            .index
            .capture_bottom_anchor(offset, g.port_main, self.follows())
            .map_err(index_error)?;
        if SizeIndex::follows_end(&anchor) {
            return Ok(None);
        }
        let first = self
            .index
            .row_at(offset.clamp(0.0, self.index.max_offset(g.port_main).max(0.0)))
            .map_err(index_error)?
            .and_then(|p| Some((p, self.index.shared_key(p)?.clone())));
        let last = SizeIndex::anchor_row(&anchor).unwrap_or(0);
        Ok(Some(BottomAnchor {
            anchor,
            first,
            last,
        }))
    }
    /// The anchor an update to a list that opens at its end restores: the
    /// last visible row's (`bottom`) when the update put rows before the
    /// window, else `ordinary`, as in every other list. Rows landed before it
    /// when the first row the port showed is kept further down (with `old`
    /// the keys before the update), or, at the list's start, when a page
    /// landed there ([`Collection::page_before_start`]: after a leading row
    /// that stays first too), or, while a `reachstart` is unanswered, rows
    /// landed after a leading run there (a "Loading" row, then a day's
    /// separator that stays). An edit inside the window moves none of that
    /// unless it is taken for that page.
    pub(super) fn end_list_anchor(
        &self,
        old: &[Rc<str>],
        ordinary: Option<index::Anchor>,
        bottom: Option<BottomAnchor>,
    ) -> Option<index::Anchor> {
        let Some(BottomAnchor {
            anchor,
            first: Some((was, key)),
            last,
        }) = bottom
        else {
            return ordinary;
        };
        let before = self.index.position(&key).is_some_and(|now| now > was)
            || (was == 0 && self.page_before_start(old))
            || (was == 0
                && self.start_asked
                && reach::page_after_leading_run(
                    old,
                    last,
                    |k| self.index.position(k),
                    |i| self.index.key(i),
                ));
        if before {
            Some(anchor)
        } else {
            ordinary
        }
    }
    /// The offset an opening list's anchor is taken at: its end, in a port of
    /// `port`, and no overshoot past it.
    pub(super) fn anchor_offset(&self, offset: f64, port: f64) -> f64 {
        if self.at_end {
            self.index.max_offset(port)
        } else {
            offset
        }
    }
    /// The list opened: a report at the end that changed nothing — every
    /// mounted row measured, and its measurements left the extent as it was
    /// (`extent`, before them; a first report's are discarded), nothing owed
    /// or corrected. Until then the window there is still
    /// filling, and a host may clamp its port to rows it has laid out before
    /// the runner measured them. From then on `scrollFollowEnd` decides.
    pub(super) fn settle_start(&mut self, extent: f64) {
        let Some(g) = &self.geometry else {
            return;
        };
        let total = self.index.total_height();
        // An empty list has not opened: its rows are yet to come.
        if self.at_end
            && (self.index.len() > 0 || self.fixed.is_some())
            && !self.pending
            && self.correction.is_none()
            && (total - extent).abs() < 0.01
            && self.mounted.iter().all(|m| {
                self.index
                    .key(m.position)
                    .is_some_and(|k| self.index.is_measured(k))
            })
            && self.index.max_offset(g.port_main) - g.offset < super::index::END_SLACK
        {
            self.at_end = false;
        }
    }
}
/// Whether a port at `offset` is where the anchor puts it (`corrected`).
/// Hosts round offsets to device pixels (4405.1667 shows as 4405.333 at
/// 3x; WebKit floors a fractional scroll range): a followed end already sent
/// (`sent`) is reached within `END_SLACK`,
/// else the same unreachable end went out with every commit and an opening
/// never settled (LLP 1010 §6.8). An end that moved is sent once; a row
/// anchor keeps 0.01, since its moves add up report on report. A port in
/// the padding before the first row is at the rows' start, unless the
/// target is in that padding too (a short list's end, LLP 1010 §6.9) or the
/// anchor is held by the port's bottom, which the padding moves (LLP
/// 1168.000 D1.2).
pub(super) fn at_target(
    anchor: &super::index::Anchor,
    corrected: f64,
    offset: f64,
    sent: f64,
) -> bool {
    let offset = if corrected < 0.0 || anchor.bottom {
        offset
    } else {
        offset.max(0.0)
    };
    let gap = (corrected - offset).abs();
    gap <= 0.01
        || (super::index::SizeIndex::follows_end(anchor)
            && gap < super::index::END_SLACK
            && (corrected - sent).abs() <= 0.01)
}
