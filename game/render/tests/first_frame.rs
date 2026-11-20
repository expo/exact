//! The first frame a surface draws shows the world `Game::first_frame` framed.
#![cfg(not(target_arch = "wasm32"))]
#[path = "fixture/device.rs"]
mod test_device;
use exact_game::*;
use exact_game_render::WorldSurface;
use exact_gpu::{fixture, Frame, Gpu, Surface};

fn gpu() -> Option<Gpu> {
    test_device::device_or_skip(fixture::device())
}

struct Framed;
impl Game for Framed {
    const ID: &'static str = "render-first-frame";
    type Args = ();
    fn setup(w: &mut World, _: &()) {
        w.insert_resource(Environment {
            ambient: 0.,
            bloom: None,
            fog: None,
            background: Some([0.; 3].into()),
            ..Default::default()
        });
        w.spawn_named("camera", (Transform::default(), Camera::default()));
        // Setup's pose: the die is far to the left, out of the picture.
        w.spawn_named(
            "die",
            (
                Transform::at(-40., 0., -8.),
                Mesh::cube(1.),
                Material::glow([4.; 3]),
            ),
        );
    }
    // A portrait canvas brings the die into the middle of the picture.
    fn first_frame(w: &mut World, input: &Input, _: &()) {
        if input.viewport().y > input.viewport().x {
            w.require_mut::<Transform>("die").position.x = 0.;
        }
    }
    fn tick(_: &mut World, _: &Input, _: &()) {}
}

#[test]
fn the_first_drawn_frame_shows_the_framed_world_at_tick_zero() {
    let Some(gpu) = gpu() else { return };
    let mut surface = WorldSurface::<Framed>::default();
    surface.bind(&[], None).unwrap();
    let frame = Frame {
        width: 240.,
        height: 360.,
        scale: 1.,
        now_ms: 0.,
        seekable: false,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (pixels, _) = fixture::render(&gpu, &mut surface, &frame).unwrap();
    assert!(surface.error().is_none(), "{:?}", surface.error());
    assert_eq!(surface.sim().unwrap().world().tick(), 0);
    assert!(pixels.at(120, 180)[0] > 128, "the die is in the middle");
}

/// Two stamped binds before the first draw tick the world; the size the host
/// gave before binding has framed it at tick 0, and the first frame shows it.
#[test]
fn stamped_binds_before_the_first_draw_keep_the_framed_world() {
    for size_first in [true, false] {
        let mut surface = WorldSurface::<Framed>::default();
        surface.clock(false);
        if size_first {
            assert!(!surface.size(240., 360.), "no world yet to frame");
            surface.bind(&[], Some(0.)).unwrap();
        } else {
            // A bind that runs no ticks, then the size: still tick 0.
            surface.bind(&[], Some(0.)).unwrap();
            assert!(surface.size(240., 360.), "the size framed the world");
        }
        surface.bind(&[], Some(1000.)).unwrap();
        let world = surface.sim().unwrap().world();
        assert_eq!(world.tick(), 60);
        assert_eq!(world.require::<Transform>("die").position.x, 0.);

        let Some(gpu) = gpu() else { continue };
        let frame = Frame {
            width: 240.,
            height: 360.,
            scale: 1.,
            now_ms: 1000.,
            seekable: false,
            period_ms: 0.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.0,
        };
        let (pixels, _) = fixture::render(&gpu, &mut surface, &frame).unwrap();
        assert!(surface.error().is_none(), "{:?}", surface.error());
        assert!(pixels.at(120, 180)[0] > 128, "the die is in the middle");
    }
}

#[test]
fn the_native_live_surface_draws_and_ticks_while_audio_is_still_in_flight() {
    use exact_game::asset::{SoundInfo, SOUND_CATALOG};
    struct Streaming;
    impl Game for Streaming {
        const ID: &'static str = "first-frame-streamed-audio";
        const STREAMED: &'static [&'static str] = &["music.sound"];
        type Args = ();
        fn setup(w: &mut World, args: &()) {
            Framed::setup(w, args);
            w.sounds([(
                "music",
                exact_game::audio::Sample::new("music.sound").looped(),
            )]);
            w.play("music").start();
        }
        fn first_frame(w: &mut World, input: &Input, args: &()) {
            Framed::first_frame(w, input, args);
        }
        fn tick(_: &mut World, _: &Input, _: &()) {}
    }
    let Some(gpu) = gpu() else { return };
    let mut surface = WorldSurface::<Streaming>::default();
    surface.clock(false);
    surface.bind(&[], None).unwrap();
    assert_eq!(surface.assets().requests, [SOUND_CATALOG]);
    let music = SoundInfo {
        rate: 48_000,
        channels: 2,
        frames: 3_000_000,
        digest: 1,
    };
    let catalog = std::collections::BTreeMap::from([("music.sound".to_owned(), music)]);
    surface.asset(SOUND_CATALOG, Ok(&exact_game::bin::to_vec(&catalog)));
    assert_eq!(surface.assets().requests, ["music.sound"]);
    let frame = Frame {
        width: 240.,
        height: 360.,
        scale: 1.,
        now_ms: 0.,
        seekable: false,
        period_ms: 16.667,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    let (pixels, _) = fixture::render(&gpu, &mut surface, &frame).unwrap();
    assert!(pixels.at(120, 180)[0] > 128);
    assert!(
        surface.ready_reasons().is_empty(),
        "{:?}",
        surface.ready_reasons()
    );
    fixture::render(
        &gpu,
        &mut surface,
        &Frame {
            now_ms: 50.,
            ..frame
        },
    )
    .unwrap();
    let sim = surface.sim().unwrap();
    assert!(sim.world().tick() > 0);
    assert!(sim.presentation_sound("music.sound").is_none());
}
