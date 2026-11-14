#![cfg(not(target_arch = "wasm32"))]
//! Every texture family draws the same material within a measured tolerance:
//! the baker's BC4/BC5/BC7 and ASTC payloads against its RGBA8 fallback, each
//! requested by the surface under its family's file name and uploaded as is.
use exact_game::asset::TextureFamily;
use exact_game::*;
use exact_game_render::{
    exact_gpu::{fixture, wgpu, Frame, Surface},
    WorldSurface,
};
use std::collections::BTreeMap;
#[path = "fixture/device.rs"]
mod test_device;

struct Crate;
impl Game for Crate {
    const ID: &'static str = "compressed-crate";
    const ASSETS: &'static [&'static str] = &["crate.model"];
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.spawn_named(
            "crate",
            (
                Transform {
                    rotation: Quat::from_rotation_y(0.6),
                    ..Default::default()
                },
                Mesh::asset("crate.model"),
            ),
        );
        w.spawn((
            Transform::at(1.3, 1.1, 1.6).looking_at(Vec3::ZERO, Vec3::Y),
            Camera::default(),
        ));
        w.insert_resource(Environment {
            fog: None,
            ..Default::default()
        });
    }
    fn tick(_: &mut World, _: &Input, _: &()) {}
}

/// The crate with its one image also sampled as normal, metallic-roughness and
/// occlusion through separate glTF textures: BC7 sRGB, BC5, BC7 linear and BC4.
fn baked() -> BTreeMap<String, Vec<u8>> {
    // One directory a call: the tests bake at once.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let call = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("compressed-{}-{call}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut gltf: serde_json::Value =
        serde_json::from_str(include_str!("../../bake/tests/fixtures/crate.gltf")).unwrap();
    gltf["textures"] = serde_json::json!([{"source": 0}, {"source": 0}, {"source": 0}]);
    let material = &mut gltf["materials"][0];
    material["normalTexture"] = serde_json::json!({"index": 1});
    material["occlusionTexture"] = serde_json::json!({"index": 2});
    material["pbrMetallicRoughness"]["metallicRoughnessTexture"] = serde_json::json!({"index": 0});
    let path = dir.join("crate.gltf");
    std::fs::write(&path, gltf.to_string()).unwrap();
    let (model, textures) = exact_game_bake::assets(&path).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    let mut data: BTreeMap<_, _> = textures
        .iter()
        .map(|(name, t)| (name.clone(), bin::to_vec(t)))
        .collect();
    data.insert(
        "crate.model".into(),
        exact_game::asset::encode_model(&model),
    );
    data
}

/// Every family the bake wrote, as the web serves them; `None` is this
/// target's bundle's (`TextureFamily::BUNDLED`).
const BOTH: &[TextureFamily] = &[TextureFamily::Bc, TextureFamily::Astc];

fn draw(
    gpu: &exact_game_render::exact_gpu::Gpu,
    data: &BTreeMap<String, Vec<u8>>,
    features: wgpu::Features,
    held: Option<&'static [TextureFamily]>,
) -> (fixture::Pixels, String, Vec<String>) {
    let s = WorldSurface::<Crate, exact_game_render::ModelExecutor, true>::default();
    let mut s = match held {
        Some(held) => s.texture_families(held),
        None => s,
    };
    s.device_ready(features);
    s.bind(&[], None).unwrap();
    let mut fetched = Vec::new();
    for _ in 0..8 {
        for file in s.assets().requests {
            s.asset(&file, Ok(&data[&file]));
            fetched.push(file);
        }
        s.prepare_assets(&gpu.device, &gpu.queue, wgpu::TextureFormat::Rgba8Unorm);
    }
    let frame = Frame {
        width: 320.,
        height: 240.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 1000. / 60.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (image, _) = fixture::render(gpu, &mut s, &frame).unwrap();
    assert!(s.take_error().is_none());
    let state = s.agent(r#"{"op":"state"}"#).unwrap();
    (image, state, fetched)
}

#[test]
fn block_payloads_draw_the_rgba8_material_within_tolerance() {
    let Some(gpu) = test_device::device_or_skip(fixture::device()) else {
        return;
    };
    let data = baked();
    let (reference, state, fetched) = draw(&gpu, &data, wgpu::Features::empty(), Some(BOTH));
    reference.save("compressed-rgba8");
    // The crate's planks (dark and light brown) cover a real share of the frame.
    let planks = reference.count(|p| p[0] > p[2] + 20 && p[1] > p[2]);
    assert!(planks > 5_000, "{planks} textured pixels");
    assert!(state.contains(r#""textureFamily":"Rgba8""#), "{state}");
    assert!(state.contains(r#""Rgba8":4"#), "{state}");
    assert!(fetched
        .iter()
        .all(|f| !f.contains(".bc.") && !f.contains(".astc.")));
    for (feature, family, formats) in [
        (
            wgpu::Features::TEXTURE_COMPRESSION_BC,
            "bc",
            r#""Bc4":1,"Bc5":1,"Bc7":2"#,
        ),
        (
            wgpu::Features::TEXTURE_COMPRESSION_ASTC,
            "astc",
            r#""Astc4x4":4"#,
        ),
    ] {
        if !gpu.device.features().contains(feature) {
            eprintln!("SKIP {family}: this device lacks {feature:?}");
            continue;
        }
        let (image, state, fetched) = draw(&gpu, &data, feature, Some(BOTH));
        image.save(&format!("compressed-{family}"));
        assert!(state.contains(formats), "{family}: {state}");
        // The device fetched only its family's files, never the fallback.
        let textures: Vec<_> = fetched.iter().filter(|f| f.ends_with(".tex")).collect();
        assert_eq!(textures.len(), 4, "{fetched:?}");
        assert!(textures
            .iter()
            .all(|f| f.ends_with(&format!(".{family}.tex"))));
        let (mut sum, mut worst, mut changed) = (0u64, 0u8, 0);
        for (a, b) in reference
            .data
            .chunks_exact(4)
            .zip(image.data.chunks_exact(4))
        {
            let d = (0..3).map(|c| a[c].abs_diff(b[c])).max().unwrap();
            sum += u64::from(d);
            worst = worst.max(d);
            changed += usize::from(d > 2);
        }
        let mean = sum as f64 / (image.data.len() / 4) as f64;
        eprintln!("{family}: mean {mean:.3}, max {worst}, {changed} pixels differ by more than 2");
        assert!(
            mean < 0.5 && worst < 24,
            "{family}: mean {mean}, max {worst}"
        );
    }
}

/// A surface fetches the family its target's bundle holds: a Mac's BC, an
/// iPhone's ASTC even on a device that grants BC too (an M-series iPad).
#[test]
fn a_surface_fetches_its_bundles_family() {
    let Some(gpu) = test_device::device_or_skip(fixture::device()) else {
        return;
    };
    let granted = gpu.device.features()
        & (wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::TEXTURE_COMPRESSION_ASTC);
    let (family, feature) = if cfg!(any(target_os = "ios", target_os = "tvos"))
        || !cfg!(target_os = "macos") && !granted.contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
    {
        ("astc", wgpu::Features::TEXTURE_COMPRESSION_ASTC)
    } else {
        ("bc", wgpu::Features::TEXTURE_COMPRESSION_BC)
    };
    if !granted.contains(feature) {
        eprintln!("SKIP: this device lacks {feature:?}, which this target's bundle holds");
        return;
    }
    let (_, state, fetched) = draw(&gpu, &baked(), granted, None);
    let textures: Vec<_> = fetched.iter().filter(|f| f.ends_with(".tex")).collect();
    assert_eq!(textures.len(), 4, "{fetched:?}");
    assert!(
        textures
            .iter()
            .all(|f| f.ends_with(&format!(".{family}.tex"))),
        "{state} {fetched:?}"
    );
}
