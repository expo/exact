//! Hook diagnostics stay outside saved state and allocate only for hooked canvases.
use super::{budget, HookWork, Hooks, Needs};
use crate::perf::Ring;
pub(crate) const NAMES: [&str; 6] = [
    "prepare",
    "compute",
    "opaque",
    "background",
    "surface",
    "post",
];
const FRAME_NAMES: [&str; 4] = ["draws", "triangles", "uploadBytes", "itemsDrawn"];

#[derive(Default)]
struct FrameMetrics {
    before: [u64; 4],
    rings: [Ring; 4],
}

#[derive(Default)]
pub(crate) struct Metrics {
    pub engine: HookWork,
    pub prepare_budget: Option<budget::PrepareSample>,
    before: Option<(HookWork, Option<HookWork>)>,
    cpu: [Ring; 6],
    frame: Option<FrameMetrics>,
    armed: bool,
    pub attachment_bytes: u64,
}
impl Metrics {
    pub fn arm(&mut self, reset: bool) {
        if self.armed && !reset {
            return;
        }
        self.armed = true;
        for ring in &mut self.cpu {
            ring.arm(reset);
        }
        if let Some(frame) = &mut self.frame {
            for ring in &mut frame.rings {
                ring.arm(reset);
            }
        }
    }
    pub fn clear_cpu(&mut self) {
        for ring in &mut self.cpu {
            ring.clear();
        }
    }
    #[cfg(test)]
    pub fn cpu_storage(&self) -> [(*const f64, usize); 6] {
        self.cpu
            .each_ref()
            .map(|ring| (ring.values.as_ptr(), ring.values.capacity()))
    }

    pub fn disarm(&mut self) {
        self.armed = false;
        self.cpu = Default::default();
        if let Some(frame) = &mut self.frame {
            frame.rings = Default::default();
        }
    }

    pub fn record_stages(
        &mut self,
        times: [Option<f64>; 6],
        hooks: &impl Hooks,
        ready: bool,
        cpu: Option<(&mut crate::perf::Perf, bool)>,
    ) {
        let work = hooks.work();
        if let Some((perf, seekable)) = cpu {
            if times.iter().any(Option::is_some) {
                perf.begin_cpu_sample(seekable, Some(self));
            }
        }
        for (ring, time) in self.cpu.iter_mut().zip(times) {
            if let Some(ms) = time {
                ring.push(ms);
            }
        }
        if ready {
            self.before.get_or_insert((self.engine, work));
        }
    }

    pub fn record_frame(&mut self, hooks: &impl Hooks, ran_hook_stage: bool, sample: bool) {
        if !ran_hook_stage {
            return;
        }
        let Some(work) = hooks.frame_work() else {
            if sample {
                self.frame = None;
            }
            return;
        };
        let counts = [work.draws, work.triangles, work.upload_bytes, work.items];
        let Some(frame) = &mut self.frame else {
            let mut frame = FrameMetrics {
                before: counts,
                ..FrameMetrics::default()
            };
            if self.armed {
                for ring in &mut frame.rings {
                    ring.arm(false);
                }
            }
            self.frame = Some(frame);
            return;
        };
        let before = std::mem::replace(&mut frame.before, counts);
        if !sample {
            return;
        }
        let [Some(draws), Some(triangles), Some(upload_bytes), Some(items)] =
            std::array::from_fn(|i| counts[i].checked_sub(before[i]))
        else {
            return;
        };
        for (ring, count) in frame
            .rings
            .iter_mut()
            .zip([draws, triangles, upload_bytes, items])
        {
            ring.push(count as f64);
        }
    }

    pub fn append(
        &self,
        out: &mut String,
        work: Option<HookWork>,
        needs: Needs,
        item_name: &str,
        budget: Option<&budget::Budget>,
        allowance: u32,
    ) {
        let stages = stages(needs);
        out.push_str(&format!(",\"renderHooks\":{{\"stages\":{stages},\"attachmentBytes\":{},\"gpuTiming\":\"optional query set; opaque/background inside forward, surface inside continuation\",\"engineWork\":{{\"beforeReady\":{},\"afterReady\":{}}},\"reportedWork\":",
            self.attachment_bytes,self.before.map_or(self.engine,|b|b.0).json(),self.engine.since(self.before.map_or(self.engine,|b|b.0)).json()));
        if let Some(work) = work {
            let before = self.before.and_then(|b| b.1).unwrap_or(work);
            out.push_str(&format!(
                "{{\"beforeReady\":{},\"afterReady\":{}}}",
                before.json(),
                work.since(before).json()
            ));
        } else {
            out.push_str("null");
        }
        out.push_str(",\"cpuMs\":{");
        for (i, (name, ring)) in NAMES.iter().zip(&self.cpu).enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{name}\":"));
            ring.json(out);
        }
        out.push_str("},\"frame\":");
        if let Some(frame) = &self.frame {
            out.push_str("{\"items\":");
            exact_game::json::quote_into(out, item_name);
            for (name, ring) in FRAME_NAMES.iter().zip(&frame.rings) {
                out.push_str(&format!(",\"{name}\":"));
                ring.json(out);
            }
            out.push('}');
        } else {
            out.push_str("null");
        }
        out.push_str(",\"budget\":");
        if let Some(budget) = budget {
            budget.append(out, allowance);
        } else {
            out.push_str("null");
        }
        out.push('}');
    }
}
pub(crate) fn append_state<H: Hooks>(
    metrics: Option<&Metrics>,
    out: &mut String,
    hooks: &H,
    budget: Option<&budget::Budget>,
) {
    if let Some(metrics) = metrics {
        metrics.append(
            out,
            hooks.work(),
            hooks.needs(),
            hooks.item_name(),
            budget,
            H::ALLOWANCE,
        );
        out.pop();
        out.push(',');
    } else {
        out.push_str(",\"renderHooks\":{");
    }
    out.push_str("\"game\":");
    match hooks.report() {
        Some(report) if game_object(report).is_ok() => out.push_str(report.trim()),
        Some(_) => out.push_str("null,\"gameError\":\"render hook report must be a JSON object\""),
        None => out.push_str("null"),
    }
    out.push('}');
}

fn game_object(text: &str) -> Result<(), exact_game::DataError> {
    use exact_game::Reader;

    let mut reader = exact_game::json::Decoder::new(text);
    reader.begin_struct()?;
    while reader.field()?.is_some() {
        reader.skip()?;
    }
    reader.finish()
}

pub(crate) fn stages(needs: Needs) -> String {
    let mut names = vec!["compute", "opaque", "background"];
    if needs.contains(Needs::SCENE_COPY) {
        names.push("surface");
    }
    if needs.contains(Needs::HDR_POST) {
        names.push("post");
    }
    exact_game::json::to_string(&names.into_iter().map(str::to_owned).collect::<Vec<_>>()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameWork;

    #[derive(Default)]
    struct CreationsOnly;
    impl Hooks for CreationsOnly {
        fn work(&self) -> Option<HookWork> {
            Some(HookWork {
                buffers: 1,
                ..HookWork::default()
            })
        }
    }

    #[derive(Default)]
    struct Reporter(FrameWork);
    impl Hooks for Reporter {
        fn frame_work(&self) -> Option<FrameWork> {
            Some(self.0)
        }
    }

    #[test]
    fn render_hooks_budget_is_null_for_an_opted_out_hook() {
        let hooks = read(&Metrics::default(), None, "items");
        assert_eq!(hooks.get("budget"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn render_hooks_game_report() {
        #[derive(Default)]
        struct GameReport(Option<&'static str>);
        impl Hooks for GameReport {
            fn report(&self) -> Option<&str> {
                self.0
            }
        }

        for metrics in [None, Some(&Metrics::default())] {
            for (report, expected, refused) in [
                (
                    Some(r#" {"units":3,"nested":[true,null]} "#),
                    serde_json::json!({"units": 3, "nested": [true, null]}),
                    false,
                ),
                (Some("[]"), serde_json::Value::Null, true),
                (Some("{\"units\":}"), serde_json::Value::Null, true),
                (Some("{} false"), serde_json::Value::Null, true),
                (None, serde_json::Value::Null, false),
            ] {
                let mut json = String::from("{\"world\":{\"ready\":true");
                append_state(metrics, &mut json, &GameReport(report), None);
                json.push_str("}}");
                let state: serde_json::Value = serde_json::from_str(&json).unwrap();
                let hooks = &state["world"]["renderHooks"];
                assert_eq!(hooks.get("game"), Some(&expected));
                assert_eq!(
                    hooks.get("gameError").and_then(|error| error.as_str()),
                    refused.then_some("render hook report must be a JSON object")
                );
            }
        }
    }

    fn reporter(counts: [u64; 4]) -> Reporter {
        let [draws, triangles, upload_bytes, items] = counts;
        Reporter(FrameWork {
            draws,
            triangles,
            upload_bytes,
            items,
        })
    }

    fn record(metrics: &mut Metrics, counts: [u64; 4], ran: bool) {
        metrics.record_frame(&reporter(counts), ran, true);
    }

    fn assert_counts(metrics: &Metrics, count: u64, max: [f64; 4]) {
        for (ring, max) in metrics.frame.as_ref().unwrap().rings.iter().zip(max) {
            assert_eq!(ring.count, count);
            assert_eq!(ring.max, max);
        }
    }

    fn read(metrics: &Metrics, work: Option<HookWork>, item_name: &str) -> serde_json::Value {
        let mut json = String::from("{\"world\":true");
        metrics.append(&mut json, work, Needs::NONE, item_name, None, 0);
        json.push('}');
        serde_json::from_str::<serde_json::Value>(&json).unwrap()["renderHooks"].take()
    }

    #[test]
    fn creations_only_has_no_frame_report() {
        let mut metrics = Metrics::default();
        metrics.arm(false);
        let hooks = CreationsOnly;
        metrics.record_stages([None; 6], &hooks, true, None);
        metrics.record_frame(&hooks, true, true);
        let hooks = read(&metrics, hooks.work(), hooks.item_name());
        assert_eq!(hooks["reportedWork"]["beforeReady"]["buffers"], 1);
        assert_eq!(hooks.get("frame"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn first_read_sets_the_baseline() {
        let mut metrics = Metrics::default();
        metrics.arm(false);
        record(&mut metrics, [3, 36, 96, 1], true);
        assert_counts(&metrics, 0, [0.; 4]);
        record(&mut metrics, [6, 72, 192, 2], true);
        let name = "fixture \"items\"";
        let state = read(&metrics, None, name);
        let frame = &state["frame"];
        assert_eq!(frame["items"], name);
        for (key, max) in [
            ("draws", 3.),
            ("triangles", 36.),
            ("uploadBytes", 96.),
            ("itemsDrawn", 1.),
        ] {
            assert_eq!(frame[key]["count"], 1);
            assert_eq!(frame[key]["max"].as_f64(), Some(max));
        }
    }

    #[test]
    fn no_stage_keeps_the_previous_read() {
        let mut metrics = Metrics::default();
        record(&mut metrics, [1, 2, 3, 1], true);
        record(&mut metrics, [3, 36, 96, 2], false);
        assert_counts(&metrics, 0, [0.; 4]);
        record(&mut metrics, [3, 36, 96, 2], true);
        assert_counts(&metrics, 1, [2., 34., 93., 1.]);
    }

    #[test]
    fn a_stage_with_no_work_pushes_zero() {
        let mut metrics = Metrics::default();
        record(&mut metrics, [1, 2, 3, 1], true);
        record(&mut metrics, [1, 2, 3, 1], true);
        assert_counts(&metrics, 1, [0.; 4]);
    }

    #[test]
    fn an_excluded_frame_advances_the_baseline_without_a_sample() {
        let mut metrics = Metrics::default();
        record(&mut metrics, [3, 36, 96, 1], true);
        metrics.record_frame(&reporter([6, 72, 192, 2]), true, false);
        assert_counts(&metrics, 0, [0.; 4]);
        record(&mut metrics, [9, 108, 288, 3], true);
        assert_counts(&metrics, 1, [3., 36., 96., 1.]);
    }

    #[test]
    fn one_decreased_counter_rebases_all_four() {
        let mut metrics = Metrics::default();
        record(&mut metrics, [1, 2, 96, 1], true);
        record(&mut metrics, [2, 3, 64, 2], true);
        assert_counts(&metrics, 0, [0.; 4]);
        record(&mut metrics, [3, 5, 72, 3], true);
        assert_counts(&metrics, 1, [1., 2., 8., 1.]);
    }

    #[test]
    fn reset_empties_the_rings_and_keeps_the_baseline() {
        let mut metrics = Metrics::default();
        metrics.arm(false);
        record(&mut metrics, [1, 2, 3, 1], true);
        record(&mut metrics, [3, 6, 9, 2], true);
        metrics.arm(true);
        assert_counts(&metrics, 0, [0.; 4]);
        assert!(metrics
            .frame
            .as_ref()
            .unwrap()
            .rings
            .iter()
            .all(|ring| ring.len == 0));
        record(&mut metrics, [4, 8, 12, 3], true);
        assert_counts(&metrics, 1, [1., 2., 3., 1.]);
    }

    #[test]
    fn two_layers_can_report_one_item() {
        let mut metrics = Metrics::default();
        record(&mut metrics, [0; 4], true);
        record(&mut metrics, [2, 24, 0, 1], true);
        assert_counts(&metrics, 1, [2., 24., 0., 1.]);
    }

    #[test]
    fn ring_storage_requires_arming_and_a_reporter() {
        let mut metrics = Metrics::default();
        record(&mut metrics, [0; 4], true);
        record(&mut metrics, [1, 2, 3, 1], true);
        assert_counts(&metrics, 1, [1., 2., 3., 1.]);
        assert!(metrics
            .frame
            .as_ref()
            .unwrap()
            .rings
            .iter()
            .all(|ring| ring.values.capacity() == 0));
        metrics.arm(false);
        assert!(metrics
            .frame
            .as_ref()
            .unwrap()
            .rings
            .iter()
            .all(|ring| !ring.values.is_empty()));
        metrics.record_frame(&CreationsOnly, true, true);
        assert!(metrics.frame.is_none());
        record(&mut metrics, [1, 2, 3, 1], true);
        assert!(metrics
            .frame
            .as_ref()
            .unwrap()
            .rings
            .iter()
            .all(|ring| !ring.values.is_empty()));
    }

    #[test]
    fn disarming_releases_storage_and_keeps_the_baseline() {
        let mut metrics = Metrics::default();
        metrics.arm(false);
        let work = [3, 36, 96, 1];
        record(&mut metrics, work, true);
        record(&mut metrics, work.map(|value| value * 2), true);
        metrics.record_stages([Some(1.); 6], &reporter(work), true, None);

        metrics.disarm();
        assert!(!metrics.armed);
        assert_counts(&metrics, 0, [0.; 4]);
        for ring in metrics
            .cpu
            .iter()
            .chain(&metrics.frame.as_ref().unwrap().rings)
        {
            assert_eq!(ring.values.capacity(), 0);
            assert_eq!(ring.count, 0);
            assert_eq!(ring.sum, 0.);
            assert_eq!(ring.max, 0.);
        }
        record(&mut metrics, work.map(|value| value * 3), true);
        assert_counts(&metrics, 1, [3., 36., 96., 1.]);

        metrics.arm(false);
        metrics.record_frame(&CreationsOnly, true, true);
        metrics.disarm();
        record(&mut metrics, work, true);
        assert!(metrics
            .frame
            .as_ref()
            .unwrap()
            .rings
            .iter()
            .all(|ring| ring.values.capacity() == 0));
    }

    #[cfg(not(target_arch = "wasm32"))]
    mod lifecycle {
        use super::*;
        use crate::{FrameView, HookGpu, RenderError, RenderWorld, WorldSurface};
        use exact_game::{Args, Game, Input, World};
        use exact_gpu::{fixture, Frame, Restore, Surface, Value};

        #[derive(Default, Args)]
        struct Options {
            #[restart]
            restart: bool,
        }

        struct Fixture;
        impl Game for Fixture {
            type Args = Options;
            const ID: &'static str = "hook-metrics-lifecycle";
            fn setup(_: &mut World, _: &Options) {}
            fn tick(_: &mut World, _: &Input, _: &Options) {}
        }

        #[derive(Default)]
        struct Reporter {
            work: FrameWork,
            reporting: bool,
        }
        impl Hooks for Reporter {
            fn prepare(
                &mut self,
                _: &HookGpu<'_>,
                view: &FrameView<'_>,
                _: &RenderWorld<'_>,
            ) -> Result<(), RenderError> {
                self.reporting = view.time.points.0 > 64.;
                self.work.upload_bytes += 1;
                Ok(())
            }

            fn frame_work(&self) -> Option<FrameWork> {
                self.reporting.then_some(self.work)
            }
        }

        type ReportingSurface = WorldSurface<Fixture, (), false, Reporter>;

        fn metrics(surface: &mut ReportingSurface) -> &Metrics {
            surface
                .renderer_for_test()
                .unwrap()
                .hook_metrics
                .as_deref()
                .unwrap()
        }

        fn assert_storage(metrics: &Metrics, armed: bool) {
            let frame = metrics.frame.iter().flat_map(|frame| &frame.rings);
            for ring in metrics.cpu.iter().chain(frame) {
                assert_eq!(ring.values.capacity() > 0, armed);
            }
        }

        fn check_disarm(replace_world: impl Fn(&mut ReportingSurface)) {
            let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
                return;
            };
            for already_reporting in [true, false] {
                let mut surface = ReportingSurface::default();
                surface.bind(&[], None).unwrap();
                surface.agent(r#"{"op":"state","perf":true}"#).unwrap();
                let mut frame = Frame {
                    width: if already_reporting { 65. } else { 64. },
                    height: 64.,
                    scale: 1.,
                    now_ms: 0.,
                    seekable: true,
                    period_ms: 0.,
                    children_generation: 0,
                    shader_generation: 0,
                    headroom: 1.,
                };
                for now in [0., 16.] {
                    frame.now_ms = now;
                    fixture::render(&gpu, &mut surface, &frame).unwrap();
                }
                let before = metrics(&mut surface);
                assert!(before.armed);
                assert_storage(before, true);
                assert_eq!(before.frame.is_some(), already_reporting);
                if already_reporting {
                    assert_counts(before, 1, [0., 0., 1., 0.]);
                }

                replace_world(&mut surface);
                let state = surface.agent(r#"{"op":"state"}"#).unwrap();
                let state: serde_json::Value = serde_json::from_str(&state).unwrap();
                assert_eq!(state["world"]["perf"]["armed"], false, "{state}");
                let after = metrics(&mut surface);
                assert!(!after.armed);
                assert_storage(after, false);
                if already_reporting {
                    assert_counts(after, 0, [0.; 4]);
                    assert!(state["world"]["renderHooks"]["frame"]["uploadBytes"]["p50"].is_null());
                }

                frame.width = 65.;
                for now in [32., 48.] {
                    frame.now_ms = now;
                    fixture::render(&gpu, &mut surface, &frame).unwrap();
                }
                let after = metrics(&mut surface);
                assert_storage(after, false);
                assert_counts(
                    after,
                    if already_reporting { 2 } else { 1 },
                    [0., 0., 1., 0.],
                );
            }
        }

        #[test]
        fn restore_disarms_hook_rings() {
            check_disarm(|surface| {
                let carry = surface.carry().unwrap().unwrap();
                surface.restore(&carry, Restore::Carry).unwrap();
            });
        }

        #[test]
        fn restart_disarms_hook_rings() {
            check_disarm(|surface| surface.bind(&[Value::Bool(true)], None).unwrap());
        }
    }
}
