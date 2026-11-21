//! @ref LLP 1015 §2; LLP 1014 D4 — the kernel tree is the painter's display list.
//!
//! One preorder walk paints node chrome, replaced content and measured text through [`Backend`].
//! Children inherit overflow clips and scroll offsets; motion supplies `transform-origin`
//! transforms and group opacity layers. Viewport boxes and clips serve agent layout and hit-testing.
//!
//! Backends: [`crate::gpu`] (vello), [`crate::ink`] (instanced geometry),
//! and [`crate::raster`] (the CPU fallback and pixel oracle).

use crate::image::Bitmap;
use crate::text::{Paragraph, Run, RunPaint, Shared, Spec, TextEngine};
use exact_kernel::style::{BackdropFilter, BackdropOp};
use exact_kernel::{
    BorderStyle, Dimension, Display, Kernel, NodeRef, NodeType, ObjectFit, Overflow, PropId,
    StyleId, StyleMask, StyleProps, ViewId,
};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;
use tiny_skia::{Pixmap, Point, Transform};
mod backend;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "android", test))]
mod presentation;
mod rotation;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "android", test))]
pub(crate) use presentation::Presentation;
pub mod border;
mod shape;
pub use shape::Shape;
pub(crate) use shape::{bezier_outline, bezier_path, svg_bezier_path};
use space::sticky;
mod box_filter;
pub(crate) mod button;
mod caret;
mod composite;
pub(crate) mod control;
pub(crate) mod damage;
pub(crate) mod dates;
mod dialog_shadow;
pub(crate) use dialog_shadow::DialogShadow;
pub mod gradient;
mod grouped;
mod overlay;
pub(crate) mod scrollbar;
pub use gradient::GradientPaint;
pub(crate) use overlay::TopSurface;
pub(crate) mod field;
mod fragments;
pub(crate) use fragments::{center as tap_point, hits, slop_hits};
pub(crate) use lift::{Ghost, Lift};
mod alert;
pub(crate) mod inline;
pub(crate) mod inline_frame;
pub(crate) mod inline_opacity;
mod layer;
mod lift;
pub(crate) mod look;
pub use look::page_color;
pub(crate) mod material;
pub(crate) use alert::Picture as AlertPicture;
pub use material::{FieldText, FieldTexts};
mod native;
pub(crate) mod neutral;
pub(crate) mod numerals;
pub(crate) mod ripple;
pub(crate) use native::object_fit_of;
pub use native::NativeKind;
mod bleed;
mod order;
mod placed;
mod presented;
mod shared;
pub use shared::{SharedElement, SharedState};
mod region;
pub mod rows;
mod shadow;
pub(crate) mod space;
mod split;
pub(crate) mod stage;
mod svg;
pub(crate) mod tape;
pub(crate) use svg::{fitted, view_box};
mod text_clip;
mod text_decoration;
mod text_shadow;
mod text_stroke;
mod tint;
pub(crate) mod zoom;
use inline::{presented_color, presented_text_colors, text_backgrounds, text_palette};
pub use presented::{PaintValues, Presented};
pub(crate) use region::{ActionNode, ActionSlot, RegionActions, ScrollBounds};
pub use svg::{resolve_with, Ink, SvgPaint};
use tint::image_tint;

/// A rectangle as (x, y, w, h).
pub type Rect4 = (f32, f32, f32, f32);

// Frozen numeric/style operands. Capture resolves environment and appearance
// once; geometry is evaluated at the published frame with ordinary f32 order.
struct BoxPaint {
    radii: [Dimension; 4],
    /// What a radius that is a calculation with a percentage resolves under.
    env: exact_kernel::Env,
    /// CSS `background-clip` (LLP 1077 D6).
    clip: exact_kernel::BackgroundClip,
    corners: Option<exact_kernel::corner::CornerShape>,
    widths: [f32; 4],
    styles: [BorderStyle; 4],
    colors: [[u8; 4]; 4],
    background: [u8; 4],
    gradients: Vec<gradient::Captured>,
    padding: [f32; 4],
    shadows: Vec<shadow::ShadowPaint>,
    /// The ordered backdrop functions, or the winning material's blur.
    backdrop: BackdropFilter,
}
struct BoxGeometry {
    outer: Shape,
    content: Rect4,
    // Left and top padding + border, summed as the kernel's measure closure
    // sums them: flowed text is measured and painted around the same bits.
    inset: (f32, f32),
    /// How far its background reaches into the window's bands (`bleed.rs`).
    bleed: [f32; 4],
}
impl BoxGeometry {
    /// The content box on the content edge's curve: where a picture or a
    /// canvas's bitmap is clipped, inside the border (CSS Backgrounds 3 §5.3).
    fn content_edge(&self) -> Shape {
        self.outer.within(self.content)
    }
}
fn paint_rect(frame: exact_kernel::Frame, offset: (f32, f32)) -> Rect4 {
    (
        frame.x - offset.0,
        frame.y - offset.1,
        frame.width,
        frame.height,
    )
}
impl BoxPaint {
    fn capture(node: &NodeRef<'_>, kernel: &Kernel, dark: bool) -> Self {
        let effective = node.computed_style(exact_kernel::StyleMask::EMPTY);
        let s = &effective;
        let widths = s.border_widths();
        let current = node.computed_row(StyleId::TextColor, |s| s.text_color);
        let colors = s.border_colors(current);
        // A length may read the safe area where the box is (LLP 1123 D9)
        let env = kernel.env_of(node.id);
        // The padding layout resolved, which input reads too: a percentage
        // is of the containing block's width
        let (left, top, right, bottom) = kernel.resolved_padding(node.key).unwrap_or_default();
        // @ref LLP 1053.000 D4 — a material wins over `backdrop-filter`; a
        // name the table lacks draws ultra-thin ([`material_note`]).
        let material = node.props.str(PropId::BackgroundMaterial).map(|name| {
            exact_kernel::generated::material(name)
                .or_else(|| exact_kernel::generated::material("ultra-thin"))
                .expect("the schema declares ultra-thin")
        });
        Self {
            radii: [
                s.border_radius_top_left.clone(),
                s.border_radius_top_right.clone(),
                s.border_radius_bottom_right.clone(),
                s.border_radius_bottom_left.clone(),
            ]
            .map(|d| d.resolve(&env)),
            env: env.clone(),
            corners: Some(s.rare.corner_shape).filter(|c| !c.is_round()),
            clip: s.background_clip,
            widths,
            styles: [
                s.border_style_top,
                s.border_style_right,
                s.border_style_bottom,
                s.border_style_left,
            ],
            colors: colors.map(|c| rgba(c.resolve(dark))),
            background: match (look::contrast(), material) {
                (Some(palette), _)
                    if !s.written.has(StyleId::BackgroundColor)
                        && s.background_color.is_some_and(|c| c.resolve(dark).a() != 0) =>
                {
                    palette.window
                }
                // The material's tint (a glass's `-exact-glass-tint` in place of the table's) where the
                // author painted no background, as the web's rule sits under an inline one (LLP 1053.000 D4).
                (_, Some(m)) if s.background_color.unwrap_or(current).resolve(dark).a() == 0 => {
                    if m.ios.starts_with("glass") && s.written.has(StyleId::GlassTint) {
                        rgba(s.rare.glass_tint.resolve(dark))
                    } else if dark {
                        m.dark
                    } else {
                        m.light
                    }
                }
                _ => rgba(s.background_color.unwrap_or(current).resolve(dark)),
            },
            gradients: gradient::Captured::capture(s, dark),
            shadows: shadow::ShadowPaint::capture(s, dark),
            backdrop: material.map_or_else(
                || s.backdrop_filter.clone(),
                |m| BackdropFilter(vec![BackdropOp::Blur(m.blur)]),
            ),
            padding: [top, right, bottom, left],
        }
    }
    fn geometry(&self, rect: Rect4) -> BoxGeometry {
        let (x, y, w, h) = rect;
        let widths = self.widths;
        let pad = self.padding;
        BoxGeometry {
            inset: (pad[3] + widths[3], pad[0] + widths[0]),
            bleed: [0.0; 4],
            outer: Shape::elliptical(
                rect,
                border::resolve_radii(self.radii.clone(), &self.env, w, h),
            )
            .with_corners(self.corners),
            content: (
                x + widths[3] + pad[3],
                y + widths[0] + pad[0],
                (w - widths[3] - widths[1] - pad[3] - pad[1]).max(0.0),
                (h - widths[0] - widths[2] - pad[0] - pad[2]).max(0.0),
            ),
        }
    }
    fn paint(&self, backend: &mut dyn Backend, geometry: &BoxGeometry, ts: Transform) {
        // The outer shadows, the list's first on top: by the backend's blur
        // where it has one, else as bands.
        for s in self.shadows.iter().rev().filter(|s| !s.inset()) {
            let bands = || s.fills(&geometry.outer, self.widths);
            backend.box_shadow(s.blurred(&geometry.outer), &geometry.outer, &bands, ts);
        }
        // @ref LLP 1053.000 D2 — the backdrop filters under the background.
        if !self.backdrop.is_none() {
            backend.backdrop_filter(&geometry.outer, &self.backdrop, ts);
        }
        let back = bleed::geometry(geometry);
        self.emit(&back, |shape, color| backend.fill(&shape, color, ts));
        // The last layer first, so the first is on top (LLP 1077 D5), within
        // the background's clip (D6).
        if let Some(clip) = self.background_shape(&back) {
            for g in self.gradients.iter().rev() {
                gradient::paint(g, &back.outer, &clip, self.widths, backend, ts);
            }
        }
        for band in self.inset_shadow_fills(geometry) {
            backend.fill_border(&band, ts);
        }
        for part in self.borders(geometry) {
            backend.fill_border(&part, ts);
        }
    }
    /// The background colour: within its `background-clip` (LLP 1077 D6).
    fn emit(&self, geometry: &BoxGeometry, mut emit: impl FnMut(Shape, [u8; 4])) {
        let Some(shape) = self.background_shape(geometry) else {
            return;
        };
        if self.background[3] > 0 && shape.rect.2 > 0.0 && shape.rect.3 > 0.0 {
            emit(shape, self.background);
        }
    }
    /// The outer `box-shadow`s, under everything else (LLP 1064 D2), the
    /// list's first on top (LLP 1077 D4).
    fn shadow_fills(&self, geometry: &BoxGeometry) -> Vec<border::BorderFill> {
        self.shadows
            .iter()
            .rev()
            .filter(|s| !s.inset())
            .flat_map(|s| s.fills(&geometry.outer, self.widths))
            .collect()
    }
    /// The inset `box-shadow`s, over the background and under the border.
    fn inset_shadow_fills(&self, geometry: &BoxGeometry) -> Vec<border::BorderFill> {
        self.shadows
            .iter()
            .rev()
            .filter(|s| s.inset())
            .flat_map(|s| s.fills(&geometry.outer, self.widths))
            .collect()
    }
    /// The border, one fill per colour, joined as the web joins sides.
    fn borders(&self, geometry: &BoxGeometry) -> Vec<border::BorderFill> {
        let mut fills =
            border::border_fills_styled(&geometry.outer, self.widths, self.colors, self.styles);
        fills.extend(border::pattern_fills(
            &geometry.outer,
            self.widths,
            self.colors,
            self.styles,
        ));
        fills
    }
}
type ProjectiveHit = ([f32; 9], Rect4, Option<Rect4>, [[f32; 3]; 2]);

/// A node's box as painted: its transformed bounding box in viewport
/// points, the clip it was painted under (viewport points, axis-aligned),
/// and its scroll offset when it is a scroll container.
#[derive(Debug, Clone)]
pub struct PaintedBox {
    /// The node.
    pub id: ViewId,
    /// The box.
    pub rect: Rect4,
    /// The first text baseline in the scroll plane of this painted box.
    pub(crate) baseline: Option<(f32, f32)>,
    /// The clip, `None` when unclipped.
    pub clip: Option<Rect4>,
    /// The scroll offset for a scroll container.
    pub scroll: Option<(f32, f32)>,
    /// Generic pointer eligibility of this painted scene, including inheritance.
    pub(crate) pointer_hit: bool,
    /// Rounded box or control face in local space, used for scrollbar coverage.
    rounded_border: Option<Rc<exact_kernel::svg::Path>>,
    shape_clips: Vec<shape::HitClip>,
    projective: Option<ProjectiveHit>,
    affine: Option<(Transform, Rect4)>,
    unpress: Transform,
    hit: Option<HitGeometry>,
}

/// Hit geometry follows the same walk with press translation excluded.
#[derive(Debug, Clone)]
struct HitGeometry {
    shape_clips: Vec<shape::HitClip>,
    rect: Rect4,
    clip: Option<Rect4>,
    affine: Option<(Transform, Rect4)>,
    projective: Option<ProjectiveHit>,
}

/// What a frame is painted from.
pub struct Scene<'a> {
    /// The kernel: frames, styles, props.
    pub kernel: &'a Kernel,
    /// @ref LLP 1038 D6 — hidden route subtrees paint no pixels or hit boxes.
    pub hidden: &'a dyn Fn(ViewId) -> bool,
    /// The roots, in order.
    pub roots: &'a [ViewId],
    /// A node's presentation values.
    pub presented: &'a dyn Fn(ViewId) -> Presented,
    /// A path's `d` while a transition moves it (LLP 1055.000 D15).
    pub paths: &'a dyn Fn(ViewId) -> Option<exact_motion::PathValue>,
    /// A `filter` while a transition moves it (LLP 1003.002 D8).
    pub filters: &'a dyn Fn(ViewId) -> Option<exact_motion::FilterValue>,
    /// Scroll offsets of scroll containers (host state, LLP 1010).
    pub scroll: &'a BTreeMap<ViewId, (f32, f32)>,
    /// The page's scroll offset: the window is a viewport over a document.
    pub page: (f32, f32),
    /// Decoded images by node.
    pub images: &'a BTreeMap<ViewId, Arc<Bitmap>>,
    /// The focused node, if any (its ring, or a field's caret, is painted).
    pub focus: Option<ViewId>,
    /// The focused text field's selection (x2apps codeedit #2).
    pub selection: Option<exact_runner::FieldSelection>,
    /// At a wrap, a click or visual-line End can choose the preceding line.
    pub caret_upstream: bool,
    /// Unbound checkboxes' own states, which the host keeps (LLP 1069.001 D4).
    pub controls: &'a BTreeMap<ViewId, bool>,
    /// Values chosen in a date, range or select, or typed into a field, since
    /// its bound value last changed: (choice, bound).
    pub chosen: &'a BTreeMap<ViewId, (String, String)>,
    /// A select's open menu, painted over everything (LLP 1069.001 D7).
    pub menu: Option<control::MenuPaint>,
    /// The pointer, in viewport points, when the host draws one.
    pub pointer: Option<(f32, f32)>,
}

mod frame;
pub use crate::canvas2d::CanvasUpdate;
pub use backend::{Backend, FrameTarget, WindowLayer, POINTER};
pub use exact_ink::Rotation;
pub use frame::Frame;

/// The painter: the walk over one backend.
pub struct Painter {
    /// Module replies from a replaced kernel must not name its new allocations.
    pub(crate) native_session: u64,
    pub(crate) split: Option<crate::presenter::split::Split>,
    pub(crate) window_canvas: Option<ViewId>,
    pub(crate) direct_canvases: std::collections::BTreeSet<ViewId>,
    pub(crate) media: native::Media,
    boxless_audio: Option<Vec<ViewId>>,
    pub(crate) alert: Option<alert::Picture>,
    /// The text engine, shared with the kernel's measurer.
    pub text: Shared,
    /// Device pixels per point.
    pub scale: f32,
    /// Which appearance a `light-dark()` colour resolves to (LLP 1034 D2):
    /// what the app's `setScheme` last said, else the system's, which the
    /// device's shell or an agent tells the presenter; `light` until one does.
    pub dark: bool,
    /// How the device writes dates, where its shell told it: what a date
    /// control shows (`dates.rs`).
    pub(crate) dates: Option<dates::Dates>,
    output: rotation::Output,
    backend: Box<dyn Backend>,
    #[cfg(test)]
    pub(crate) rank_passes: usize,
    /// Retained until the kernel commits; scroll and damage paints reuse it.
    pub(crate) paint_epoch: Option<u64>,
    ranks: Rc<BTreeMap<ViewId, i64>>,
    /// This epoch's walked nodes' potentials, and passes since a full one.
    fresh_order: HashMap<ViewId, exact_kernel::paint_order::Potentials>,
    rank_increments: u32,
    pub(crate) placements: BTreeMap<ViewId, crate::placement::Placement>,
    /// The canvases whose children their surfaces composite, and their pictures.
    pub(crate) composites: composite::Composites,
    /// Each 2D canvas's latest bitmap (LLP 1056).
    pub(crate) canvases: BTreeMap<ViewId, crate::canvas2d::CanvasPaint>,
    /// Each hatched node's overlay as last published (LLP 1075.003.000.001 §2.2.1).
    pub(crate) overlays: BTreeMap<ViewId, Arc<Pixmap>>,
    pub(crate) top_surface: Option<TopSurface>,
    pub(crate) scrollbars: scrollbar::State,
    viewport: (f32, f32),
    /// How far the window reaches past each edge of the viewport (`bleed.rs`).
    pub(crate) bands: [f32; 4],
    cpu_ms: Option<f64>,
    /// What a reorder lifts: a row in its list, or a ghost over everything.
    pub(crate) lift: Lift,
    pub(crate) shared: SharedState,
    // Accepted owners retain their paragraph leases while a subsequent backend frame fails.
    accepted_text: BTreeMap<exact_kernel::NodeKey, Rc<Paragraph>>,
    pub(crate) reader_highlights:
        BTreeMap<exact_kernel::NodeKey, inline_frame::selection::Highlight>,
    pub(crate) field_text: HashMap<ViewId, Rc<field::PaintedField>>,
    pub(crate) caret_hidden: bool,
    pub(crate) composition: Option<Rc<field::Composition>>,
    pub(crate) inline_text: HashMap<ViewId, Rc<inline_frame::PaintedInline>>,
    region_picture: Option<Rc<region::Picture>>,
    region_frame: Option<region::Published>,
    damage: damage::Retained,
    /// `backgroundMaterial` names the schema lacks.
    materials: std::collections::BTreeSet<String>,
    notes: Vec<String>,
    svg_warned: std::collections::BTreeSet<exact_kernel::NodeKey>,
    decoration_warning: bool,
    /// The node a 3D island paints flat, its own transform being the warp's (LLP 1077 D8).
    pub(crate) flatten: Option<ViewId>,
    /// The current focus's visibility decision, separate from the last input's modality.
    pub(crate) ringless: Option<ViewId>,
    /// The focus visual follows WinUI's input tracking, not Chrome's `:focus-visible` (LLP
    /// 1115.004 D9.2): pointer movement counts as input, and a key
    /// held with Ctrl, Alt or the Windows key counts as keyboard input.
    pub(crate) winui_focus: bool,
    /// The last input was a pointer; Windows and Android start in this mode.
    pub(crate) pointer_modality: bool,
    /// Android's touch mode (LLP 1115.001 D7.6): a finger or pen also hides the current ring.
    pub(crate) touch_mode: bool,
    /// Whose shapes a symbol role takes: the process's (`look::symbols`).
    pub(crate) symbols: exact_kernel::SymbolSet,
    /// The control under a pointer that hovers, and the one a contact holds
    /// down (`presenter/feedback.rs`), which the neutral look washes.
    pub(crate) feedback: (Option<ViewId>, Option<ViewId>),
    /// The control a held `active` presses (`presenter/held.rs`), beside a contact's.
    pub(crate) held_press: Option<ViewId>,
    /// The focus is on something that presses, which Material shows by its
    /// state layer (`ripple.rs`).
    pub(crate) focus_presses: bool,
    /// Material's ripples, each from its press's touch (`ripple.rs`).
    pub(crate) ripples: ripple::Ripples,
    /// The background the last box painted shows, its states' over its own,
    /// which its `background-clip: text` fills with (`text_clip`).
    clipped: Option<(ViewId, [u8; 4])>,
    /// Each hosted text field's content box in its own border box (x, y,
    /// width, height), as last walked (`field.rs`, LLP 1115.001 D8).
    pub(crate) field_content: HashMap<ViewId, [f32; 4]>,
    rows: rows::Rows,
    /// The `svg` painting now: its elements the reader animates.
    svg_layers: Vec<(ViewId, Presented)>,
    /// How many boxes the last walk painted: the next one's room.
    boxes_hint: usize,
    /// Numeric text rolling (LLP 1077 D15), and the clock it rolls by.
    pub(crate) numerals: numerals::Numerals,
    /// Material's fields' labels, floated or resting (`material.rs`).
    pub(crate) labels: material::Labels,
    /// The routes the presenter places itself, and their kept drawings.
    pub(crate) stage: stage::Stage,
    kept: stage::Kept,
    /// Each zoomable container's zoom as the presenter set it (`zoom.rs`).
    pub(crate) zoom: BTreeMap<ViewId, f32>,
}

struct Walk<'a, 'b> {
    scene: &'b Scene<'a>,
    /// Offscreen boxes can cover the frame's scrollbars too.
    scrollbar_cover: bool,
    hit_space: Option<(Transform, Option<Rect4>)>,
    boxes: Vec<PaintedBox>,
    text: BTreeMap<exact_kernel::NodeKey, Rc<Paragraph>>,
    fields: HashMap<ViewId, Rc<field::PaintedField>>,
    inline: HashMap<ViewId, Rc<inline_frame::PaintedInline>>,
    skip: Option<exact_kernel::NodeKey>,
    replay: Option<&'b region::Replay<'b>>,
    /// Each node's rank among its siblings (LLP 1083.000 §2.4), twice its
    /// value, from the kernel's one definition.
    ranks: Rc<BTreeMap<ViewId, i64>>,
    /// Painting a ghost, whose root this is: the `visibility: hidden` the
    /// row inherits from its wrapper shows, one its own nodes set does not
    /// (`lift.rs`; b6 review B4).
    reveal: Option<ViewId>,
    alert: Option<exact_kernel::NodeKey>,
    dialog: Option<ViewId>,
}

impl Painter {
    /// Whether `node` has a material (which blurs its backdrop); notes a name
    /// the schema lacks, once, for the host's log (LLP 1053.000 D4).
    fn material_note(&mut self, node: &NodeRef<'_>) -> bool {
        let Some(name) = node.props.str(PropId::BackgroundMaterial) else {
            return false;
        };
        if exact_kernel::generated::material(name).is_none() && self.materials.insert(name.into()) {
            self.notes.push(format!(
                "backgroundMaterial `{name}` is not a material; drawing ultra-thin"
            ));
        }
        true
    }

    /// Host warnings noted since the last call.
    pub fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// A painter over a backend.
    pub fn new(text: Shared, scale: f32, backend: Box<dyn Backend>) -> Painter {
        Painter {
            native_session: native::new_session(),
            split: None,
            window_canvas: None,
            direct_canvases: Default::default(),
            media: Default::default(),
            boxless_audio: None,
            text,
            scale,
            dark: false,
            dates: None,
            backend,
            output: Default::default(),
            #[cfg(test)]
            rank_passes: 0,
            alert: None,
            paint_epoch: None,
            fresh_order: HashMap::new(),
            rank_increments: 0,
            ranks: Rc::default(),
            accepted_text: BTreeMap::new(),
            lift: Lift::default(),
            shared: SharedState::default(),
            region_picture: None,
            region_frame: None,
            damage: Default::default(),
            materials: Default::default(),
            notes: Vec::new(),
            svg_warned: Default::default(),
            decoration_warning: false,
            placements: BTreeMap::new(),
            composites: Default::default(),
            canvases: BTreeMap::new(),
            overlays: BTreeMap::new(),
            top_surface: None,
            scrollbars: Default::default(),
            viewport: (0., 0.),
            bands: [0.0; 4],
            cpu_ms: None,
            flatten: None,
            ringless: None,
            winui_focus: cfg!(windows),
            pointer_modality: cfg!(windows) || cfg!(target_os = "android"),
            touch_mode: cfg!(target_os = "android"),
            symbols: look::symbols(),
            feedback: (None, None),
            held_press: None,
            focus_presses: false,
            ripples: Default::default(),
            clipped: None,
            field_content: HashMap::new(),
            field_text: HashMap::new(),
            reader_highlights: BTreeMap::new(),
            caret_hidden: false,
            composition: None,
            inline_text: HashMap::new(),
            rows: Default::default(),
            boxes_hint: 0,
            svg_layers: Vec::new(),
            numerals: Default::default(),
            labels: Default::default(),
            stage: Default::default(),
            kept: Default::default(),
            zoom: BTreeMap::new(),
        }
    }

    /// The backend's name.
    pub fn backend(&self) -> &'static str {
        self.backend.name()
    }

    pub(crate) fn recycle_frame(&mut self, pixels: Arc<Pixmap>) {
        if let Ok(pixels) = Arc::try_unwrap(pixels) {
            self.backend.recycle_frame(pixels);
        }
    }

    #[cfg(windows)]
    pub(crate) fn recycle_window_layers(
        &mut self,
        below: Pixmap,
        above: Arc<Pixmap>,
        rows: (u32, u32),
    ) {
        // Damage snapshots contain only the overlay after a split, not a
        // complete CPU picture. A window repaint rebuilds both layers.
        self.damage.pixels = None;
        self.damage.next.clear();
        self.backend
            .recycle_window_layers(below, Arc::try_unwrap(above).ok(), Some(rows));
    }

    #[cfg(windows)]
    pub(crate) fn take_window_underlay(&mut self) -> Option<WindowLayer> {
        self.backend.take_window_underlay()
    }

    /// Something the painter animates owes a frame: a number rolling, a
    /// field's label floating, a ripple spreading or fading.
    pub(crate) fn moving(&self) -> bool {
        let (now, still) = (self.numerals.now, self.numerals.still);
        self.numerals.rolling() || self.labels.moving() || self.ripples.moving(now, still)
    }

    /// When the last of them comes to rest, ms.
    pub(crate) fn settles_at(&self) -> Option<f64> {
        let now = self.numerals.now;
        (self.numerals.settles_at().into_iter())
            .chain(self.labels.settles_at())
            .chain(self.ripples.settles_at(now))
            .reduce(f64::max)
    }

    #[cfg(windows)]
    pub(crate) fn window_overlay_rows(&self) -> Option<(u32, u32)> {
        self.backend.window_overlay_rows()
    }

    /// Another backend from here on (the CPU's, once the GPU's failed a frame).
    pub fn replace_backend(&mut self, backend: Box<dyn Backend>) {
        self.backend = backend;
    }

    /// The last frame's (paint, readback) milliseconds; CPU readback is zero.
    pub fn last_frame_ms(&self) -> Option<(f64, f64)> {
        self.backend
            .last_frame_ms()
            .or(self.cpu_ms.map(|ms| (ms, 0.)))
    }

    /// Accepted leaves that fell back after an incomplete bounded walk.
    pub fn flow_failures(&self) -> impl Iterator<Item = exact_kernel::NodeKey> + '_ {
        self.accepted_text
            .iter()
            .filter_map(|(key, p)| p.flow_incomplete().then_some(*key))
    }

    /// Accepted leases outside the current text catalog, deduplicated by Rc.
    /// A catalog swap followed by failed frames retains the previous accepted
    /// set until successful replacement. Diagnostic-only; not total residency.
    pub fn retiring_text_residency(&self) -> crate::text::RetiringResidency {
        self.text
            .borrow()
            .retiring_accepted(self.accepted_text.values())
    }

    /// Paint the scene into a viewport of the given size (points).
    pub fn paint(&mut self, scene: &Scene<'_>, viewport: (f32, f32)) -> Result<Frame, String> {
        self.paint_selected(scene, viewport, None, None)
    }

    /// Paint exactly the registered selected branch. An accepted publication
    /// requires its native snapshot; live candidate text is never a fallback.
    pub(crate) fn paint_region(
        &mut self,
        scene: &Scene<'_>,
        viewport: (f32, f32),
        region: &crate::content_region::ContentRegionState,
        collection_limits: &BTreeMap<ViewId, f32>,
        actions: &mut RegionActions<'_>,
    ) -> Result<Frame, String> {
        region.validate_scale(self.scale)?;
        self.validate_region_presentation(scene, region)?;
        if self.backend.name() == "gpu" {
            return Err("content-region trial requires CPU painting".into());
        }
        let receipt = region
            .receipt()
            .ok_or("content region has no successful layout")?;
        match &receipt.selection {
            exact_kernel::RegionSelection::Pending(key) if *key == region.binding().pending => {
                let frame =
                    self.paint_selected(scene, viewport, Some(region.binding().content), None)?;
                self.region_picture = None;
                self.region_frame = Some(region::Published {
                    incarnation: region.incarnation().clone(),
                    selection: None,
                });
                Ok(frame)
            }
            exact_kernel::RegionSelection::Pending(_) => {
                Err("content region placeholder identity mismatch".into())
            }
            exact_kernel::RegionSelection::Accepted(publication) => {
                let picture = if receipt.current {
                    // A flat native paint/hit snapshot, never an app/layout
                    // graph. Only bounded action scalars are copied; paragraph
                    // UTF-8 and cold text lookup remain outside this path.
                    region::Picture::capture(
                        self,
                        scene,
                        region,
                        publication,
                        collection_limits,
                        actions,
                    )?
                } else {
                    self.region_picture
                        .as_ref()
                        .filter(|p| p.belongs_to(region))
                        .cloned()
                        .ok_or("retained content has no matching native picture")?
                };
                // Validate source, project once, and preflight every prepared
                // text query before backend.begin or any shell/content emission.
                let replay = region::Replay::prepare(
                    &picture,
                    scene,
                    receipt.origin,
                    region.binding().content,
                    viewport,
                    self.scale,
                    actions,
                )?;
                let frame = self.paint_selected(
                    scene,
                    viewport,
                    Some(region.binding().pending),
                    Some(&replay),
                )?;
                self.region_frame = Some(region::Published {
                    incarnation: region.incarnation().clone(),
                    selection: Some((picture.publication().clone(), receipt.origin)),
                });
                self.region_picture = Some(picture);
                Ok(frame)
            }
        }
    }

    fn paint_selected_for(
        &mut self,
        scene: &Scene<'_>,
        viewport: (f32, f32),
        skip: Option<exact_kernel::NodeKey>,
        replay: Option<&region::Replay<'_>>,
        target: FrameTarget,
    ) -> Result<Frame<Option<Arc<Pixmap>>>, String> {
        #[cfg(target_os = "macos")]
        let _trace = exact_ink::device::apple::Trace::new(c"paint walk");
        let started = std::time::Instant::now();
        self.viewport = viewport;
        self.backend.page(page_color(self.dark));
        let partial = if let Some(previous) = self.damage.pixels.as_ref().filter(|_| {
            self.output.rotation == exact_ink::Rotation::R0
                && !self.damage.next.is_empty()
                && self.top_surface.is_none()
                && (!self.scrollbars.enabled || self.split.is_some())
        }) {
            self.backend.begin_damage(
                viewport.0,
                viewport.1,
                self.scale,
                previous,
                &self.damage.next,
            )
        } else {
            self.backend.begin(viewport.0, viewport.1, self.scale);
            false
        };
        if partial {
            self.damage.last = self.damage.next.clone();
        } else {
            self.damage.last.clear();
        }
        self.damage.next.clear();
        self.damage.unsupported = false;
        if self.paint_epoch != Some(scene.kernel.epoch()) {
            self.svg_warned
                .retain(|key| scene.kernel.node_by_key(*key).is_some());
            self.refresh_ranks(scene.kernel);
            self.order_shared(scene);
            self.paint_epoch = Some(scene.kernel.epoch());
            #[cfg(test)]
            {
                self.rank_passes += 1;
            }
        }
        self.drop_unrendered_flights(scene);
        self.backend.shared_frame(&self.shared);
        let mut walk = Walk {
            scene,
            scrollbar_cover: self.scrollbars.enabled,
            hit_space: None,
            boxes: Vec::with_capacity(self.boxes_hint),
            text: BTreeMap::new(),
            fields: HashMap::new(),
            inline: HashMap::new(),
            skip,
            replay,
            ranks: self.ranks.clone(),
            reveal: None,
            alert: self.alert.as_ref().map(|a| a.key),
            dialog: None,
        };
        let old_bars = std::mem::take(&mut self.scrollbars.bars);
        self.rows_begin(&walk);
        let kept = self.kept_places(&walk, None, scene.roots);
        let space = Transform::from_translate(-scene.page.0, -scene.page.1);
        let last = self.stage_after(scene.roots);
        for (at, root) in scene.roots.iter().enumerate() {
            self.replay_kept(walk.scene, &kept, at, space);
            self.node(&mut walk, *root, Transform::identity(), scene.page, None);
            self.paint_stage(&mut walk, *root, Some(*root) == last);
        }
        self.replay_kept(walk.scene, &kept, scene.roots.len(), space);
        self.paint_sheets(&mut walk);
        self.paint_ghost(&mut walk);
        self.paint_alert(&mut walk);
        self.rows_end();
        if let Some(menu) = &scene.menu {
            self.menu(menu);
        }
        self.paint_top_surface(viewport);
        if let Some((px, py)) = scene.pointer {
            self.backend.pointer(px, py);
        }
        self.boxes_hint = walk.boxes.len();
        #[cfg(target_os = "macos")]
        drop(_trace);
        let finished = self
            .output
            .finish(self.backend.as_mut(), target)
            .and_then(|pixels| {
                if target == FrameTarget::Pixels && pixels.is_none() {
                    Err("paint backend returned no pixels for capture".into())
                } else {
                    Ok(pixels)
                }
            });
        if self.backend.name() == "cpu" {
            self.cpu_ms = Some(started.elapsed().as_secs_f64() * 1000.);
        }
        // Publication is the ownership boundary. On Err the previous accepted
        // set remains intact; candidate leases simply unwind with `walk`.
        self.numerals.painted();
        self.labels.painted();
        self.ripples.painted(self.numerals.now, self.numerals.still);
        if finished.is_ok() {
            self.accepted_text = walk.text;
            self.field_text = walk.fields;
            self.inline_text = walk.inline;
        } else {
            self.scrollbars.bars = old_bars;
            self.discard_rows();
            drop(walk.text);
        }
        // Pending measurements end with every paint attempt, including failure.
        self.text.borrow_mut().finish_text_frame();
        let pixmap = finished?.map(Arc::new);
        self.damage.pixels = pixmap.clone().filter(|_| {
            self.output.rotation == exact_ink::Rotation::R0
                && (self.split.is_some()
                    || self
                        .accepted_text
                        .values()
                        .any(|p| !p.fragments().is_empty()))
        });
        self.damage.dark = self.dark;
        self.damage.scrollbars = self.scrollbars.ink();
        self.damage.focus_ink = damage::field_focus_ink(scene.kernel, scene.focus);
        Ok(Frame {
            pixmap,
            boxes: walk.boxes,
        })
    }

    fn node_content(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
        clip_rect: Option<Rect4>,
    ) {
        let offset = sticky(walk.scene, id, offset);
        if self.placed(walk, id, ts, offset, clip_rect) {
            self.row_refuse();
            return;
        }
        if let Some(replay) = walk.replay.filter(|r| {
            walk.scene
                .kernel
                .node(id)
                .is_some_and(|n| n.key == r.content)
        }) {
            replay.paint(self, walk, ts, offset, clip_rect);
            return;
        }
        let Some(node) = walk.scene.kernel.node(id) else {
            return;
        };
        // An SVG element is its `svg`'s content, painted there (LLP 1055 D4).
        if walk.skip == Some(node.key) || node.node_type.is_svg_element() {
            return;
        }
        if (walk.scene.hidden)(id) || walk.draws_alert_elsewhere(&node) || node.is_inline_run() {
            return;
        }
        if node.style.display == Display::None
            || (node.props.str(PropId::SemanticTag) == Some("dialog") && walk.dialog != Some(id))
        {
            self.boxless_audio(walk, &node);
            return;
        }
        // The roles drawn in the accent read the nearest authored
        // `accent-color`, for this node and, until one authors its own, below.
        let _accent = accent_scope(&node);
        let f = node.frame;
        let (x, y, w, h) = paint_rect(f, offset);
        let p = (walk.scene.presented)(id);
        self.row_node(walk.scene.kernel, node.key, f, &p);
        // @ref LLP 1077 D8 — turned or moved in space: painted apart, warped.
        if self.spatial(walk, &node, &p, (x, y, w, h), ts, offset, clip_rect) {
            self.row_refuse();
            self.damage.unsupported = true;
            return;
        }
        // @ref LLP 1043.000 §3 D7 — collect damage eligibility during the
        // existing paint walk, not an extra whole-document walk per flow tick.
        // A shadow paints outside the node's box, where damage never looks.
        self.damage.unsupported |= p.moves()
            || p.dark.is_some_and(|dark| dark != self.dark)
            || p.opacity != 1.0
            || node.node_type == NodeType::Image
            || !node.style.box_shadow.0.is_empty()
            // A backdrop reads what is under it, beyond any damage.
            || !node.style.backdrop_filter.is_none()
            || self.material_note(&node)
            || !p.colors.is_empty();
        let origin = node.style.transform_origin.resolve(w, h);
        let (hit_parent, hit_clip) = walk.hit_space.unwrap_or((ts, clip_rect));
        let mut hit_p = p;
        hit_p.press_translate = [0.; 4];
        let layer = match self.flatten == Some(id) {
            true => layer::Opened { lowered: 0 },
            false => self.box_layer(node.key, &p, (x, y, w, h), origin, ts),
        };
        self.damage.unsupported |= layer.lowered != 0;
        let ts = if layer.transform() {
            // The layer turns it; a layout transition's offset is drawn.
            ts.pre_translate(p.layout[0], p.layout[1])
        } else if p.moves() && self.flatten != Some(id) {
            // About `transform-origin`, the centre unless authored (LLP 1061 D6).
            ts.pre_concat(p.transform((x, y, w, h), origin))
        } else {
            ts
        };
        let hit_ts = if self.flatten == Some(id) {
            hit_parent
        } else if layer.transform() {
            hit_parent.pre_translate(p.layout[0], p.layout[1])
        } else {
            hit_parent.pre_concat(hit_p.transform((x, y, w, h), origin))
        };
        walk.hit_space = (hit_ts != ts || hit_clip != clip_rect).then_some((hit_ts, hit_clip));
        let scrolls = {
            let (ox, oy) = effective_overflow(&node);
            ox != Overflow::Visible || oy != Overflow::Visible
        };
        // A hidden box keeps its geometry. A visible descendant still paints and takes hits.
        // A ghost reveals the wrapper's inherited visibility (LLP 1094).
        let visible = paints(walk.scene.kernel, id, walk.reveal);
        let hit_start = walk.boxes.len();
        walk.boxes.push(PaintedBox {
            baseline: None,
            rounded_border: None,
            shape_clips: Vec::new(),
            id,
            pointer_hit: node
                .computed_row(exact_kernel::StyleId::PointerEvents, |s| s.pointer_events)
                != exact_kernel::PointerEvents::None
                && visible,
            projective: None,
            affine: Some((ts, (x, y, w, h))),
            hit: walk.hit_space.map(|_| HitGeometry {
                shape_clips: Vec::new(),
                rect: bbox(hit_ts, (x, y, w, h)),
                clip: hit_clip,
                affine: Some((hit_ts, (x, y, w, h))),
                projective: None,
            }),
            unpress: if p.press == 1. {
                Transform::identity()
            } else {
                let rect = (x, y, w, h);
                let mut base = hit_p;
                base.press = 1.;
                base.press_translate = [0.; 4];
                hit_p
                    .transform(rect, origin)
                    .invert()
                    .map(|inverse| inverse.pre_concat(base.transform(rect, origin)))
                    .unwrap_or_default()
            },
            rect: bbox(ts, (x, y, w, h)),
            clip: clip_rect,
            scroll: scrolls.then(|| walk.scene.scroll.get(&id).copied().unwrap_or((0.0, 0.0))),
        });
        let opacity = if layer.opacity() {
            1.0
        } else {
            p.opacity.clamp(0.0, 1.0)
        };
        // CSS opacity is paint only: a transparent subtree is still hit (the
        // walk records its boxes) and draws through a backend that draws nothing.
        // Visibility is not that: only this box's own paint is skipped.
        let drawn = (opacity <= 0.0)
            .then(|| self.replace_picture_backend(walk.scene.kernel, Box::new(layer::Unpainted)));
        if drawn.is_none() && opacity < 1.0 {
            self.backend.push_opacity(opacity);
        }
        // The node's own appearance (a `color-scheme` above it, LLP 1034 §8)
        // for its mask and everything it paints below.
        let previous = self.dark;
        self.dark = p.dark.unwrap_or(previous);
        // @ref LLP 1077 D2 — the mask is the border box's gradient's alpha.
        let mask = gradient::Captured::mask(node.style, self.dark).map(|m| m.place((x, y, w, h)));
        if mask.is_some() {
            self.backend.push_mask(&Shape::rect((x, y, w, h)), ts);
        }
        // @ref LLP 1043.000 §3 D7 — polygon demo ink and exclusion share an outline.
        let (sx, sy, sw, sh) = p.surface((x, y, w, h));
        let css = node.style.rare.clip_path.resolved(sw, sh);
        let clip_ts = ts.pre_translate(sx, sy);
        let path_clip = css.has_path() && self.backend.push_css_clip(&css, clip_ts);
        self.box_content(walk, &node, (x, y, w, h), ts, offset, clip_rect);
        shape::clip_hits(
            &mut walk.boxes[hit_start..],
            &css,
            clip_ts,
            hit_ts.pre_translate(sx, sy),
        );
        self.dark = previous;
        if path_clip {
            self.backend.pop_clip();
        }
        if let Some(mask) = &mask {
            self.backend.pop_mask(&Shape::rect((x, y, w, h)), mask, ts);
        }
        match drawn {
            Some(picture) => drop(self.restore_picture_backend(walk.scene.kernel, picture)),
            None if opacity < 1.0 => self.backend.pop_opacity(),
            None => {}
        }
        self.layer_close(layer);
    }

    fn content(
        &mut self,
        walk: &mut Walk<'_, '_>,
        node: &NodeRef<'_>,
        rect: Rect4,
        ts: Transform,
        offset: (f32, f32),
        clip_rect: Option<Rect4>,
    ) {
        let owner = walk.boxes.len() - 1;
        let paints_self = paints(walk.scene.kernel, node.id, walk.reveal);
        let shown = (walk.scene.presented)(node.id);
        // Paint motion's values over the captured box (LLP 1055.000 D6,
        // LLP 1062 D5): background, border sides and shadow; then its states,
        // its looks and a press's background (LLP 1061 D7, LLP 1121 D8).
        let mut paint = BoxPaint::capture(node, walk.scene.kernel, self.dark)
            .presented(&shown.colors)
            .states(node, &shown, self.dark);
        if let Some(section) = walk.scene.kernel.grouped_card_section(node.id) {
            if let Some(owner) = walk.scene.kernel.node(section) {
                paint = paint.section_shadow(&owner, &(walk.scene.presented)(section), self.dark);
            }
        } else if walk.scene.kernel.grouped_section_card(node.id).is_some() {
            paint.shadows.clear();
        }
        self.split_row(node, &mut paint);
        if self.bare(node) {
            paint.background = [0; 4];
        }
        self.clipped = Some((node.id, paint.background));
        let mut geometry = paint.geometry(rect);
        // @ref LLP 1063 — a layout transition's size is the surface's alone.
        let mut surface = paint.geometry(shown.surface(rect));
        self.flight_geometry(node, &mut geometry);
        self.flight_geometry(node, &mut surface);
        // A hidden box paints none of its own chrome. Children still do,
        // and a text node still walks its runs so a visible inline paints.
        if paints_self {
            surface.bleed = self.bleed(walk.scene.kernel, node.id, &surface.outer, ts);
            paint.paint(self.backend.as_mut(), &surface, ts);
            self.column_rules(walk.scene.kernel, node, rect, ts);
            let (dark, feedback) = (
                self.dark,
                (self.feedback.0, self.presses(node.id).then_some(node.id)),
            );
            let kernel = walk.scene.kernel;
            neutral::row(
                self.backend.as_mut(),
                kernel,
                node,
                &surface.outer,
                ts,
                dark,
                feedback,
            );
            let focused = walk.scene.focus == Some(node.id);
            self.pressable(node, &surface.outer, focused, ts);
        }
        let outer = geometry.outer;
        let content = geometry.content;
        // Clipped by this box when its overflow is not visible: its children,
        // and a text's own lines (CSS Overflow 3 §3).
        let (ox, oy) = effective_overflow(node);
        let clip = field::overflow_clip(&surface.outer, paint.widths, (ox, oy));
        let mut cover = surface.outer;
        match node.node_type {
            NodeType::Video if native::is_audio(node) && (content.2 <= 0.0 || content.3 <= 0.0) => {
                self.record_boxless_audio(node);
            }
            NodeType::Text => {
                if let Some(clip) = &clip {
                    self.backend.push_clip(clip, ts);
                }
                let baseline = self.text_node(walk, node, &geometry, rect, ts);
                walk.boxes[owner].baseline = baseline;
                if clip.is_some() {
                    self.backend.pop_clip();
                }
            }
            // The rest is this element's own paint (its picture, field, or control).
            _ if !paints_self => {}
            // @ref LLP 1056 D7 — a 2D canvas's kept bitmap fills its content box.
            NodeType::Canvas => {
                self.row_refuse();
                let clips = [geometry.content_edge()];
                if self.window_canvas == Some(node.id) {
                    self.backend.split_window_canvas(content, &clips, ts);
                } else if let Some(c) = self.canvases.get(&node.id) {
                    if let Some(rect) = object_fit(
                        (c.pixels.width(), c.pixels.height()),
                        node.style.object_fit,
                        &node.style.object_position,
                        content,
                        &walk.scene.kernel.env_of(node.id),
                    ) {
                        self.backend
                            .canvas_frame(&c.pixels, c.update, rect, &clips, ts);
                    }
                } else {
                    self.native(walk, node, &clips[0], ts);
                }
            }
            NodeType::Image => {
                self.row_image(node.id, walk.scene.images.get(&node.id));
                if node
                    .props
                    .str(exact_kernel::PropId::ImageSource)
                    .is_some_and(crate::image::is_symbol)
                {
                    // Its `color` as presented, through a transition or a press (LLP 1081
                    // stage 2), unless the platform's sheet tinted it: a grouped row's leading
                    // symbol and its chevron, as UIKit tints a list cell's image.
                    let sheet = node.style.mask.has(StyleId::TintColor)
                        && !node.style.written.has(StyleId::TintColor);
                    let ink = if sheet {
                        image_tint(node, &shown, self.dark)
                    } else {
                        presented_color(walk, node, self.dark)
                    };
                    self.symbol(
                        node,
                        geometry.content_edge(),
                        ink,
                        ts,
                        &walk.scene.kernel.env_of(node.id),
                    );
                } else {
                    let edge = geometry.content_edge();
                    self.row_slot(node.id, content, edge, ts);
                    self.backend.slot_begin(node.id);
                    self.picture(walk, node, content, edge, ts);
                    self.backend.slot_end();
                }
            }
            // Its container, then its text and caret, or the shell's own
            // field in their place (`field.rs`).
            NodeType::TextInput => {
                self.text_input(walk, node, rect, surface.outer.rect, content, &outer, ts)
            }
            NodeType::Svg => self.svg(walk, node, paint.content_shape(&geometry), ts),
            // @ref LLP 1042.001 D5 — the poster under the picture, until
            // its source's first frame.
            NodeType::Video => {
                let edge = geometry.content_edge();
                if !self.media.framed.contains(&node.key) {
                    self.picture(walk, node, content, edge, ts);
                }
                self.native(walk, node, &edge, ts)
            }
            NodeType::WebView | NodeType::NativeView => {
                self.native(walk, node, &geometry.content_edge(), ts)
            }
            NodeType::Control if node.props.str(PropId::Type) == Some("select") => {
                let label =
                    control::select_label(walk.scene.kernel, node, walk.scene.chosen.get(&node.id));
                let focused = walk.scene.focus == Some(node.id);
                let label = label.as_deref().unwrap_or("");
                cover = self
                    .field_control(walk, node, content, &outer, focused, ts, label, true)
                    .unwrap_or(cover);
            }
            // @ref LLP 1069.001 D7 — a date control is a field showing its
            // value (`control::date_text`); `type` sets it.
            NodeType::Control
                if matches!(
                    node.props.str(PropId::Type),
                    Some("date" | "time" | "datetime-local")
                ) =>
            {
                let chosen = walk.scene.chosen.get(&node.id);
                let shown = control::date_text(node, chosen, self.dates.as_ref());
                let focused = walk.scene.focus == Some(node.id);
                cover = self
                    .field_control(walk, node, content, &outer, focused, ts, &shown, false)
                    .unwrap_or(cover);
            }
            NodeType::Control if node.props.str(PropId::Type) == Some("range") => {
                self.range_control(node, content, ts, walk.scene.chosen.get(&node.id))
            }
            NodeType::Control if node.props.str(PropId::Type) == Some("progress") => {
                if node.props.get(PropId::Value).is_some() {
                    control::progress_bar(self.backend.as_mut(), node, content, ts, self.dark)
                } else {
                    control::progress(self.backend.as_mut(), node, content, ts, self.dark)
                }
            }
            NodeType::Control if node.props.str(PropId::Type) == Some("button") => {
                let face = walk.scene.kernel.press_face(node.id).unwrap_or_default();
                let rows = walk
                    .scene
                    .kernel
                    .button_face_style(node.id)
                    .expect("button");
                cover = self.button_control(node, surface.outer.rect, ts, &face, &rows, walk);
            }
            NodeType::Control
                if look::neutral() && node.props.str(PropId::Type) == Some("file") =>
            {
                self.file_control(node, content, ts);
            }
            NodeType::Control => {
                let held = walk.scene.controls.get(&node.id).copied();
                let hovered = self.feedback.0 == Some(node.id);
                let backend = self.backend.as_mut();
                control::paint(
                    backend, node, content, ts, self.dark, held, hovered, self.scale,
                );
                let focused = walk.scene.focus == Some(node.id);
                self.mark_ripple(node, content, held, focused, ts);
            }
            _ => {}
        }
        if walk.scrollbar_cover {
            walk.boxes[owner].rounded_border = cover.rounded_outline();
        }
        // Children, moved by this box's scroll offset when it scrolls.
        if let Some((hit_ts, hit_clip)) = walk.hit_space.as_mut() {
            if let Some(clip) = &clip {
                let own = bbox(*hit_ts, clip.rect);
                *hit_clip = Some(hit_clip.map_or(own, |c| intersect(c, own)));
            }
        }
        let clips = clip.is_some();
        let mut child_rect = clip_rect;
        if let Some(clip) = &clip {
            self.backend.push_clip(clip, ts);
            let own = bbox(ts, clip.rect);
            child_rect = Some(match clip_rect {
                Some(c) => intersect(c, own),
                None => own,
            });
        }
        // `clip` clips without being a scroll container (CSS Overflow 3 §3).
        let scrolls = !matches!(ox, Overflow::Visible | Overflow::Clip)
            || !matches!(oy, Overflow::Visible | Overflow::Clip);
        let child_offset = if scrolls {
            let (sx, sy) = walk
                .scene
                .scroll
                .get(&node.id)
                .copied()
                .unwrap_or((0.0, 0.0));
            (offset.0 + sx, offset.1 + sy)
        } else {
            offset
        };
        if paints_self {
            self.alert_title(walk, node, ts, child_offset);
        }
        let child_start = walk.boxes.len();
        self.children(walk, node, ts, child_offset, child_rect);
        if let Some(clip) = &clip {
            let hit_ts = walk.hit_space.map_or(ts, |(ts, _)| ts);
            shape::overflow_hits(&mut walk.boxes[child_start..], clip, ts, hit_ts);
        }
        if clips {
            self.backend.pop_clip();
        }
        if paints_self {
            self.node_ring(walk, node, &outer, ts);
        }
        // @ref LLP 1075.003.000.001 §2.2.1 — a hatch's overlay: over the
        // node's own paint and its descendants, clipped to its border box.
        if let Some(pixels) = self.overlays.get(&node.id).cloned() {
            self.row_refuse();
            self.damage.unsupported = true;
            let clips = [Shape::rect(rect), surface.outer];
            self.backend.canvas(&pixels, rect, &clips, ts);
        }
        self.hosted_view(node, rect, ts);
        self.paint_scrollbars(walk, node, owner, (ox, oy));
    }
}

/// CSS object-fit and object-position in the content box (LLP 1011 §4).
pub fn object_fit(
    natural: (u32, u32),
    fit: ObjectFit,
    position: &exact_kernel::object::Position,
    content: Rect4,
    env: &exact_kernel::Env,
) -> Option<Rect4> {
    exact_kernel::object::placed(
        (natural.0 as f32, natural.1 as f32),
        fit,
        position,
        content,
        env,
    )
}

/// A text node's paragraph spec from its rows (the kernel's defaults are
/// CSS's, so every row reads directly).
/// Its white space is collapsed per CSS unless the row preserves it; the
/// caller that replaces the runs collapses again ([`Spec::collapse_white_space`]).
pub fn text_spec(s: &StyleProps, text: &str) -> Spec {
    Spec {
        strut: Run::from_style("", exact_kernel::TextStyle::from_style(s)),
        runs: vec![Run::from_style(
            text,
            exact_kernel::TextStyle::from_style(s),
        )],
        align: s.text_align.physical(s.direction),
        line_clamp: s.line_clamp,
        overflow_wrap: s.overflow_wrap,
        white_space: s.white_space,
        text_wrap_style: s.text_wrap_style,
        direction: s.direction,
        text_indent: s.text_indent,
        text_box_trim: s.text_box_trim,
        text_box_edge: s.text_box_edge,
    }
    .collapse_white_space()
}

pub use field::effective_overflow;

/// A scroll container's content extent: the kernel's scrollable overflow,
/// floored with its children's extent plus the end padding (Taffy's block
/// containers do not always count it), never less than the box itself. A
/// child in a block of its own (a popover's, the viewport) is elsewhere.
pub fn content_size(node: &NodeRef<'_>, kernel: &Kernel) -> (f32, f32) {
    // Read the end padding the engine laid out with, a bar's cover included
    let (_, _, pad_right, pad_bottom) = kernel.layout_padding(node.key).unwrap_or_default();
    let mut w = node.frame.width.max(node.content.0);
    let mut h = node.frame.height.max(node.content.1);
    for child in node.children() {
        if let Some(c) = kernel.node_unblocked(child) {
            w = w.max(c.frame.x - node.frame.x + c.frame.width + pad_right);
            h = h.max(c.frame.y - node.frame.y + c.frame.height + pad_bottom);
        }
    }
    (w, h)
}

/// The painting scope for `node` (`roles::authored_accent`): the roles drawn
/// in the accent read the nearest `accent-color` it or an ancestor authored,
/// as Apple's tint and the web's `var(--exact-accent, AccentColor)` do.
pub(crate) fn accent_scope(node: &NodeRef<'_>) -> exact_kernel::style::roles::AccentScope {
    exact_kernel::style::roles::authored_accent(accent_of(node))
}

/// The nearest `accent-color` `node` or an ancestor authored.
pub(crate) fn accent_of(node: &NodeRef<'_>) -> Option<exact_kernel::ColorValue> {
    node.computed_row(StyleId::AccentColor, |s| s.accent_color)
}

/// A kernel color's channels.
pub fn rgba(c: exact_kernel::Color) -> [u8; 4] {
    [c.r(), c.g(), c.b(), c.a()]
}

/// The bounding box of a rectangle under a transform.
pub fn bbox(ts: Transform, r: Rect4) -> Rect4 {
    if ts.is_identity() {
        return r;
    }
    let corners = [
        (r.0, r.1),
        (r.0 + r.2, r.1),
        (r.0, r.1 + r.3),
        (r.0 + r.2, r.1 + r.3),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (cx, cy) in corners {
        let mut p = Point::from_xy(cx, cy);
        ts.map_point(&mut p);
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    (x0, y0, x1 - x0, y1 - y0)
}

pub(crate) fn intersect(a: Rect4, b: Rect4) -> Rect4 {
    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = (a.0 + a.2).min(b.0 + b.2);
    let y1 = (a.1 + a.3).min(b.1 + b.3);
    (x0, y0, (x1 - x0).max(0.0), (y1 - y0).max(0.0))
}

#[cfg(test)]
mod inline_image_tests;
#[cfg(test)]
mod paragraph_tests;

/// Whether `id` paints, is hit, and is exposed. In a ghost, [`revealed`];
/// otherwise the computed `visibility` (inherited). A missing node does not
/// veto a caller that already has nothing to draw.
pub(crate) fn paints(kernel: &Kernel, id: ViewId, reveal: Option<ViewId>) -> bool {
    match reveal {
        Some(root) => revealed(kernel, id, root),
        None => kernel.node(id).is_none_or(|n| {
            n.computed_row(exact_kernel::StyleId::Visibility, |s| s.visibility)
                == exact_kernel::Visibility::Visible
        }),
    }
}

/// Whether `id` shows in the ghost of `root`: the nearest `visibility` set
/// on it or an ancestor below the ghost's root decides, as in the web's
/// clone whose root alone is made visible (`group-glue.js`); none set shows.
pub(crate) fn revealed(kernel: &Kernel, id: ViewId, root: ViewId) -> bool {
    let arena = kernel.arena();
    let mut at = arena.key_of(id).map(|k| k.index);
    while let Some(slot) = at {
        if arena.local_id(slot) == root {
            return true;
        }
        let style = arena.style(slot);
        if style.mask.has(exact_kernel::StyleId::Visibility) {
            return style.visibility == exact_kernel::Visibility::Visible;
        }
        at = arena.parent(slot);
    }
    true
}
