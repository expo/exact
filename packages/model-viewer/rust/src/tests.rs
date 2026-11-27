use super::*;
use exact_gpu::{fixture, PointerKind, PointerPhase};
use exact_model_gltf::glb;
use exact_model_gltf::{
    math::*,
    scene::{AlphaMode, Material, Primitive},
};
use std::sync::Arc;
use styles::{NodeStyle, Occlusion};

#[path = "rays_tests.rs"]
mod rays;

#[path = "hotspots_tests.rs"]
mod hotspots;

const GLB: &[u8] = include_bytes!("../../test/fixture/assets/assembly.glb");
const MESHOPT: &[u8] = include_bytes!("../../test/fixture/assets/assembly-meshopt.glb");
fn args() -> Vec<Value> {
    arguments::defaults()
}

#[test]
fn performance_reports_work_only_while_requested() {
    let mut s = viewer();
    assert!(s.take_performance(0.).is_none());
    assert!(s.agent(r#"{"op":"state"}"#).is_none());
    assert!(s.agent(r#"{"op":"state","performance":true}"#).is_none());
    s.set_performance_reporting(true);
    s.asset("assembly.glb", Ok(MESHOPT));
    message(&mut s, "pick 200 200", 0.);
    let snapshot: serde_json::Value =
        serde_json::from_str(&s.take_performance(0.).expect("model timings")).unwrap();
    for field in ["parseMs", "parseWaitMs", "prepareMs", "pickMs"] {
        assert!(
            snapshot[field].as_f64().is_some_and(|ms| ms >= 0.),
            "{field}"
        );
    }
    assert!(snapshot.get("uploadMs").is_none(), "nothing uploaded yet");
    for request in [
        r#"{"op":"state"}"#,
        r#"{"op":"state","performance":false}"#,
        r#"{"op":"state","look":true}"#,
        r#"{"op":"state","draws":true}"#,
        r#"{"op":"state","visible":true}"#,
    ] {
        assert!(s.agent(request).is_none(), "{request}");
    }
    let reply: serde_json::Value =
        serde_json::from_str(&s.agent(r#"{"op":"state","performance":true}"#).unwrap()).unwrap();
    assert_eq!(reply["performance"]["parseMs"], snapshot["parseMs"]);
    assert!(reply["performance"]["sinceArrivalMs"].is_number());
    s.set_performance_reporting(false);
    assert!(s.take_performance(1000.).is_none());
    assert!(s.agent(r#"{"op":"state","performance":true}"#).is_none());
    s.set_performance_reporting(true);
    let snapshot: serde_json::Value =
        serde_json::from_str(&s.take_performance(2000.).unwrap()).unwrap();
    assert_eq!(snapshot, serde_json::json!({"triangles": 108}));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_parse_waits_for_poll_and_reports_only_the_current_source() {
    let mut s = ModelViewer::default();
    s.set_performance_reporting(true);
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    s.bind(&a, None).unwrap();
    s.asset("assembly.glb", Ok(MESHOPT));
    assert!(s.preparing());
    assert!(s.model().is_none());
    assert!(!s.frame_ready());
    let result = s.receiver.recv().unwrap();
    assert!(
        result.scene.as_ref().unwrap().1.bytes() > 0,
        "the worker built the BVH"
    );
    assert!(result.parse_ms.is_some());
    a[0] = text("assets/other.glb");
    s.bind(&a, None).unwrap();
    s.finish(result);
    assert!(s.model().is_none());
    assert_eq!(s.take_performance(0.).as_deref(), Some("{}"));
    s.asset("other.glb", Ok(MESHOPT));
    let result = s.receiver.recv().unwrap();
    s.finish(result);
    assert!(!s.preparing());
    assert!(s.model().is_some());
    assert!(!s.frame_ready());
    let snapshot: serde_json::Value =
        serde_json::from_str(&s.take_performance(0.).unwrap()).unwrap();
    assert!(snapshot["parseMs"].is_number());
    assert!(snapshot["prepareMs"].is_number());

    s.asset("other.glb", Ok(MESHOPT));
    let result = s.receiver.recv().unwrap();
    assert!(result.parse_ms.is_some());
    s.set_performance_reporting(false);
    s.set_performance_reporting(true);
    s.finish(result);
    assert!(!s.preparing());
    assert!(s.model().is_some());
    let snapshot: serde_json::Value =
        serde_json::from_str(&s.take_performance(0.).unwrap()).unwrap();
    assert!(snapshot.get("parseMs").is_none(), "{snapshot}");
    assert!(snapshot.get("parseWaitMs").is_none(), "{snapshot}");
    assert!(snapshot["prepareMs"].is_number());

    s.asset("other.glb", Ok(MESHOPT));
    let result = s.receiver.recv().unwrap();
    s.set_performance_reporting(true);
    s.finish(result);
    let snapshot: serde_json::Value =
        serde_json::from_str(&s.take_performance(0.).unwrap()).unwrap();
    assert!(snapshot["parseMs"].is_number());
    assert!(snapshot["parseWaitMs"].is_number());
}
fn text(s: &str) -> Value {
    Value::str(s)
}
fn values(json: &str) -> Value {
    exact_gpu::json::parse_values(json).unwrap().remove(0)
}
fn viewer() -> ModelViewer {
    let mut s = ModelViewer::default();
    s.clock(true);
    s.size(400., 400.);
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    a[1] = Value::Bool(true);
    a[2] = text("0deg 90deg 105%");
    a[3] = text("0m 0m 0m");
    a[8] = values(
        r#"[[["Housing","",1,"visible","occluder"],["Impeller","",1,"visible","revealed"]]]"#,
    );
    s.bind(&a, Some(0.)).unwrap();
    s.assets();
    s.asset("assembly.glb", Ok(GLB));
    s
}
fn frame(now: f64) -> Frame {
    Frame {
        width: 400.,
        height: 400.,
        scale: 1.,
        now_ms: now,
        seekable: true,
        period_ms: 16.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    }
}
fn message(s: &mut ModelViewer, text: &str, at_ms: f64) {
    s.input(&InputEvent::Message {
        text: text.into(),
        at_ms,
    });
}
fn pointer(phase: PointerPhase, x: f32, y: f32, at_ms: f64) -> InputEvent {
    InputEvent::Pointer {
        id: 1,
        phase,
        x,
        y,
        dx: 0.,
        dy: 0.,
        kind: PointerKind::Mouse,
        buttons: match phase {
            PointerPhase::Down | PointerPhase::Move => 1,
            PointerPhase::Up | PointerPhase::Cancel => 0,
        },
        shift_key: false,
        ctrl_key: false,
        alt_key: false,
        meta_key: false,
        at_ms,
    }
}

#[test]
fn arguments_are_positional_and_refusals_name_the_field() {
    let default = Arguments::parse(&[]).unwrap();
    assert!(!default.controls);
    assert_eq!(default.decay, 50.);
    let mut a = args();
    a[2] = text("1rad 0deg 50cm");
    a[3] = text("auto 2mm -3cm");
    a[4] = text("45deg");
    let parsed = Arguments::parse(&a).unwrap();
    assert_eq!(parsed.orbit.radius, 0.5);
    assert_eq!(parsed.target, [None, Some(0.002), Some(-0.03)]);
    for (i, bad) in [
        (1, text("true")),
        (2, text("0deg 90deg 5px")),
        (2, text("0deg 90deg NaNm")),
        (3, text("auto 1m")),
        (4, text("0deg")),
        (5, text("no")),
        (6, text("yes")),
        (7, Value::Number(-1.)),
        (8, values(r#"[[["Housing","AccentColor"]]]"#)),
        (8, values(r#"[[["Housing","red",2]]]"#)),
        (9, text("none")),
        (10, text("0m 0m 0")),
        (11, text("0 0 0")),
        (12, Value::Number(2.)),
    ] {
        let mut a = args();
        a[i] = bad;
        assert!(
            Arguments::parse(&a)
                .unwrap_err()
                .0
                .contains(arguments::NAMES[i]),
            "{}",
            arguments::NAMES[i]
        );
    }
    let short = NodeStyle::parse_list(&values(r#"[[["Housing","red"]]]"#)).unwrap();
    assert_eq!(short[0].opacity, 1.);
    assert!(short[0].visible);
    assert_eq!(short[0].occluded_opacity, 0.3);
    let mut s = viewer();
    let old = s.arguments.clone();
    let mut bad = args();
    bad[11] = text("NaN 0 0");
    assert!(s.bind(&bad, None).is_err());
    assert_eq!(s.arguments, old);
}

#[test]
fn deeper_styles_win_whole_and_later_equal_names_win() {
    let scene = glb::parse(GLB).unwrap();
    let entries=NodeStyle::parse_list(&values(r##"[[["Impeller","red",0.6],["Assembly","blue",0.2],["Housing","green"],["Housing","#ffffff"]]]"##)).unwrap();
    let styles = styles::resolve(&scene, &entries);
    assert_eq!(styles[scene.find("Impeller").unwrap()].opacity, 0.6);
    let housing = &styles[scene.find("Housing").unwrap()];
    assert_eq!(housing.color, Some([1., 1., 1., 1.]));
    assert_eq!(housing.opacity, 1.);
    assert_eq!(styles[scene.find("#4").unwrap()].opacity, 0.2);
    let mut primitive = Primitive {
        node: 0,
        indices: 0..3,
        material: Material {
            color: [0.4, 0.5, 0.6, 0.2],
            alpha_mode: AlphaMode::Blend,
            alpha_cutoff: 0.5,
        },
    };
    let mut style = NodeStyle {
        occlusion: Occlusion::Occluder,
        opacity: 0.,
        ..Default::default()
    };
    assert!(Effective::new(&primitive, &style).visible);
    assert!(Effective::new(&primitive, &style).opaque());
    style.occlusion = Occlusion::None;
    assert!(!Effective::new(&primitive, &style).visible);
    style.opacity = 0.5;
    assert_eq!(Effective::new(&primitive, &style).color[3], 0.1);
    primitive.material.alpha_mode = AlphaMode::Mask;
    assert!(!Effective::new(&primitive, &style).visible);
    primitive.material.alpha_mode = AlphaMode::Opaque;
    assert_eq!(Effective::new(&primitive, &style).color[3], 0.5);
    style.visible = false;
    assert!(!Effective::new(&primitive, &style).visible);
}

#[test]
fn picking_follows_the_four_passes_visibility_and_clip() {
    for bytes in [GLB, MESHOPT] {
        let mut s = viewer();
        s.asset("assembly.glb", Ok(bytes));
        let m = s.model_mut().unwrap();
        let cast = |m: &Model, x: f32, y: f32| {
            pick::pick(
                &m.scene,
                &m.bvh,
                &m.effective,
                m.clip,
                [x, y, 5.],
                [0., 0., -1.],
                [0., f32::INFINITY],
            )
        };
        assert_eq!(cast(m, 0.4, 0.2), "Impeller");
        assert_eq!(cast(m, -0.4, 0.), "Plate");
        assert_eq!(cast(m, 3., 3.), "");
        assert_eq!(cast(m, 1.35, -0.35), "");
        let i = m.scene.find("Impeller").unwrap();
        for (pi, p) in m.scene.primitives.iter().enumerate() {
            if p.node == i {
                m.effective[pi].visible = false;
            }
        }
        assert_eq!(cast(m, 0.4, 0.2), "Housing");
        m.clip = Clip {
            position: [0., 0., 0.],
            normal: [1., 0., 0.],
        };
        assert_eq!(cast(m, -0.4, 0.), "");
        assert_eq!(cast(m, 0.4, 0.2), "Housing");
    }
    let mut s = viewer();
    message(&mut s, "pick 399 0", 0.);
    assert_eq!(s.messages(), [""]);
    let before = s.model().unwrap().camera.goal;
    s.arguments.controls = false;
    s.input(&pointer(PointerPhase::Down, 0., 0., 0.));
    s.input(&pointer(PointerPhase::Move, 50., 50., 20.));
    assert_eq!(s.model().unwrap().camera.goal, before);
}

#[test]
fn camera_decay_poles_fling_named_views_and_settled_records() {
    let mut s = viewer();
    let m = s.model_mut().unwrap();
    m.camera.step(&m.scene, 0.);
    let start = m.camera.pose;
    m.camera.orbit(0.5, 0.2);
    m.camera.step(&m.scene, 50.);
    assert!((m.camera.pose.theta - (start.theta - 0.5 * (1. - (-3f32).exp()))).abs() < 1e-6);
    m.camera.step(&m.scene, 500.);
    assert!(!m.camera.moving());
    assert_eq!(m.camera.pose, m.camera.goal);
    let top = camera::Pose { phi: 0., ..start };
    let basis = top.basis();
    assert!(dot(cross(basis.0, basis.1), basis.2) > 0.999);
    assert!(top.view().into_iter().flatten().all(f32::is_finite));
    m.camera.reset(&m.scene);
    m.camera.step(&m.scene, 1000.);
    let initial = m.camera.goal;
    s.published();
    s.input(&pointer(PointerPhase::Down, 100., 100., 1000.));
    s.input(&pointer(PointerPhase::Move, 140., 100., 1050.));
    let before = s.model().unwrap().camera.goal.theta;
    s.input(&pointer(PointerPhase::Up, 140., 100., 1050.));
    let m = s.model_mut().unwrap();
    assert!(
        m.camera.goal.theta < before,
        "release adds the last 100ms velocity"
    );
    m.camera.step(&m.scene, 1500.);
    assert!(!m.camera.moving());
    let record = s.published().unwrap();
    assert!(record.contains("cameraOrbit"));
    assert!(s.published().is_none());
    message(&mut s, "reset", 1500.);
    let m = s.model_mut().unwrap();
    m.camera.step(&m.scene, 2000.);
    assert_eq!(s.model().unwrap().camera.pose, initial);
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    a[2] = text("0deg 0deg 105%");
    a[7] = Value::Number(0.);
    s.bind(&a, Some(2000.)).unwrap();
    let m = s.model().unwrap();
    assert_eq!(m.camera.pose.phi, 0.);
    assert_eq!(m.camera.pose, m.camera.goal);
}

#[test]
fn lifecycle_retains_pending_drops_stale_clears_failures_and_reloads() {
    let mut s = viewer();
    assert!(!s.frame_ready());
    assert!(!s.loaded);
    let generation = s.generation();
    let camera = s.model().unwrap().camera.pose;
    let mut a = args();
    a[0] = text("assets/other.glb");
    s.bind(&a, None).unwrap();
    assert!(!s.frame_ready());
    assert!(s.preparing());
    assert!(s.model().is_some());
    assert!(s.generation() > generation);
    let stale = s.source.load().unwrap().sequence;
    a[0] = text("assets/last.glb");
    s.bind(&a, None).unwrap();
    s.finish(Parsed {
        timing_session: s.timing_session,
        parse_ms: None,
        sequence: stale,
        name: "other.glb".into(),
        scene: glb::parse(MESHOPT).map(|s| {
            let b = crate::bvh::Bvh::new(&s);
            (s, b)
        }),
    });
    assert!(s.preparing());
    assert_eq!(s.model().unwrap().camera.pose, camera);
    s.asset("last.glb", Ok(MESHOPT));
    assert!(!s.frame_ready());
    assert!(s.error.is_empty());
    a[0] = text("assets/missing.glb");
    s.bind(&a, None).unwrap();
    s.asset("missing.glb", Err(AssetError::Missing));
    assert!(!s.frame_ready());
    assert!(!s.loaded);
    assert!(s.model().is_none());
    assert!(s.error.contains("assets/missing.glb"));
    a[0] = text("assets/broken.glb");
    s.bind(&a, None).unwrap();
    s.asset("broken.glb", Ok(&GLB[..20]));
    assert!(s.error.contains("broken.glb"));
    assert!(!s.frame_ready());
    for src in [
        "https://example.org/model.glb",
        "assets/../model.glb",
        "assets/model.gltf",
    ] {
        a[0] = text(src);
        s.bind(&a, None).unwrap();
        assert!(s.error.contains(src));
        assert!(!s.frame_ready());
    }
    a[0] = text("");
    s.bind(&a, None).unwrap();
    assert!(s.error.is_empty());
    assert!(s.model().is_none());
    let mut s = viewer();
    s.model_mut().unwrap().camera.pose.theta = 1.;
    s.asset("assembly.glb", Ok(MESHOPT));
    assert_eq!(s.model().unwrap().camera.pose.theta, 1.);
    s.device_lost();
    assert!(s.model().is_some());
}

fn render_srgb(gpu: &exact_gpu::Gpu, s: &mut ModelViewer) -> fixture::Pixels {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 400,
            height: 400,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    assert!(!s.render(
        &frame(1000.),
        &gpu.device,
        &gpu.queue,
        &mut encoder,
        &view,
        format
    ));
    gpu.queue.submit([encoder.finish()]);
    fixture::read(gpu, &texture).unwrap()
}
#[test]
fn gpu_four_passes_materials_clip_transparency_and_one_sample() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut s = viewer();
    let pixels = render_srgb(&gpu, &mut s);
    pixels.save("assembly");
    assert!(pixels
        .data
        .chunks_exact(4)
        .all(|p| p[..3].iter().all(|v| *v <= p[3])));
    let (encoded, _) = fixture::render(&gpu, &mut s, &frame(1000.)).unwrap();
    assert!(pixels
        .data
        .iter()
        .zip(&encoded.data)
        .all(|(a, b)| a.abs_diff(*b) <= 1));
    assert!(s.loaded);
    assert!(s.frame_ready());
    assert!(s.srgb_target());
    assert!(!s.opaque());
    assert_eq!(pixels.at(0, 0), [0, 0, 0, 0]);
    let project = |s: &ModelViewer, p: V3| {
        let m = s.model().unwrap();
        let p = exact_model_gltf::math::project(&m.camera.matrix(1.), p);
        (
            ((p[0] / p[3] + 1.) * 200.).round() as u32,
            ((1. - p[1] / p[3]) * 200.).round() as u32,
        )
    };
    let (x, y) = project(&s, [0.4, 0.2, 0.15]);
    let faded = pixels.at(x, y);
    let (hx, hy) = project(&s, [0.85, -0.8, 0.7]);
    let housing = pixels.at(hx, hy);
    let close = |actual: [u8; 4], expected: [u8; 4]| {
        assert!(
            actual
                .into_iter()
                .zip(expected)
                .all(|(a, b)| a.abs_diff(b) <= 3),
            "{actual:?} differs from {expected:?}"
        );
    };
    close(housing, [168, 186, 202, 255]);
    // Both faces of the closed impeller blend at 0.3 over the opaque housing.
    close(faded, [204, 159, 159, 255]);
    let (bx, by) = project(&s, [1.35, 0.15, 0.2]);
    let blend = pixels.at(bx, by);
    close(blend, [31, 59, 80, 89]);
    let over_white = std::array::from_fn(|i| {
        if i == 3 {
            255
        } else {
            blend[i].saturating_add(255 - blend[3])
        }
    });
    close(over_white, [197, 225, 246, 255]);
    let (mx, my) = project(&s, [1.35, -0.35, 0.2]);
    assert_eq!(pixels.at(mx, my), [0, 0, 0, 0]);
    assert_eq!(
        renderer::samples(wgpu::TextureFormatFeatureFlags::empty()),
        1
    );
    s.force_samples = Some(1);
    let single = render_srgb(&gpu, &mut s);
    for (x, y) in [(x, y), (hx, hy), (bx, by), (mx, my), (0, 0)] {
        for (a, b) in pixels.at(x, y).into_iter().zip(single.at(x, y)) {
            assert!((a as i32 - b as i32).abs() <= 2);
        }
    }
    s.arguments.clip_position = [Some(0.); 3];
    s.arguments.clip_normal = [0., 0., -1.];
    let a = s.arguments.clone();
    s.model_mut().unwrap().style(&a);
    let cut = render_srgb(&gpu, &mut s);
    cut.save("assembly-clipped");
    close(cut.at(x, y), [231, 123, 89, 255]);
    s.device_lost();
    assert!(s.model().unwrap().renderer.is_none());
    render_srgb(&gpu, &mut s);
}

#[test]
fn a_source_change_retires_the_previous_asset_subscription() {
    let mut s = viewer();
    let mut a = args();
    a[0] = text("assets/assembly-meshopt.glb");
    s.bind(&a, None).unwrap();
    let changes = s.assets();
    assert_eq!(changes.retired, ["assembly.glb"]);
    assert_eq!(changes.requests, ["assembly-meshopt.glb"]);
    s.asset("assembly-meshopt.glb", Ok(MESHOPT));
    a[0] = text("");
    s.bind(&a, None).unwrap();
    assert_eq!(s.assets().retired, ["assembly-meshopt.glb"]);
    assert!(!s.accepts_asset_update("assembly-meshopt.glb"));
}

#[test]
fn input_uses_the_drawn_projection_and_respects_disabled_controls() {
    let mut s = viewer();
    s.size(100., 400.);
    s.arguments.decay = 0.;
    let a = s.arguments.clone();
    let m = s.model_mut().unwrap();
    m.camera.rebind(&m.scene, &a, false);
    let before = m.camera.pose;
    let fov = m.camera.vertical_fov(0.25);
    let (_, up, _) = before.basis();
    let expected = add(
        before.target,
        mul(up, 20. * 2. * before.radius * (fov / 2.).tan() / 400.),
    );
    let pan = |phase, x, y, at| {
        let mut e = pointer(phase, x, y, at);
        if let InputEvent::Pointer { shift_key, .. } = &mut e {
            *shift_key = true;
        }
        e
    };
    s.input(&pan(PointerPhase::Down, 50., 200., 0.));
    s.input(&pan(PointerPhase::Move, 50., 220., 50.));
    s.input(&pan(PointerPhase::Up, 50., 220., 50.));
    let pose = s.model().unwrap().camera.pose;
    assert!(dot(sub(pose.target, expected), sub(pose.target, expected)) < 1e-10);
    assert_eq!(pose.theta, before.theta);
    assert_eq!(pose.radius, before.radius);
    s.arguments.disable_pan = true;
    s.arguments.disable_zoom = true;
    s.input(&pan(PointerPhase::Down, 50., 200., 100.));
    s.input(&pan(PointerPhase::Move, 50., 240., 150.));
    s.input(&pan(PointerPhase::Up, 50., 240., 150.));
    s.input(&InputEvent::Wheel {
        dx: 0.,
        dy: -20.,
        x: 50.,
        y: 200.,
        shift_key: false,
        ctrl_key: true,
        alt_key: false,
        meta_key: false,
        at_ms: 150.,
    });
    assert_eq!(s.model().unwrap().camera.pose, pose);
    assert!(s.messages().is_empty());
}

#[test]
fn touch_pinch_keyboard_and_cancel_keep_one_camera_goal() {
    let mut s = viewer();
    s.arguments.decay = 0.;
    let a = s.arguments.clone();
    let m = s.model_mut().unwrap();
    m.camera.rebind(&m.scene, &a, false);
    let initial = m.camera.pose;
    let touch = |id, phase, x, at_ms| {
        let mut e = pointer(phase, x, 200., at_ms);
        if let InputEvent::Pointer { id: i, kind, .. } = &mut e {
            *i = id;
            *kind = PointerKind::Touch;
        }
        e
    };
    s.input(&touch(1, PointerPhase::Down, 150., 0.));
    s.input(&touch(2, PointerPhase::Down, 250., 0.));
    s.input(&touch(1, PointerPhase::Move, 100., 50.));
    s.input(&touch(2, PointerPhase::Move, 300., 50.));
    s.input(&touch(1, PointerPhase::Up, 100., 50.));
    s.input(&touch(2, PointerPhase::Up, 300., 50.));
    let pose = s.model().unwrap().camera.pose;
    assert!((pose.radius - initial.radius / 2.).abs() < 1e-6);
    assert!(
        dot(
            sub(pose.target, initial.target),
            sub(pose.target, initial.target)
        ) < 1e-10
    );
    assert!(s.messages().is_empty(), "neither finger in a pinch picks");
    let key = |code: &str| InputEvent::Key {
        code: code.into(),
        key: code.into(),
        down: true,
        repeat: false,
        at_ms: 100.,
    };
    s.input(&key("ArrowLeft"));
    assert!(
        (s.model().unwrap().camera.pose.theta - initial.theta - std::f32::consts::PI / 8.).abs()
            < 1e-6
    );
    s.input(&key("PageUp"));
    assert!((s.model().unwrap().camera.pose.radius - pose.radius * 0.9).abs() < 1e-6);
    s.arguments.decay = 50.;
    s.model_mut().unwrap().camera.decay = 50.;
    s.input(&touch(1, PointerPhase::Down, 150., 100.));
    s.input(&touch(1, PointerPhase::Move, 170., 150.));
    let goal = s.model().unwrap().camera.goal;
    s.input(&touch(1, PointerPhase::Cancel, 170., 150.));
    assert_eq!(s.model().unwrap().camera.goal, goal);
    assert!(s.messages().is_empty());
}

#[test]
fn the_agent_clock_has_a_deadline_for_camera_motion() {
    let mut s = viewer();
    s.input(&pointer(PointerPhase::Down, 100., 100., 0.));
    s.input(&pointer(PointerPhase::Move, 140., 100., 50.));
    s.input(&pointer(PointerPhase::Up, 140., 100., 50.));
    let reply: serde_json::Value =
        serde_json::from_str(&s.agent(r#"{"op":"clock","now":50,"settle":true}"#).unwrap())
            .unwrap();
    assert_eq!(reply["quiescent"], false);
    let end = reply["settleAt"].as_f64().unwrap();
    assert!(end > 50.);
    let m = s.model_mut().unwrap();
    m.camera.step(&m.scene, end);
    assert!(!s.model().unwrap().camera.moving());
    let reply: serde_json::Value = serde_json::from_str(
        &s.agent(&serde_json::json!({"op":"clock","now":end,"settle":true}).to_string())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reply["quiescent"], true);
    assert!(reply.get("settleAt").is_none());
}

fn layered_scene(depths: &[(f32, bool)], occluder: bool) -> (Scene, Vec<Effective>) {
    use exact_model_gltf::scene::Vertex;
    let mut scene = glb::parse(GLB).unwrap();
    scene.vertices.clear();
    scene.indices.clear();
    scene.primitives.clear();
    for (name, triangles) in [
        ("Impeller", depths.to_vec()),
        ("Housing", if occluder { vec![(0., true)] } else { vec![] }),
    ] {
        let node = scene.find(name).unwrap();
        let start = scene.indices.len() as u32;
        for (z, front) in triangles {
            let base = scene.vertices.len() as u32;
            for p in [[-1., -1., z], [1., -1., z], [0., 1., z]] {
                scene.vertices.push(Vertex {
                    p,
                    n: [0., 0., 1.],
                    node: node as u32,
                    primitive: scene.primitives.len() as u32,
                });
            }
            scene.indices.extend(if front {
                [base, base + 1, base + 2]
            } else {
                [base, base + 2, base + 1]
            });
        }
        scene.primitives.push(Primitive {
            node,
            indices: start..scene.indices.len() as u32,
            material: Material::default(),
        });
    }
    let effective = vec![
        Effective {
            color: [1., 0., 0., 0.5],
            visible: true,
            occlusion: Occlusion::Revealed,
            occluded_opacity: 0.3,
        },
        Effective {
            color: [0., 0., 1., 1.],
            visible: true,
            occlusion: Occlusion::Occluder,
            occluded_opacity: 0.3,
        },
    ];
    (scene, effective)
}

#[test]
fn a_farther_revealed_triangle_still_takes_the_faded_pick() {
    let (scene, effective) = layered_scene(&[(1., true), (-1., true)], true);
    assert_eq!(
        pick::pick(
            &scene,
            &crate::bvh::Bvh::new(&scene),
            &effective,
            Clip::default(),
            [0., 0., 5.],
            [0., 0., -1.],
            [0., f32::INFINITY]
        ),
        "Impeller"
    );
}

#[test]
fn translucent_revealed_back_faces_are_picked_only_when_drawn_faded() {
    for (occluder, expected) in [(false, ""), (true, "Impeller")] {
        let (scene, effective) = layered_scene(&[(-1., false)], occluder);
        assert_eq!(
            pick::pick(
                &scene,
                &crate::bvh::Bvh::new(&scene),
                &effective,
                Clip::default(),
                [0., 0., 5.],
                [0., 0., -1.],
                [0., f32::INFINITY]
            ),
            expected
        );
    }
}

#[test]
fn gpu_culls_translucent_revealed_back_faces_except_in_the_faded_pass() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    for occluder in [false, true] {
        let mut s = viewer();
        let m = s.model_mut().unwrap();
        (m.scene, m.effective) = layered_scene(&[(-1., false)], occluder);
        let pixels = render_srgb(&gpu, &mut s);
        if occluder {
            assert!(pixels.at(200, 200)[0] > 0);
            assert_eq!(pixels.at(200, 200)[3], 255);
        } else {
            assert_eq!(pixels.at(200, 200), [0, 0, 0, 0]);
        }
    }
}

#[path = "../../gltf/tests/support/mod.rs"]
mod gltf_support;

#[test]
fn gpu_mirrored_translucent_nodes_keep_their_front_faces_and_picks() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    for normals in [false, true] {
        for reflected in [false, true] {
            let (mut json, _) = gltf_support::triangle();
            let mut bin: Vec<_> = [-1f32, -1., 0., 1., -1., 0., 0., 1., 0.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect();
            if normals {
                bin.extend(
                    [0f32, 0., 1.]
                        .repeat(3)
                        .into_iter()
                        .flat_map(f32::to_le_bytes),
                );
                json["bufferViews"]
                    .as_array_mut()
                    .unwrap()
                    .push(serde_json::json!({"buffer":0,"byteOffset":36,"byteLength":36}));
                json["accessors"].as_array_mut().unwrap().push(
                    serde_json::json!({"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}));
                json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"] = 1.into();
            }
            json["buffers"][0]["byteLength"] = bin.len().into();
            json["nodes"] = serde_json::json!([
                {"children":[1], "scale":[if reflected { -1. } else { 1. },1,1]},
                {"mesh":0,"name":"Pane"}
            ]);
            json["materials"] = serde_json::json!([{"alphaMode":"BLEND"}]);
            json["materials"][0]["pbrMetallicRoughness"] =
                serde_json::json!({"baseColorFactor":[1,0,0,0.5]});
            json["meshes"][0]["primitives"][0]["material"] = 0.into();
            let mut s = viewer();
            let a = s.arguments.clone();
            let m = s.model_mut().unwrap();
            m.scene = glb::parse(&gltf_support::glb(&json, &bin)).unwrap();
            m.bvh = crate::bvh::Bvh::new(&m.scene);
            m.style(&a);
            let pixels = render_srgb(&gpu, &mut s);
            assert!(
                pixels.at(200, 200)[3] > 0,
                "normals={normals}, reflected={reflected}"
            );
            message(&mut s, "pick 200 200", 1000.);
            assert_eq!(s.messages(), ["Pane"]);
        }
    }
}

#[test]
fn a_saved_panned_view_restores_its_projection() {
    for decay in [0., 50.] {
        let mut s = viewer();
        s.size(100., 400.);
        s.arguments.decay = decay;
        let m = s.model_mut().unwrap();
        let initial_fov = m.camera.vertical_fov(0.25);
        m.camera.decay = decay;
        let pan = |phase, x, y, at| {
            let mut e = pointer(phase, x, y, at);
            if let InputEvent::Pointer { shift_key, .. } = &mut e {
                *shift_key = true;
            }
            e
        };
        s.input(&pan(PointerPhase::Down, 10., 200., 0.));
        s.input(&pan(PointerPhase::Move, 70., 240., 50.));
        s.input(&pan(PointerPhase::Up, 70., 240., 50.));
        let m = s.model_mut().unwrap();
        m.camera.step(&m.scene, 100.);
        let expected = exact_model_gltf::framing::Framing::new(
            &m.scene,
            m.camera.pose.target,
            m.camera.pose.fov,
        );
        assert!((m.camera.vertical_fov(0.25) - expected.vertical_field_of_view(0.25)).abs() < 1e-6);
        m.camera.step(&m.scene, 1000.);
        let pose = m.camera.pose;
        let fov = m.camera.vertical_fov(0.25);
        let record: serde_json::Value = serde_json::from_str(&s.published().unwrap()).unwrap();
        message(&mut s, "reset", 1000.);
        let m = s.model_mut().unwrap();
        m.camera.step(&m.scene, 2000.);
        assert!((m.camera.vertical_fov(0.25) - initial_fov).abs() < 1e-6);
        let mut a = args();
        a[0] = text("assets/assembly.glb");
        a[2] = text(record["cameraOrbit"].as_str().unwrap());
        a[3] = text(record["cameraTarget"].as_str().unwrap());
        a[4] = text(record["fieldOfView"].as_str().unwrap());
        a[7] = Value::Number(0.);
        s.bind(&a, Some(2000.)).unwrap();
        let m = s.model().unwrap();
        assert!(
            (m.camera.vertical_fov(0.25) - fov).abs() < 1e-6,
            "decay={decay}: saved {fov}, restored {}",
            m.camera.vertical_fov(0.25)
        );
        for x in [20., 50., 80.] {
            let before = pose.ray(x, 200., 100., 400., fov);
            let after = m
                .camera
                .pose
                .ray(x, 200., 100., 400., m.camera.vertical_fov(0.25));
            assert!(dot(sub(before.0, after.0), sub(before.0, after.0)) < 1e-8);
            assert!(dot(sub(before.1, after.1), sub(before.1, after.1)) < 1e-8);
        }
    }
}

#[test]
fn source_lifecycle_table_keeps_subscriptions_and_the_latest_delivery() {
    #[derive(Clone, Copy, Debug)]
    enum Step<'a> {
        Bind(&'static str),
        Deliver(&'a [u8]),
        Missing,
        Finish,
    }
    use Step::*;
    let (mut json, bin) = gltf_support::triangle();
    json["nodes"][0]["name"] = "Latest".into();
    let latest = gltf_support::glb(&json, &bin);
    for steps in [
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Deliver(&latest),
            Bind("other"),
            Bind("assembly"),
            Finish,
        ],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Bind("other"),
            Bind("assembly"),
            Finish,
        ],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Bind(""),
            Bind("assembly"),
            Finish,
        ],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Deliver(MESHOPT),
            Bind("other"),
            Bind("assembly"),
            Finish,
        ],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Deliver(&GLB[..20]),
            Bind(""),
            Bind("assembly"),
            Finish,
        ],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Bind("other"),
            Bind("assembly"),
            Deliver(MESHOPT),
            Finish,
        ],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Deliver(&GLB[..20]),
            Deliver(MESHOPT),
            Finish,
        ],
        vec![Bind("assembly"), Deliver(GLB), Bind("other"), Finish],
        vec![Bind("assembly"), Deliver(GLB), Bind(""), Finish],
        vec![
            Bind("assembly"),
            Deliver(GLB),
            Missing,
            Bind(""),
            Bind("assembly"),
            Finish,
        ],
    ] {
        let mut s = ModelViewer::default();
        s.clock(true);
        let mut sequence = 0;
        let mut newest = GLB;
        for step in &steps {
            match *step {
                Bind(name) => {
                    let old_name = s.source.name().map(str::to_string);
                    let old_sequence = s.generation;
                    let mut a = args();
                    a[0] = text(&if name.is_empty() {
                        String::new()
                    } else {
                        format!("assets/{name}.glb")
                    });
                    s.bind(&a, None).unwrap();
                    let changes = s.assets();
                    assert_eq!(
                        changes.retired,
                        old_name.into_iter().collect::<Vec<_>>(),
                        "{steps:?}"
                    );
                    assert_eq!(
                        changes.requests,
                        if name.is_empty() {
                            vec![]
                        } else {
                            vec![format!("{name}.glb")]
                        },
                        "{steps:?}"
                    );
                    assert!(s.generation > old_sequence);
                }
                Deliver(bytes) => {
                    if sequence == 0 {
                        let Source::Pending(load) = std::mem::take(&mut s.source) else {
                            panic!("{steps:?}")
                        };
                        sequence = load.sequence;
                        s.jobs.insert(
                            load.name.clone(),
                            Job {
                                sequence,
                                bytes: Some(Arc::from(bytes)),
                            },
                        );
                        s.source = Source::Parsing {
                            load,
                            job: sequence,
                        };
                    } else {
                        s.asset("assembly.glb", Ok(bytes));
                    }
                    newest = bytes;
                    assert_eq!(s.jobs.len(), 1);
                    assert!(s.preparing());
                }
                Missing => {
                    s.asset("assembly.glb", Err(AssetError::Missing));
                    assert!(s.error.contains("file not found"));
                    assert!(s.model().is_none());
                }
                Finish => {
                    s.finish(Parsed {
                        timing_session: s.timing_session,
                        parse_ms: None,
                        sequence,
                        name: "assembly.glb".into(),
                        scene: glb::parse(GLB).map(|s| {
                            let b = crate::bvh::Bvh::new(&s);
                            (s, b)
                        }),
                    });
                    assert!(s.jobs.is_empty());
                    match s.arguments.src.as_str() {
                        "assets/assembly.glb" => {
                            let missing = steps.iter().any(|s| matches!(s, Missing));
                            assert_eq!(s.preparing(), missing, "{steps:?}");
                            if missing {
                                assert!(s.model().is_none());
                            } else {
                                match glb::parse(newest) {
                                    Ok(expected) => {
                                        let scene = &s.model().unwrap().scene;
                                        assert_eq!(scene.triangles(), expected.triangles());
                                        assert_eq!(
                                            scene.nodes.iter().map(|n| &n.name).collect::<Vec<_>>(),
                                            expected
                                                .nodes
                                                .iter()
                                                .map(|n| &n.name)
                                                .collect::<Vec<_>>()
                                        );
                                    }
                                    Err(_) => {
                                        assert!(s.model().is_none());
                                        assert!(s.error.contains("assembly.glb"));
                                    }
                                }
                            }
                            // A completed parse cannot present a frame it has not drawn.
                            assert!(!s.frame_ready());
                            assert!(!s.loaded);
                            // The re-established subscription accepts the next development delivery.
                            s.asset("assembly.glb", Ok(MESHOPT));
                            assert!(s.error.is_empty());
                            assert!(s.model().is_some());
                        }
                        "assets/other.glb" => {
                            assert!(s.preparing());
                            assert!(s.model().is_none());
                        }
                        "" => {
                            assert!(!s.preparing());
                            assert!(s.model().is_none());
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
    }
}

#[test]
fn fling_velocity_includes_the_pause_before_release() {
    for (release, x, expected_degrees) in [
        (100., 140., -54.),
        (101., 140., -36.),
        (200., 140., -36.),
        (100., 160., -81.),
    ] {
        let mut s = viewer();
        s.input(&pointer(PointerPhase::Down, 100., 100., 0.));
        s.input(&pointer(PointerPhase::Move, 140., 100., 1.));
        s.input(&pointer(PointerPhase::Up, x, 100., release));
        let theta = s.model().unwrap().camera.goal.theta.to_degrees();
        assert!(
            (theta - expected_degrees).abs() < 0.001,
            "release {release}: {theta}"
        );
    }
}

#[test]
fn pinch_fling_adds_radius_velocity_and_clamps_to_the_model() {
    for factor in [2., 10., 0.25] {
        let mut s = viewer();
        s.arguments.disable_pan = true;
        let touch = |id, phase, x, at_ms| {
            let mut e = pointer(phase, x, 200., at_ms);
            if let InputEvent::Pointer { id: i, kind, .. } = &mut e {
                *i = id;
                *kind = PointerKind::Touch;
            }
            e
        };
        let m = s.model_mut().unwrap();
        m.camera.goal.radius = 10.;
        let r = m.camera.framing.radius;
        s.input(&touch(1, PointerPhase::Down, 150., 0.));
        s.input(&touch(2, PointerPhase::Down, 250., 0.));
        s.input(&touch(2, PointerPhase::Move, 150. + 100. * factor, 100.));
        let b = s.model().unwrap().camera.goal.radius;
        s.input(&touch(1, PointerPhase::Up, 150., 100.));
        s.input(&touch(2, PointerPhase::Up, 150. + 100. * factor, 100.));
        let expected = (b + (b - 10.) * 0.5).clamp(0.15 * r, 12. * r);
        let actual = s.model().unwrap().camera.goal.radius;
        assert!(
            (actual - expected).abs() < 1e-6,
            "factor {factor}: {actual} != {expected}"
        );
    }
}

#[test]
fn release_movement_keeps_the_pan_gesture() {
    for decay in [0., 50.] {
        for mode in ["right", "shift", "control", "command"] {
            let mut poses = vec![];
            for final_move in [false, true] {
                let mut s = viewer();
                s.model_mut().unwrap().camera.decay = decay;
                let pan = |phase, x, at| {
                    let mut e = pointer(phase, x, 200., at);
                    if let InputEvent::Pointer {
                        buttons,
                        shift_key,
                        ctrl_key,
                        meta_key,
                        ..
                    } = &mut e
                    {
                        if phase != PointerPhase::Up {
                            *buttons = if mode == "right" { 2 } else { 1 };
                            *shift_key = mode == "shift";
                            *ctrl_key = mode == "control";
                            *meta_key = mode == "command";
                        }
                    }
                    e
                };
                s.input(&pan(PointerPhase::Down, 100., 0.));
                s.input(&pan(PointerPhase::Move, 140., 50.));
                if final_move {
                    s.input(&pan(PointerPhase::Move, 180., 100.));
                }
                s.input(&pan(PointerPhase::Up, 180., 100.));
                poses.push(s.model().unwrap().camera.goal);
            }
            assert_eq!(poses[0].theta, 0., "decay={decay}, mode={mode}");
            assert_eq!(poses[0], poses[1], "decay={decay}, mode={mode}");
        }
    }
}

#[test]
fn release_movement_keeps_the_pinch_after_either_finger_lifts() {
    for decay in [0., 50.] {
        for first_lift in [false, true] {
            let mut poses = vec![];
            for final_move in [false, true] {
                let mut s = viewer();
                s.arguments.disable_pan = true;
                let m = s.model_mut().unwrap();
                m.camera.decay = decay;
                m.camera.goal.radius = 10.;
                let touch = |id, phase, x, at| {
                    let mut e = pointer(phase, x, 200., at);
                    if let InputEvent::Pointer { id: i, kind, .. } = &mut e {
                        *i = id;
                        *kind = PointerKind::Touch;
                    }
                    e
                };
                s.input(&touch(1, PointerPhase::Down, 150., 0.));
                s.input(&touch(2, PointerPhase::Down, 250., 0.));
                s.input(&touch(2, PointerPhase::Move, 350., 50.));
                if final_move {
                    s.input(&touch(2, PointerPhase::Move, 390., 100.));
                }
                if first_lift {
                    s.input(&touch(1, PointerPhase::Up, 150., 100.));
                }
                s.input(&touch(2, PointerPhase::Up, 390., 100.));
                if !first_lift {
                    s.input(&touch(1, PointerPhase::Up, 150., 100.));
                }
                poses.push(s.model().unwrap().camera.goal);
            }
            assert_eq!(poses[0].theta, 0., "decay={decay}, first_lift={first_lift}");
            assert_eq!(poses[0], poses[1], "decay={decay}, first_lift={first_lift}");
        }
    }
}

#[test]
fn disabling_controls_cancels_contacts_keys_and_fling_history() {
    let mut s = viewer();
    s.input(&InputEvent::Key {
        code: "ShiftLeft".into(),
        key: "Shift".into(),
        down: true,
        repeat: false,
        at_ms: 0.,
    });
    s.input(&pointer(PointerPhase::Down, 100., 100., 0.));
    s.input(&pointer(PointerPhase::Move, 140., 100., 50.));
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    a[2] = text("0deg 90deg 105%");
    a[3] = text("0m 0m 0m");
    s.bind(&a, Some(50.)).unwrap();
    s.input(&pointer(PointerPhase::Up, 180., 100., 100.));
    a[1] = Value::Bool(true);
    s.bind(&a, Some(100.)).unwrap();
    let before = s.model().unwrap().camera.goal;
    s.input(&pointer(PointerPhase::Move, 200., 100., 110.));
    assert_eq!(
        s.model().unwrap().camera.goal,
        before,
        "hover after re-enable"
    );
    s.input(&InputEvent::Key {
        code: "ArrowLeft".into(),
        key: "ArrowLeft".into(),
        down: true,
        repeat: false,
        at_ms: 120.,
    });
    let after = s.model().unwrap().camera.goal;
    assert_eq!(after.target, before.target, "Shift must no longer be held");
    assert!((after.theta - before.theta - std::f32::consts::PI / 8.).abs() < 1e-6);
    s.input(&pointer(PointerPhase::Up, 220., 100., 130.));
    assert_eq!(s.model().unwrap().camera.goal, after, "no stale fling");
}

#[test]
fn picks_reject_geometry_outside_the_camera_depth_range() {
    let gpu = fixture::device_or_skip(fixture::device());
    let mut s = viewer();
    let m = s.model_mut().unwrap();
    m.camera.pose = camera::Pose {
        theta: 0.,
        phi: std::f32::consts::FRAC_PI_2,
        radius: 4.,
        target: [0.; 3],
        ..m.camera.pose
    };
    m.camera.goal = m.camera.pose;
    (m.scene, m.effective) = layered_scene(&[(0., true)], true);
    m.effective[0].occlusion = Occlusion::None;
    m.effective[0].color[3] = 1.;
    for (z, x) in [(3.998, 200), (3.99601, 280)] {
        let m = s.model_mut().unwrap();
        m.renderer = None;
        for p in &mut m.scene.vertices[3..] {
            p.p[2] = z;
        }
        m.bvh = crate::bvh::Bvh::new(&m.scene);
        message(&mut s, &format!("pick {x} 200"), 0.);
        assert_eq!(s.messages(), ["Impeller"]);
        if let Some(gpu) = &gpu {
            let pixels = render_srgb(gpu, &mut s);
            let pixel = pixels.at(x, 200);
            assert!(pixel[0] > 0);
            assert_eq!(pixel[2], 0);
        }
    }
    let m = s.model_mut().unwrap();
    m.effective[1].visible = false;
    m.renderer = None;
    for p in &mut m.scene.vertices[..3] {
        p.p[2] = -1000.;
    }
    m.bvh = crate::bvh::Bvh::new(&m.scene);
    message(&mut s, "pick 200 200", 0.);
    assert_eq!(s.messages(), [""]);
    if let Some(gpu) = &gpu {
        assert_eq!(render_srgb(gpu, &mut s).at(200, 200), [0; 4]);
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn destroying_a_surface_joins_superseded_parsers() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let mut s = ModelViewer::default();
    let mut a = args();
    for (name, count) in [("assembly", 1), ("superseded", 2)] {
        a[0] = text(&format!("assets/{name}.glb"));
        s.bind(&a, None).unwrap();
        s.asset(&format!("{name}.glb"), Ok(GLB));
        assert_eq!(s.parsers.len(), count);
    }
    let mut releases = vec![];
    let completed = Arc::new(AtomicUsize::new(0));
    for _ in 0..2 {
        let (release, held) = mpsc::channel();
        let completed = completed.clone();
        s.parsers.push(std::thread::spawn(move || {
            held.recv().unwrap();
            glb::parse(GLB).unwrap();
            completed.fetch_add(1, Ordering::SeqCst);
        }));
        releases.push(release);
    }
    // Neither job belongs to the current source.
    a[0] = text("assets/other.glb");
    s.bind(&a, None).unwrap();
    let expected_joins = s.parsers.len();
    let (joining, began) = mpsc::channel();
    s.parser_joins = Some(joining);
    let (destroyed, done) = mpsc::channel();
    let destroyer = std::thread::spawn(move || {
        drop(s);
        destroyed.send(completed.load(Ordering::SeqCst)).unwrap();
    });
    // The hook is owned by the surface, so a missing join path disconnects it.
    let joining = matches!(began.recv(), Ok(None));
    for release in releases {
        release.send(()).unwrap();
    }
    let mut joined = 0;
    for result in began {
        result.unwrap().unwrap();
        joined += 1;
    }
    let finished_at_destruction = done.recv().unwrap();
    destroyer.join().unwrap();
    assert!(joining, "destruction skipped the parser joins");
    assert_eq!(joined, expected_joins, "destruction skipped a parser join");
    assert_eq!(finished_at_destruction, 2, "destruction detached a parser");
}

#[test]
fn gpu_readiness_tracks_the_picture_drawn_through_replacements_and_failures() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut s = viewer();
    s.set_performance_reporting(true);
    s.asset("assembly.glb", Ok(GLB));
    assert!(!s.frame_ready());
    let (first, _) = fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert!(s.frame_ready());
    assert!(s.loaded);
    let snapshot: serde_json::Value =
        serde_json::from_str(&s.take_performance(0.).unwrap()).unwrap();
    assert!(snapshot["uploadMs"].is_number());
    assert!(snapshot["loadToSubmitMs"].as_f64().unwrap() >= snapshot["parseMs"].as_f64().unwrap());
    fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    let next: serde_json::Value =
        serde_json::from_str(&s.agent(r#"{"op":"state","performance":true}"#).unwrap()).unwrap();
    assert_eq!(
        next["performance"]["loadToSubmitMs"],
        snapshot["loadToSubmitMs"]
    );
    let mut a = args();
    a[0] = text("assets/other.glb");
    a[2] = text("0deg 90deg 105%");
    a[3] = text("0m 0m 0m");
    a[8] = values(
        r#"[[["Housing","",1,"visible","occluder"],["Impeller","",1,"visible","revealed"]]]"#,
    );
    s.bind(&a, None).unwrap();
    let (pending, wants) = fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert!(wants);
    assert!(!s.frame_ready());
    assert!(!s.loaded);
    assert_eq!(pending.data, first.data);
    s.asset("other.glb", Ok(MESHOPT));
    assert!(!s.frame_ready());
    fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert!(s.frame_ready());
    assert!(s.loaded);
    s.asset("other.glb", Ok(&GLB[..20]));
    assert!(!s.frame_ready());
    assert!(!s.loaded);
    let (failed, _) = fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert!(s.frame_ready());
    assert!(failed.data.iter().all(|v| *v == 0));
    assert!(s.error.contains("other.glb"));
    a[0] = text("");
    s.bind(&a, None).unwrap();
    assert!(!s.frame_ready());
    let (empty, _) = fixture::render(&gpu, &mut s, &frame(0.)).unwrap();
    assert!(s.frame_ready());
    assert!(!s.loaded);
    assert!(s.error.is_empty());
    assert!(empty.data.iter().all(|v| *v == 0));
}

#[test]
fn device_lost_re_requests_pending_asset_through_module() {
    static REGISTRY: exact_gpu::Registry = exact_gpu::Registry {
        surfaces: &[("modelViewer", 13, crate::surface)],
        shaders: &[],
    };
    let mut module = exact_gpu::Module::new(&REGISTRY);
    module.set_seekable(true);
    let id = module.create_headless("modelViewer").unwrap();
    let mut a = args();
    a[0] = text("assets/assembly.glb");
    assert!(module.bind(id, &a, None));
    assert_eq!(module.take_assets(id).requests, ["assembly.glb"]);
    // Losing the device before delivery clears outstanding requests in Module;
    // ModelViewer must re-emit the pending asset so recovery can deliver it.
    module.lose_device();
    assert_eq!(module.take_assets(id).requests, ["assembly.glb"]);
    assert!(module.asset(id, "assembly.glb", Ok(GLB)));
    assert!(module.take_assets(id).requests.is_empty());
    let clock: serde_json::Value =
        serde_json::from_str(&module.agent(id, r#"{"op":"clock"}"#).unwrap()).unwrap();
    assert_eq!(clock["quiescent"], true);

    // A superseded parse awaiting fresh delivery (load.sequence != job) also re-requests.
    let mut s = ModelViewer::default();
    s.bind(&a, None).unwrap();
    let _ = s.assets();
    let Source::Pending(load) = std::mem::take(&mut s.source) else {
        panic!("expected pending source");
    };
    s.source = Source::Parsing {
        job: load.sequence + 1,
        load,
    };
    s.device_lost();
    assert_eq!(s.assets().requests, ["assembly.glb"]);
}
