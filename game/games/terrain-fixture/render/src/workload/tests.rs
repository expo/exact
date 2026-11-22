use super::*;
use terrain_fixture_logic::{Motion, SECTION_COUNT};

fn flight(compare: bool) -> WorkloadSettings {
    WorkloadSettings {
        radius: 8,
        motion: Motion::Fly,
        compare,
        ..WorkloadSettings::default()
    }
}

fn take(
    work: &mut Workload<()>,
    budget: &mut FrameBudget,
    settings: WorkloadSettings,
    tick: u64,
    dirty: bool,
) -> Option<Unit<()>> {
    let unit = work.next(budget, settings, tick, 60, dirty)?;
    budget.spent(unit.small(), || false);
    Some(unit)
}

fn section_units(
    work: &mut Workload<()>,
    settings: WorkloadSettings,
    tick: u64,
    allowance: u32,
) -> [usize; 2] {
    let mut budget = FrameBudget::new(settings.budget, allowance);
    let mut counts = [0, 0];
    while let Some(unit) = take(work, &mut budget, settings, tick, false) {
        match unit {
            Unit::Section(job) if job.kind == SectionWork::Compare => {
                counts[0] += 1;
                work.compared(job, true, settings.blend);
            }
            Unit::Section(job) => {
                counts[1] += 1;
                assert!(work.install(job, settings));
            }
            Unit::Retire(_) => work.retired(),
            _ => panic!("unexpected work"),
        }
    }
    counts
}

#[test]
fn decision_empty_flight_queues_each_section_once() {
    let settings = WorkloadSettings {
        radius: 40,
        ..flight(true)
    };
    let mut work = Workload::default();
    work.advance(settings, 0, 60, true, 0.);
    for crossing in 0..10 {
        work.advance(settings, 27 + 48 * crossing, 60, true, 800.);
    }
    let window = 81 * 81 * 8;
    let pending = work.pending(settings);
    assert!(!work.retiring());
    assert_eq!(work.discarded.queued_left, 10 * 648);
    let mut budget = FrameBudget::new(settings.budget, 1);
    let Some(Unit::Section(job)) = take(&mut work, &mut budget, settings, 459, false) else {
        panic!("a missing section should mesh first");
    };
    assert_eq!(job.kind, SectionWork::Mesh);
    let eye = settings.motion.eye(459, 60).to_array();
    let center_distance = |origin: Origin| {
        origin
            .into_iter()
            .zip(eye)
            .map(|(axis, eye)| {
                let delta = (axis + SIDE / 2) as f32 - eye;
                delta * delta
            })
            .sum::<f32>()
    };
    let nearest = settings
        .window(459, 60)
        .sections()
        .map(center_distance)
        .min_by(f32::total_cmp)
        .unwrap();
    assert_eq!(center_distance(job.origin), nearest);
    assert!(work.install(job, settings));
    let [compares, meshes] = section_units(&mut work, settings, 459, 100_000);
    assert_eq!((pending, compares, meshes + 1), (window, 0, window));
}

#[test]
fn decision_full_flight_queues_only_new_work_at_each_crossing() {
    let settings = flight(true);
    let mut work = Workload::default();
    work.advance(settings, 0, 60, true, 0.);
    section_units(&mut work, settings, 0, 3000);
    work.advance(settings, 27, 60, true, 800.);
    assert_eq!(work.pending(settings), 136 + 2176);
    work.advance(settings, 75, 60, true, 800.);
    assert_eq!(work.refills.pending[&2].pending, 136);
    assert_eq!(section_units(&mut work, settings, 75, 3000), [2040, 272]);
}

#[test]
fn decision_installed_sections_stay_drawable_during_compare_and_rebuild() {
    let settings = flight(true);
    let mut work = Workload::default();
    let initial = settings.window(0, 60);
    work.advance(settings, 0, 60, true, 0.);
    assert_eq!(
        section_units(&mut work, settings, 0, 3000),
        [0, SECTION_COUNT]
    );
    let shifted = settings.window(27, 60);
    work.advance(settings, 27, 60, true, 800.);
    assert_eq!(
        initial
            .sections()
            .filter(|&origin| work.draw_eligible(origin))
            .count(),
        2176
    );
    let mut budget = FrameBudget::new(settings.budget, 3000);
    while let Some(unit) = take(&mut work, &mut budget, settings, 27, false) {
        match unit {
            Unit::Retire(_) => work.retired(),
            Unit::Section(job) if job.kind == SectionWork::Mesh => {
                assert!(work.install(job, settings));
            }
            Unit::Section(job) => {
                assert!(shifted.contains(job.origin));
                work.compared(job, false, settings.blend);
                assert_eq!(work.remesh.unwrap().origin, job.origin);
                assert!(initial.contains(job.origin) && work.draw_eligible(job.origin));
                return;
            }
            _ => panic!("unexpected work"),
        }
    }
    panic!("expected a compare");
}

#[test]
fn decision_flight_shows_current_sections_with_sixty_four_units_a_frame() {
    use terrain_fixture_logic::height;

    let settings = WorkloadSettings {
        radius: 40,
        ..flight(true)
    };
    let mut work = Workload::default();
    let mut installed = BTreeSet::new();
    let mut peak = 0;
    for frame in 0..600 {
        let tick = if frame < 12 {
            0
        } else {
            27 + 48 * (frame / 12 - 1)
        };
        work.advance(settings, tick, 60, true, 1000. / 60.);
        let mut budget = FrameBudget::new(settings.budget, 64);
        while let Some(unit) = take(&mut work, &mut budget, settings, tick, false) {
            match unit {
                Unit::Section(job) if job.kind == SectionWork::Compare => {
                    work.compared(job, installed.contains(&job.origin), settings.blend);
                }
                Unit::Section(job) => {
                    assert!(work.install(job, settings));
                    if !crate::mesh::section_with(job.origin, height, false, 12, 0)
                        .indices
                        .is_empty()
                    {
                        installed.insert(job.origin);
                    }
                }
                Unit::Retire(origin) => {
                    if !work.contains(origin) {
                        installed.remove(&origin);
                    }
                    work.retired();
                }
                _ => panic!("unexpected work"),
            }
        }
        peak = peak.max(work.pending(settings));
    }
    let eligible = installed
        .into_iter()
        .filter(|&origin| work.draw_eligible(origin))
        .count();
    println!("{eligible} draw-eligible sections at frame 600 (peak backlog {peak})");
    assert!(
        eligible > 0,
        "{eligible} draw-eligible sections at frame 600 (peak backlog {peak})"
    );
    assert!(
        peak <= 81 * 81 * 8,
        "peak backlog {peak}, draw-eligible sections {eligible}"
    );
}

#[test]
fn decision_report_is_bounded_after_a_thousand_crossings() {
    let settings = flight(true);
    let mut work = Workload::<()>::default();
    work.advance(settings, 0, 60, true, 0.);
    for crossing in 0..1000 {
        work.advance(settings, 27 + 48 * crossing, 60, true, 800.);
    }
    let report = work.report(
        settings,
        27 + 48 * 999,
        Default::default(),
        false,
        Default::default(),
        Default::default(),
    );
    assert!(report.len() < 8192, "report grew to {} bytes", report.len());
    assert_eq!(report.matches(r#""ageMs":"#).count(), 4);
    assert!(report.contains(r#""unfinishedCount":1000"#));
    assert!(report.contains(r#""oldestUnfinishedMs":800000"#));
    for _ in 0..1000 {
        let id = work.refills.start(800.);
        work.refills.pending.get_mut(&id).unwrap().pending = 1;
        work.refills.resolve(Some(id), 1, false, false);
    }
    assert_eq!(work.refills.finished.len(), FINISHED_SAMPLES);
    assert_eq!(work.refills.finished_count, 1000);
    let report = work.report(
        settings,
        27 + 48 * 999,
        Default::default(),
        false,
        Default::default(),
        Default::default(),
    );
    assert!(report.len() < 8192, "report grew to {} bytes", report.len());
}

#[test]
fn decision_crossing_pending_accounts_for_the_queue() {
    let settings = flight(true);
    let mut work = Workload::default();
    work.advance(settings, 0, 60, true, 0.);
    section_units(&mut work, settings, 0, 3000);
    for crossing in 0..18 {
        let tick = 27 + 48 * crossing;
        work.advance(settings, tick, 60, true, 800.);
        // One unit is 64 of a crossing's 136 retirements: the queue falls behind.
        section_units(&mut work, settings, tick, 1);
    }
    let pending = work
        .refills
        .pending
        .values()
        .map(|crossing| crossing.pending)
        .sum::<usize>();
    assert_eq!(pending, work.queued);
    assert_eq!(work.queued, work.pending(settings));
    assert_eq!(work.refills.abandoned_oldest[0].id, 1);
    assert_eq!(work.refills.abandoned_oldest[0].dropped, 2312);
    assert!(work.discarded.queued_left > 0);
}

#[test]
fn decision_shift_compares_each_retained_address_and_meshes_each_entering_address() {
    for compare in [false, true] {
        let settings = flight(compare);
        let mut work = Workload::default();
        work.advance(settings, 0, 60, true, 0.);
        assert_eq!(
            section_units(&mut work, settings, 0, 3000),
            [0, SECTION_COUNT]
        );
        work.advance(settings, 27, 60, true, 16.);
        let entering = 17 * 8;
        assert_eq!(
            section_units(&mut work, settings, 27, 3000),
            [if compare { SECTION_COUNT - entering } else { 0 }, entering]
        );
    }
}

#[test]
fn decision_sixty_four_compares_or_retirements_spend_one_unit() {
    let settings = flight(true);
    // After a one-chunk shift: 136 retirements are two units and a remainder that costs
    // nothing, 136 meshes are 136 units and 2,176 compares are 34.
    for (allowance, compares) in [(172, 2176), (171, 2112)] {
        let mut work = Workload::default();
        work.advance(settings, 0, 60, true, 0.);
        section_units(&mut work, settings, 0, 3000);
        work.advance(settings, 27, 60, true, 16.);
        assert_eq!(
            section_units(&mut work, settings, 27, allowance),
            [compares, 17 * 8]
        );
    }
}

#[test]
fn decision_edits_before_they_are_due_add_no_fill_units() {
    for edits in [0, 4] {
        let settings = WorkloadSettings {
            edits,
            ..WorkloadSettings::default()
        };
        let mut work = Workload::default();
        work.advance(settings, 0, 60, false, 0.);
        assert_eq!(
            section_units(&mut work, settings, 0, 3000),
            [0, SECTION_COUNT]
        );
        assert_eq!(work.completed, SECTION_COUNT as u64);
        assert_eq!(work.pending(settings), 0);
    }
}

#[test]
fn decision_a_late_unit_goes_to_the_section_and_count_ignores_the_deadline() {
    let settings = WorkloadSettings {
        blend: Blend::Background,
        ..WorkloadSettings::default()
    };
    let mut work = Workload::<()>::default();
    work.advance(settings, 0, 60, true, 0.);
    let mut late = FrameBudget::new(Budget::Deadline, 1);
    assert!(matches!(
        work.next(&mut late, settings, 0, 60, true),
        Some(Unit::Section(_))
    ));
    late.spent(false, || true);
    assert!(work.next(&mut late, settings, 0, 60, true).is_none());

    // A passed deadline ends a run of small operations at its first.
    let mut small = FrameBudget::new(Budget::Deadline, 64);
    small.spent(true, || true);
    assert_eq!(small.remaining, 0);
    let mut small = FrameBudget::new(Budget::Deadline, 2);
    for _ in 0..SMALL_PER_UNIT - 1 {
        small.spent(true, || false);
    }
    assert_eq!(small.remaining, 2);
    small.spent(true, || false);
    assert_eq!(small.remaining, 1);

    let mut deadline = FrameBudget::new(Budget::Deadline, 64);
    deadline.spent(false, || true);
    assert_eq!(deadline.remaining, 0);
    let mut count = FrameBudget::new(Budget::Count(8), 64);
    count.spent(false, || panic!("count budget read the clock"));
    assert_eq!(count.remaining, 7);
}

#[test]
fn decision_late_start_skips_old_edits_and_caps_due_work() {
    let settings = WorkloadSettings {
        edits: 4,
        ..WorkloadSettings::default()
    };
    let mut work = Workload::default();
    work.advance(settings, 600, 60, true, 16.);
    assert_eq!(section_units(&mut work, settings, 600, 1), [0, 1]);
    assert_eq!(work.next_edit, 40);
    assert!(work
        .columns
        .values()
        .flat_map(|c| c.sections.values())
        .all(|r| r.number == 0));

    section_units(&mut work, settings, 600, 3000);
    work.advance(settings, 660, 60, true, 16.);
    let mut budget = FrameBudget::new(settings.budget, 1);
    let Some(Unit::Edit { tick, index }) = take(&mut work, &mut budget, settings, 660, false)
    else {
        panic!("one due edit")
    };
    work.edit(settings, tick, index, 60);
    assert!(take(&mut work, &mut budget, settings, 660, false).is_none());
    assert_eq!(work.next_edit, 41);
}

#[test]
fn decision_crossings_keep_separate_timers_and_include_the_crossing_frame() {
    let settings = flight(false);
    let mut work = Workload::default();
    work.advance(settings, 0, 60, true, 0.);
    section_units(&mut work, settings, 0, 3000);

    let entering = 17 * 8;
    work.advance(settings, 27, 60, true, 12.);
    // Two units retire the sections that left; the third is one mesh.
    section_units(&mut work, settings, 27, 3);
    work.advance(settings, 75, 60, true, 16.);
    assert_eq!(work.refills.started, 2);
    assert_eq!(work.refills.pending[&1].pending, entering - 1);
    assert_eq!(work.refills.pending[&2].pending, entering);
    let report = work.report(
        settings,
        75,
        Default::default(),
        true,
        Default::default(),
        Default::default(),
    );
    assert!(
        report.contains(r#""id":1,"ageMs":28,"pending":135"#),
        "{report}"
    );
    assert!(
        report.contains(r#""id":2,"ageMs":16,"pending":136"#),
        "{report}"
    );

    for _ in 0..3 * entering {
        section_units(&mut work, settings, 75, 1);
        if !work.refills.finished.is_empty() {
            break;
        }
    }
    assert_eq!(work.refills.finished[0], 28_000);
    assert!((1..=entering).contains(&work.refills.pending[&2].pending));
    work.advance(settings, 76, 60, true, 17.);
    section_units(&mut work, settings, 76, 3000);
    assert_eq!(work.refills.finished[1], 33_000);
}

#[test]
fn decision_refill_waits_for_the_blended_upload() {
    let settings = WorkloadSettings {
        blend: Blend::Background,
        ..flight(false)
    };
    let mut work = Workload::default();
    work.advance(settings, 0, 60, true, 0.);
    section_units(&mut work, settings, 0, 3000);
    work.advance(settings, 27, 60, true, 16.);
    section_units(&mut work, settings, 27, 3000);
    assert!(work.refills.finished.is_empty());
    assert_eq!(work.refills.waiting[0].id, 1);
    work.advance(settings, 28, 60, true, 20.);
    let mut budget = FrameBudget::new(settings.budget, 64);
    assert!(matches!(
        take(&mut work, &mut budget, settings, 28, true),
        Some(Unit::Blend)
    ));
    work.blended();
    assert_eq!(work.refills.finished[0], 36_000);
}

#[test]
fn decision_discard_counters_keep_stale_staged_and_queued_work_distinct() {
    let settings = flight(false);
    let mut work = Workload::default();
    work.advance(settings, 0, 60, true, 0.);
    let leaving = settings.window(0, 60).sections().next().unwrap();
    let staying = [leaving[0] + SIDE, leaving[1], leaving[2]];
    for policy in [Inflight::Discard, Inflight::Keep] {
        let job = work.job(staying, SectionWork::Mesh, None, true);
        work.stage_if_repair(job, ()).unwrap();
        work.invalidate(staying, true);
        work.advance(settings, 1, 60, true, 12.);
        let mut budget = FrameBudget::new(settings.budget, 1);
        let Some(Unit::Install(job, ())) = take(&mut work, &mut budget, settings, 1, false) else {
            panic!("stale staged result")
        };
        assert_eq!(
            work.install(
                job,
                WorkloadSettings {
                    inflight: policy,
                    ..settings
                }
            ),
            policy == Inflight::Keep
        );
    }
    let job = work.job(leaving, SectionWork::Mesh, None, true);
    work.stage_if_repair(job, ()).unwrap();
    work.advance(settings, 27, 60, true, 16.);
    let mut budget = FrameBudget::new(settings.budget, 200);
    assert!(matches!(
        take(&mut work, &mut budget, settings, 27, false),
        Some(Unit::DropStaged(()))
    ));
    assert!(matches!(
        take(&mut work, &mut budget, settings, 27, false),
        Some(Unit::Section(_))
    ));
    assert_eq!(
        work.discarded,
        Discarded {
            stale: 1,
            staged_left: 1,
            queued_left: 17 * 8
        }
    );
    assert_eq!(work.kept, 1);
}

#[test]
fn decision_report_survives_a_state_read_without_a_device() {
    use exact_game_render::{Hooks, WorldSurface};
    use exact_gpu::{Surface, Value};
    use terrain_fixture_logic::TerrainFixture;

    struct Reporter(String);
    impl Default for Reporter {
        fn default() -> Self {
            let settings = flight(false);
            let mut work = Workload::default();
            work.advance(settings, 0, 60, true, 0.);
            section_units(&mut work, settings, 0, 3000);
            work.advance(settings, 27, 60, true, 12.);
            section_units(&mut work, settings, 27, 3);
            work.advance(settings, 75, 60, true, 16.);
            let buffers = super::super::BufferStats {
                created: 94,
                reused: 17,
                destroyed: 1,
                bytes_written: 7104,
            };
            Self(work.report(
                settings,
                75,
                Default::default(),
                false,
                buffers,
                Default::default(),
            ))
        }
    }
    impl Hooks for Reporter {
        fn report(&self) -> Option<&str> {
            Some(&self.0)
        }
    }

    let mut surface = WorldSurface::<TerrainFixture, (), false, Reporter>::default();
    let args = [
        "8", "fly", "0", "0", "discard", "off", "12", "off", "deadline", "on", "off",
    ];
    surface.bind(&args.map(Value::str), None).unwrap();
    let state = surface.agent(r#"{"op":"state"}"#).unwrap();
    assert!(
        state.contains(r#""renderHooks":{"game":{"tick":75,"pending":271"#),
        "{state}"
    );
    assert!(
        state.contains(r#""id":1,"ageMs":28,"pending":135"#),
        "{state}"
    );
    assert!(
        state.contains(
            r#""sectionBuffers":{"created":94,"reused":17,"destroyed":1,"bytesWritten":7104}"#
        ),
        "{state}"
    );
}

#[test]
fn decision_reset_clears_work_and_counters() {
    let settings = WorkloadSettings {
        edits: 4,
        ..flight(true)
    };
    let mut work = Workload::default();
    work.advance(settings, 600, 60, true, 16.);
    work.advance(settings, 640, 60, true, 16.);
    let origin = settings.window(640, 60).sections().next().unwrap();
    work.invalidate(origin, true);
    let job = work.job(origin, SectionWork::Mesh, Some(1), true);
    work.stage_if_repair(job, ()).unwrap();
    work.gpu_bytes(0, 100);
    work.cpu_bytes(0, 200);
    work.discarded.stale = 1;
    work.kept = 1;
    work.reset();
    assert!(work.window.is_none());
    assert!(work.columns.is_empty() && work.retire.is_empty() && work.staged.is_empty());
    assert_eq!(work.repair_count, 0);
    assert_eq!(
        (
            work.next_edit,
            work.completed,
            work.kept,
            work.refills.started
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(work.gpu, Bytes::default());
    assert_eq!(work.cpu, Bytes::default());
    assert_eq!(work.discarded, Discarded::default());
}

#[test]
fn decision_a_crossing_can_compare_against_a_staged_repair_without_jobs() {
    let settings = flight(true);
    let mut work = Workload::<()>::default();
    work.advance(settings, 0, 60, true, 0.);
    section_units(&mut work, settings, 0, 3000);
    work.advance(settings, 27, 60, true, 16.);
    let origin = *work
        .columns
        .values()
        .find_map(|column| column.compares.front()?.origins.last())
        .unwrap();
    work.invalidate(origin, true);
    let repair = work.repair_job(settings).unwrap();
    assert_eq!(repair.origin, origin);
    work.compared(repair, false, settings.blend);
    let staged = work.remesh.take().unwrap();
    work.stage_if_repair(staged, ()).unwrap();

    let reuse = work.compare_job().unwrap();
    assert_eq!(
        (reuse.origin, reuse.revision),
        (staged.origin, staged.revision)
    );
    work.compared(reuse, false, settings.blend);
    let duplicate = work.remesh.unwrap();
    assert_eq!(
        (duplicate.origin, duplicate.revision),
        (staged.origin, staged.revision)
    );
}
