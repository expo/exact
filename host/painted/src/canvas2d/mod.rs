//! Canvas 2D on Linux (LLP 1056 D7): the recorded lists replayed into a
//! tiny-skia pixmap, the canvas's kept bitmap, under both painters. The
//! painters composite each canvas's latest snapshot in its content box; the
//! GPU painter updates changed pixels when it still has the previous revision.
//!
//! The list's geometry is resolved (`exact_canvas::list`): paths arrive in
//! canvas coordinates, so a fill or clip draws them under the base scale; a
//! stroke takes them back to user space and strokes under base ∘ author, so
//! a line's width follows the author's matrix as the canvas's does. Every
//! paint goes through one pipeline (`draw.rs`): shadows, the clip-extent
//! operators, patterns, images and conic gradients; text is glyph outlines
//! (`text.rs`).

pub(crate) mod capture;
mod damage;
mod draw;
mod prefix;
pub(crate) mod text;

pub(crate) use capture::capture;
use draw::Geom;
use exact_canvas::list::{self, text_at, Op};
use exact_kernel::ViewId;
use exact_runner::CanvasList;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use text::{CanvasText, RunStyle};
use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, Shader,
    Stroke, StrokeDash, Transform,
};

pub use damage::CanvasUpdate;

/// One canvas's snapshot for the painters.
#[derive(Clone)]
pub(crate) struct CanvasPaint {
    /// Pixels as last replayed.
    pub pixels: Arc<Pixmap>,
    pub update: Option<CanvasUpdate>,
}

pub(crate) type CanvasSnapshots = Rc<RefCell<BTreeMap<ViewId, CanvasPaint>>>;

/// Painted surfaces own their GPU readbacks. For 2D draws every painter reads
/// the replayer's snapshot table, so a finished paint holds no bitmap reference.
#[derive(Clone, Default)]
pub(crate) struct CanvasPictures {
    pub(crate) drawings: CanvasSnapshots,
    pub(crate) readbacks: BTreeMap<ViewId, CanvasPaint>,
}

impl CanvasPictures {
    pub(crate) fn get(&self, view: &ViewId) -> Option<CanvasPaint> {
        self.readbacks
            .get(view)
            .cloned()
            .or_else(|| self.drawings.borrow().get(view).cloned())
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn is_empty(&self) -> bool {
        self.readbacks.is_empty() && self.drawings.borrow().is_empty()
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Style {
    Color([f64; 4]),
    /// A gradient or pattern, by id.
    Object(u32),
}

#[derive(Clone)]
struct State {
    author: Transform,
    fill: Style,
    stroke: Style,
    width: f32,
    cap: LineCap,
    join: LineJoin,
    miter: f32,
    dash: Vec<f32>,
    dash_offset: f32,
    alpha: f32,
    composite: usize,
    shadow_color: [f64; 4],
    shadow_blur: f32,
    shadow_offset: (f32, f32),
    smoothing: (bool, u8),
    font: Option<Arc<RunStyle>>,
    clip: Option<Mask>,
}

impl State {
    pub(super) fn eq_unclipped(&self, other: &State) -> bool {
        self.clip.is_none()
            && other.clip.is_none()
            && self.author == other.author
            && self.fill == other.fill
            && self.stroke == other.stroke
            && self.width == other.width
            && self.cap == other.cap
            && self.join == other.join
            && self.miter == other.miter
            && self.dash == other.dash
            && self.dash_offset == other.dash_offset
            && self.alpha == other.alpha
            && self.composite == other.composite
            && self.shadow_color == other.shadow_color
            && self.shadow_blur == other.shadow_blur
            && self.shadow_offset == other.shadow_offset
            && self.smoothing == other.smoothing
            && self.font == other.font
    }
}

impl Default for State {
    fn default() -> Self {
        State {
            author: Transform::identity(),
            fill: Style::Color([0.0, 0.0, 0.0, 1.0]),
            stroke: Style::Color([0.0, 0.0, 0.0, 1.0]),
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter: 10.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            alpha: 1.0,
            composite: 0,
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            shadow_offset: (0.0, 0.0),
            smoothing: (true, 0),
            font: None,
            clip: None,
        }
    }
}

enum Object {
    Linear([f32; 4], Vec<(f32, [f64; 4])>),
    Radial([f32; 6], Vec<(f32, [f64; 4])>),
    Conic([f32; 3], Vec<(f32, [f64; 4])>),
    Pattern {
        image: u32,
        repetition: u8,
        transform: Transform,
    },
    Image(String),
}

/// One decoded image handle and whether every pixel in it is opaque.
#[derive(Clone)]
pub(crate) struct CachedImage {
    pub pixels: Arc<Pixmap>,
    pub opaque: bool,
}

impl CachedImage {
    pub(crate) fn new(pixels: Pixmap) -> Self {
        let opaque = pixels.data().as_chunks::<4>().0.iter().all(|p| p[3] == 255);
        Self {
            pixels: Arc::new(pixels),
            opaque,
        }
    }
}

/// The decoded image handles, shared by every replayer (LLP 1056 D9).
pub(crate) type ImageCache = HashMap<String, CachedImage>;

/// One canvas's replayer: its bitmap and the state the lists build on.
pub(crate) struct Replayer {
    /// Shared with the painters' snapshot until the next list draws into it,
    /// which copies it only while a painter still holds the last one.
    pixmap: Arc<Pixmap>,
    update: CanvasUpdate,
    /// True when every pixel in `pixmap` is transparent zero.
    cleared: bool,
    base: Transform,
    scale: f32,
    lifetime: u64,
    generation: u32,
    state: State,
    stack: Vec<State>,
    path: PathBuilder,
    /// A `Path2D`'s segments for the next path paint.
    scratch: PathBuilder,
    objects: HashMap<u32, Object>,
    /// Bumped whenever an existing entry in `objects` is overwritten or mutated.
    objects_epoch: u64,
    /// Active while building/advancing a prefix checkpoint to record every
    /// `ImageCache` handle read.
    image_reads: Option<Vec<(String, usize)>>,
    last_draw: Option<prefix::LastDraw>,
    checkpoint: Option<prefix::Checkpoint>,
}

fn blend(k: usize) -> BlendMode {
    use BlendMode::*;
    [
        SourceOver,
        SourceIn,
        SourceOut,
        SourceAtop,
        DestinationOver,
        DestinationIn,
        DestinationOut,
        DestinationAtop,
        Plus,
        Source,
        Xor,
        Multiply,
        Screen,
        Overlay,
        Darken,
        Lighten,
        ColorDodge,
        ColorBurn,
        HardLight,
        SoftLight,
        Difference,
        Exclusion,
        Hue,
        Saturation,
        Color,
        Luminosity,
    ]
    .get(k)
    .copied()
    .unwrap_or(SourceOver)
}

fn color(c: [f64; 4], alpha: f32) -> Color {
    Color::from_rgba(
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
        (c[3] as f32 * alpha).clamp(0.0, 1.0),
    )
    .unwrap_or(Color::TRANSPARENT)
}

/// What a replay needs beyond the list: the images and the text engine.
pub(crate) struct Env<'a> {
    pub images: &'a ImageCache,
    pub text: Option<&'a CanvasText>,
}

impl Replayer {
    fn new(w: u32, h: u32, scale: f64, lifetime: u64, generation: u32) -> Option<Replayer> {
        Some(Replayer {
            pixmap: Arc::new(Pixmap::new(w, h)?),
            update: CanvasUpdate::after(0),
            cleared: true,
            base: Transform::from_scale(scale as f32, scale as f32),
            scale: scale as f32,
            lifetime,
            generation,
            state: State::default(),
            stack: Vec::new(),
            path: PathBuilder::new(),
            scratch: PathBuilder::new(),
            objects: HashMap::new(),
            objects_epoch: 0,
            image_reads: None,
            last_draw: None,
            checkpoint: None,
        })
    }

    /// Clear the entire bitmap without cloning a shared snapshot first.
    pub(super) fn clear_all(&mut self) {
        if self.cleared {
            return;
        }
        self.damage_all();
        if let Some(pm) = Arc::get_mut(&mut self.pixmap) {
            pm.data_mut().fill(0);
        } else if let Some(pm) = Pixmap::new(self.pixmap.width(), self.pixmap.height()) {
            self.pixmap = Arc::new(pm);
        }
        self.cleared = true;
    }

    /// Mutable pixel buffer for an operation that overwrites every pixel in the
    /// bitmap: avoids copying a shared snapshot when allocating a fresh one.
    pub(super) fn overwrite_all_pixmap(&mut self) -> &mut Pixmap {
        if Arc::get_mut(&mut self.pixmap).is_none() {
            if let Some(pm) = Pixmap::new(self.pixmap.width(), self.pixmap.height()) {
                self.pixmap = Arc::new(pm);
            }
        }
        Arc::make_mut(&mut self.pixmap)
    }

    /// Zero a rectangle whose device edges cover whole pixels with no clip.
    fn clear_whole_pixels(&mut self, x: f64, y: f64, w: f64, h: f64) -> bool {
        if self.state.clip.is_some() {
            return false;
        }
        let ts = self.device();
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let Some((x0, y0, x1, y1)) = whole_pixel_rect(ts, x, y, w, h, pw, ph) else {
            return false;
        };
        if x0 >= x1 || y0 >= y1 || self.cleared {
            return true;
        }
        if x0 == 0 && y0 == 0 && x1 == pw as usize && y1 == ph as usize {
            self.clear_all();
            return true;
        }
        self.damage_edges(x0, y0, x1, y1);
        let stride = pw as usize * 4;
        let data = Arc::make_mut(&mut self.pixmap).data_mut();
        for row in y0..y1 {
            let start = row * stride + x0 * 4;
            let end = row * stride + x1 * 4;
            data[start..end].fill(0);
        }
        true
    }

    /// Fill an axis-aligned whole-pixel rectangle with a solid colour when
    /// source-over (or copy over the whole canvas) replaces pixels directly.
    fn fill_whole_pixels(&mut self, x: f64, y: f64, w: f64, h: f64) -> bool {
        let Style::Color(c) = self.state.fill else {
            return false;
        };
        if self.state.clip.is_some() || self.shadowed() {
            return false;
        }
        let mode = blend(self.state.composite);
        let copy = mode == BlendMode::Source;
        if mode != BlendMode::SourceOver && !copy {
            return false;
        }
        let ts = self.device();
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let Some((x0, y0, x1, y1)) = whole_pixel_rect(ts, x, y, w, h, pw, ph) else {
            return false;
        };
        let full = x0 == 0 && y0 == 0 && x1 == pw as usize && y1 == ph as usize;
        if copy && !full {
            return false;
        }
        let px = draw::premultiply_color(c, self.state.alpha);
        if !copy && px.alpha() != 255 && !self.cleared {
            return false;
        }
        if x0 >= x1 || y0 >= y1 {
            return true;
        }
        self.damage_edges(x0, y0, x1, y1);
        if full {
            if px.alpha() == 0 {
                self.clear_all();
            } else {
                self.overwrite_all_pixmap().pixels_mut().fill(px);
                self.cleared = false;
            }
            return true;
        }
        if px.alpha() == 0 && self.cleared {
            return true;
        }
        let width = pw as usize;
        let pixels = Arc::make_mut(&mut self.pixmap).pixels_mut();
        for row in y0..y1 {
            let start = row * width + x0;
            let end = row * width + x1;
            pixels[start..end].fill(px);
        }
        if px.alpha() != 0 {
            self.cleared = false;
        }
        true
    }

    fn device(&self) -> Transform {
        self.base.pre_concat(self.state.author)
    }

    fn stroke_style(&self) -> Stroke {
        Stroke {
            width: self.state.width,
            miter_limit: self.state.miter,
            line_cap: self.state.cap,
            line_join: self.state.join,
            dash: StrokeDash::new(self.state.dash.clone(), self.state.dash_offset),
        }
    }

    /// Fill a path in canvas coordinates with the fill style.
    fn fill_canvas_path(&mut self, path: &Path, rule: FillRule, env: &Env<'_>) {
        let ts = self.base;
        let source = self.source(self.state.fill, env);
        self.draw(Geom::Fill(path, rule, ts), source);
    }

    /// Stroke a path in canvas coordinates: back to user space, stroked
    /// under base ∘ author.
    fn stroke_canvas_path(&mut self, path: &Path, env: &Env<'_>) {
        if self.state.author.is_identity() {
            self.stroke_user_path(path, env);
            return;
        }
        let Some(inv) = self.state.author.invert() else {
            return;
        };
        if let Some(user) = path.clone().transform(inv) {
            self.stroke_user_path(&user, env);
        }
    }

    fn stroke_user_path(&mut self, user: &Path, env: &Env<'_>) {
        let stroke = self.stroke_style();
        let ts = self.device();
        let source = self.source(self.state.stroke, env);
        self.draw(Geom::Stroke(user, stroke, ts), source);
    }

    fn clip_path(&mut self, path: Option<Path>, rule: FillRule) {
        let clip = self.state.clip.get_or_insert_with(|| {
            let mut m = Mask::new(self.pixmap.width(), self.pixmap.height()).expect("sized");
            m.data_mut().fill(255);
            m
        });
        match path {
            Some(path) => clip.intersect_path(&path, rule, true, self.base),
            None => clip.data_mut().fill(0),
        }
    }

    fn rect_path(x: f64, y: f64, w: f64, h: f64) -> Option<Path> {
        let mut pb = PathBuilder::new();
        let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
        pb.move_to(x, y);
        pb.line_to(x + w, y);
        pb.line_to(x + w, y + h);
        pb.line_to(x, y + h);
        pb.close();
        pb.finish()
    }

    fn rule(v: f64) -> FillRule {
        if v == 1.0 {
            FillRule::EvenOdd
        } else {
            FillRule::Winding
        }
    }

    fn stops(&mut self, id: u32) -> Option<&mut Vec<(f32, [f64; 4])>> {
        match self.objects.get_mut(&id)? {
            Object::Linear(_, s) | Object::Radial(_, s) | Object::Conic(_, s) => Some(s),
            _ => None,
        }
    }

    fn text(&mut self, fill: bool, r: &list::Record<'_>, env: &Env<'_>) {
        let (Some(engine), Some(font)) = (env.text, self.state.font.clone()) else {
            return;
        };
        let text = text_at(r, 4);
        let shaped = engine.shape(&font, &text, r.at(3) == 1.0);
        let Some(path) = shaped.path else {
            return;
        };
        let (x, y, sx) = (r.at(0) as f32, r.at(1) as f32, r.at(2) as f32);
        let ts = self.device().pre_translate(x, y).pre_scale(sx, 1.0);
        if fill {
            let source = self.source(self.state.fill, env);
            self.draw(Geom::Fill(&path, FillRule::Winding, ts), source);
        } else {
            let stroke = self.stroke_style();
            let source = self.source(self.state.stroke, env);
            self.draw(Geom::Stroke(&path, stroke, ts), source);
        }
    }

    fn insert_object(&mut self, id: u32, obj: Object) {
        if self.objects.insert(id, obj).is_some() {
            self.objects_epoch = self.objects_epoch.wrapping_add(1);
        }
    }

    fn apply_record(&mut self, r: &list::Record<'_>, env: &Env<'_>) {
        let f = |i: usize| r.at(i) as f32;
        let c4 = |i: usize| [r.at(i), r.at(i + 1), r.at(i + 2), r.at(i + 3)];
        match r.op {
            Op::Save => self.stack.push(self.state.clone()),
            Op::Restore => {
                if let Some(s) = self.stack.pop() {
                    self.state = s;
                }
            }
            Op::Reset => {
                self.clear_all();
                self.state = State::default();
                self.stack.clear();
                self.path = PathBuilder::new();
                self.scratch = PathBuilder::new();
            }
            Op::SetTransform => {
                self.state.author = Transform::from_row(f(0), f(1), f(2), f(3), f(4), f(5))
            }
            Op::FillColor => self.state.fill = Style::Color(c4(0)),
            Op::FillGradient | Op::FillPattern => self.state.fill = Style::Object(r.at(0) as u32),
            Op::StrokeColor => self.state.stroke = Style::Color(c4(0)),
            Op::StrokeGradient | Op::StrokePattern => {
                self.state.stroke = Style::Object(r.at(0) as u32)
            }
            Op::LineWidth => self.state.width = f(0),
            Op::LineCap => {
                self.state.cap = [LineCap::Butt, LineCap::Round, LineCap::Square][r.at(0) as usize]
            }
            Op::LineJoin => {
                self.state.join =
                    [LineJoin::Miter, LineJoin::Round, LineJoin::Bevel][r.at(0) as usize]
            }
            Op::MiterLimit => self.state.miter = f(0),
            Op::LineDash => self.state.dash = r.operands().map(|v| v as f32).collect(),
            Op::LineDashOffset => self.state.dash_offset = f(0),
            Op::GlobalAlpha => self.state.alpha = f(0),
            Op::Composite => self.state.composite = r.at(0) as usize,
            Op::ShadowColor => self.state.shadow_color = c4(0),
            Op::ShadowBlur => self.state.shadow_blur = f(0),
            Op::ShadowOffset => self.state.shadow_offset = (f(0), f(1)),
            Op::ImageSmoothing => self.state.smoothing = (r.at(0) == 1.0, r.at(1) as u8),
            Op::LinearGradient => {
                let g = Object::Linear([f(1), f(2), f(3), f(4)], Vec::new());
                self.insert_object(r.at(0) as u32, g);
            }
            Op::RadialGradient => {
                let g = Object::Radial([f(1), f(2), f(3), f(4), f(5), f(6)], Vec::new());
                self.insert_object(r.at(0) as u32, g);
            }
            Op::ConicGradient => {
                let g = Object::Conic([f(1), f(2), f(3)], Vec::new());
                self.insert_object(r.at(0) as u32, g);
            }
            Op::ColorStop => {
                let at = f(1);
                let c = c4(2);
                if let Some(stops) = self.stops(r.at(0) as u32) {
                    // Stops at one offset keep their order (spec).
                    let i = stops.partition_point(|(o, _)| *o <= at);
                    stops.insert(i, (at, c));
                    self.objects_epoch = self.objects_epoch.wrapping_add(1);
                }
            }
            Op::Pattern => {
                let p = Object::Pattern {
                    image: r.at(1) as u32,
                    repetition: r.at(2) as u8,
                    transform: Transform::identity(),
                };
                self.insert_object(r.at(0) as u32, p);
            }
            Op::PatternTransform => {
                if let Some(Object::Pattern { transform, .. }) =
                    self.objects.get_mut(&(r.at(0) as u32))
                {
                    *transform = Transform::from_row(f(1), f(2), f(3), f(4), f(5), f(6));
                    self.objects_epoch = self.objects_epoch.wrapping_add(1);
                }
            }
            Op::Image => {
                self.insert_object(r.at(0) as u32, Object::Image(text_at(r, 1)));
            }
            Op::BeginPath => self.path = PathBuilder::new(),
            Op::MoveTo => self.path.move_to(f(0), f(1)),
            Op::LineTo => self.path.line_to(f(0), f(1)),
            Op::QuadTo => self.path.quad_to(f(0), f(1), f(2), f(3)),
            Op::CubicTo => self.path.cubic_to(f(0), f(1), f(2), f(3), f(4), f(5)),
            Op::ClosePath => self.path.close(),
            Op::PathMoveTo => self.scratch.move_to(f(0), f(1)),
            Op::PathLineTo => self.scratch.line_to(f(0), f(1)),
            Op::PathQuadTo => self.scratch.quad_to(f(0), f(1), f(2), f(3)),
            Op::PathCubicTo => self.scratch.cubic_to(f(0), f(1), f(2), f(3), f(4), f(5)),
            Op::PathClose => self.scratch.close(),
            Op::Fill => {
                if let Some(path) = self.path.clone().finish() {
                    self.fill_canvas_path(&path, Self::rule(r.at(0)), env);
                }
            }
            Op::FillPath => {
                if let Some(path) = std::mem::take(&mut self.scratch).finish() {
                    self.fill_canvas_path(&path, Self::rule(r.at(0)), env);
                }
            }
            Op::Stroke => {
                if let Some(path) = self.path.clone().finish() {
                    self.stroke_canvas_path(&path, env);
                }
            }
            Op::StrokePath => {
                if let Some(path) = std::mem::take(&mut self.scratch).finish() {
                    self.stroke_canvas_path(&path, env);
                }
            }
            Op::Clip => {
                let path = self.path.clone().finish();
                self.clip_path(path, Self::rule(r.at(0)));
            }
            Op::ClipPath => {
                let path = std::mem::take(&mut self.scratch).finish();
                self.clip_path(path, Self::rule(r.at(0)));
            }
            Op::FillRect => {
                if !self.fill_whole_pixels(r.at(0), r.at(1), r.at(2), r.at(3)) {
                    if let Some(p) = Self::rect_path(r.at(0), r.at(1), r.at(2), r.at(3)) {
                        let ts = self.device();
                        let source = self.source(self.state.fill, env);
                        self.draw(Geom::Fill(&p, FillRule::Winding, ts), source);
                    }
                }
            }
            Op::StrokeRect => {
                let (x, y, w, h) = (r.at(0), r.at(1), r.at(2), r.at(3));
                if w == 0.0 && h == 0.0 {
                    return;
                }
                let p = if w == 0.0 || h == 0.0 {
                    let mut pb = PathBuilder::new();
                    pb.move_to(x as f32, y as f32);
                    pb.line_to((x + w) as f32, (y + h) as f32);
                    pb.finish()
                } else {
                    Self::rect_path(x, y, w, h)
                };
                if let Some(p) = p {
                    self.stroke_user_path(&p, env);
                }
            }
            Op::ClearRect => {
                if !self.clear_whole_pixels(r.at(0), r.at(1), r.at(2), r.at(3)) {
                    if let Some(p) = Self::rect_path(r.at(0), r.at(1), r.at(2), r.at(3)) {
                        // Destination-out with opaque black clears what the
                        // rectangle covers; tiny-skia's `Clear` ignores the mask.
                        let paint = Paint {
                            shader: Shader::SolidColor(Color::BLACK),
                            blend_mode: BlendMode::DestinationOut,
                            anti_alias: true,
                            ..Default::default()
                        };
                        let ts = self.device();
                        self.damage_path(p.bounds().transform(ts));
                        Arc::make_mut(&mut self.pixmap).fill_path(
                            &p,
                            &paint,
                            FillRule::Winding,
                            ts,
                            self.state.clip.as_ref(),
                        );
                    }
                }
            }
            Op::Font => {
                self.state.font = Some(Arc::new(RunStyle {
                    size: f(0),
                    weight: r.at(1) as u16,
                    style: r.at(2) as u8,
                    stretch: f(3),
                    kerning: r.at(5) as u8,
                    letter_spacing: f(7),
                    word_spacing: f(8),
                    families: text_at(r, 9).split(',').map(str::to_string).collect(),
                }))
            }
            Op::FillText => self.text(true, r, env),
            Op::StrokeText => self.text(false, r, env),
            Op::DrawImage => self.draw_image(r, env),
            Op::PutImageData => self.put_image_data(r),
        }
    }

    fn apply(&mut self, bytes: &[u8], env: &Env<'_>) -> Result<(), String> {
        let recs = list::records(bytes).map_err(|e| e.to_string())?;
        for r in &recs {
            self.apply_record(r, env);
        }
        Ok(())
    }

    /// Apply all lists of one draw, using prefix checkpointing when a
    /// single-list full redraw repeats an unchanged prefix of paint ops.
    fn apply_draw(&mut self, lists: &[Vec<u8>], env: &Env<'_>) -> Result<(), String> {
        self.update = CanvasUpdate::after(self.update.revision);
        if lists.len() != 1
            || self.state.clip.is_some()
            || !self.stack.is_empty()
            || !self.scratch.is_empty()
        {
            self.last_draw = None;
            self.checkpoint = None;
            for l in lists {
                self.apply(l, env)?;
            }
            return Ok(());
        }
        let bytes = &lists[0];
        let recs = list::records(bytes).map_err(|e| e.to_string())?;
        let common_recs = self
            .last_draw
            .as_ref()
            .map_or(0, |prev| prefix::common_record_count(&prev.bytes, bytes));
        let (pw, ph) = (self.pixmap.width(), self.pixmap.height());
        let Some(plan) =
            prefix::scan_prefix(&recs, common_recs, pw, ph, self.base, self.state.author)
        else {
            self.last_draw = None;
            self.checkpoint = None;
            for r in &recs {
                self.apply_record(r, env);
            }
            return Ok(());
        };
        let text_ptr = env.text.map_or(0, |t| t as *const CanvasText as usize);
        if plan.checkpoint_recs == 0 {
            self.checkpoint = None;
            for r in &recs {
                self.apply_record(r, env);
            }
        } else {
            let entry_state = self.state.clone();
            let objects_epoch = self.objects_epoch;
            let reusable = self.checkpoint.as_ref().is_some_and(|cp| {
                cp.matches(
                    bytes,
                    plan.checkpoint_recs,
                    &entry_state,
                    objects_epoch,
                    text_ptr,
                    env,
                )
            });
            if reusable {
                let mut cp = self.checkpoint.take().unwrap();
                self.restore_tail(&cp);
                self.state = cp.state.clone();
                self.stack = cp.stack.clone();
                self.path = cp.path.clone();
                self.scratch = PathBuilder::new();
                self.cleared = cp.cleared;
                if plan.checkpoint_recs > cp.recs {
                    let mut reads = std::mem::take(&mut cp.images);
                    self.image_reads = Some(reads);
                    for r in &recs[cp.recs..plan.checkpoint_recs] {
                        self.apply_record(r, env);
                    }
                    reads = self.image_reads.take().unwrap_or_default();
                    cp.recs = plan.checkpoint_recs;
                    cp.bytes = bytes[..plan.checkpoint_bytes].to_vec();
                    cp.images = reads;
                    cp.pixmap = self.pixmap.clone();
                    cp.state = self.state.clone();
                    cp.stack = self.stack.clone();
                    cp.path = self.path.clone();
                    cp.cleared = self.cleared;
                }
                self.checkpoint = Some(cp);
            } else {
                self.checkpoint = None;
                self.image_reads = Some(Vec::new());
                for r in &recs[..plan.checkpoint_recs] {
                    self.apply_record(r, env);
                }
                let images = self.image_reads.take().unwrap_or_default();
                self.checkpoint = Some(prefix::Checkpoint {
                    recs: plan.checkpoint_recs,
                    bytes: bytes[..plan.checkpoint_bytes].to_vec(),
                    starts_with_reset: plan.starts_with_reset,
                    entry_state,
                    objects_epoch,
                    text_ptr,
                    images,
                    pixmap: self.pixmap.clone(),
                    tail: None,
                    state: self.state.clone(),
                    stack: self.stack.clone(),
                    path: self.path.clone(),
                    cleared: self.cleared,
                });
            }
            let prefix_damage = self.update.area.take();
            for r in &recs[plan.checkpoint_recs..] {
                self.apply_record(r, env);
            }
            self.checkpoint.as_mut().unwrap().tail = self.update.area;
            self.include_damage(prefix_damage);
        }
        self.last_draw = Some(prefix::LastDraw {
            bytes: bytes.to_vec(),
        });
        Ok(())
    }
}

/// A layout dimension arrives in points rounded to f32. Recover an integer
/// device edge when converting it back to points gives exactly that input.
/// Limit the extra snap to less than half an 8-bit coverage step, so a large
/// transform cannot turn a visibly fractional edge into a whole pixel.
fn device_edge(point: f64, scale: f32, offset: f32) -> f64 {
    let (scale, offset) = (f64::from(scale), f64::from(offset));
    let edge = point * scale + offset;
    let pixel = edge.round();
    let error = (edge - pixel).abs();
    if error <= 1e-4
        || (error < 0.5 / 255. && point == f64::from(((pixel - offset) / scale) as f32))
    {
        pixel
    } else {
        edge
    }
}

/// Clipped integer device-pixel rectangle `[x0, y0, x1, y1)` for an
/// axis-aligned rect `(x, y, w, h)` under `ts`, or `None` if rotated/skewed
/// or any in-bounds edge is not on a whole device pixel.
pub(super) fn whole_pixel_rect(
    ts: Transform,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    pw: u32,
    ph: u32,
) -> Option<(usize, usize, usize, usize)> {
    if ts.kx.abs() > 1e-6 || ts.ky.abs() > 1e-6 || ts.sx == 0.0 || ts.sy == 0.0 {
        return None;
    }
    let clip = |point, scale, offset, limit: u32| {
        let edge = device_edge(point, scale, offset).clamp(0., f64::from(limit));
        (edge.fract() == 0.).then_some(edge as usize)
    };
    let x0 = clip(x, ts.sx, ts.tx, pw)?;
    let x1 = clip(x + w, ts.sx, ts.tx, pw)?;
    let y0 = clip(y, ts.sy, ts.ty, ph)?;
    let y1 = clip(y + h, ts.sy, ts.ty, ph)?;
    Some((x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)))
}

pub(super) fn is_full_clear_rect(
    ts: Transform,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    pw: u32,
    ph: u32,
) -> bool {
    matches!(
        whole_pixel_rect(ts, x, y, w, h, pw, ph),
        Some((0, 0, x1, y1)) if x1 == pw as usize && y1 == ph as usize
    )
}

/// A whole recording replayed into a fresh bitmap of `size` points at
/// `scale` (a hatch's overlay, LLP 1075.003.000.001 §2.2.1): nothing is
/// carried from an earlier one. `None` for a box with no area.
pub(crate) fn replay(
    lists: &[Vec<u8>],
    size: (f32, f32),
    scale: f32,
    env: &Env<'_>,
) -> Result<Option<Arc<Pixmap>>, String> {
    let (w, h) = ((size.0 * scale).ceil(), (size.1 * scale).ceil());
    let side = crate::raster::MAX_FRAME_SIDE as f32;
    if !(w >= 1.0 && h >= 1.0) || w > side || h > side {
        return Ok(None);
    }
    let Some(mut replayer) = Replayer::new(w as u32, h as u32, f64::from(scale), 0, 0) else {
        return Ok(None);
    };
    for list in lists {
        replayer.apply(list, env)?;
    }
    Ok(Some(replayer.pixmap))
}

/// Every 2D canvas's replayer and snapshot, the decoded images, and the
/// text engine.
#[derive(Default)]
pub(crate) struct Canvases {
    completed: BTreeMap<ViewId, u64>,
    replayers: BTreeMap<ViewId, Replayer>,
    pub(crate) snapshots: CanvasSnapshots,
    pub(crate) images: ImageCache,
    pub(crate) text: Option<Arc<CanvasText>>,
}

impl Canvases {
    /// Apply the runner's stamped lists in order (LLP 1056 D4). A fresh one
    /// allocates the generation's bitmap, cleared; a zero size is none.
    pub(crate) fn apply(&mut self, lists: Vec<CanvasList>) -> Vec<String> {
        let mut errors = Vec::new();
        let mut touched = Vec::new();
        let env = Env {
            images: &self.images,
            text: self.text.as_deref(),
        };
        let mut snapshots = self.snapshots.borrow_mut();
        for c in lists {
            if c.fresh {
                self.completed.remove(&c.view);
                snapshots.remove(&c.view);
                match Replayer::new(
                    c.pixel_width,
                    c.pixel_height,
                    c.scale,
                    c.lifetime,
                    c.generation,
                ) {
                    Some(r) => {
                        self.replayers.insert(c.view, r);
                        touched.push(c.view);
                    }
                    None => {
                        self.replayers.remove(&c.view);
                    }
                }
            }
            let Some(r) = self.replayers.get_mut(&c.view) else {
                continue;
            };
            if r.lifetime != c.lifetime || r.generation != c.generation {
                continue;
            }
            // Its snapshot is replaced below: it holds the bitmap no longer.
            snapshots.remove(&c.view);
            if let Err(e) = r.apply_draw(&c.lists, &env) {
                errors.push(format!("canvas {}: {e}", c.view));
            }
            if c.seq > 0 {
                self.completed.insert(c.view, c.seq);
            }
            touched.push(c.view);
        }
        touched.dedup();
        for view in touched {
            if let Some(r) = self.replayers.get(&view) {
                snapshots.insert(
                    view,
                    CanvasPaint {
                        pixels: r.pixmap.clone(),
                        update: Some(r.update),
                    },
                );
            }
        }
        errors
    }

    /// Drop the canvases whose nodes are gone.
    pub(crate) fn retain(&mut self, live: &[ViewId]) {
        self.completed.retain(|v, _| live.contains(v));
        self.replayers.retain(|v, _| live.contains(v));
        self.snapshots.borrow_mut().retain(|v, _| live.contains(v));
    }
}

#[cfg(test)]
mod tests;
