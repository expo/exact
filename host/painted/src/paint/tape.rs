//! A drawing kept apart from the tree: the draw calls a walk made, owned
//! (paragraphs, pictures and paths kept alive), so they can be drawn again
//! once what made them is gone. The stage (`stage.rs`) keeps a route's last
//! drawing so that it can still slide out after the commit that removed it,
//! and a bar's picture so that the page can draw it under a sheet's scrim.
//!
//! A tape is recorded in its own space and replayed through a transform.
//! It opens no reader layers. A kept row the walk drew this frame it may
//! hold by reference ([`RowPin`]), on a backend that draws one through a
//! transform ([`Backend::rows_through`]): the backend's recording is then not
//! freed while the tape holds it. The row also keeps portable commands:
//! a backend without rows, including a CPU filter, draws those instead.
//! A transform that scales or turns the tape uses [`Backend::row_draw_through`].
//! Sticky constraints read surviving scrollers at replay. It may hold another
//! tape the same paint kept. A text field the shell hosts is drawn into it
//! as this painter draws one (`field.rs`), since the shell's own view goes
//! with its node.
//!
//! The recorder is Janic Duplessis's, from his exit-animation work on the
//! appandflow fork (`paint/exit.rs` there).
use super::rows::RowPin;
use super::space::mul3;
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
    Image(
        Arc<Bitmap>,
        Rect4,
        Vec<Shape>,
        Transform,
        Option<[u8; 4]>,
        exact_kernel::ImageRendering,
    ),
    Island(Arc<Pixmap>, Rect4, Transform, u8),
    PlaneBegin([f32; 9]),
    PlaneEnd,
    Canvas(
        Arc<Pixmap>,
        Option<CanvasUpdate>,
        Rect4,
        Vec<Shape>,
        Transform,
    ),
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
    /// A kept row at its top-left, covering bounds relative to it, drawn
    /// for a placement of this scale.
    Row(Rc<RowPin>, (f32, f32), Rect4, (f32, f32), Rc<Tape>),
    Sticky(Sticky, Rc<Tape>, Transform),
    Filter(Box<box_filter::Filter>, Rc<Tape>, Transform),
    /// Another tape, through a transform.
    Tape(Rc<Tape>, Transform),
}

/// A drawing, in the space it was recorded in.
#[derive(Default)]
pub(crate) struct Tape {
    ops: Vec<Op>,
    rows: bool,
    dynamic: bool,
}

#[derive(Clone, Copy)]
pub(super) struct Live<'a, 'scene> {
    pub(super) scene: &'a Scene<'scene>,
    pub(super) scroll_grids: &'a [(ViewId, Option<f32>)],
    pub(super) viewport: (f32, f32),
    pub(super) scale: f32,
}

/// Removed sticky geometry, evaluated against its surviving scroller.
pub(super) struct Sticky {
    constraint: exact_kernel::StickyConstraint,
    scroller: exact_kernel::NodeKey,
    recorded: (f32, f32),
    /// The display's scale, where it is drawn on its pixels (`grid.rs`).
    grid: Option<f32>,
}

impl Sticky {
    pub(super) fn capture(
        walk: &super::Walk<'_, '_>,
        id: ViewId,
        grid: Option<f32>,
    ) -> Option<Self> {
        let scene = walk.scene;
        let node = scene.kernel.node(id)?;
        let constraint = scene.kernel.sticky_constraint(node.key)?;
        let scroller = scene.kernel.node(constraint.scroller)?;
        let scroll_grid = space::sticky_owner_grid(scene, &node, &walk.scroll_grids).flatten();
        let scroll = space::sticky_scroll(scene, &scroller, scroll_grid);
        let grid = grid.filter(|_| !super::grid::leaves_grid(&node, &(scene.presented)(id)));
        Some(Self {
            constraint,
            scroller: scroller.key,
            recorded: super::grid::sticky_shift(constraint, &scroller, scroll, grid),
            grid,
        })
    }

    fn offset(&self, live: Option<Live<'_, '_>>) -> (f32, f32) {
        live.and_then(|live| {
            let scroller = live.scene.kernel.node_by_key(self.scroller)?;
            let grid = space::scroll_grid(&scroller, live.scroll_grids).flatten();
            let scroll = space::sticky_scroll(live.scene, &scroller, grid);
            Some(super::grid::sticky_shift(
                self.constraint,
                &scroller,
                scroll,
                self.grid,
            ))
        })
        .unwrap_or(self.recorded)
    }

    pub(super) fn unplace(&self, inverse: Transform) -> Transform {
        inverse.post_translate(-self.recorded.0, -self.recorded.1)
    }
}

/// The backend a tape is recorded through; a clone shares what it records.
/// It keeps a projective plane only for a backend that draws one (`planes`),
/// and otherwise lets the painter record its software warp.
#[derive(Clone, Default)]
pub(crate) struct Recorder {
    ops: Rc<RefCell<Vec<Op>>>,
    planes: bool,
    blends: bool,
}

impl Recorder {
    /// A recorder for a tape `target` will replay.
    pub(crate) fn for_target(target: &dyn Backend) -> Self {
        Self {
            ops: Rc::default(),
            planes: target.supports_planes(),
            blends: target.supports_blend(),
        }
    }

    /// Removed drawings can enter a CPU filter during replay. Record a
    /// projective subtree's software warp instead of a backend-only plane.
    pub(super) fn portable(target: &dyn Backend) -> Self {
        Self {
            planes: false,
            ..Self::for_target(target)
        }
    }

    fn push(&self, op: Op) {
        self.ops.borrow_mut().push(op);
    }

    /// Kept row `pin` with its top-left at `origin`, covering `bounds`
    /// relative to it, drawn for a placement in the page scaling by `scale`.
    pub(super) fn row(
        &self,
        pin: Rc<RowPin>,
        origin: (f32, f32),
        bounds: Rect4,
        scale: (f32, f32),
        drawing: Rc<Tape>,
    ) {
        self.push(Op::Row(pin, origin, bounds, scale, drawing));
    }

    pub(super) fn sticky(&self, sticky: Sticky, tape: Rc<Tape>, ts: Transform) {
        self.push(Op::Sticky(sticky, tape, ts));
    }

    pub(super) fn filter(&self, filter: box_filter::Filter, tape: Rc<Tape>, ts: Transform) {
        self.push(Op::Filter(Box::new(filter), tape, ts));
    }

    /// Another tape, drawn through `ts`.
    pub(super) fn nest(&self, tape: Rc<Tape>, ts: Transform) {
        self.push(Op::Tape(tape, ts));
    }

    /// What was recorded, as a tape; the recorder starts again empty.
    pub(crate) fn tape(&self) -> Tape {
        let ops = std::mem::take(&mut *self.ops.borrow_mut());
        let rows = ops.iter().any(|op| match op {
            Op::Row(..) => true,
            Op::Tape(tape, _) | Op::Sticky(_, tape, _) | Op::Filter(_, tape, _) => tape.rows,
            _ => false,
        });
        let dynamic = ops.iter().any(|op| match op {
            Op::Sticky(..) => true,
            Op::Tape(tape, _) | Op::Row(_, _, _, _, tape) | Op::Filter(_, tape, _) => tape.dynamic,
            _ => false,
        });
        Tape { ops, rows, dynamic }
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
        rendering: exact_kernel::ImageRendering,
    ) {
        self.push(Op::Image(
            image.clone(),
            dst,
            clips.to_vec(),
            ts,
            tint,
            rendering,
        ));
    }
    fn supports_blend(&self) -> bool {
        self.blends
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
        self.push(Op::Canvas(
            pixels,
            None,
            dst,
            Vec::new(),
            Transform::identity(),
        ));
    }
    fn canvas(&mut self, pixels: &Arc<Pixmap>, dst: Rect4, clips: &[Shape], ts: Transform) {
        self.canvas_frame(pixels, None, dst, clips, ts);
    }
    fn canvas_frame(
        &mut self,
        pixels: &Arc<Pixmap>,
        update: Option<CanvasUpdate>,
        dst: Rect4,
        clips: &[Shape],
        ts: Transform,
    ) {
        self.push(Op::Canvas(pixels.clone(), update, dst, clips.to_vec(), ts));
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
    /// Conservative paint bounds, including nested drawings and sticky
    /// constraints evaluated at the same live scroll as replay.
    pub(super) fn bounds(&self, live: Live<'_, '_>, ts: Transform) -> Option<Rect4> {
        self.bounds_in(live, mul(ts, &IDENTITY))
    }

    pub(super) fn bounds_in(&self, live: Live<'_, '_>, matrix: [f32; 9]) -> Option<Rect4> {
        let mut area = None;
        let mut planes = vec![matrix];
        let mut blur = 0.0;
        for op in &self.ops {
            let space = *planes.last().unwrap();
            let mapped = |ts, rect| space::mapped_bounds(&mul3(&space, &mul(ts, &IDENTITY)), rect);
            let bound = match op {
                Op::Fill(s, _, ts) | Op::Gradient(s, _, ts) | Op::Backdrop(s, _, ts) => {
                    mapped(*ts, s.rect)
                }
                Op::Shadow(s, _, sigma, _, ts) => mapped(*ts, grow(s.rect, 3.0 * sigma + 2.0)),
                Op::BoxShadow(blurred, _, bands, ts) => {
                    let mut b = blurred
                        .and_then(|(_, sigma, s)| mapped(*ts, grow(s.rect, 3.0 * sigma + 2.0)));
                    for band in bands {
                        merge(
                            &mut b,
                            path_bounds(&band.region).and_then(|r| mapped(*ts, r)),
                        );
                    }
                    b
                }
                Op::Border(b, ts) => path_bounds(&b.region).and_then(|r| mapped(*ts, r)),
                Op::Image(_, r, _, ts, _, _)
                | Op::Island(_, r, ts, _)
                | Op::Canvas(_, _, r, _, ts) => mapped(*ts, *r),
                Op::Text(p, _, at, ts) => p
                    .paint_bounds(live.scale)
                    .and_then(|r| mapped(*ts, grow((r.0 + at.0, r.1 + at.1, r.2, r.3), blur))),
                Op::Svg(s, ts) => s.path.bounds().and_then(|r| {
                    let stroke = if s.stroke.is_some() {
                        s.width * s.miter.max(2.0)
                    } else {
                        0.0
                    };
                    mapped(*ts, grow(r, stroke))
                }),
                Op::Row(_, at, _, size, drawing) => {
                    if size.0 == 0.0 || size.1 == 0.0 {
                        continue;
                    }
                    let ts =
                        Transform::from_translate(at.0, at.1).pre_scale(1.0 / size.0, 1.0 / size.1);
                    drawing.bounds_in(live, mul3(&space, &mul(ts, &IDENTITY)))
                }
                Op::Sticky(sticky, drawing, ts) => {
                    let d = sticky.offset(Some(live));
                    drawing.bounds_in(
                        live,
                        mul3(&space, &mul(ts.pre_translate(d.0, d.1), &IDENTITY)),
                    )
                }
                Op::Filter(filter, drawing, ts) => mapped(*ts, filter.region(drawing, live)),
                Op::Tape(drawing, ts) => {
                    drawing.bounds_in(live, mul3(&space, &mul(*ts, &IDENTITY)))
                }
                Op::PlaneBegin(h) => {
                    planes.push(mul3(&space, h));
                    None
                }
                Op::PlaneEnd => {
                    if planes.len() > 1 {
                        planes.pop();
                    }
                    None
                }
                Op::TextBlur(sigma) => {
                    blur = 3.0 * sigma;
                    None
                }
                Op::SvgClip(..)
                | Op::Clip(..)
                | Op::CssClip(..)
                | Op::PopClip
                | Op::Opacity(..)
                | Op::PopOpacity
                | Op::Mask(..)
                | Op::PopMask(..) => None,
            };
            merge(&mut area, bound);
        }
        area
    }

    /// Whether it holds kept rows.
    pub(crate) fn draws_rows(&self) -> bool {
        self.rows
    }

    /// Draw the tape through `g` into `b`. Inside a plane, ops are in the
    /// plane's own space.
    pub(crate) fn replay(&self, b: &mut dyn Backend, text: &mut TextEngine, g: Transform) {
        self.replay_live(b, text, g, None);
    }

    pub(super) fn replay_live(
        &self,
        b: &mut dyn Backend,
        text: &mut TextEngine,
        g: Transform,
        live: Option<Live<'_, '_>>,
    ) {
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
                Op::Image(image, dst, clips, ts, tint, rendering) => {
                    b.image(image, *dst, clips, t(*ts), *tint, *rendering)
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
                Op::Canvas(pixels, update, dst, clips, ts) => {
                    b.canvas_frame(pixels, *update, *dst, clips, t(*ts))
                }
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
                Op::Row(pin, origin, bounds, scale, drawing) => {
                    if scale.0 == 0.0 || scale.1 == 0.0 {
                        continue;
                    }
                    let at = g.pre_translate(origin.0, origin.1);
                    let ts = at.pre_scale(1.0 / scale.0, 1.0 / scale.1);
                    if planes != 0 || !b.rows_through() || drawing.dynamic {
                        drawing.replay_live(b, text, ts, live);
                    } else {
                        let near = |a: f32, b: f32| (a - b).abs() <= 1e-4 * b.abs().max(1.0);
                        if g.kx == 0.0 && g.ky == 0.0 && near(g.sx, scale.0) && near(g.sy, scale.1)
                        {
                            let page = (at.tx + bounds.0, at.ty + bounds.1, bounds.2, bounds.3);
                            if !b.row_culled(page) {
                                b.row_draw(pin.id(), (at.tx, at.ty), page);
                            }
                        } else if !b.row_culled(bbox(ts, *bounds)) {
                            b.row_draw_through(pin.id(), ts, *bounds);
                        }
                    }
                }
                Op::Sticky(sticky, tape, ts) => {
                    let d = sticky.offset(live);
                    tape.replay_live(b, text, t(*ts).pre_translate(d.0, d.1), live);
                }
                Op::Filter(filter, drawing, ts) => filter.replay(b, text, drawing, t(*ts), live),
                Op::Tape(tape, ts) => tape.replay_live(b, text, t(*ts), live),
            }
        }
    }
}

const IDENTITY: [f32; 9] = [1., 0., 0., 0., 1., 0., 0., 0., 1.];

fn merge(area: &mut Option<Rect4>, rect: Option<Rect4>) {
    if let Some(rect) = rect.filter(|r| r.2 > 0.0 && r.3 > 0.0) {
        *area = Some(rows::union(*area, rect));
    }
}

fn grow((x, y, w, h): Rect4, reach: f32) -> Rect4 {
    (x - reach, y - reach, w + 2.0 * reach, h + 2.0 * reach)
}

fn path_bounds(path: &[border::PathOp]) -> Option<Rect4> {
    let points: Vec<_> = path.iter().flat_map(border::PathOp::points).collect();
    let (mut x0, mut y0, mut x1, mut y1) = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for p in points.as_chunks::<2>().0 {
        x0 = x0.min(p[0]);
        y0 = y0.min(p[1]);
        x1 = x1.max(p[0]);
        y1 = y1.max(p[1]);
    }
    (!points.is_empty()).then_some((x0, y0, x1 - x0, y1 - y0))
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

    /// On Ink a kept drawing holds its rows' draw calls, not the rows, so a
    /// scale or turn draws each row whole and blending with what is under
    /// it: a red row multiplied over green at 0.8 is black, and a row longer
    /// than the viewport turned 90° shows its far end.
    #[test]
    fn ink_draws_a_scaled_or_turned_drawing_whole_and_blended() {
        let mut ink = crate::ink::Ink::new().unwrap();
        assert!(!ink.rows_through());
        let mut red = Pixmap::new(100, 100).unwrap();
        red.fill(tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let red = Arc::new(red);
        let blended = record(|b| {
            b.island_image(
                red.clone(),
                (0.0, 0.0, 100.0, 100.0),
                Transform::identity(),
                1,
            )
        });
        let long = record(|b| {
            b.fill(
                &Shape::rect((0.0, 0.0, 400.0, 20.0)),
                [255, 0, 0, 255],
                Transform::identity(),
            )
        });
        let turned = Transform::from_translate(60.0, 0.0).pre_concat(Transform::from_rotate(90.0));
        let cases = [
            (
                blended,
                [0, 255, 0, 255],
                (100.0, 100.0),
                Transform::from_row(0.8, 0.0, 0.0, 0.8, 10.0, 10.0),
                [(50, 50, [0, 0, 0]), (95, 95, [0, 255, 0])],
            ),
            (
                long,
                [255; 4],
                (100.0, 400.0),
                turned,
                [(50, 350, [255, 0, 0]), (20, 350, [255, 255, 255])],
            ),
        ];
        for (tape, under, size, g, pixels) in cases {
            ink.begin(size.0, size.1, 1.0);
            let page = Shape::rect((0.0, 0.0, size.0, size.1));
            ink.fill(&page, under, Transform::identity());
            tape.replay(&mut ink, &mut TextEngine::new(), g);
            let frame = ink.finish().unwrap();
            for (x, y, rgb) in pixels {
                let px = frame.pixel(x, y).unwrap();
                let near = |a: u8, b: u8| a.abs_diff(b) < 30;
                let ok =
                    near(px.red(), rgb[0]) && near(px.green(), rgb[1]) && near(px.blue(), rgb[2]);
                assert!(ok, "{g:?} ({x}, {y}): {px:?}, not {rgb:?}");
            }
        }
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
