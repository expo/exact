use super::*;

#[test]
fn hotspot_records_validate_their_fields_before_accepting_a_bind() {
    for (record, field) in [
        (r#"[[[3]]]"#, "child"),
        (r#"[[["label",false]]]"#, "node"),
        (r#"[[["label","Impeller","auto 0m 0m"]]]"#, "position"),
        (r#"[[["label","Impeller","0m 0m"]]]"#, "position"),
        (r#"[[["label","Impeller","","0 0 0"]]]"#, "normal"),
        (r#"[[["label","Impeller","","NaN 0 1"]]]"#, "normal"),
    ] {
        let mut s = viewer();
        let old = s.arguments.clone();
        let mut a = args();
        a[9] = values(record);
        let refusal = s.bind(&a, None).expect_err(field);
        assert!(refusal.0.contains(&format!("hotspots[0].{field}")));
        assert_eq!(s.arguments, old);
    }
}

#[test]
fn hotspots_place_host_composited_children_and_leave_other_children_alone() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut s = viewer();
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    a[2] = text("0deg 90deg 105%");
    a[3] = text("0m 0m 0m");
    a[9] = values(r#"[[["inlet","Assembly","0m 0m 0m"],["plate","Plate"]]]"#);
    s.bind(&a, Some(0.)).unwrap();
    s.child(0, "loading", None, [0., 0., 400., 20.]);
    s.child(1, "inlet", None, [7., 11., 60., 24.]);
    s.child(2, "plate", None, [3., 5., 40., 20.]);
    fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert_eq!(s.children_mode(), exact_gpu::ChildrenMode::Each);
    assert!(s.placement(0).is_none());
    let inlet = s.placement(1).expect("a hotspot must place its child");
    assert!(!inlet.hidden);
    assert_eq!(inlet.homography, [1., 0., 170., 0., 1., 188., 0., 0., 1.]);
    assert_eq!(
        s.placement_opacity(1),
        1.,
        "the labelled subtree does not occlude itself"
    );
    assert!(s.placement(2).unwrap().depth > inlet.depth);
    // Hosts can retire the loader and read new child slots before another frame.
    s.child(0, "inlet", None, [7., 11., 60., 24.]);
    s.child(1, "plate", None, [3., 5., 40., 20.]);
    s.child(2, "", None, [0.; 4]);
    assert_eq!(s.placement(0).unwrap().homography, inlet.homography);
    assert!(s.placement(1).unwrap().depth > inlet.depth);
}

fn anchored(records: &str) -> (ModelViewer, Vec<Value>) {
    let mut s = viewer();
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    a[2] = text("0deg 90deg 4m");
    a[3] = text("0m 0m 0m");
    a[4] = text("90deg");
    a[7] = Value::Number(0.);
    a[9] = values(records);
    s.bind(&a, Some(0.)).unwrap();
    s.child(0, "label", None, [3., 7., 60., 24.]);
    (s, a)
}
fn update(s: &mut ModelViewer, now: f64) {
    let mut h = std::mem::take(&mut s.hotspots);
    h.update(s.model(), &s.arguments, &frame(now));
    s.hotspots = h;
}

#[test]
fn a_pending_source_does_not_retest_the_unchanged_displayed_model() {
    let (mut s, mut a) = anchored(r#"[[["label","Impeller","0m 0m 0m"]]]"#);
    s.set_performance_reporting(true);
    update(&mut s, 0.);
    assert_eq!(s.hotspots.occlusion_updates, 1);
    let opacity = s.placement_opacity(0);
    a[0] = text("assets/replacement.glb");
    s.bind(&a, None).unwrap();
    update(&mut s, 0.);
    assert_eq!(s.hotspots.occlusion_updates, 0);
    assert_eq!(s.placement_opacity(0), opacity);
    s.asset("replacement.glb", Ok(MESHOPT));
    update(&mut s, 0.);
    assert_eq!(s.hotspots.occlusion_updates, 1);
}

#[test]
fn hotspot_anchors_use_the_first_active_node_with_the_requested_name() {
    let (mut s, mut a) = anchored(r#"[[["label","Bolt"]]]"#);
    let (mut json, bin) = gltf_support::triangle();
    json["nodes"] = serde_json::json!([
        {"name":"Bolt"},
        {"name":"Bolt", "mesh":0},
        {"name":"Bolt", "mesh":0, "translation":[2,0,0]}
    ]);
    json["scenes"] = serde_json::json!([{"nodes":[1,2]}]);
    s.asset("assembly.glb", Ok(&gltf_support::glb(&json, &bin)));
    for (position, centre) in [("", [225., 175.]), ("0m 0m 0m", [200., 200.])] {
        a[9] = values(&format!(r#"[[["label","Bolt","{position}"]]]"#));
        s.bind(&a, None).unwrap();
        update(&mut s, 0.);
        let p = s.placement(0).unwrap();
        assert!(
            !p.hidden,
            "an inactive duplicate must not hide the active anchor"
        );
        assert_eq!([p.homography[2] + 30., p.homography[5] + 12.], centre);
    }
}

#[test]
fn hotspot_hidden_rules_and_empty_nodes_follow_visibility_and_clip() {
    for (node, position, hidden) in [
        ("Assembly", "0m 0m 0m", false),
        ("Impeller", "", false),
        ("Inlet", "", false),
        ("Missing", "0m 0m 0m", true),
        ("Assembly", "0m 0m 100m", true),
        ("Assembly", "100m 0m 0m", true),
    ] {
        let (mut s, mut a) = anchored(&format!(r#"[[["label","{node}","{position}"]]]"#));
        update(&mut s, 0.);
        assert_eq!(s.placement(0).unwrap().hidden, hidden, "{node}: {position}");
        a[8] = values(&format!(r#"[[["{node}","",0,"hidden"]]]"#));
        s.bind(&a, None).unwrap();
        for now in [1., 2.] {
            update(&mut s, now);
            assert!(s.placement(0).unwrap().hidden);
        }
    }
    let (mut s, mut a) = anchored(r#"[[["label","Assembly","0m 0m -0.1m"]]]"#);
    a[11] = text("0 0 1");
    s.bind(&a, None).unwrap();
    update(&mut s, 0.);
    assert!(s.placement(0).unwrap().hidden);
    a[11] = text("0 0 -1");
    s.bind(&a, None).unwrap();
    update(&mut s, 1.);
    assert!(!s.placement(0).unwrap().hidden);
    s.asset("assembly.glb", Err(AssetError::Missing));
    update(&mut s, 2.);
    assert!(s.placement(0).unwrap().hidden);
}

#[test]
fn hotspot_occlusion_excludes_its_subtree_and_uses_every_opaque_visible_blocker() {
    for (node, position, normal, opacity) in [
        ("Assembly", "0m 0m 0m", "", 1.),
        ("Housing", "0.9m 0.5m 0m", "", 1.),
        ("Assembly", "0m 0m 0m", "0 0 -1", 0.25),
        ("Impeller", "0.4m 0m 0m", "", 0.25),
    ] {
        let (mut s, _) = anchored(&format!(
            r#"[[["label","{node}","{position}","{normal}"]]]"#
        ));
        update(&mut s, 0.);
        assert!(!s.placement(0).unwrap().hidden);
        assert_eq!(s.placement_opacity(0), opacity, "{node} {normal}");
    }
    let (mut s, mut a) = anchored(r#"[[["label","Impeller","-0.35m 0m 0m"]]]"#);
    a[8] = values(r#"[[["Housing","",0.2],["Shaft","",0]]]"#);
    s.bind(&a, None).unwrap();
    update(&mut s, 0.);
    assert_eq!(s.placement_opacity(0), 0.25, "the ordinary plate blocks");
    a[8] = values(r#"[[["Housing","",0.2],["Shaft","",0],["Plate","",0.5]]]"#);
    s.bind(&a, None).unwrap();
    update(&mut s, 1.);
    assert_eq!(
        s.placement_opacity(0),
        1.,
        "translucent geometry does not block"
    );
    a[8] = values(r#"[[["Housing","",0.2],["Shaft","",0],["Plate","",0,"visible","occluder"]]]"#);
    s.bind(&a, None).unwrap();
    update(&mut s, 2.);
    assert_eq!(s.placement_opacity(0), 0.25, "occluders ignore opacity");
    a[11] = text("0 0 -1");
    a[10] = text("0m 0m 0.75m");
    s.bind(&a, None).unwrap();
    update(&mut s, 3.);
    assert_eq!(s.placement_opacity(0), 1., "cut geometry does not block");
    a[11] = text("");
    a[12] = Value::Number(0.);
    s.bind(&a, None).unwrap();
    update(&mut s, 4.);
    assert_eq!(s.placement_opacity(0), 0.);
    assert!(
        !s.placement(0).unwrap().hidden,
        "a fully dimmed label stays accessible"
    );
}

#[test]
fn hotspot_placements_track_the_camera_round_to_device_pixels_and_retire_children() {
    let (mut s, mut a) = anchored(r#"[[["label","Assembly","1m 0m 0m"]]]"#);
    s.child(0, "label", None, [30., 40., 59.5, 23.5]);
    let mut f = frame(0.);
    f.scale = 2.;
    let mut h = std::mem::take(&mut s.hotspots);
    h.update(s.model(), &s.arguments, &f);
    s.hotspots = h;
    let p = s.placement(0).unwrap();
    assert_eq!(p.homography, [1., 0., 220.5, 0., 1., 188.5, 0., 0., 1.]);
    a[2] = text("90deg 90deg 4m");
    s.bind(&a, None).unwrap();
    update(&mut s, 1.);
    let p = s.placement(0).unwrap();
    assert!((p.homography[2] + 59.5 * 0.5 - 200.).abs() <= 0.5);
    assert!(p.depth > -4.);
    a[9] = values("[[]]");
    s.bind(&a, None).unwrap();
    update(&mut s, 2.);
    assert!(s.placement(0).is_none());
    assert_eq!(s.placement_opacity(0), 1.);
    s.child(0, "", None, [0.; 4]);
    assert!(s.hotspots.children.is_empty());
}

#[test]
fn hotspot_occlusion_is_throttled_in_motion_and_refreshed_at_rest_and_after_reload() {
    let (mut s, _) = anchored(r#"[[["label","Assembly","1m 0m 0m","0 0 1"]]]"#);
    s.set_performance_reporting(true);
    update(&mut s, 0.);
    let first_ms = s.hotspots.occlusion_ms.expect("occlusion was measured");
    assert_eq!(s.hotspots.occlusion_updates, 1);
    assert_eq!(s.hotspots.occlusion_total_ms, first_ms);
    assert_eq!(s.placement_opacity(0), 1.);
    let x = s.placement(0).unwrap().homography[2];
    let m = s.model_mut().unwrap();
    m.camera.pose.theta = std::f32::consts::PI;
    m.camera.goal.theta = std::f32::consts::PI + 0.1;
    update(&mut s, 50.);
    assert_eq!(s.hotspots.occlusion_ms, Some(first_ms));
    assert_eq!(s.hotspots.occlusion_updates, 1);
    assert_ne!(
        s.placement(0).unwrap().homography[2],
        x,
        "positions follow every frame"
    );
    assert_eq!(s.placement_opacity(0), 1.);
    update(&mut s, 100.);
    assert_eq!(s.placement_opacity(0), 0.25);
    let m = s.model_mut().unwrap();
    m.camera.pose.theta = 0.;
    m.camera.goal = m.camera.pose;
    update(&mut s, 101.);
    assert_eq!(s.placement_opacity(0), 1.);
    s.asset("assembly.glb", Ok(MESHOPT));
    assert!(
        s.hotspots.dirty,
        "a replacement invalidates occlusion even with a kept camera"
    );
}

fn label_texture(gpu: &exact_gpu::Gpu, rgba: [u8; 4]) -> wgpu::TextureView {
    let size = wgpu::Extent3d {
        width: 2,
        height: 2,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hotspot fixture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        texture.as_image_copy(),
        &rgba.repeat(4),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(8),
            rows_per_image: None,
        },
        size,
    );
    texture.create_view(&Default::default())
}

#[test]
fn captured_hotspots_blend_in_encoded_srgb_over_opaque_geometry() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let (mut s, mut a) = anchored(r#"[[["label","Assembly","0m 0m 0m"]]]"#);
    for samples in [1, 4] {
        s.force_samples = Some(samples);
        for color in ["#000000", "#204060"] {
            a[8] = values(&format!(r#"[[["Assembly","{color}"]]]"#));
            s.bind(&a, None).unwrap();
            s.child(0, "label", None, [0., 0., 20., 20.]);
            let background = fixture::render(&gpu, &mut s, &frame(0.))
                .unwrap()
                .0
                .at(200, 200);
            assert_eq!(background[3], 255, "the label must overlap opaque geometry");
            for rgba in [[255; 4], [96, 32, 16, 128]] {
                let texture = label_texture(&gpu, rgba);
                s.child(0, "label", Some(&texture), [0., 0., 20., 20.]);
                for (normal, opacity) in [("", 1.), ("0 0 -1", 0.25)] {
                    a[9] = values(&format!(
                        r#"[[["label","Assembly","0m 0m 0m","{normal}"]]]"#
                    ));
                    s.bind(&a, None).unwrap();
                    let expected: [u8; 4] = std::array::from_fn(|k| {
                        (f32::from(rgba[k]) * opacity
                            + f32::from(background[k]) * (1. - f32::from(rgba[3]) / 255. * opacity))
                            .round() as u8
                    });
                    for pixels in [
                        fixture::render(&gpu, &mut s, &frame(0.)).unwrap().0,
                        render_srgb(&gpu, &mut s),
                    ] {
                        let actual = pixels.at(200, 200);
                        for k in 0..4 {
                            assert!(
                                actual[k].abs_diff(expected[k]) <= 2,
                                "{actual:?} != {expected:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn hotspot_captured_textures_draw_dim_blend_and_overlap_in_placement_order() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let (mut s, mut a) = anchored(r#"[[["label","Assembly","1.5m 1.5m 0m"]]]"#);
    let green = label_texture(&gpu, [0, 128, 0, 128]);
    let blue = label_texture(&gpu, [0, 0, 255, 255]);
    s.child(0, "label", Some(&green), [0., 0., 20., 20.]);
    for samples in [1, 4] {
        s.force_samples = Some(samples);
        for dim in [false, true] {
            a[9] = values(&format!(
                r#"[[["label","Assembly","1.5m 1.5m 0m","{}"]]]"#,
                if dim { "0 0 -1" } else { "" }
            ));
            s.bind(&a, None).unwrap();
            let expected = if dim {
                [0, 32, 0, 32]
            } else {
                [0, 128, 0, 128]
            };
            for pixels in [
                fixture::render(&gpu, &mut s, &frame(0.)).unwrap().0,
                render_srgb(&gpu, &mut s),
            ] {
                let actual = pixels.at(275, 125);
                for k in 0..4 {
                    assert!(
                        actual[k].abs_diff(expected[k]) <= 2,
                        "{actual:?} != {expected:?}"
                    );
                }
            }
        }
    }
    a[9] =
        values(r#"[[["near","Assembly","1.125m 1.125m 1m"],["far","Assembly","1.5m 1.5m 0m"]]]"#);
    s.bind(&a, None).unwrap();
    s.child(0, "near", Some(&blue), [0., 0., 20., 20.]);
    s.child(1, "far", Some(&green), [0., 0., 20., 20.]);
    let pixels = fixture::render(&gpu, &mut s, &frame(0.)).unwrap().0;
    assert_eq!(pixels.at(275, 125), [0, 0, 255, 255]);
    assert!(s.placement(0).unwrap().depth > s.placement(1).unwrap().depth);
    a[9] = values(r#"[[["near","Assembly","100m 0m 0m"]]]"#);
    s.bind(&a, None).unwrap();
    let pixels = fixture::render(&gpu, &mut s, &frame(0.)).unwrap().0;
    assert_eq!(
        pixels.at(275, 125),
        [0; 4],
        "hidden and unplaced children are not drawn by the surface"
    );
    s.device_lost();
    s.child(0, "near", Some(&blue), [0., 0., 20., 20.]);
    a[9] = values(r#"[[["near","Assembly","1.5m 1.5m 0m"]]]"#);
    s.bind(&a, None).unwrap();
    assert_eq!(
        fixture::render(&gpu, &mut s, &frame(0.))
            .unwrap()
            .0
            .at(275, 125),
        [0, 0, 255, 255]
    );
}
