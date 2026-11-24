//! What the motion engine says to paint over a node's style rows: the four
//! compositor values, an SVG shape's geometry (LLP 1055.000 D15), paint
//! motion's colours and shadow while they move (LLP 1055.000 D6, LLP 1062),
//! and a layout transition's box (LLP 1063).

use super::shadow::ShadowPaint;
use super::{bbox, BoxPaint, HitGeometry, PaintedBox, Rect4};
use exact_kernel::StyleProps;
use exact_motion::{Property, Value};
use tiny_skia::Transform;

/// A node's presentation values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Presented {
    /// The view's resolved appearance, when supplied by the host.
    pub dark: Option<bool>,
    /// `translate`'s lengths, points.
    pub translate: (f32, f32),
    /// `translate`'s percentages of the border box (chess diary #4),
    /// resolved where the box is painted ([`Presented::translate_at`]).
    pub translate_percent: (f32, f32),
    /// Factors, x then y.
    pub scale: (f32, f32),
    /// Host feedback, multiplied into scale without entering the motion engine.
    pub press: f32,
    /// Host feedback: x px, x percent, y px, y percent.
    pub press_translate: [f32; 4],
    /// Progress toward the pressed background (LLP 1061 D7).
    pub press_background: f32,
    /// Progress toward the pressed foreground (LLP 1061 D7).
    pub press_color: f32,
    /// Degrees.
    pub rotate: f32,
    /// The presented rotation axis, including an animated axis.
    pub rotate_axis: exact_kernel::style::space::RotateAxis,
    /// Zero to one.
    pub opacity: f32,
    /// An SVG shape's presented `r`, `stroke-dashoffset`, `cx`, `cy`, `x`, `y`, `rx`, `ry`.
    pub svg: [Option<f32>; 8],
    /// Presented paint while it moves (LLP 1055.000 D6, LLP 1062): one slot
    /// per [`Property::PAINT`]; `None` paints the row.
    pub colors: PaintValues,
    /// A layout transition's offset from the laid-out origin and its
    /// surface's difference from the laid-out size, `[dx, dy, dw, dh]`
    /// (LLP 1063).
    pub layout: [f32; 4],
    /// What the reader plays instead of the engine (`crate::host::lower`'s
    /// bits): painted at the underlying values, in a layer it moves.
    pub lowered: u8,
    /// Whether its `:hover` and `:focus-visible` looks hold (LLP 1121 D8).
    pub looks: [bool; 2],
}

impl Presented {
    /// Nothing moved.
    pub const IDENTITY: Presented = Presented {
        dark: None,
        translate: (0.0, 0.0),
        translate_percent: (0.0, 0.0),
        scale: (1.0, 1.0),
        press: 1.0,
        press_translate: [0.; 4],
        press_background: 0.0,
        press_color: 0.0,
        rotate: 0.0,
        rotate_axis: exact_kernel::style::space::RotateAxis([0.0, 0.0, 1.0]),
        opacity: 1.0,
        svg: [None; 8],
        colors: PaintValues::NONE,
        layout: [0.0; 4],
        lowered: 0,
        looks: [false; 2],
    };

    /// The committed style's values (what the engine starts from).
    pub fn from_style(s: &StyleProps) -> Presented {
        let rotation = exact_motion::rotation::presented(exact_motion::rotation::value(
            s.rotate as f64,
            s.rotate_axis.0.map(f64::from),
        ));
        Presented {
            dark: None,
            translate: (s.translate.x, s.translate.y),
            translate_percent: (s.translate_percent.x, s.translate_percent.y),
            scale: (s.scale.x, s.scale.y),
            press: 1.0,
            press_translate: [0.; 4],
            press_background: 0.0,
            press_color: 0.0,
            rotate: rotation.x as f32,
            rotate_axis: exact_kernel::style::space::RotateAxis(
                exact_motion::rotation::axis(rotation).map(|v| v as f32),
            ),
            opacity: s.opacity,
            svg: [None; 8],
            colors: PaintValues::NONE,
            layout: Presented::IDENTITY.layout,
            lowered: 0,
            looks: [false; 2],
        }
    }

    pub(crate) fn background(
        &self,
        node: &exact_kernel::NodeRef<'_>,
        kernel: &exact_kernel::Kernel,
        dark: bool,
    ) -> [u8; 4] {
        BoxPaint::capture(node, kernel, dark)
            .presented(&self.colors)
            .states(node, self, dark)
            .background
    }

    /// The used translation of a `w` × `h` border box: its lengths plus
    /// its percentages of the box, as CSS resolves `translate`.
    pub fn translate_at(&self, w: f32, h: f32) -> (f32, f32) {
        let (px, py) = self.translate_percent;
        (
            self.translate.0 + self.press_translate[0] + (px + self.press_translate[1]) / 100.0 * w,
            self.translate.1 + self.press_translate[2] + (py + self.press_translate[3]) / 100.0 * h,
        )
    }

    pub(super) fn changes_axes(&self) -> bool {
        self.scale.0 * self.press != 1.0 || self.scale.1 * self.press != 1.0 || self.rotate != 0.0
    }

    pub(super) fn moves(&self) -> bool {
        self.translate != (0.0, 0.0)
            || self.translate_percent != (0.0, 0.0)
            || self.scale != (1.0, 1.0)
            || self.press != 1.0
            || self.press_translate != [0.; 4]
            || self.rotate != 0.0
            || self.layout != Presented::IDENTITY.layout
    }

    /// The box `(x, y, w, h)` painted through its presentation: CSS's
    /// individual transforms about `origin`, then outermost the layout
    /// transition's offset (LLP 1063); its size is the surface's alone
    /// ([`Presented::surface`]).
    pub(crate) fn transform(
        &self,
        (x, y, w, h): (f32, f32, f32, f32),
        (ox, oy): (f32, f32),
    ) -> Transform {
        let (cx, cy) = (x + ox, y + oy);
        let [dx, dy, ..] = self.layout;
        let (tx, ty) = self.translate_at(w, h);
        Transform::from_translate(cx + dx + tx, cy + dy + ty)
            .pre_rotate(self.rotate)
            .pre_scale(self.scale.0 * self.press, self.scale.1 * self.press)
            .pre_translate(-cx, -cy)
    }

    /// The box's surface — its background, border, shadow and the clip it
    /// puts on its children — at the size a layout transition shows, from
    /// its top-left corner. Its content and children keep the laid-out
    /// geometry: a growing card reveals its title, never squashes it.
    pub(super) fn surface(&self, (x, y, w, h): (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let [.., dw, dh] = self.layout;
        (x, y, (w + dw).max(0.0), (h + dh).max(0.0))
    }
}

impl PaintedBox {
    pub(crate) fn for_hit(&self) -> Self {
        let mut result = self.clone();
        result.use_hit();
        result
    }

    fn use_hit(&mut self) {
        if let Some(hit) = self.hit.take() {
            self.rect = hit.rect;
            self.clip = hit.clip;
            self.affine = hit.affine;
            self.projective = hit.projective;
            self.shape_clips = hit.shape_clips;
        }
    }

    pub(super) fn geometry(&self) -> HitGeometry {
        HitGeometry {
            shape_clips: self.shape_clips.clone(),
            rect: self.rect,
            clip: self.clip,
            affine: self.affine,
            projective: self.projective,
        }
    }

    /// Placed by a perspective (`space.rs`), so no rectangle bounds it.
    pub(crate) fn projected(&self) -> bool {
        self.projective.is_some()
    }

    /// The surface's box through the same transform the painter used.
    /// Projective canvas placements have no affine surface observation.
    pub fn surface(&self, shown: Presented) -> Option<Rect4> {
        if self.projective.is_some() {
            return None;
        }
        self.affine.map(|(ts, rect)| bbox(ts, shown.surface(rect)))
    }

    /// Project the local center, which need not be the projected AABB center.
    pub fn center(&self) -> (f32, f32) {
        if let Some((inv, rect, _, _)) = self.projective {
            if let Some(h) = crate::placement::inverse(inv) {
                return crate::placement::map(&h, rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
            }
        }
        (
            self.rect.0 + self.rect.2 / 2.,
            self.rect.1 + self.rect.3 / 2.,
        )
    }
}

impl super::PaintedBox {
    /// Undo only this node's feedback in its own reference box. The painted
    /// matrix keeps the authored scale, rotation, origin and all ancestors.
    pub(crate) fn unpressed(mut self, clips_self: bool) -> Self {
        if self.unpress == Transform::identity() {
            return self;
        }
        if let Some((ts, rect)) = self.affine {
            let ts = ts.pre_concat(self.unpress);
            self.affine = Some((ts, rect));
            self.rect = super::bbox(ts, rect);
            if let Some((inv, bounds, _, planes)) = self.projective.as_mut() {
                *bounds = self.rect;
                if let Some(map) = crate::placement::inverse(*inv) {
                    self.rect = crate::placement::clipped_bounds(&map, *planes, *bounds);
                }
            }
            // The walk adds this node's clip first, then its ancestors' clips.
            // Recompute only the former in the unscaled tracking geometry.
            if clips_self {
                if let Some(clip) = self.shape_clips.first_mut() {
                    clip.inverse = super::space::inverse_affine(ts.pre_translate(rect.0, rect.1))
                        .map(|t| {
                            let inverse = [t.sx, t.kx, t.tx, t.ky, t.sy, t.ty, 0., 0., 1.];
                            self.projective
                                .map_or(inverse, |(outer, ..)| super::space::mul3(&inverse, &outer))
                        });
                }
            }
            self.unpress = Transform::identity();
        }
        self
    }
}

/// Paint motion's presented values, one slot per [`Property::PAINT`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintValues([Option<Value>; Property::PAINT.len()]);

impl PaintValues {
    /// None: every paint property shows its row.
    pub const NONE: PaintValues = PaintValues([None; Property::PAINT.len()]);

    fn slot(property: Property) -> usize {
        Property::PAINT
            .iter()
            .position(|p| *p == property)
            .expect("a paint property")
    }

    /// Paint `value` over the property's row, or the row again.
    pub fn set(&mut self, property: Property, value: Option<Value>) {
        self.0[Self::slot(property)] = value;
    }

    /// Whether anything is presented over the rows.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(Option::is_none)
    }

    /// A presented value, if one is.
    pub fn value(&self, property: Property) -> Option<Value> {
        self.0[Self::slot(property)]
    }

    /// A presented colour, straight 8-bit channels.
    pub fn color(&self, property: Property) -> Option<[u8; 4]> {
        self.0[Self::slot(property)].map(Value::to_rgba8)
    }
}

impl BoxPaint {
    /// A grouped section's card takes the section's shadow; the section
    /// itself draws none.
    pub(super) fn grouped(
        mut self,
        walk: &super::Walk<'_, '_>,
        id: exact_kernel::ViewId,
        dark: bool,
    ) -> Self {
        let kernel = walk.scene.kernel;
        if let Some(section) = kernel.grouped_card_section(id) {
            if let Some(owner) = kernel.node(section) {
                return self.section_shadow(&owner, &(walk.scene.presented)(section), dark);
            }
        } else if kernel.grouped_section_card(id).is_some() {
            self.shadows.clear();
        }
        self
    }

    /// Only the shadow projects from a grouped section onto its card.
    fn section_shadow(
        mut self,
        section: &exact_kernel::NodeRef<'_>,
        shown: &Presented,
        dark: bool,
    ) -> Self {
        for (state, on) in [
            (exact_kernel::LOOK_HOVER, shown.looks[0]),
            (exact_kernel::LOOK_FOCUS_VISIBLE, shown.looks[1]),
        ] {
            if let Some((_, look)) = section.looks().iter().find(|(s, _)| *s == state && on) {
                self.look_shadow(look, dark);
            }
        }
        self
    }

    fn look_shadow(&mut self, look: &StyleProps, dark: bool) {
        use exact_kernel::StyleId::BoxShadow;
        if look.mask.union(look.written).has(BoxShadow) {
            self.shadows = if look.mask.has(BoxShadow) {
                ShadowPaint::capture(look, dark)
            } else {
                Vec::new()
            };
        }
    }

    /// The captured box with paint motion's values over its rows.
    pub(super) fn presented(mut self, paint: &PaintValues) -> BoxPaint {
        if paint.is_empty() {
            return self;
        }
        if let Some(c) = paint.color(Property::BackgroundColor) {
            self.background = c;
        }
        let sides = [
            Property::BorderTopColor,
            Property::BorderRightColor,
            Property::BorderBottomColor,
            Property::BorderLeftColor,
        ];
        for (color, side) in self.colors.iter_mut().zip(sides) {
            if let Some(c) = paint.color(side) {
                *color = c;
            }
        }
        let geometry = paint.0[PaintValues::slot(Property::BoxShadow)];
        let color = paint.color(Property::ShadowColor);
        if geometry.is_some() || color.is_some() {
            self.shadows = ShadowPaint::over(std::mem::take(&mut self.shadows), geometry, color);
        }
        self
    }

    /// The box with its pressed background (LLP 1061 D7) `progress` of the
    /// way over its own.
    pub(super) fn pressed(mut self, style: &StyleProps, progress: f32, dark: bool) -> BoxPaint {
        if progress != 0.0 && style.mask.has(exact_kernel::StyleId::PressBackgroundColor) {
            let pressed = super::rgba(style.press_background_color.resolve(dark));
            self.background = press_mix(self.background, pressed, progress);
        }
        self
    }
}

impl BoxPaint {
    /// The box in its states (LLP 1121 D8): its `:hover` then its
    /// `:focus-visible` look while they hold (`shown.looks`), at once, then
    /// the press's paint at its progress (LLP 1061 D7, which `:active`
    /// lowers to).
    pub(super) fn states(
        self,
        node: &exact_kernel::NodeRef<'_>,
        shown: &Presented,
        dark: bool,
    ) -> BoxPaint {
        let look = |state| {
            node.looks()
                .iter()
                .find(|(s, _)| *s == state)
                .map(|(_, l)| l)
        };
        // `currentcolor` in a look is the node's colour as shown (a colour
        // transition's), as in its rows.
        let computed = node.computed_row(exact_kernel::StyleId::TextColor, |s| s.text_color);
        let current = (shown.colors.color(Property::Color))
            .unwrap_or_else(|| super::rgba(computed.resolve(dark)));
        let mut paint = self;
        for (state, on) in [
            (exact_kernel::LOOK_HOVER, shown.looks[0]),
            (exact_kernel::LOOK_FOCUS_VISIBLE, shown.looks[1]),
        ] {
            if let Some(l) = look(state).filter(|_| on) {
                paint = paint.looked(l, current, dark);
            }
        }
        if node.node_type == exact_kernel::NodeType::Control
            && node.props.str(exact_kernel::PropId::Type) == Some("button")
        {
            paint
        } else {
            paint.pressed(node.style, shown.press_background, dark)
        }
    }

    /// The box with a look's rows in place of its own: its background, its
    /// border sides' colours and its shadow.
    fn looked(mut self, look: &StyleProps, current: [u8; 4], dark: bool) -> BoxPaint {
        use exact_kernel::StyleId::*;
        // A row it clears (`unset`) is written with no value: CSS's initial
        // one (a transparent background, a `currentcolor` side, no shadow).
        let names = |id| look.mask.union(look.written).has(id);
        let rgba = |c: Option<exact_kernel::ColorValue>| {
            c.map_or(current, |c| super::rgba(c.resolve(dark)))
        };
        if names(BackgroundColor) {
            self.background = match look.mask.has(BackgroundColor) {
                true => rgba(look.background_color),
                false => [0; 4],
            };
        }
        let sides = [
            (BorderColorTop, look.border_color_top),
            (BorderColorRight, look.border_color_right),
            (BorderColorBottom, look.border_color_bottom),
            (BorderColorLeft, look.border_color_left),
        ];
        for (color, (id, side)) in self.colors.iter_mut().zip(sides) {
            if names(id) {
                *color = rgba(side.filter(|_| look.mask.has(id)));
            }
        }
        self.look_shadow(look, dark);
        self
    }
}

/// `to` mixed `t` of the way over `from`, straight 8-bit channels, as CSS
/// interpolates colours: premultiplied, so a transparent end fades rather
/// than darkens.
pub(super) fn press_mix(from: [u8; 4], to: [u8; 4], t: f32) -> [u8; 4] {
    let [a, b] = [from, to].map(|c| Value::rgba8(c[0], c[1], c[2], c[3]));
    a.lerp(b, f64::from(t)).to_rgba8()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_skia::Point;

    #[test]
    fn per_axis_scale_composes_press_and_rejects_collapsed_hits() {
        let shown = Presented {
            scale: (-2.0, 0.5),
            press: 0.5,
            ..Presented::IDENTITY
        };
        let ts = shown.transform((0., 0., 100., 100.), (50., 50.));
        assert_eq!(bbox(ts, (0., 0., 100., 100.)), (0., 37.5, 100., 25.));
        assert!(shown.changes_axes());
        let collapsed = Presented {
            scale: (1., 0.),
            ..Presented::IDENTITY
        };
        let ts = collapsed.transform((0., 0., 100., 100.), (50., 50.));
        assert!(super::super::space::inverse_affine(ts).is_none());
        assert_eq!(bbox(ts, (0., 0., 100., 100.)), (0., 50., 100., 0.));
        assert!(Presented {
            scale: (1., 0.5),
            ..Presented::IDENTITY
        }
        .moves());
    }

    #[test]
    fn a_layout_springs_size_overshoot_is_clamped_to_zero() {
        let shown = Presented {
            layout: [5.0, -8.0, -150.0, -120.0],
            ..Presented::IDENTITY
        };
        assert_eq!(
            shown.surface((10.0, 20.0, 100.0, 40.0)),
            (10.0, 20.0, 0.0, 0.0)
        );
    }

    #[test]
    fn a_layout_transition_moves_the_box_and_sizes_only_its_surface() {
        let map = |p: &Presented, x: f32, y: f32| {
            let mut point = [Point::from_xy(x, y)];
            p.transform((10.0, 20.0, 100.0, 40.0), (50.0, 20.0))
                .map_points(&mut point);
            (point[0].x, point[0].y)
        };
        let grow = Presented {
            layout: [5.0, -8.0, 0.0, -30.0],
            ..Presented::IDENTITY
        };
        // Moved by the offset, never scaled: its content keeps its size.
        assert_eq!(map(&grow, 10.0, 20.0), (15.0, 12.0));
        assert_eq!(map(&grow, 110.0, 60.0), (115.0, 52.0));
        assert_eq!(
            grow.surface((10.0, 20.0, 100.0, 40.0)),
            (10.0, 20.0, 100.0, 10.0)
        );
        // An authored scale stays about the center.
        let both = Presented {
            scale: (0.5, 0.5),
            ..grow
        };
        assert_eq!(map(&both, 10.0, 20.0), (40.0, 22.0));
    }
}
