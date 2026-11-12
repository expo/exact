//! Host measurements and collection geometry, validated before mutation.
use super::*;

impl Collection {
    pub(super) fn measured_size(&self, row: &Mounted, raw: f64) -> f64 {
        let position = row.position;
        let size = if row.display_none {
            0.0
        } else {
            raw + self.gap
        };
        let zero = self
            .zero_heights
            .contains(self.index.key(position).unwrap());
        if (raw == 0.0) == zero {
            self.index.denoised(position, size)
        } else {
            size
        }
    }

    /// Preflight the entire report before releasing any other collection's pins.
    pub(super) fn prepare_feedback(
        &self,
        feedback: &CollectionFeedback,
        fill: CollectionFill,
    ) -> Result<Option<BTreeMap<ViewId, usize>>, InstanceError> {
        feedback
            .validate()
            .map_err(|_| invalid("invalid collection feedback"))?;
        if feedback.view != self.view
            || feedback.revision != self.revision
            || self
                .geometry
                .as_ref()
                .is_some_and(|g| feedback.scroll_sequence < g.scroll_sequence)
        {
            return Ok(None);
        }
        if [feedback.focus_view, feedback.interaction_view]
            .into_iter()
            .flatten()
            .any(|view| self.pin(Some(view)).is_none())
        {
            return Ok(None);
        }
        // @ref LLP 1072 §5 — a build-only report keeps the port, width and
        // pins: a report that changes them may retire, so it is immediate.
        if fill.create_only
            && self.geometry.as_ref().is_none_or(|g| {
                (
                    g.port_cross,
                    g.port_main,
                    g.cross,
                    g.focus_view,
                    g.interaction_view,
                ) != (
                    feedback.port_cross,
                    feedback.port_main,
                    feedback.cross,
                    feedback.focus_view,
                    feedback.interaction_view,
                )
            })
        {
            return Err(InstanceError::InvalidCollectionFeedback);
        }
        // Reject batches mentioning retired/unpublished wrappers or epochs atomically.
        let by_view: BTreeMap<_, _> = self
            .mounted
            .iter()
            .enumerate()
            .map(|(i, r)| (r.wrapper, i))
            .collect();
        let mut max_value = self
            .index
            .total_height()
            .max(feedback.padding[0])
            .max(feedback.padding[1]);
        for m in &feedback.measurements {
            if by_view
                .get(&m.view)
                .is_none_or(|i| self.mounted[*i].epoch != m.epoch)
            {
                return Ok(None);
            }
            max_value = max_value.max(m.size + self.gap);
        }
        let changed_width = self
            .geometry
            .as_ref()
            .is_none_or(|g| g.cross != feedback.cross);
        if changed_width && !self.zero_heights.is_empty() {
            max_value = max_value.max(self.estimated_height);
        }
        if self.index.padded_extent_definitely_fits(max_value) {
            return Ok(Some(by_view));
        }
        let mut preview = self.index.preview_heights();
        if changed_width {
            // Width invalidation revives confirmed zero rows at the estimate;
            // measurements from the old width will be discarded.
            for key in &self.zero_heights {
                if let Some(position) = self.index.position(key) {
                    preview.set_height(position, self.estimated_height);
                }
            }
        } else {
            // Linear subtraction/addition can round below the committed binary
            // sum. Preview only changed leaves and their ancestors, without
            // cloning/scanning the N-row index or allocating for equal samples.
            for measurement in &feedback.measurements {
                let row = &self.mounted[by_view[&measurement.view]];
                preview.set_height(row.position, self.measured_size(row, measurement.size));
            }
        }
        let extent = preview.total_height();
        // Physical content size and relative end correction add padding in
        // different orders; both must fit native geometry before any mutation.
        let [leading, trailing] = feedback.padding;
        let padded_extent = (extent + leading + trailing).max(extent + trailing + leading);
        if !padded_extent.is_finite() || padded_extent > f32::MAX as f64 {
            return Err(InstanceError::InvalidCollectionFeedback);
        }
        Ok(Some(by_view))
    }
    /// Called only after preflight and release of the transferred pin categories.
    pub(super) fn feedback(
        &mut self,
        u: &mut Update<'_>,
        frames: &[Frame],
        mut feedback: CollectionFeedback,
        by_view: &BTreeMap<ViewId, usize>,
        mut fill: CollectionFill,
    ) -> Result<(bool, Option<CollectionEdges>), InstanceError> {
        let waited = self.request_waits();
        self.animating = fill.animating;
        // A report gives where the port is, unless the host animates.
        self.port_unknown &= fill.animating;
        // @ref LLP 1010 §6.9 — the padding the host laid out, taken in now
        // that the report is accepted. A new padding (a rotation's safe area)
        // is no travel: the anchor is taken on the old range and restored on
        // the new, so a followed end follows it.
        let padding = feedback.padding;
        let padding_changed = padding != self.padding();
        if !padding_changed {
            if let Some(edge) = self.travel_within(u, &feedback, by_view, fill)? {
                // No request, or one that waits on: the snapshot stays as it was.
                self.scrolling = fill.scrolling;
                self.reveal_shown(u);
                return Ok((false, edge));
            }
        }
        let changed_width = self
            .geometry
            .as_ref()
            .is_none_or(|g| g.cross != feedback.cross);
        // A new port size or width retires every row that left and builds
        // the whole window: only travel is sliced. A list's first report
        // retires nothing, so a slice there builds what shows and leaves its
        // lead pending (a tap's response frame, LLP 1072 §5).
        if self.geometry.as_ref().is_some_and(|g| {
            (g.port_cross, g.port_main, g.cross)
                != (feedback.port_cross, feedback.port_main, feedback.cross)
        }) {
            fill.limit = None;
        }
        // A new pin retires every row that left too, but keeps the report's
        // limit: a pinned row is already mounted, so it builds nothing, and
        // a finger landing on a moving list or lifting off one would
        // otherwise build the whole led window in one main-thread report
        // (25 rows, 25 ms, at a 6,000 pt/s fling's release).
        let pins_changed = self.geometry.as_ref().is_some_and(|g| {
            (g.focus_view, g.interaction_view) != (feedback.focus_view, feedback.interaction_view)
        });
        // An enclosing list is moving (LLP 1070 F2): build what this list
        // owes and nothing else, first report and resize included.
        if fill.ancestor_moving {
            fill.limit = Some(0);
        }
        if changed_width {
            self.end_preview(u)?;
        }
        if self
            .geometry
            .as_ref()
            .is_some_and(|g| g.interaction_view != feedback.interaction_view)
        {
            self.lose_preview_pin(u)?;
        }
        let previous = self.snapshot();
        self.scrolling = fill.scrolling;
        self.follow_into_view(fill);
        // A request that waited lands in the report that ends its wait (§2.6)
        let landing = waited && self.target.is_some() && !self.request_waits();
        if landing {
            self.refuse_if_inner_gone(u, frames)?;
        }
        // @ref LLP 1070 H4, Q3 (a): a row list anchors only an estimate
        // replaced by a first measurement, the jump virtualization makes. A
        // card measured again moves what follows it, as Chrome, which does
        // not anchor on the inline axis, moves it. So its re-measurements
        // land before the anchor is taken, including after invalidation.
        if self.axis == ListAxis::Horizontal && !changed_width {
            let (again, first): (Vec<_>, Vec<_>) = feedback.measurements.drain(..).partition(|m| {
                let row = &self.mounted[by_view[&m.view]];
                self.index.has_measurement_at(row.position)
            });
            for measurement in again {
                self.measure(by_view, measurement)?;
            }
            feedback.measurements = first;
        }
        let anchor_height = self
            .geometry
            .as_ref()
            .map_or(feedback.port_main, |g| g.port_main);
        self.leave_end_if_moved(fill.velocity);
        let extent = self.index.total_height();
        let anchor = Some(match self.restoring(&feedback) {
            Some(anchor) => anchor,
            None => {
                let resized = anchor_height != feedback.port_main && fill.velocity == 0.0;
                self.report_anchor(feedback.offset, anchor_height, padding, resized)?
            }
        });
        self.set_padding(padding);
        self.set_geometry(CollectionFeedback {
            measurements: Vec::new(),
            ..feedback.clone()
        });
        self.correction = None;
        if changed_width {
            self.invalidate_height_estimates()?;
        }
        // Measurements from the previous width are deliberately discarded. A fresh
        // snapshot supplies the new epoch for the next post-layout feedback.
        if !changed_width {
            for measurement in feedback.measurements.drain(..) {
                self.measure(by_view, measurement)?;
            }
        }
        self.check_preview_height(u)?;
        self.restore(anchor)?;
        let done_nested = self.settle_into_view(u, landing);
        // One that landed: its row is built with its window, and a nested
        // request's inner list begins its part.
        let row = if landing { self.target_row() } else { None };
        self.realize_window_with_row(u, frames, false, fill, row, pins_changed)?;
        if let Some((key, inner)) = done_nested {
            // Done where the port stands: its inner part begins in the outer
            // row the port shows, as at rest; refused, nothing has moved.
            let _ = self.begin_inner_part(u, frames, &key, &inner)?;
        } else if row.is_some() {
            self.begin_inner(u, frames)?;
        }
        self.reveal_shown(u);
        self.settle_start(extent);
        let mut now = self.snapshot();
        // Receiving a newer sequence without changing rows/extent/correction is
        // a fact update, not a new frame (avoids post-layout feedback loops).
        now.scroll_sequence = previous.scroll_sequence;
        let changed = padding_changed || previous != now;
        if changed {
            advance(&mut self.revision)?;
        }
        Ok((changed, self.edge_event(fill)?))
    }
    /// One mounted row's measured size into the index.
    pub(super) fn measure(
        &mut self,
        by_view: &BTreeMap<ViewId, usize>,
        measurement: RowMeasurement,
    ) -> Result<(), InstanceError> {
        let row = &self.mounted[by_view[&measurement.view]];
        let key = self.index.shared_key(row.position).unwrap().clone();
        let size = self.measured_size(row, measurement.size);
        self.index
            .set_measured_height_at(row.position, row.token, size)
            .map_err(index_error)?;
        if measurement.size == 0.0 {
            self.zero_heights.insert(key.to_string());
        } else if !self.zero_heights.is_empty() {
            self.zero_heights.remove(&*key);
        }
        Ok(())
    }
}
