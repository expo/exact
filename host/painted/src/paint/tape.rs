//! A drawing kept apart from the tree: the draw calls a walk made, owned
//! (paragraphs, pictures and paths kept alive), so they can be drawn again
//! once what made them is gone. The stage (`stage.rs`) keeps a route's last
//! drawing so that it can still slide out after the commit that removed it,
//! and a bar's picture so that the page can draw it under a sheet's scrim.
//!
//! A tape is recorded in its own space and replayed through a transform.
//! It draws no kept rows and opens no reader layers, so nothing it holds is
//! freed when its subtree goes; a text field the shell hosts is drawn into
//! it as this painter draws one (`field.rs`), since the shell's own view goes
//! with its node.
//!
//! The recorder is Janic Duplessis's, from his exit-animation work on the
//! appandflow fork (`paint/exit.rs` there).
use super::svg::Ink;
use super::*;
use std::cell::RefCell;

enum OwnedInk {
    Solid([u8; 4]),
    Gradient(
        exact_kernel::svg::server::Server,
        Vec<(f32, [u8; 4])>,
        [f32; 6],
    ),
    Pattern(Arc<Pixmap>, [f32; 6], f32),
}

impl OwnedInk {
    fn own(ink: &Ink<'_>) -> Self {
        match ink {
            Ink::Solid(c) => Self::Solid(*c),
            Ink::Gradient {
                server,
                stops,
                transform,
            } => Self::Gradient((*server).clone(), stops.clone(), *transform),
            Ink::Pattern {
                tile,
                transform,
                opacity,
            } => Self::Pattern(tile.clone(), *transform, *opacity),
        }
    }

    fn ink(&self) -> Ink<'_> {
        match self {
            Self::Solid(c) => Ink::Solid(*c),
            Self::Gradient(server, stops, transform) => Ink::Gradient {
                server,
                stops: stops.clone(),
                transform: *transform,
            },
            Self::Pattern(tile, transform, opacity) => Ink::Pattern {
                tile: tile.clone(),
                transform: *transform,
                opacity: *opacity,
            },
        }
    }
}

struct Svg {
    path: exact_kernel::svg::Path,
    fill: Option<OwnedInk>,
    even_odd: bool,
    stroke: Option<OwnedInk>,
    order: [u8; 3],
    width: f32,
    cap: u8,
    join: u8,
    miter: f32,
    dash: Vec<f32>,
    phase: f32,
}

enum Op {
    Fill(Shape, [u8; 4], Transform),
    Gradient(Shape, GradientPaint, Transform),
    Backdrop(Shape, BackdropFilter, Transform),
    Shadow(Shape, [u8; 4], f32, Shape, Transform),
    BoxShadow(
        Option<([u8; 4], f32, Shape)>,
        Shape,
        Vec<border::BorderFill>,
        Transform,
    ),
    TextBlur(f32),
    Border(border::BorderFill, Transform),
    Image(Arc<Bitmap>, Rect4, Vec<Shape>, Transform, Option<[u8; 4]>),
    Island(Arc<Pixmap>, Rect4, Transform, u8),
    PlaneBegin([f32; 9]),
    PlaneEnd,
    Canvas(Arc<Pixmap>, Rect4, Vec<Shape>, Transform),
    Text(Rc<Paragraph>, Vec<RunPaint>, (f32, f32), Transform),
    Svg(Box<Svg>, Transform),
    SvgClip(exact_kernel::svg::scene::Clip, Transform),
    Clip(Shape, Transform),
    CssClip(exact_kernel::clip::ClipPath, Transform),
    PopClip,
    Opacity(f32),
    PopOpacity,
    Mask(Shape, Transform),
    PopMask(Shape, Result<GradientPaint, [u8; 4]>, Transform),
}

/// A drawing, in the space it was recorded in.
#[derive(Default)]
pub(crate) struct Tape {
    ops: Vec<Op>,
}

/// The backend a tape is recorded through; a clone shares what it records.
/// It keeps a projective plane only for a backend that draws one (`planes`),
/// and otherwise lets the painter record its software warp.
#[derive(Clone, Default)]
pub(crate) struct Recorder {
    ops: Rc<RefCell<Vec<Op>>>,
    planes: bool,
}

impl Recorder {
    /// A recorder for a tape `target` will replay.
    pub(crate) fn for_target(target: &dyn Backend) -> Self {
        Self {
            ops: Rc::default(),
            planes: target.supports_planes(),
        }
    }

    fn push(&self, op: Op) {
        self.ops.borrow_mut().push(op);
    }

    /// What was recorded, as a tape; the recorder starts again empty.
    pub(crate) fn tape(&self) -> Tape {
        Tape {
            ops: std::mem::take(&mut *self.ops.borrow_mut()),
        }
    }
}

/// Record what `paint` draws into a tape of its own.
pub(crate) fn record(paint: impl FnOnce(&mut dyn Backend)) -> Tape {
    let recorder = Recorder::default();
    paint(&mut recorder.clone());
    recorder.tape()
}

impl Backend for Recorder {
    fn name(&self) -> &'static str {
        "tape"
    }
    fn begin(&mut self, _: f32, _: f32, _: f32) {}
    fn fill(&mut self, shape: &Shape, color: [u8; 4], ts: Transform) {
        self.push(Op::Fill(*shape, color, ts));
    }
    fn fill_gradient(&mut self, shape: &Shape, gradient: &GradientPaint, ts: Transform) {
        self.push(Op::Gradient(*shape, gradient.clone(), ts));
    }
    fn backdrop_filter(&mut self, shape: &Shape, filter: &BackdropFilter, ts: Transform) {
        self.push(Op::Backdrop(*shape, filter.clone(), ts));
    }
    fn blurred_shadow(
        &mut self,
        shape: &Shape,
        color: [u8; 4],
        sigma: f32,
        outer: &Shape,
        ts: Transform,
    ) -> bool {
        self.push(Op::Shadow(*shape, color, sigma, *outer, ts));
        true
    }
    /// The blur and its bands both: which one draws is the replaying
    /// backend's to say.
    fn box_shadow(
        &mut self,
        blurred: Option<([u8; 4], f32, Shape)>,
        outer: &Shape,
        bands: &dyn Fn() -> Vec<border::BorderFill>,
        ts: Transform,
    ) {
        self.push(Op::BoxShadow(blurred, *outer, bands(), ts));
    }
    fn text_blur(&mut self, sigma: f32) {
        self.push(Op::TextBlur(sigma));
    }
    fn fill_border(&mut self, part: &border::BorderFill, ts: Transform) {
        self.push(Op::Border(part.clone(), ts));
    }
    fn image(
        &mut self,
        image: &Arc<Bitmap>,
        dst: Rect4,
        clips: &[Shape],
        ts: Transform,
        tint: Option<[u8; 4]>,
    ) {
        self.push(Op::Image(image.clone(), dst, clips.to_vec(), ts, tint));
    }
    fn supports_island_image(&self) -> bool {
        true
    }
    fn island_image(&mut self, pixels: Arc<Pixmap>, dst: Rect4, ts: Transform, mode: u8) {
        self.push(Op::Island(pixels, dst, ts, mode));
    }
    fn plane_begin(&mut self, h: [f32; 9]) -> bool {
        if self.planes {
            self.push(Op::PlaneBegin(h));
        }
        self.planes
    }
    fn plane_end(&mut self) {
        self.push(Op::PlaneEnd);
    }
    /// A masked SVG island or a placed canvas child, already in the walk's
    /// space: a canvas drawn there.
    fn surface_image(&mut self, pixels: Arc<Pixmap>, dst: Rect4) {
        self.push(Op::Canvas(pixels, dst, Vec::new(), Transform::identity()));
    }
    fn canvas(&mut self, pixels: &Arc<Pixmap>, dst: Rect4, clips: &[Shape], ts: Transform) {
        self.push(Op::Canvas(pixels.clone(), dst, clips.to_vec(), ts));
    }
    fn text(
        &mut self,
        _: &mut TextEngine,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        origin: (f32, f32),
        ts: Transform,
    ) {
        if let Some(p) = paragraph.retained() {
            self.push(Op::Text(p, palette.to_vec(), origin, ts));
        }
    }
    fn svg_path(&mut self, s: &SvgPaint<'_>, ts: Transform) {
        let svg = Svg {
            path: s.path.clone(),
            fill: s.fill.as_ref().map(OwnedInk::own),
            even_odd: s.even_odd,
            stroke: s.stroke.as_ref().map(OwnedInk::own),
            order: s.order,
            width: s.width,
            cap: s.cap,
            join: s.join,
            miter: s.miter,
            dash: s.dash.clone(),
            phase: s.phase,
        };
        self.push(Op::Svg(Box::new(svg), ts));
    }
    fn push_svg_clip(&mut self, clip: &exact_kernel::svg::scene::Clip, ts: Transform) -> usize {
        self.push(Op::SvgClip(clip.clone(), ts));
        1
    }
    fn push_clip(&mut self, shape: &Shape, ts: Transform) {
        self.push(Op::Clip(*shape, ts));
    }
    fn push_css_clip(&mut self, path: &exact_kernel::clip::ClipPath, ts: Transform) -> bool {
        self.push(Op::CssClip(path.clone(), ts));
        true
    }
    fn pop_clip(&mut self) {
        self.push(Op::PopClip);
    }
    fn push_opacity(&mut self, alpha: f32) {
        self.push(Op::Opacity(alpha));
    }
    fn pop_opacity(&mut self) {
        self.push(Op::PopOpacity);
    }
    fn push_mask(&mut self, shape: &Shape, ts: Transform) {
        self.push(Op::Mask(*shape, ts));
    }
    fn pop_mask(&mut self, shape: &Shape, mask: &Result<GradientPaint, [u8; 4]>, ts: Transform) {
        self.push(Op::PopMask(*shape, mask.clone(), ts));
    }
    fn pointer(&mut self, _: f32, _: f32) {}
    fn finish(&mut self) -> Result<Pixmap, String> {
        Err("a tape has no pixels".into())
    }
}

impl Tape {
    /// Draw the tape through `g` into `b`. Inside a plane, ops are in the
    /// plane's own space.
    pub(crate) fn replay(&self, b: &mut dyn Backend, text: &mut TextEngine, g: Transform) {
        let mut planes = 0usize;
        let mut pops: Vec<usize> = Vec::new();
        for op in &self.ops {
            let t = |ts: Transform| if planes == 0 { g.pre_concat(ts) } else { ts };
            match op {
                Op::Fill(s, c, ts) => b.fill(s, *c, t(*ts)),
                Op::Gradient(s, p, ts) => b.fill_gradient(s, p, t(*ts)),
                Op::Backdrop(s, f, ts) => b.backdrop_filter(s, f, t(*ts)),
                Op::Shadow(s, c, sigma, outer, ts) => {
                    b.blurred_shadow(s, *c, *sigma, outer, t(*ts));
                }
                Op::BoxShadow(blurred, outer, bands, ts) => {
                    b.box_shadow(*blurred, outer, &|| bands.clone(), t(*ts))
                }
                Op::TextBlur(sigma) => b.text_blur(*sigma),
                Op::Border(part, ts) => b.fill_border(part, t(*ts)),
                Op::Image(image, dst, clips, ts, tint) => {
                    b.image(image, *dst, clips, t(*ts), *tint)
                }
                Op::Island(pixels, dst, ts, mode) => {
                    b.island_image(pixels.clone(), *dst, t(*ts), *mode)
                }
                Op::PlaneBegin(h) => {
                    let h = match planes {
                        0 => mul(g, h),
                        _ => *h,
                    };
                    b.plane_begin(h);
                    planes += 1;
                }
                Op::PlaneEnd => {
                    planes = planes.saturating_sub(1);
                    b.plane_end();
                }
                Op::Canvas(pixels, dst, clips, ts) => b.canvas(pixels, *dst, clips, t(*ts)),
                Op::Text(p, palette, origin, ts) => b.text(text, p, palette, *origin, t(*ts)),
                Op::Svg(s, ts) => {
                    let paint = SvgPaint {
                        path: &s.path,
                        fill: s.fill.as_ref().map(OwnedInk::ink),
                        even_odd: s.even_odd,
                        stroke: s.stroke.as_ref().map(OwnedInk::ink),
                        order: s.order,
                        width: s.width,
                        cap: s.cap,
                        join: s.join,
                        miter: s.miter,
                        dash: s.dash.clone(),
                        phase: s.phase,
                    };
                    b.svg_path(&paint, t(*ts));
                }
                Op::SvgClip(clip, ts) => pops.push(b.push_svg_clip(clip, t(*ts))),
                Op::Clip(s, ts) => {
                    b.push_clip(s, t(*ts));
                    pops.push(1);
                }
                Op::CssClip(path, ts) => pops.push(usize::from(b.push_css_clip(path, t(*ts)))),
                Op::PopClip => {
                    for _ in 0..pops.pop().unwrap_or(0) {
                        b.pop_clip();
                    }
                }
                Op::Opacity(a) => b.push_opacity(*a),
                Op::PopOpacity => b.pop_opacity(),
                Op::Mask(s, ts) => b.push_mask(s, t(*ts)),
                Op::PopMask(s, mask, ts) => b.pop_mask(s, mask, t(*ts)),
            }
        }
    }
}

/// `g` (affine) after the projective map `h`, both row-major.
fn mul(g: Transform, h: &[f32; 9]) -> [f32; 9] {
    let a = [g.sx, g.kx, g.tx, g.ky, g.sy, g.ty, 0.0, 0.0, 1.0];
    let mut out = [0.0; 9];
    for r in 0..3 {
        for c in 0..3 {
            out[r * 3 + c] = (0..3).map(|k| a[r * 3 + k] * h[k * 3 + c]).sum();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A masked SVG island reaches the backend as a surface image: the tape
    /// keeps it and draws it where it was, moved by the replay.
    #[test]
    fn a_tape_keeps_a_surface_image() {
        let mut red = Pixmap::new(4, 4).unwrap();
        red.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let tape = record(|b| b.surface_image(Arc::new(red), (10.0, 10.0, 4.0, 4.0)));
        let mut raster = crate::raster::Raster::new();
        raster.begin(40.0, 40.0, 1.0);
        let mut engine = TextEngine::new();
        let moved = Transform::from_translate(5.0, 5.0);
        tape.replay(&mut raster, &mut engine, moved);
        let pixels = raster.finish().unwrap();
        let at = |x, y| pixels.pixel(x, y).unwrap();
        assert_eq!((at(16, 16).red(), at(16, 16).green()), (255, 0));
        let before = at(11, 11);
        assert!(
            before.alpha() == 0 || before.green() > 0,
            "nothing where it was before the move"
        );
    }

    /// A box shadow on a tape draws on a backend without a blur of its own,
    /// as its bands, as the live walk draws it there.
    #[test]
    fn a_tape_keeps_a_box_shadows_bands() {
        let shadow = super::super::shadow::ShadowPaint::lifted(1.0);
        let outer = Shape::rect((10.0, 10.0, 20.0, 20.0));
        let tape = record(|b| {
            let bands = || shadow.fills(&outer, [0.0; 4]);
            b.box_shadow(
                shadow.blurred(&outer),
                &outer,
                &bands,
                Transform::identity(),
            )
        });
        let mut raster = crate::raster::Raster::new();
        raster.begin(60.0, 60.0, 1.0);
        tape.replay(&mut raster, &mut TextEngine::new(), Transform::identity());
        let pixels = raster.finish().unwrap();
        let below = pixels.pixel(20, 34).unwrap();
        assert!(below.alpha() > 0, "the shadow under the box: {below:?}");
    }

    /// A plane is kept only for a backend that draws one; for another the
    /// recorder declines it, so the painter records its software warp.
    #[test]
    fn a_tape_keeps_a_plane_only_for_a_backend_that_draws_one() {
        let h = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let cpu = crate::raster::Raster::new();
        let mut for_cpu = Recorder::for_target(&cpu);
        assert!(!for_cpu.plane_begin(h));
        assert!(for_cpu.tape().ops.is_empty());
        let mut for_canvas = Recorder::for_target(&crate::canvas::Recorder::new());
        assert!(for_canvas.plane_begin(h));
    }
}
