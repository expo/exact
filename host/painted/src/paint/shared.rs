//! Named live subtrees kept by the Android reader (LLP 1013.000).
use super::*;
use exact_kernel::motion::motion_node;

/// The reader captures geometry before applying the frame that carries this state.
#[derive(Default)]
pub struct SharedState {
    pub(crate) frames: Vec<crate::host::flights::FlightFrame>,
    pub(crate) reduced: bool,
    pub(crate) alive: Vec<u64>,
    pub(crate) recorded: std::collections::HashSet<u64>,
    epoch: Option<u64>,
}

impl SharedState {
    pub(crate) fn update<D: exact_runner::DataSource>(&mut self, host: &crate::Host<D>) {
        self.frames = host.flight_frames();
        self.reduced = host.runner().viewport().preferences.reduced_motion;
        self.epoch = Some(host.kernel().epoch());
    }
}

/// A named subtree's geometry, before its reader's scroll and motion transforms.
#[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
pub struct SharedElement<'a> {
    pub(crate) key: u64,
    pub(crate) rect: Rect4,
    pub(crate) radius: f32,
    pub(crate) opacity: f32,
    pub(crate) transform: Transform,
    pub(crate) image: Option<(&'a Arc<Bitmap>, Rect4)>,
    pub(crate) fit: u32,
    pub(crate) content: Rect4,
}

impl Painter {
    /// Kept rows do not visit their names again. Publish their current paint order
    /// with the lifecycle so flights retain stacking when those rows are reused.
    pub(super) fn order_shared(&mut self, scene: &Scene<'_>) {
        self.shared.alive.clear();
        if !scene.kernel.has_prop(PropId::SharedElement) {
            self.shared.recorded.clear();
            return;
        }
        let mut stack: Vec<_> = scene.roots.iter().copied().rev().collect();
        while let Some(id) = stack.pop() {
            let Some(node) = scene.kernel.node(id) else {
                continue;
            };
            // A subtree that does not render records no definition, so its
            // names are not alive: the reader must not keep flying them.
            if (scene.hidden)(id) || node.style.display == Display::None {
                continue;
            }
            if node
                .props
                .str(PropId::SharedElement)
                .is_some_and(|s| !s.is_empty())
            {
                self.shared.alive.push(motion_node(node.key));
            }
            let mut children = node.children();
            self.order_children(&mut children, &self.ranks);
            if let Some((_, lifted)) = self.lift.arrange.filter(|(owner, _)| *owner == node.key) {
                if let Some(at) = children
                    .iter()
                    .position(|id| scene.kernel.node(*id).is_some_and(|n| n.key == lifted))
                {
                    let lifted = children.remove(at);
                    children.push(lifted);
                }
            }
            stack.extend(children.into_iter().rev());
        }
        let alive: std::collections::HashSet<_> = self.shared.alive.iter().copied().collect();
        self.shared.recorded.retain(|key| alive.contains(key));
    }

    /// A flight whose destination stopped rendering ends with its definition.
    pub(super) fn drop_unrendered_flights(&mut self) {
        let alive = &self.shared.alive;
        self.shared
            .frames
            .retain(|f| alive.contains(&motion_node(f.to)));
    }

    pub(super) fn node_at(
        &mut self,
        walk: &mut Walk<'_, '_>,
        id: ViewId,
        ts: Transform,
        offset: (f32, f32),
        clip: Option<Rect4>,
    ) {
        let named = self.backend.name() == "canvas"
            && self.shared.epoch.is_some()
            && walk.dialog.is_none()
            && walk.scene.kernel.node(id).is_some_and(|n| {
                n.props
                    .str(PropId::SharedElement)
                    .is_some_and(|s| !s.is_empty())
                    && n.style.display != Display::None
                    && !(walk.scene.hidden)(id)
            });
        if !named {
            self.node_content(walk, id, ts, offset, clip);
            return;
        }
        let node = walk.scene.kernel.node(id).expect("named live node");
        let rect = paint_rect(node.frame, sticky(walk.scene, id, offset));
        let shown = (walk.scene.presented)(id);
        let paint = BoxPaint::capture(&node, walk.scene.kernel, self.dark);
        let geometry = paint.geometry(rect);
        let transform = if shown.lowered & crate::host::lower::LOWER_TRANSFORM == 0 {
            ts.pre_concat(
                shown.transform(rect, node.style.transform_origin.resolve(rect.2, rect.3)),
            )
        } else {
            ts.pre_translate(shown.layout[0], shown.layout[1])
        };
        // `visibility: hidden` paints no pixels of this node, so a flight must
        // not draw its raster either; visible descendants stay live content.
        let painted = paints(walk.scene.kernel, id, walk.reveal);
        let image = (painted && node.node_type == NodeType::Image)
            .then(|| walk.scene.images.get(&id))
            .flatten()
            .and_then(|image| {
                object_fit(image.natural(), node.style.object_fit, geometry.content)
                    .map(|rect| (image, rect))
            });
        // Symbols, animated pictures and decorated images keep their live drawing.
        // A plain raster can independently interpolate its fitted image rectangle.
        let plain_image = painted
            && node.node_type == NodeType::Image
            && !node
                .props
                .str(PropId::ImageSource)
                .is_some_and(|s| s.starts_with("symbol:"))
            && image_tint(&node, &shown, self.dark).is_none()
            && image.is_none_or(|(i, _)| i.animation_file().is_none())
            && node.style.filter.is_none()
            && node.style.rare.mask_image.gradient().is_none()
            && node.style.rare.clip_path.commands().is_empty()
            && paint.background[3] == 0
            && paint.gradients.is_empty()
            && paint.widths.iter().all(|w| *w == 0.0)
            && paint.shadows.is_empty();
        let radius = geometry
            .outer
            .radii
            .iter()
            .flat_map(|&(x, y)| [x, y])
            .fold(0.0, f32::max);
        let opened = self.backend.shared_begin(&SharedElement {
            key: motion_node(node.key),
            rect: shown.surface(rect),
            radius,
            opacity: shown.opacity,
            transform,
            image,
            fit: if !plain_image {
                0
            } else {
                match node.style.object_fit {
                    ObjectFit::Fill => 1,
                    ObjectFit::Contain => 2,
                    ObjectFit::Cover => 3,
                    ObjectFit::None => 4,
                    ObjectFit::ScaleDown => 5,
                }
            },
            content: geometry.content,
        });
        self.node_content(walk, id, ts, offset, clip);
        if opened {
            self.backend.shared_end();
            self.shared.recorded.insert(motion_node(node.key));
        }
    }

    pub(super) fn flight_geometry(&self, node: &NodeRef<'_>, geometry: &mut BoxGeometry) {
        if self.shared.frames.iter().any(|f| f.to == node.key) && !self.shared.reduced {
            geometry.outer.radii = [(0.0, 0.0); 4];
        }
    }
}
