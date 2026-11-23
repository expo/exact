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

/// `frame` with each edge at the device pixel nearest its place in the page,
/// at `scale` pixels a point, as a browser paints a box (Chromium's
/// `PhysicalRect::PixelSnappedIntRect`): two boxes that meet in layout share
/// the edge, a box's size is the distance between its rounded edges, and no
/// rounding adds up down the tree. An edge is taken to Chromium's layout unit
/// first, 1/64 of a device pixel, so a half that float sums left a hair short
/// is a half, then rounded a half up. (The Apple host takes 1/64 of a point,
/// a whole number of these at its whole scales; at 2.625 that step would put
/// the two edges of a box one pixel tall on one pixel.) A box layout gave a
/// size keeps a pixel of it (`SnapSizeToPixel`).
fn on_pixels(frame: Frame, scale: f32) -> Frame {
    let k = f64::from(scale);
    let pixel = |v: f32| f64::from(pixel(v, scale));
    let size = |from: f64, to: f64, laid: f32| match to - from {
        d if d == 0.0 && f64::from(laid) * k > 1.0 / 16.0 => 1.0,
        d => d,
    };
    let (x0, y0) = (pixel(frame.x), pixel(frame.y));
    let (x1, y1) = (pixel(frame.x + frame.width), pixel(frame.y + frame.height));
    Frame {
        x: (x0 / k) as f32,
        y: (y0 / k) as f32,
        width: (size(x0, x1, frame.width) / k) as f32,
        height: (size(y0, y1, frame.height) / k) as f32,
    }
}

/// The device pixel `v` points round to at `scale` pixels a point, by
/// [`on_pixels`]' rule: a scroll offset is rounded as an edge is, so one an
/// app left on a half pixel is the same pixel after every move of whole
/// pixels on from it, whatever a float sum left of the half.
pub(crate) fn pixel(v: f32, scale: f32) -> f32 {
    ((f64::from(v) * f64::from(scale) * 64.0).round() / 64.0 + 0.5).floor() as f32
}

/// `c`, a sticky box's constraint in the space of its scroller at `origin`,
/// with every edge where it is drawn: the box, its limit, the scroller's port
/// and the lines the box sticks to. A scroll of whole pixels then moves the
/// box by whole pixels, and a stuck one stays on its pixel, flush with its
/// scroller's drawn edge. Rounding the moved box's place instead, scroll and
/// all (the Apple host's `PixelGrid.shift`), would not at a fractional scale:
/// a pixel of scroll is no whole number of layout units there.
fn sticky_on_pixels(mut c: StickyConstraint, origin: Frame, scale: f32) -> StickyConstraint {
    let o = on_pixels(origin, scale);
    let grid = |r: [f32; 4]| {
        let rect = Frame {
            x: origin.x + r[0],
            y: origin.y + r[1],
            width: r[2] - r[0],
            height: r[3] - r[1],
        };
        let f = on_pixels(rect, scale);
        let (x, y) = (f.x - o.x, f.y - o.y);
        [x, y, x + f.width, y + f.height]
    };
    let [top, right, bottom, left] = c.insets.map(|i| i.unwrap_or(0.0));
    let p = c.port;
    let lines = grid([p[0] + left, p[1] + top, p[2] - right, p[3] - bottom]);
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
/// scrolled to `scroll`; on a display's pixels (`grid`, its scale) a whole
/// number of them ([`sticky_on_pixels`]).
pub(super) fn sticky_shift(
    c: StickyConstraint,
    scroller: &NodeRef<'_>,
    scroll: (f32, f32),
    grid: Option<f32>,
) -> (f32, f32) {
    match grid {
        Some(s) => sticky_on_pixels(c, scroller.frame, s)
            .offset((pixel(scroll.0, s) / s, pixel(scroll.1, s) / s)),
        None => c.offset(scroll),
    }
}

impl BoxPaint {
    /// The content box's width in a border box `w` wide.
    pub(super) fn content_width(&self, w: f32) -> f32 {
        (w - self.widths[3] - self.widths[1] - self.padding[3] - self.padding[1]).max(0.0)
    }
}

impl Painter {
    /// The display's scale, when this painter draws on its pixels.
    pub(super) fn grid(&self) -> Option<f32> {
        self.pixel_grid.then_some(self.scale)
    }

    /// `node`'s frame as this painter draws it: on a display's pixels each
    /// edge on a whole one ([`on_pixels`]), else as layout made it.
    pub(super) fn drawn(&self, node: &NodeRef<'_>) -> Frame {
        match self.grid() {
            Some(scale) => on_pixels(node.frame, scale),
            None => node.frame,
        }
    }

    /// A scroll offset as this painter draws it: a whole number of a
    /// display's pixels ([`pixel`]), so what a box's rows are moved by
    /// leaves them on the pixels their frames are on.
    pub(super) fn drawn_offset(&self, (x, y): (f32, f32)) -> (f32, f32) {
        match self.grid() {
            Some(s) => (pixel(x, s) / s, pixel(y, s) / s),
            None => (x, y),
        }
    }
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
}
