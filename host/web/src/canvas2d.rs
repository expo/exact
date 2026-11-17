//! Canvas 2D on the web, the wasm half (LLP 1056 D4, D7): the browser lays
//! a canvas out, so `canvas2d-glue.js` watches each 2D canvas with a
//! `ResizeObserver` and reports its content box and device scale
//! (`exact_canvas_geometry`); the runner draws, and the stamped lists go to
//! the glue as `canvas2d` ops, replayed into the canvas element's real
//! `CanvasRenderingContext2D`. Frames are the page's: while a canvas wants
//! one, the glue advances the clock each animation frame, and the runner
//! draws the canvases that asked at the landed time.

use super::*;
use exact_runner::Geometry;

/// What the page has been told.
#[derive(Default)]
pub(crate) struct Watch {
    watched: SortedSet<ViewId>,
    pub(crate) captures: SortedMap<u64, RequestOut>,
    pub(crate) snapshotting: SortedMap<u64, exact_runner::canvas_capture::CanvasCapture>,
    frames: bool,
    /// The lists the last two batches with lists named by address: the glue
    /// reads them in place while it applies the batch (a batch applied
    /// inside another's is the second).
    kept: [Vec<exact_runner::CanvasList>; 2],
}

impl<D: DataSource> Host<D> {
    /// Route captures through browser geometry before snapshotting their drawing.
    pub(crate) fn canvas_capture_request(
        &mut self,
        r: RequestOut,
        batch: &mut Batch,
        immediate: &mut Vec<(u64, Outcome)>,
    ) -> Option<RequestOut> {
        if matches!(
            r.request.surface.as_deref(),
            Some(exact_runner::SurfaceRequest::Canvas { .. })
        ) && !self.runner.plan().surfaces.is_empty()
        {
            batch.canvas_capture_prepare(r.ticket);
            self.canvas2d.captures.insert(r.ticket, r);
            return None;
        }
        match r.request.take_canvas_capture() {
            Some(Ok(c)) => batch.canvas_capture(r.ticket, &c, r.request.grants.as_deref()),
            Some(Err(refusal)) => immediate.push((r.ticket, refusal)),
            None => return Some(r),
        }
        None
    }

    /// This turn's canvas work: new canvases to watch, due draws and their
    /// lists, and whether frames are wanted.
    pub(crate) fn canvas_turn(&mut self, batch: &mut Batch) {
        self.canvas_draw_turn(batch, false);
    }

    /// Retain all lists when several draw turns share one browser batch.
    fn canvas_draw_turn(&mut self, batch: &mut Batch, continuing: bool) -> bool {
        if self.runner.plan().surfaces.is_empty() {
            return false;
        }
        let live = self.runner.canvas_views();
        for view in &live {
            if self.canvas2d.watched.insert(*view) {
                batch.canvas2d_watch(*view);
            }
        }
        let gone: Vec<ViewId> = self
            .canvas2d
            .watched
            .iter()
            .copied()
            .filter(|v| !live.contains(v))
            .collect();
        for v in gone {
            self.canvas2d.watched.remove(&v);
        }
        self.runner.draw_canvases(&|_| true);
        let lists = self.runner.take_canvas_lists();
        for c in &lists {
            batch.canvas2d(c);
        }
        let drew = !lists.is_empty();
        if drew {
            if !continuing {
                self.canvas2d.kept.swap(0, 1);
                self.canvas2d.kept[1].clear();
            }
            self.canvas2d.kept[1].extend(lists);
        }
        for (ticket, r) in &self.canvas2d.captures {
            if !r.request.canvas_capture_pending() {
                batch.canvas_capture_prepare(*ticket);
            }
        }
        // Handles a Rust draw asked for: the page loads them (LLP 1056 D9).
        let images = self.runner.take_canvas_image_requests();
        if !images.is_empty() {
            batch.canvas2d_images(&images);
        }
        let frames = self.runner.canvas_wants_frame();
        if frames != self.canvas2d.frames {
            self.canvas2d.frames = frames;
            batch.canvas2d_frames(frames);
        }
        drew
    }

    /// The browser copied the completed drawing; encoding may finish later.
    pub fn canvas_snapshotted(&mut self, ticket: u64) -> bool {
        self.canvas2d
            .snapshotting
            .remove(&ticket)
            .is_some_and(|c| c.snapshotted().is_ok())
    }

    /// The page has reported this commit's geometry and applied its draws.
    pub fn canvas_capture_ready(&mut self, ticket: u64) -> String {
        let mut batch = Batch::new();
        if let Some(r) = self.canvas2d.captures.remove(&ticket) {
            if self.runner.holds(ticket) {
                let mut drew = false;
                for turn in 0..16 {
                    drew = self.canvas_draw_turn(&mut batch, turn != 0);
                    if !r.request.canvas_capture_pending() || !drew {
                        break;
                    }
                }
                self.runner.fail_stalled_canvas_capture(&r.request);
                if r.request.canvas_capture_pending() {
                    if drew {
                        batch.canvas_capture_prepare(ticket);
                    }
                    self.canvas2d.captures.insert(ticket, r);
                    return self.finish(batch, None);
                }
                match r.request.take_canvas_capture() {
                    Some(Ok(c)) => {
                        batch.canvas_capture(ticket, &c, r.request.grants.as_deref());
                        self.canvas2d.snapshotting.insert(ticket, c);
                    }
                    Some(Err(outcome)) => {
                        return match self.runner.fulfill(ticket, outcome) {
                            Ok(Some(receipt)) => self.batch_from(
                                batch,
                                &[Timed {
                                    at_ms: self.now_ms,
                                    receipt,
                                }],
                                None,
                            ),
                            Ok(None) => self.finish(batch, None),
                            Err(e) => self.finish(batch, Some(&format!("{e:?}"))),
                        };
                    }
                    None => unreachable!("only canvas captures wait for browser geometry"),
                }
            }
        }
        self.finish(batch, None)
    }

    /// A 2D canvas's content box (CSS px) and device scale, from the page's
    /// `ResizeObserver`; the batch with any draw it caused.
    pub fn canvas_geometry(&mut self, view: ViewId, width: f64, height: f64, scale: f64) -> String {
        let bitmap = self.runner.canvas_bitmap(view);
        self.runner.set_canvas_geometry(
            view,
            Geometry {
                width,
                height,
                scale,
                bitmap,
                settings: self.runner.canvas_settings(view),
            },
        );
        let mut batch = Batch::new();
        self.canvas_turn(&mut batch);
        self.finish(batch, None)
    }
}

impl<D: DataSource> Host<D> {
    /// The page decoded (or failed) an image handle (LLP 1056 D9): `payload`
    /// is `src\0width\0height\0ok\0lifetimes`, the lifetimes a JSON array of
    /// the canvases whose TypeScript draw asked for it on the page. The
    /// canvases that asked draw again, cause `"image"`.
    pub fn canvas_image(&mut self, payload: &str) -> String {
        let mut parts = payload.split('\0');
        let src = parts.next().unwrap_or("");
        let num = |p: Option<&str>| p.and_then(|v| exact_num::parse_f64(v).ok()).unwrap_or(0.0);
        let (w, h) = (num(parts.next()), num(parts.next()));
        let ok = parts.next() == Some("1");
        let lifetimes: Vec<u64> = parts
            .next()
            .unwrap_or("")
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .filter_map(|v| v.trim().parse().ok())
            .collect();
        let result = if ok {
            Ok((w as u32, h as u32))
        } else {
            Err("the image did not load".to_string())
        };
        self.runner.canvas_image(src, result, &lifetimes);
        let mut batch = Batch::new();
        self.canvas_turn(&mut batch);
        self.finish(batch, None)
    }

    /// A font finished loading on the page (LLP 1056 D8): canvases that drew
    /// text draw again, cause `"font"`.
    pub fn canvas_fonts(&mut self) -> String {
        self.runner.canvas_fonts_loaded();
        let mut batch = Batch::new();
        self.canvas_turn(&mut batch);
        self.finish(batch, None)
    }
}

/// GPU surfaces' export group (LLP 1047 D3), beside [`crate::host!`]: the generated
/// entry invokes it when the plan has a canvas with a surface, and the glue
/// calls it only for one.
#[macro_export]
macro_rules! surface_exports {
    () => {
        /// A 2D canvas's content box and device scale (LLP 1056 D4), from
        /// the page's `ResizeObserver`; returns the batch length.
        #[no_mangle]
        pub extern "C" fn exact_canvas_geometry(
            view: u32,
            width: f64,
            height: f64,
            scale: f64,
        ) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().canvas_geometry(view, width, height, scale))
        }

        /// Confirm the browser copied a completed drawing.
        #[no_mangle]
        pub extern "C" fn exact_canvas_snapshotted(ticket: f64) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().canvas_snapshotted(ticket as u64) as u32)
        }

        /// Finish a capture after the page reports this commit's canvas boxes.
        #[no_mangle]
        pub extern "C" fn exact_canvas_capture_ready(ticket: f64) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().canvas_capture_ready(ticket as u64))
        }

        /// A Canvas 2D image handle loaded or failed (LLP 1056 D9); the
        /// payload is in the input buffer. Returns the batch length.
        #[no_mangle]
        pub extern "C" fn exact_canvas_image(len: u32) -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().canvas_image(len as usize))
        }

        /// A font finished loading (LLP 1056 D8); returns the batch length.
        #[no_mangle]
        pub extern "C" fn exact_canvas_fonts() -> u32 {
            EXACT_BRIDGE.with(|b| b.borrow_mut().canvas_fonts())
        }

        /// Publish or clear a named surface record; returns the batch length.
        #[no_mangle]
        pub extern "C" fn exact_surface_record(len: u32) -> u32 {
            EXACT_BRIDGE.with(|b| {
                let Ok(mut bridge) = b.try_borrow_mut() else {
                    eprintln!("exact_surface_record refused: nested bridge export");
                    return 0;
                };
                bridge.surface_record(len as usize)
            })
        }
    };
}
