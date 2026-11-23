//! Boxes on a display's pixels, for a backend whose drawing is replayed
//! there box by box ([`Backend::pixel_grid`]: the Android reader, which keeps
//! a scroller's rows and draws them again wherever they are placed). Layout's
//! frames are fractional, and a box drawn where it falls has its edges
//! blended across two pixels; kept text drawn at a new fraction of a pixel is
//! one more entry in the renderer's text cache. The Apple host draws its
//! views on the grid too (`PixelGridIOS.swift`, `PixelGridMac.swift`). Only
//! what is drawn is on the grid: everything else reads the frames layout made.

use super::*;
use exact_kernel::{Frame, StickyConstraint};

/// A paint origin in document points. Keep the precision of the display's
/// pixels until the painter subtracts this origin from a box's edges.
pub(super) type PaintOffset = (f64, f64);

pub(super) fn paint_origin((x, y): (f32, f32), offset: PaintOffset) -> (f32, f32) {
    (
        (f64::from(x) - offset.0) as f32,
        (f64::from(y) - offset.1) as f32,
    )
}

pub(super) fn paint_rect(frame: Frame, offset: PaintOffset) -> Rect4 {
    let (x, y) = paint_origin((frame.x, frame.y), offset);
    (x, y, frame.width, frame.height)
}

/// As in Chromium's `PhysicalRect::PixelSnappedIntRect`, adjoining boxes
/// share their rounded edge. A box thinner than a pixel can keep one pixel
/// of its size (`SnapSizeToPixel`).
fn pixel_box(frame: Frame, scale: f32) -> [f64; 4] {
    let pixel = |v| device_pixel(v, scale);
    let size = |from: f64, to: f64, laid: f32| match to - from {
        0.0 if f64::from(laid) * f64::from(scale) > 1.0 / 16.0 => 1.0,
        d => d,
    };
    let (x, y) = (pixel(frame.x), pixel(frame.y));
    [
        x,
        y,
        size(x, pixel(frame.x + frame.width), frame.width),
        size(y, pixel(frame.y + frame.height), frame.height),
    ]
}

fn on_pixels(frame: Frame, scale: f32) -> Frame {
    let [x, y, width, height] = pixel_box(frame, scale).map(|v| (v / f64::from(scale)) as f32);
    Frame {
        x,
        y,
        width,
        height,
    }
}

/// Quantize to 1/64 of a device pixel before rounding half up. This keeps
/// float noise close to a half pixel from choosing a different edge.
pub(crate) fn device_pixel(v: f32, scale: f32) -> f64 {
    round_device_pixel(f64::from(v * scale))
}

fn round_device_pixel(pixels: f64) -> f64 {
    ((pixels * 64.0).round() / 64.0 + 0.5).floor()
}

pub(super) fn page_scroll_grid(scale: Option<f32>) -> Vec<(ViewId, Option<f32>)> {
    scale
        .into_iter()
        .map(|s| (crate::presenter::PAGE, Some(s)))
        .collect()
}

#[cfg(any(target_os = "android", test))]
pub(crate) fn pixel_delta(from: (f32, f32), to: (f32, f32), scale: f32) -> (f32, f32) {
    let axis = |from, to| (device_pixel(from, scale) - device_pixel(to, scale)) as f32;
    (axis(from.0, to.0), axis(from.1, to.1))
}

pub(super) fn scroll_offset((x, y): (f32, f32), grid: Option<f32>) -> PaintOffset {
    match grid {
        Some(scale) => (
            device_pixel(x, scale) / f64::from(scale),
            device_pixel(y, scale) / f64::from(scale),
        ),
        None => (f64::from(x), f64::from(y)),
    }
}

/// `c`, a sticky box's constraint in the space of its scroller at `origin`,
/// with every edge where it is drawn: the box, its limit, the scroller's port
/// and the lines the box sticks to. A scroll of whole pixels then moves the
/// box by whole pixels, and a stuck one stays on its pixel, flush with its
/// scroller's drawn edge. Rounding the moved box's place instead, scroll and
/// all (the Apple host's `PixelGrid.shift`), would not at a fractional scale:
/// a pixel of scroll is no whole number of layout units there.
fn sticky_on_pixels(mut c: StickyConstraint, origin: Frame, scale: f32) -> StickyConstraint {
    let k = f64::from(scale);
    let ox = device_pixel(origin.x, scale);
    let oy = device_pixel(origin.y, scale);
    let edge_x = |x| ((device_pixel(origin.x + x, scale) - ox) / k) as f32;
    let edge_y = |y| ((device_pixel(origin.y + y, scale) - oy) / k) as f32;
    let grid = |r: [f32; 4]| {
        let rect = Frame {
            x: origin.x + r[0],
            y: origin.y + r[1],
            width: r[2] - r[0],
            height: r[3] - r[1],
        };
        let [x, y, w, h] = pixel_box(rect, scale);
        [x - ox, y - oy, x - ox + w, y - oy + h].map(|v| (v / k) as f32)
    };
    let [top, right, bottom, left] = c.insets.map(|i| i.unwrap_or(0.0));
    let p = c.port;
    let lines = [
        edge_x(p[0] + left),
        edge_y(p[1] + top),
        edge_x(p[2] - right),
        edge_y(p[3] - bottom),
    ];
    (c.natural, c.limit, c.port) = (grid(c.natural), grid(c.limit), grid(p));
    let p = c.port;
    let at = [
        lines[1] - p[1],
        p[2] - lines[2],
        p[3] - lines[3],
        lines[0] - p[0],
    ];
    for (inset, at) in c.insets.iter_mut().zip(at) {
        *inset = inset.map(|_| at);
    }
    c
}

/// How far a sticky box moves from where layout put it, in `scroller`
/// scrolled to its drawn offset. Snap the constraint when the sticky box is
/// drawn on the grid.
pub(super) fn sticky_shift(
    c: StickyConstraint,
    scroller: &NodeRef<'_>,
    scroll: (f32, f32),
    grid: Option<f32>,
) -> (f32, f32) {
    grid.map_or(c, |s| sticky_on_pixels(c, scroller.frame, s))
        .offset(scroll)
}

pub(super) fn rect_on_grid(frame: Frame, offset: PaintOffset, grid: Option<f32>) -> Rect4 {
    let Some(scale) = grid else {
        return paint_rect(frame, offset);
    };
    let k = f64::from(scale);
    let [x, y, w, h] = pixel_box(frame, scale);
    (
        ((x - offset.0 * k) / k) as f32,
        ((y - offset.1 * k) / k) as f32,
        (w / k) as f32,
        (h / k) as f32,
    )
}

impl BoxPaint {
    /// The content box's width in a border box `w` wide.
    pub(super) fn content_width(&self, w: f32) -> f32 {
        (w - self.widths[3] - self.widths[1] - self.padding[3] - self.padding[1]).max(0.0)
    }
}

impl Painter {
    #[cfg(test)]
    pub(crate) fn enable_test_pixel_grid(&mut self) {
        self.pixel_grid = true;
    }

    pub(super) fn without_grid<T>(
        &mut self,
        suspend: bool,
        draw: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.grid_suspended;
        self.grid_suspended |= suspend;
        let result = draw(self);
        self.grid_suspended = previous;
        result
    }

    /// The display's scale, when this painter draws on its pixels.
    pub(super) fn grid(&self) -> Option<f32> {
        (self.pixel_grid && !self.grid_suspended).then_some(self.scale)
    }

    /// `node`'s frame as this painter draws it: on a display's pixels each
    /// edge on a whole one ([`on_pixels`]), else as layout made it.
    pub(super) fn drawn(&self, node: &NodeRef<'_>) -> Frame {
        match self.grid() {
            Some(scale) => on_pixels(node.frame, scale),
            None => node.frame,
        }
    }

    pub(super) fn drawn_rect(&self, node: &NodeRef<'_>, offset: PaintOffset) -> Rect4 {
        rect_on_grid(node.frame, offset, self.grid())
    }

    pub(super) fn node_rect(
        &self,
        node: &NodeRef<'_>,
        offset: PaintOffset,
        shown: &Presented,
    ) -> Rect4 {
        if leaves_grid(node, shown) {
            paint_rect(node.frame, offset)
        } else {
            self.drawn_rect(node, offset)
        }
    }

    pub(super) fn pixel_phase(&self, frame: Frame) -> Option<(f64, f64)> {
        let scale = self.grid()?;
        let phase = |v: f32| f64::from(v * scale) - device_pixel(v, scale);
        Some((phase(frame.x), phase(frame.y)))
    }

    /// Row recording origins are on the same grid as boxes.
    pub(super) fn drawn_offset(&self, (x, y): (f32, f32)) -> PaintOffset {
        match self.grid() {
            Some(s) => (
                device_pixel(x, s) / f64::from(s),
                device_pixel(y, s) / f64::from(s),
            ),
            None => (f64::from(x), f64::from(y)),
        }
    }

    pub(super) fn drawn_scroll(&self, scroll: (f32, f32)) -> PaintOffset {
        scroll_offset(scroll, self.grid())
    }
}

pub(super) fn leaves_grid(node: &NodeRef<'_>, shown: &Presented) -> bool {
    shown.changes_axes()
        || shown.rotate_axis.is_3d()
        || node.style.translate_z != 0.
        || zoom::range(node).is_some()
}

pub(super) fn changes_axes(ts: Transform) -> bool {
    ts.sx != 1. || ts.sy != 1. || ts.kx != 0. || ts.ky != 0.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(frame: Frame) -> [f32; 4] {
        let f = on_pixels(frame, 2.625);
        [f.x, f.y, f.width, f.height].map(|v| (v * 2.625).round())
    }

    #[test]
    fn a_half_pixel_a_float_sum_left_short_is_a_half_and_rounds_up() {
        // 4 px is 10.5 pixels at 2.625 a px; a sum that came out a bit under
        // 4 is the same edge.
        let x = |x: f32| {
            pixels(Frame {
                x,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            })[0]
        };
        assert_eq!([x(4.0), x(3.999_999_8), x(-4.0)], [11.0, 11.0, -10.0]);
    }

    #[test]
    fn a_box_a_pixel_tall_is_a_pixel_tall_wherever_it_falls() {
        // A hairline divider box, and one thinner than half a pixel, on
        // every sixteenth of a pixel down a page.
        for i in 0..2000 {
            let y = i as f32 / (16.0 * 2.625);
            let tall = |height: f32| {
                pixels(Frame {
                    x: 0.0,
                    y,
                    width: 10.0,
                    height,
                })[3]
            };
            assert_eq!([tall(1.0 / 2.625), tall(0.15)], [1.0, 1.0], "at {y} px");
        }
    }

    #[test]
    fn a_stuck_box_stays_on_one_pixel_as_its_scroller_scrolls() {
        // `top: 12px` in a scroller 7.1 px down the page: the line is 50.14
        // pixels down at 2.625 a px and the scroller 18.64, drawn 31 apart.
        let c = StickyConstraint {
            scroller: 1,
            natural: [0.0, 0.0, 200.0, 30.0],
            limit: [0.0, 0.0, 200.0, 3000.0],
            port: [0.0, 0.0, 200.0, 200.0],
            insets: [Some(12.0), None, None, None],
        };
        let origin = Frame {
            x: 0.3,
            y: 7.1,
            width: 200.0,
            height: 200.0,
        };
        let c = sticky_on_pixels(c, origin, 2.625);
        for pixels in 0..640 {
            let (_, dy) = c.offset((0.0, pixels as f32 / 2.625));
            let top = (c.natural[1] + dy) * 2.625 - pixels as f32;
            assert!((top - 31.0).abs() < 1e-3, "at {top} after {pixels} pixels");
        }
    }

    #[test]
    fn opposing_sticky_inset_lines_can_share_a_pixel() {
        let origin = Frame {
            x: 0.,
            y: 0.,
            width: 200.,
            height: 200.,
        };
        for (start, end, line) in [(None, 199.9, 0.), (Some(100.), 99.9, 263.)] {
            let c = sticky_on_pixels(
                StickyConstraint {
                    scroller: 1,
                    natural: [0., 0., 20., 20.],
                    limit: [0., 0., 200., 200.],
                    port: [0., 0., 200., 200.],
                    insets: [start, Some(end), Some(end), start],
                },
                origin,
                2.625,
            );
            for v in [
                c.port[2] - c.insets[1].unwrap(),
                c.port[3] - c.insets[2].unwrap(),
            ] {
                assert!(
                    (v * 2.625 - line).abs() < 0.001,
                    "end line at {} px",
                    v * 2.625
                );
            }
        }
    }
}
