use exact_shader_uniforms::{parse, Builtins, UniformType, Value, ARGUMENTS};

fn block(members: &str) -> String {
    format!("struct Uniforms {{ {members} }}\n@group(0) @binding(0) var<uniform> u: Uniforms;")
}

#[test]
fn wgsl_layout_and_padding() {
    for (ty, alignment, size) in [
        ("f32", 4usize, 4usize),
        ("i32", 4, 4),
        ("u32", 4, 4),
        ("vec2f", 8, 8),
        ("vec3f", 16, 12),
        ("vec4f", 16, 16),
    ] {
        let layout = parse(&block(&format!("value: {ty},"))).unwrap();
        assert_eq!(layout.members[0].offset, 0, "{ty}");
        assert_eq!(layout.members[0].ty.alignment(), alignment, "{ty}");
        assert_eq!(layout.members[0].ty.size(), size, "{ty}");
        assert_eq!(layout.alignment, alignment, "{ty}");
        assert_eq!(layout.size, size.next_multiple_of(alignment), "{ty}");
    }
    for (members, offsets, size) in [
        (
            "time: f32, resolution: vec2f, speed: f32, tint: vec4f,",
            vec![0, 8, 16, 32],
            48,
        ),
        ("a: vec3f, b: f32, c: vec2f,", vec![0, 12, 16], 32),
        ("a: f32, b: vec3f, c: vec3f,", vec![0, 16, 32], 48),
        ("a: i32, b: u32, c: f32", vec![0, 4, 8], 12),
    ] {
        let layout = parse(&block(members)).unwrap();
        assert_eq!(layout.size, size);
        assert_eq!(
            layout.members.iter().map(|m| m.offset).collect::<Vec<_>>(),
            offsets
        );
    }
    for (short, generic) in [
        ("vec2f", "vec2<f32>"),
        ("vec3f", "vec3<f32>"),
        ("vec4f", "vec4<f32>"),
    ] {
        let short = parse(&block(&format!("value: {short},"))).unwrap();
        let generic = parse(&block(&format!("value: {generic},"))).unwrap();
        assert_eq!(short.members[0].ty, generic.members[0].ty);
        assert_eq!(short.size, generic.size);
    }
}

#[test]
fn wgsl_blankspace_separates_uniform_declarations() {
    for space in [
        "\u{200e}", "\u{200f}", " ", "\t", "\n", "\u{b}", "\u{c}", "\r", "\u{85}", "\u{2028}",
        "\u{2029}",
    ] {
        for declaration in [
            format!("var{space}<uniform> u: U;"),
            format!("var<uniform{space}> u: U;"),
        ] {
            let source = format!("struct U {{ time: f32 }} {declaration}");
            let layout = parse(&source).unwrap();
            assert_eq!(layout.name.as_deref(), Some("U"), "{source:?}");
            assert_eq!(layout.size, 4, "{source:?}");
        }
    }
}

#[test]
fn wgsl_direction_marks_separate_member_names_and_types() {
    for space in ['\u{200e}', '\u{200f}'] {
        for members in [
            format!("time{space}: f32, speed: f32"),
            format!("time: f32, speed:{space}f32"),
        ] {
            let layout = parse(&block(&members)).unwrap();
            let bytes = layout
                .pack(
                    &[("speed", Value::Number(3.0))],
                    Builtins {
                        time: 2.5,
                        ..Builtins::default()
                    },
                )
                .unwrap();
            assert_eq!(floats(&bytes), [2.5, 3.0], "{members:?}");
            assert_eq!(layout.members[0].name, "time");
            assert_eq!(layout.members[1].name, "speed");
        }
    }
}

#[test]
fn line_comments_end_at_every_wgsl_line_break() {
    for ending in [
        "\u{2028}", "\u{2029}", "\u{85}", "\u{b}", "\u{c}", "\r", "\n", "\r\n",
    ] {
        let source = block(&format!("a: f32, // café{ending}  b: f32,\n  c: f32,"));
        let layout = parse(&source).unwrap();
        assert_eq!(
            layout
                .members
                .iter()
                .map(|m| (m.name.as_str(), m.offset))
                .collect::<Vec<_>>(),
            [("a", 0), ("b", 4), ("c", 8)],
            "{ending:?}"
        );
        assert_eq!(layout.size, 12, "{ending:?}");
        let at = layout.members[1].location;
        assert_eq!(at.offset, source.find("b: f32").unwrap());
        assert_eq!((at.line, at.column), (2, 3), "{ending:?}");
        let at = layout.members[2].location;
        assert_eq!((at.line, at.column), (3, 3), "{ending:?}");

        let source = format!(
            "// heading\r\n{}",
            block(&format!("a: f32, // comment{ending}  bad: bool,"))
        );
        let error = parse(&source).unwrap_err();
        assert!(error.message.contains("bad"));
        let at = error.location.unwrap();
        assert_eq!(at.offset, source.find("bad: bool").unwrap());
        assert_eq!((at.line, at.column), (3, 3), "{ending:?}");
    }
}

#[test]
fn uniform_address_spaces_accept_a_trailing_comma() {
    for address in [
        "uniform",
        "uniform,",
        "uniform /* address */, /* trailing */",
    ] {
        let source =
            format!("struct U {{ a: f32, b: f32 }} @group(0) @binding(0) var<{address}> u: U;");
        let layout = parse(&source).unwrap();
        assert_eq!(layout.name.as_deref(), Some("U"));
        assert_eq!(layout.size, 8);
        assert_eq!(layout.members[1].offset, 4);
    }
}

#[test]
fn vector_templates_accept_a_trailing_comma() {
    for (vector, ty) in [
        ("vec2", UniformType::Vec2f),
        ("vec3", UniformType::Vec3f),
        ("vec4", UniformType::Vec4f),
    ] {
        for arguments in ["f32", "f32,", "f32 /* element */, /* trailing */"] {
            let layout =
                parse(&block(&format!("value: {vector}<{arguments}>, tail: f32"))).unwrap();
            assert_eq!(layout.members[0].ty, ty);
            assert_eq!(layout.members[1].offset, ty.size());
            assert_eq!(
                layout.size,
                (ty.size() + 4).next_multiple_of(ty.alignment())
            );
        }
    }
}

#[test]
fn non_layout_member_attributes_preserve_layout_and_values() {
    let source =
        block("@location(0) @interpolate(flat) tint: vec4f, @builtin(frag_depth) depth: f32,")
            + "\n@fragment fn main() -> @location(0) vec4f { return u.tint; }";
    let layout = parse(&source).unwrap();
    assert_eq!(layout.members[0].name, "tint");
    assert_eq!(layout.members[0].offset, 0);
    assert_eq!(layout.members[1].name, "depth");
    assert_eq!(layout.members[1].offset, 16);
    assert_eq!(layout.size, 32);
    let bytes = layout
        .pack(
            &[
                ("tint", Value::Color([1.0, 0.0, 0.0, 1.0])),
                ("depth", Value::Number(0.5)),
            ],
            Builtins::default(),
        )
        .unwrap();
    assert_eq!(floats(&bytes), [1.0, 0.0, 0.0, 1.0, 0.5, 0.0, 0.0, 0.0]);
    for attribute in ["align", "size"] {
        let error = parse(&block(&format!(
            "@location(0) @{attribute}(16) tint: vec4f,"
        )))
        .unwrap_err();
        assert!(error.message.contains(&format!("@{attribute}")), "{error}");
        assert!(error.message.contains("tint"), "{error}");
    }
}

#[test]
fn package_compiles_without_checkout_files() {
    use std::{fs, path::Path, process::Command};

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = crate_dir.join(format!("target/relocation-{}", std::process::id()));
    let installed = root.join("app/node_modules/@exact/shader/uniforms");
    copy_dir(&crate_dir.join("src"), &installed.join("src"));
    fs::copy(crate_dir.join("Cargo.toml"), installed.join("Cargo.toml")).unwrap();
    let output = Command::new(env!("CARGO"))
        .args(["check", "--offline", "--quiet", "--target-dir"])
        .arg(root.join("target"))
        .current_dir(&installed)
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn finds_the_bound_struct_among_comments_and_other_declarations() {
    let source = "// struct Fake { x: bool } var<uniform> f: Fake;\n\
        /* outer /* } var<uniform> fake: Bad; */ comment */\n\
        struct Other { @size(32) unused: mat4x4f, }\n\
        @binding(0) @group(0) var /* comment */ < uniform > data: Chosen;\n\
        fn helper() { var local: f32; if true { local = 2; } }\n\
        struct Chosen { a: vec2 < f32 >, b: vec3<f32>, c: vec4f, café: f32, }";
    let layout = parse(source).unwrap();
    assert_eq!(layout.name.as_deref(), Some("Chosen"));
    assert_eq!(layout.size, 64);
    assert_eq!(layout.members[1].ty, UniformType::Vec3f);
    assert_eq!(layout.members[3].name, "café");
    assert_eq!(layout.members[0].location.line, 6);
    let empty = parse("struct Unused { x: bool } fn f() { var x: u32; }").unwrap();
    assert!(empty.members.is_empty());
    assert_eq!(empty.size, 0);
    assert!(empty.pack(&[], Builtins::default()).unwrap().is_empty());
    assert!(empty
        .pack(&[("unknown", Value::Number(1.0))], Builtins::default())
        .unwrap_err()
        .to_string()
        .contains("unknown"));
}

#[test]
fn unsupported_members_name_the_declaration() {
    for ty in [
        "bool",
        "f16",
        "vec2i",
        "vec4<u32>",
        "mat4x4f",
        "array<f32, 4>",
        "Other",
        "Alias",
    ] {
        let source = format!(
            "alias Alias = f32; struct Other {{ a: f32 }}\n{}",
            block(&format!("bad: {ty},"))
        );
        let error = parse(&source).unwrap_err();
        assert!(error.message.contains("bad"), "{ty}: {error}");
        assert_eq!(error.location.unwrap().line, 2);
    }
    for attribute in ["@align(16)", "@size(16)"] {
        let error = parse(&block(&format!("{attribute} bad: f32,"))).unwrap_err();
        assert!(error.message.contains("bad"), "{error}");
        assert!(error.message.contains(attribute.split('(').next().unwrap()));
    }
    for (declaration, ty) in [
        ("alias f32 = u32;", "f32"),
        ("alias f32 = u32;", "vec2<f32>"),
        ("alias vec2f = vec4f;", "vec2f"),
        ("struct vec2f { a: f32 }", "vec2f"),
    ] {
        let source = format!("{declaration} {}", block(&format!("bad: {ty},")));
        let error = parse(&source).unwrap_err();
        assert!(error.message.contains("bad"), "{error}");
    }
    for reserved in ARGUMENTS {
        let error = parse(&block(&format!("{reserved}: f32,"))).unwrap_err();
        assert!(error.message.contains(reserved), "{error}");
    }
    for (name, ty) in [
        ("time", "u32"),
        ("resolution", "vec3f"),
        ("devicePixelRatio", "vec2f"),
    ] {
        let error = parse(&block(&format!("{name}: {ty},"))).unwrap_err();
        assert!(error.message.contains(name), "{error}");
    }
}

#[test]
fn malformed_blocks_are_errors() {
    for source in [
        "/* unterminated",
        "struct U { x: f32",
        "struct U { x f32 } var<uniform> u: U;",
        "struct U { x: f32 y: f32 } var<uniform> u: U;",
        "struct U { x: f32, x: f32 } var<uniform> u: U;",
        "struct U {} var<uniform> u: U;",
        "var<uniform> u: Missing;",
        "alias U = f32; var<uniform> u: U;",
        "var<uniform> u: f32;",
        "struct U { x: f32 } var<uniform> u: U; var<uniform> v: U;",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
    let error = parse("// heading\nstruct U {\n  bad: bool,\n}\nvar<uniform> u: U;").unwrap_err();
    let at = error.location.unwrap();
    assert_eq!((at.line, at.column), (3, 3));
    assert_eq!(at.offset, 24);
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

#[test]
fn packs_values_by_name_in_little_endian_with_zero_padding() {
    let layout = parse(&block(
        "a: f32, b: i32, c: u32, d: vec2f, e: vec3f, f: vec4f, zero: f32,",
    ))
    .unwrap();
    let bytes = layout
        .pack(
            &[
                (
                    "f",
                    Value::List(&[
                        Value::Number(1.0),
                        Value::Number(2.0),
                        Value::Number(3.0),
                        Value::Number(4.0),
                    ]),
                ),
                (
                    "e",
                    Value::List(&[Value::Number(-1.0), Value::Number(0.5), Value::Number(2.0)]),
                ),
                (
                    "d",
                    Value::List(&[Value::Number(12.0), Value::Number(13.0)]),
                ),
                ("c", Value::Number(u32::MAX as f64)),
                ("b", Value::Number(i32::MIN as f64)),
                ("a", Value::Number(1.25)),
            ],
            Builtins::default(),
        )
        .unwrap();
    let mut expected = vec![0; 80];
    for (offset, value) in [
        (0, 1.25),
        (16, 12.0),
        (20, 13.0),
        (32, -1.0),
        (36, 0.5),
        (40, 2.0),
        (48, 1.0),
        (52, 2.0),
        (56, 3.0),
        (60, 4.0),
    ] {
        expected[offset..offset + 4].copy_from_slice(&f32::to_le_bytes(value));
    }
    expected[4..8].copy_from_slice(&i32::MIN.to_le_bytes());
    expected[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(bytes, expected);

    let layout = parse(&block("flag: u32, tint: vec4f,")).unwrap();
    let bytes = layout
        .pack(
            &[
                ("flag", Value::Bool(true)),
                (
                    "tint",
                    Value::Color([10.0 / 255.0, 132.0 / 255.0, 1.0, 128.0 / 255.0]),
                ),
            ],
            Builtins::default(),
        )
        .unwrap();
    assert_eq!(&bytes[..4], &1u32.to_le_bytes());
    assert_eq!(
        floats(&bytes[16..]),
        vec![10.0 / 255.0, 132.0 / 255.0, 1.0, 128.0 / 255.0]
    );
    assert_eq!(
        &layout
            .pack(&[("flag", Value::Bool(false))], Builtins::default())
            .unwrap()[..4],
        &[0; 4]
    );
}

#[test]
fn builtins_fill_only_unbound_members() {
    let layout = parse(&block(
        "time: f32, resolution: vec2f, devicePixelRatio: f32, other: f32,",
    ))
    .unwrap();
    let builtins = Builtins {
        time: 2.5,
        resolution: [800.0, 600.0],
        device_pixel_ratio: 2.0,
    };
    let bytes = layout.pack(&[], builtins).unwrap();
    assert_eq!(floats(&bytes), vec![2.5, 0.0, 800.0, 600.0, 2.0, 0.0]);
    let bytes = layout
        .pack(
            &[
                ("time", Value::Number(9.0)),
                (
                    "resolution",
                    Value::List(&[Value::Number(1.0), Value::Number(2.0)]),
                ),
                ("devicePixelRatio", Value::Number(1.0)),
            ],
            builtins,
        )
        .unwrap();
    assert_eq!(floats(&bytes), vec![9.0, 0.0, 1.0, 2.0, 1.0, 0.0]);
}

#[test]
fn invalid_values_name_the_argument_and_unknown_names_list_members() {
    for (ty, values) in [
        (
            "f32",
            vec![
                Value::Null,
                Value::Bool(true),
                Value::Number(f64::NAN),
                Value::Number(f64::INFINITY),
                Value::Number(f64::MAX),
            ],
        ),
        (
            "i32",
            vec![
                Value::Bool(true),
                Value::Number(1.5),
                Value::Number(i32::MIN as f64 - 1.0),
                Value::Number(i32::MAX as f64 + 1.0),
                Value::Number(f64::NAN),
            ],
        ),
        (
            "u32",
            vec![
                Value::Number(-1.0),
                Value::Number(0.5),
                Value::Number(u32::MAX as f64 + 1.0),
                Value::Number(f64::INFINITY),
            ],
        ),
        (
            "vec2f",
            vec![
                Value::Number(1.0),
                Value::List(&[Value::Number(1.0)]),
                Value::List(&[Value::Number(1.0), Value::Bool(false)]),
                Value::Color([1.0, 0.0, 0.0, 1.0]),
            ],
        ),
        (
            "vec3f",
            vec![
                Value::List(&[Value::Number(1.0), Value::Number(2.0)]),
                Value::Color([1.0, 0.0, 0.0, 1.0]),
            ],
        ),
        (
            "vec4f",
            vec![
                Value::String("AccentColor"),
                Value::List(&[
                    Value::Number(0.0),
                    Value::Number(0.0),
                    Value::Number(0.0),
                    Value::Number(f64::MAX),
                ]),
            ],
        ),
    ] {
        let layout = parse(&block(&format!("value: {ty},"))).unwrap();
        for value in values {
            let error = layout
                .pack(&[("value", value)], Builtins::default())
                .unwrap_err();
            assert!(error.message.contains("value"), "{ty}: {error}");
        }
    }
    let layout = parse(&block("speed: f32, tint: vec4f,")).unwrap();
    let error = layout
        .pack(&[("typo", Value::Number(1.0))], Builtins::default())
        .unwrap_err();
    for name in ["typo", "speed", "tint"] {
        assert!(error.message.contains(name), "{error}");
    }
}

#[test]
fn colours_pack_as_encoded_unpremultiplied_numbers() {
    let layout = parse(&block("tint: vec4f, tail: f32,")).unwrap();
    for rgba in [
        [10.0 / 255.0, 132.0 / 255.0, 1.0, 1.0],
        [0.25, 0.5, 0.75, 0.2],
        [1.0, 0.5, 0.25, 0.0],
    ] {
        let bytes = layout
            .pack(&[("tint", Value::Color(rgba))], Builtins::default())
            .unwrap();
        assert_eq!(&floats(&bytes)[..4], &rgba.map(|v| v as f32));
        assert_eq!(&bytes[16..], &[0; 16]);
    }
    for channel in 0..4 {
        for invalid in [-0.1, 1.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut rgba = [0.5; 4];
            rgba[channel] = invalid;
            let error = layout
                .pack(&[("tint", Value::Color(rgba))], Builtins::default())
                .unwrap_err();
            assert!(error.message.contains("tint"), "{error}");
        }
    }
    for source in ["red", "#0a84ff", "color(display-p3 1 0 0)"] {
        let error = layout
            .pack(&[("tint", Value::String(source))], Builtins::default())
            .unwrap_err();
        assert!(error.message.contains("tint"), "{error}");
    }
}
