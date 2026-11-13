//! Looks registered at tick zero and not shown (issue #129): under the live
//! clock the first frame waits only for what the world presents, the rest is
//! prepared after it, and textures are written off the presenting thread.
use super::cosmetics::{hue, quad, solid};
use super::*;

const LOOKS: [(&str, &str, [u8; 3]); 3] = [
    ("red.model", "red.tex", [255, 0, 0]),
    ("green.model", "green.tex", [0, 255, 0]),
    ("blue.model", "blue.tex", [0, 0, 255]),
];
/// Which look the die shows: a new world when it changes.
#[derive(Default, exact_game::Args)]
struct Shown {
    look: u32,
}
struct Looks;
impl Game for Looks {
    const ID: &'static str = "registered-looks";
    type Args = Shown;
    fn setup(w: &mut World, shown: &Shown) {
        for (i, (model, texture, _)) in LOOKS.into_iter().enumerate() {
            let mesh = w.generated_model(model, quad(texture)).unwrap();
            if i == shown.look as usize {
                w.spawn_named("die", (Transform::default(), mesh));
            }
        }
        w.spawn((Transform::at(0., 0., 4.), Camera::default()));
    }
    fn tick(_: &mut World, _: &Input, _: &Shown) {}
}
fn delivered(clock_seekable: bool) -> WorldSurface<Looks, crate::ModelExecutor, true> {
    let mut s = WorldSurface::<Looks, crate::ModelExecutor, true>::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(&[Value::Number(0u32.into())], None).unwrap();
    s.clock(clock_seekable);
    let mut requested = s.assets().requests;
    requested.sort();
    assert_eq!(requested, ["blue.tex", "green.tex", "red.tex"]);
    for (_, texture, rgb) in LOOKS {
        s.asset(texture, Ok(&solid(rgb)));
    }
    s
}
fn prepared<G: Game>(s: &WorldSurface<G, crate::ModelExecutor, true>) -> Vec<&'static str> {
    let sim = s.sim().unwrap();
    LOOKS
        .iter()
        .flat_map(|(model, texture, _)| [*model, *texture])
        .filter(|name| sim.is_prepared(name))
        .collect()
}

#[test]
fn a_live_first_frame_waits_only_for_the_presented_look() {
    let Some(gpu) = gpu() else { return };
    let hashes = crate::models::model_hash_count();
    let mut s = delivered(false);
    let live = |now| Frame {
        seekable: false,
        ..frame(now)
    };
    // Before the first frame only the presented look is prepared; its
    // texture may still be on the worker.
    s.prepare_assets(
        &gpu.device,
        &gpu.queue,
        exact_gpu::wgpu::TextureFormat::Rgba8Unorm,
    );
    assert_eq!(prepared(&s)[0], "red.model");
    assert!(
        prepared(&s).iter().all(|n| n.starts_with("red.")),
        "{:?}",
        prepared(&s)
    );
    let mut now = 0.;
    let mut first = None;
    for _ in 0..500 {
        now += 20.;
        let (pixels, _) = fixture::render(&gpu, &mut s, &live(now)).unwrap();
        if s.ready_work.is_some() {
            first = Some(pixels);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(s.error().is_none(), "{:?}", s.error());
    let first = first.expect("a live frame drew");
    // The first frame shows the presented look, prepared alone.
    assert_eq!(hue(&first), 0);
    assert_eq!(prepared(&s), ["red.model", "red.tex"]);
    assert!(s.asset_work(), "the other looks are still to prepare");
    // Later frames prepare the rest, the textures off this thread.
    for _ in 0..500 {
        if !s.asset_work() {
            break;
        }
        now += 20.;
        fixture::render(&gpu, &mut s, &live(now)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_eq!(prepared(&s).len(), 6, "{:?}", prepared(&s));
    assert!(s.sim().unwrap().device_assets_ready());
    // A generated model's digest is its identity: the renderer hashes none.
    assert_eq!(crate::models::model_hash_count(), hashes);
}

#[test]
fn the_agents_clock_prepares_every_look_before_its_first_frame() {
    let Some(gpu) = gpu() else { return };
    let mut s = delivered(true);
    let (pixels, _) = fixture::render(&gpu, &mut s, &frame(20.)).unwrap();
    assert_eq!(hue(&pixels), 0);
    assert_eq!(prepared(&s).len(), 6, "{:?}", prepared(&s));
    assert!(!s.asset_work());
}

fn live(now: f64) -> Frame {
    Frame {
        seekable: false,
        ..frame(now)
    }
}

#[test]
fn a_look_shown_before_the_first_frame_is_released_from_waiting() {
    let Some(gpu) = gpu() else { return };
    let mut s = delivered(false);
    s.prepare_assets(
        &gpu.device,
        &gpu.queue,
        exact_gpu::wgpu::TextureFormat::Rgba8Unorm,
    );
    // Green waited for the first frame; then a new world shows it first.
    s.bind(&[Value::Number(1u32.into())], None).unwrap();
    let mut now = 0.;
    let mut first = None;
    for _ in 0..500 {
        now += 20.;
        let (pixels, _) = fixture::render(&gpu, &mut s, &live(now)).unwrap();
        if s.ready_work.is_some() {
            first = Some(pixels);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(s.error().is_none(), "{:?}", s.error());
    let first = first.expect("the first frame drew the look now shown");
    assert_eq!(hue(&first), 1);
    assert!(prepared(&s).contains(&"green.tex"), "{:?}", prepared(&s));
}

#[test]
fn the_agent_taking_the_clock_prepares_what_waited() {
    let Some(gpu) = gpu() else { return };
    let mut s = delivered(false);
    s.prepare_assets(
        &gpu.device,
        &gpu.queue,
        exact_gpu::wgpu::TextureFormat::Rgba8Unorm,
    );
    assert!(!prepared(&s).contains(&"blue.tex"));
    s.clock(true);
    let (pixels, _) = fixture::render(&gpu, &mut s, &frame(20.)).unwrap();
    assert_eq!(hue(&pixels), 0);
    // Nothing waits, and nothing is left on the worker, under the agent's clock.
    assert!(!s.asset_work());
    assert_eq!(prepared(&s).len(), 6, "{:?}", prepared(&s));
    assert!(s.sim().unwrap().device_assets_ready());
}

/// The die's look changes while the world runs.
#[derive(Default, exact_game::Args)]
struct Live {
    #[live]
    look: u32,
}
struct Switch;
impl Game for Switch {
    const ID: &'static str = "switched-looks";
    type Args = Live;
    fn setup(w: &mut World, _: &Live) {
        for (model, texture, _) in LOOKS {
            let mesh = w.generated_model(model, quad(texture)).unwrap();
            if model == "red.model" {
                w.spawn_named("die", (Transform::default(), mesh));
            }
        }
        w.spawn((Transform::at(0., 0., 4.), Camera::default()));
    }
    fn tick(w: &mut World, input: &Input, live: &Live) {
        let model = LOOKS[live.look as usize].0;
        if !matches!(&*w.require::<Mesh>("die"), Mesh::Asset(name) if name == model) {
            *w.require_mut::<Mesh>("die") = Mesh::asset(model);
        }
        if input.messages().iter().any(|m| m == "import") {
            w.invalidate_presentation();
        }
    }
}
type Switched = WorldSurface<Switch, crate::ModelExecutor, true>;
/// A live surface past its first frame, the time of that frame.
fn switched(gpu: &Gpu) -> (Switched, f64) {
    let mut s = Switched::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(&[Value::Number(0u32.into())], None).unwrap();
    s.clock(false);
    for (_, texture, rgb) in LOOKS {
        s.asset(texture, Ok(&solid(rgb)));
    }
    let mut now = 0.;
    while s.ready_work.is_none() {
        assert!(now < 10_000., "a live frame drew");
        now += 20.;
        fixture::render(gpu, &mut s, &live(now)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    (s, now)
}

/// Holds `s`'s upload worker until dropped, on an unwind too: the surface's
/// drop joins the worker, which would wait forever on a hold left set.
struct Held(std::sync::Arc<std::sync::atomic::AtomicBool>);
impl Held {
    fn new(s: &Switched) -> Self {
        let hold = s.uploads.as_ref().unwrap().hold.clone();
        hold.store(true, std::sync::atomic::Ordering::Release);
        Self(hold)
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

#[test]
fn after_the_first_frame_a_look_still_uploading_keeps_the_world_ticking() {
    let Some(gpu) = gpu() else { return };
    let (mut s, mut now) = switched(&gpu);
    // Green's upload is held on the worker.
    let hold = Held::new(&s);
    s.bind(&[Value::Number(1u32.into())], None).unwrap();
    let tick = s.sim().unwrap().world().tick();
    for _ in 0..5 {
        now += 20.;
        let (pixels, _) = fixture::render(&gpu, &mut s, &live(now)).unwrap();
        // The old look stands in (#90) while the frames go on.
        assert_eq!(hue(&pixels), 0);
    }
    assert!(
        s.sim().unwrap().world().tick() > tick,
        "the world ticks during the upload"
    );
    assert!(!prepared(&s).contains(&"green.tex"));
    drop(hold);
    let mut hues = Vec::new();
    while hues.last() != Some(&1) {
        assert!(hues.len() < 500, "{hues:?}");
        now += 20.;
        hues.push(hue(&fixture::render(&gpu, &mut s, &live(now)).unwrap().0));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn a_replacement_texture_delivered_live_replaces_the_one_drawn() {
    let Some(gpu) = gpu() else { return };
    let (mut s, mut now) = switched(&gpu);
    s.asset("red.tex", Ok(&solid([0, 0, 255])));
    let mut hues = Vec::new();
    while hues.last() != Some(&2) {
        assert!(hues.len() < 500, "the replacement never drew: {hues:?}");
        now += 20.;
        hues.push(hue(&fixture::render(&gpu, &mut s, &live(now)).unwrap().0));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn an_older_upload_that_fails_after_its_replacement_arrives_is_dropped() {
    let Some(gpu) = gpu() else { return };
    let (mut s, mut now) = switched(&gpu);
    let written = s.uploads.as_ref().unwrap().written.clone();
    let before = written.load(std::sync::atomic::Ordering::Acquire);
    // A: a BC7 texture, which this device (no BC feature) refuses to write.
    let refused = exact_game::bin::to_vec(&exact_game::asset::TextureData {
        width: 4,
        height: 4,
        format: exact_game::asset::TextureFormat::Bc7,
        mips: vec![vec![0; 16], vec![0; 16], vec![0; 16]],
        srgb: true,
        ..Default::default()
    });
    let held = Held::new(&s);
    s.asset("red.tex", Ok(&refused));
    now += 20.;
    fixture::render(&gpu, &mut s, &live(now)).unwrap();
    drop(held);
    // A's failure is back from the worker before B, its replacement, arrives.
    while written.load(std::sync::atomic::Ordering::Acquire) == before {
        std::thread::yield_now();
    }
    s.asset("red.tex", Ok(&solid([0, 0, 255])));
    let mut hues = Vec::new();
    while hues.last() != Some(&2) {
        assert!(s.error().is_none(), "{:?}", s.error());
        assert!(hues.len() < 500, "the replacement never drew: {hues:?}");
        now += 20.;
        hues.push(hue(&fixture::render(&gpu, &mut s, &live(now)).unwrap().0));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(s.sim().unwrap().asset_failures().next().is_none());
}

#[test]
fn a_new_picture_from_a_message_waits_for_its_shown_texture() {
    let Some(gpu) = gpu() else { return };
    let (mut s, mut now) = switched(&gpu);
    let held = Held::new(&s);
    let generation = s.generation();
    s.bind(&[Value::Number(1u32.into())], None).unwrap();
    s.input(&exact_gpu::InputEvent::Message {
        text: "import".into(),
        at_ms: now + 1.,
    });
    now += 20.;
    fixture::render(&gpu, &mut s, &live(now)).unwrap();
    assert_eq!(s.generation(), generation + 1);
    assert!(
        !s.frame_ready(),
        "the message's frame cannot present the old look"
    );
    for _ in 0..3 {
        now += 20.;
        fixture::render(&gpu, &mut s, &live(now)).unwrap();
        assert!(!s.frame_ready(), "a pending shown texture cannot present");
        assert!(s.ready_work.is_none());
    }
    assert!(!prepared(&s).contains(&"green.tex"));
    drop(held);
    let mut complete = None;
    for _ in 0..500 {
        now += 20.;
        let pixels = fixture::render(&gpu, &mut s, &live(now)).unwrap().0;
        if s.frame_ready() {
            complete = Some(pixels);
            break;
        }
        std::thread::yield_now();
    }
    assert_eq!(hue(&complete.expect("the new picture became ready")), 1);
    assert!(prepared(&s).contains(&"green.tex"));
}

#[test]
fn invalidating_presentation_changes_the_surface_generation_without_changing_state() {
    let mut s = surface();
    let sim = s.sim.as_mut().unwrap();
    let hash = sim.world().hash();
    let save = sim.save().unwrap();
    let generation = sim.generation();
    sim.world_mut().invalidate_presentation();
    assert_eq!(sim.generation(), generation + 1);
    assert_eq!(sim.world().hash(), hash);
    assert_eq!(sim.save().unwrap(), save);
    assert_eq!(s.generation(), generation + 1);
    s.sim
        .as_mut()
        .unwrap()
        .world_mut()
        .invalidate_presentation();
    assert_eq!(s.generation(), generation + 2);
}
