use super::{Job, SectionWork, Workload, SIDE, SMALL_PER_UNIT};
use terrain_fixture_logic::{Inflight, WorkloadSettings};

impl<T: Default> Workload<T> {
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn next_jobs(&mut self, settings: WorkloadSettings, tick: u64, hz: u32) -> Vec<Job> {
        let job = self
            .jobs_remesh
            .pop_front()
            .map(|job| self.job(job.origin, job.kind, job.crossing, job.repair))
            .or_else(|| self.repair_job(settings))
            .or_else(|| self.mesh_job(settings, tick, hz));
        let batch = if let Some(job) = job {
            vec![job]
        } else {
            self.compare_jobs()
        };
        for job in &batch {
            assert!(self.in_flight.insert(job.origin));
            self.flight_repairs += usize::from(job.repair);
        }
        batch
    }

    fn compare_jobs(&mut self) -> Vec<Job> {
        let available = |column: &super::Column| {
            column.compares.front().is_some_and(|batch| {
                batch
                    .origins
                    .last()
                    .is_some_and(|origin| !self.in_flight.contains(origin))
            })
        };
        let Some(column) = self
            .columns
            .values_mut()
            .filter(|column| available(column))
            .min_by_key(|column| column.compares.front().unwrap().crossing)
        else {
            return Vec::new();
        };
        let mut work = Vec::new();
        while work.len() < SMALL_PER_UNIT as usize && available(column) {
            work.push(Self::pop_compare(column));
        }
        work.into_iter()
            .map(|(origin, crossing)| self.job(origin, SectionWork::Compare, crossing, false))
            .collect()
    }

    pub(crate) fn return_jobs(&mut self, batch: &[Job]) {
        for &job in batch {
            self.received_job(job);
        }
        let first = batch[0];
        if first.repair {
            self.queue_repair(first.origin, first.kind == SectionWork::Mesh);
        } else if first.kind == SectionWork::Mesh {
            self.jobs_remesh.push_front(first);
        } else {
            let column = self
                .columns
                .get_mut(&first.origin[0].div_euclid(SIDE))
                .unwrap();
            for job in batch.iter().rev() {
                if let Some(queued) = column
                    .compares
                    .front_mut()
                    .filter(|b| b.crossing == job.crossing)
                {
                    queued.origins.push(job.origin);
                } else {
                    column.compares.push_front(super::CompareBatch {
                        crossing: job.crossing,
                        origins: vec![job.origin],
                    });
                }
            }
        }
    }

    pub(crate) fn drop_job(&mut self, job: Job) {
        self.received_job(job);
        self.drop_leaving(job);
    }

    pub(crate) fn drop_leaving_remeshes(&mut self) {
        let count = self.jobs_remesh.len();
        for _ in 0..count {
            let job = self.jobs_remesh.pop_front().unwrap();
            if self.contains(job.origin) {
                self.jobs_remesh.push_back(job);
            } else {
                self.drop_leaving(job);
            }
        }
    }

    fn drop_leaving(&mut self, job: Job) {
        if job.repair {
            self.discarded.queued_left += 1;
        } else {
            self.drop_queued(job.crossing, 1);
        }
    }

    pub(crate) fn received_job(&mut self, job: Job) {
        assert!(self.in_flight.remove(&job.origin));
        self.flight_repairs -= usize::from(job.repair);
    }

    pub(crate) fn compare_result(&mut self, job: Job, equal: bool, settings: WorkloadSettings) {
        let stale = self.revision(job.origin).map_or(0, |r| r.number) != job.revision;
        self.completed += 1;
        if stale && settings.inflight == Inflight::Discard {
            self.discarded.stale += 1;
            self.retry_job(job);
            return;
        }
        if stale {
            self.kept += 1;
        }
        if equal {
            if !job.repair {
                self.ready_to_compare(job.origin);
            }
            self.resolve(job, settings.blend);
        } else if job.repair {
            self.queue_repair(job.origin, true);
        } else {
            self.jobs_remesh.push_back(Job {
                kind: SectionWork::Mesh,
                ..job
            });
        }
    }

    pub(crate) fn retry_job(&mut self, job: Job) {
        if !job.repair {
            self.jobs_remesh.push_back(job);
        }
    }

    pub(crate) fn refresh_after_kept_mesh(&mut self, job: Job) {
        if self
            .revision(job.origin)
            .is_some_and(|r| r.number != job.revision)
        {
            self.queue_repair(job.origin, false);
        }
    }

    pub(crate) fn job_bytes(&mut self, bytes: u64, peak: u64) {
        self.job_bytes = bytes;
        self.cpu.peak = self.cpu.peak.max(self.cpu.current + peak);
    }
}
