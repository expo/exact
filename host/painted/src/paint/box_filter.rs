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
        offset: (f32, f32),
        clip_rect: Option<Rect4>,
    ) {
        let chain = (self.backend.supports_island_image() && !node.style.filter.is_none())
            .then(|| {
                exact_kernel::svg::scene::box_filter(
                    &node.style.filter,
                    node.style.text_color,
                    self.dark,
                )
            })
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
        // A picture is not a retained row, and damage never looks past a box.
        self.row_refuse();
        self.damage.unsupported = true;
        let (x, y, w, h) = rect;
        let cap = (2.0 * w.max(h).max(64.0)).max(width.max(height));
        let ((left, top, right, bottom), _) = space::reach(walk, node, rect, offset, cap);
        let mut picture: Box<dyn Backend> = Box::new(crate::raster::Raster::transparent());
        picture.begin(width, height, self.scale);
        let frame = std::mem::replace(&mut self.backend, picture);
        self.content(walk, node, rect, ts, offset, clip_rect);
        let painted = std::mem::replace(&mut self.backend, frame).finish();
        let Ok(source) = painted else {
            return;
        };
        // What the chain reads and writes: the box and what it paints past
        // itself, grown by how far the chain spreads (its region over a box
        // of no size), in the box's space.
        let (rx, ry, rw, rh) = chain.region;
        let region = (
            x - left + rx,
            y - top + ry,
            left + w + right + rw,
            top + h + bottom + rh,
        );
        let upright = ts.kx == 0.0 && ts.ky == 0.0 && ts.sx > 0.0 && ts.sy > 0.0;
        let k = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        if !(k.is_finite() && k > 0.0) {
            return;
        }
        // Its bounds on the device, whole pixels, within the frame.
        let s = self.scale;
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
        self.backend.island_image(
            Arc::new(pixels),
            (x0 / s, y0 / s, cw as f32 / s, ch as f32 / s),
            Transform::identity(),
            0,
        );
    }
}
