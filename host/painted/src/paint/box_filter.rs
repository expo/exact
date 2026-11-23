//! CSS `filter` on a box (Filter Effects 1 §12; LLP 1055.000 §0): the box
//! and what it holds painted as they would show unfiltered, into a picture
//! of their own, the kernel's chain run over it, and the result placed
//! where they would have painted, so the box's clip, mask and opacity apply
//! after the filter, as CSS orders them. An SVG element's filter is its
//! `svg`'s (`island.rs`).
//!
//! The walk is the same walk with its backend swapped, so boxes, fields and
//! hits are recorded exactly as without a filter. The chain runs in device
//! pixels: exact under a translate and a scale, which is every box that is
//! not turned. A turned or skewed box is blurred and coloured rightly, but a
//! `drop-shadow()`'s offset follows the screen's axes, not the box's.
use super::*;

/// A frame larger than this on a side is not pictured: the box paints
/// unfiltered, as before.
const MAX_SIDE: u32 = 8192;

impl Painter {
    /// A node's content, through its `filter` when it has a chain and the
    /// backend places a picture (a fully transparent box's walk does not).
    pub(super) fn box_content(
        &mut self,
        walk: &mut Walk<'_, '_>,
        node: &NodeRef<'_>,
        rect: Rect4,
        ts: Transform,
        offset: PaintOffset,
        clip_rect: Option<Rect4>,
    ) {
        // A running transition's list over the row (LLP 1003.002 D8).
        let moving = (walk.scene.filters)(node.id)
            .map(|f| exact_kernel::motion::filter_list(&f, node.text_color()));
        let list = moving.as_ref().unwrap_or(&node.style.filter);
        let chain = (self.backend.supports_island_image() && !list.is_none())
            .then(|| exact_kernel::svg::scene::box_filter(list, node.style.text_color, self.dark))
            .flatten()
            .filter(|chain| !chain.primitives.is_empty());
        let (width, height) = self.viewport;
        let (pw, ph) = (
            (width * self.scale).ceil() as u32,
            (height * self.scale).ceil() as u32,
        );
        let Some(chain) = chain.filter(|_| pw > 0 && ph > 0 && pw <= MAX_SIDE && ph <= MAX_SIDE)
        else {
            self.content(walk, node, rect, ts, offset, clip_rect);
            return;
        };
        // What the chain reads and writes: the box and what it paints past
        // itself, grown by how far the chain spreads (its region over a box
        // of no size), in the box's space.
        let (x, y, w, h) = rect;
        let cap = (2.0 * w.max(h).max(64.0)).max(width.max(height));
        let ((left, top, right, bottom), _) = space::reach(self, walk, node, rect, ts, offset, cap);
        let (rx, ry, rw, rh) = chain.region;
        let region = (
            x - left + rx,
            y - top + ry,
            left + w + right + rw,
            top + h + bottom + rh,
        );
        // Damage never looks past a box; a row's bounds hold what it paints.
        self.damage.unsupported = true;
        self.row_reach(bbox(ts, region));
        // A backend that filters on the GPU draws the box's ops through the
        // chain, so its row stays kept (LLP 1172.023.001 D1).
        if self.backend.push_filter(&chain.primitives, region, ts) {
            self.content(walk, node, rect, ts, offset, clip_rect);
            self.backend.pop_filter();
            return;
        }
        // A picture is not a retained row.
        self.row_refuse();
        if let Some((recorder, inverse)) = self.recording().zip(ts.invert()) {
            let drawing = self.record_layer(|painter| {
                painter.content(walk, node, rect, ts, offset, clip_rect);
            });
            let normalized = tape::Recorder::default();
            normalized.nest(Rc::new(drawing), inverse);
            recorder.filter(
                Filter {
                    chain,
                    region,
                    viewport: self.viewport,
                    scale: self.scale,
                },
                Rc::new(normalized.tape()),
                ts,
            );
            return;
        }
        // What the picture reads: the region on the device, in whole pixels,
        // within the window grown by how far the chain reaches (at most four
        // windows of pixels), so the window's pixels read what lies past its
        // edge. A row is walked from its own origin, so there the window
        // stands at the row and the reach holds a filter left of or above
        // it. A region past that is drawn only over the window, which is all
        // of a page it shows.
        let s = self.scale;
        let (pw, ph) = (pw as f32, ph as f32);
        let b = bbox(ts, region);
        let (rx, ry, rw, rh) = chain.region;
        let device = ts.sx.hypot(ts.ky).max(ts.kx.hypot(ts.sy)) * s;
        let reach = [-rx, rx + rw, -ry, ry + rh].into_iter().fold(0.0, f32::max) * device;
        // (pw + 2r)(ph + 2r) = 4 pw ph.
        let most = (((pw + ph).powi(2) + 12.0 * pw * ph).sqrt() - (pw + ph)) / 4.0;
        let r = reach.min(most).ceil();
        let whole = (
            (b.0 * s).floor(),
            (b.1 * s).floor(),
            ((b.0 + b.2) * s).ceil(),
            ((b.1 + b.3) * s).ceil(),
        );
        let (x0, y0, x1, y1) = (
            whole.0.max(-r),
            whole.1.max(-r),
            whole.2.min(pw + r),
            whole.3.min(ph + r),
        );
        let shown = match (x0, y0, x1, y1) == whole {
            true => whole,
            false => (x0.max(0.0), y0.max(0.0), x1.min(pw), y1.min(ph)),
        };
        let (cw, ch) = ((x1 - x0).max(0.0), (y1 - y0).max(0.0));
        let mut picture: Box<dyn Backend> =
            Box::new(crate::raster::Raster::transparent_at((x0, y0)));
        picture.begin(cw / s, ch / s, s);
        let frame = self.replace_picture_backend(walk.scene.kernel, picture);
        self.content(walk, node, rect, ts, offset, clip_rect);
        let painted = self
            .restore_picture_backend(walk.scene.kernel, frame)
            .finish();
        let Ok(mut pixels) = painted else {
            return;
        };
        let upright = ts.kx == 0.0 && ts.ky == 0.0 && ts.sx > 0.0 && ts.sy > 0.0;
        let k = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        if !(k.is_finite() && k > 0.0 && shown.2 > shown.0 && shown.3 > shown.1) {
            return;
        }
        let (cw, ch) = (pixels.width(), pixels.height());
        // Upright, a box unit is `ts`'s scale in pixels and the picture's
        // corner is a point of the box's space; turned, only the scale is.
        let (filter, space) = if upright {
            (
                exact_kernel::svg::filter::Filter {
                    region,
                    primitives: chain.primitives,
                },
                exact_svg_raster::filter::Space {
                    origin: ((x0 / s - ts.tx) / ts.sx, (y0 / s - ts.ty) / ts.sy),
                    scale: (ts.sx * s, ts.sy * s),
                },
            )
        } else {
            (
                exact_kernel::svg::filter::Filter {
                    region: (0.0, 0.0, cw as f32 / (k * s), ch as f32 / (k * s)),
                    primitives: chain.primitives,
                },
                exact_svg_raster::filter::Space {
                    origin: (0.0, 0.0),
                    scale: (k * s, k * s),
                },
            )
        };
        exact_svg_raster::filter::run(&filter, pixels.data_mut(), cw as usize, ch as usize, space);
        let (sx0, sy0, sx1, sy1) = shown;
        let pixels = match shown == (x0, y0, x1, y1) {
            true => pixels,
            false => match crop(&pixels, (sx0 - x0, sy0 - y0, sx1 - sx0, sy1 - sy0)) {
                Some(window) => window,
                None => return,
            },
        };
        let (cw, ch) = (pixels.width(), pixels.height());
        self.backend.island_image(
            Arc::new(pixels),
            (sx0 / s, sy0 / s, cw as f32 / s, ch as f32 / s),
            Transform::identity(),
            0,
        );
    }
}

/// `(x, y, w, h)` of `pixels`, in whole pixels; `None` when it holds none.
fn crop(pixels: &Pixmap, (x, y, w, h): Rect4) -> Option<Pixmap> {
    let (x, y) = (x as usize, y as usize);
    let mut out = Pixmap::new(w as u32, h as u32)?;
    let (from, row) = (pixels.width() as usize * 4, w as usize * 4);
    for (line, dst) in out.data_mut().chunks_exact_mut(row).enumerate() {
        let at = (y + line) * from + x * 4;
        dst.copy_from_slice(&pixels.data()[at..at + row]);
    }
    Some(out)
}

/// The filter belongs to removed paint; its source commands remain replayable.
pub(super) struct Filter {
    chain: exact_kernel::svg::filter::Filter,
    region: Rect4,
    viewport: (f32, f32),
    scale: f32,
}

impl Filter {
    pub(super) fn region(&self, drawing: &tape::Tape, live: tape::Live<'_, '_>) -> Rect4 {
        let bounds = drawing.bounds(live, Transform::identity()).map(|r| {
            let (x, y, w, h) = self.chain.region;
            (r.0 + x, r.1 + y, r.2 + w, r.3 + h)
        });
        bounds.map_or(self.region, |r| rows::union(Some(self.region), r))
    }

    pub(super) fn replay(
        &self,
        backend: &mut dyn Backend,
        text: &mut TextEngine,
        drawing: &tape::Tape,
        ts: Transform,
        live: Option<tape::Live<'_, '_>>,
    ) {
        let (viewport, scale) = live.map_or((self.viewport, self.scale), |l| (l.viewport, l.scale));
        let mut raster = crate::raster::Raster::transparent();
        raster.begin(viewport.0, viewport.1, scale);
        drawing.replay_live(&mut raster, text, ts, live);
        if let Ok(source) = raster.finish() {
            let region = live.map_or(self.region, |l| self.region(drawing, l));
            filtered(backend, source, self.chain.clone(), region, ts, scale);
        }
    }
}

fn filtered(
    backend: &mut dyn Backend,
    source: Pixmap,
    chain: exact_kernel::svg::filter::Filter,
    region: Rect4,
    ts: Transform,
    scale: f32,
) {
    let upright = ts.kx == 0.0 && ts.ky == 0.0 && ts.sx > 0.0 && ts.sy > 0.0;
    let k = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
    if !(k.is_finite() && k > 0.0) {
        return;
    }
    // Its bounds on the device, whole pixels, within the frame.
    let s = scale;
    let b = bbox(ts, region);
    let x0 = (b.0 * s).floor().max(0.0);
    let y0 = (b.1 * s).floor().max(0.0);
    let x1 = ((b.0 + b.2) * s).ceil().min(source.width() as f32);
    let y1 = ((b.1 + b.3) * s).ceil().min(source.height() as f32);
    if !(x1 > x0 && y1 > y0) {
        return;
    }
    let (cw, ch) = ((x1 - x0) as u32, (y1 - y0) as u32);
    let Some(mut pixels) = Pixmap::new(cw, ch) else {
        return;
    };
    let stride = source.width() as usize * 4;
    let row = cw as usize * 4;
    for (line, out) in pixels.data_mut().chunks_exact_mut(row).enumerate() {
        let at = (y0 as usize + line) * stride + x0 as usize * 4;
        out.copy_from_slice(&source.data()[at..at + row]);
    }
    // Upright, a box unit is `ts`'s scale in pixels and the picture's
    // corner is a point of the box's space; turned, only the scale is.
    let (filter, space) = if upright {
        (
            exact_kernel::svg::filter::Filter {
                region,
                primitives: chain.primitives,
            },
            exact_svg_raster::filter::Space {
                origin: ((x0 / s - ts.tx) / ts.sx, (y0 / s - ts.ty) / ts.sy),
                scale: (ts.sx * s, ts.sy * s),
            },
        )
    } else {
        (
            exact_kernel::svg::filter::Filter {
                region: (0.0, 0.0, cw as f32 / (k * s), ch as f32 / (k * s)),
                primitives: chain.primitives,
            },
            exact_svg_raster::filter::Space {
                origin: (0.0, 0.0),
                scale: (k * s, k * s),
            },
        )
    };
    exact_svg_raster::filter::run(&filter, pixels.data_mut(), cw as usize, ch as usize, space);
    backend.island_image(
        Arc::new(pixels),
        (x0 / s, y0 / s, cw as f32 / s, ch as f32 / s),
        Transform::identity(),
        0,
    );
}
