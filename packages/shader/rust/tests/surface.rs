use exact_gpu::{fixture, wgpu, AssetError, Frame, Surface, Value};

fn frame(now_ms: f64) -> Frame {
    Frame {
        width: 16.0,
        height: 12.0,
        scale: 1.0,
        now_ms,
        seekable: true,
        period_ms: 0.0,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    }
}

fn inputs(play: Option<&str>, values: &[(&str, Value)]) -> Vec<Value> {
    vec![
        Value::str("assets/test.wgsl"),
        Value::Number(30.0),
        play.map(Value::str).unwrap_or(Value::Unit),
        Value::Unit,
        Value::List(
            values
                .iter()
                .map(|(name, value)| Value::List(vec![Value::str(name), value.clone()].into()))
                .collect::<Vec<_>>()
                .into(),
        ),
    ]
}

fn load(text: &str, play: Option<&str>, values: &[(&str, Value)]) -> Box<dyn Surface> {
    let mut surface = exact_shader::surface();
    surface.bind(&inputs(play, values), None).unwrap();
    assert_eq!(surface.assets().requests, ["test.wgsl"]);
    assert!(!surface.frame_ready());
    surface.asset("test.wgsl", Ok(text.as_bytes()));
    assert!(surface.take_error().is_none());
    surface
}

fn draw(gpu: &exact_gpu::Gpu, surface: &mut dyn Surface, frame: &Frame) -> (fixture::Pixels, bool) {
    surface.prepare_assets(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm);
    let result = fixture::render(gpu, surface, frame).unwrap();
    assert!(surface.frame_ready(), "{:?}", surface.take_error());
    result
}

fn near(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "{actual:?} != {expected:?}"
    );
}

fn numbers(values: &[f64]) -> Value {
    Value::List(
        values
            .iter()
            .copied()
            .map(Value::Number)
            .collect::<Vec<_>>()
            .into(),
    )
}

#[test]
fn a_fragment_file_draws_encoded_srgb_with_opaque_alpha() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = load(
        "@fragment fn solid() -> @location(0) vec4f { return vec4f(0.25, 0.5, 0.75, 0.0); }",
        None,
        &[],
    );
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(0.0));
    for point in [(0, 0), (8, 6), (15, 11)] {
        near(pixels.at(point.0, point.1), [64, 128, 191, 255]);
        assert_eq!(pixels.at(point.0, point.1)[3], 255);
    }
    assert!(!wants);
    assert!(surface.opaque());
    assert!(!surface.srgb_target());
    assert_eq!(surface.max_fps(), 30);
    assert_eq!(surface.published().as_deref(), Some("{\"error\":\"\"}"));
    assert_eq!(surface.published(), None);
}

#[test]
fn premultiplied_alpha_reaches_the_page_and_a_switch_blends_until_its_picture_draws() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let text = "@fragment fn edge(@builtin(position) at: vec4f) -> @location(0) vec4f {
        return select(vec4f(0.25, 0.125, 0., 0.5), vec4f(0.), at.x > 8.);
    }";
    let mut surface = load(text, None, &[]);
    let (pixels, _) = draw(&gpu, surface.as_mut(), &frame(0.0));
    near(pixels.at(2, 6), [64, 32, 0, 255]);
    assert_eq!(pixels.at(12, 6), [0, 0, 0, 255]);
    assert!(surface.opaque());

    let mut bound = inputs(None, &[]);
    for (mode, inside, outside) in [
        ("premultiplied", [64, 32, 0, 128], [0, 0, 0, 0]),
        ("opaque", [64, 32, 0, 255], [0, 0, 0, 255]),
    ] {
        bound[3] = Value::str(mode);
        surface.bind(&bound, None).unwrap();
        // Until the new picture draws, the one shown may write alpha.
        assert!(!surface.opaque(), "{mode}");
        let (pixels, _) = draw(&gpu, surface.as_mut(), &frame(0.0));
        near(pixels.at(2, 6), inside);
        assert_eq!(pixels.at(12, 6), outside);
        assert_eq!(surface.opaque(), mode == "opaque");
    }

    bound[3] = Value::str("straight");
    let error = surface.bind(&bound, None).unwrap_err();
    assert_eq!(error.0, "alphaMode: expected opaque or premultiplied");
    assert!(surface.opaque());
}

#[test]
fn clearing_a_loaded_source_discards_retained_pixels_until_a_new_frame() {
    static REGISTRY: exact_gpu::Registry = exact_gpu::Registry {
        surfaces: &[("shader", 4, exact_shader::surface)],
        shaders: &[],
    };
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut module = exact_gpu::Module::new(&REGISTRY);
    module.set_gpu(gpu);
    let id = module.create_headless("shader").unwrap();
    let args = inputs(None, &[]);
    assert!(module.bind(id, &args, None));
    module.take_assets(id);
    let text = b"@fragment fn solid() -> @location(0) vec4f { return vec4f(0.2, 0.4, 0.6, 0.); }";
    assert!(module.asset(id, "test.wgsl", Ok(text)));
    let mut pixels = vec![0; 16 * 12 * 4];
    assert_eq!(
        module.readback_into(id, &frame(0.), &mut pixels),
        Some(false)
    );
    assert_eq!(&pixels[..4], &[51, 102, 153, 255]);
    assert!(module
        .take_published(id)
        .unwrap()
        .contains("\"presented\":true"));

    assert!(module.bind(id, &[Value::str("")], Some(1000.)));
    for now in [1000., 2000.] {
        assert_eq!(
            module.readback_into(id, &frame(now), &mut pixels),
            Some(false)
        );
        assert!(pixels.iter().all(|&byte| byte == 0), "{:?}", &pixels[..4]);
    }
    assert_eq!(module.presentation_flags(id) & 4, 4);
    assert!(module
        .take_published(id)
        .unwrap()
        .contains("\"presented\":false"));

    assert!(module.bind(id, &args, Some(3000.)));
    module.take_assets(id);
    assert!(module.asset(id, "test.wgsl", Ok(text)));
    assert_eq!(
        module.readback_into(id, &frame(3000.), &mut pixels),
        Some(false)
    );
    assert_eq!(&pixels[..4], &[51, 102, 153, 255]);
    assert_eq!(module.presentation_flags(id) & 4, 0);
    assert!(module
        .take_published(id)
        .unwrap()
        .contains("\"presented\":true"));
}

const TYPES: &str = "
struct Uniforms { f: f32, i: i32, n: u32, v2: vec2f, v3: vec3f, v4: vec4f, tint: vec4f }
@group(0) @binding(0) var<uniform> u: Uniforms;
@fragment fn types(@builtin(position) at: vec4f) -> @location(0) vec4f {
    switch u32(at.x) / 2u {
        case 0u: { return vec4f(u.f, 0., 0., 0.); }
        case 1u: { return vec4f(f32(u.i) / 4., 0., 0., 0.); }
        case 2u: { return vec4f(f32(u.n) / 4., 0., 0., 0.); }
        case 3u: { return vec4f(u.v2, 0., 0.); }
        case 4u: { return vec4f(u.v3, 0.); }
        case 5u: { return u.v4; }
        default: { return u.tint; }
    }
}";

#[test]
fn every_bound_type_changes_its_pixels_and_a_refused_bind_keeps_them() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut values = vec![
        ("f", Value::Number(0.25)),
        ("i", Value::Number(-1.0)),
        ("n", Value::Bool(true)),
        ("v2", numbers(&[0.25, 0.5])),
        ("v3", numbers(&[0.25, 0.5, 0.75])),
        ("v4", numbers(&[0.25, 0.5, 0.75, 0.0])),
        ("tint", Value::str("#0a84ff80")),
    ];
    let mut surface = load(TYPES, None, &values);
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(0.0));
    assert!(!wants);
    for (x, expected) in [
        (0, [64, 0, 0, 255]),
        (2, [0, 0, 0, 255]),
        (4, [64, 0, 0, 255]),
        (6, [64, 128, 0, 255]),
        (8, [64, 128, 191, 255]),
        (10, [64, 128, 191, 255]),
        (12, [10, 132, 255, 255]),
    ] {
        near(pixels.at(x, 6), expected);
    }
    let changed = [
        Value::Number(0.75),
        Value::Number(3.0),
        Value::Number(3.0),
        numbers(&[0.75, 0.25]),
        numbers(&[0.75, 0.25, 0.5]),
        numbers(&[0.75, 0.25, 0.5, 0.2]),
        Value::str("color(display-p3 1 0 0 / 0.2)"),
    ];
    for (index, value) in changed.into_iter().enumerate() {
        values[index].1 = value;
        surface.bind(&inputs(None, &values), Some(0.0)).unwrap();
        let (next, wants) = draw(&gpu, surface.as_mut(), &frame(0.0));
        assert!(!wants);
        assert_ne!(pixels.at(index as u32 * 2, 6), next.at(index as u32 * 2, 6));
    }
    let (accepted, _) = draw(&gpu, surface.as_mut(), &frame(0.0));
    for (name, value) in [
        ("absent", Value::Number(1.0)),
        ("i", Value::Number(0.5)),
        ("v2", numbers(&[1.0])),
        ("tint", Value::str("CanvasText")),
    ] {
        let mut refused = values.clone();
        refused.retain(|(n, _)| *n != name);
        refused.push((name, value));
        let error = surface.bind(&inputs(None, &refused), None).unwrap_err();
        assert!(error.0.contains(name), "{}", error.0);
        assert_eq!(draw(&gpu, surface.as_mut(), &frame(0.0)).0, accepted);
    }
}

#[test]
fn resolution_and_scale_follow_the_frame_and_author_values_win() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let text = "struct U { resolution: vec2f, devicePixelRatio: f32 }
        @group(0) @binding(0) var<uniform> u: U;
        @fragment fn sizes() -> @location(0) vec4f {
            return vec4f(u.resolution / 100., u.devicePixelRatio / 4., 0.);
        }";
    let mut surface = load(text, None, &[]);
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(0.0));
    near(pixels.at(8, 6), [41, 31, 64, 255]);
    assert!(!wants);
    let scaled = Frame {
        width: 20.,
        height: 10.,
        scale: 2.,
        ..frame(1000.0)
    };
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &scaled);
    near(pixels.at(20, 10), [102, 51, 128, 255]);
    assert!(!wants);
    surface
        .bind(
            &inputs(
                None,
                &[
                    ("resolution", numbers(&[50., 25.])),
                    ("devicePixelRatio", Value::Number(4.)),
                ],
            ),
            None,
        )
        .unwrap();
    near(
        draw(&gpu, surface.as_mut(), &scaled).0.at(20, 10),
        [128, 64, 255, 255],
    );
}

const TIME: &str = "struct U { time: f32 }
    @group(0) @binding(0) var<uniform> u: U;
    @fragment fn clock() -> @location(0) vec4f { return vec4f(u.time / 4., 0., 0., 0.); }";

#[test]
fn time_runs_on_the_host_clock_and_pause_excludes_held_time() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = load(TIME, None, &[]);
    for (now, red) in [(0., 0), (1000., 64), (2500., 159)] {
        let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(now));
        near(pixels.at(8, 6), [red, 0, 0, 255]);
        assert!(wants);
    }
    surface
        .bind(&inputs(Some("paused"), &[]), Some(3000.))
        .unwrap();
    for now in [3000., 10000.] {
        let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(now));
        near(pixels.at(8, 6), [191, 0, 0, 255]);
        assert!(!wants);
    }
    surface
        .bind(&inputs(Some("running"), &[]), Some(12000.))
        .unwrap();
    near(
        draw(&gpu, surface.as_mut(), &frame(13000.)).0.at(8, 6),
        [255, 0, 0, 255],
    );
}

#[test]
fn a_new_source_keeps_the_adopted_uniforms_and_clock_until_ready() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = load(TIME, None, &[]);
    draw(&gpu, surface.as_mut(), &frame(0.));
    let mut next = inputs(None, &[("tint", Value::str("#336699"))]);
    next[0] = Value::str("assets/next.wgsl");
    surface.bind(&next, Some(1000.)).unwrap();
    assert_eq!(surface.generation(), 2);
    let (pixels, wants) = fixture::render(&gpu, surface.as_mut(), &frame(2000.)).unwrap();
    assert!(!surface.frame_ready());
    near(pixels.at(8, 6), [128, 0, 0, 255]);
    assert!(wants);
    surface.asset("test.wgsl", Err(AssetError::Missing));
    assert!(surface.take_error().is_none());
    surface.asset(
        "next.wgsl",
        Ok(b"struct U { tint: vec4f }
        @group(0) @binding(0) var<uniform> u: U;
        @fragment fn solid() -> @location(0) vec4f { return u.tint; }"),
    );
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(3000.));
    near(pixels.at(8, 6), [51, 102, 153, 255]);
    assert!(!wants);
}

#[test]
fn reduced_motion_holds_time_unless_running_is_written_and_bound_time_rests() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = load(TIME, None, &[]);
    surface.reduced_motion(true);
    for now in [10000., 11000.] {
        let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(now));
        assert_eq!(pixels.at(8, 6), [0, 0, 0, 255]);
        assert!(!wants);
    }
    surface
        .bind(&inputs(Some("running"), &[]), Some(12000.))
        .unwrap();
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(13000.));
    near(pixels.at(8, 6), [64, 0, 0, 255]);
    assert!(wants);
    surface.bind(&inputs(None, &[]), Some(13000.)).unwrap();
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(15000.));
    near(pixels.at(8, 6), [64, 0, 0, 255]);
    assert!(!wants);
    surface.reduced_motion(false);
    let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(16000.));
    near(pixels.at(8, 6), [64, 0, 0, 255]);
    assert!(wants);
    near(
        draw(&gpu, surface.as_mut(), &frame(17000.)).0.at(8, 6),
        [128, 0, 0, 255],
    );
    surface
        .bind(
            &inputs(Some("running"), &[("time", Value::Number(2.5))]),
            Some(17000.),
        )
        .unwrap();
    for now in [17000., 20000.] {
        let (pixels, wants) = draw(&gpu, surface.as_mut(), &frame(now));
        near(pixels.at(8, 6), [159, 0, 0, 255]);
        assert!(!wants);
    }
}

#[test]
fn empty_missing_and_invalid_sources_leave_the_first_picture_unready() {
    let mut surface = exact_shader::surface();
    surface.bind(&[], None).unwrap();
    assert!(surface.assets().requests.is_empty());
    assert!(!surface.frame_ready());
    for src in [
        "https://example.com/a.wgsl",
        "app:/a.wgsl",
        "assets/../a.wgsl",
        "assets/",
    ] {
        assert!(surface
            .bind(&[Value::str(src)], None)
            .unwrap_err()
            .0
            .contains("src"));
    }
    surface.bind(&inputs(None, &[]), None).unwrap();
    surface.asset("stale.wgsl", Err(AssetError::Missing));
    assert!(surface.take_error().is_none());
    surface.asset("test.wgsl", Err(AssetError::Missing));
    assert!(surface.take_error().unwrap().0.contains("assets/test.wgsl"));
    assert!(!surface.frame_ready());
    assert!(surface.published().unwrap().contains("could not load"));
    assert!(surface.published().is_none());
    surface
        .bind(&[Value::str("assets/next.wgsl")], None)
        .unwrap();
    let changes = surface.assets();
    assert_eq!(changes.retired, ["test.wgsl"]);
    assert_eq!(changes.requests, ["test.wgsl", "next.wgsl"]);
    assert_eq!(surface.generation(), 2);
    surface.asset(
        "next.wgsl",
        Ok(b"@fragment fn nope() -> @location(0) vec4f { return wrong; }"),
    );
    assert!(surface.take_error().unwrap().0.contains("wrong"));
    let record = surface.published().unwrap();
    assert!(record.contains("next.wgsl:1:"), "{record}");
    assert!(!record.contains("\\n"), "{record}");
    assert!(!surface.frame_ready());
}

#[test]
fn pipeline_validation_reports_a_first_shader_failure_without_drawing() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = load(
        "@fragment fn input(@location(0) missing: vec4f) -> @location(0) vec4f { return missing; }",
        None,
        &[],
    );
    surface.prepare_assets(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm);
    let error = surface.take_error().unwrap().0;
    assert!(error.contains("assets/test.wgsl"), "{error}");
    assert!(
        error.contains("Location[0]") || error.contains("location"),
        "{error}"
    );
    assert!(!surface.frame_ready());
    assert!(!surface.preparing());
    assert!(surface.take_error().is_none());
    assert!(
        !fixture::render(&gpu, surface.as_mut(), &frame(0.))
            .unwrap()
            .1
    );
}

#[test]
fn an_undrawn_shader_clears_a_reused_target_to_show_the_canvas_styles() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("reused shader target"),
        size: wgpu::Extent3d {
            width: 16,
            height: 12,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());
    for (name, source) in [
        ("empty", None),
        ("awaiting the first asset", None),
        ("failed parse", Some("@fragment fn broken() { return wrong; }")),
        (
            "invalid uniform",
            Some("struct U { red: vec2f } @group(0) @binding(0) var<uniform> u: U;
                @fragment fn color() -> @location(0) vec4f { return vec4f(u.red, 0., 1.); }"),
        ),
        (
            "failed pipeline",
            Some("@fragment fn input(@location(0) missing: vec4f) -> @location(0) vec4f { return missing; }"),
        ),
    ] {
        let mut surface = exact_shader::surface();
        if name != "empty" {
            let values = if name == "invalid uniform" {
                vec![("red", Value::Number(1.))]
            } else {
                vec![]
            };
            surface.bind(&inputs(None, &values), None).unwrap();
        }
        if let Some(source) = source {
            surface.asset("test.wgsl", Ok(source.as_bytes()));
        }
        for _ in 0..2 {
            let mut encoder = gpu.device.create_command_encoder(&Default::default());
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 1.,
                            g: 0.,
                            b: 1.,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            assert!(!surface.render(
                &frame(0.), &gpu.device, &gpu.queue, &mut encoder, &view, texture.format(),
            ));
            gpu.queue.submit([encoder.finish()]);
            assert_eq!(fixture::read(&gpu, &texture).unwrap().at(8, 6), [0; 4], "{name}");
            assert!(!surface.frame_ready(), "{name}");
            assert!(!surface.frame_presentable(), "{name}");
        }
    }
}

#[test]
fn a_retained_animation_is_presentable_while_its_replacement_is_not_ready() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let mut surface = load(TIME, None, &[]);
    assert!(!surface.frame_presentable());
    draw(&gpu, surface.as_mut(), &frame(0.));
    let mut next = inputs(None, &[]);
    next[0] = Value::str("assets/next.wgsl");
    surface.bind(&next, Some(0.)).unwrap();
    for (now, red) in [(1000., 64), (2000., 128)] {
        let (pixels, wants) = fixture::render(&gpu, surface.as_mut(), &frame(now)).unwrap();
        near(pixels.at(8, 6), [red, 0, 0, 255]);
        assert!(wants);
        assert!(surface.frame_presentable());
        assert!(!surface.frame_ready());
    }
    surface.device_lost();
    assert!(!surface.frame_presentable());
    fixture::render(&gpu, surface.as_mut(), &frame(2000.)).unwrap();
    assert!(surface.frame_presentable());
    assert!(!surface.frame_ready());
    surface.bind(&[Value::str("")], Some(2000.)).unwrap();
    assert!(surface.is_empty());
    assert!(!surface.frame_presentable());
    surface.bind(&next, Some(2000.)).unwrap();
    assert!(!surface.frame_presentable());
    surface.asset("next.wgsl", Ok(TIME.as_bytes()));
    draw(&gpu, surface.as_mut(), &frame(3000.));
    assert!(surface.frame_presentable());
}
