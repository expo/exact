use crate::workload::{FrameBudget, Job, Origin, SectionWork, Workload};
use crate::{height, mesh, Candidate, PaddedInput, Passes, WorkloadSettings, SIDE};
use exact_game_render::{
    Cancel, Done, FrameView, Full, HookGpu, HookTime, Job as RenderJob, JobError, Jobs, Lane,
    Limits, Take,
};
use std::collections::BTreeMap;
use std::sync::Arc;

const LIMITS: Limits = Limits {
    queued_bytes: 32 * 1024,
    running: 4,
    completed_bytes: 32 * 1024 * 1024,
    take_per_frame: <Passes as exact_game_render::Hooks>::ALLOWANCE as usize,
};

type Batch = Arc<[Job]>;

struct Input {
    work: Batch,
    previous: Vec<Option<Arc<Vec<u8>>>>,
    settings: WorkloadSettings,
}

enum Output {
    Mesh(Candidate),
    Compared(u64),
}

impl RenderJob for Input {
    type Output = Output;

    fn bytes(&self) -> usize {
        // Previous inputs are still counted as resident sections.
        size_of::<Self>()
            + std::mem::size_of_val(&*self.work)
            + self.previous.capacity() * size_of::<Option<Arc<Vec<u8>>>>()
    }

    fn output_bytes(output: &Output) -> usize {
        match output {
            Output::Mesh(candidate) => size_of::<Output>() + candidate.cpu_bytes() as usize,
            Output::Compared(_) => size_of::<Output>(),
        }
    }

    fn run(self, cancel: &Cancel) -> Result<Output, JobError> {
        let first = self.work[0];
        if first.kind == SectionWork::Mesh {
            let mesh = mesh::section_with_cancel(
                first.origin,
                height,
                first.changed,
                self.settings.vertex,
                self.settings.light,
                || cancel.is_set(),
            )
            .ok_or_else(|| JobError("terrain mesh cancelled".into()))?;
            return Ok(Output::Mesh(Candidate {
                input: self.settings.compare.then(|| padded_input(first)),
                mesh,
            }));
        }
        let mut equal = 0;
        for (index, (&job, old)) in self.work.iter().zip(self.previous).enumerate() {
            if cancel.is_set() {
                return Err(JobError("terrain compare cancelled".into()));
            }
            let same = old.is_some_and(|old| old.as_slice() == &*padded_input(job));
            equal |= u64::from(same) << index;
        }
        Ok(Output::Compared(equal))
    }
}

fn padded_input(job: Job) -> Box<[u8]> {
    mesh::padded_input(job.origin, height, job.changed)
}

fn key([x, y, z]: Origin) -> u64 {
    // Aligned z leaves the low bits for the vertical section.
    (u64::from(x as u32) << 32) | u64::from(z as u32) | (y / SIDE) as u64
}

fn priority(job: Job, settings: WorkloadSettings, tick: u64, hz: u32) -> i32 {
    if job.kind == SectionWork::Compare && !job.repair {
        return i32::MIN;
    }
    let eye = settings.motion.eye(tick, hz).to_array();
    let distance = job
        .origin
        .into_iter()
        .zip(eye)
        .map(|(origin, eye)| {
            let delta = origin as f32 + SIDE as f32 / 2. - eye;
            delta * delta
        })
        .sum::<f32>();
    let distance = (distance as i32).min(i32::MAX / 2);
    if job.repair {
        i32::MAX / 2 - distance
    } else {
        -distance
    }
}

pub(crate) struct TerrainJobs {
    queue: Jobs<Input>,
    pending: PendingJobs,
}

#[derive(Default)]
struct PendingJobs {
    flight: BTreeMap<u64, Batch>,
}

trait Results: Iterator<Item = Done<Output>> {
    fn submit(
        &mut self,
        key: u64,
        generation: u64,
        priority: i32,
        input: Input,
    ) -> Result<(), Full<Input>>;
    fn sample_retained_bytes(&mut self) -> (usize, usize);
}

impl Results for Take<'_, Input> {
    fn submit(
        &mut self,
        key: u64,
        generation: u64,
        priority: i32,
        input: Input,
    ) -> Result<(), Full<Input>> {
        self.submit(key, generation, priority, input)
    }

    fn sample_retained_bytes(&mut self) -> (usize, usize) {
        self.sample_retained_bytes()
    }
}

struct Prepare<'a, 'gpu> {
    gpu: Option<&'a HookGpu<'gpu>>,
    time: &'a HookTime,
    tick: u64,
    hz: u32,
    camera: [f32; 3],
}

impl Prepare<'_, '_> {
    fn deadline_passed(&self) -> bool {
        self.time.deadline.is_some_and(|deadline| deadline.passed())
    }
}

impl TerrainJobs {
    pub(crate) fn new() -> Self {
        Self {
            queue: Jobs::new(LIMITS),
            pending: PendingJobs::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.queue.clear();
        self.pending = PendingJobs::default();
    }

    pub(crate) fn report(&self) -> exact_game_render::JobsReport {
        self.queue.report()
    }
}

impl PendingJobs {
    fn reconcile(&mut self, workload: &mut Workload<Candidate>, mut cancel: impl FnMut(u64)) {
        workload.drop_leaving_remeshes();
        self.flight.retain(|&key, batch| {
            if workload.contains(batch[0].origin) {
                return true;
            }
            cancel(key);
            for &job in batch.iter() {
                workload.drop_job(job);
            }
            false
        });
    }

    fn submit_next(
        &mut self,
        passes: &mut Passes,
        results: &mut impl Results,
        tick: u64,
        hz: u32,
    ) -> Option<usize> {
        let settings = passes.settings;
        let work = passes.workload.next_jobs(settings, tick, hz);
        let first = *work.first()?;
        let previous = if first.kind == SectionWork::Compare {
            work.iter()
                .map(|job| {
                    passes
                        .sections
                        .get(&job.origin)
                        .and_then(|section| section.input.as_ref())
                        .and_then(PaddedInput::shared)
                })
                .collect()
        } else {
            Vec::new()
        };
        let work: Batch = work.into();
        let input = Input {
            work: work.clone(),
            previous,
            settings,
        };
        let bytes = input.bytes();
        let key = key(first.origin);
        let generation = passes.workload.generation();
        match results.submit(key, generation, priority(first, settings, tick, hz), input) {
            Ok(()) => {
                assert!(self.flight.insert(key, work).is_none());
                Some(bytes)
            }
            Err(Full(input)) => {
                passes.workload.return_jobs(&input.work);
                None
            }
        }
    }

    fn fill(&mut self, passes: &mut Passes, results: &mut impl Results, frame: &Prepare<'_, '_>) {
        let mut submitted = 0;
        while submitted < LIMITS.queued_bytes {
            let Some(bytes) = self.submit_next(passes, results, frame.tick, frame.hz) else {
                break;
            };
            submitted += bytes;
        }
    }
}

impl Passes {
    pub(crate) fn prepare_jobs(
        &mut self,
        gpu: &HookGpu<'_>,
        view: &FrameView<'_>,
        tick: u64,
        hz: u32,
    ) -> bool {
        if view.time.allowance == 0 || self.job_error.is_some() {
            return false;
        }
        let mut jobs = self.jobs.take().unwrap();
        jobs.pending
            .reconcile(&mut self.workload, |key| jobs.queue.cancel(key));
        let frame = Prepare {
            gpu: Some(gpu),
            time: &view.time,
            tick,
            hz,
            camera: view.frame.camera_position.to_array(),
        };
        let mut results = jobs.queue.take(gpu, view);
        let lane = results.lane();
        let worked = self.prepare_job_units(&mut jobs.pending, &mut results, &frame, lane);
        if self.job_error.is_some() {
            jobs.clear();
            self.workload.job_bytes(0, 0);
        }
        self.jobs = Some(jobs);
        worked
    }

    fn prepare_job_units(
        &mut self,
        pending: &mut PendingJobs,
        results: &mut impl Results,
        frame: &Prepare<'_, '_>,
        lane: Lane,
    ) -> bool {
        let mut budget = FrameBudget::new(self.settings.budget, frame.time.allowance);
        budget.use_deadline();
        self.water.dirty = self.water.needs_upload(frame.camera);
        let (bytes, peak) = results.sample_retained_bytes();
        self.workload.job_bytes(bytes as u64, peak as u64);
        let mut worked = false;
        while budget.remaining() > 0 {
            if let Some(unit) =
                self.workload
                    .next_before_section(&mut budget, self.settings, frame.tick, frame.hz)
            {
                let small = unit.small();
                self.run_unit(frame.gpu, unit, frame.hz, frame.camera);
                budget.spent(small, || frame.deadline_passed());
                worked = true;
                continue;
            }
            if lane == Lane::Inline && pending.flight.is_empty() {
                pending.submit_next(self, results, frame.tick, frame.hz);
            }
            if let Some(done) = results.next() {
                let work = pending.flight.remove(&done.key).unwrap();
                let (bytes, peak) = results.sample_retained_bytes();
                let taken = match &done.result {
                    Ok(output) => Input::output_bytes(output),
                    Err(error) => error.0.capacity(),
                };
                self.workload
                    .job_bytes(bytes as u64, peak.max(bytes + taken) as u64);
                match done.result {
                    Ok(output) => self.apply_job(frame.gpu, &work, output),
                    Err(error) => {
                        self.job_error = Some(error.0);
                        return true;
                    }
                }
                budget.received(|| frame.deadline_passed());
                worked = true;
                continue;
            }
            if !self
                .workload
                .can_spend_on_other_work(&budget, self.settings)
            {
                break;
            }
            let Some(unit) = Workload::next_blend(&mut budget, self.water.dirty) else {
                break;
            };
            self.run_unit(frame.gpu, unit, frame.hz, frame.camera);
            budget.spent(false, || frame.deadline_passed());
            worked = true;
        }
        worked |= self.finish_blended_refills();
        if lane == Lane::Pool {
            pending.fill(self, results, frame);
        }
        let (bytes, peak) = results.sample_retained_bytes();
        self.workload.job_bytes(bytes as u64, peak as u64);
        worked || lane == Lane::Pool
    }

    fn apply_job(&mut self, gpu: Option<&HookGpu<'_>>, work: &[Job], output: Output) {
        for &job in work {
            self.workload.received_job(job);
        }
        match output {
            Output::Mesh(candidate) => {
                let job = work[0];
                let cpu_bytes = candidate.cpu_bytes();
                match self.workload.stage_if_repair(job, candidate) {
                    Ok(()) => self.workload.cpu_bytes(0, cpu_bytes),
                    Err(candidate) => self.install(gpu, job, candidate),
                }
            }
            Output::Compared(bits) => {
                for (index, &job) in work.iter().enumerate() {
                    self.workload
                        .compare_result(job, bits & (1 << index) != 0, self.settings);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
