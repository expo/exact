//! A canvas's lists (LLP 1056 D3) replayed into a vello scene, with the
//! reference replayer's semantics (`host/apple/Sources/ExactKit/
//! Canvas2D*.swift`): paths arrive in canvas coordinates, so a fill or clip
//! draws them under the base scale; a stroke takes them back to user space
//! and strokes under base ∘ author, so its width follows the author's
//! matrix. Paints go through `paint.rs`: global alpha, the operators, the
//! shadows. The state, the save stack, the clips, the current path and the
//! gradients, patterns and image handles persist from replay to replay, as
//! a web canvas keeps them.

use crate::paint::{Geom, Style};
use exact_canvas::list::{self, text_at, Op, Record};
use std::collections::{HashMap, VecDeque};
use vello::kurbo::{Affine, BezPath, Cap, Join, Rect, Shape};
use vello::peniko::{Color, Fill, ImageData};
use vello::Scene;

/// What a replay reads beyond its lists: the host's text and images.
pub trait Host {
    /// A text line's outlines (LLP 1056 ABI `text_path`): line space, y up,
    /// at the run's left end on its alphabetic baseline.
    fn text(&mut self, font: &[f64], text: &[u32], rtl: bool) -> Option<BezPath>;
    /// An image handle's pixels, premultiplied, or none when not decoded.
    fn image(&mut self, src: &str) -> Option<ImageData>;
}

/// The state a paint applies (`Canvas2DState`).
#[derive(Clone, Debug)]
pub struct State {
    pub author: Affine,
    pub fill: Style,
    pub stroke: Style,
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
    pub miter: f64,
    pub dash: Vec<f64>,
    pub dash_offset: f64,
    pub alpha: f64,
    pub composite: usize,
    pub shadow_color: [f64; 4],
    pub shadow_blur: f64,
    pub shadow_offset: (f64, f64),
    pub smoothing: bool,
    pub quality: u8,
    pub font: Option<std::rc::Rc<[f64]>>,
    /// Clip layers pushed since the matching `save`.
    pub clips: usize,
}

impl Default for State {
    fn default() -> Self {
        State {
            author: Affine::IDENTITY,
            fill: Style::Color([0.0, 0.0, 0.0, 1.0]),
            stroke: Style::Color([0.0, 0.0, 0.0, 1.0]),
            width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            miter: 10.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            alpha: 1.0,
            composite: 0,
            shadow_color: [0.0; 4],
            shadow_blur: 0.0,
            shadow_offset: (0.0, 0.0),
            smoothing: true,
            quality: 0,
            font: None,
            clips: 0,
        }
    }
}

/// A gradient: its geometry and stops (offset, colour), kept sorted.
#[derive(Clone, Debug)]
pub enum Gradient {
    Linear([f64; 4], Vec<(f64, [f64; 4])>),
    Radial([f64; 6], Vec<(f64, [f64; 4])>),
    Conic([f64; 3], Vec<(f64, [f64; 4])>),
}

/// A pattern: an image handle, its repetition and its transform.
#[derive(Clone, Debug)]
pub struct Pattern {
    pub image: u32,
    pub repetition: u8,
    pub transform: Affine,
}

/// A shadow a paint needs drawn offscreen and blurred before the scene
/// renders (`paint.rs` makes them).
pub struct ShadowJob {
    /// The shape's paint at its alpha, device pixels, offset to the origin.
    pub scene: Scene,
    pub width: u32,
    pub height: u32,
    /// Standard deviation, device pixels.
    pub sigma: f64,
    /// The shadow colour, non-premultiplied 0–1.
    pub color: [f32; 4],
    /// The placeholder the main scene draws; the blurred texture stands in.
    pub image: ImageData,
}

/// One replay's output.
pub struct Frame {
    pub scene: Scene,
    /// The canvas's pixels before this replay still show: the scene draws
    /// over them.
    pub keep: bool,
    pub shadows: Vec<ShadowJob>,
    /// Clip masks for later replays (`Replayer::close`); each reads only
    /// masks kept from earlier replays.
    pub masks: Vec<MaskJob>,
}

/// A persistent clip's coverage, rendered once and reused across frames.
pub struct MaskJob {
    pub scene: Scene,
    pub slot: u32,
}

enum Clip {
    Path(BezPath, Fill),
    /// `paths`: the keys (`clip_key`) of the last `MASK_PATHS` paths folded
    /// in or clipped again, least recent first.
    Mask {
        slot: u32,
        paths: VecDeque<Vec<u64>>,
    },
}

impl Clip {
    fn push(&self, scene: &mut Scene, base: Affine, whole: Rect) {
        match self {
            Self::Path(path, fill) => scene.push_clip_layer(*fill, base, path),
            Self::Mask { slot, .. } => {
                scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &whole);
                // Unlike a path clip, this layer must survive even in a
                // tile wholly inside its rectangle: the buffer supplies
                // the coverage. Coarse and fine recognize this marker.
                let data = &mut scene.encoding_mut().draw_data;
                let blend = data.len() - 2;
                data[blend] = 0x20000;
                data[blend + 1] = (*slot as f32).to_bits();
            }
        }
    }

    fn pop(&self, scene: &mut Scene) {
        scene.pop_layer();
    }
}

const CLIPS_PER_MASK: usize = 8;
/// A path clipped again while a mask remembers it is skipped, and moves to
/// the newest end; one the mask has forgotten intersects again, which keeps
/// the region and only lowers the coverage along its antialiased edge.
const MASK_PATHS: usize = 64;

/// One canvas's replay state.
pub struct Replayer {
    /// Canvas units to backing pixels.
    pub scale: f64,
    /// Backing size, pixels.
    pub width: u32,
    pub height: u32,
    pub state: State,
    stack: Vec<State>,
    path: BezPath,
    scratch: BezPath,
    pub gradients: HashMap<u32, Gradient>,
    pub patterns: HashMap<u32, Pattern>,
    pub images: HashMap<u32, String>,
    /// Clips in force, outermost first; paths are in canvas coordinates,
    /// masks in backing pixels. Each save level compacts independently.
    open: Vec<Clip>,
    /// The slot the next mask renders into: the masks kept from earlier
    /// replays hold the slots below it.
    next_mask: u32,
    /// What could not be drawn, by op name.
    pub unsupported: Vec<&'static str>,
}

fn rule(v: f64) -> Fill {
    if v == 1.0 {
        Fill::EvenOdd
    } else {
        Fill::NonZero
    }
}

impl Replayer {
    pub fn new(width: u32, height: u32, scale: f64) -> Replayer {
        Replayer {
            scale,
            width,
            height,
            state: State::default(),
            stack: Vec::new(),
            path: BezPath::new(),
            scratch: BezPath::new(),
            gradients: HashMap::new(),
            patterns: HashMap::new(),
            images: HashMap::new(),
            open: Vec::new(),
            next_mask: 0,
            unsupported: Vec::new(),
        }
    }

    /// Canvas coordinates to device pixels.
    pub fn base(&self) -> Affine {
        Affine::scale(self.scale)
    }

    /// User space to device pixels.
    pub fn device(&self) -> Affine {
        self.base() * self.state.author
    }

    /// The canvas in device pixels.
    pub fn whole(&self) -> Rect {
        Rect::new(0.0, 0.0, f64::from(self.width), f64::from(self.height))
    }

    pub fn note(&mut self, what: &'static str) {
        if !self.unsupported.contains(&what) {
            self.unsupported.push(what);
        }
    }

    /// A clip is in force.
    pub fn clipped(&self) -> bool {
        !self.open.is_empty()
    }

    /// Start a scene over the kept pixels: the clips in force pushed again.
    pub fn begin(&self, scene: &mut Scene) {
        for clip in &self.open {
            clip.push(scene, self.base(), self.whole());
        }
    }

    /// Close the clip layers still open at the end of a scene.
    pub fn end(&self, scene: &mut Scene) {
        for clip in self.open.iter().rev() {
            clip.pop(scene);
        }
    }

    pub(crate) fn mask_count(&self) -> u32 {
        self.next_mask
    }

    /// Densely number the retained masks; GPU coverage follows the same move.
    pub(crate) fn finish_masks(&mut self) -> Vec<u32> {
        let mut slots = Vec::new();
        for clip in &mut self.open {
            if let Clip::Mask { slot, .. } = clip {
                let old = *slot;
                *slot = slots.len() as u32;
                slots.push(old);
            }
        }
        self.next_mask = slots.len() as u32;
        slots
    }

    /// Replay one list into `frame`; an unreadable list applies nothing.
    pub fn apply(
        &mut self,
        bytes: &[u8],
        frame: &mut Frame,
        host: &mut dyn Host,
    ) -> Result<(), String> {
        let recs = list::records(bytes).map_err(|e| e.to_string())?;
        for r in recs {
            self.record(&r, frame, host);
        }
        Ok(())
    }

    /// Drop what the scene drew: something now covers every pixel.
    fn restart(&self, frame: &mut Frame) {
        frame.scene.reset();
        frame.shadows.clear();
        frame.keep = false;
        self.begin(&mut frame.scene);
    }

    fn pop_clips(&mut self, scene: &mut Scene, n: usize) {
        for _ in 0..n.min(self.open.len()) {
            self.open.pop().unwrap().pop(scene);
        }
    }

    /// `clip()`: the region becomes its intersection with `path`. A path
    /// already in force leaves it unchanged, and a rectangle over the
    /// rectangle this save level opened last replaces that one with their
    /// intersection. Other paths add a layer until `close` compacts them.
    fn clip(&mut self, scene: &mut Scene, path: BezPath, fill: Fill) {
        let key = self
            .open
            .iter()
            .any(|clip| matches!(clip, Clip::Mask { .. }))
            .then(|| clip_key(&path, fill));
        for clip in &mut self.open {
            match clip {
                Clip::Path(p, f) if *p == path && *f == fill => return,
                Clip::Path(..) => {}
                Clip::Mask { paths, .. } => {
                    let at = key.as_ref().and_then(|k| paths.iter().position(|p| p == k));
                    if let Some(at) = at {
                        let k = paths.remove(at).expect("a remembered path");
                        paths.push_back(k);
                        return;
                    }
                }
            }
        }
        let last = if self.state.clips > 0 {
            self.open.last()
        } else {
            None
        };
        let folded = match (last, rect_of(&path)) {
            (Some(Clip::Path(last, _)), Some(r)) => rect_of(last).map(|l| l.intersect(r)),
            _ => None,
        };
        if let Some(r) = folded {
            scene.pop_layer();
            self.open.pop();
            self.state.clips -= 1;
            let path = r.to_path(0.1);
            scene.push_clip_layer(Fill::NonZero, self.base(), &path);
            self.open.push(Clip::Path(path, Fill::NonZero));
        } else {
            scene.push_clip_layer(fill, self.base(), &path);
            self.open.push(Clip::Path(path, fill));
        }
        self.state.clips += 1;
    }

    /// End the replay's scene, then fold each save level holding more than
    /// `CLIPS_PER_MASK` clips into one mask that later replays push instead.
    /// This replay's scene keeps its own layers, so a mask is made only for
    /// clips that outlive the replay, and at most one per save level: a
    /// level that clips without `save` every frame keeps a bounded stack.
    pub fn close(&mut self, frame: &mut Frame) {
        self.end(&mut frame.scene);
        let (base, whole) = (self.base(), self.whole());
        let mut open = std::mem::take(&mut self.open).into_iter();
        let levels = self
            .stack
            .iter_mut()
            .map(|s| &mut s.clips)
            .chain([&mut self.state.clips]);
        for clips in levels {
            let level: Vec<Clip> = open.by_ref().take(*clips).collect();
            if level.len() <= CLIPS_PER_MASK {
                self.open.extend(level);
                continue;
            }
            let slot = self.next_mask;
            self.next_mask += 1;
            let (job, mask) = compact(level, slot, base, whole);
            frame.masks.push(job);
            self.open.push(mask);
            *clips = 1;
        }
        debug_assert!(open.next().is_none(), "clip counts cover the open clips");
    }

    /// Whether a `fillRect` (or `clearRect`) of `r` now covers every pixel
    /// (`Canvas2DRecord.swift` `covers`).
    fn covers(&self, r: Rect, clear: bool) -> bool {
        let s = &self.state;
        let m = s.author.as_coeffs();
        if m[1] != 0.0 || m[2] != 0.0 || self.clipped() {
            return false;
        }
        if !clear {
            let Style::Color(c) = s.fill else {
                return false;
            };
            if c[3] != 1.0 || s.alpha != 1.0 || s.composite != 0 {
                return false;
            }
        }
        let b = self.device().transform_rect_bbox(r.abs());
        b.x0 <= 0.0
            && b.y0 <= 0.0
            && b.x1 >= f64::from(self.width)
            && b.y1 >= f64::from(self.height)
    }

    #[allow(clippy::too_many_lines)]
    fn record(&mut self, r: &Record<'_>, frame: &mut Frame, host: &mut dyn Host) {
        let c4 = |i: usize| [r.at(i), r.at(i + 1), r.at(i + 2), r.at(i + 3)];
        let pt = |i: usize| (r.at(i), r.at(i + 1));
        let rect = || Rect::new(r.at(0), r.at(1), r.at(0) + r.at(2), r.at(1) + r.at(3));
        match r.op {
            Op::Save => {
                self.stack.push(self.state.clone());
                self.state.clips = 0;
            }
            Op::Restore => {
                if let Some(s) = self.stack.pop() {
                    let n = self.state.clips;
                    self.pop_clips(&mut frame.scene, n);
                    self.state = s;
                }
            }
            Op::Reset => {
                self.pop_clips(&mut frame.scene, self.open.len());
                self.state = State::default();
                self.stack.clear();
                self.path = BezPath::new();
                self.scratch = BezPath::new();
                self.restart(frame);
            }
            Op::SetTransform => {
                self.state.author =
                    Affine::new([r.at(0), r.at(1), r.at(2), r.at(3), r.at(4), r.at(5)])
            }
            Op::FillColor => self.state.fill = Style::Color(c4(0)),
            Op::FillGradient => self.state.fill = Style::Gradient(r.at(0) as u32),
            Op::FillPattern => self.state.fill = Style::Pattern(r.at(0) as u32),
            Op::StrokeColor => self.state.stroke = Style::Color(c4(0)),
            Op::StrokeGradient => self.state.stroke = Style::Gradient(r.at(0) as u32),
            Op::StrokePattern => self.state.stroke = Style::Pattern(r.at(0) as u32),
            Op::LineWidth => self.state.width = r.at(0),
            Op::LineCap => {
                self.state.cap = [Cap::Butt, Cap::Round, Cap::Square][(r.at(0) as usize).min(2)]
            }
            Op::LineJoin => {
                self.state.join = [Join::Miter, Join::Round, Join::Bevel][(r.at(0) as usize).min(2)]
            }
            Op::MiterLimit => self.state.miter = r.at(0),
            Op::LineDash => self.state.dash = r.operands().collect(),
            Op::LineDashOffset => self.state.dash_offset = r.at(0),
            Op::GlobalAlpha => self.state.alpha = r.at(0),
            Op::Composite => self.state.composite = (r.at(0) as usize).min(25),
            Op::ShadowColor => self.state.shadow_color = c4(0),
            Op::ShadowBlur => self.state.shadow_blur = r.at(0),
            Op::ShadowOffset => self.state.shadow_offset = (r.at(0), r.at(1)),
            Op::ImageSmoothing => {
                self.state.smoothing = r.at(0) != 0.0;
                self.state.quality = (r.at(1) as u8).min(2);
            }
            Op::LinearGradient => {
                let g = Gradient::Linear([r.at(1), r.at(2), r.at(3), r.at(4)], Vec::new());
                self.gradients.insert(r.at(0) as u32, g);
            }
            Op::RadialGradient => {
                let g = Gradient::Radial(
                    [r.at(1), r.at(2), r.at(3), r.at(4), r.at(5), r.at(6)],
                    Vec::new(),
                );
                self.gradients.insert(r.at(0) as u32, g);
            }
            Op::ConicGradient => {
                self.gradients.insert(
                    r.at(0) as u32,
                    Gradient::Conic([r.at(1), r.at(2), r.at(3)], Vec::new()),
                );
            }
            Op::ColorStop => {
                let (at, c) = (r.at(1), c4(2));
                if let Some(
                    Gradient::Linear(_, s) | Gradient::Radial(_, s) | Gradient::Conic(_, s),
                ) = self.gradients.get_mut(&(r.at(0) as u32))
                {
                    let i = s.partition_point(|(o, _)| *o <= at);
                    s.insert(i, (at, c));
                }
            }
            Op::Pattern => {
                let p = Pattern {
                    image: r.at(1) as u32,
                    repetition: r.at(2) as u8,
                    transform: Affine::IDENTITY,
                };
                self.patterns.insert(r.at(0) as u32, p);
            }
            Op::PatternTransform => {
                if let Some(p) = self.patterns.get_mut(&(r.at(0) as u32)) {
                    p.transform =
                        Affine::new([r.at(1), r.at(2), r.at(3), r.at(4), r.at(5), r.at(6)]);
                }
            }
            Op::Image => {
                self.images.insert(r.at(0) as u32, text_at(r, 1));
            }
            Op::BeginPath => self.path = BezPath::new(),
            Op::MoveTo => self.path.move_to(pt(0)),
            Op::LineTo => line_to(&mut self.path, pt(0)),
            Op::QuadTo => {
                ensure_start(&mut self.path, pt(0));
                self.path.quad_to(pt(0), pt(2))
            }
            Op::CubicTo => {
                ensure_start(&mut self.path, pt(0));
                self.path.curve_to(pt(0), pt(2), pt(4))
            }
            Op::ClosePath => close(&mut self.path),
            Op::PathMoveTo => self.scratch.move_to(pt(0)),
            Op::PathLineTo => line_to(&mut self.scratch, pt(0)),
            Op::PathQuadTo => {
                ensure_start(&mut self.scratch, pt(0));
                self.scratch.quad_to(pt(0), pt(2))
            }
            Op::PathCubicTo => {
                ensure_start(&mut self.scratch, pt(0));
                self.scratch.curve_to(pt(0), pt(2), pt(4))
            }
            Op::PathClose => close(&mut self.scratch),
            Op::Fill | Op::FillPath => {
                let path = if r.op == Op::Fill {
                    self.path.clone()
                } else {
                    std::mem::take(&mut self.scratch)
                };
                if path.elements().is_empty() {
                    return;
                }
                let base = self.base();
                let style = self.state.fill.clone();
                self.paint(frame, host, Geom::Fill(&path, rule(r.at(0)), base), &style);
            }
            Op::Stroke | Op::StrokePath => {
                let path = if r.op == Op::Stroke {
                    self.path.clone()
                } else {
                    std::mem::take(&mut self.scratch)
                };
                if path.elements().is_empty() {
                    return;
                }
                let inv = self.state.author.inverse();
                if !inv.is_finite() {
                    return;
                }
                self.stroke_user(frame, host, &(inv * path));
            }
            Op::Clip | Op::ClipPath => {
                let path = if r.op == Op::Clip {
                    self.path.clone()
                } else {
                    std::mem::take(&mut self.scratch)
                };
                self.clip(&mut frame.scene, path, rule(r.at(0)));
            }
            Op::FillRect => {
                let rc = rect();
                if self.covers(rc, false) {
                    self.restart(frame);
                }
                let t = self.device();
                let style = self.state.fill.clone();
                let p = rc.abs().to_path(0.1);
                self.paint(frame, host, Geom::Fill(&p, Fill::NonZero, t), &style);
            }
            Op::StrokeRect => {
                let (x, y, w, h) = (r.at(0), r.at(1), r.at(2), r.at(3));
                if w == 0.0 && h == 0.0 {
                    return;
                }
                let p = if w == 0.0 || h == 0.0 {
                    let mut b = BezPath::new();
                    b.move_to((x, y));
                    b.line_to((x + w, y + h));
                    b
                } else {
                    let mut b = BezPath::new();
                    b.move_to((x, y));
                    b.line_to((x + w, y));
                    b.line_to((x + w, y + h));
                    b.line_to((x, y + h));
                    b.close_path();
                    b
                };
                self.stroke_user(frame, host, &p);
            }
            Op::ClearRect => {
                let rc = rect();
                if self.covers(rc, true) {
                    self.restart(frame);
                    return;
                }
                self.clear(frame, rc);
            }
            Op::Font => self.state.font = Some(r.operands().collect::<Vec<_>>().into()),
            Op::FillText | Op::StrokeText => {
                if r.len() < 4 {
                    return;
                }
                let Some(font) = self.state.font.clone() else {
                    return;
                };
                let text: Vec<u32> = (4..r.len()).map(|i| r.at(i) as u32).collect();
                let Some(outline) = host.text(&font, &text, r.at(3) != 0.0) else {
                    return;
                };
                let t = self.device()
                    * Affine::translate((r.at(0), r.at(1)))
                    * Affine::scale_non_uniform(r.at(2), 1.0)
                    * Affine::FLIP_Y;
                if r.op == Op::FillText {
                    let style = self.state.fill.clone();
                    self.paint(frame, host, Geom::Fill(&outline, Fill::NonZero, t), &style);
                } else {
                    let stroke = self.stroke_style();
                    let style = self.state.stroke.clone();
                    self.paint(frame, host, Geom::Stroke(&outline, stroke, t), &style);
                }
            }
            Op::DrawImage => self.draw_image(frame, host, r),
            Op::PutImageData => self.put_image_data(frame, r),
        }
    }

    /// Stroke `user` (user space) with the stroke style under base ∘ author.
    fn stroke_user(&mut self, frame: &mut Frame, host: &mut dyn Host, user: &BezPath) {
        let stroke = self.stroke_style();
        let t = self.device();
        let style = self.state.stroke.clone();
        self.paint(frame, host, Geom::Stroke(user, stroke, t), &style);
    }

    /// `clearRect`: transparent black over the rectangle, under the author
    /// matrix and the clip; no alpha, operator or shadow.
    fn clear(&mut self, frame: &mut Frame, rc: Rect) {
        use vello::peniko::{BlendMode, Color, Compose, Mix};
        let t = self.device();
        let shape = rc.abs().to_path(0.1);
        let bbox = t.transform_rect_bbox(rc.abs()).intersect(self.whole());
        if bbox.is_zero_area() {
            return;
        }
        frame.scene.push_layer(
            Fill::NonZero,
            BlendMode::new(Mix::Normal, Compose::DestOut),
            1.0,
            Affine::IDENTITY,
            &bbox,
        );
        frame
            .scene
            .fill(Fill::NonZero, t, Color::BLACK, None, &shape);
        frame.scene.pop_layer();
    }

    /// `putImageData`: raw backing pixels over the dirty rectangle, with no
    /// transform, clip, alpha, operator or shadow (`Compose::Copy`).
    fn put_image_data(&mut self, frame: &mut Frame, r: &Record<'_>) {
        use vello::peniko::{
            BlendMode, Blob, Compose, ImageAlphaType, ImageBrush, ImageFormat, ImageQuality, Mix,
        };
        let (x, y, w, h) = (
            r.at(0),
            r.at(1),
            r.at(2).max(0.0) as u32,
            r.at(3).max(0.0) as u32,
        );
        if w == 0 || h == 0 || r.len() < 4 + (w * h) as usize {
            return;
        }
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for i in 0..(w * h) as usize {
            px.extend_from_slice(&(r.at(4 + i) as u32).to_be_bytes());
        }
        let image = ImageData {
            data: Blob::new(std::sync::Arc::new(px)),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: w,
            height: h,
        };
        let scene = &mut frame.scene;
        self.end(scene);
        // Copy as a clear then a source-over draw: a `Compose::Copy` layer
        // cleared whole tiles around the rectangle (fx-pixels on dark).
        let at = Rect::new(x, y, x + f64::from(w), y + f64::from(h));
        scene.push_layer(
            Fill::NonZero,
            BlendMode::new(Mix::Normal, Compose::DestOut),
            1.0,
            Affine::IDENTITY,
            &at,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            vello::peniko::Color::BLACK,
            None,
            &at,
        );
        scene.pop_layer();
        let brush = ImageBrush::new(image).with_quality(ImageQuality::Low);
        scene.draw_image(&brush, Affine::translate((x, y)));
        self.begin(scene);
    }

    /// `drawImage`: `image sx sy sw sh dx dy dw dh`, the source clipped to
    /// the image, the destination in user space.
    fn draw_image(&mut self, frame: &mut Frame, host: &mut dyn Host, r: &Record<'_>) {
        let Some(src) = self.images.get(&(r.at(0) as u32)).cloned() else {
            return;
        };
        let Some(image) = host.image(&src) else {
            return;
        };
        let (sx, sy, sw, sh) = (r.at(1), r.at(2), r.at(3), r.at(4));
        let dest = Rect::new(r.at(5), r.at(6), r.at(5) + r.at(7), r.at(6) + r.at(8));
        if sw <= 0.0 || sh <= 0.0 || dest.width() == 0.0 || dest.height() == 0.0 {
            return;
        }
        let (kx, ky) = (dest.width() / sw, dest.height() / sh);
        // Image pixels to user space: the whole image, placed so the source
        // rectangle lands on the destination.
        let to_user = Affine::new([kx, 0.0, 0.0, ky, dest.x0 - sx * kx, dest.y0 - sy * ky]);
        let t = self.device();
        let shape = dest.abs().to_path(0.1);
        self.paint(
            frame,
            host,
            Geom::Image(&shape, t, image, to_user),
            &Style::Color([0.0; 4]),
        );
    }

    /// The stroke with the state's line style; an all-zero dash list is solid.
    pub fn stroke_style(&self) -> vello::kurbo::Stroke {
        let s = &self.state;
        let mut stroke = vello::kurbo::Stroke::new(s.width)
            .with_caps(s.cap)
            .with_join(s.join)
            .with_miter_limit(s.miter);
        if s.dash.iter().any(|v| *v != 0.0) {
            stroke = stroke.with_dashes(s.dash_offset, s.dash.iter().copied());
        }
        stroke
    }
}

/// `lineTo` on an empty path is a `moveTo` (as `CGMutablePath` takes it).
fn line_to(p: &mut BezPath, to: (f64, f64)) {
    if p.elements().is_empty() {
        p.move_to(to);
    } else {
        p.line_to(to);
    }
}

/// A curve on an empty path starts at its first control point (the
/// recorder resolves the spec's rule; this keeps kurbo's path well formed).
fn ensure_start(p: &mut BezPath, at: (f64, f64)) {
    if p.elements().is_empty() {
        p.move_to(at);
    }
}

/// The axis-aligned rectangle `p` encloses, when it is one closed subpath
/// of four corners (a trailing `moveTo`, as `rect()` leaves, draws nothing).
fn rect_of(p: &BezPath) -> Option<Rect> {
    use vello::kurbo::{PathEl, Point};
    let mut pts: Vec<Point> = Vec::with_capacity(5);
    let mut closed = false;
    for el in p.elements() {
        match (*el, closed) {
            (PathEl::MoveTo(a), false) if pts.is_empty() => pts.push(a),
            (PathEl::LineTo(a), false) if (1..5).contains(&pts.len()) => pts.push(a),
            (PathEl::ClosePath, false) => closed = true,
            (PathEl::MoveTo(_), true) => {}
            _ => return None,
        }
    }
    if pts.len() == 5 && pts[4] == pts[0] {
        pts.pop();
    }
    if !closed || pts.len() != 4 {
        return None;
    }
    // Edges alternately horizontal and vertical, none of zero length.
    let across = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        a.y == b.y && a.x != b.x
    };
    let down = |i: usize| {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        a.x == b.x && a.y != b.y
    };
    let rect = (across(0) && down(1) && across(2) && down(3))
        || (down(0) && across(1) && down(2) && across(3));
    rect.then(|| Rect::from_points(pts[0], pts[2]))
}

fn close(p: &mut BezPath) {
    if !p.elements().is_empty() {
        p.close_path();
    }
}

/// The coverage of `clips` (one save level, outermost first) as a mask job
/// rendering into `slot`, and the clip that stands for them.
fn compact(clips: Vec<Clip>, slot: u32, base: Affine, whole: Rect) -> (MaskJob, Clip) {
    let mut scene = Scene::new();
    for clip in &clips {
        clip.push(&mut scene, base, whole);
    }
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &whole);
    for clip in clips.iter().rev() {
        clip.pop(&mut scene);
    }
    // A level's mask is its first clip, so its paths are the oldest.
    let mut paths = VecDeque::new();
    for clip in clips {
        match clip {
            Clip::Path(path, fill) => paths.push_back(clip_key(&path, fill)),
            Clip::Mask {
                paths: previous, ..
            } => paths.extend(previous),
        }
    }
    paths.drain(..paths.len().saturating_sub(MASK_PATHS));
    (MaskJob { scene, slot }, Clip::Mask { slot, paths })
}

/// Exact path identities survive rasterization so clipping a path twice is
/// still idempotent. Normalize signed zero as `BezPath` equality does.
fn clip_key(path: &BezPath, fill: Fill) -> Vec<u64> {
    use vello::kurbo::PathEl;
    let mut key = vec![u64::from(fill == Fill::EvenOdd)];
    for element in path.elements() {
        let origin = vello::kurbo::Point::ORIGIN;
        let (tag, points, count) = match *element {
            PathEl::MoveTo(p) => (0, [p, origin, origin], 1),
            PathEl::LineTo(p) => (1, [p, origin, origin], 1),
            PathEl::QuadTo(a, b) => (2, [a, b, origin], 2),
            PathEl::CurveTo(a, b, c) => (3, [a, b, c], 3),
            PathEl::ClosePath => (4, [origin; 3], 0),
        };
        key.push(tag);
        for p in &points[..count] {
            key.extend([p.x, p.y].map(|v| if v == 0.0 { 0 } else { v.to_bits() }));
        }
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoHost;

    impl Host for NoHost {
        fn text(&mut self, _: &[f64], _: &[u32], _: bool) -> Option<BezPath> {
            None
        }
        fn image(&mut self, _: &str) -> Option<ImageData> {
            None
        }
    }

    fn circle(ctx: &exact_canvas::Context2d, x: f64) {
        ctx.begin_path();
        ctx.arc(x, 50.0, 40.0, 0.0, std::f64::consts::TAU).unwrap();
        ctx.clip();
    }

    /// A curve clipped every frame stays skipped however many different
    /// curves are folded into the mask after it, and the mask remembers a
    /// bounded number of paths.
    #[test]
    fn a_mask_remembers_a_bounded_number_of_paths_and_keeps_the_recurring_ones() {
        let ctx = exact_canvas::Context2d::new();
        let mut replayer = Replayer::new(200, 100, 1.0);
        let mut recurring = None;
        for i in 0..600 {
            circle(&ctx, 100.0);
            circle(&ctx, 70.0 + i as f64 / 100.0);
            let mut frame = Frame {
                scene: Scene::new(),
                keep: true,
                shadows: Vec::new(),
                masks: Vec::new(),
            };
            replayer.begin(&mut frame.scene);
            for list in ctx.take_lists() {
                replayer.apply(&list, &mut frame, &mut NoHost).unwrap();
            }
            replayer.close(&mut frame);
            let recurring = recurring.get_or_insert_with(|| match &replayer.open[0] {
                Clip::Path(path, _) => path.clone(),
                Clip::Mask { .. } => unreachable!("frame 0 clips paths"),
            });
            let again = replayer.open.iter().skip(1).any(|clip| match clip {
                Clip::Path(path, _) => path == recurring,
                Clip::Mask { .. } => false,
            });
            assert!(!again, "frame {i}: the recurring curve was clipped again");
        }
        let remembered: usize = replayer
            .open
            .iter()
            .map(|clip| match clip {
                Clip::Mask { paths, .. } => paths.len(),
                Clip::Path(..) => 0,
            })
            .sum();
        assert!(remembered <= 64, "the mask remembers {remembered} paths");
    }
}
