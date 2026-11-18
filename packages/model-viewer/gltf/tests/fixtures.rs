use exact_model_gltf::{
    framing::Framing,
    glb::parse,
    scene::{AlphaMode, Scene},
};

const PLAIN: &[u8] = include_bytes!("../../test/fixture/assets/assembly.glb");
const OPTIMIZED: &[u8] = include_bytes!("../../test/fixture/assets/assembly-meshopt.glb");

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

fn triangles(scene: &Scene, primitive: usize) -> Vec<[[f32; 3]; 3]> {
    let range = scene.primitives[primitive].indices.clone();
    let mut triangles: Vec<_> = scene.indices[range.start as usize..range.end as usize]
        .as_chunks::<3>()
        .0
        .iter()
        .map(|indices| {
            let mut points = std::array::from_fn(|i| scene.vertices[indices[i] as usize].p);
            points.sort_by(|a, b| a.partial_cmp(b).unwrap());
            points
        })
        .collect();
    triangles.sort_by(|a, b| a.partial_cmp(b).unwrap());
    triangles
}

#[test]
fn optimized_assembly_preserves_geometry_hierarchy_and_material_ranges() {
    let plain = parse(PLAIN).unwrap();
    let optimized = parse(OPTIMIZED).unwrap();
    for scene in [&plain, &optimized] {
        assert_eq!(scene.triangles(), 108);
        assert_eq!(scene.nodes.len(), 10);
        assert_eq!(scene.primitives.len(), 9);
        assert_eq!(scene.nodes[0].name, "Assembly");
        assert_eq!(scene.nodes[4].name, "#4");
        assert!(scene.nodes[0].parent.is_none());
        assert!(scene.nodes.iter().skip(1).all(|n| n.parent == Some(0)));
        assert_eq!(scene.nodes[0].children, (1..10).collect::<Vec<_>>());
        assert_eq!(scene.nodes[0].subtree_bounds, scene.bounds);
        assert_eq!(scene.nodes[8].primitives.len(), 2);
        assert_eq!(scene.nodes[8].indices.len(), 72);
        assert!(scene.nodes[9].bounds.is_none());
        let mut at = 0;
        for (i, p) in scene.primitives.iter().enumerate() {
            assert_eq!(p.indices, at..at + 36);
            at += 36;
            for &index in &scene.indices[p.indices.start as usize..p.indices.end as usize] {
                let v = scene.vertices[index as usize];
                assert_eq!(v.primitive as usize, i);
                assert_eq!(v.node as usize, p.node);
                close(v.n.iter().map(|x| x * x).sum::<f32>(), 1.0);
            }
        }
        let housing = scene.primitives[0].material;
        assert_eq!(housing.alpha_mode, AlphaMode::Opaque);
        assert_eq!(housing.color, [0.4, 0.5, 0.6, 0.2]);
        let start = scene.nodes[8].primitives.start;
        assert_eq!(
            scene.primitives[start].material.alpha_mode,
            AlphaMode::Blend
        );
        close(scene.primitives[start].material.color[3], 0.35);
        assert_eq!(
            scene.primitives[start + 1].material.alpha_mode,
            AlphaMode::Mask
        );
        close(scene.primitives[start + 1].material.alpha_cutoff, 0.6);
        close(scene.primitives[start + 1].material.color[3], 0.4);
        let a = scene.nodes[6].bounds.unwrap().centre();
        let b = scene.nodes[7].bounds.unwrap().centre();
        for (x, y) in a.into_iter().zip([-0.8, 0.8, 0.8]) {
            close(x, y);
        }
        for (x, y) in b.into_iter().zip([0.8, 0.8, 0.8]) {
            close(x, y);
        }
    }
    for i in 0..plain.primitives.len() {
        assert_eq!(
            plain.primitives[i].material,
            optimized.primitives[i].material
        );
        let a = triangles(&plain, i);
        let mut b = triangles(&optimized, i);
        assert_eq!(a.len(), b.len());
        // Optimization may reorder equal-coordinate vertices and triangles.
        for triangle in a {
            let matched = b
                .iter()
                .position(|other| {
                    triangle
                        .iter()
                        .flatten()
                        .zip(other.iter().flatten())
                        .all(|(a, b)| (a - b).abs() < 1e-4)
                })
                .unwrap_or_else(|| panic!("primitive {i}: {triangle:?}"));
            b.swap_remove(matched);
        }
        assert!(b.is_empty());
    }
}

#[test]
fn framing_matches_hand_computed_values_and_canvas_aspect() {
    let mut scene = parse(PLAIN).unwrap();
    scene.vertices.truncate(3);
    scene.indices = vec![0, 1, 2];
    for (v, p) in scene
        .vertices
        .iter_mut()
        .zip([[3., 0., 0.], [0., 4., 0.], [0., 0., -3.]])
    {
        v.p = p;
    }
    let framing = Framing::new(&scene, [0.; 3], std::f32::consts::FRAC_PI_3);
    close(framing.radius, 4.);
    close(framing.distance, 8.);
    let ideal_aspect = 3. / 8. * 3f32.sqrt();
    close(framing.ideal_aspect, ideal_aspect);
    close(
        framing.vertical_field_of_view(2.),
        std::f32::consts::FRAC_PI_3,
    );
    close(framing.vertical_field_of_view(0.5), 2. * 0.75f32.atan());
    let shifted = Framing::new(&scene, [3., 4., 0.], std::f32::consts::FRAC_PI_3);
    close(shifted.radius, 34f32.sqrt());
    close(shifted.distance, 2. * 34f32.sqrt());
    scene.vertices.clear();
    scene.indices.clear();
    let framing = Framing::new(&scene, [0.; 3], 30f32.to_radians());
    assert!(framing.distance.is_finite() && framing.distance > 0.);
    assert!(framing.vertical_field_of_view(1.).is_finite());
}
