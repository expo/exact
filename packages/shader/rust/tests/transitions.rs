use exact_gpu::{fixture, Frame, Module, Registry};

static REGISTRY: Registry = Registry {
    surfaces: &[("shader", 4, exact_shader::surface)],
    shaders: &[],
};

const RED: &str = "struct U { red: f32 }
    @group(0) @binding(0) var<uniform> u: U;
    @fragment fn color() -> @location(0) vec4f { return vec4f(u.red, 0., 0., 0.); }";
const BLUE: &str = "@fragment fn color() -> @location(0) vec4f { return vec4f(0., 0., 1., 0.); }";
const BROKEN: &str = "@fragment fn color() -> @location(0) vec4f { return wrong; }";

enum Event {
    Bind(&'static str, bool),
    Asset(&'static str, &'static str, bool),
    Read([u8; 4]),
}

#[test]
fn source_binding_and_picture_transitions_publish_only_the_current_picture() {
    use Event::*;
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let cases: &[(&str, &[(Event, bool)])] = &[
        (
            "source changes before arrival",
            &[
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Bind(r#"{"src":"assets/b.wgsl"}"#, true), false),
                (Asset("a.wgsl", RED, false), false),
                (Read([0, 0, 0, 0]), false),
                (Asset("b.wgsl", BLUE, true), false),
                (Read([0, 0, 255, 255]), true),
            ],
        ),
        (
            "replacement retains the previous picture without presenting the new source",
            &[
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Asset("a.wgsl", RED, true), false),
                (Read([128, 0, 0, 255]), true),
                (Bind(r#"{"src":"assets/b.wgsl"}"#, true), false),
                (Read([128, 0, 0, 255]), false),
                (Asset("b.wgsl", BLUE, true), false),
                (Read([0, 0, 255, 255]), true),
            ],
        ),
        (
            "invalid initial uniform recovers from the retained source",
            &[
                (
                    Bind(r#"{"src":"assets/a.wgsl","red":"invalid"}"#, true),
                    false,
                ),
                (Asset("a.wgsl", RED, false), false),
                (Read([0, 0, 0, 0]), false),
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Read([128, 0, 0, 255]), true),
                (
                    Bind(r#"{"src":"assets/a.wgsl","red":"invalid"}"#, false),
                    true,
                ),
                (Read([128, 0, 0, 255]), true),
            ],
        ),
        (
            "clear then reload before arrival cannot resurrect the readback",
            &[
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Asset("a.wgsl", RED, true), false),
                (Read([128, 0, 0, 255]), true),
                (Bind(r#"{"src":""}"#, true), false),
                (Bind(r#"{"src":"assets/b.wgsl"}"#, true), false),
                (Read([0, 0, 0, 0]), false),
                (Bind(r#"{"src":""}"#, true), false),
                (Read([0, 0, 0, 0]), false),
                (Bind(r#"{"src":"assets/b.wgsl"}"#, true), false),
                (Read([0, 0, 0, 0]), false),
                (Asset("b.wgsl", BLUE, true), false),
                (Read([0, 0, 255, 255]), true),
            ],
        ),
        (
            "arguments changed before a replacement leave the drawn binding intact",
            &[
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Asset("a.wgsl", RED, true), false),
                (Read([128, 0, 0, 255]), true),
                (Bind(r#"{"src":"assets/a.wgsl","red":0.75}"#, true), false),
                (Bind(r#"{"src":"assets/b.wgsl"}"#, true), false),
                (Read([128, 0, 0, 255]), false),
                (Asset("b.wgsl", BLUE, true), false),
                (Read([0, 0, 255, 255]), true),
            ],
        ),
        (
            "a clear drops an outstanding source and its late arrival",
            &[
                (
                    Bind(r#"{"src":"assets/a.wgsl","red":"invalid"}"#, true),
                    false,
                ),
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Bind(r#"{"src":""}"#, true), false),
                (Asset("a.wgsl", RED, false), false),
                (Read([0, 0, 0, 0]), false),
                (Bind(r#"{"src":"assets/a.wgsl","red":0.75}"#, true), false),
                (Asset("a.wgsl", RED, true), false),
                (Read([191, 0, 0, 255]), true),
            ],
        ),
        (
            "a named alpha mode rebuilds the picture and a refused one keeps it",
            &[
                (Bind(r#"{"src":"assets/a.wgsl","red":0.25}"#, true), false),
                (Asset("a.wgsl", RED, true), false),
                (Read([64, 0, 0, 255]), true),
                (
                    Bind(
                        r#"{"src":"assets/a.wgsl","red":0.25,"alphaMode":"premultiplied"}"#,
                        true,
                    ),
                    false,
                ),
                (Read([64, 0, 0, 0]), true),
                (Bind(r#"{"alphaMode":"straight"}"#, false), true),
                (Bind(r#"{"src":"assets/a.wgsl","red":0.75}"#, true), false),
                (Read([191, 0, 0, 255]), true),
            ],
        ),
        (
            "a failed parse accepts the fixed file",
            &[
                (Bind(r#"{"src":"assets/a.wgsl","red":0.5}"#, true), false),
                (Asset("a.wgsl", BROKEN, false), false),
                (Read([0, 0, 0, 0]), false),
                (Asset("a.wgsl", RED, true), false),
                (Read([128, 0, 0, 255]), true),
            ],
        ),
    ];
    let mut failures = Vec::new();
    let mut module = Module::new(&REGISTRY);
    module.set_gpu(gpu);
    for (name, steps) in cases {
        let id = module.create_headless("shader").unwrap();
        let mut presented = false;
        for (index, (event, expected_presented)) in steps.iter().enumerate() {
            let mut check = |actual, expected, what| {
                if actual != expected {
                    failures.push(format!(
                        "{name}, step {index}: {what}: {actual} != {expected}"
                    ));
                }
            };
            match event {
                Bind(json, accepted) => {
                    check(module.bind_json(id, json, Some(0.)), *accepted, "bind");
                    module.take_assets(id);
                }
                Asset(path, text, accepted) => {
                    check(
                        module.asset(id, path, Ok(text.as_bytes())),
                        *accepted,
                        "asset",
                    );
                }
                Read(expected) => {
                    let frame = Frame {
                        width: 4.,
                        height: 4.,
                        scale: 1.,
                        now_ms: 0.,
                        seekable: true,
                        period_ms: 0.,
                        children_generation: 0,
                        shader_generation: 0,
                        headroom: 1.,
                    };
                    let mut pixels = [0; 4 * 4 * 4];
                    check(
                        module.readback_into(id, &frame, &mut pixels).is_some(),
                        true,
                        "readback",
                    );
                    // A fractional channel, 0.5 as 127.5, may round either way
                    // (Vulkan, "Conversion From Floating-Point to Normalized
                    // Fixed-Point"); 0 and 255 are exact.
                    let off = |(a, b): (&u8, &u8)| match b {
                        0 | 255 => a != b,
                        _ => a.abs_diff(*b) > 1,
                    };
                    if pixels[..4].iter().zip(expected).any(off) {
                        failures.push(format!(
                            "{name}, step {index}: pixel {:?} != {expected:?}",
                            &pixels[..4]
                        ));
                    }
                }
            }
            if let Some(record) = module.take_published(id) {
                presented = record.contains("\"presented\":true");
            }
            if presented != *expected_presented {
                failures.push(format!(
                    "{name}, step {index}: presented {presented} != {expected_presented}"
                ));
            }
            module.take_error();
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
