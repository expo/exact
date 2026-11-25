//! CSS's 3D transforms (LLP 1077 D8): a box turned out of the screen's plane
//! or moved along z, under its parent's `perspective`, painted flat apart and
//! warped into the frame through the plane's homography — the placement
//! route canvas children take (`placed.rs`, `placement.rs`), so paint and hit
//! testing share one map. Its own children flatten into its plane, CSS's
//! initial `transform-style: flat`.
use super::*;

/// tiny-skia's axis-aligned inverse accepts a zero scale. Hit geometry
/// needs an actual inverse, including when a collapsed box is rotated.
pub(crate) fn inverse_affine(ts: Transform) -> Option<Transform> {
    let det = f64::from(ts.sx) * f64::from(ts.sy) - f64::from(ts.kx) * f64::from(ts.ky);
    if det == 0. || !det.is_finite() {
        return None;
    }
    ts.invert().filter(Transform::is_finite)
}

/// A 4×4 matrix, row-major, acting on column vectors.
type M4 = [[f32; 4]; 4];

fn mul(a: &M4, b: &M4) -> M4 {
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}
fn translate(x: f32, y: f32, z: f32) -> M4 {
    [
        [1., 0., 0., x],
        [0., 1., 0., y],
        [0., 0., 1., z],
        [0., 0., 0., 1.],
    ]
}
fn scale(x: f32, y: f32) -> M4 {
    [
        [x, 0., 0., 0.],
        [0., y, 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ]
}
/// CSS `rotate3d()` (Transforms 2 §13): a turn of `deg` about the axis.
fn rotate(axis: [f32; 3], deg: f32) -> M4 {
    let n = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
    let [x, y, z] = axis.map(|v| v / n);
    let (s, c) = deg.to_radians().sin_cos();
    let t = 1.0 - c;
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y, 0.],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x, 0.],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c, 0.],
        [0., 0., 0., 1.],
    ]
}

/// `a` after `b`, two 3×3 maps row-major: what a point goes through when it
/// is mapped by `b` and then by `a`.
pub(crate) fn mul3(a: &[f32; 9], b: &[f32; 9]) -> [f32; 9] {
    let mut out = [0.0; 9];
    for i in 0..3 {
        for j in 0..3 {
            out[i * 3 + j] = (0..3).map(|k| a[i * 3 + k] * b[k * 3 + j]).sum();
        }
    }
    out
}

/// How far a box's outer shadows paint past it: left, top, right, bottom.
fn shadow_reach(node: &exact_kernel::NodeRef<'_>) -> (f32, f32, f32, f32) {
    let mut out = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for s in node.style.box_shadow.shadows().iter().filter(|s| !s.inset) {
        let r = super::shadow::shadow_outset(s.blur, s.spread);
        out.0 = out.0.max(r - s.offset.x);
        out.1 = out.1.max(r - s.offset.y);
        out.2 = out.2.max(r + s.offset.x);
        out.3 = out.3.max(r + s.offset.y);
    }
    out
}

/// How far past its border box `node` paints, left, top, right, bottom: its
/// outer shadows' reach whole and, unless it clips, its descendants' painted
/// boxes — each moved by its presented translate, grown to its diagonal
/// when turned, scaled or in a plane of its own, and by its own shadows —
/// bounded by `cap`, and whether that cap cut a descendant.
#[allow(clippy::too_many_arguments)]
pub(super) fn reach(
    painter: &Painter,
    walk: &Walk<'_, '_>,
    node: &exact_kernel::NodeRef<'_>,
    (x, y, w, h): Rect4,
    ts: Transform,
    offset: PaintOffset,
    cap: f32,
) -> (Rect4, bool) {
    let mut out = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let (ox, oy) = effective_overflow(node);
    if ox == Overflow::Visible && oy == Overflow::Visible {
        let mut stack = node.children();
        while let Some(id) = stack.pop() {
            let Some(child) = walk.scene.kernel.node(id) else {
                continue;
            };
            let q = (walk.scene.presented)(id);
            let (cx, cy, cw, ch) = paint_rect(child.frame, offset);
            let (tx, ty) = q.translate_at(cw, ch);
            let (mx, my) = (
                cx + cw / 2.0 + tx + q.layout[0],
                cy + ch / 2.0 + ty + q.layout[1],
            );
            let kx = (q.scale.0 * q.press).abs().max(1.0);
            let ky = (q.scale.1 * q.press).abs().max(1.0);
            let turned =
                q.rotate % 360.0 != 0.0 || q.rotate_axis.is_3d() || child.style.translate_z != 0.0;
            let (hw, hh) = if turned {
                let d = (kx * cw).hypot(ky * ch);
                (d, d)
            } else {
                (kx * cw / 2.0, ky * ch / 2.0)
            };
            let sh = shadow_reach(&child);
            out.0 = out.0.max(x - (mx - hw - sh.0));
            out.1 = out.1.max(y - (my - hh - sh.1));
            out.2 = out.2.max(mx + hw + sh.2 - (x + w));
            out.3 = out.3.max(my + hh + sh.3 - (y + h));
            let (cox, coy) = effective_overflow(&child);
            if cox == Overflow::Visible && coy == Overflow::Visible {
                stack.extend(child.children());
            }
        }
        if let Some((rx, ry, rw, rh)) = painter.kept_bounds(walk, node, ts, offset) {
            out.0 = out.0.max(x - rx);
            out.1 = out.1.max(y - ry);
            out.2 = out.2.max(rx + rw - (x + w));
            out.3 = out.3.max(ry + rh - (y + h));
        }
    }
    // Bounded: a runaway descendant must not ask for an unbounded island;
    // the box's own shadow is never cut.
    let own = shadow_reach(node);
    (
        (
            out.0.min(cap).max(own.0),
            out.1.min(cap).max(own.1),
            out.2.min(cap).max(own.2),
            out.3.min(cap).max(own.3),
        ),
        out.0 > cap || out.1 > cap || out.2 > cap || out.3 > cap,
    )
}

/// The bounds of `r`'s corners mapped through `h`; `None` when a corner is
/// behind the viewer.
pub(crate) fn mapped_bounds(h: &[f32; 9], r: Rect4) -> Option<Rect4> {
    let mut b = (
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    );
    for (px, py) in [
        (r.0, r.1),
        (r.0 + r.2, r.1),
        (r.0, r.1 + r.3),
        (r.0 + r.2, r.1 + r.3),
    ] {
        if h[6] * px + h[7] * py + h[8] <= 0.0 {
            return None;
        }
        let (mx, my) = crate::placement::map(h, px, py);
        b = (b.0.min(mx), b.1.min(my), b.2.max(mx), b.3.max(my));
    }
    Some((b.0, b.1, b.2 - b.0, b.3 - b.1))
}

fn spatial_matrix(
    parent: Option<(NodeRef<'_>, exact_kernel::Frame)>,
    node: &exact_kernel::NodeRef<'_>,
    p: &Presented,
    (x, y, w, h): Rect4,
    offset: PaintOffset,
) -> M4 {
    let s = node.style;
    let axis = p.rotate_axis.0;
    // The box's own transform about `transform-origin`, as CSS orders
    // the individual properties: translate, rotate, scale.
    let (ox, oy) = s.transform_origin.resolve(w, h);
    let (cx, cy) = (x + ox, y + oy);
    let [dx, dy, ..] = p.layout;
    let (tx, ty) = p.translate_at(w, h);
    let mut m = translate(cx + dx + tx, cy + dy + ty, s.translate_z);
    m = mul(&m, &rotate(axis, p.rotate));
    m = mul(&m, &scale(p.scale.0 * p.press, p.scale.1 * p.press));
    m = mul(&m, &translate(-cx, -cy, 0.0));
    // Its parent's `perspective`, about the parent's `perspective-origin`.
    if let Some((parent, frame)) = parent {
        let d = parent.style.perspective;
        if d > 0.0 {
            let (px, py, pw, ph) = paint_rect(frame, offset);
            let (pox, poy) = parent.style.rare.perspective_origin.resolve(pw, ph);
            let (ax, ay) = (px + pox, py + poy);
            let mut persp = translate(ax, ay, 0.0);
            persp = mul(
                &persp,
                &[
                    [1., 0., 0., 0.],
                    [0., 1., 0., 0.],
                    [0., 0., 1., 0.],
                    [0., 0., -1.0 / d, 1.],
                ],
            );
            persp = mul(&persp, &translate(-ax, -ay, 0.0));
            m = mul(&persp, &m);
        }
    }
    m
}

pub(crate) fn mapping(
    kernel: &Kernel,
    node: &exact_kernel::NodeRef<'_>,
    p: &Presented,
) -> [f32; 9] {
    let f = node.frame;
    let rect = (f.x, f.y, f.width, f.height);
    let parent = node.parent.and_then(|id| kernel.node(id)).map(|parent| {
        let frame = parent.frame;
        (parent, frame)
    });
    mapping_at(parent, node, p, rect, (0., 0.))
}

pub(super) fn mapping_at(
    parent: Option<(NodeRef<'_>, exact_kernel::Frame)>,
    node: &NodeRef<'_>,
    p: &Presented,
    rect: Rect4,
    offset: PaintOffset,
) -> [f32; 9] {
    if p.rotate_axis.is_3d() || node.style.translate_z != 0. {
        plane(&spatial_matrix(parent, node, p, rect, offset))
    } else {
        let transform = p.transform(rect, node.style.transform_origin.resolve(rect.2, rect.3));
        crate::placement::compose([1., 0., 0., 0., 1., 0., 0., 0., 1.], transform, 0., 0.)
    }
}

fn plane(m: &M4) -> [f32; 9] {
    [
        m[0][0], m[0][1], m[0][3], m[1][0], m[1][1], m[1][3], m[3][0], m[3][1], m[3][3],
    ]
}

impl Painter {
    /// Admit only draws the recorder reproduces in a projective plane. CPU
    /// effects need the software island's viewport, SVG has unsupported inks
    /// and blends, and a nested plane may itself need a software fallback.
    fn native_plane(
        &self,
        walk: &Walk<'_, '_>,
        root: ViewId,
        matrix: [f32; 9],
        rect: Rect4,
        clamped: bool,
    ) -> Option<[f32; 9]> {
        if clamped || !matrix.iter().all(|v| v.is_finite()) {
            return None;
        }
        mapped_bounds(&matrix, rect)?;
        let inverse = crate::placement::inverse(matrix)?;
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let node = walk.scene.kernel.node(id)?;
            let s = node.style;
            // Start with boxes and ordinary text. Other content keeps its
            // existing software rendering until all of its draws are supported.
            if !matches!(
                node.node_type,
                NodeType::View | NodeType::Text | NodeType::ScrollView
            ) || !s.filter.is_none()
                || !s.backdrop_filter.is_none()
                || node.props.str(PropId::BackgroundMaterial).is_some()
                || s.rare.mask_image.gradient().is_some()
                || !s.rare.corner_shape.is_round()
                || s.background_clip == exact_kernel::BackgroundClip::Text
                || node.computed_row(StyleId::TextShadow, |s| s.text_shadow.shadow().is_some())
                || node.computed_row(StyleId::TextStrokeWidth, |s| s.text_stroke_width) > 0.0
                || s.content_transition != exact_kernel::ContentTransition::None
                || self.placements.contains_key(&id)
                || (id != root
                    && ((walk.scene.presented)(id).rotate_axis.is_3d() || s.translate_z != 0.0))
            {
                return None;
            }
            stack.extend(node.children());
        }
        Some(inverse)
    }

    /// Paint `node` through its plane's homography when it is turned or
    /// moved in space; `false` when it is not, and the caller paints it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn spatial(
        &mut self,
        walk: &mut Walk<'_, '_>,
        node: &exact_kernel::NodeRef<'_>,
        p: &Presented,
        ts: Transform,
        offset: PaintOffset,
        clip: Option<Rect4>,
    ) -> bool {
        let s = node.style;
        if self.flatten == Some(node.id) || !(p.rotate_axis.is_3d() || s.translate_z != 0.0) {
            return false;
        }
        let (x, y, w, h) = paint_rect(node.frame, offset);
        if w <= 0.0 || h <= 0.0 {
            self.unpainted(walk, node);
            return true;
        }
        let parent = self.split_parent(walk.scene.kernel, node);
        let m = spatial_matrix(parent, node, p, (x, y, w, h), offset);
        let own = crate::placement::compose(plane(&m), ts, 0.0, 0.0);
        let (hit_ts, hit_clip) = walk.hit_space.unwrap_or((ts, clip));
        let mut hit_p = *p;
        hit_p.press_translate = [0.; 4];
        let hit_m = spatial_matrix(
            self.split_parent(walk.scene.kernel, node),
            node,
            &hit_p,
            (x, y, w, h),
            offset,
        );
        let hit_own = crate::placement::compose(plane(&hit_m), hit_ts, 0., 0.);
        // The island covers what the box paints past itself: its outer
        // shadows and any overflowing descendant (nothing past a clip).
        let cap = (2.0 * w.max(h).max(64.0)).max(self.viewport.0.max(self.viewport.1));
        let ((left, top, right, bottom), clamped) =
            reach(self, walk, node, (x, y, w, h), ts, offset, cap);
        let island = (x - left, y - top, w + left + right, h + top + bottom);
        let (ix, iy, iw, ih) = island;
        // Both planes map from the origin of the painted island.
        let island_origin = translate(ix, iy, 0.);
        let m = mul(&m, &island_origin);
        let plane = plane(&m);
        // Turned away from the viewer: `backface-visibility: hidden` paints
        // nothing (the plane's normal is its z column after the turn).
        let facing = m[2][2] * if m[3][3] < 0.0 { -1.0 } else { 1.0 };
        let turned = (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * m[3][3] < 0.0 || facing < 0.0;
        if turned && s.backface_visibility == exact_kernel::BackfaceVisibility::Hidden {
            self.unpainted(walk, node);
            return true;
        }
        if let Some(inv) = self.native_plane(walk, node.id, own, island, clamped) {
            if self.backend.plane_begin(own) {
                // The software pixmap clips even glyph overhang and affine
                // descendants whose combined transforms escape `reach`.
                self.backend
                    .push_clip(&Shape::rect(island), Transform::identity());
                let flatten = self.flatten.replace(node.id);
                let mut child_walk = Walk {
                    scene: walk.scene,
                    scrollbar_cover: walk.scrollbar_cover,
                    scroll_grids: walk.scroll_grids.clone(),
                    hit_space: None,
                    boxes: Vec::new(),
                    text: BTreeMap::new(),
                    fields: HashMap::new(),
                    inline: HashMap::new(),
                    skip: None,
                    replay: None,
                    ranks: Rc::clone(&walk.ranks),
                    reveal: walk.reveal,
                    alert: walk.alert,
                    dialog: walk.dialog,
                };
                self.without_grid(true, |painter| {
                    painter.node_at(
                        &mut child_walk,
                        node.id,
                        Transform::identity(),
                        offset,
                        None,
                    );
                });
                self.flatten = flatten;
                self.backend.pop_clip();
                self.backend.plane_end();
                let hit_inv = crate::placement::inverse(hit_own).unwrap_or(inv);
                project(walk, child_walk, inv, clip, hit_inv, hit_clip);
                return true;
            }
        }
        let device = crate::placement::compose(plane, ts, 0.0, 0.0);
        let Some(inv) = crate::placement::inverse(device) else {
            self.unpainted(walk, node);
            return true;
        };
        let mut painter = Painter::new(
            self.text.clone(),
            self.scale,
            Box::new(crate::raster::Raster::transparent()),
        );
        painter.dark = self.dark;
        painter.symbols = self.symbols;
        painter.split = self.split.clone();
        painter.placements = self.placements.clone();
        painter.canvases = self.canvases.clone();
        painter.pixel_grid = self.pixel_grid;
        painter.grid_suspended = true;
        painter.flatten = Some(node.id);
        painter.capture_boxless_audio(walk.scene.kernel);
        // An offscreen subtree includes the same outgoing drawings as the
        // live walk, under this plane's current placement and compositing.
        painter.stage = std::mem::take(&mut self.stage);
        painter.kept = std::mem::take(&mut self.kept);
        // Nested warps can fill the last backing pixel even when the CSS clip
        // through that pixel is fractional.
        let backing = |size: f32| {
            let scale = f64::from(self.scale);
            ((f64::from(size) * scale).ceil() / scale) as f32
        };
        painter.viewport = (backing(iw), backing(ih));
        painter
            .backend
            .begin(painter.viewport.0, painter.viewport.1, self.scale);
        let mut child_walk = Walk {
            scene: walk.scene,
            scrollbar_cover: walk.scrollbar_cover,
            scroll_grids: walk.scroll_grids.clone(),
            hit_space: None,
            boxes: Vec::new(),
            text: BTreeMap::new(),
            fields: HashMap::new(),
            inline: HashMap::new(),
            skip: None,
            replay: None,
            ranks: Rc::clone(&walk.ranks),
            reveal: walk.reveal,
            alert: walk.alert,
            dialog: walk.dialog,
        };
        painter.node_at(
            &mut child_walk,
            node.id,
            Transform::identity(),
            (offset.0 + f64::from(ix), offset.1 + f64::from(iy)),
            None,
        );
        self.stage = std::mem::take(&mut painter.stage);
        self.kept = std::mem::take(&mut painter.kept);
        self.emit_boxless_audio(walk.scene.kernel, painter.boxless_audio.take());
        let planes = [[0., 0., 1.]; 2];
        if let Ok(source) = painter.backend.finish() {
            if let Some((pixels, rect)) =
                crate::placement::warp_soft(&source, device, planes, self.scale, self.warp_extent())
            {
                self.backend.surface_image(Arc::new(pixels), rect);
            }
        }
        let hit_plane = self::plane(&mul(&hit_m, &island_origin));
        let hit_device = crate::placement::compose(hit_plane, hit_ts, 0., 0.);
        let hit_inv = crate::placement::inverse(hit_device).unwrap_or(inv);
        project(walk, child_walk, inv, clip, hit_inv, hit_clip);
        true
    }
}

/// Move the plane's painted boxes and hit maps into its parent's coordinates.
fn project(
    walk: &mut Walk<'_, '_>,
    child_walk: Walk<'_, '_>,
    inv: [f32; 9],
    clip: Option<Rect4>,
    hit_inv: [f32; 9],
    hit_clip: Option<Rect4>,
) {
    walk.text.extend(child_walk.text);
    walk.fields.extend(child_walk.fields);
    walk.inline.extend(child_walk.inline);
    for b in child_walk.boxes {
        let hit = project_box(b.for_hit(), hit_inv, hit_clip);
        let mut b = project_box(b, inv, clip);
        b.hit = Some(hit.geometry());
        walk.boxes.push(b);
    }
}

fn project_box(mut b: PaintedBox, inv: [f32; 9], clip: Option<Rect4>) -> PaintedBox {
    b.map_clips(inv);
    let planes = [[0., 0., 1.]; 2];
    // Compose nested planes and keep clips in the plane that owns them.
    let (local, local_clip, inv, planes) = match b.projective {
        Some((inner, r, c, p)) => {
            let mid = b.clip.and_then(|m| mapped_bounds(&inner, m));
            let c = match (c, mid) {
                (Some(c), Some(m)) => Some(intersect(c, m)),
                (c, m) => c.or(m),
            };
            (r, c, mul3(&inner, &inv), p)
        }
        None => (b.rect, b.clip, inv, planes),
    };
    if let Some(map) = crate::placement::inverse(inv) {
        b.rect = crate::placement::clipped_bounds(&map, planes, local);
    }
    b.projective = Some((inv, local, local_clip, planes));
    b.clip = clip;
    b
}

// A painted box in a scroll container's coordinates (`presenter/reveal.rs`, `presenter/links.rs`).
impl PaintedBox {
    /// The complete mapping from this box's scroll coordinates to the viewport.
    pub(crate) fn scroll_mapping(&self) -> Option<[f32; 9]> {
        let ts = self.affine.map_or(Transform::identity(), |(ts, _)| ts);
        let affine = [ts.sx, ts.kx, ts.tx, ts.ky, ts.sy, ts.ty, 0., 0., 1.];
        match self.projective {
            Some((inv, ..)) => Some(space::mul3(&crate::placement::inverse(inv)?, &affine)),
            None => Some(affine),
        }
    }

    pub(crate) fn scroll_rect(&self) -> Rect4 {
        self.affine.map_or_else(
            || self.projective.map_or(self.rect, |(_, rect, ..)| rect),
            |(_, rect)| rect,
        )
    }

    /// Bounds in a scroll container's plane, retaining the painter's depth clipping.
    pub(crate) fn scroll_bounds(&self, mapping: &[f32; 9]) -> Option<Rect4> {
        let Some((_, _, _, planes)) = self.projective else {
            return space::mapped_bounds(mapping, self.scroll_rect());
        };
        // Depth planes are in the projective island, after the affine transform.
        let planes = planes.map(|[x, y, z]| {
            self.affine.map_or([x, y, z], |(ts, _)| {
                [
                    x * ts.sx + y * ts.ky,
                    x * ts.kx + y * ts.sy,
                    x * ts.tx + y * ts.ty + z,
                ]
            })
        });
        let bounds = crate::placement::clipped_bounds(mapping, planes, self.scroll_rect());
        (bounds != (0., 0., 0., 0.)
            && [bounds.0, bounds.1, bounds.2, bounds.3]
                .iter()
                .all(|n| n.is_finite()))
        .then_some(bounds)
    }

    /// An inline run replaces the paragraph's local box while keeping its transforms.
    pub(crate) fn with_scroll_rect(mut self, rect: Rect4) -> Option<Self> {
        let hit = self.hit.take().and_then(|hit| {
            let mut b = self.clone();
            b.hit = Some(hit);
            b.for_hit().with_scroll_rect(rect).map(|b| b.geometry())
        });
        let local = if let Some((ts, local)) = self.affine.as_mut() {
            *local = rect;
            bbox(*ts, rect)
        } else {
            rect
        };
        if let Some((_, bounds, _, _)) = self.projective.as_mut() {
            *bounds = local;
        }
        self.rect = rect;
        self.rect = self.scroll_bounds(&self.scroll_mapping()?)?;
        self.hit = hit;
        Some(self)
    }
}

/// @ref LLP 1083 D4 — a sticky box moves by its constraint's offset at its
/// scroller's scroll: the scroller's own, the page's for a root that does
/// not scroll itself, none for a box that only clips. Painted there, it is
/// hit there too.
pub(super) fn sticky(
    scene: &Scene<'_>,
    id: ViewId,
    offset: PaintOffset,
    grid: Option<f32>,
    scroll_grids: &[(ViewId, Option<f32>)],
) -> PaintOffset {
    let kernel = scene.kernel;
    let Some(node) = kernel
        .node(id)
        .filter(|n| n.layout_position() == exact_kernel::PositionType::Sticky)
    else {
        return offset;
    };
    let scroll_grid = sticky_owner_grid(scene, &node, scroll_grids).flatten();
    let (dx, dy) = sticky_displacement(scene, &node, grid, scroll_grid);
    (offset.0 - dx, offset.1 - dy)
}

pub(super) fn sticky_displacement(
    scene: &Scene<'_>,
    node: &NodeRef<'_>,
    grid: Option<f32>,
    scroll_grid: Option<f32>,
) -> PaintOffset {
    let kernel = scene.kernel;
    let Some((c, scroller)) = kernel
        .sticky_constraint(node.key)
        .and_then(|c| Some((c, kernel.node(c.scroller)?)))
    else {
        return (0., 0.);
    };
    let scroll = sticky_scroll(scene, &scroller, scroll_grid);
    let grid = grid.filter(|_| !grid::leaves_grid(node, &(scene.presented)(node.id)));
    let (dx, dy) = grid::sticky_shift(c, &scroller, scroll, grid);
    match grid {
        Some(s) => (
            grid::device_pixel(dx, s) / f64::from(s),
            grid::device_pixel(dy, s) / f64::from(s),
        ),
        None => (dx.into(), dy.into()),
    }
}

pub(super) fn sticky_owner_grid(
    scene: &Scene<'_>,
    node: &NodeRef<'_>,
    scroll_grids: &[(ViewId, Option<f32>)],
) -> Option<Option<f32>> {
    let scroller = scene
        .kernel
        .node(scene.kernel.sticky_constraint(node.key)?.scroller)?;
    scroll_grid(&scroller, scroll_grids)
}

pub(super) fn scroll_grid(
    scroller: &NodeRef<'_>,
    scroll_grids: &[(ViewId, Option<f32>)],
) -> Option<Option<f32>> {
    let id = if effective_overflow(scroller) == (Overflow::Visible, Overflow::Visible)
        && scroller.parent.is_none()
    {
        crate::presenter::PAGE
    } else {
        scroller.id
    };
    scroll_grids
        .iter()
        .rfind(|(owner, _)| *owner == id)
        .map(|(_, grid)| *grid)
}

pub(super) fn sticky_scroll(
    scene: &Scene<'_>,
    scroller: &NodeRef<'_>,
    grid: Option<f32>,
) -> (f32, f32) {
    let scroll = if effective_overflow(scroller) != (Overflow::Visible, Overflow::Visible) {
        scene
            .scroll
            .get(&scroller.id)
            .copied()
            .unwrap_or((0.0, 0.0))
    } else if scroller.parent.is_none() {
        scene.page
    } else {
        (0.0, 0.0)
    };
    let (x, y) = grid::scroll_offset(scroll, grid);
    (x as f32, y as f32)
}

pub(crate) fn content_size_with(
    node: &NodeRef<'_>,
    kernel: &Kernel,
    presented: &dyn Fn(ViewId) -> Presented,
    scaled_presented: &std::collections::BTreeSet<ViewId>,
) -> (f32, f32) {
    fn overflow(
        node: &NodeRef<'_>,
        kernel: &Kernel,
        presented: &dyn Fn(ViewId) -> Presented,
    ) -> Option<(Rect4, bool)> {
        let mut p = presented(node.id);
        p.press = 1.;
        if p.scale.0 == 0. || p.scale.1 == 0. {
            return None;
        }
        let f = node.frame;
        let border = (f.x, f.y, f.width, f.height);
        let axes = effective_overflow(node);
        let mut bounds = border;
        let mut scaled_below = false;
        for id in node.children() {
            let Some(child) = kernel.node_unblocked(id) else {
                continue;
            };
            scaled_below |= presented(id).scale != (1., 1.);
            let Some((c, scaled)) = overflow(&child, kernel, presented) else {
                continue;
            };
            scaled_below |= scaled;
            let left = if axes.0 == Overflow::Visible {
                bounds.0.min(c.0)
            } else {
                bounds.0
            };
            let top = if axes.1 == Overflow::Visible {
                bounds.1.min(c.1)
            } else {
                bounds.1
            };
            let right = if axes.0 == Overflow::Visible {
                (bounds.0 + bounds.2).max(c.0 + c.2)
            } else {
                bounds.0 + bounds.2
            };
            let bottom = if axes.1 == Overflow::Visible {
                (bounds.1 + bounds.3).max(c.1 + c.3)
            } else {
                bounds.1 + bounds.3
            };
            bounds = (left, top, right - left, bottom - top);
        }
        // Keep layout overflow on branches whose descendants were not rescaled.
        if !scaled_below {
            bounds.2 = bounds.2.max(node.content.0);
            bounds.3 = bounds.3.max(node.content.1);
        }
        let map = space::mapping(kernel, node, &p);
        let after = space::mapped_bounds(&map, bounds).unwrap_or(border);
        // CSS Transforms 1 section 3: keep the border box before and after
        // its transform. Descendants pass through their ancestor's transform.
        let left = border.0.min(after.0);
        let top = border.1.min(after.1);
        let right = (border.0 + border.2).max(after.0 + after.2);
        let bottom = (border.1 + border.3).max(after.1 + after.3);
        Some((
            (left, top, right - left, bottom - top),
            scaled_below || p.scale != (1., 1.),
        ))
    }
    // Check the sparse scale sets through parent links. Unrelated swipe
    // indicators must not make every scroller search its whole subtree.
    let transformed = kernel.has_scaled_descendant(node.key)
        || scaled_presented.iter().any(|&id| {
            let mut parent = kernel.node(id).and_then(|n| n.parent);
            while let Some(id) = parent {
                let Some(ancestor) = kernel.node(id) else {
                    break;
                };
                if ancestor.key == node.key {
                    return true;
                }
                parent = ancestor.parent;
            }
            false
        });
    if !transformed {
        let (_, _, pr, pb) = kernel.layout_padding(node.key).unwrap_or_default();
        let mut size = (
            node.frame.width.max(node.content.0),
            node.frame.height.max(node.content.1),
        );
        for id in node.children() {
            if let Some(c) = kernel.node_unblocked(id) {
                size.0 = size.0.max(c.frame.x - node.frame.x + c.frame.width + pr);
                size.1 = size.1.max(c.frame.y - node.frame.y + c.frame.height + pb);
            }
        }
        return size;
    }
    let (_, _, pr, pb) = kernel.layout_padding(node.key).unwrap_or_default();
    let mut size = (node.frame.width, node.frame.height);
    for id in node.children() {
        if let Some(child) = kernel.node_unblocked(id) {
            let Some((r, _)) = overflow(&child, kernel, presented) else {
                continue;
            };
            size.0 = size.0.max(r.0 + r.2 - node.frame.x + pr);
            size.1 = size.1.max(r.1 + r.3 - node.frame.y + pb);
        }
    }
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{presenter::PainterChoice, Presenter};
    use std::{cell::Cell, path::PathBuf, time::Instant};

    #[test]
    fn unrelated_swipe_scales_do_not_walk_scroll_content() {
        let source = r#"component App
  state n = 0
  action noop
    n = n + 1
  view
    column
      list testId="port" width=120 height=120
        for row in range(1000) key=row
          view width=160 height=20 translate="50px 70px"
            view width=100 height=20
      list testId="other" width=120 height=120
        for row in range(1000) key=row
          view width=100 height=20 swiperight=noop
            view swipeIndicator=true width=10 height=10 scale=0.5
"#;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (400., 500.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("port")[0])
            .unwrap();
        assert!(kernel.has_scale());
        assert!(!kernel.has_scaled_descendant(node.key));
        // Main uses layout overflow and immediate children, ignoring translate.
        let expected = (160., 20_000.);
        let reads = Cell::new(0);
        let presented = |id| {
            reads.set(reads.get() + 1);
            p.host().presented(id)
        };
        let start = Instant::now();
        for _ in 0..1000 {
            assert_eq!(
                content_size_with(&node, kernel, &presented, p.host().scaled_presented()),
                expected
            );
        }
        eprintln!("1000-row content_size, 1000 reads: {:?}", start.elapsed());
        assert_eq!(
            reads.get(),
            0,
            "the transform path must not read presentation"
        );

        // Held scales use a separate sparse set and obey the same boundary.
        let other = kernel
            .node_by_key(kernel.find_by_test_id("other")[0])
            .unwrap()
            .id;
        let port = node.id;
        for id in [other, port] {
            let held = p
                .host_mut()
                .hold_begin(id, exact_motion::Property::Scale, 0.)
                .unwrap()
                .unwrap();
            p.host_mut()
                .hold_update(held.token, exact_motion::Value::new(2., 0.5), 0.)
                .unwrap();
            // The hold must reach the presented-scale set, or this section tests nothing.
            assert!(p.host().scaled_presented().contains(&id));
            let kernel = p.host().kernel();
            let node = kernel.node(port).unwrap();
            assert_eq!(
                content_size_with(
                    &node,
                    kernel,
                    &|_| panic!("an unrelated or own scale must not enter the transform path"),
                    p.host().scaled_presented(),
                ),
                expected
            );
            p.host_mut()
                .hold_end(held.token, exact_motion::HoldEnd::Cancel, 0.)
                .unwrap();
        }
    }
}
