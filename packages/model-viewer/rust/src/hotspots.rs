//! Canvas-child anchors and occlusion (LLP 1133.039 D8).
use crate::{
    arguments::{error, list, normal, position, string, Arguments},
    bvh::Ray,
    camera::Pose,
    styles::{self, Occlusion},
    Model,
};
use exact_gpu::{wgpu, Frame, Placement, SurfaceError, Value};
use exact_model_gltf::{math::*, scene::Scene};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hotspot {
    pub child: String,
    pub node: String,
    pub position: Option<V3>,
    pub normal: Option<V3>,
}
impl Hotspot {
    pub fn parse_list(value: &Value) -> Result<Vec<Self>, SurfaceError> {
        list(value, "hotspots")?
            .iter()
            .enumerate()
            .map(|(i, value)| {
                let name = format!("hotspots[{i}]");
                let fields = match value {
                    Value::Record(v) | Value::List(v) => v,
                    _ => return Err(error(&name, "expected a ModelHotspot record")),
                };
                if fields.len() > 4 {
                    return Err(error(&name, "too many fields"));
                }
                let mut h = Self::default();
                if let Some(v) = fields.first() {
                    h.child = string(v, &format!("{name}.child"))?.into();
                }
                if let Some(v) = fields.get(1) {
                    h.node = string(v, &format!("{name}.node"))?.into();
                }
                if let Some(v) = fields.get(2) {
                    let field = format!("{name}.position");
                    let s = string(v, &field)?;
                    if !s.is_empty() {
                        let p = position(s, &field)?;
                        let [Some(x), Some(y), Some(z)] = p else {
                            return Err(error(&field, "expected three lengths"));
                        };
                        h.position = Some([x, y, z]);
                    }
                }
                if let Some(v) = fields.get(3) {
                    let field = format!("{name}.normal");
                    let s = string(v, &field)?;
                    if !s.is_empty() {
                        h.normal = Some(normal(s, &field)?);
                    }
                }
                Ok(h)
            })
            .collect()
    }
}

#[derive(Default)]
pub struct Child {
    pub name: String,
    pub frame: [f32; 4],
    pub texture: Option<wgpu::TextureView>,
    pub group: Option<wgpu::BindGroup>,
    pub placement: Option<Placement>,
    pub opacity: f32,
    occluded: bool,
}
#[derive(Default)]
pub struct Hotspots {
    pub reporting: bool,
    pub occlusion_ms: Option<f64>,
    pub occlusion_total_ms: f64,
    pub occlusion_updates: u64,
    pub children: Vec<Child>,
    pub dirty: bool,
    tested_pose: Option<Pose>,
    tested_at: Option<f64>,
    pub gpu: Option<crate::labels::Labels>,
}
impl Hotspots {
    pub fn child(
        &mut self,
        index: usize,
        name: &str,
        texture: Option<&wgpu::TextureView>,
        frame: [f32; 4],
    ) {
        if name.is_empty() && texture.is_none() && frame == [0.; 4] {
            if let Some(child) = self.children.get_mut(index) {
                *child = Child::default();
            }
            while self
                .children
                .last()
                .is_some_and(|c| c.name.is_empty() && c.frame == [0.; 4])
            {
                self.children.pop();
            }
            return;
        }
        self.children
            .resize_with(self.children.len().max(index + 1), Child::default);
        let child = &mut self.children[index];
        self.dirty |= child.name != name;
        child.name = name.into();
        child.frame = frame;
        if child.texture.as_ref() != texture {
            child.texture = texture.cloned();
            child.group = None;
        }
    }

    pub fn update(&mut self, model: Option<&Model>, a: &Arguments, frame: &Frame) {
        let retest = model.is_some_and(|m| {
            self.dirty
                || self.tested_pose.is_none()
                || (!m.camera.moving() && self.tested_pose != Some(m.camera.pose))
                || (m.camera.moving()
                    && self
                        .tested_at
                        .is_none_or(|t| frame.now_ms - t >= 100. || frame.now_ms < t))
        });
        let node_styles = model.map(|m| styles::resolve(&m.scene, &a.nodes));
        let start = (self.reporting && retest).then(crate::timing::Stamp::now);
        for child in &mut self.children {
            child.placement = None;
            child.opacity = 1.;
            let Some(h) = a
                .hotspots
                .iter()
                .rev()
                .find(|h| !h.child.is_empty() && h.child == child.name)
            else {
                continue;
            };
            let mut placement = Placement {
                hidden: true,
                homography: [1., 0., 0., 0., 1., 0., 0., 0., 1.],
                depth: 0.,
                clip_depth: [[0., 0., 1.]; 2],
            };
            child.placement = Some(placement);
            let Some(m) = model else { continue };
            let Some(node) = m
                .scene
                .nodes
                .iter()
                .position(|n| n.active && n.name == h.node)
            else {
                continue;
            };
            let n = &m.scene.nodes[node];
            let anchor = h.position.unwrap_or_else(|| {
                n.subtree_bounds
                    .map_or_else(|| transform_point(&n.world, [0.; 3]), |b| b.centre())
            });
            let eye = m.camera.pose.eye();
            let delta = sub(anchor, eye);
            let (right, up, back) = m.camera.pose.basis();
            let depth = -dot(delta, back);
            if depth <= 0. || !m.clip.keeps(anchor) {
                continue;
            }
            if retest {
                child.occluded = occluded(m, node, anchor, h.normal);
            }
            let unit = depth * (m.camera.vertical_fov(frame.width / frame.height) * 0.5).tan();
            let x = frame.width * 0.5 + dot(delta, right) * frame.height / (2. * unit);
            let y = frame.height * 0.5 - dot(delta, up) * frame.height / (2. * unit);
            if !(0. ..=frame.width).contains(&x) || !(0. ..=frame.height).contains(&y) {
                continue;
            }
            // A group is visible if any of its geometry is visible. Empty nodes use their style.
            let mut geometry = false;
            let mut visible = false;
            for (p, effective) in m.scene.primitives.iter().zip(&m.effective) {
                if descendant(&m.scene, p.node, node) {
                    geometry = true;
                    visible |= effective.visible;
                }
            }
            if geometry && !visible {
                continue;
            }
            let style = &node_styles.as_ref().unwrap()[node];
            if !geometry
                && (!style.visible
                    || (style.opacity == 0. && style.occlusion != Occlusion::Occluder))
            {
                continue;
            }
            placement.hidden = false;
            placement.depth = -depth;
            placement.homography[2] =
                ((x - child.frame[2] * 0.5) * frame.scale).round() / frame.scale;
            placement.homography[5] =
                ((y - child.frame[3] * 0.5) * frame.scale).round() / frame.scale;
            child.placement = Some(placement);
            child.opacity = if child.occluded {
                a.min_hotspot_opacity
            } else {
                1.
            };
        }
        if retest {
            self.occlusion_ms = start.map(|s| s.elapsed());
            if let Some(ms) = self.occlusion_ms {
                self.occlusion_total_ms += ms;
                self.occlusion_updates += 1;
            }
            self.tested_pose = model.map(|m| m.camera.pose);
            self.tested_at = Some(frame.now_ms);
            self.dirty = false;
        }
    }

    pub fn reset_timing(&mut self) {
        self.occlusion_ms = None;
        self.occlusion_total_ms = 0.;
        self.occlusion_updates = 0;
    }
}

fn descendant(scene: &Scene, mut node: usize, root: usize) -> bool {
    loop {
        if node == root {
            return true;
        }
        let Some(parent) = scene.nodes[node].parent else {
            return false;
        };
        node = parent;
    }
}

pub(super) fn occluded(m: &Model, node: usize, anchor: V3, normal: Option<V3>) -> bool {
    let eye = m.camera.pose.eye();
    let delta = sub(anchor, eye);
    if normal.is_some_and(|n| dot(n, delta) >= 0.) {
        return true;
    }
    let distance = dot(delta, delta).sqrt();
    let direction = norm(delta);
    let near = m.camera.ray_depth(direction)[0];
    for (p_index, (p, s)) in m.scene.primitives.iter().zip(&m.effective).enumerate() {
        if !s.visible || !s.opaque() || descendant(&m.scene, p.node, node) {
            continue;
        }
        if m.bvh.hits(
            &m.scene,
            p_index,
            Ray {
                origin: eye,
                direction,
                depth: [near, distance],
                clip: m.clip,
            },
            |t, _, _| t < distance,
        ) {
            return true;
        }
    }
    false
}
