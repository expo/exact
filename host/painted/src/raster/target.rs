//! A covering clear is resolved only when its pixels are read or drawn into.
//! Keeping the storage private makes layer transfers resolve it too.
use crate::paint::WindowLayer;
use std::ops::Range;
use tiny_skia::{Color, Pixmap, Point, Rect, Transform};

#[derive(Default)]
pub(super) struct Target {
    pixels: Option<Pixmap>,
    clear: Option<Color>,
    rows: Range<u32>,
    #[cfg(test)]
    clears: usize,
}

impl Target {
    /// Reused overlays are transparent outside their last reported ink rows.
    pub(super) fn overlay(mut pixels: Pixmap, rows: Range<u32>) -> Self {
        let rows = if rows.start <= rows.end && rows.end <= pixels.height() {
            rows
        } else {
            0..pixels.height()
        };
        let stride = pixels.width() as usize * 4;
        pixels.data_mut()[rows.start as usize * stride..rows.end as usize * stride].fill(0);
        Self {
            pixels: Some(pixels),
            ..Self::default()
        }
    }

    pub(super) fn painted(pixels: Pixmap) -> Self {
        Self {
            rows: 0..pixels.height(),
            pixels: Some(pixels),
            ..Self::default()
        }
    }

    pub(super) fn cleared(pixels: Option<Pixmap>, color: Color) -> Self {
        Self {
            rows: 0..if color.alpha() == 0. {
                0
            } else {
                pixels.as_ref().map_or(0, Pixmap::height)
            },
            pixels,
            clear: Some(color),
            #[cfg(test)]
            clears: 0,
        }
    }

    pub(super) fn clear(&mut self, color: Color) {
        self.clear = Some(color);
        self.rows = 0..if color.alpha() == 0. {
            0
        } else {
            self.pixels.as_ref().map_or(0, Pixmap::height)
        };
    }

    fn resolve(&mut self) {
        if let (Some(color), Some(pixels)) = (self.clear.take(), self.pixels.as_mut()) {
            pixels.fill(color);
            #[cfg(test)]
            {
                self.clears += 1;
            }
        }
    }

    pub(super) fn rows(&self) -> Range<u32> {
        self.rows.clone()
    }

    fn include(&mut self, rows: Range<u32>) {
        if rows.is_empty() {
            return;
        }
        self.rows = if self.rows.is_empty() {
            rows
        } else {
            self.rows.start.min(rows.start)..self.rows.end.max(rows.end)
        };
    }

    pub(super) fn pixels(&mut self) -> Option<&mut Pixmap> {
        self.resolve();
        self.rows = 0..self.pixels.as_ref()?.height();
        self.pixels.as_mut()
    }

    pub(super) fn svg_pixels(&mut self, bounds: Rect, dev: Transform) -> Option<&mut Pixmap> {
        if !super::reach::within(bounds, dev) {
            return None;
        }
        self.bounded_pixels(bounds, dev)
    }

    pub(super) fn bounded_pixels(&mut self, bounds: Rect, dev: Transform) -> Option<&mut Pixmap> {
        let p = self.pixels.as_ref()?;
        let mut corners = [
            Point::from_xy(bounds.left(), bounds.top()),
            Point::from_xy(bounds.right(), bounds.bottom()),
        ];
        dev.map_points(&mut corners);
        if p.width() > 8191
            || p.height() > 8191
            || !dev.is_finite()
            || dev.kx != 0.
            || dev.ky != 0.
            || dev.sx <= 0.
            || dev.sy <= 0.
            || corners.iter().any(|p| {
                !p.x.is_finite() || !p.y.is_finite() || p.x.abs() > 8191. || p.y.abs() > 8191.
            })
        {
            return self.pixels();
        }
        let height = p.height() as f32;
        let rows = (corners[0].y.floor() - 2.).clamp(0., height) as u32
            ..(corners[1].y.ceil() + 2.).clamp(0., height) as u32;
        self.resolve();
        self.include(rows);
        self.pixels.as_mut()
    }

    /// The closure reports every device row its drawing could write.
    pub(super) fn draw_rows(&mut self, draw: impl FnOnce(&mut Pixmap) -> Range<u32>) {
        self.resolve();
        if let Some(pixels) = self.pixels.as_mut() {
            let rows = draw(pixels);
            self.include(rows);
        }
    }

    pub(super) fn take(&mut self) -> Option<Pixmap> {
        self.resolve();
        self.pixels.take()
    }

    pub(super) fn take_layer(&mut self) -> Option<WindowLayer> {
        let pixels = self.pixels.take()?;
        Some(match self.clear.take() {
            Some(color) => {
                let c = color.premultiply().to_color_u8();
                WindowLayer::Solid {
                    color: [c.red(), c.green(), c.blue(), c.alpha()],
                    storage: pixels,
                }
            }
            None => WindowLayer::Pixels(pixels),
        })
    }

    /// The replacement has been allocated transparent and has no ink yet.
    pub(super) fn replace_empty(&mut self, pixels: Pixmap) -> Option<Pixmap> {
        let old = self.take();
        self.rows = 0..0;
        self.pixels = Some(pixels);
        old
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn opacity_ink_rows_match_full_compositing_for_bounds_and_fallbacks() {
        for scale in [1., 1.25, 1.5, 2.] {
            for case in 0..10 {
                let mut r = Raster::new();
                r.page([31, 113, 207, 127]);
                r.begin(120., 160., scale);
                r.push_opacity(0.37);
                let identity = Transform::identity();
                let shape = Shape::new((12.25, 51.5, 52.75, 27.125), [7.5; 4]);
                match case {
                    0 => {}
                    1 => r.fill(&shape, [157, 43, 201, 83], identity),
                    2 | 3 => {
                        r.push_clip(&Shape::rect((12., 52., 48., 24.)), identity);
                        let fill = if case == 2 {
                            Shape::rect((0., 0., 120., 160.))
                        } else {
                            Shape::new((0., 0., 120., 160.), [4.; 4])
                        };
                        r.fill(&fill, [157, 43, 201, 255], identity);
                        r.pop_clip();
                    }
                    4 | 5 => {
                        let path = exact_kernel::svg::parse_d("M12 52L64 52L38 78Z");
                        r.svg_path(
                            &crate::paint::SvgPaint {
                                path: &path,
                                fill: Some(crate::paint::Ink::Solid([157, 43, 201, 83])),
                                stroke: (case == 5)
                                    .then_some(crate::paint::Ink::Solid([217, 151, 32, 255])),
                                even_odd: false,
                                order: [0, 1, 2],
                                width: 13.,
                                cap: 2,
                                join: 0,
                                miter: 8.,
                                dash: vec![],
                                phase: 0.,
                            },
                            identity,
                        );
                    }
                    6 => {
                        r.push_opacity(0.61);
                        r.fill(&shape, [157, 43, 201, 83], identity);
                        r.pop_opacity();
                    }
                    7 => r.fill(
                        &Shape::rect((0., 0., 120., 160.)),
                        [157, 43, 201, 255],
                        identity,
                    ),
                    8 => r.fill(&shape, [157, 43, 201, 83], Transform::from_rotate(17.)),
                    _ => r.pointer(30., 60.),
                }
                let rows = r.target.rows();
                if case <= 4 {
                    assert!(rows.end - rows.start < r.height / 2);
                }
                r.target.resolve();
                let layer = r.target.pixels.as_ref().unwrap();
                for (y, bytes) in layer
                    .data()
                    .chunks_exact(layer.width() as usize * 4)
                    .enumerate()
                {
                    if !rows.contains(&(y as u32)) {
                        assert!(
                            bytes.iter().all(|&byte| byte == 0),
                            "scale={scale} case={case} row={y}"
                        );
                    }
                }
                let mut expected = r.layers.last().unwrap().0.clone();
                expected.draw_pixmap(
                    0,
                    0,
                    layer.as_ref(),
                    &PixmapPaint {
                        opacity: 0.37,
                        ..Default::default()
                    },
                    identity,
                    None,
                );
                r.pop_opacity();
                assert_eq!(
                    r.finish().unwrap().data(),
                    expected.data(),
                    "scale={scale} case={case}"
                );
            }
        }
    }

    #[test]
    fn svg_stroke_rows_cover_caps_joins_and_hairlines() {
        for (d, width, cap, join, miter) in [
            ("M2 8H14", 2., 0, 0, 4.),
            ("M8 0L24 16", 8., 2, 2, 1.),
            ("M0 36L16 0L32 36", 10., 0, 0, 8.),
            ("M0 8C6 -8 18 24 32 8", 0.25, 1, 1, 1.),
        ] {
            let path = exact_kernel::svg::parse_d(d);
            for dev in [
                Transform::from_row(1.25, 0., 0., 1.5, 20.5, 120.25),
                Transform::from_row(1.25, 0.125, 0., 1.5, 20.5, 120.25),
            ] {
                let mut r = Raster::transparent();
                r.begin(96., 384., 1.);
                r.svg_path(
                    &crate::paint::SvgPaint {
                        path: &path,
                        fill: None,
                        stroke: Some(crate::paint::Ink::Solid([32, 71, 159, 255])),
                        even_odd: false,
                        order: [1, 0, 2],
                        width,
                        cap,
                        join,
                        miter,
                        dash: if cap == 2 { vec![8., 3.] } else { vec![] },
                        phase: 0.,
                    },
                    dev,
                );
                let rows = r.target.rows();
                if dev.ky == 0. {
                    assert!(rows.len() < r.height as usize / 2, "{d}: {rows:?}");
                } else {
                    assert_eq!(rows, 0..r.height);
                }
                let pixels = r.finish().unwrap();
                let mut ink = pixels
                    .data()
                    .chunks_exact(96 * 4)
                    .enumerate()
                    .filter(|(_, row)| row.iter().any(|&v| v != 0))
                    .map(|(y, _)| y as u32);
                assert!(rows.contains(&ink.next().expect("stroke must paint")));
                assert!(ink.all(|y| rows.contains(&y)), "{d}: {rows:?}");
            }
        }
    }

    #[test]
    fn recycled_frames_keep_storage_but_clear_old_ink_and_resize() {
        let mut raster = Raster::new();
        raster.begin(40., 24., 1.);
        raster.fill(
            &Shape::rect((4., 2., 20., 16.)),
            [210, 20, 40, 255],
            Transform::identity(),
        );
        let first = raster.finish().unwrap();
        let address = first.data().as_ptr();
        raster.recycle_frame(first);
        raster.page([30, 80, 120, 255]);
        raster.begin(40., 24., 1.);
        let second = raster.finish().unwrap();
        assert_eq!(second.data().as_ptr(), address);
        assert!(second
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [30, 80, 120, 255]));
        raster.recycle_frame(second);
        raster.begin(60., 32., 1.5);
        let resized = raster.finish().unwrap();
        assert_eq!((resized.width(), resized.height()), (90, 48));
        assert!(resized
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [30, 80, 120, 255]));
        raster.recycle_frame(resized);
        raster.transparent = true;
        raster.begin(60., 32., 1.5);
        assert!(raster.finish().unwrap().data().iter().all(|&v| v == 0));
    }

    #[test]
    fn overlay_rows_cover_ink_and_reset_for_reuse_and_unbounded_draws() {
        for scale in [1., 1.25, 1.5, 2.] {
            for y in [-10.25, 16.75, 210.] {
                for rounded in [false, true] {
                    let mut r = Raster::new();
                    r.begin(420., 240., scale);
                    assert!(r.split_window_canvas(
                        (0., 0., 420., 240.),
                        &[],
                        Transform::identity()
                    ));
                    let mut shape = Shape::rect((4., y, 408., 40.));
                    if rounded {
                        shape.radii = [(8., 8.); 4];
                    }
                    r.fill(&shape, [29, 71, 183, 83], Transform::identity());
                    let (start, end) = r.window_overlay_rows().unwrap();
                    assert!(end - start < r.height / 2);
                    let pixels = r.finish().unwrap();
                    assert_eq!(r.window_overlay_rows(), Some((start, end)));
                    for (row, bytes) in pixels
                        .data()
                        .chunks_exact(pixels.width() as usize * 4)
                        .enumerate()
                    {
                        if !(start..end).contains(&(row as u32)) {
                            assert!(
                                bytes.iter().all(|&v| v == 0),
                                "scale={scale} y={y} row={row}"
                            );
                        }
                    }
                    let below = r.take_window_underlay().unwrap().into_pixels();
                    r.recycle_window_layers(below, Some(pixels), Some((start, end)));
                    r.begin(420., 240., scale);
                    assert!(r.split_window_canvas(
                        (0., 0., 420., 240.),
                        &[],
                        Transform::identity()
                    ));
                    assert_eq!(r.window_overlay_rows(), Some((0, 0)));
                    assert!(r
                        .target
                        .pixels
                        .as_ref()
                        .unwrap()
                        .data()
                        .iter()
                        .all(|&v| v == 0));
                    r.pointer(20., 80.);
                    assert_eq!(r.window_overlay_rows(), Some((0, r.height)));
                }
            }
        }
    }

    #[test]
    fn covering_fills_replace_clears_but_reads_and_transfers_resolve_them() {
        let full = Shape::rect((0., 0., 40., 24.));
        let identity = Transform::identity();
        let a = [31, 72, 109, 255];
        let b = [210, 40, 60, 255];
        let mut r = Raster::new();
        r.begin(40., 24., 1.);
        r.fill(&full, a, identity);
        r.fill(&full, b, identity);
        assert_eq!(r.target.clears, 0);
        let pixels = r.finish().unwrap();
        assert!(pixels.data().as_chunks::<4>().0.iter().all(|p| *p == b));
        assert_eq!(r.target.clears, 1);

        // Resolve the parent's fill before an opacity layer starts.
        r.begin(40., 24., 1.);
        r.fill(&full, a, identity);
        r.push_opacity(0.5);
        r.fill(&full, b, identity);
        r.pop_opacity();
        let mut expected = Pixmap::new(40, 24).unwrap();
        expected.fill(Color::from_rgba8(a[0], a[1], a[2], a[3]));
        let mut layer = Pixmap::new(40, 24).unwrap();
        layer.fill(Color::from_rgba8(b[0], b[1], b[2], b[3]));
        expected.draw_pixmap(
            0,
            0,
            layer.as_ref(),
            &PixmapPaint {
                opacity: 0.5,
                ..Default::default()
            },
            identity,
            None,
        );
        assert_eq!(r.finish().unwrap().data(), expected.data());

        // A partial or translucent draw needs the pending background first.
        for (shape, color) in [
            (Shape::rect((3., 5., 12., 8.)), b),
            (full, [210, 40, 60, 83]),
        ] {
            r.begin(40., 24., 1.);
            r.fill(&full, a, identity);
            r.fill(&shape, color, identity);
            expected.fill(Color::from_rgba8(a[0], a[1], a[2], a[3]));
            expected.fill_path(
                &rounded_rect(&shape).unwrap(),
                &solid(color),
                FillRule::Winding,
                identity,
                None,
            );
            assert_eq!(r.finish().unwrap().data(), expected.data());
        }
    }

    #[test]
    fn reused_split_layers_clear_before_drawing_and_after_resize() {
        let full = Shape::rect((0., 0., 40., 24.));
        let identity = Transform::identity();
        let mut r = Raster::new();
        for size in [(40., 24.), (40., 24.), (20., 12.)] {
            r.begin(size.0, size.1, 1.);
            let rect = (0., 0., size.0, size.1);
            r.fill(&Shape::rect(rect), [31, 72, 109, 255], identity);
            assert!(r.split_window_canvas(rect, &[], identity));
            assert_eq!(r.target.clears, 0);
            let layer = r.take_window_underlay().unwrap();
            assert!(matches!(
                &layer,
                WindowLayer::Solid {
                    color: [31, 72, 109, 255],
                    ..
                }
            ));
            let below = layer.into_pixels();
            assert!(below
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [31, 72, 109, 255]));
            let above = r.finish().unwrap();
            assert!(above.data().iter().all(|&v| v == 0));
            let mut dirty_above = above;
            dirty_above.fill(Color::from_rgba8(255, 20, 30, 255));
            r.recycle_window_layers(below, Some(dirty_above), None);
        }
        r.begin(40., 24., 1.);
        r.fill(&full, [1, 2, 3, 255], identity);
        // Beginning again abandons the previous pending clear.
        r.begin(40., 24., 1.);
        assert!(r.finish().unwrap().data().iter().all(|&v| v == 255));
        let mut transparent = Raster::transparent();
        transparent.begin(40., 24., 1.);
        assert!(transparent.finish().unwrap().data().iter().all(|&v| v == 0));
    }

    #[test]
    fn split_background_color_is_premultiplied_and_any_partial_paint_removes_it() {
        for alpha in [0, 83, 255] {
            for patch in [false, true] {
                let mut r = Raster::new();
                r.page([219, 71, 253, alpha]);
                r.begin(40., 24., 1.);
                let mut expected = Pixmap::new(40, 24).unwrap();
                expected.fill(Color::from_rgba8(219, 71, 253, alpha));
                if patch {
                    let shape = Shape::rect((4., 5., 12., 8.));
                    let color = [210, 40, 60, 83];
                    r.fill(&shape, color, Transform::identity());
                    expected.fill_path(
                        &rounded_rect(&shape).unwrap(),
                        &solid(color),
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
                assert!(r.split_window_canvas((0., 0., 40., 24.), &[], Transform::identity()));
                let layer = r.take_window_underlay().unwrap();
                assert_eq!(matches!(layer, WindowLayer::Pixels(_)), patch);
                assert_eq!(layer.into_pixels().data(), expected.data());
            }
        }
    }
}
