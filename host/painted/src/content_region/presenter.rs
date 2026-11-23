//! Selected native content, successful-paint publication and autonomous wake.
use super::*;
use crate::paint::FrameTarget;
impl<D: DataSource> Presenter<D> {
    /// Reuse a presented CPU frame only after every other reader has released it.
    pub fn recycle_frame(&mut self, pixels: Arc<Pixmap>) {
        self.brush.recycle_frame(pixels);
    }
    /// The last frame's (paint, readback) milliseconds; CPU readback is zero.
    pub fn last_frame_ms(&self) -> Option<(f64, f64)> {
        self.brush.last_frame_ms()
    }
    /// Explicit opt-in, before the first layout, for a native consumer.
    #[allow(clippy::too_many_arguments)]
    pub fn boot_with_content_region(
        plan: &[u8],
        data: D,
        viewport: (f32, f32),
        scale: f32,
        assets: PathBuf,
        choice: PainterChoice,
        region: crate::content_region::ContentRegionRegistration,
    ) -> Result<(Self, Option<String>), HostError> {
        let (mut presenter, error) = Self::boot_with_assets(
            crate::host::PlanBytes::Copied(plan),
            data,
            viewport,
            scale,
            Assets::embedded(assets),
            PainterBoot::selected(choice),
            TextEngine::shared_for_assets,
            None,
            "/",
            Some(region),
            crate::app::Services::default(),
        )?;
        presenter.measure(); // LLP 1079 D1
        Ok((presenter, error))
    }
    /// Region completion readiness. No timer or input event is needed to resume.
    #[cfg(unix)]
    pub fn content_region_fd(&self) -> Option<std::os::unix::io::RawFd> {
        self.host.content_region_fd()
    }
    pub(super) fn poll_content_region(&mut self) -> Option<String> {
        if let Some(error) = self.host.content_region_paint_scale(self.brush.scale) {
            return Some(error);
        }
        match self.host.poll_content_region() {
            Ok(true) => {
                self.dirty = true;
                self.queue_collections();
                None
            }
            Ok(false) => None,
            Err(e) => Some(e),
        }
    }
    /// Paint a frame and publish its immutable pixels, hits and native source together.
    /// Text-cache eviction waits for the paint's end
    /// ([`crate::text::cache::deferring_eviction`]): a paint that misses walks
    /// the cache once, not once per paragraph.
    pub fn frame(&mut self) -> Arc<Pixmap> {
        self.frame_for(FrameTarget::Pixels)
            .expect("capture has pixels or a CPU fallback")
    }

    pub(crate) fn frame_for(&mut self, target: FrameTarget) -> Option<Arc<Pixmap>> {
        self.frame_with_canvas_tick(target, true)
    }

    pub(crate) fn frame_with_canvas_tick(
        &mut self,
        target: FrameTarget,
        canvas_frame: bool,
    ) -> Option<Arc<Pixmap>> {
        let had_damage = self.dirty || self.picture_dirty;
        let preserves_picture = target == FrameTarget::Geometry;
        #[cfg(windows)]
        let preserves_picture = preserves_picture
            || (self.surfaces.direct_window.is_some() && target != FrameTarget::Window);
        // Its session's roles, whatever else lives on the thread (LLP 1121 stage 5).
        let _roles = self.host.runner().bind_roles();
        let frame = crate::text::cache::deferring_eviction(|| {
            // The probe draws nothing, so the canvases' tick stays with the frame's own paint.
            if self.focus_reveal.is_some() || !self.host.flights.reveal.is_empty() {
                self.paint_frame(FrameTarget::Geometry, false, true);
                if self.last_frame_succeeded {
                    self.reveal_before_paint();
                }
            }
            let frame = self.paint_frame(target, canvas_frame, false);
            if !self.last_frame_succeeded {
                return frame;
            }
            let immediate = !self.display.submitting();
            let scrollbar_hover = immediate && self.scrollbars_shown();
            // Layout can move a caret, a fragment target (LLP 1153 D5),
            // or the exposed part of a scrollbar under a resting pointer.
            let revealed = self.reveal_field_cursor();
            let held = self.hold_fragment();
            let frame = if revealed.is_some() || held || scrollbar_hover {
                let frame = self.paint_frame(target, false, false);
                if immediate && self.last_frame_succeeded {
                    self.scrollbars_shown();
                }
                frame
            } else {
                frame
            };
            // Report a field's caret reveal after the reveal repaint publishes
            // its updated range geometry, so `rangeFrame()` in `scroll=` sees
            // the post-scroll position.
            if let Some(id) = revealed.filter(|_| self.last_frame_succeeded) {
                self.scroll_end_moved(id, Completion::Instant);
                self.collection_scrolled(id);
                if self.dirty {
                    return self.paint_frame(target, false, false);
                }
            }
            frame
        });
        // A geometry query or non-window snapshot has not presented the new picture.
        if preserves_picture {
            self.picture_dirty |= had_damage;
        } else if self.last_frame_succeeded {
            self.picture_dirty = false;
        }
        frame
    }
    /// Whether the last paint produced a current picture rather than a fallback.
    pub fn last_frame_succeeded(&self) -> bool {
        self.last_frame_succeeded
    }
    fn paint_frame(
        &mut self,
        target: FrameTarget,
        canvas_frame: bool,
        reveal_probe: bool,
    ) -> Option<Arc<Pixmap>> {
        self.brush.shared.update(&self.host);
        self.brush.composition = self.ime_paint();
        self.alert_sync();
        self.prepare_overlay();
        if self.brush.scrollbars.enabled {
            self.brush.scrollbars.limits = self.collection_scroll_limits();
        }
        self.brush.placements = self.surfaces.placements(&self.host, true);
        self.brush.composites.canvases = self.surfaces.composites();
        if target != FrameTarget::Geometry {
            self.draw_and_capture_canvases(canvas_frame);
            let previous = std::mem::take(&mut self.brush.canvases.readbacks);
            self.brush.canvases.readbacks = self.surfaces.pixels(
                &mut self.host,
                self.brush.scale,
                previous,
                self.brush.window_canvas,
            );
        }
        // The hatches' overlays as published (LLP 1075.003.000.001 §2.2.1):
        // read here, at the start of the paint; the walk calls no hatch.
        self.brush.overlays = (self.hatches.overlays.iter())
            .map(|(id, shown)| (*id, shown.pixels.clone()))
            .collect();
        // The display carrier stages the paint's owners/boxes and publishes
        // them only on the matching flip. Headless/agent frames stay immediate.
        let deferred = self.display.submitting();
        if let Some(error) = self.poll_content_region() {
            self.host.log(error);
        }
        if let Some(error) = self.refine_collections() {
            self.host.log(error);
        }
        self.refresh_looks(None); // the focus's ring (LLP 1121 D8)
        self.brush.focus_presses = self.focus.is_some_and(|id| self.presses_itself(id));
        self.restore_scroll_anchors();
        self.arm_scroll_anchors();
        self.scroll_timelines();
        let mut dirty = self.host.take_row_dirty();
        // A ripple paints its node's row again every frame it shows.
        self.brush.ripples.keys().for_each(|key| dirty.node(key));
        self.sync_media(&mut dirty);
        self.brush.numerals.now = self.host.now();
        self.brush.numerals.still = self.host.runner().viewport().preferences.reduced_motion;
        // A geometry paint records no layer, and leaves a played ripple played.
        self.brush.ripples.reader = self.host.lowering && target != FrameTarget::Geometry;
        self.brush.rows_dirty(self.host.kernel(), dirty);
        self.sync_stage();
        let stage = self.stage_paint();
        self.brush.set_stage(self.host.kernel(), stage);
        let shows = self.stage_shows();
        let roots = self.host.roots();
        let collection_limits = if self.host.content_region().is_some() {
            self.collection_scroll_limits()
        } else {
            BTreeMap::new()
        };
        self.brush.flow_damage(
            &self.host,
            &self.boxes,
            self.viewport,
            self.page,
            &self.scroll,
            self.pointer,
            self.focus,
        );
        let model_scroll = deferred.then(|| self.collection_paint_scroll()).flatten();
        // Display submission temporarily takes the boxes that place the menu.
        let menu = self.menu_paint().or_else(|| self.display.menu.clone());
        let selection = self.focus.map(|id| self.field_selection(id));
        let host = &self.host;
        let presented = |id: ViewId| host.presented(id);
        let page = if deferred {
            self.model_page()
        } else {
            self.page
        };
        let scene = Scene {
            kernel: host.kernel(),
            hidden: &|id| match shows.is_empty() {
                true => host.route_visibility(id).0,
                false => host.route_hidden_but(id, &shows),
            },
            roots: &roots,
            presented: &presented,
            paths: &|id| host.presented_path(id),
            filters: &|id| host.presented_filter(id),
            scroll: model_scroll.as_ref().unwrap_or(&self.scroll),
            page,
            images: &self.images.bitmaps,
            focus: self.focus,
            selection,
            caret_upstream: self.focus.is_some_and(|id| self.field_cursor(id).upstream),
            pointer: self.pointer,
            controls: &self.controls,
            chosen: &self.chosen,
            menu,
        };
        let region = host.content_region();
        let feedback_before = region.is_some_and(|r| r.publication_painted());
        // Capture only at a successful current CPU picture, before display_frame
        // restores A. Retained replay never obtains new B handlers/arguments.
        let mut handlers = None;
        let mut capture = |key, kind| {
            let node = host.kernel().node_by_key(key)?;
            let handlers = handlers.get_or_insert_with(|| host.runner().handlers());
            handlers
                .get(&node.id)
                .filter(|events| events.contains(&kind))?;
            Some(host.runner().capture_action_binding(key, kind))
        };
        let eligible = |key| host.retained_action_eligible(key);
        let motion = |key, picture: &std::rc::Rc<()>| {
            self.retained_motion
                .as_ref()
                .is_some_and(|permit| permit.allows(host, key, picture))
        };
        let mut actions = crate::paint::RegionActions {
            capture: &mut capture,
            eligible: &eligible,
            motion: &motion,
        };
        let mut paint = |brush: &mut Painter| match region {
            _ if reveal_probe => brush.reveal_probe(&scene, self.viewport),
            Some(region) => brush
                .paint_region(
                    &scene,
                    self.viewport,
                    region,
                    &collection_limits,
                    &mut actions,
                )
                .map(|frame| frame.map_pixels(Some)),
            None => brush.paint_for(&scene, self.viewport, target),
        };
        let mut painted = paint(&mut self.brush);
        if let Err(e) = &painted {
            if (self.choice == PainterChoice::Auto && matches!(self.brush.backend(), "gpu" | "ink"))
                || self.brush.lost_device_needs_cpu()
            {
                eprintln!("exact: paint: {e}; painting on the CPU from here");
                self.brush.replace_backend(Box::new(Raster::new()));
                self.painter = cpu_info();
                painted = paint(&mut self.brush);
            }
        }
        let notes = self.brush.take_notes();
        let pictures = std::mem::take(&mut self.brush.composites.pictures);
        self.surfaces.children_pictures(pictures);
        self.last_frame_succeeded = painted.is_ok();
        let (pixmap, boxes) = match painted {
            Ok(Frame { pixmap, boxes }) => {
                if region.is_some() && !deferred {
                    // One bounded-to-viewport retained surface, separate from
                    // glyph/font/image ledgers. No copy on ordinary opt-out.
                    self.last_region_frame = pixmap.clone();
                    self.last_region_scale = Some(self.brush.scale.to_bits());
                    if let Some(e) = self.host.content_region_painted(&self.brush) {
                        self.host.log(e);
                    }
                }
                (pixmap, boxes)
            }
            Err(e) => {
                eprintln!("exact: paint: {e}");
                if let Some(old) = &self.last_region_frame {
                    // Preserve old pixels without stretching or stale outside
                    // margins in the DRM carrier. New viewport area is opaque;
                    // old source and hit positions keep their original origin.
                    (
                        Some(retained_surface(old, self.viewport, self.brush.scale)),
                        if self.last_region_scale == Some(self.brush.scale.to_bits()) {
                            std::mem::take(&mut self.boxes)
                        } else {
                            Vec::new()
                        },
                    )
                } else {
                    let w = ((self.viewport.0 * self.brush.scale).round() as u32).max(1);
                    let h = ((self.viewport.1 * self.brush.scale).round() as u32).max(1);
                    let mut blank = Pixmap::new(w, h).expect("a viewport has pixels");
                    blank.fill(tiny_skia::Color::WHITE);
                    (Some(Arc::new(blank)), std::mem::take(&mut self.boxes))
                }
            }
        };
        if self.last_frame_succeeded && !reveal_probe && self.host.flights.enabled {
            self.host
                .flights
                .recorded
                .clone_from(&self.brush.shared.recorded);
            self.host
                .retire_unrendered_flights(&self.brush.shared.unrendered);
        }
        for key in self.brush.flow_failures().collect::<Vec<_>>() {
            if let Some(node) = self.host.kernel().node_by_key(key) {
                self.host.log(format!(
                    "wrap-flow: text #{} is incomplete and uses ordinary layout",
                    node.id
                ));
            }
        }
        for note in notes {
            self.host.log(note);
        }
        self.boxes = boxes;
        self.boxes_serial += 1;
        if self.last_frame_succeeded {
            self.boxes_page = page;
            self.ranges.shown.set(self.ranges.session());
        }
        self.publish_ranges();
        if self.last_frame_succeeded {
            self.host.flow_damage.clear();
        }
        if self.last_frame_succeeded && !deferred {
            if let Some(region) = self.host.content_region() {
                // Replay clamps against the selected picture BEFORE drawing.
                // Publish precisely those offsets only after backend success;
                // failed B must not shrink A's still-visible scroll state.
                for b in &self.boxes {
                    if let Some(offset) = b.scroll {
                        if self.host.kernel().node(b.id).is_some_and(|n| {
                            self.brush.region_scroll_bounds(region, n.key).is_some()
                        }) && (offset != (0., 0.) || self.scroll.contains_key(&b.id))
                        {
                            self.scroll.insert(b.id, offset);
                        }
                    }
                }
            }
        }
        if !deferred
            && !feedback_before
            && self.last_frame_succeeded
            && self
                .host
                .content_region()
                .is_some_and(|r| r.publication_painted())
        {
            self.queue_collections();
        }
        self.dirty = self.collection.pending();
        if self.last_frame_succeeded {
            self.image_paint_pending = false;
        }
        if let Some(e) = self.sync_images() {
            self.host.log(e);
        }
        pixmap
    }
}

fn retained_surface(old: &Arc<Pixmap>, viewport: (f32, f32), scale: f32) -> Arc<Pixmap> {
    let width = ((viewport.0 * scale).round() as u32).max(1);
    let height = ((viewport.1 * scale).round() as u32).max(1);
    if (width, height) == (old.width(), old.height()) {
        return old.clone();
    }
    let mut next = Pixmap::new(width, height).expect("a viewport has pixels");
    next.fill(tiny_skia::Color::WHITE);
    let copied = old.width().min(width) as usize * 4;
    for y in 0..old.height().min(height) as usize {
        next.data_mut()[y * width as usize * 4..][..copied]
            .copy_from_slice(&old.data()[y * old.width() as usize * 4..][..copied]);
    }
    Arc::new(next)
}
