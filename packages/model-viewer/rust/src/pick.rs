use crate::{
    bvh::{Bvh, Ray},
    styles::{Effective, Occlusion},
};
use exact_model_gltf::{math::*, scene::Scene};

#[derive(Clone, Copy, Default)]
pub struct Clip {
    pub position: V3,
    pub normal: V3,
}
impl Clip {
    pub fn keeps(self, point: V3) -> bool {
        dot(sub(point, self.position), self.normal) >= 0.
    }
}

pub fn triangle(origin: V3, direction: V3, a: V3, b: V3, c: V3) -> Option<f32> {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let h = cross(direction, ac);
    let det = dot(ab, h);
    if det.abs() < 1e-12 {
        return None;
    }
    let q = sub(origin, a);
    let u = dot(q, h) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let r = cross(q, ab);
    let v = dot(direction, r) / det;
    if v < 0. || u + v > 1. {
        return None;
    }
    let t = dot(ac, r) / det;
    (t > 0.).then_some(t)
}

pub fn pick(
    scene: &Scene,
    bvh: &Bvh,
    effective: &[Effective],
    clip: Clip,
    origin: V3,
    direction: V3,
    depth: [f32; 2],
) -> String {
    let nearer = |hit: &mut Option<(f32, usize, u32)>, t, i, offset| {
        if hit
            .is_none_or(|(nearest, primitive, first)| (t, i, offset) < (nearest, primitive, first))
        {
            *hit = Some((t, i, offset));
        }
    };
    let mut occluder = None;
    let ray = Ray {
        origin,
        direction,
        depth,
        clip,
    };
    for (i, style) in effective.iter().enumerate() {
        if style.visible && style.occlusion == Occlusion::Occluder {
            bvh.hits(scene, i, ray, |t, offset, _| {
                nearer(&mut occluder, t, i, offset);
                false
            });
        }
    }
    let occluder_depth = occluder.map_or(f32::INFINITY, |(t, _, _)| t);
    let (mut opaque, mut revealed, mut translucent) = (None, None, None);
    for (i, style) in effective.iter().enumerate() {
        if !style.visible || style.occlusion == Occlusion::Occluder {
            continue;
        }
        bvh.hits(scene, i, ray, |t, offset, front| {
            if style.opaque() && t < occluder_depth {
                nearer(&mut opaque, t, i, offset);
            }
            // A primitive can cross the occluder: its nearer hit must not hide its faded hit.
            if style.occlusion == Occlusion::Revealed
                && t > occluder_depth
                && style.occluded_opacity > 0.
            {
                nearer(&mut revealed, t, i, offset);
            }
            if !style.opaque() && front {
                nearer(&mut translucent, t, i, offset);
            }
            false
        });
    }
    opaque
        .or(revealed)
        .or(occluder)
        .or(translucent)
        .map(|(_, i, _)| scene.nodes[scene.primitives[i].node].name.clone())
        .unwrap_or_default()
}
