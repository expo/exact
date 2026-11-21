use std::path::PathBuf;

struct App(PathBuf);

impl App {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target")
            .join(format!(
                "shader-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        let mut app = Self(path);
        app.write(
            "package.json",
            r#"{"dependencies":{"@fixture/fragment":"1.0.0"}}"#,
        );
        app.write(
            "node_modules/@fixture/fragment/package.json",
            include_str!("../shader/package.json"),
        );
        app.write(
            "node_modules/@fixture/fragment/exact.json",
            include_str!("../shader/exact.json"),
        );
        app.write("assets/plain.wgsl", include_str!("../shader/plain.wgsl"));
        app.0 = app.0.canonicalize().unwrap();
        app
    }

    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn compile(&self, call: &str) -> Result<exact_plan::Plan, contract::CompileError> {
        self.write(
            "app.contract",
            &format!("component App\n  view\n    canvas surface={call}\n"),
        );
        contract::compile_path(&self.0.join("app.contract"))
    }
}

impl Drop for App {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn literal_sources_are_checked() {
    let app = App::new();
    app.compile("fragment(src=\"assets/plain.wgsl\")").unwrap();
    let error = app
        .compile("fragment(src=\"assets/missing.wgsl\")")
        .unwrap_err();
    assert_eq!(error.id, "analyze-surface-shader", "{error}");
    assert_eq!(
        error.file.as_deref(),
        Some(app.0.join("app.contract").as_path())
    );
    assert_eq!((error.span.line, error.span.col), (3, 5));
    assert!(error.message.contains("assets/missing.wgsl"), "{error}");
    for src in [
        "plain.wgsl",
        "https://example.org/plain.wgsl",
        "assets/../plain.wgsl",
    ] {
        let error = app.compile(&format!("fragment(src={src:?})")).unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert_eq!((error.span.line, error.span.col), (3, 5));
        assert!(error.message.contains(src), "{error}");
    }
    app.compile("fragment(\"assets/plain.wgsl\")").unwrap();
}

#[test]
fn naga_errors_name_the_shader_file_and_original_position() {
    let app = App::new();
    for (source, position) in [
        ("// broken\n@fragment fn picture() -> @location(0) vec4f {\n  return missing;\n}\n", (3, 10)),
        ("// invalid return type\n@fragment fn picture() -> @location(0) vec4f {\n  return 1.0;\n}\n", (3, 10)),
    ] {
        app.write("assets/bad.wgsl", source);
        let error = app.compile("fragment(src=\"assets/bad.wgsl\")").unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert_eq!(error.file.as_deref(), Some(app.0.join("assets/bad.wgsl").as_path()));
        assert_eq!((error.span.line, error.span.col), position, "{error}");
        let json: serde_json::Value = serde_json::from_str(&error.to_json()).unwrap();
        assert_eq!(json["line"], position.0);
        assert_eq!(json["col"], position.1);
        assert!(contract::excerpt(&error).contains(source.lines().nth(2).unwrap()));
    }
}

#[test]
fn core_fragment_inputs_are_accepted() {
    let app = App::new();
    app.write(
        "assets/core.wgsl",
        "@fragment fn picture(@builtin(sample_index) sample: u32) -> @location(0) vec4f { return vec4f(f32(sample), 0, 0, 1); }\n",
    );
    app.compile("fragment(src=\"assets/core.wgsl\")").unwrap();
}

#[test]
fn core_half_precision_builtins_are_accepted_without_f16() {
    let app = App::new();
    app.write(
        "assets/core.wgsl",
        "@fragment fn picture(@builtin(position) at: vec4f) -> @location(0) vec4f {\n  let packed = pack2x16float(at.xy);\n  let unpacked = unpack2x16float(packed);\n  return vec4f(unpacked, quantizeToF16(at.z), 1);\n}\n",
    );
    app.compile("fragment(src=\"assets/core.wgsl\")").unwrap();
}

fn optional_feature_is_refused(source: &str, extension: &str) {
    use exact_gpu_reflect::naga;
    let module = naga::front::wgsl::parse_str(source).unwrap();
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap();
    let app = App::new();
    app.write("assets/optional.wgsl", source);
    let error = app
        .compile("fragment(src=\"assets/optional.wgsl\")")
        .unwrap_err();
    assert_eq!(error.id, "analyze-surface-shader", "{error}");
    assert_eq!(
        error.file.as_deref(),
        Some(app.0.join("assets/optional.wgsl").as_path())
    );
    assert_eq!((error.span.line, error.span.col), (1, 8), "{error}");
    assert!(
        error
            .message
            .contains(&format!("`{extension}` extension is not supported")),
        "{error}"
    );
}

#[test]
fn optional_f16_is_refused() {
    optional_feature_is_refused(
        "enable f16;\n@fragment fn picture() -> @location(0) vec4f { return vec4f(f32(1h)); }\n",
        "f16",
    );
}

#[test]
fn optional_clip_distances_are_refused() {
    optional_feature_is_refused(
        "enable clip_distances;\nstruct Output { @builtin(position) at: vec4f, @builtin(clip_distances) clip: array<f32, 1>, }\n@vertex fn geometry() -> Output { return Output(vec4f(0), array<f32, 1>(1)); }\n@fragment fn picture() -> @location(0) vec4f { return vec4f(1); }\n",
        "clip_distances",
    );
}

#[test]
fn optional_dual_source_blending_is_refused() {
    optional_feature_is_refused(
        "enable dual_source_blending;\nstruct Output { @location(0) @blend_src(0) color: vec4f, @location(0) @blend_src(1) weight: vec4f, }\n@fragment fn picture() -> Output { return Output(vec4f(1), vec4f(0)); }\n",
        "dual_source_blending",
    );
}

#[test]
fn string_uniform_errors_name_the_declared_type() {
    let app = App::new();
    for (ty, expected) in [
        ("f32", "expected a finite number in the f32 range"),
        ("i32", "expected a whole number (or a bool for u32)"),
        ("u32", "expected a whole number (or a bool for u32)"),
        ("vec2f", "expected a list of 2 numbers"),
        ("vec3f", "expected a list of 3 numbers"),
    ] {
        app.write("assets/value.wgsl", &format!("struct U {{ value: {ty}, }}\n@group(0) @binding(0) var<uniform> u: U;\n@fragment fn picture() -> @location(0) vec4f {{ return vec4f(1); }}\n"));
        for value in ["fast", "red"] {
            let error = app
                .compile(&format!(
                    "fragment(src=\"assets/value.wgsl\", value={value:?})"
                ))
                .unwrap_err();
            assert_eq!(error.id, "analyze-surface-shader", "{error}");
            assert_eq!(
                error.message,
                format!("uniform argument `value`: {expected}"),
                "{ty}: {error}"
            );
            assert_eq!(
                error.file.as_deref(),
                Some(app.0.join("app.contract").as_path())
            );
            assert_eq!((error.span.line, error.span.col), (3, 5), "{error}");
        }
    }
}

#[test]
fn a_fragment_pipeline_needs_defaults_for_the_overrides_it_uses() {
    let app = App::new();
    let call = r#"fragment(src="assets/override.wgsl")"#;
    let write = |initializer: &str, value: &str| {
        app.write(
            "assets/override.wgsl",
            &format!("override unused: f32;\n@id(7) override opacity: f32{initializer};\nfn alpha() -> f32 {{ return opacity; }}\n@fragment fn picture() -> @location(0) vec4f {{ return vec4f({value}); }}"),
        )
    };
    write("", "1");
    app.compile(call).unwrap();
    write(" = 1", "alpha()");
    app.compile(call).unwrap();
    write("", "alpha()");
    let error = app.compile(call).unwrap_err();
    assert_eq!(error.id, "analyze-surface-shader", "{error}");
    assert_eq!(
        error.file.as_deref(),
        Some(app.0.join("assets/override.wgsl").as_path())
    );
    assert_eq!(error.span.line, 2, "{error}");
    assert!(
        error.message.contains("opacity") && error.message.contains("default"),
        "{error}"
    );
}

#[test]
fn shader_declarations_follow_the_fragment_and_uniform_rules() {
    let app = App::new();
    let fragment = "@fragment fn picture() -> @location(0) vec4f { return vec4f(1); }\n";
    let cases = [
        ("@compute @workgroup_size(1) fn calculate() {}\n".to_string(), "calculate"),
        ("@vertex fn geometry() -> @builtin(position) vec4f { return vec4f(0); }\n".into(), "geometry"),
        ("fn helper() {}\n".into(), "@fragment"),
        ("@fragment fn picture(@location(0) uv: vec2f) -> @location(0) vec4f { return vec4f(uv, 0, 1); }\n".into(), "uv"),
        ("struct Input { @location(0) uv: vec2f, }\n@fragment fn picture(input: Input) -> @location(0) vec4f { return vec4f(input.uv, 0, 1); }\n".into(), "input"),
        ("@fragment fn picture() -> @location(1) vec4f { return vec4f(1); }\n".into(), "@location(0) vec4f"),
        ("@fragment fn picture() -> @location(0) vec3f { return vec3f(1); }\n".into(), "@location(0) vec4f"),
        (format!("@group(0) @binding(0) var image: texture_2d<f32>;\n{fragment}"), "image"),
        (format!("struct U {{ speed: f32, }}\n@group(1) @binding(0) var<uniform> u: U;\n{fragment}"), "u"),
        (format!("struct U {{ speed: f32, }}\n@group(0) @binding(1) var<uniform> u: U;\n{fragment}"), "u"),
    ];
    for (source, expected) in cases {
        app.write("assets/rule.wgsl", &source);
        let error = app
            .compile("fragment(src=\"assets/rule.wgsl\")")
            .unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert_eq!(
            error.file.as_deref(),
            Some(app.0.join("assets/rule.wgsl").as_path())
        );
        assert!(error.span.line > 0 && error.span.col > 0, "{error}");
        assert!(error.message.contains(expected), "{error}");
    }
    for extra in [vec!["other"], vec!["other", "third"]] {
        let mut source = fragment.to_string();
        for name in &extra {
            source.push_str(&format!(
                "@fragment fn {name}() -> @location(0) vec4f {{ return vec4f(0); }}\n"
            ));
        }
        app.write("assets/rule.wgsl", &source);
        let error = app
            .compile("fragment(src=\"assets/rule.wgsl\")")
            .unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert_eq!(
            error.file.as_deref(),
            Some(app.0.join("assets/rule.wgsl").as_path())
        );
        assert_eq!((error.span.line, error.span.col), (2, 14), "{error}");
        for name in extra {
            assert!(error.message.contains(name), "{error}");
        }
    }
    for (declaration, expected) in [
        ("transform: mat4x4f", "transform"),
        ("weights: array<vec4f, 2>", "weights"),
        ("weights: array<f32, 2>", "weights"),
        ("enabled: bool", "enabled"),
        ("direction: vec2u", "direction"),
        ("@align(16) speed: f32", "speed"),
        ("@size(16) speed: f32", "speed"),
        ("speed: Amount", "speed"),
        ("nested: Other", "nested"),
        ("src: f32", "src"),
        ("maxFps: f32", "maxFps"),
        ("animationPlayState: f32", "animationPlayState"),
        ("time: u32", "time"),
        ("resolution: vec3f", "resolution"),
        ("devicePixelRatio: vec2f", "devicePixelRatio"),
    ] {
        let source = format!("alias Amount = f32;\nstruct Other {{ a: f32, }}\nstruct U {{\n  {declaration},\n}}\n@group(0) @binding(0) var<uniform> u: U;\n{fragment}");
        app.write("assets/rule.wgsl", &source);
        let error = app
            .compile("fragment(src=\"assets/rule.wgsl\")")
            .unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert!(error.message.contains(expected), "{error}");
        assert_eq!(
            (error.span.line, error.span.col),
            (4, 3 + declaration.find(expected).unwrap() as u32),
            "{error}"
        );
    }
    app.write("assets/rule.wgsl", fragment);
    app.compile("fragment(src=\"assets/rule.wgsl\")").unwrap();
    app.write("assets/rule.wgsl", "struct Input { @builtin(position) at: vec4f, }\nstruct Output { @location(0) color: vec4f, }\n@fragment fn picture(input: Input) -> Output { return Output(input.at); }\n");
    app.compile("fragment(src=\"assets/rule.wgsl\")").unwrap();
}

#[test]
fn uniform_names_and_literal_values_are_checked() {
    let app = App::new();
    app.compile("fragment(src=\"assets/plain.wgsl\", speed=2, tint=\"color(display-p3 1 0 0)\", maxFps=30, animationPlayState=\"paused\")").unwrap();
    app.compile("fragment(src=\"assets/plain.wgsl\", tint=[0.1, 0.2, 0.3, 1])")
        .unwrap();
    for argument in [
        "speeed=1",
        "speed=true",
        "tint=[1, 2]",
        "tint=\"CanvasText\"",
        "tint=\"currentColor\"",
        "tint=\"not a colour\"",
    ] {
        let error = app
            .compile(&format!("fragment(src=\"assets/plain.wgsl\", {argument})"))
            .unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert_eq!(
            error.file.as_deref(),
            Some(app.0.join("app.contract").as_path())
        );
        assert_eq!((error.span.line, error.span.col), (3, 5));
        assert!(
            error.message.contains(argument.split('=').next().unwrap()),
            "{error}"
        );
        if argument.starts_with("speeed") {
            assert!(error.message.contains("speed, tint"), "{error}");
        }
    }
    app.write("assets/integers.wgsl", "struct U { signed: i32, enabled: u32, }\n@group(0) @binding(0) var<uniform> u: U;\n@fragment fn picture() -> @location(0) vec4f { return vec4f(f32(u.signed + i32(u.enabled))); }\n");
    app.compile("fragment(src=\"assets/integers.wgsl\", signed=-2147483648, enabled=true)")
        .unwrap();
    for argument in [
        "signed=1.5",
        "signed=2147483648",
        "enabled=-1",
        "enabled=4294967296",
    ] {
        let error = app
            .compile(&format!(
                "fragment(src=\"assets/integers.wgsl\", {argument})"
            ))
            .unwrap_err();
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
    }
}

#[test]
fn computed_sources_values_and_unmarked_surfaces_are_left_to_binding() {
    let app = App::new();
    app.compile("unchecked(src=\"assets/missing.wgsl\", unknown=1)")
        .unwrap();
    app.compile("fragment()").unwrap();
    app.write("app.contract", "component App\n  state file = \"assets/missing.wgsl\"\n  state value = \"computed\"\n  view\n    column\n      canvas surface=fragment(src=file, unknown=value)\n      canvas surface=fragment(src=\"assets/plain.wgsl\", speed=value)\ncomponent Unused\n  view\n    canvas surface=fragment(src=\"assets/missing.wgsl\")\n");
    contract::compile_path(&app.0.join("app.contract")).unwrap();
    app.write("app.contract", "component App\n  state value = 1\n  view\n    canvas surface=fragment(src=\"assets/plain.wgsl\", unknown=value)\n");
    let error = contract::compile_path(&app.0.join("app.contract")).unwrap_err();
    assert_eq!(error.id, "analyze-surface-shader", "{error}");
    assert!(error.message.contains("unknown"), "{error}");
}

#[test]
fn shader_findings_keep_imported_canvas_locations_and_collect_independent_errors() {
    let app = App::new();
    app.write("panel.contract", "component Panel\n  view\n    column\n      canvas surface=fragment(src=\"assets/missing.wgsl\")\n      canvas surface=fragment(src=\"assets/plain.wgsl\", unknown=1)\n");
    app.write(
        "app.contract",
        "use Panel from \"./panel.contract\"\ncomponent App\n  view\n    Panel()\n",
    );
    for mapped in [false, true] {
        let errors = contract::compile_path_all(&app.0.join("app.contract"), mapped)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 2, "{errors:?}");
        for (error, line) in errors.iter().zip([4, 5]) {
            assert_eq!(error.id, "analyze-surface-shader", "{error}");
            assert_eq!(
                error.file.as_deref(),
                Some(app.0.join("panel.contract").as_path())
            );
            assert_eq!((error.span.line, error.span.col), (line, 7), "{error}");
        }
    }
    app.write("app.contract", "component App\n  view\n    canvas surface=fragment(src=\"assets/plain.wgsl\", first=1, second=2, speed=true)\n");
    let errors = contract::compile_path_all(&app.0.join("app.contract"), false)
        .err()
        .unwrap();
    assert_eq!(errors.len(), 3, "{errors:?}");
    for (error, name) in errors.iter().zip(["first", "second", "speed"]) {
        assert_eq!(error.id, "analyze-surface-shader", "{error}");
        assert!(error.message.contains(name), "{error}");
    }
}

#[test]
fn the_manifest_names_the_shader_source_argument() {
    let app = App::new();
    let mut declaration: serde_json::Value =
        serde_json::from_str(include_str!("../shader/exact.json")).unwrap();
    let path = "node_modules/@fixture/fragment/exact.json";
    declaration["gpu"]["surfaces"]["fragment"]["wgsl"] = serde_json::json!("file");
    app.write(path, &declaration.to_string());
    app.compile("fragment(file=\"assets/plain.wgsl\", speed=1)")
        .unwrap();
    let error = app
        .compile("fragment(file=\"assets/missing.wgsl\")")
        .unwrap_err();
    assert_eq!(error.id, "analyze-surface-shader", "{error}");
    for value in [
        serde_json::json!(""),
        serde_json::json!(1),
        serde_json::json!("not-an-argument"),
    ] {
        declaration["gpu"]["surfaces"]["fragment"]["wgsl"] = value;
        app.write(path, &declaration.to_string());
        let error = app.compile("fragment()").unwrap_err();
        assert_eq!(error.id, "bake-gpu-declaration", "{error}");
        assert!(
            error.message.contains("wgsl must name an argument"),
            "{error}"
        );
    }
}

#[test]
fn uniform_reader_matches_naga_for_all_blocks_through_four_members() {
    let types = ["f32", "i32", "u32", "vec2f", "vec3f", "vec4f"];
    let mut sources = Vec::new();
    for count in 1..=4 {
        for mut code in 0..6_usize.pow(count) {
            let mut source = String::from("struct U {\n");
            for i in 0..count {
                source.push_str(&format!("  m{i}: {},\n", types[code % 6]));
                code /= 6;
            }
            source.push_str("}\n@group(0) @binding(0) var<uniform> u: U;\n@fragment fn picture() -> @location(0) vec4f { return vec4f(1); }\n");
            sources.push(source);
        }
    }
    assert_eq!(sources.len(), 1554);
    let workers = std::thread::available_parallelism().map_or(1, usize::from);
    std::thread::scope(|scope| {
        for batch in sources.chunks(sources.len().div_ceil(workers)) {
            scope.spawn(move || {
                for source in batch {
                    let layout = exact_shader_uniforms::parse(source).unwrap();
                    let module = exact_gpu_reflect::inspect(source).unwrap();
                    let (_, global) = module.global_variables.iter().next().unwrap();
                    let exact_gpu_reflect::naga::TypeInner::Struct { members, span } =
                        &module.types[global.ty].inner
                    else {
                        panic!("{source}")
                    };
                    assert_eq!(layout.size, *span as usize, "{source}");
                    assert_eq!(layout.members.len(), members.len(), "{source}");
                    for (actual, oracle) in layout.members.iter().zip(members) {
                        assert_eq!(actual.name, oracle.name.as_deref().unwrap(), "{source}");
                        assert_eq!(actual.offset, oracle.offset as usize, "{source}");
                    }
                }
            });
        }
    });
}
