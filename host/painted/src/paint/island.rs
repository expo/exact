//! SVG islands on Linux (LLP 1055.000 D2, D7, D10): what vello and a clip
//! stack cannot express is painted by a child painter over tiny-skia into
//! pixels, worked on by `exact-svg-raster`, and composited back through
//! [`Backend::surface_image`]: the same path on the CPU and GPU painters.
//! tiny-skia is already this host's; the module is linked in-process here
//! (§8 ruling 4 keeps it out of the Apple core only).

use super::{Painter, Rect4, Shape};
use exact_kernel::svg::filter::Filter;
use exact_kernel::svg::scene::{Item, Mask, ShapePaint};
use exact_kernel::svg::transform as tf;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};

/// The largest island side, in device pixels.
const MAX_SIDE: f32 = 4096.0;

/// `r` (x, y, w, h) mapped by `ts`: its bounds.
pub(super) fn bounds(ts: Transform, r: (f32, f32, f32, f32)) -> Rect4 {
    let mut pts = [
        tiny_skia::Point::from_xy(r.0, r.1),
        tiny_skia::Point::from_xy(r.0 + r.2, r.1),
        tiny_skia::Point::from_xy(r.0, r.1 + r.3),
        tiny_skia::Point::from_xy(r.0 + r.2, r.1 + r.3),
    ];
    ts.map_points(&mut pts);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    (x0, y0, x1 - x0, y1 - y0)
}

/// Conservative paint bounds in the recording's space, independent of the
/// window. Text uses the same shaped glyphs and placement as the painter.
pub(super) fn blend_bounds(
    painter: &mut Painter,
    items: &[Item],
    ts: Transform,
    origin: Transform,
) -> Option<Rect4> {
    use exact_kernel::svg::scene::Kind;
    let mut area: Option<Rect4> = None;
    for item in items {
        let own = item
            .transform
            .as_ref()
            .map_or(ts, |t| ts.pre_concat(super::affine(t.affine())));
        let mut rect = if let Some(filter) = &item.filter {
            bounds(own, filter.region)
        } else if let Some(mask) = &item.mask {
            bounds(own, mask.region)
        } else {
            match &item.kind {
                Kind::Group(children) => blend_bounds(painter, children, own, origin)?,
                Kind::Viewport {
                    rect,
                    view,
                    clip,
                    children,
                } => {
                    let Some(view) = view else { continue };
                    if *clip {
                        bounds(own, *rect)
                    } else {
                        blend_bounds(
                            painter,
                            children,
                            own.pre_concat(super::affine(*view)),
                            origin,
                        )?
                    }
                }
                Kind::Shape(shape) => {
                    let mut rect = shape.path.bounds().unwrap_or_default();
                    let mut space = own;
                    if shape.stroke.is_some() {
                        if shape.non_scaling {
                            rect = bounds(super::affine(item.ctm), rect);
                            space = origin;
                        }
                        // Square caps can reach sqrt(2) half-widths from an
                        // endpoint, independently of the join's miter limit.
                        let pad = shape.width * shape.miter.max(std::f32::consts::SQRT_2) / 2.0;
                        rect = (
                            rect.0 - pad,
                            rect.1 - pad,
                            rect.2 + 2.0 * pad,
                            rect.3 + 2.0 * pad,
                        );
                    }
                    let mut rect = bounds(space, rect);
                    if !shape.markers.is_empty() {
                        rect = union(rect, blend_bounds(painter, &shape.markers, own, origin)?);
                    }
                    rect
                }
                Kind::Text(text) => bounds(own, painter.svg_text_bounds(text)?),
            }
        };
        if let Some(clip) = &item.clip {
            rect = intersect(rect, clip_bounds(clip, own));
        }
        if rect.2 > 0. && rect.3 > 0. {
            area = Some(area.map_or(rect, |a| union(a, rect)));
        }
    }
    Some(area.unwrap_or_default())
}

fn intersect(a: Rect4, b: Rect4) -> Rect4 {
    let x = a.0.max(b.0);
    let y = a.1.max(b.1);
    (
        x,
        y,
        ((a.0 + a.2).min(b.0 + b.2) - x).max(0.),
        ((a.1 + a.3).min(b.1 + b.3) - y).max(0.),
    )
}

fn clip_bounds(clip: &exact_kernel::svg::scene::Clip, ts: Transform) -> Rect4 {
    let area = clip
        .shapes
        .iter()
        .filter_map(|s| s.path.bounds())
        .map(|r| bounds(ts, r))
        .reduce(union)
        .unwrap_or_default();
    clip.then
        .as_ref()
        .map_or(area, |next| intersect(area, clip_bounds(next, ts)))
}

pub(super) fn union(a: Rect4, b: Rect4) -> Rect4 {
    let x = a.0.min(b.0);
    let y = a.1.min(b.1);
    (
        x,
        y,
        (a.0 + a.2).max(b.0 + b.2) - x,
        (a.1 + a.3).max(b.1 + b.3) - y,
    )
}

impl Painter {
    /// Paint into a transparent island covering `rect` (points in the
    /// frame's space, whole device pixels), `paint` given the shift that
    /// maps frame points to island points. `None` when it has no area.
    pub(in crate::paint) fn island(
        &mut self,
        rect: Rect4,
        paint: impl FnOnce(&mut Painter, Transform),
    ) -> Option<Pixmap> {
        let scale = self.scale;
        let mut child = Painter::new(
            self.text.clone(),
            scale,
            Box::new(crate::raster::Raster::transparent()),
        );
        child.dark = self.dark;
        child.symbols = self.symbols;
        child.viewport = (rect.2, rect.3);
        child.backend.begin(rect.2, rect.3, scale);
        paint(&mut child, Transform::from_translate(-rect.0, -rect.1));
        child.backend.finish().ok()
    }

    /// `rect` snapped outward to device pixels and limited to the frame.
    fn island_rect(&self, rect: Rect4) -> Option<Rect4> {
        let s = self.scale;
        let (vw, vh) = self.viewport;
        let x0 = (rect.0 * s).floor().max(0.0);
        let y0 = (rect.1 * s).floor().max(0.0);
        let x1 = ((rect.0 + rect.2) * s).ceil().min(vw * s);
        let y1 = ((rect.1 + rect.3) * s).ceil().min(vh * s);
        // A mask inside a blend picture may be wide but shallow. Keep the
        // same pixel budget without rejecting a side the parent already holds.
        (x1 > x0 && y1 > y0 && (x1 - x0) * (y1 - y0) <= MAX_SIDE * MAX_SIDE)
            .then(|| (x0 / s, y0 / s, (x1 - x0) / s, (y1 - y0) / s))
    }

    /// A masked item (LLP 1055.000 D10): the element and the mask's content
    /// each rendered to an island over the mask region, the content turned
    /// into coverage by luminance or alpha, the element scaled by it.
    pub(super) fn svg_masked(
        &mut self,
        item: &Item,
        mask: &Mask,
        own: Transform,
        origin: Transform,
    ) {
        let Some(rect) = self.island_rect(bounds(own, mask.region)) else {
            return;
        };
        let Some(mut element) = self.island(rect, |p, shift| {
            p.svg_effects(item, shift.pre_concat(own), shift.pre_concat(origin));
        }) else {
            return;
        };
        let Some(mut content) = self.island(rect, |p, shift| {
            let at = shift.pre_concat(own);
            p.backend.push_clip(&Shape::rect(mask.region), at);
            for m in &mask.items {
                p.svg_item(m, at, shift.pre_concat(origin));
            }
            p.backend.pop_clip();
        }) else {
            return;
        };
        exact_svg_raster::mask_coverage(content.data_mut(), mask.luminance);
        exact_svg_raster::apply_coverage(element.data_mut(), content.data(), 4);
        self.backend.surface_image(Arc::new(element), rect);
    }

    /// A filtered item (LLP 1055.000 D14): what it draws, rendered into an
    /// island over the filter region in its own user space at the scale it
    /// shows at, run through the chain, and drawn back in that space.
    pub(super) fn svg_filtered(
        &mut self,
        item: &Item,
        filter: &Filter,
        own: Transform,
        origin: Transform,
    ) {
        let (mut rx, mut ry, mut rw, mut rh) = filter.region;
        if filter.primitives.is_empty() || !(rw > 0.0 && rh > 0.0) {
            return;
        }
        // Unrotated, the island snaps outward to device pixels, as Chrome
        // lays a filter's pixels on the device's.
        if own.kx == 0.0 && own.ky == 0.0 && own.sx > 0.0 && own.sy > 0.0 {
            let (s, dx, dy) = (self.scale, own.tx * self.scale, own.ty * self.scale);
            let (ax, ay) = (own.sx * s, own.sy * s);
            let x0 = (rx * ax + dx).floor();
            let y0 = (ry * ay + dy).floor();
            let x1 = ((rx + rw) * ax + dx).ceil();
            let y1 = ((ry + rh) * ay + dy).ceil();
            (rx, ry, rw, rh) = (
                (x0 - dx) / ax,
                (y0 - dy) / ay,
                (x1 - x0) / ax,
                (y1 - y0) / ay,
            );
        }
        let det = (own.sx * own.sy - own.kx * own.ky).abs().sqrt();
        let k = det * self.scale;
        if !(k.is_finite() && k > 0.0) {
            return;
        }
        let pw = (rw * k).round().clamp(1.0, MAX_SIDE);
        let ph = (rh * k).round().clamp(1.0, MAX_SIDE);
        let scale = self.scale;
        let (sx, sy) = (pw / rw, ph / rh);
        let Some(mut pixels) = self.island((0.0, 0.0, pw / scale, ph / scale), |p, _| {
            // User space to island points: the region's corner at 0.
            let at = Transform::from_scale(sx / scale, sy / scale).pre_translate(-rx, -ry);
            // A non-scaling stroke draws in `origin`'s space: carry the
            // same mapping from the element's user space.
            let inv = own.invert().unwrap_or_default();
            p.svg_kind(item, at, at.pre_concat(inv).pre_concat(origin));
        }) else {
            return;
        };
        let (w, h) = (pixels.width() as usize, pixels.height() as usize);
        exact_svg_raster::filter::run(
            filter,
            pixels.data_mut(),
            w,
            h,
            exact_svg_raster::filter::Space {
                origin: (rx, ry),
                scale: (w as f32 / rw, h as f32 / rh),
            },
        );
        self.backend
            .island_image(Arc::new(pixels), (rx, ry, rw, rh), own, 0);
    }

    /// What an item draws, through its filter when it has one.
    pub(super) fn svg_effects(&mut self, item: &Item, own: Transform, origin: Transform) {
        match &item.filter {
            Some(f) => self.svg_filtered(item, f, own, origin),
            None => self.svg_kind(item, own, origin),
        }
    }

    /// A pattern paint as an ink (LLP 1055.000 D7): its tile rendered at the
    /// scale it shows at in `space` (points), repeated from the tile's
    /// origin in pattern space.
    pub(super) fn pattern_ink(
        &mut self,
        p: &ShapePaint,
        space: Transform,
        to_path: [f32; 6],
    ) -> Option<super::Ink<'static>> {
        let pat = p.pattern.as_ref()?;
        let (tx, ty, tw, th) = pat.tile;
        // Pattern space to the path's space, then to frame points.
        let m = tf::mul(to_path, pat.transform);
        let d = space.pre_concat(Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]));
        let k = (d.sx * d.sy - d.kx * d.ky).abs().sqrt() * self.scale;
        if !(k.is_finite() && k > 0.0) {
            return None;
        }
        let pw = (tw * k).ceil().clamp(1.0, MAX_SIDE);
        let ph = (th * k).ceil().clamp(1.0, MAX_SIDE);
        let scale = self.scale;
        let tile = self.island((0.0, 0.0, pw / scale, ph / scale), |child, _| {
            let ts =
                Transform::from_scale(pw / scale / tw, ph / scale / th).pre_translate(-tx, -ty);
            for item in &pat.items {
                child.svg_item(item, ts, ts);
            }
        })?;
        let (w, h) = (tile.width() as f32, tile.height() as f32);
        let pixel = [tw / w, 0.0, 0.0, th / h, tx, ty];
        Some(super::Ink::Pattern {
            tile: Arc::new(tile),
            transform: tf::mul(m, pixel),
            opacity: p.opacity,
        })
    }
}
