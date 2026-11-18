mod support;
use exact_model_gltf::{
    framing::Framing,
    glb::{parse, parse_with_limit},
    math::{dot, norm, sub},
};
use serde_json::json;
use support::{glb, triangle};

fn close(a: [f32; 3], b: [f32; 3]) {
    assert!(
        a.into_iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-5),
        "{a:?} != {b:?}"
    );
}

#[test]
fn bounds_and_framing_ignore_unreferenced_positions_with_or_without_normals() {
    let (mut json, mut bin) = triangle();
    bin.extend([100f32, 0., 0.].into_iter().flat_map(f32::to_le_bytes));
    for _ in 0..4 {
        bin.extend([0f32, 0., 1.].into_iter().flat_map(f32::to_le_bytes));
    }
    bin.extend([0, 1, 2]);
    json["buffers"][0]["byteLength"] = json!(bin.len());
    json["bufferViews"] = json!([
        {"buffer":0,"byteLength":48},
        {"buffer":0,"byteOffset":48,"byteLength":48},
        {"buffer":0,"byteOffset":96,"byteLength":3}
    ]);
    json["accessors"] = json!([
        {"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"},
        {"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"},
        {"bufferView":2,"componentType":5121,"count":3,"type":"SCALAR"}
    ]);
    json["meshes"][0]["primitives"][0]["indices"] = json!(2);
    json["nodes"] = json!([{"mesh":0}, {"children":[0]}]);
    json["scenes"][0]["nodes"] = json!([1]);
    for with_normals in [false, true] {
        if with_normals {
            json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(1);
        }
        let scene = parse(&glb(&json, &bin)).unwrap();
        let bounds = scene.nodes[0].bounds.unwrap();
        close(bounds.min, [0.; 3]);
        close(bounds.max, [1., 1., 0.]);
        close(bounds.centre(), [0.5, 0.5, 0.]);
        assert_eq!(scene.nodes[0].subtree_bounds, Some(bounds));
        assert_eq!(scene.nodes[1].subtree_bounds, Some(bounds));
        assert_eq!(scene.bounds, Some(bounds));
        close(scene.centre, [0.5, 0.5, 0.]);
        for (target, expected) in [
            (scene.centre, [std::f32::consts::FRAC_1_SQRT_2, 1., 1.]),
            ([0.; 3], [1., 2f32.sqrt(), std::f32::consts::FRAC_1_SQRT_2]),
        ] {
            let framing = Framing::new(&scene, target, std::f32::consts::FRAC_PI_2);
            close(
                [framing.radius, framing.distance, framing.ideal_aspect],
                expected,
            );
        }
    }
}

#[test]
fn missing_normals_are_flat_for_indexed_and_unindexed_triangle_topologies() {
    let (json, bin) = triangle();
    for vertex in parse(&glb(&json, &bin)).unwrap().vertices {
        close(vertex.n, [0., 0., 1.]);
    }
    let points = [[0f32, 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let diagonal = -1. / 3f32.sqrt();
    for (mode, source, expanded, second_normal) in [
        (4, vec![0, 1, 2, 0, 2, 3], [0, 1, 2, 0, 2, 3], [1., 0., 0.]),
        (5, vec![0, 1, 2, 3], [0, 1, 2, 2, 1, 3], [diagonal; 3]),
        (6, vec![0, 1, 2, 3], [0, 1, 2, 0, 2, 3], [1., 0., 0.]),
    ] {
        for indexed in [false, true] {
            let (mut json, _) = triangle();
            let positions = if indexed {
                points.to_vec()
            } else {
                source.iter().map(|&i| points[i]).collect()
            };
            let mut bin: Vec<_> = positions
                .iter()
                .flatten()
                .flat_map(|p| p.to_le_bytes())
                .collect();
            let length = bin.len();
            json["bufferViews"][0]["byteLength"] = json!(length);
            json["accessors"][0]["count"] = json!(positions.len());
            json["meshes"][0]["primitives"][0]["mode"] = json!(mode);
            if indexed {
                bin.extend(source.iter().map(|&i| i as u8));
                json["bufferViews"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"buffer":0,"byteOffset":length,"byteLength":source.len()}));
                json["accessors"].as_array_mut().unwrap().push(
                    json!({"bufferView":1,"componentType":5121,"count":source.len(),"type":"SCALAR"}),
                );
                json["meshes"][0]["primitives"][0]["indices"] = json!(1);
            }
            json["buffers"][0]["byteLength"] = json!(bin.len());
            let file = glb(&json, &bin);
            let scene = parse_with_limit(&file, 192).unwrap();
            assert_eq!(scene.triangles(), 2);
            assert_eq!(scene.primitives[0].indices, 0..6);
            for (i, &index) in scene.indices.iter().enumerate() {
                let vertex = scene.vertices[index as usize];
                close(vertex.p, points[expanded[i]]);
                close(vertex.n, if i < 3 { [0., 0., 1.] } else { second_normal });
                assert_eq!(vertex.node, 0);
                assert_eq!(vertex.primitive, 0);
            }
            assert!(parse_with_limit(&file, 191)
                .unwrap_err()
                .contains("max_buffer_size"));
        }
    }
}

#[test]
fn small_quantized_models_keep_unit_normals() {
    let (mut json, _) = triangle();
    let mut bin: Vec<_> = [0i16, 0, 0, 0, 30000, 0, 0, 0, 0, 30000, 0, 0]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    for _ in 0..3 {
        bin.extend([0f32, 0., 1.].into_iter().flat_map(f32::to_le_bytes));
    }
    json["extensionsRequired"] = json!(["KHR_mesh_quantization"]);
    json["buffers"][0]["byteLength"] = json!(bin.len());
    json["bufferViews"] = json!([
        {"buffer":0,"byteLength":24,"byteStride":8},
        {"buffer":0,"byteOffset":24,"byteLength":36}
    ]);
    json["accessors"][0]["componentType"] = json!(5122);
    json["accessors"]
        .as_array_mut()
        .unwrap()
        .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}));
    json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(1);
    json["nodes"][0]["scale"] = json!([1e-7, 1e-7, 1e-7]);
    let scene = parse(&glb(&json, &bin)).unwrap();
    close(scene.vertices[1].p, [0.003, 0., 0.]);
    for vertex in scene.vertices {
        close(vertex.n, [0., 0., 1.]);
    }
    for magnitude in [f32::from_bits(1), 1e-30, 1e30, f32::MAX] {
        close(
            norm([magnitude, 0., -magnitude]),
            [
                std::f32::consts::FRAC_1_SQRT_2,
                0.,
                -std::f32::consts::FRAC_1_SQRT_2,
            ],
        );
    }
    close(norm([0.; 3]), [0.; 3]);
}

#[test]
fn integer_positions_preserve_stride_offset_and_normalization() {
    for (component, normalized, minimum, maximum, expected) in [
        (5120, false, -128i32, 127i32, [-128., 127., 1.]),
        (5120, true, -128, 127, [-1., 1., 1. / 127.]),
        (5121, false, 0, 255, [0., 255., 1.]),
        (5121, true, 0, 255, [0., 1., 1. / 255.]),
        (5122, false, -32768, 32767, [-32768., 32767., 1.]),
        (5122, true, -32768, 32767, [-1., 1., 1. / 32767.]),
        (5123, false, 0, 65535, [0., 65535., 1.]),
        (5123, true, 0, 65535, [0., 1., 1. / 65535.]),
    ] {
        let size = if component <= 5121 { 1 } else { 2 };
        let mut bin = vec![0u8; 28];
        for i in 0..3 {
            for (c, value) in [minimum, maximum, 1].into_iter().enumerate() {
                bin[4 + i * 8 + c * size..4 + i * 8 + (c + 1) * size]
                    .copy_from_slice(&value.to_le_bytes()[..size]);
            }
        }
        let (mut json, _) = triangle();
        json["extensionsUsed"] = json!(["KHR_mesh_quantization"]);
        json["buffers"][0]["byteLength"] = json!(bin.len());
        json["bufferViews"][0] = json!({"buffer":0,"byteLength":bin.len(),"byteStride":8});
        json["accessors"][0] = json!({"bufferView":0,"byteOffset":4,"componentType":component,"normalized":normalized,"type":"VEC3","count":3});
        let scene = parse(&glb(&json, &bin)).unwrap();
        for vertex in scene.vertices {
            close(vertex.p, expected);
        }
    }
}

#[test]
fn quantized_normals_use_the_inverse_transpose_and_keep_mirrored_sides() {
    for (component, maximum) in [(5120, 127i32), (5122, 32767i32)] {
        let size = if component == 5120 { 1 } else { 2 };
        let (mut json, mut bin) = triangle();
        // Three identical diagonal normals, padded to eight bytes each.
        for _ in 0..3 {
            for value in [maximum, maximum, 0] {
                bin.extend(&value.to_le_bytes()[..size]);
            }
            bin.resize(bin.len() + 8 - 3 * size, 0);
        }
        json["buffers"][0]["byteLength"] = json!(bin.len());
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":36,"byteLength":24,"byteStride":8}));
        json["accessors"].as_array_mut().unwrap().push(json!({"bufferView":1,"componentType":component,"normalized":true,"count":3,"type":"VEC3"}));
        json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(1);
        json["nodes"][0]["scale"] = json!([-2, 1, 3]);
        let scene = parse(&glb(&json, &bin)).unwrap();
        let unit = 1.25f32.sqrt();
        for vertex in scene.vertices {
            close(vertex.n, [-0.5 / unit, 1. / unit, 0.]);
        }
    }
}

#[test]
fn all_index_widths_and_triangle_topologies_keep_their_winding() {
    for component in [5121, 5123, 5125] {
        let (mut json, mut bin) = triangle();
        let size = match component {
            5121 => 1,
            5123 => 2,
            _ => 4,
        };
        for i in [2u32, 0, 1] {
            bin.extend(&i.to_le_bytes()[..size]);
        }
        json["buffers"][0]["byteLength"] = json!(bin.len());
        json["bufferViews"]
            .as_array_mut()
            .unwrap()
            .push(json!({"buffer":0,"byteOffset":36,"byteLength":3*size}));
        json["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":1,"componentType":component,"count":3,"type":"SCALAR"}));
        json["meshes"][0]["primitives"][0]["indices"] = json!(1);
        let scene = parse(&glb(&json, &bin)).unwrap();
        let points: Vec<_> = scene
            .indices
            .iter()
            .map(|&i| scene.vertices[i as usize].p)
            .collect();
        assert_eq!(points, [[0., 1., 0.], [0., 0., 0.], [1., 0., 0.]]);
        bin[36..36 + size].copy_from_slice(&3u32.to_le_bytes()[..size]);
        assert!(parse(&glb(&json, &bin))
            .unwrap_err()
            .contains("accessor 1: index"));
    }
    for (mode, expected) in [(5, vec![0, 1, 2, 2, 1, 3]), (6, vec![0, 1, 2, 0, 2, 3])] {
        let (mut json, _) = triangle();
        let bin: Vec<_> = [0f32, 0., 0., 1., 0., 0., 0., 1., 0., 1., 1., 0.]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        json["buffers"][0]["byteLength"] = json!(bin.len());
        json["bufferViews"][0]["byteLength"] = json!(bin.len());
        json["accessors"][0]["count"] = json!(4);
        json["meshes"][0]["primitives"][0]["mode"] = json!(mode);
        let scene = parse(&glb(&json, &bin)).unwrap();
        let source = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 0.]];
        let points: Vec<_> = scene
            .indices
            .iter()
            .map(|&i| scene.vertices[i as usize].p)
            .collect();
        assert_eq!(
            points,
            expected.iter().map(|&i| source[i]).collect::<Vec<_>>()
        );
    }
}

#[test]
fn default_scene_hierarchy_composes_matrix_and_trs_in_file_order() {
    let (mut json, bin) = triangle();
    json["nodes"] = json!([
        {"name":"child", "mesh":0, "matrix":[1,0,0,0,0,1,0,0,0,0,1,0,2,0,0,1]},
        {"name":"root", "children":[0], "translation":[10,20,30], "scale":[2,3,4],
         "rotation":[0,0, std::f64::consts::FRAC_1_SQRT_2,std::f64::consts::FRAC_1_SQRT_2]},
        {"name":"outside", "mesh":0, "translation":[1000,0,0]}
    ]);
    json["scenes"] = json!([{"nodes":[2]}, {"nodes":[1]}]);
    json["scene"] = json!(1);
    let scene = parse(&glb(&json, &bin)).unwrap();
    assert_eq!(scene.nodes[0].parent, Some(1));
    assert_eq!(
        scene
            .nodes
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>(),
        ["child", "root", "outside"]
    );
    assert!(scene.nodes[0].active && scene.nodes[1].active && !scene.nodes[2].active);
    assert_eq!(scene.triangles(), 1);
    close(scene.vertices[0].p, [10., 24., 30.]);
    close(scene.vertices[1].p, [10., 26., 30.]);
    close(scene.vertices[2].p, [7., 24., 30.]);
    let edge = sub(scene.vertices[1].p, scene.vertices[0].p);
    close(edge, [0., 2., 0.]);
    assert!(dot(edge, scene.vertices[0].n).abs() < 1e-5);
    assert_eq!(scene.nodes[1].subtree_bounds, scene.nodes[0].bounds);
    // A shared name does not merge independent nodes or geometry.
    json["scenes"][1]["nodes"] = json!([1, 2]);
    json["nodes"][2]["name"] = json!("child");
    assert_eq!(parse(&glb(&json, &bin)).unwrap().triangles(), 2);
    for nodes in [
        json!([{"children":[1]}, {"children":[0]}]),
        json!([{"children":[2]}, {"children":[2]}, {}]),
        json!([{"children":[99]}]),
    ] {
        json["nodes"] = nodes;
        assert!(parse(&glb(&json, &bin)).unwrap_err().contains("node"));
    }
}

#[test]
fn world_reflections_keep_triangle_fronts_with_or_without_normals() {
    use exact_model_gltf::math::cross;
    for with_normals in [false, true] {
        for mode in [4, 5, 6] {
            for child_reflected in [false, true] {
                let (mut json, mut bin) = triangle();
                if with_normals {
                    bin.extend(
                        [0f32, 0., 1.]
                            .repeat(3)
                            .into_iter()
                            .flat_map(f32::to_le_bytes),
                    );
                    json["bufferViews"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"buffer":0,"byteOffset":36,"byteLength":36}));
                    json["accessors"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}));
                    json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = json!(1);
                }
                json["buffers"][0]["byteLength"] = json!(bin.len());
                json["meshes"][0]["primitives"][0]["mode"] = json!(mode);
                json["nodes"] = json!([
                    {"children":[1],"scale":[-1,1,1]},
                    {"mesh":0,"matrix":[if child_reflected {-1} else {1},0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}
                ]);
                let scene = parse(&glb(&json, &bin)).unwrap();
                let [a, b, c] = std::array::from_fn(|k| scene.vertices[scene.indices[k] as usize]);
                assert!(
                    dot(cross(sub(b.p, a.p), sub(c.p, a.p)), a.n) > 0.99,
                    "normals={with_normals}, mode={mode}, child reflection={child_reflected}"
                );
            }
        }
    }
}
