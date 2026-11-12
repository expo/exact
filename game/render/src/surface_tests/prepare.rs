//! `Game::PREPARE` and pipeline timing (issue #86): a table drawn empty, then
//! a throw that spawns skinned and rigid models at once.
use super::*;
use exact_game::asset::{self, Model};

/// How many of each model the throw spawns (0: none yet).
#[derive(Default, exact_game::Args)]
struct Throw {
    #[live]
    count: u32,
}
struct Table<const PREPARED: bool>;
impl<const PREPARED: bool> Game for Table<PREPARED> {
    const ID: &'static str = "prepared-table";
    const ASSETS: &'static [&'static str] = &["rig.model", "crate.model"];
    const PREPARE: &'static [(&'static str, u32)] = if PREPARED {
        &[("rig.model", 16), ("crate.model", 16)]
    } else {
        &[]
    };
    type Args = Throw;
    fn setup(w: &mut World, _: &Throw) {
        w.spawn((Transform::at(0., 2., 12.), Camera::default()));
    }
    fn tick(w: &mut World, _: &Input, throw: &Throw) {
        if throw.count == 0 || w.query::<&Mesh>().iter().next().is_some() {
            return;
        }
        for i in 0..throw.count {
            let x = i as f32 - throw.count as f32 / 2.;
            w.spawn((Transform::at(x, 0., 0.), Mesh::asset("rig.model")));
            w.spawn((Transform::at(x, 2., 0.), Mesh::asset("crate.model")));
        }
    }
}
fn models() -> [Model; 2] {
    let mut rig = crate::test_model::skinned_model();
    rig.materials.fill(Default::default());
    rig.textures.clear();
    let mut held: Model =
        exact_game::bin::from_slice(include_bytes!("../../../bake/tests/fixtures/crate.model"))
            .unwrap();
    held.materials.fill(Default::default());
    held.textures.clear();
    [rig, held]
}
/// The device work after the empty table's frames, and after the throw's.
fn throw<const PREPARED: bool>(gpu: &Gpu) -> [crate::world::assets::Work; 2] {
    let mut s = WorldSurface::<Table<PREPARED>, crate::ModelExecutor, true>::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(&[Value::Number(0u32.into())], None).unwrap();
    let [rig, held] = models();
    s.asset("rig.model", Ok(&asset::encode_model(&rig)));
    s.asset("crate.model", Ok(&asset::encode_model(&held)));
    let mut now = 0.;
    let mut frames = |s: &mut WorldSurface<Table<PREPARED>, crate::ModelExecutor, true>, n| {
        for _ in 0..n {
            now += 20.;
            fixture::render(gpu, s, &frame(now)).unwrap();
        }
        assert!(s.error().is_none(), "{:?}", s.error());
        let r = s.renderer_for_test().unwrap();
        r.pipelines.wait_specializations();
        r.residency_work()
    };
    crate::perf::CLOCK_READS.with(|n| n.set(0));
    let table = frames(&mut s, 8);
    // The warm-up after a seekable frame times nothing.
    assert_eq!(crate::perf::CLOCK_READS.with(|n| n.get()), 0);
    s.bind(&[Value::Number(16u32.into())], None).unwrap();
    let thrown = frames(&mut s, 4);
    let r = s.renderer_for_test().unwrap();
    let skinned = r.models.records.iter().filter(|r| r.skin.is_some()).count();
    assert!(
        r.models.records.len() >= 32 && skinned >= 16,
        "the throw draws: {} records, {skinned} skinned",
        r.models.records.len()
    );
    [table, thrown]
}

#[test]
fn a_prepared_throw_grows_no_buffer_and_builds_no_pipeline() {
    let Some(gpu) = gpu() else { return };
    let [table, thrown] = throw::<true>(&gpu);
    assert!(
        table.buffer_reallocations > 0,
        "the reserve grew the buffers"
    );
    let after = thrown.since(table);
    assert_eq!(after.buffer_reallocations, 0, "the buffers were sized");
    assert_eq!(after.pipeline_creations, 0, "the pipelines were made");
    // Without the list, the same throw grows them and builds its pipelines.
    let [table, thrown] = throw::<false>(&gpu);
    let after = thrown.since(table);
    assert!(after.buffer_reallocations > 0, "{}", after.json());
    assert!(after.pipeline_creations > 0, "{}", after.json());
}

#[test]
fn pipeline_time_is_reported_on_live_frames_only() {
    let Some(gpu) = gpu() else { return };
    let mut s = WorldSurface::<Table<false>, crate::ModelExecutor, true>::default();
    s.device_ready(exact_gpu::wgpu::Features::empty());
    s.bind(&[Value::Number(1u32.into())], None).unwrap();
    let [rig, held] = models();
    s.asset("rig.model", Ok(&asset::encode_model(&rig)));
    s.asset("crate.model", Ok(&asset::encode_model(&held)));
    // A host on a live clock prepares assets, which makes the renderer,
    // before each render; the first frame that draws is the first one timed.
    s.clock(false);
    for now in [0., 20., 40., 60.] {
        let live = Frame {
            seekable: false,
            ..frame(now)
        };
        s.prepare_assets(
            &gpu.device,
            &gpu.queue,
            exact_gpu::wgpu::TextureFormat::Rgba8Unorm,
        );
        fixture::render(&gpu, &mut s, &live).unwrap();
        if s.perf.pipeline.count != 0 {
            break;
        }
    }
    assert!(s.error().is_none(), "{:?}", s.error());
    let count = s.perf.pipeline.count;
    assert!(count > 0, "a live frame drew");
    assert!(s.perf.pipeline.max > 0., "the first frame made pipelines");
    // The first-frame total covers every timed frame up to it.
    assert!(s
        .perf
        .pipeline_ready
        .is_some_and(|ms| ms >= s.perf.pipeline.max));
    // On the agent's clock a frame reads no clock: it times nothing.
    s.clock(true);
    crate::perf::CLOCK_READS.with(|n| n.set(0));
    s.prepare_assets(
        &gpu.device,
        &gpu.queue,
        exact_gpu::wgpu::TextureFormat::Rgba8Unorm,
    );
    fixture::render(&gpu, &mut s, &frame(80.)).unwrap();
    assert_eq!(crate::perf::CLOCK_READS.with(|n| n.get()), 0);
    assert_eq!(s.perf.pipeline.count, count);
}
