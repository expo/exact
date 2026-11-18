use super::*;

#[test]
fn triangle_boundary_picks_and_occlusion_match_brute_force() {
    let mut s = viewer();
    let m = s.model_mut().unwrap();
    (m.scene, m.effective) = layered_scene(&[(0., true)], false);
    m.scene.nodes[m.scene.primitives[0].node].name = "Panel".into();
    m.effective[0].color[3] = 1.;
    m.effective[0].occlusion = Occlusion::None;
    m.bvh = crate::bvh::Bvh::new(&m.scene);
    let origin = [-1., -0.90000004, 0.2];
    let vertex = [-1., -1., 0.];
    let direction = norm(sub(vertex, origin));
    let depth = [0.01, 100.];
    let expected = brute::pick(&m.scene, &m.effective, m.clip, origin, direction, depth);
    assert_eq!(expected, "Panel");
    assert_eq!(
        pick::pick(
            &m.scene,
            &m.bvh,
            &m.effective,
            m.clip,
            origin,
            direction,
            depth
        ),
        expected
    );
    m.camera.pose.target = origin;
    m.camera.pose.radius = 0.;
    let node = m.scene.find("Housing").unwrap();
    let mut blocked = 0;
    for distance in [0.5, 1., 2., 4., 8.] {
        let anchor = add(origin, mul(direction, distance));
        let expected = brute_occluded(m, node, anchor, None);
        assert_eq!(crate::hotspots::occluded(m, node, anchor, None), expected);
        blocked += usize::from(expected);
    }
    assert!(blocked > 0);
}

#[test]
fn accelerated_picks_and_occlusion_match_brute_force_over_random_rays() {
    use exact_model_gltf::scene::Vertex;
    let mut seed = 0x0072_4113_3039_u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 40) as f32 / (1u32 << 24) as f32
    };
    let mut s = viewer();
    let m = s.model_mut().unwrap();
    for p in &mut m.scene.primitives {
        let start = m.scene.indices.len() as u32;
        for _ in 0..64 {
            let centre = std::array::from_fn::<_, 3, _>(|_| random() * 6. - 3.);
            for _ in 0..3 {
                m.scene.indices.push(m.scene.vertices.len() as u32);
                m.scene.vertices.push(Vertex {
                    p: std::array::from_fn(|k| centre[k] + random() * 2. - 1.),
                    n: [0., 0., 1.],
                    node: p.node as u32,
                    primitive: 0,
                });
            }
        }
        p.indices = start..m.scene.indices.len() as u32;
    }
    m.bvh = crate::bvh::Bvh::new(&m.scene);
    let mut picked = 0;
    let mut blocked = 0;
    for i in 0..4096 {
        m.camera.pose.theta = random() * std::f32::consts::TAU;
        m.camera.pose.phi = random() * std::f32::consts::PI;
        m.camera.pose.radius = random() * 6. + 0.01;
        let eye = m.camera.pose.eye();
        let mut anchor = std::array::from_fn(|_| random() * 8. - 4.);
        if i % 8 == 1 {
            anchor = m.scene.vertices[(random() * m.scene.vertices.len() as f32) as usize].p;
        }
        if i % 4 == 0 {
            anchor[i % 3] = eye[i % 3];
        }
        let direction = norm(sub(anchor, eye));
        let depth = if i % 2 == 0 {
            [random(), random() * 6. + 1.]
        } else {
            m.camera.ray_depth(direction)
        };
        m.clip = Clip {
            position: std::array::from_fn(|_| random() * 2. - 1.),
            normal: if i % 3 == 0 {
                [0.; 3]
            } else {
                std::array::from_fn(|_| random() * 2. - 1.)
            },
        };
        for style in &mut m.effective {
            style.visible = random() > 0.15;
            style.color[3] = if random() > 0.4 { 1. } else { 0.5 };
            style.occluded_opacity = if random() > 0.2 { 0.3 } else { 0. };
            style.occlusion = match (random() * 3.) as u32 {
                0 => Occlusion::None,
                1 => Occlusion::Occluder,
                _ => Occlusion::Revealed,
            };
        }
        let expected = brute::pick(&m.scene, &m.effective, m.clip, eye, direction, depth);
        assert_eq!(
            pick::pick(
                &m.scene,
                &m.bvh,
                &m.effective,
                m.clip,
                eye,
                direction,
                depth
            ),
            expected,
            "pick ray {i}"
        );
        picked += usize::from(!expected.is_empty());
        let node = i % m.scene.nodes.len();
        let normal = (i % 5 == 0).then(|| std::array::from_fn(|_| random() * 2. - 1.));
        let expected = brute_occluded(m, node, anchor, normal);
        assert_eq!(
            crate::hotspots::occluded(m, node, anchor, normal),
            expected,
            "occlusion ray {i}"
        );
        blocked += usize::from(expected);
    }
    assert!(picked > 100 && picked < 3996, "{picked}");
    assert!(blocked > 100 && blocked < 3996, "{blocked}");
}

fn brute_occluded(m: &Model, node: usize, anchor: V3, normal: Option<V3>) -> bool {
    let eye = m.camera.pose.eye();
    let delta = sub(anchor, eye);
    if normal.is_some_and(|n| dot(n, delta) >= 0.) {
        return true;
    }
    let distance = dot(delta, delta).sqrt();
    let direction = norm(delta);
    let near = m.camera.ray_depth(direction)[0];
    for (p, style) in m.scene.primitives.iter().zip(&m.effective) {
        let mut parent = Some(p.node);
        let mut excluded = false;
        while let Some(index) = parent {
            excluded |= index == node;
            parent = m.scene.nodes[index].parent;
        }
        if !style.visible || !style.opaque() || excluded {
            continue;
        }
        for tri in m.scene.indices[p.indices.start as usize..p.indices.end as usize]
            .as_chunks::<3>()
            .0
        {
            let [a, b, c] = std::array::from_fn(|k| m.scene.vertices[tri[k] as usize].p);
            if pick::triangle(eye, direction, a, b, c).is_some_and(|t| {
                t >= near && t < distance && m.clip.keeps(add(eye, mul(direction, t)))
            }) {
                return true;
            }
        }
    }
    false
}

/// Run on demand with EXACT_MODEL_BENCH_GLB pointing at the generated D11 model.
#[test]
#[ignore = "D11 wall-clock measurement, outside the hourly lane"]
fn generated_model_ray_budgets() {
    let bytes = std::fs::read(std::env::var("EXACT_MODEL_BENCH_GLB").unwrap()).unwrap();
    let mut s = viewer();
    s.set_performance_reporting(true);
    s.asset("assembly.glb", Ok(&bytes));
    assert_eq!(s.model().unwrap().scene.triangles(), 500_000);
    let mut a = s.arguments.clone();
    a.hotspots = (0..20)
        .map(|i| crate::hotspots::Hotspot {
            child: format!("h{i}"),
            node: "Impeller".into(),
            position: Some([
                (i % 5) as f32 * 0.18 - 0.36,
                (i / 5) as f32 * 0.18 - 0.27,
                0.5,
            ]),
            normal: None,
        })
        .collect();
    for (i, h) in a.hotspots.iter().enumerate() {
        s.child(i, &h.child, None, [0., 0., 10., 10.]);
    }
    s.arguments = a;
    eprintln!(
        "BVH {} bytes; parse/build {:.3} ms",
        s.model().unwrap().bvh.bytes(),
        s.timing.as_ref().unwrap().parse_ms.unwrap()
    );
    let mut picks = vec![];
    let mut occlusion = vec![];
    for i in 0..26 {
        message(&mut s, "pick 200 200", 0.);
        assert_eq!(s.messages(), ["Impeller"]);
        let mut h = std::mem::take(&mut s.hotspots);
        h.dirty = true;
        h.update(s.model(), &s.arguments, &frame(0.));
        if i > 0 {
            picks.push(s.timing.as_ref().unwrap().pick_ms.unwrap());
            occlusion.push(h.occlusion_ms.unwrap());
        }
        s.hotspots = h;
    }
    picks.sort_by(f64::total_cmp);
    occlusion.sort_by(f64::total_cmp);
    eprintln!(
        "D11 pick median/max {:.4}/{:.4} ms; 20 hotspots {:.4}/{:.4} ms",
        picks[12], picks[24], occlusion[12], occlusion[24]
    );
    assert!(picks[24] <= 2., "pick budget: {} ms", picks[24]);
    assert!(
        occlusion[24] <= 4.,
        "occlusion budget: {} ms",
        occlusion[24]
    );
}

mod brute {
    use crate::{
        bvh::Ray,
        styles::{Effective, Occlusion},
    };
    use exact_model_gltf::{math::*, scene::Scene};

    use crate::pick::triangle;

    fn hits(
        scene: &Scene,
        effective: &[Effective],
        ray: Ray,
        occluders: bool,
        mut visit: impl FnMut(f32, usize, bool),
    ) {
        let Ray {
            origin,
            direction,
            depth,
            clip,
        } = ray;
        for (i, p) in scene.primitives.iter().enumerate() {
            let style = effective[i];
            if !style.visible || (style.occlusion == Occlusion::Occluder) != occluders {
                continue;
            }
            for tri in scene.indices[p.indices.start as usize..p.indices.end as usize]
                .as_chunks::<3>()
                .0
            {
                let [a, b, c] = std::array::from_fn(|k| scene.vertices[tri[k] as usize].p);
                if let Some(t) = triangle(origin, direction, a, b, c) {
                    if (depth[0]..=depth[1]).contains(&t)
                        && clip.keeps(add(origin, mul(direction, t)))
                    {
                        let front = dot(cross(sub(b, a), sub(c, a)), direction) < 0.;
                        visit(t, i, front);
                    }
                }
            }
        }
    }

    pub fn pick(
        scene: &Scene,
        effective: &[Effective],
        clip: crate::pick::Clip,
        origin: V3,
        direction: V3,
        depth: [f32; 2],
    ) -> String {
        let nearer = |hit: &mut Option<(f32, usize)>, t, i| {
            if hit.is_none_or(|(nearest, _)| t < nearest) {
                *hit = Some((t, i));
            }
        };
        let mut occluder = None;
        hits(
            scene,
            effective,
            Ray {
                clip,
                origin,
                direction,
                depth,
            },
            true,
            |t, i, _| {
                nearer(&mut occluder, t, i);
            },
        );
        let occluder_depth = occluder.map_or(f32::INFINITY, |(t, _)| t);
        let (mut opaque, mut revealed, mut translucent) = (None, None, None);
        hits(
            scene,
            effective,
            Ray {
                clip,
                origin,
                direction,
                depth,
            },
            false,
            |t, i, front| {
                let style = effective[i];
                if style.opaque() && t < occluder_depth {
                    nearer(&mut opaque, t, i);
                }
                // A primitive can cross the occluder: its nearer hit must not hide its faded hit.
                if style.occlusion == Occlusion::Revealed
                    && t > occluder_depth
                    && style.occluded_opacity > 0.
                {
                    nearer(&mut revealed, t, i);
                }
                if !style.opaque() && front {
                    nearer(&mut translucent, t, i);
                }
            },
        );
        opaque
            .or(revealed)
            .or(occluder)
            .or(translucent)
            .map(|(_, i)| scene.nodes[scene.primitives[i].node].name.clone())
            .unwrap_or_default()
    }
}
