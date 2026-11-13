//! An SVG shape's fill and stroke as the reader draws them: `PATH` and
//! `STROKE` ops, a `DASH` before a dashed stroke (`Backend::svg_path`).
use super::{path_bounds, Recorder, DASH, PATH, STROKE};
use crate::paint::border::PathOp;
use crate::paint::SvgPaint;
use tiny_skia::Transform;

impl Recorder {
    pub(super) fn svg_shape(&mut self, s: &SvgPaint<'_>, ts: Transform) {
        let ops: Vec<PathOp> = s
            .path
            .0
            .iter()
            .map(|seg| match *seg {
                exact_kernel::svg::Seg::Move(x, y) => PathOp::Move(x, y),
                exact_kernel::svg::Seg::Line(x, y) => PathOp::Line(x, y),
                exact_kernel::svg::Seg::Cubic(a, b, c, d, x, y) => PathOp::Cubic(a, b, c, d, x, y),
                exact_kernel::svg::Seg::Close => PathOp::Close,
            })
            .collect();
        let grow = if s.stroke.is_some() { s.width } else { 0.0 };
        self.need(path_bounds(&ops).and_then(|(x, y, w, h)| {
            self.on_page((x - grow, y - grow, w + 2.0 * grow, h + 2.0 * grow), ts)
        }));
        self.transform(ts);
        // In `paint-order` (SVG 2 §13.8); the markers are the walk's own.
        for part in s.order {
            match part {
                0 => self.svg_fill(s, &ops),
                1 => self.svg_stroke(s, &ops),
                _ => {}
            }
        }
    }

    fn svg_fill(&mut self, s: &SvgPaint<'_>, ops: &[PathOp]) {
        if let Some(c) = s.fill.as_ref().and_then(|ink| self.ink(ink)) {
            self.ops.extend([PATH, c, u32::from(s.even_odd)]);
            self.path(ops);
        }
    }

    fn svg_stroke(&mut self, s: &SvgPaint<'_>, ops: &[PathOp]) {
        // A gradient's SHADER goes before the DASH, which the reader keeps
        // as it does a shader until the STROKE.
        let Some(c) = s.stroke.as_ref().and_then(|ink| self.ink(ink)) else {
            return;
        };
        if !s.dash.is_empty() {
            self.ops.push(DASH);
            // A layer playing the dash offset finds its phase here.
            self.layers.note_dash(self.ops.len());
            self.f(s.phase);
            self.ops.push(s.dash.len() as u32);
            for d in &s.dash {
                self.f(*d);
            }
        }
        self.ops.extend([STROKE, c]);
        self.f(s.width);
        self.ops.extend([u32::from(s.cap), u32::from(s.join)]);
        self.f(s.miter);
        self.path(ops);
    }
}

#[cfg(test)]
mod tests {
    use super::super::stream::{drawing, end};
    use super::*;
    use crate::paint::{Backend, Ink};
    use exact_kernel::svg::{Path, Seg};

    /// The ops of a recording, by where each starts.
    fn ops(r: &Recorder) -> Vec<(usize, u32)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < r.ops.len() {
            out.push((i, r.ops[i]));
            i = end(&r.ops, i).expect("a walkable stream");
        }
        out
    }

    /// The issue's triangle, filled red and stroked blue, 8 wide.
    fn triangle(r: &mut Recorder, order: [u8; 3], miter: f32) {
        let path = Path(vec![
            Seg::Move(0., 0.),
            Seg::Line(40., 0.),
            Seg::Line(40., 40.),
            Seg::Close,
        ]);
        let paint = SvgPaint {
            path: &path,
            fill: Some(Ink::Solid([255, 0, 0, 255])),
            even_odd: false,
            stroke: Some(Ink::Solid([0, 0, 255, 255])),
            order,
            width: 8.,
            cap: 0,
            join: 0,
            miter,
            dash: vec![],
            phase: 0.,
        };
        r.svg_path(&paint, Transform::identity());
    }

    fn recorder() -> Recorder {
        let mut r = Recorder::new();
        r.begin(60., 60., 1.);
        r.ops.clear();
        r
    }

    /// `paint-order: stroke` paints the stroke first, so the fill covers
    /// its inner half; `normal` paints the fill first.
    #[test]
    fn paint_order_orders_the_fill_and_the_stroke() {
        for (order, painted) in [([0, 1, 2], [PATH, STROKE]), ([1, 0, 2], [STROKE, PATH])] {
            let mut r = recorder();
            triangle(&mut r, order, 4.);
            let drawn: Vec<u32> = ops(&r)
                .into_iter()
                .map(|(_, op)| op)
                .filter(|op| *op != super::super::MATRIX)
                .collect();
            assert_eq!(drawn, painted, "{order:?}");
        }
    }

    /// `stroke-miterlimit` reaches the reader in its STROKE op, after the
    /// join, and the stream still walks.
    #[test]
    fn a_stroke_carries_its_miter_limit() {
        let mut r = recorder();
        triangle(&mut r, [0, 1, 2], 10.);
        let (at, _) = ops(&r).into_iter().find(|(_, op)| *op == STROKE).unwrap();
        assert_eq!(f32::from_bits(r.ops[at + 2]), 8.);
        assert_eq!(r.ops[at + 3..at + 5], [0, 0], "butt, miter");
        assert_eq!(f32::from_bits(r.ops[at + 5]), 10.);
        assert_eq!(r.ops[at + 6], 4, "then the path's four segments");
        assert!(drawing(&r.ops).is_some());
    }
}
