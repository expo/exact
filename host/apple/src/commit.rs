//! Receipt publication and the presentation that follows it.
use super::*;

impl<D: DataSource> Host<D> {
    /// [`crate::link::TurnMotion`]'s commit: what Core Animation can play
    /// of the motion a commit's sync started, and the rest resampled.
    pub(crate) fn lower_committed_motion(&mut self, sync: &MotionSync) {
        let runner = &self.runner;
        svg_lower::eligibility(
            runner.kernel(),
            &mut self.engine,
            sync,
            self.svg.box_motion,
            self.svg.live_filters,
            &|v| !runner.handlers_of(v).is_empty(),
        );
        svg_lower::hand_over_paths(runner.kernel(), &mut self.engine, self.svg.live_filters);
        // A reveal changes a held play without changing its style row.
        for (key, _) in &sync.animations {
            if let Some(view) = self.keys.get(&node_key(*key)).copied() {
                self.svg.element(runner.kernel(), view);
            }
        }
        for view in svg_lower::resample(
            runner.kernel(),
            &mut self.engine,
            self.svg.live_filters,
            &|v| !runner.handlers_of(v).is_empty(),
        ) {
            self.svg.element(self.runner.kernel(), view);
        }
    }

    pub(super) fn commit(&mut self, receipts: &[Timed], error: Option<String>) -> String {
        self.commit_into(receipts, error, Batch::new())
    }

    pub(super) fn commit_into(
        &mut self,
        receipts: &[Timed],
        error: Option<String>,
        batch: Batch,
    ) -> String {
        let (mut batch, error) = self.commit_tree(receipts, error, batch);
        // A receipt can end an Arrange contact; its terminal runs at receipt time.
        let arrange = self.arrange_after_commit(&mut batch);
        self.group_after_commit(&mut batch);
        self.commit_finish(batch, error.or(arrange))
    }

    /// The receipts into the mirror, the engine and layout; not yet presented.
    pub(super) fn commit_tree(
        &mut self,
        receipts: &[Timed],
        error: Option<String>,
        mut batch: Batch,
    ) -> (Batch, Option<String>) {
        let _roles = self.runner.bind_roles(); // Its own, crossing (LLP 1121 stage 5).
        self.native_retire_removed_owner(&mut batch);
        self.native_note_receipts(receipts);
        // A commit that changed no node cannot move a route's overflow; marking
        // it would wake a reply pump after every firing of an idle timer.
        self.route_overflow.dirty |= receipts.iter().any(|t| !unchanged(&t.receipt));
        let bulk_handlers = (receipts
            .iter()
            .map(|t| t.receipt.created.len())
            .sum::<usize>()
            > 1)
        .then(|| self.runner.handlers());
        self.close_new_top_layer(receipts);
        if receipts.iter().any(|t| !unchanged(&t.receipt)) {
            self.prepare_flat(&mut batch);
        }
        let mut repaint = Vec::new();
        let mut omitted_gone = IdSet::default();
        for t in receipts {
            let r = &t.receipt;
            // Flights capture their leavers before an exit takes them
            // out of the presenter's maps (LLP 1013.000 D4.1).
            self.begin_flights(r, &mut batch);
            if let Some(motion) = self.turn.motion {
                (motion.exits)(self, r, &mut batch);
            }
            for key in &r.destroyed {
                if let Some(id) = self.keys.remove(key) {
                    self.ranks.remove(&id);
                    self.paint.runs.remove(&id);
                    self.paint.opacities.remove(&id);
                    if let Some((owner, _)) = self.inline_runs.remove(&id) {
                        self.dirty_paragraphs.insert(owner);
                    } else if self.svg.destroyed(id) {
                        self.mirror.remove(&id);
                    } else if !self.native_selected_id(id) {
                        let omitted = self.flat.nodes.remove(&id);
                        if omitted {
                            omitted_gone.insert(id);
                        }
                        self.mirror.remove(&id);
                        self.end_flight_of(id);
                        let motion = self.turn.motion;
                        let held = motion.is_some_and(|m| (m.exit_holds)(self, id, &mut batch));
                        if !held && !omitted {
                            batch.destroy(id);
                        }
                    }
                    self.transform_drags.remove(id);
                }
            }
            repaint.extend(self.renew_views(r, &mut batch));
            for key in &r.created {
                if let Some(node) = self.runner.kernel().node_by_key(*key) {
                    let id = node.id;
                    let handlers = bulk_handlers.as_ref().map_or_else(
                        || self.runner.handlers_of(id),
                        |all| all.get(&id).cloned().unwrap_or_default(),
                    );
                    self.create(id, &handlers, &mut batch);
                }
            }
            for key in &r.touched {
                if let Some(node) = self.runner.kernel().node_by_key(*key) {
                    let id = node.id;
                    self.update(id, &mut batch);
                }
            }
        }
        self.withdraw_flat_hits(&omitted_gone, &mut batch);
        // The `portal` movers placement moved this batch or before.
        let mut movers: Vec<ViewId> = self.declared_told.iter().map(|&(m, _)| m).collect();
        if !receipts.is_empty() {
            self.tell_declared_parents(&mut batch);
            movers.extend(self.declared_told.iter().map(|&(m, _)| m));
        }
        // Receipts share the final kernel tree. A parent touched by an early
        // timer can already name a child created by a later timer in this seek.
        // All surviving views must exist before any final child list is attached.
        let mut parents: Vec<ViewId> = (receipts.iter())
            .flat_map(|t| t.receipt.created.iter().chain(t.receipt.touched.iter()))
            .filter_map(|key| self.runner.kernel().node_by_key(*key).map(|n| n.id))
            .collect();
        // A list that receives a mover goes first, so a presenter moves its
        // view in one step: AppKit keeps the focus of a view it moves, not of
        // one removed and then added (LLP 1013.002 D5).
        if !movers.is_empty() {
            parents.sort_by_key(|&id| !self.receives(id, &movers));
        }
        for id in parents {
            self.emit_children(id, &mut batch);
        }
        for id in std::mem::take(&mut self.flat.parents) {
            if self.runner.kernel().node(id).is_some() {
                self.emit_children(id, &mut batch);
            }
        }
        self.emit_paragraphs(&mut batch);
        let roots = self.runner.roots();
        if roots != self.roots {
            self.roots = roots.clone();
            batch.roots(&roots);
        }
        if !receipts.is_empty() {
            self.emit_title(&mut batch);
        }
        let mut height_target_error = None;
        // Runner receipts retain due-time order. Unobserved motion starts at
        // that due time; a late receipt cannot rewind an already presented
        // frame/hold. Match Web's floor at the engine's current presentation
        // time, not this batch's final time. No timer work enters pointer moves.
        for t in receipts {
            let seek = self.engine.advance(self.engine_time(t.at_ms));
            debug_assert!(seek.is_ok(), "the clock never runs backwards here");
            let mut sync = self.runner.kernel().motion_sync(&t.receipt);
            self.presence.looks.apply(self.runner.kernel(), &mut sync);
            self.spare_exits(&mut sync);
            let applied = sync.apply(&mut self.engine);
            debug_assert!(applied.is_ok(), "kernel rows are always valid engine input");
            if let Some(motion) = self.turn.motion {
                (motion.commit)(self, &sync);
                (motion.play_exits)(self, &mut batch);
            }
            self.start_flights(&t.receipt);
            if let Some(motion) = self.turn.motion {
                (motion.seed_layout)(self, &t.receipt, &mut batch);
            }
            self.sync_paint(&t.receipt, &mut batch);
            if let Some(drag) = self.turn.drag {
                (drag.reconcile_heights)(self, &mut batch, true);
            }
            let synced = self.sync_height_owner();
            debug_assert!(synced.is_ok(), "validated height sync");
            if let Some(motion) = self.turn.motion {
                if let Err(error) = (motion.heights)(self) {
                    height_target_error.get_or_insert(error);
                }
            }
            // Latest target/declaration must reach the held slot before an
            // invalidated header cancels it (negative delays sample at once).
            if let Some(drag) = self.turn.drag {
                (drag.cancel_invalid_height)(self);
                (drag.reconcile_transforms)(self, &mut batch);
            }
        }
        self.repaint_renewed(&repaint, &mut batch);
        let seek = self.engine.advance(self.engine_time(self.now_ms));
        debug_assert!(seek.is_ok(), "the clock never runs backwards here");
        // An advance, a list's feedback or an action whose commits changed no
        // node (state nothing shows; a `sent` reader answered at each MIDI
        // message's time, LLP 1125 D6, read by nothing on screen; a scroll
        // that mounted no row) lays out only what such a commit can move
        // (`inert_layout`), not a whole layout and paint order walk.
        let inert = self.inert_entry && receipts.iter().all(|t| unchanged(&t.receipt));
        let layout_error = if height_target_error.is_some() {
            self.layout_owed = true;
            height_target_error
        } else if inert {
            self.inert_layout(&mut batch, !receipts.is_empty()).err()
        } else {
            self.layout(&mut batch).err()
        };
        if layout_error.is_some() && !receipts.is_empty() {
            self.emit_ranks(&mut batch);
        }
        if let Some(canvas) = self.turn.canvas {
            (canvas.turn)(self, &mut batch);
        }
        for s in self.runner.take_surface_updates() {
            batch.surface(&s);
        }
        // The capabilities the actions called, after their commits, in order.
        // @ref LLP 1038 D7 — drain once, after all commits in this batch.
        if let Some(change) = self.runner.take_router_change() {
            batch.router(&change);
        }
        for c in self.runner.take_commands() {
            batch.command(&c.name, &c.args, c.source);
        }
        batch.timed(&mut self.runner, false);
        (batch, error.or(layout_error))
    }

    /// Persist, then the presentation after every commit in the batch.
    pub(super) fn commit_finish(&mut self, mut batch: Batch, error: Option<String>) -> String {
        self.persist();
        self.emit_language(&mut batch);
        if let Some(drag) = self.turn.drag {
            (drag.emit_transforms)(self, &mut batch);
        }
        self.present(&mut batch, false);
        self.finish(batch, error)
    }
}
