use std::collections::{btree_map::Entry, BTreeMap, BTreeSet, VecDeque};
use std::fmt::Write;
use terrain_fixture_logic::{
    edit, edit_count, Blend, Budget, Inflight, Window, WorkloadSettings, LEVELS, SEED, SIDE,
};

pub(crate) type Origin = [i32; 3];

fn x_chunks(window: Window) -> std::ops::RangeInclusive<i32> {
    let x = window.center[0];
    x - window.radius..=x + window.radius
}

fn sections_per_column(window: Window) -> usize {
    ((2 * window.radius + 1) * LEVELS) as usize
}

fn axis_distance(origin: i32, eye: f32) -> f32 {
    let delta = origin as f32 + SIDE as f32 / 2. - eye;
    delta * delta
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SectionWork {
    Mesh,
    Compare,
}

#[derive(Default)]
struct Revision {
    number: u64,
    changed: bool,
    installed: bool,
}

#[derive(Default)]
struct Column {
    next_mesh: usize,
    mesh_crossing: Option<u64>,
    sections: BTreeMap<Origin, Revision>,
    repairs: BTreeSet<Origin>,
    // Each installed origin belongs to either ready or one compare batch.
    ready: Vec<Origin>,
    compares: VecDeque<CompareBatch>,
}

struct CompareBatch {
    crossing: Option<u64>,
    origins: Vec<Origin>,
}

struct MeshPosition {
    y: i32,
    z: i32,
    distance: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Job {
    pub(crate) origin: Origin,
    pub(crate) kind: SectionWork,
    pub(crate) changed: bool,
    revision: u64,
    crossing: Option<u64>,
    repair: bool,
}

struct Staged<T> {
    job: Job,
    value: T,
    frame: u64,
}

pub(crate) enum Unit<T> {
    Retire(Origin),
    Edit { tick: u64, index: u64 },
    Section(Job),
    Install(Job, T),
    DropStaged(T),
    Blend,
}

pub(crate) struct FrameBudget {
    remaining: u32,
    deadline: bool,
    blended: bool,
    ran_section: bool,
}

impl FrameBudget {
    pub(crate) fn new(budget: Budget, allowance: u32) -> Self {
        Self {
            remaining: match budget {
                Budget::Count(n) => allowance.min(n),
                Budget::Deadline => allowance,
            },
            deadline: budget == Budget::Deadline,
            blended: false,
            ran_section: false,
        }
    }

    pub(crate) fn spent(&mut self, deadline_passed: impl FnOnce() -> bool) {
        self.remaining -= 1;
        if self.deadline && deadline_passed() {
            self.remaining = 0;
        }
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Discarded {
    stale: u64,
    staged_left: u64,
    queued_left: u64,
}

#[derive(Default, Debug, PartialEq, Eq)]
struct Bytes {
    current: u64,
    peak: u64,
}

impl Bytes {
    fn replace(&mut self, removed: u64, added: u64) {
        self.current = self.current - removed + added;
        self.peak = self.peak.max(self.current);
    }
}

const OLDEST_CROSSINGS: usize = 4;
// A 60-second flight at one crossing per 800 ms finishes up to 75 on schedule.
const FINISHED_SAMPLES: usize = 128;

#[derive(Clone, Copy, Debug)]
struct Crossing {
    id: u64,
    start_ms: f64,
    pending: usize,
    dropped: usize,
}

fn keep_oldest(crossings: &mut Vec<Crossing>, crossing: Crossing) {
    crossings.push(crossing);
    crossings.sort_unstable_by_key(|crossing| crossing.id);
    crossings.truncate(OLDEST_CROSSINGS);
}

#[derive(Default)]
struct Refills {
    now_ms: f64,
    started: u64,
    pending: BTreeMap<u64, Crossing>,
    abandoned: u64,
    abandoned_oldest: Vec<Crossing>,
    waiting: VecDeque<Crossing>,
    waiting_count: u64,
    waiting_oldest: Vec<Crossing>,
    finished: VecDeque<u64>,
    finished_count: u64,
}

impl Refills {
    fn start(&mut self, interval: f64) -> u64 {
        self.started += 1;
        let id = self.started;
        self.pending.insert(
            id,
            Crossing {
                id,
                start_ms: self.now_ms - interval,
                pending: 0,
                dropped: 0,
            },
        );
        id
    }

    fn resolve(&mut self, id: Option<u64>, count: usize, dropped: bool, blend: bool) {
        let Some(id) = id.filter(|_| count != 0) else {
            return;
        };
        let crossing = self.pending.get_mut(&id).unwrap();
        crossing.pending -= count;
        if dropped {
            crossing.dropped += count;
        }
        if crossing.pending != 0 {
            return;
        }
        let crossing = self.pending.remove(&id).unwrap();
        if crossing.dropped > 0 {
            self.abandoned += 1;
            keep_oldest(&mut self.abandoned_oldest, crossing);
        } else if blend {
            self.waiting_count += 1;
            keep_oldest(&mut self.waiting_oldest, crossing);
            if self.waiting.len() == FINISHED_SAMPLES {
                self.waiting.pop_front();
            }
            self.waiting.push_back(crossing);
        } else {
            self.finished_count += 1;
            self.finish_sample(crossing);
        }
    }

    fn finish_sample(&mut self, crossing: Crossing) {
        if self.finished.len() == FINISHED_SAMPLES {
            self.finished.pop_front();
        }
        self.finished
            .push_back(((self.now_ms - crossing.start_ms) * 1000.).round() as u64);
    }

    fn blend_current(&mut self) -> bool {
        if self.waiting_count == 0 {
            return false;
        }
        self.finished_count += std::mem::take(&mut self.waiting_count);
        while let Some(crossing) = self.waiting.pop_front() {
            self.finish_sample(crossing);
        }
        self.waiting_oldest.clear();
        true
    }

    fn unfinished_count(&self) -> u64 {
        self.pending.len() as u64 + self.abandoned + self.waiting_count
    }

    fn write(&self, out: &mut String) {
        let mut oldest = Vec::new();
        for &crossing in self
            .pending
            .values()
            .take(OLDEST_CROSSINGS)
            .chain(&self.abandoned_oldest)
            .chain(&self.waiting_oldest)
        {
            keep_oldest(&mut oldest, crossing);
        }
        let age = |crossing: &Crossing| (self.now_ms - crossing.start_ms).round() as u64;
        let oldest_age = oldest
            .first()
            .map_or_else(|| "null".into(), |c| age(c).to_string());
        write!(
            out,
            ",\"refills\":{{\"started\":{},\"finishedCount\":{},\"unfinishedCount\":{},\"oldestUnfinishedMs\":{oldest_age},\"finishedUs\":[",
            self.started, self.finished_count, self.unfinished_count()
        )
        .unwrap();
        let mut separator = "";
        for elapsed in &self.finished {
            write!(out, "{separator}{elapsed}").unwrap();
            separator = ",";
        }
        out.push_str("],\"unfinished\":[");
        separator = "";
        for crossing in oldest {
            write!(
                out,
                "{separator}{{\"id\":{},\"ageMs\":{},\"pending\":{},\"dropped\":{}}}",
                crossing.id,
                age(&crossing),
                crossing.pending,
                crossing.dropped
            )
            .unwrap();
            separator = ",";
        }
        out.push_str("]}");
    }
}

#[derive(Default)]
pub(crate) struct Workload<T> {
    window: Option<Window>,
    columns: BTreeMap<i32, Column>,
    mesh_order: Vec<MeshPosition>,
    camera_x: f32,
    retire: VecDeque<BTreeMap<Origin, Revision>>,
    queued: usize,
    repair_count: usize,
    staged: VecDeque<Staged<T>>,
    remesh: Option<Job>,
    next_edit: u64,
    frame: u64,
    refills: Refills,
    completed: u64,
    kept: u64,
    discarded: Discarded,
    gpu: Bytes,
    cpu: Bytes,
    thermal: Option<&'static str>,
    thermal_changes: u64,
    last_known_thermal: Option<&'static str>,
}

impl<T: Default> Workload<T> {
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn advance(
        &mut self,
        settings: WorkloadSettings,
        tick: u64,
        hz: u32,
        live: bool,
        interval: f64,
    ) -> bool {
        self.frame += 1;
        let ages_changed = live && interval > 0. && self.refills.unfinished_count() != 0;
        if live {
            self.refills.now_ms += interval;
        }
        self.camera_x = settings.motion.eye(tick, hz).x;
        let window = settings.window(tick, hz);
        if self.window == Some(window) {
            return ages_changed;
        }

        let sections = sections_per_column(window);
        let old = self.window.replace(window);
        let crossing = if let Some(old) = old {
            // The fixture travels along x and resets the workload when its radius changes.
            debug_assert_eq!(
                (old.center[1], old.radius),
                (window.center[1], window.radius)
            );
            for x in x_chunks(old).filter(|x| !x_chunks(window).contains(x)) {
                self.leave_column(x, sections);
            }
            if let Some(job) = self.remesh.take_if(|job| !window.contains(job.origin)) {
                if job.repair {
                    self.discarded.queued_left += 1;
                } else {
                    self.drop_queued(job.crossing, 1);
                }
            }
            (live && window.center[0].abs_diff(old.center[0]) == 1)
                .then(|| self.refills.start(interval))
        } else {
            self.next_edit = edit_count(tick, settings.edits, hz);
            None
        };

        let mut added = 0;
        for x in x_chunks(window) {
            match self.columns.entry(x) {
                Entry::Vacant(entry) => {
                    entry.insert(Column {
                        mesh_crossing: crossing,
                        ..Column::default()
                    });
                    added += sections;
                }
                Entry::Occupied(mut entry) => {
                    let column = entry.get_mut();
                    if settings.compare && !column.ready.is_empty() {
                        let origins = std::mem::take(&mut column.ready);
                        added += origins.len();
                        column
                            .compares
                            .push_back(CompareBatch { crossing, origins });
                    }
                }
            }
        }
        self.queued += added;
        if let Some(id) = crossing {
            self.refills.pending.get_mut(&id).unwrap().pending = added;
        }
        true
    }

    fn leave_column(&mut self, x: i32, sections: usize) {
        let column = self.columns.remove(&x).unwrap();
        self.drop_queued(column.mesh_crossing, sections - column.next_mesh);
        for batch in column.compares {
            self.drop_queued(batch.crossing, batch.origins.len());
        }
        self.repair_count -= column.repairs.len();
        self.discarded.queued_left += column.repairs.len() as u64;
        if !column.sections.is_empty() {
            self.retire.push_back(column.sections);
        }
    }

    fn drop_queued(&mut self, crossing: Option<u64>, count: usize) {
        self.queued -= count;
        self.discarded.queued_left += count as u64;
        self.refills.resolve(crossing, count, true, false);
    }

    fn mesh_job(&mut self, settings: WorkloadSettings, tick: u64, hz: u32) -> Option<Job> {
        let window = self.window?;
        if self.mesh_order.is_empty() {
            let eye = settings.motion.eye(tick, hz);
            let center = window.center[1];
            for z in center - window.radius..=center + window.radius {
                for y in 0..LEVELS {
                    let (y, z) = (y * SIDE, z * SIDE);
                    self.mesh_order.push(MeshPosition {
                        y,
                        z,
                        distance: axis_distance(y, eye.y) + axis_distance(z, eye.z),
                    });
                }
            }
            self.mesh_order
                .sort_by(|a, b| a.distance.total_cmp(&b.distance));
        }
        let (x, column) = self
            .columns
            .iter_mut()
            .filter(|(_, column)| column.next_mesh < self.mesh_order.len())
            .min_by(|(x, a), (other_x, b)| {
                let distance = |x: i32, column: &Column| {
                    axis_distance(x * SIDE, self.camera_x)
                        + self.mesh_order[column.next_mesh].distance
                };
                distance(**x, a).total_cmp(&distance(**other_x, b))
            })?;
        let pos = &self.mesh_order[column.next_mesh];
        let origin = [*x * SIDE, pos.y, pos.z];
        let crossing = column.mesh_crossing;
        column.next_mesh += 1;
        Some(self.job(origin, SectionWork::Mesh, crossing, false))
    }

    fn compare_job(&mut self) -> Option<Job> {
        let column = self
            .columns
            .values_mut()
            .filter(|column| !column.compares.is_empty())
            .min_by_key(|column| column.compares.front().unwrap().crossing)?;
        let batch = column.compares.front_mut().unwrap();
        let origin = batch.origins.pop().unwrap();
        let crossing = batch.crossing;
        if batch.origins.is_empty() {
            column.compares.pop_front();
        }
        Some(self.job(origin, SectionWork::Compare, crossing, false))
    }

    pub(crate) fn next(
        &mut self,
        budget: &mut FrameBudget,
        settings: WorkloadSettings,
        tick: u64,
        hz: u32,
        blend_dirty: bool,
    ) -> Option<Unit<T>> {
        if budget.remaining == 0 {
            return None;
        }
        let sections_pending = self.pending(settings) > 0 || self.retiring();
        if (budget.remaining > 1 || budget.ran_section || !sections_pending)
            && self.next_edit < edit_count(tick, settings.edits, hz)
        {
            let index = self.next_edit;
            self.next_edit += 1;
            let tick = ((u128::from(index) + 1) * u128::from(hz))
                .div_ceil(u128::from(settings.edits)) as u64;
            return Some(Unit::Edit { tick, index });
        }
        if let Some(column) = self.retire.front_mut() {
            let (origin, _) = column.pop_first().unwrap();
            if column.is_empty() {
                self.retire.pop_front();
            }
            budget.ran_section = true;
            return Some(Unit::Retire(origin));
        }
        let frame = self.frame;
        if let Some(Staged { job, value, .. }) =
            self.staged.pop_front_if(|stage| stage.frame < frame)
        {
            budget.ran_section = true;
            if self.contains(job.origin) {
                return Some(Unit::Install(job, value));
            }
            self.discarded.staged_left += 1;
            return Some(Unit::DropStaged(value));
        }
        let job = self
            .remesh
            .take()
            .map(|job| self.job(job.origin, SectionWork::Mesh, job.crossing, job.repair))
            .or_else(|| {
                let origin = self
                    .columns
                    .values_mut()
                    .find_map(|column| column.repairs.pop_first())?;
                self.repair_count -= 1;
                let kind = if settings.compare {
                    SectionWork::Compare
                } else {
                    SectionWork::Mesh
                };
                Some(self.job(origin, kind, None, true))
            })
            .or_else(|| self.mesh_job(settings, tick, hz))
            .or_else(|| self.compare_job());
        if let Some(job) = job {
            budget.ran_section = true;
            return Some(Unit::Section(job));
        }
        if blend_dirty && !budget.blended {
            budget.blended = true;
            return Some(Unit::Blend);
        }
        None
    }

    pub(crate) fn edit(&mut self, settings: WorkloadSettings, tick: u64, index: u64, hz: u32) {
        let edit = edit(SEED, tick, index, settings.window(tick, hz));
        if self.contains(edit.origin) {
            self.invalidate(edit.origin, edit.changes_input);
        }
        self.completed += 1;
    }

    fn invalidate(&mut self, origin: Origin, changes_input: bool) {
        let column = self.columns.get_mut(&origin[0].div_euclid(SIDE)).unwrap();
        let revision = column.sections.entry(origin).or_default();
        revision.number += 1;
        revision.changed ^= changes_input;
        if revision.installed && column.repairs.insert(origin) {
            self.repair_count += 1;
        }
    }

    fn revision(&self, origin: Origin) -> Option<&Revision> {
        self.columns
            .get(&origin[0].div_euclid(SIDE))?
            .sections
            .get(&origin)
    }

    fn job(&self, origin: Origin, kind: SectionWork, crossing: Option<u64>, repair: bool) -> Job {
        let (revision, changed) = self
            .revision(origin)
            .map_or((0, false), |r| (r.number, r.changed));
        Job {
            origin,
            kind,
            changed,
            revision,
            crossing,
            repair,
        }
    }

    pub(crate) fn compared(&mut self, job: Job, equal: bool, blend: Blend) {
        self.completed += 1;
        if equal {
            if !job.repair {
                self.ready_to_compare(job.origin);
            }
            self.resolve(job, blend);
        } else {
            self.remesh = Some(Job {
                kind: SectionWork::Mesh,
                ..job
            });
        }
    }

    pub(crate) fn stage_if_repair(&mut self, job: Job, value: T) -> Result<(), T> {
        if !job.repair {
            return Err(value);
        }
        self.completed += 1;
        self.staged.push_back(Staged {
            job,
            value,
            frame: self.frame,
        });
        Ok(())
    }

    pub(crate) fn install(&mut self, job: Job, settings: WorkloadSettings) -> bool {
        let stale = self.revision(job.origin).map_or(0, |r| r.number) != job.revision;
        if stale {
            if settings.inflight == Inflight::Discard {
                self.discarded.stale += 1;
                return false;
            }
            self.kept += 1;
        }
        let column = self
            .columns
            .get_mut(&job.origin[0].div_euclid(SIDE))
            .unwrap();
        column.sections.entry(job.origin).or_default().installed = true;
        if settings.compare && !job.repair {
            self.ready_to_compare(job.origin);
        }
        self.completed += 1;
        self.resolve(job, settings.blend);
        true
    }

    fn ready_to_compare(&mut self, origin: Origin) {
        self.columns
            .get_mut(&origin[0].div_euclid(SIDE))
            .unwrap()
            .ready
            .push(origin);
    }

    fn resolve(&mut self, job: Job, blend: Blend) {
        if !job.repair {
            self.queued -= 1;
            self.refills
                .resolve(job.crossing, 1, false, blend != Blend::Off);
        }
    }

    pub(crate) fn blend_current(&mut self) -> bool {
        self.refills.blend_current()
    }

    pub(crate) fn blended(&mut self) {
        self.completed += 1;
        self.blend_current();
    }

    pub(crate) fn retired(&mut self) {
        self.completed += 1;
    }

    pub(crate) fn contains(&self, origin: Origin) -> bool {
        self.window.is_some_and(|window| window.contains(origin))
    }

    pub(crate) fn draw_eligible(&self, installed: Origin) -> bool {
        // Keep the installed mesh visible while its replacement is prepared (LLP 1182 §2).
        self.contains(installed)
    }

    pub(crate) fn pending(&self, settings: WorkloadSettings) -> usize {
        self.window.map_or_else(
            || {
                let window = settings.window(0, 60);
                x_chunks(window).count() * sections_per_column(window)
            },
            |_| {
                self.queued
                    + self.repair_count
                    + self.staged.len()
                    + usize::from(self.remesh.is_some_and(|job| job.repair))
            },
        )
    }

    pub(crate) fn retiring(&self) -> bool {
        !self.retire.is_empty()
    }

    pub(crate) fn gpu_bytes(&mut self, removed: u64, added: u64) {
        self.gpu.replace(removed, added);
    }

    pub(crate) fn cpu_bytes(&mut self, removed: u64, added: u64) {
        self.cpu.replace(removed, added);
    }

    pub(crate) fn thermal(&mut self, state: Option<&'static str>) -> bool {
        if self.thermal == state {
            return false;
        }
        if state.is_some() {
            if self.last_known_thermal.is_some() && self.last_known_thermal != state {
                self.thermal_changes += 1;
            }
            self.last_known_thermal = state;
        }
        self.thermal = state;
        true
    }

    pub(crate) fn report(
        &self,
        settings: WorkloadSettings,
        tick: u64,
        drew: bool,
        uploads: u64,
        timestamp_queries: bool,
        buffers: super::BufferStats,
    ) -> String {
        let mut report = String::new();
        let crossing_interval = settings
            .motion
            .crossing_interval_ms()
            .map_or_else(|| "null".into(), |ms| ms.to_string());
        let thermal = self
            .thermal
            .map_or_else(|| "null".into(), |state| format!("\"{state}\""));
        write!(report, "{{\"tick\":{tick},\"pending\":{},\"completed\":{},\"kept\":{},\"drew\":{drew},\"lightCells\":{},\"uploadBytes\":{uploads},\"discarded\":{{\"stale\":{},\"stagedLeft\":{},\"queuedLeft\":{}}},\"gpuBufferBytes\":{},\"peakGpuBufferBytes\":{},\"cpuBytes\":{},\"peakCpuBytes\":{},\"crossingIntervalMs\":{crossing_interval},\"timestampQueries\":{timestamp_queries},\"thermalState\":{thermal},\"thermalChanges\":{}",
            self.pending(settings), self.completed, self.kept, settings.light, self.discarded.stale,
            self.discarded.staged_left, self.discarded.queued_left, self.gpu.current, self.gpu.peak,
            self.cpu.current, self.cpu.peak, self.thermal_changes).unwrap();
        write!(
            report,
            ",\"sectionBuffers\":{{\"created\":{},\"reused\":{},\"destroyed\":{},\"bytesWritten\":{}}}",
            buffers.created, buffers.reused, buffers.destroyed, buffers.bytes_written
        )
        .unwrap();
        self.refills.write(&mut report);
        report.push('}');
        report
    }
}

#[cfg(test)]
mod tests {
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
        budget.spent(|| false);
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
        let report = work.report(settings, 27 + 48 * 999, false, 0, false, Default::default());
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
        let report = work.report(settings, 27 + 48 * 999, false, 0, false, Default::default());
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
            section_units(&mut work, settings, tick, 64);
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
        late.spent(|| true);
        assert!(work.next(&mut late, settings, 0, 60, true).is_none());

        let mut deadline = FrameBudget::new(Budget::Deadline, 64);
        deadline.spent(|| true);
        assert_eq!(deadline.remaining, 0);
        let mut count = FrameBudget::new(Budget::Count(8), 64);
        count.spent(|| panic!("count budget read the clock"));
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
        section_units(&mut work, settings, 27, entering as u32 + 1);
        work.advance(settings, 75, 60, true, 16.);
        assert_eq!(work.refills.started, 2);
        assert_eq!(work.refills.pending[&1].pending, entering - 1);
        assert_eq!(work.refills.pending[&2].pending, entering);
        let report = work.report(settings, 75, false, 0, true, Default::default());
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
            let Some(Unit::Install(job, ())) = take(&mut work, &mut budget, settings, 1, false)
            else {
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
                section_units(&mut work, settings, 27, 137);
                work.advance(settings, 75, 60, true, 16.);
                let buffers = super::super::BufferStats {
                    created: 94,
                    reused: 17,
                    destroyed: 1,
                    bytes_written: 7104,
                };
                Self(work.report(settings, 75, false, 0, false, buffers))
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
}
