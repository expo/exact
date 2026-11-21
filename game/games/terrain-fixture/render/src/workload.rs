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
    // A repair that already failed its compare goes straight to meshing.
    repairs: BTreeMap<Origin, bool>,
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
    pub(crate) repair: bool,
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

/// The reuse compares or retirements that make one unit of the allowance. Either is far cheaper
/// than a mesh (a compare measured about 0.01 ms), and a chunk crossing of a full window at
/// radius 40 queues 51,840 compares: counted one a unit, they take at least 810 frames at 64 a
/// frame, whatever time the frames have left. A run shorter than this before a larger unit, or
/// at the end of a call, is not charged, and the deadline is read after every operation.
pub(crate) const SMALL_PER_UNIT: u32 = 64;

impl<T> Unit<T> {
    pub(crate) fn small(&self) -> bool {
        matches!(self, Self::Retire(_))
            || matches!(self, Self::Section(job) if job.kind == SectionWork::Compare)
    }
}

pub(crate) struct FrameBudget {
    remaining: u32,
    small: u32,
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
            small: 0,
            deadline: budget == Budget::Deadline,
            blended: false,
            ran_section: false,
        }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.remaining as usize
    }

    pub(crate) fn received(&mut self, deadline_passed: impl FnOnce() -> bool) {
        self.ran_section = true;
        self.spent(false, deadline_passed);
    }

    pub(crate) fn use_deadline(&mut self) {
        self.deadline = true;
    }

    pub(crate) fn spent(&mut self, small: bool, deadline_passed: impl FnOnce() -> bool) {
        self.small = if small {
            (self.small + 1) % SMALL_PER_UNIT
        } else {
            0
        };
        if self.small == 0 {
            self.remaining -= 1;
        }
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
    jobs_remesh: VecDeque<Job>,
    in_flight: BTreeSet<Origin>,
    flight_repairs: usize,
    generation: u64,
    next_edit: u64,
    frame: u64,
    refills: Refills,
    completed: u64,
    kept: u64,
    discarded: Discarded,
    gpu: Bytes,
    cpu: Bytes,
    job_bytes: u64,
    thermal: Option<&'static str>,
    thermal_changes: u64,
    last_known_thermal: Option<&'static str>,
}

impl<T: Default> Workload<T> {
    pub(crate) fn reset(&mut self) {
        *self = Self {
            generation: self.generation,
            ..Self::default()
        };
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
        self.generation += 1;
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
        let (origin, crossing) = Self::pop_compare(column);
        Some(self.job(origin, SectionWork::Compare, crossing, false))
    }

    fn pop_compare(column: &mut Column) -> (Origin, Option<u64>) {
        let batch = column.compares.front_mut().unwrap();
        let origin = batch.origins.pop().unwrap();
        let crossing = batch.crossing;
        if batch.origins.is_empty() {
            column.compares.pop_front();
        }
        (origin, crossing)
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
        if let Some(unit) = self.next_before_section(budget, settings, tick, hz) {
            return Some(unit);
        }
        if !settings.jobs {
            let job = self
                .remesh
                .take()
                .map(|job| self.job(job.origin, SectionWork::Mesh, job.crossing, job.repair))
                .or_else(|| self.repair_job(settings))
                .or_else(|| self.mesh_job(settings, tick, hz))
                .or_else(|| self.compare_job());
            if let Some(job) = job {
                budget.ran_section = true;
                return Some(Unit::Section(job));
            }
        }
        Self::next_blend(budget, blend_dirty)
    }

    pub(crate) fn can_spend_on_other_work(
        &self,
        budget: &FrameBudget,
        settings: WorkloadSettings,
    ) -> bool {
        budget.remaining > 1
            || budget.ran_section
            || self.pending(settings) == 0 && !self.retiring()
    }

    pub(crate) fn next_before_section(
        &mut self,
        budget: &mut FrameBudget,
        settings: WorkloadSettings,
        tick: u64,
        hz: u32,
    ) -> Option<Unit<T>> {
        if budget.remaining == 0 {
            return None;
        }
        if self.next_edit < edit_count(tick, settings.edits, hz)
            && self.can_spend_on_other_work(budget, settings)
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
        None
    }

    pub(crate) fn next_blend(budget: &mut FrameBudget, blend_dirty: bool) -> Option<Unit<T>> {
        if budget.remaining != 0 && blend_dirty && !budget.blended {
            budget.blended = true;
            return Some(Unit::Blend);
        }
        None
    }

    fn repair_job(&mut self, settings: WorkloadSettings) -> Option<Job> {
        let (origin, mesh) = self.columns.values_mut().find_map(|column| {
            let origin = *column
                .repairs
                .keys()
                .find(|origin| !self.in_flight.contains(*origin))?;
            Some((origin, column.repairs.remove(&origin).unwrap()))
        })?;
        self.repair_count -= 1;
        let kind = if settings.compare && !mesh {
            SectionWork::Compare
        } else {
            SectionWork::Mesh
        };
        Some(self.job(origin, kind, None, true))
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
        if revision.installed {
            self.queue_repair(origin, false);
        }
    }

    fn queue_repair(&mut self, origin: Origin, mesh: bool) {
        let column = self.columns.get_mut(&origin[0].div_euclid(SIDE)).unwrap();
        match column.repairs.entry(origin) {
            Entry::Vacant(entry) => {
                entry.insert(mesh);
                self.repair_count += 1;
            }
            Entry::Occupied(mut entry) => *entry.get_mut() |= mesh,
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
                self.completed += u64::from(settings.jobs && !job.repair);
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
                    + self.flight_repairs
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

    #[cfg(test)]
    pub(crate) fn counts(&self) -> (u64, u64, [u64; 3]) {
        (
            self.completed,
            self.kept,
            [
                self.discarded.stale,
                self.discarded.staged_left,
                self.discarded.queued_left,
            ],
        )
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
            self.cpu.current + self.job_bytes, self.cpu.peak, self.thermal_changes).unwrap();
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
mod tests;

mod jobs;
