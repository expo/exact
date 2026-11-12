//! Clips the recorder emits only when something needs them. A rounded clip in
//! HWUI is a path clip, the expensive kind; Android's own views avoid it by
//! drawing rounded content directly. So a pushed clip stays pending until a
//! drawing inside it reaches outside its rectangle or into a rounded corner;
//! a picture whose corners coincide with the clips' corners is drawn as a
//! rounded rect filled by the picture (`IMAGE_RRECT`), needing no clip at all.
//! Pending clips are materialized outermost first, so the emitted clips are
//! always a prefix of the stack and saves and restores nest.
use super::*;

/// A clip the walk pushed: the shape in its transform, and whether it has
/// been written into the stream after a save.
#[derive(Clone)]
pub(super) struct Clip {
    geometry: Geometry,
    ts: Transform,
    /// Page-space rect and radii, when the transform is a scale and translate.
    page: Option<(Rect4, [(f32, f32); 4])>,
    emitted: bool,
}

#[derive(Clone)]
enum Geometry {
    Rect(Shape),
    Paths(Vec<(bool, Vec<PathOp>)>),
}

/// Independently filled paths unioned before they clip.
pub(super) const CLIP_UNION: u32 = 45;
/// A layer whose alpha masks the isolated content below it.
pub(super) const MASK_LAYER: u32 = 46;

/// `ts` as (sx, sy, tx, ty) when it only scales and translates.
fn axis(ts: Transform) -> Option<(f32, f32, f32, f32)> {
    (ts.kx == 0.0 && ts.ky == 0.0 && ts.sx > 0.0 && ts.sy > 0.0)
        .then_some((ts.sx, ts.sy, ts.tx, ts.ty))
}

/// `r` in page space under `ts`.
pub(super) fn map(r: Rect4, ts: Transform) -> Option<Rect4> {
    let (sx, sy, tx, ty) = axis(ts)?;
    Some((r.0 * sx + tx, r.1 * sy + ty, r.2 * sx, r.3 * sy))
}

fn intersect(a: Rect4, b: Rect4) -> Rect4 {
    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = (a.0 + a.2).min(b.0 + b.2);
    let y1 = (a.1 + a.3).min(b.1 + b.3);
    (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

/// The four corners of `r` (top-left, top-right, bottom-right, bottom-left)
/// and, for each, the square a radius `(rx, ry)` cuts into.
fn corner_square(r: Rect4, k: usize, (rx, ry): (f32, f32)) -> Rect4 {
    match k {
        0 => (r.0, r.1, rx, ry),
        1 => (r.0 + r.2 - rx, r.1, rx, ry),
        2 => (r.0 + r.2 - rx, r.1 + r.3 - ry, rx, ry),
        _ => (r.0, r.1 + r.3 - ry, rx, ry),
    }
}

fn corner(r: Rect4, k: usize) -> (f32, f32) {
    match k {
        0 => (r.0, r.1),
        1 => (r.0 + r.2, r.1),
        2 => (r.0 + r.2, r.1 + r.3),
        _ => (r.0, r.1 + r.3),
    }
}

fn overlaps(a: Rect4, b: Rect4) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

const EPS: f32 = 0.01;

/// Whether drawing within `b` (page space) is unaffected by the clip.
fn unclipped(b: Rect4, (r, radii): (Rect4, [(f32, f32); 4])) -> bool {
    let inside = b.0 >= r.0 - EPS
        && b.1 >= r.1 - EPS
        && b.0 + b.2 <= r.0 + r.2 + EPS
        && b.1 + b.3 <= r.1 + r.3 + EPS;
    inside
        && (0..4).all(|k| {
            radii[k].0 <= 0.0 || radii[k].1 <= 0.0 || !overlaps(b, corner_square(r, k, radii[k]))
        })
}

impl Clip {
    pub(super) fn emitted(&self) -> bool {
        self.emitted
    }

    /// Written and closed again (inside a picture slot): pending as before.
    pub(super) fn reopen(&mut self) {
        self.emitted = false;
    }
}

impl Recorder {
    pub(super) fn native_element(
        &mut self,
        id: ViewId,
        kind: crate::paint::NativeKind,
        shape: &Shape,
        ts: Transform,
        props: &str,
        direct: bool,
    ) {
        let bounds = self.on_page(shape.rect, ts);
        if self.culled(bounds) {
            return;
        }
        self.need(bounds);
        self.transform(ts);
        if kind == crate::paint::NativeKind::Surface {
            self.direct.surfaces += 1;
            let full = bounds.is_some_and(|(x, y, w, h)| {
                x.abs() < EPS
                    && y.abs() < EPS
                    && (w - self.direct.size.0).abs() < EPS
                    && (h - self.direct.size.1).abs() < EPS
            });
            if direct
                && full
                && ts == Transform::identity()
                && self.direct.isolation == 0
                && self.row.is_none()
                && self.group.is_none()
                && self.layers.unlayered()
                && self.slot.is_none()
                && self.again.is_none()
                && self.clips.iter().all(|c| !c.emitted)
                && shape.radii.iter().all(|r| *r == (0.0, 0.0))
                && shape.corners.is_none()
            {
                self.direct.at = Some(self.ops.len());
            }
        }
        self.ops.extend([NATIVE, id, kind as u32]);
        self.rect_radii(shape);
        self.string(props);
    }

    fn paths_clip(&mut self, paths: Vec<(bool, Vec<PathOp>)>, ts: Transform) {
        self.clips.push(Clip {
            geometry: Geometry::Paths(paths),
            ts,
            page: None,
            emitted: false,
        });
        self.materialize(self.clips.len() - 1);
    }

    pub(super) fn css_clip(&mut self, css: &exact_kernel::clip::ClipPath, ts: Transform) -> bool {
        if css.commands().is_empty() {
            return false;
        }
        let path = css
            .commands()
            .iter()
            .map(|(op, v)| match op {
                'M' => PathOp::Move(v[0], v[1]),
                'L' => PathOp::Line(v[0], v[1]),
                'C' => PathOp::Cubic(v[0], v[1], v[2], v[3], v[4], v[5]),
                'Z' => PathOp::Close,
                _ => unreachable!("validated CSS path"),
            })
            .collect();
        self.paths_clip(
            vec![(css.rule() == exact_kernel::FillRule::Evenodd, path)],
            ts,
        );
        true
    }

    pub(super) fn svg_clip(
        &mut self,
        clip: &exact_kernel::svg::scene::Clip,
        ts: Transform,
    ) -> usize {
        let mut level = Some(clip);
        let mut pushed = 0;
        while let Some(c) = level {
            let paths = c
                .shapes
                .iter()
                .map(|s| {
                    let path = s
                        .path
                        .0
                        .iter()
                        .map(|seg| match *seg {
                            exact_kernel::svg::Seg::Move(x, y) => PathOp::Move(x, y),
                            exact_kernel::svg::Seg::Line(x, y) => PathOp::Line(x, y),
                            exact_kernel::svg::Seg::Cubic(a, b, c, d, x, y) => {
                                PathOp::Cubic(a, b, c, d, x, y)
                            }
                            exact_kernel::svg::Seg::Close => PathOp::Close,
                        })
                        .collect();
                    (s.even_odd, path)
                })
                .collect();
            self.paths_clip(paths, ts);
            pushed += 1;
            level = c.then.as_deref();
        }
        pushed
    }

    pub(super) fn mask_end(
        &mut self,
        shape: &Shape,
        mask: &Result<GradientPaint, [u8; 4]>,
        ts: Transform,
    ) {
        self.need(None);
        self.ops.push(MASK_LAYER);
        match mask {
            Ok(gradient) => self.fill_gradient(shape, gradient, ts),
            Err(color) => self.fill(shape, *color, ts),
        }
        // Composite the entire mask layer, including transparency outside the border box.
        self.ops.push(RESTORE);
        self.matrix = None;
        self.pop_opacity();
    }

    pub(super) fn clip_push(&mut self, s: &Shape, ts: Transform) {
        let page = self.on_page(s.rect, ts).map(|r| {
            let (sx, sy, _, _) = axis(ts).expect("mapped");
            (r, s.radii.map(|(x, y)| (x * sx, y * sy)))
        });
        self.clips.push(Clip {
            geometry: Geometry::Rect(*s),
            ts,
            page,
            emitted: false,
        });
        if page.is_none() {
            let depth = self.clips.len() - 1;
            self.materialize(depth);
        }
    }

    pub(super) fn clip_pop(&mut self) {
        if self.clips.pop().is_some_and(|c| c.emitted) {
            self.ops.push(RESTORE);
            // The restore puts back the reader's matrix from before the save.
            self.matrix = None;
        }
    }

    /// Write every pending clip up to and including `depth`, outermost first.
    pub(super) fn materialize(&mut self, depth: usize) {
        for i in 0..=depth.min(self.clips.len().saturating_sub(1)) {
            if self.clips.is_empty() || self.clips[i].emitted {
                continue;
            }
            let (geometry, ts) = (self.clips[i].geometry.clone(), self.clips[i].ts);
            self.transform(ts);
            match geometry {
                Geometry::Rect(shape) => {
                    self.ops.push(CLIP_RRECT);
                    self.rect_radii(&shape);
                }
                Geometry::Paths(paths) => {
                    if paths.len() <= 1 {
                        let (rule, ops) = paths
                            .first()
                            .map_or((false, &[][..]), |(r, p)| (*r, &p[..]));
                        self.ops.extend([CLIP_PATH, u32::from(rule)]);
                        self.path(ops);
                    } else {
                        let at = self.ops.len();
                        self.ops.extend([CLIP_UNION, 0, paths.len() as u32]);
                        for (rule, ops) in paths {
                            self.ops.push(u32::from(rule));
                            self.path(&ops);
                        }
                        self.ops[at + 1] = (self.ops.len() - at - 2) as u32;
                    }
                }
            }
            self.clips[i].emitted = true;
        }
    }

    /// Whether a drawing within `bounds` (page space) would be clipped away
    /// entirely: outside some clip's rectangle (the list's overscan rows, out
    /// of its viewport). Such drawings are not recorded at all.
    pub(super) fn culled(&self, bounds: Option<Rect4>) -> bool {
        let Some(b) = bounds else { return false };
        self.clips
            .iter()
            .any(|c| c.page.is_some_and(|(r, _)| !overlaps(b, r)))
    }

    /// Whether the clips leave nothing at all (two of them do not meet: a
    /// scroller moved out of its page's view).
    pub(super) fn clipped_out(&self) -> bool {
        let mut left: Option<Rect4> = None;
        for (r, _) in self.clips.iter().filter_map(|c| c.page) {
            let meet = left.map_or(r, |l| intersect(l, r));
            if meet.2 <= 0.0 || meet.3 <= 0.0 {
                return true;
            }
            left = Some(meet);
        }
        false
    }

    /// Before drawing within `bounds` (page space; `None` when unknown):
    /// write the pending clips the drawing needs.
    pub(super) fn need(&mut self, bounds: Option<Rect4>) {
        let deepest = self.clips.iter().rposition(|c| {
            !c.emitted
                && match (bounds, c.page) {
                    (Some(b), Some(page)) => !unclipped(b, page),
                    _ => true,
                }
        });
        if let Some(d) = deepest {
            self.materialize(d);
        }
    }

    /// A picture drawn as a rounded rect, its corners the clips' own corners,
    /// when every clip around it allows that; `false` to draw it as before.
    pub(super) fn image_rrect(
        &mut self,
        id: u32,
        dst: Rect4,
        own: &[Shape],
        ts: Transform,
        tint: Option<[u8; 4]>,
    ) -> bool {
        let Some(d) = self.on_page(dst, ts) else {
            return false;
        };
        let (sx, sy, _, _) = axis(ts).expect("mapped");
        let mut region = d;
        let mut shapes: Vec<(Rect4, [(f32, f32); 4])> = Vec::new();
        for c in own {
            let Some(r) = map(c.rect, ts) else {
                return false;
            };
            shapes.push((r, c.radii.map(|(x, y)| (x * sx, y * sy))));
        }
        for c in self.clips.iter().filter(|c| !c.emitted) {
            let Some(p) = c.page else { return false };
            shapes.push(p);
        }
        for (r, _) in &shapes {
            region = intersect(region, *r);
        }
        if region.2 <= 0.0 || region.3 <= 0.0 || self.culled(Some(region)) {
            return true;
        }
        let mut radii = [(0.0f32, 0.0f32); 4];
        for (r, rr) in &shapes {
            for k in 0..4 {
                if rr[k].0 <= 0.0
                    || rr[k].1 <= 0.0
                    || !overlaps(region, corner_square(*r, k, rr[k]))
                {
                    continue;
                }
                let (a, b) = (corner(region, k), corner(*r, k));
                if (a.0 - b.0).abs() > 0.5 || (a.1 - b.1).abs() > 0.5 {
                    return false;
                }
                radii[k] = (radii[k].0.max(rr[k].0), radii[k].1.max(rr[k].1));
            }
        }
        // Page space; inside a row, the matrix moves it to the row's origin.
        self.transform(Transform::identity());
        self.image_tint(tint);
        self.ops.extend([IMAGE_RRECT, id]);
        for v in [d.0, d.1, d.2, d.3, region.0, region.1, region.2, region.3] {
            self.f(v);
        }
        for (x, y) in radii {
            self.f(x);
            self.f(y);
        }
        true
    }
}

#[cfg(test)]
mod effect_tests {
    use super::super::stream::end;
    use super::*;
    use exact_kernel::clip::ClipPath;
    use exact_kernel::svg::scene::{Clip as SvgClip, ClipShape};

    #[test]
    fn css_clip_records_the_path_and_restores_its_scope() {
        let mut r = Recorder::new();
        let ts = Transform::from_translate(12.0, 9.0);
        let css = ClipPath::parse("path(evenodd, 'M0 0 L100 0 L0 100 Z')").unwrap();
        assert!(r.push_css_clip(&css, ts));
        r.fill(&Shape::rect((0., 0., 100., 100.)), [255; 4], ts);
        r.pop_clip();
        let (ops, _) = super::super::stream::drawing(&r.ops).unwrap();
        assert_eq!(ops[7..10], [CLIP_PATH, 1, 4]);
        assert_eq!(ops.last(), Some(&RESTORE));
        assert!(r.clips.is_empty());
    }

    #[test]
    fn svg_clip_records_its_shapes() {
        let mut r = Recorder::new();
        let clip = SvgClip {
            shapes: vec![ClipShape {
                path: exact_kernel::svg::parse_d_whole("M0 0 L100 0 L0 100 Z").unwrap(),
                even_odd: false,
            }],
            then: None,
        };
        assert_eq!(r.push_svg_clip(&clip, Transform::identity()), 1);
        assert!(!r.ops.is_empty());
        r.pop_clip();
        assert_eq!(r.ops.last(), Some(&RESTORE));
    }

    #[test]
    fn svg_clip_unions_each_child_rule_before_intersecting_its_own_clip() {
        let shape = |data, even_odd| ClipShape {
            path: exact_kernel::svg::parse_d_whole(data).unwrap(),
            even_odd,
        };
        let clip = SvgClip {
            shapes: vec![
                shape("M0 0 L80 0 L0 80 Z", false),
                shape("M0 0 L0 80 L80 0 Z", true),
            ],
            then: Some(Box::new(SvgClip {
                shapes: vec![],
                then: None,
            })),
        };
        let mut r = Recorder::new();
        let ts = Transform::identity();
        r.push_clip(&Shape::new((0., 0., 100., 100.), [8.; 4]), ts);
        assert_eq!(r.push_svg_clip(&clip, ts), 2);
        // The pending rounded ancestor is outside the union. Reversing the
        // second child's winding must not subtract it from the first child.
        let union = end(&r.ops, end(&r.ops, 0).unwrap()).unwrap();
        assert_eq!(r.ops[7], CLIP_RRECT);
        assert_eq!(r.ops[union..union + 4], [CLIP_UNION, 25, 2, 0]);
        assert_eq!(r.ops[union + 15], 1, "the second child's evenodd rule");
        let empty = end(&r.ops, union).unwrap();
        assert_eq!(
            r.ops[empty..],
            [CLIP_PATH, 0, 0],
            "empty SVG clips hide all content"
        );
        for _ in 0..3 {
            r.pop_clip();
        }
        assert_eq!(&r.ops[r.ops.len() - 3..], &[RESTORE; 3]);
        assert!(super::super::stream::drawing(&r.ops).is_some());
    }

    #[test]
    fn css_curves_and_pending_clips_survive_nested_scopes() {
        let mut r = Recorder::new();
        let ts = Transform::from_translate(12., 9.);
        assert!(!r.push_css_clip(&ClipPath::default(), ts));
        assert!(r.ops.is_empty());
        r.push_clip(&Shape::rect((0., 0., 120., 120.)), Transform::identity());
        let css = ClipPath::parse("path('M0 0 Q50 100 100 0 Z')").unwrap();
        assert!(r.push_css_clip(&css, ts));
        r.push_clip(&Shape::rect((10., 10., 20., 20.)), ts);
        r.fill(&Shape::rect((0., 0., 100., 100.)), [255; 4], ts);
        for _ in 0..3 {
            r.pop_clip();
        }
        r.fill(
            &Shape::rect((0., 0., 4., 4.)),
            [255; 4],
            Transform::identity(),
        );
        let mut at = 0;
        let mut codes = Vec::new();
        while at < r.ops.len() {
            codes.push(r.ops[at]);
            at = end(&r.ops, at).unwrap();
        }
        assert_eq!(
            codes,
            [
                MATRIX, CLIP_RRECT, MATRIX, CLIP_PATH, CLIP_RRECT, RRECT, RESTORE, RESTORE,
                RESTORE, MATRIX, RRECT
            ]
        );
    }

    #[test]
    fn mask_alpha_changes_the_recorded_composite() {
        let record = |alpha| {
            let mut r = Recorder::new();
            let s = Shape::rect((0., 0., 100., 100.));
            let ts = Transform::identity();
            r.push_mask(&s, ts);
            r.fill(&s, [255, 0, 0, 255], ts);
            r.pop_mask(&s, &Err([0, 0, 0, alpha]), ts);
            r.ops
        };
        assert_ne!(record(255), record(0));
    }

    #[test]
    fn gradient_mask_is_isolated_from_its_siblings_and_uses_its_own_transform() {
        use exact_kernel::{gradient::Geometry, Color};
        let mut r = Recorder::new();
        let s = Shape::rect((0., 0., 100., 100.));
        let ts = Transform::from_translate(20., 30.);
        let gradient = GradientPaint {
            geometry: Geometry::Linear {
                start: (0., 0.),
                end: (0., 100.),
            },
            repeat: false,
            stops: vec![(0., Color::rgba(0, 0, 0, 255)), (1., Color::TRANSPARENT)],
        };
        r.fill(&s, [0, 0, 255, 255], Transform::identity());
        r.push_mask(&s, ts);
        r.push_mask(&s, ts);
        r.fill(&s, [255, 0, 0, 255], Transform::identity());
        r.pop_mask(&s, &Ok(gradient), ts);
        r.pop_mask(&s, &Err([0, 0, 0, 128]), ts);
        r.fill(&s, [0, 255, 0, 255], Transform::identity());
        let mut at = 0;
        let mut codes = Vec::new();
        while at < r.ops.len() {
            codes.push(r.ops[at]);
            if r.ops[at] == super::super::gradient::SHADER {
                assert_eq!(r.ops[at + 18] >> 24, 255);
                assert_eq!(r.ops[at + 20] >> 24, 0);
            }
            at = end(&r.ops, at).unwrap();
        }
        assert_eq!(
            codes,
            [
                MATRIX,
                RRECT,
                LAYER,
                LAYER,
                RRECT,
                MASK_LAYER,
                MATRIX,
                super::super::gradient::SHADER,
                RRECT,
                RESTORE,
                RESTORE,
                MASK_LAYER,
                MATRIX,
                RRECT,
                RESTORE,
                RESTORE,
                MATRIX,
                RRECT
            ]
        );
        assert_eq!(r.direct.isolation, 0);
        assert_eq!(r.matrix.unwrap()[4..], [0., 0.]);
    }
}

#[cfg(test)]
mod direct_tests {
    use super::*;
    use crate::paint::NativeKind;

    fn surface(r: &mut Recorder, shape: &Shape, ts: Transform, opaque_sdr: bool) {
        r.native(91, NativeKind::Surface, shape, ts, "{}", opaque_sdr);
    }
    fn direct(r: &mut Recorder) -> bool {
        r.finish().unwrap();
        let ops = FINISHED.with(|f| f.borrow_mut().take().unwrap());
        let mut at = 0;
        while at < ops.len() {
            if ops[at] == NATIVE {
                return ops[at + 2] & 256 != 0;
            }
            at = super::super::stream::end(&ops, at).unwrap();
        }
        false
    }

    #[test]
    fn direct_presentation_requires_an_unisolated_full_viewport_canvas() {
        let ts = Transform::identity();
        let full = Shape::rect((0., 0., 400., 800.));
        for case in 0..11 {
            let mut r = Recorder::new();
            r.begin(400., 800., 2.75);
            match case {
                1 => r.push_opacity(1.0),
                2 => r.push_opacity(0.5),
                3 => r.push_mask(&full, ts),
                4 => r.push_clip(&Shape::new(full.rect, [10.; 4]), ts),
                5 => r.group_begin(7, (0., 0.)),
                6 => {
                    r.layer_begin(7, ts, (0., 0.), [0., 0., 1., 0., 1., 0., 0.]);
                }
                _ => (),
            }
            let shape = match case {
                7 => Shape::rect((0., 0., 320., 800.)),
                8 => Shape::new(full.rect, [10.; 4]),
                _ => full,
            };
            surface(&mut r, &shape, ts, case != 9);
            if case == 10 {
                surface(&mut r, &full, ts, true);
            }
            match case {
                1..=3 => r.pop_opacity(),
                4 => r.pop_clip(),
                5 => r.group_end(),
                6 => r.layer_end(),
                _ => (),
            }
            assert_eq!(direct(&mut r), case == 0, "case {case}");
        }
        let mut r = Recorder::new();
        r.begin(400., 800., 1.);
        surface(&mut r, &full, ts, true);
        assert!(direct(&mut r));
        // A rotated/split window is qualified against this paint's dimensions.
        r.begin(800., 400., 1.);
        surface(&mut r, &full, ts, true);
        assert!(!direct(&mut r));
        r.begin(800., 400., 1.);
        surface(&mut r, &Shape::rect((0., 0., 800., 400.)), ts, true);
        assert!(direct(&mut r));
        r.begin(800., 400., 1.);
        surface(
            &mut r,
            &Shape::rect((-10., 0., 800., 400.)),
            Transform::from_translate(10., 0.),
            true,
        );
        assert!(!direct(&mut r));
    }

    #[test]
    fn retained_rows_only_forbid_direct_presentation_when_their_body_needs_world() {
        use exact_kernel::style::{BackdropFilter, BackdropOp};
        for backdrop in [false, true] {
            let mut r = Recorder::new();
            let ts = Transform::identity();
            let full = Shape::rect((0., 0., 400., 800.));
            r.begin(400., 800., 1.);
            r.row_begin(2, (0., 0.), None);
            if backdrop {
                r.backdrop_filter(&full, &BackdropFilter(vec![BackdropOp::Blur(2.)]), ts);
            } else {
                r.fill(&full, [255, 0, 0, 255], ts);
            }
            r.row_end(full.rect);
            r.finish().unwrap();
            r.begin(400., 800., 1.);
            surface(&mut r, &full, ts, true);
            r.row_draw(2, (0., 0.), full.rect);
            assert_eq!(direct(&mut r), !backdrop);
            r.row_free(2);
            r.begin(400., 800., 1.);
            surface(&mut r, &full, ts, true);
            assert!(direct(&mut r));
        }
    }
}
