use super::*;
#[derive(Default)]
struct Hook {
    seekable: bool,
    suspended: bool,
    unlocked: bool,
}
impl Executor for Hook {
    fn wants_audio(&self) -> bool {
        true
    }
    fn clock(&mut self, seekable: bool) {
        self.seekable = seekable;
    }
    fn suspend(&mut self, suspended: bool) {
        self.suspended = suspended;
    }
    fn unlock(&mut self) {
        self.unlocked = !self.seekable && !self.suspended;
    }
}
struct GameTest;
impl Game for GameTest {
    type Args = ();
    const NAME: &'static str = "lifecycle";
    const ID: &'static str = "test.lifecycle";
    fn setup(_: &mut World, _: &()) {}
    fn tick(_: &mut World, _: &exact_game::Input, _: &()) {}
}
#[test]
fn lifecycle_only_changes_presentation_and_clock_precedes_first_input() {
    let mut surface = WorldSurface::<GameTest, Hook>::default();
    surface.clock(false);
    surface.bind(&[], None).unwrap();
    assert_eq!(surface.messages(), ["exact:audio"]);
    assert!(surface.messages().is_empty());
    let saved = surface.carry();
    surface.input(&InputEvent::Key {
        code: "KeyW".into(),
        key: "w".into(),
        down: true,
        repeat: false,
        at_ms: 0.,
    });
    assert!(surface.presentation.unlocked);
    let saved_after_input = surface.carry();
    for seekable in [false, true] {
        surface.clock(seekable);
        surface.lifecycle(Lifecycle::Hidden);
        surface.lifecycle(Lifecycle::Interrupted);
        surface.lifecycle(Lifecycle::Visible);
        assert!(surface.presentation.suspended);
        surface.lifecycle(Lifecycle::Resumed);
        assert!(!surface.presentation.suspended);
        assert_eq!(surface.carry(), saved_after_input);
    }
    assert_ne!(saved, saved_after_input); // only the actual input changes saved state
}

#[test]
fn a_press_elsewhere_in_the_page_unlocks_audio_and_changes_no_state() {
    let mut surface = WorldSurface::<GameTest, Hook>::default();
    surface.clock(false);
    surface.bind(&[], None).unwrap();
    let saved = surface.carry();
    assert!(!surface.presentation.unlocked);
    surface.lifecycle(Lifecycle::Activated);
    assert!(surface.presentation.unlocked);
    assert!(!surface.presentation.suspended);
    assert_eq!(surface.carry(), saved);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn format_change_invalidates_device_assets_and_redelivers_textures() {
    struct Textured;
    impl Game for Textured {
        type Args = ();
        const ID: &'static str = "format-change-textured";
        fn setup(w: &mut World, _: &()) {
            w.spawn_named(
                "hero",
                (
                    exact_game::Transform::default(),
                    exact_game::Mesh::asset("hero.model"),
                ),
            );
            w.spawn((
                exact_game::Transform::at(2.8, 2., 3.8)
                    .looking_at(exact_game::Vec3::ZERO, exact_game::Vec3::Y),
                exact_game::Camera::default(),
            ));
        }
        fn tick(_: &mut World, _: &exact_game::Input, _: &()) {}
    }

    let Some(gpu) = crate::test_device::device_or_skip(exact_gpu::fixture::device()) else {
        return;
    };
    let mut s = WorldSurface::<Textured, crate::ModelExecutor, true>::default();
    s.device_ready(wgpu::Features::empty(), wgpu::Limits::downlevel_defaults());
    s.bind(&[], None).unwrap();
    let model: exact_game::asset::Model =
        exact_game::bin::from_slice(include_bytes!("../../../bake/tests/fixtures/crate.model"))
            .unwrap();
    let model_bytes = exact_game::asset::encode_model(&model);
    let tex = include_bytes!("../../../bake/tests/fixtures/crate/0-srgb-straight.tex");
    let frame = Frame {
        width: 32.,
        height: 32.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };

    s.assets();
    s.asset("hero.model", Ok(&model_bytes));
    s.asset(&model.textures[0], Ok(tex));
    let (before, _) = exact_gpu::fixture::render(&gpu, &mut s, &frame).unwrap();
    let initial_format = s.format.expect("renderer created");
    let other_format = if initial_format == wgpu::TextureFormat::Rgba8Unorm {
        wgpu::TextureFormat::Rgba8UnormSrgb
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    };
    assert!(s
        .render
        .as_ref()
        .unwrap()
        .0
        .models
        .textures
        .contains_key(&model.textures[0]));

    // Changing the target format on the same device must invalidate device assets
    // so the host redelivers GPU textures to the rebuilt renderer.
    s.prepare_assets(&gpu.device, &gpu.queue, other_format);
    assert!(s.device);
    let changes = s.assets();
    assert!(changes.retired.contains(&model.textures[0]), "{changes:?}");
    assert!(changes.requests.contains(&model.textures[0]), "{changes:?}");
    s.asset(&model.textures[0], Ok(tex));
    s.prepare_assets(&gpu.device, &gpu.queue, other_format);
    assert!(s
        .render
        .as_ref()
        .unwrap()
        .0
        .models
        .textures
        .contains_key(&model.textures[0]));

    // Switching back to the fixture format and redelivering reproduces the frame.
    s.prepare_assets(&gpu.device, &gpu.queue, initial_format);
    let back = s.assets();
    assert!(back.requests.contains(&model.textures[0]), "{back:?}");
    s.asset(&model.textures[0], Ok(tex));
    let (after, _) = exact_gpu::fixture::render(&gpu, &mut s, &frame).unwrap();
    assert!(
        before == after,
        "format round-trip must preserve textured pixels"
    );
}
