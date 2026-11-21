use super::*;
use exact_game_render::Hooks;
use std::collections::VecDeque;
use terrain_fixture_logic::{edit, Blend, Budget, Inflight, Motion, SECTION_COUNT, SEED};

fn hook_time(allowance: u32, seekable: bool) -> HookTime {
    let mut time = HookTime::default();
    time.allowance = allowance;
    time.seekable = seekable;
    time
}

#[derive(Default)]
struct InlineResults {
    queued: VecDeque<(u64, u64, Input)>,
    blocked: bool,
    capacity: Option<usize>,
    meshes: u64,
    spent_outside_queue: bool,
}

impl InlineResults {
    fn cancel(&mut self, key: u64) {
        self.queued.retain(|(queued, _, _)| *queued != key);
    }

    fn post_at_elapsed_deadline(&mut self) -> Option<Input> {
        if self.spent_outside_queue {
            return None;
        }
        self.queued.pop_front().map(|(_, _, input)| input)
    }
}

impl Results for InlineResults {
    fn submit(
        &mut self,
        key: u64,
        generation: u64,
        _: i32,
        input: Input,
    ) -> Result<(), Full<Input>> {
        if self.capacity == Some(self.queued.len()) {
            return Err(Full(input));
        }
        self.queued.push_back((key, generation, input));
        Ok(())
    }

    fn sample_retained_bytes(&mut self) -> (usize, usize) {
        let bytes = self.queued.iter().map(|(_, _, input)| input.bytes()).sum();
        (bytes, bytes)
    }

    fn mark_late_unit_spent(&mut self) {
        self.spent_outside_queue = true;
    }
}

impl Iterator for InlineResults {
    type Item = Done<Output>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.blocked {
            return None;
        }
        let (key, generation, input) = self.queued.pop_front()?;
        let result = input.run(&Cancel::default());
        self.meshes += u64::from(matches!(result, Ok(Output::Mesh(_))));
        Some(Done {
            key,
            generation,
            result,
        })
    }
}

#[derive(Default)]
struct Drive {
    passes: Passes,
    pending: PendingJobs,
    results: InlineResults,
}

impl Drive {
    fn step(&mut self, settings: WorkloadSettings, tick: u64, allowance: u32) {
        self.step_at(settings, tick, &hook_time(allowance, true));
    }

    fn step_at(&mut self, settings: WorkloadSettings, tick: u64, time: &HookTime) {
        let passes = &mut self.passes;
        passes.begin_work(settings, tick, 60, time);
        let camera = settings.motion.eye(tick, 60).to_array();
        if settings.jobs {
            self.pending
                .reconcile(&mut passes.workload, |key| self.results.cancel(key));
            passes.prepare_job_units(
                &mut self.pending,
                &mut self.results,
                &Prepare {
                    gpu: None,
                    time,
                    tick,
                    hz: 60,
                    camera,
                },
                if time.seekable {
                    Lane::Inline
                } else {
                    Lane::Pool
                },
                |_| time.deadline.is_some_and(|deadline| deadline.passed()),
            );
        } else {
            passes.prepare_work(None, time, tick, 60, camera);
        }
    }
}

fn flying(jobs: bool) -> WorkloadSettings {
    WorkloadSettings {
        jobs,
        compare: true,
        motion: Motion::Fly,
        radius: 8,
        ..WorkloadSettings::default()
    }
}

struct PoolResults {
    queue: Jobs<Input>,
    submitted: Vec<SectionWork>,
    refused_bytes: Option<usize>,
}

impl PoolResults {
    fn new(queue: Jobs<Input>) -> Self {
        Self {
            queue,
            submitted: Vec::new(),
            refused_bytes: None,
        }
    }

    fn prepare(&mut self, passes: &mut Passes, pending: &mut PendingJobs, tick: u64) {
        passes.prepare_job_units(
            pending,
            self,
            &Prepare {
                gpu: None,
                time: &hook_time(64, false),
                tick,
                hz: 60,
                camera: passes.settings.motion.eye(tick, 60).to_array(),
            },
            Lane::Pool,
            |_| false,
        );
    }
}

impl Iterator for PoolResults {
    type Item = Done<Output>;

    fn next(&mut self) -> Option<Self::Item> {
        // Keep workers stopped so admission is independent of their timing.
        None
    }
}

impl Results for PoolResults {
    fn submit(
        &mut self,
        key: u64,
        generation: u64,
        priority: i32,
        input: Input,
    ) -> Result<(), Full<Input>> {
        let kind = input.work[0].kind;
        if let Err(Full(input)) = self.queue.submit(key, generation, priority, input) {
            self.refused_bytes = Some(input.bytes());
            return Err(Full(input));
        }
        self.submitted.push(kind);
        Ok(())
    }

    fn sample_retained_bytes(&mut self) -> (usize, usize) {
        self.queue.sample_retained_bytes()
    }

    fn mark_late_unit_spent(&mut self) {}
}

fn finish_mesh(passes: &mut Passes, tick: u64) {
    let work = passes.workload.next_jobs(passes.settings, tick, 60);
    assert_eq!(work.len(), 1);
    assert_eq!(work[0].kind, SectionWork::Mesh);
    passes.apply_job(None, &work, Output::Mesh(Candidate::default()));
}

#[test]
fn jobs_pool_fills_at_least_sixty_four_jobs_with_compares_pending() {
    let settings = WorkloadSettings {
        radius: 40,
        light: 8000,
        ..flying(true)
    };
    let mut passes = Passes::default();
    let time = hook_time(64, false);
    passes.begin_work(settings, 0, 60, &time);
    let start = settings.window(0, 60);
    for _ in start.sections() {
        finish_mesh(&mut passes, 0);
    }
    let tick = 27;
    passes.begin_work(settings, tick, 60, &time);
    let entering = settings.window(tick, 60).entering(Some(start)).count();
    for _ in 0..entering - 4 {
        finish_mesh(&mut passes, tick);
    }

    let TerrainJobs { queue, mut pending } = passes.jobs.take().unwrap();
    let mut results = PoolResults::new(queue);
    results.prepare(&mut passes, &mut pending, tick);
    assert_eq!(results.submitted.first(), Some(&SectionWork::Mesh));
    assert_eq!(results.submitted.last(), Some(&SectionWork::Compare));
    let report = results.queue.report();
    assert!(
        report.queued >= 64,
        "one fill queued {} jobs",
        report.queued
    );
    let limit = 256 * 1024;
    let refused = results
        .refused_bytes
        .expect("queue should refuse a compare batch");
    assert!(
        report.queued_bytes <= limit && report.queued_bytes + refused > limit,
        "{} bytes queued before refusing {refused} more",
        report.queued_bytes
    );
    results.prepare(&mut passes, &mut pending, tick);
    assert_eq!(results.queue.report().queued_bytes, report.queued_bytes);
}

#[test]
fn jobs_pool_admits_one_oversize_job_when_empty() {
    let mut passes = Passes::default();
    let settings = WorkloadSettings {
        jobs: true,
        ..WorkloadSettings::default()
    };
    passes.begin_work(settings, 0, 60, &hook_time(64, false));
    let queue = Jobs::new(Limits {
        queued_bytes: 1,
        ..LIMITS
    });
    let mut results = PoolResults::new(queue);
    results.prepare(&mut passes, &mut PendingJobs::default(), 0);
    let report = results.queue.report();
    assert_eq!(report.queued, 1);
    assert!(report.queued_bytes > 1);
    assert!(results.refused_bytes.is_some());
}

#[test]
fn jobs_flying_with_one_unit_retires_sections_before_more_results() {
    let drive = |jobs| {
        let mut drive = Drive::default();
        let settings = WorkloadSettings {
            budget: Budget::Count(1),
            ..flying(jobs)
        };
        for _ in 0..SECTION_COUNT {
            drive.step(settings, 0, 64);
        }
        assert_eq!(drive.passes.sections.len(), SECTION_COUNT);
        for tick in [27, 28, 29, 75, 76, 77, 123, 124, 125] {
            drive.step(settings, tick, 64);
        }
        drive.passes.sections.into_keys().collect::<Vec<_>>()
    };
    let on = drive(true);
    assert_eq!(on, drive(false));
    assert!(on
        .iter()
        .all(|&origin| flying(true).window(125, 60).contains(origin)));
}

#[test]
fn jobs_shift_with_work_in_flight_matches_the_installed_window() {
    let drive = |jobs| {
        let mut drive = Drive::default();
        let settings = flying(jobs);
        for tick in [0, 27, 75, 123, 171] {
            for advance in 0..3 {
                drive.results.blocked = jobs && advance == 2;
                drive.step(settings, tick, 64);
                if jobs && advance == 2 {
                    assert!(!drive.pending.flight.is_empty());
                }
            }
        }
        drive.results.blocked = false;
        for _ in 0..80 {
            if drive.passes.workload.pending(settings) == 0 {
                break;
            }
            drive.step(settings, 171, 64);
        }
        assert_eq!(drive.passes.workload.pending(settings), 0);
        assert_eq!(drive.passes.sections.len(), SECTION_COUNT);
        drive.passes.sections.into_keys().collect::<Vec<_>>()
    };
    assert_eq!(drive(true), drive(false));
}

#[test]
fn jobs_inline_submission_sees_the_frame_edit() {
    let settings = WorkloadSettings {
        edits: 60,
        ..WorkloadSettings::default()
    };
    let edit = edit(SEED, 1, 0, settings.window(1, 60));
    let mut workload = Workload::<()>::default();
    workload.advance(settings, 0, 60, false, 0.);
    let prior = (0..SECTION_COUNT)
        .find(|_| workload.next_jobs(settings, 0, 60)[0].origin == edit.origin)
        .unwrap();
    for jobs in [false, true] {
        let settings = WorkloadSettings { jobs, ..settings };
        let mut drive = Drive::default();
        for _ in 0..prior {
            drive.step(settings, 0, 1);
        }
        for _ in 0..SECTION_COUNT.div_ceil(64) + 1 {
            drive.step(settings, 1, 64);
        }
        assert_eq!(drive.passes.workload.pending(settings), 0);
        assert_eq!(
            drive.passes.workload.counts(),
            (SECTION_COUNT as u64 + 1, 0, [0; 3])
        );
        if jobs {
            assert_eq!(drive.results.meshes, SECTION_COUNT as u64);
        }
    }
}

#[test]
fn jobs_edit_in_flight_uses_the_production_installer() {
    for inflight in [Inflight::Discard, Inflight::Keep] {
        let mut drive = Drive::default();
        let settings = WorkloadSettings {
            jobs: true,
            compare: true,
            inflight,
            ..WorkloadSettings::default()
        };
        drive.results.blocked = true;
        drive.step(settings, 0, 1);
        let job = drive.pending.flight.values().next().unwrap()[0];
        let index = (0..)
            .find(|&index| {
                let edit = edit(SEED, 1, index, settings.window(1, 60));
                edit.origin == job.origin && edit.changes_input
            })
            .unwrap();
        drive.passes.workload.edit(settings, 1, index, 60);
        drive.results.blocked = false;
        drive.step(settings, 1, 1);
        let workload = &drive.passes.workload;
        let sections = &drive.passes.sections;
        if inflight == Inflight::Keep {
            assert_eq!(workload.counts(), (2, 1, [0; 3]));
            assert!(sections.contains_key(&job.origin));
            assert_eq!(
                sections[&job.origin].input.as_ref().unwrap().cells(),
                &*padded_input(job)
            );
        } else {
            assert_eq!(workload.counts(), (2, 0, [1, 0, 0]));
            assert!(!sections.contains_key(&job.origin));
        }
        for tick in 2..=4 {
            drive.step(settings, tick, 1);
        }
        assert_eq!(
            drive.passes.sections[&job.origin]
                .input
                .as_ref()
                .unwrap()
                .cells(),
            &*mesh::padded_input(job.origin, height, true)
        );
    }
}

#[test]
fn jobs_clock_and_same_tick_restore_keep_work_but_rewind_clears_it() {
    let mut passes = Passes::default();
    let settings = flying(true);
    let mut time = hook_time(1, true);
    time.reset = false;
    passes.begin_work(settings, 10, 60, &time);
    let work = passes.workload.next_jobs(settings, 10, 60);
    assert!(passes
        .jobs
        .as_mut()
        .unwrap()
        .queue
        .submit(
            key(work[0].origin),
            passes.workload.generation(),
            0,
            Input {
                work: work.into(),
                previous: Vec::new(),
                settings
            },
        )
        .is_ok());
    let mut restored = time;
    restored.reset = true;
    passes.begin_work(settings, 11, 60, &restored);
    assert_eq!(passes.jobs.as_ref().unwrap().queue.pending(), 1);
    passes.begin_work(settings, 11, 60, &time);
    assert_eq!(passes.jobs.as_ref().unwrap().queue.pending(), 1);
    passes.begin_work(settings, 10, 60, &time);
    assert_eq!(passes.jobs.as_ref().unwrap().queue.pending(), 0);
}

#[test]
fn jobs_full_requeues_a_repair_without_meshing_the_next_revision_twice() {
    let mut drive = Drive::default();
    let settings = WorkloadSettings {
        jobs: true,
        ..WorkloadSettings::default()
    };
    for _ in 0..SECTION_COUNT.div_ceil(64) {
        drive.step(settings, 0, 64);
    }
    let prior = drive.results.meshes;
    let window = settings.window(1, 60);
    let (index, first) = (0..)
        .map(|index| (index, edit(SEED, 1, index, window)))
        .find(|(_, edit)| edit.changes_input && drive.passes.sections.contains_key(&edit.origin))
        .unwrap();
    let second = (index + 1..)
        .find(|&index| {
            let edit = edit(SEED, 2, index, window);
            edit.changes_input && edit.origin == first.origin
        })
        .unwrap();
    drive.passes.workload.edit(settings, 1, index, 60);
    drive.results.capacity = Some(0);
    drive.step(settings, 1, 1);
    assert_eq!(drive.results.meshes, prior);
    drive.passes.workload.edit(settings, 2, second, 60);
    drive.results.capacity = None;
    for _ in 0..5 {
        drive.step(settings, 2, 1);
    }
    assert_eq!(drive.passes.workload.pending(settings), 0);
    assert_eq!(drive.results.meshes - prior, 1);
}

#[test]
fn jobs_count_one_reserves_sections_when_an_edit_is_due_each_frame() {
    let run = |jobs| {
        let mut drive = Drive::default();
        let settings = WorkloadSettings {
            radius: 8,
            jobs,
            compare: true,
            budget: Budget::Count(1),
            edits: 60,
            ..WorkloadSettings::default()
        };
        for tick in 0..SECTION_COUNT as u64 {
            drive.step(settings, tick, 64);
        }
        assert_eq!(drive.passes.workload.pending(settings), 0);
        drive.passes.sections.into_keys().collect::<Vec<_>>()
    };
    assert_eq!(run(false), run(true));
}

#[test]
fn jobs_late_frame_reserves_the_unit_for_a_section_over_dirty_blend() {
    let run = |jobs| {
        let mut drive = Drive::default();
        let settings = WorkloadSettings {
            jobs,
            compare: true,
            blend: Blend::Background,
            ..WorkloadSettings::default()
        };
        let time = hook_time(1, false);
        drive.passes.begin_work(settings, 0, 60, &time);
        drive.passes.water.dirty = true;
        drive.step_at(settings, 0, &time);
        assert!(
            drive.passes.water.dirty,
            "blend spent the reserved section unit"
        );
        if jobs {
            assert!(
                !drive.results.queued.is_empty(),
                "the pool was not refilled"
            );
        }
        let before = drive.passes.workload.pending(settings);
        drive.step_at(settings, 1, &time);
        (
            before - drive.passes.workload.pending(settings),
            drive.passes.water.dirty,
        )
    };
    let synchronous = run(false);
    assert_eq!(synchronous, (1, true));
    assert_eq!(run(true), synchronous);
}

#[test]
fn jobs_pool_refills_even_when_the_frame_did_edits_and_more_are_due() {
    let mut drive = Drive::default();
    let settings = WorkloadSettings {
        jobs: true,
        edits: 60,
        ..WorkloadSettings::default()
    };
    let time = hook_time(2, false);
    drive.passes.begin_work(settings, 0, 60, &time);
    drive.step_at(settings, 5, &time);
    assert_eq!(drive.passes.workload.counts().0, 1);
    assert!(!drive.results.queued.is_empty());
}

#[test]
fn jobs_refill_finishes_after_the_installed_sections_and_blend_are_current() {
    for jobs in [false, true] {
        let mut drive = Drive::default();
        let settings = WorkloadSettings {
            blend: Blend::Background,
            ..flying(jobs)
        };
        let mut time = hook_time(64, false);
        time.interval_ms = 16.;
        let report = |drive: &Drive| {
            drive.passes.workload.report(
                settings,
                27,
                Default::default(),
                false,
                Default::default(),
                Default::default(),
            )
        };
        for _ in 0..80 {
            drive.step_at(settings, 0, &time);
        }
        assert_eq!(drive.passes.workload.pending(settings), 0);
        drive.step_at(settings, 27, &time);
        assert!(
            report(&drive)
                .contains(r#""refills":{"started":1,"finishedCount":0,"unfinishedCount":1"#),
            "{}",
            report(&drive)
        );
        for _ in 0..30 {
            drive.step_at(settings, 27, &time);
        }
        assert_eq!(drive.passes.workload.pending(settings), 0);
        assert!(!drive.passes.water.dirty);
        assert_eq!(drive.passes.sections.len(), SECTION_COUNT);
        assert!(
            report(&drive)
                .contains(r#""refills":{"started":1,"finishedCount":1,"unfinishedCount":0"#),
            "{}",
            report(&drive)
        );
    }
}

#[test]
fn jobs_kept_stale_mesh_finishes_its_refill_before_the_repair() {
    let mut drive = Drive::default();
    let settings = WorkloadSettings {
        inflight: Inflight::Keep,
        compare: false,
        ..flying(true)
    };
    let mut time = hook_time(64, false);
    time.interval_ms = 16.;
    for _ in 0..80 {
        drive.step_at(settings, 0, &time);
    }
    assert_eq!(drive.passes.workload.pending(settings), 0);

    drive.results.capacity = Some(1);
    for _ in 0..SECTION_COUNT {
        drive.step_at(settings, 27, &time);
        if drive.passes.workload.pending(settings) == 1 {
            break;
        }
    }
    assert_eq!(drive.passes.workload.pending(settings), 1);
    let (_, _, last) = drive.results.queued.front().unwrap();
    let index = (0..)
        .find(|&index| {
            let edit = edit(SEED, 27, index, settings.window(27, 60));
            edit.origin == last.work[0].origin && edit.changes_input
        })
        .unwrap();
    drive.passes.workload.edit(settings, 27, index, 60);
    drive.step_at(settings, 27, &time);
    let workload = &drive.passes.workload;
    assert_eq!(workload.counts().1, 1);
    assert_eq!(workload.pending(settings), 1);
    let report = workload.report(
        settings,
        27,
        Default::default(),
        false,
        Default::default(),
        Default::default(),
    );
    assert!(
        report.contains(r#""refills":{"started":1,"finishedCount":1,"unfinishedCount":0"#),
        "{report}"
    );
    for _ in 0..2 {
        drive.step_at(settings, 27, &time);
    }
    assert_eq!(drive.passes.workload.pending(settings), 0);
}

#[test]
fn jobs_cpu_peak_counts_a_taken_result_before_installing_it() {
    let settings = WorkloadSettings {
        jobs: true,
        compare: true,
        ..WorkloadSettings::default()
    };
    for seekable in [true, false] {
        let mut drive = Drive::default();
        drive.results.capacity = Some(1);
        let time = hook_time(1, seekable);
        if !seekable {
            drive.step_at(settings, 0, &time);
        }
        drive.step_at(settings, 0, &time);
        assert_eq!(drive.passes.sections.len(), 1);
        let (&origin, section) = drive.passes.sections.first_key_value().unwrap();
        let installed = section.input.as_ref().unwrap().cells().len();
        let mesh = mesh::section_with(origin, height, false, settings.vertex, settings.light);
        let taken = size_of::<Output>()
            + installed
            + mesh.vertices.capacity()
            + mesh.indices.capacity() * size_of::<u32>();
        let report = drive.passes.workload.report(
            settings,
            0,
            Default::default(),
            false,
            Default::default(),
            Default::default(),
        );
        assert!(
            report.contains(&format!(r#""peakCpuBytes":{taken},"#)),
            "{report}"
        );
    }
}

#[test]
fn jobs_device_loss_zeroes_hook_counters_on_either_lane() {
    for jobs in [false, true] {
        let settings = WorkloadSettings {
            jobs,
            ..WorkloadSettings::default()
        };
        let mut drive = Drive::default();
        drive.step(settings, 0, 1);
        drive.passes.section_buffers.stats.created = 3;
        drive.passes.section_buffers.stats.reused = 2;
        drive.passes.section_buffers.stats.bytes_written = 1024;
        drive.passes.device_lost();
        assert!(drive.passes.jobs.is_none());
        assert_eq!(
            drive.passes.section_buffers.stats,
            crate::BufferStats::default()
        );
    }
}

#[test]
fn mesh_wire_runs_match_on_a_used_instance() {
    use exact_game_render::{run_remote_job, WireJob};

    let make = |origin| Input {
        work: Arc::from([Job::mesh_input(origin, true)]),
        previous: Vec::new(),
        settings: WorkloadSettings {
            light: 8000,
            compare: true,
            vertex: 64,
            ..WorkloadSettings::default()
        },
    };
    let dispatch = |job: Input| {
        let mut input = Vec::new();
        job.encode(&mut input);
        let tagged = run_remote_job(Passes::REMOTE, Input::NAME, &input, |_| {}).unwrap();
        assert_eq!(tagged[0], 0);
        let Output::Mesh(candidate) = Input::decode_output(&tagged[1..]).unwrap() else {
            panic!("expected a mesh");
        };
        candidate
    };
    let other = dispatch(make([0, 0, 0]));
    let origin = [16, 48, -16];
    let Output::Mesh(expected) = make(origin).run(&Cancel::default()).unwrap() else {
        panic!("expected a mesh");
    };
    assert_ne!(other.mesh.vertices, expected.mesh.vertices);
    let got = dispatch(make(origin));
    assert_eq!(got.mesh.vertices, expected.mesh.vertices);
    assert_eq!(got.mesh.indices, expected.mesh.indices);
    assert_eq!(
        got.mesh.vertices.capacity(),
        expected.mesh.vertices.capacity()
    );
    assert_eq!(
        got.mesh.indices.capacity(),
        expected.mesh.indices.capacity()
    );
    assert!(expected.input.is_some());
    assert_eq!(got.input, expected.input);
}

#[test]
fn a_staged_install_spends_the_late_worker_post() {
    for staged in [false, true] {
        let settings = WorkloadSettings {
            jobs: true,
            ..WorkloadSettings::default()
        };
        let time = hook_time(1, false);
        let mut drive = Drive::default();
        drive.passes.begin_work(settings, 0, 60, &time);
        let mut job = drive.passes.workload.next_jobs(settings, 0, 60)[0];
        drive.passes.workload.return_jobs(&[job]);
        if staged {
            job.repair = true;
            let candidate = Candidate {
                input: Some(padded_input(job)),
                ..Candidate::default()
            };
            drive.passes.workload.cpu_bytes(0, candidate.cpu_bytes());
            assert!(drive
                .passes
                .workload
                .stage_if_repair(job, candidate)
                .is_ok());
        }
        drive.results.blocked = true;
        drive.passes.begin_work(settings, 1, 60, &time);
        drive.passes.prepare_job_units(
            &mut drive.pending,
            &mut drive.results,
            &Prepare {
                gpu: None,
                time: &time,
                tick: 1,
                hz: 60,
                camera: [0.; 3],
            },
            Lane::Worker,
            |_| false,
        );
        assert_eq!(drive.passes.sections.contains_key(&job.origin), staged);
        assert!(!drive.results.queued.is_empty());
        let queued = drive.results.queued.len();
        assert_eq!(drive.results.post_at_elapsed_deadline().is_some(), !staged);
        assert_eq!(drive.results.queued.len(), queued - usize::from(!staged));
    }
}

#[test]
fn a_result_installed_past_the_deadline_blocks_the_closing_post() {
    let settings = WorkloadSettings {
        jobs: true,
        compare: true,
        ..WorkloadSettings::default()
    };
    let time = hook_time(2, false);
    let mut drive = Drive::default();
    drive.passes.begin_work(settings, 0, 60, &time);
    drive.results.capacity = Some(2);
    for _ in 0..2 {
        drive
            .pending
            .submit_next(&mut drive.passes, &mut drive.results, 0, 60)
            .unwrap();
    }
    let origin = drive.results.queued[0].2.work[0].origin;
    let deadline_passed = |passes: &Passes| passes.sections.contains_key(&origin);
    assert!(!deadline_passed(&drive.passes));
    drive.passes.prepare_job_units(
        &mut drive.pending,
        &mut drive.results,
        &Prepare {
            gpu: None,
            time: &time,
            tick: 0,
            hz: 60,
            camera: [0.; 3],
        },
        Lane::Worker,
        deadline_passed,
    );
    assert!(deadline_passed(&drive.passes));
    assert_eq!(drive.results.meshes, 1);
    assert_eq!(drive.results.queued.len(), 2);
    assert!(drive.results.post_at_elapsed_deadline().is_none());
}
