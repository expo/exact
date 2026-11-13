//! Texture uploads off the presenting thread, for a renderer under the live
//! clock (expo/exact#129): a worker hashes, validates and writes each
//! delivered texture ([`super::textures::write`]), and the renderer installs
//! it at its next preparation. Under the agent's clock uploads stay inline,
//! so a seekable frame never waits on a thread.
use super::textures::{write, Written};
use exact_game::asset::TextureData;
use exact_gpu::wgpu;
use std::collections::BTreeMap;
use std::sync::mpsc;

struct Job {
    name: String,
    serial: u64,
    data: TextureData,
}
type Done = (String, u64, Result<Written, String>);

pub(crate) struct Uploads {
    jobs: Option<mpsc::Sender<Job>>,
    done: mpsc::Receiver<Done>,
    worker: Option<std::thread::JoinHandle<()>>,
    /// Each name on the worker and the serial of its latest delivery: an
    /// older upload of a name redelivered since is not installed.
    latest: BTreeMap<String, u64>,
    serial: u64,
    in_flight: usize,
    /// While set, the worker takes no job: a test's upload still in flight.
    #[cfg(test)]
    pub(crate) hold: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Uploads the worker has finished and sent back.
    #[cfg(test)]
    pub(crate) written: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl Uploads {
    /// A worker on `device`, or `None` where no thread could be made (the
    /// caller then uploads inline).
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        let (jobs, todo) = mpsc::channel::<Job>();
        let (finished, done) = mpsc::channel();
        let (device, queue) = (device.clone(), queue.clone());
        #[cfg(test)]
        let hold = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(test)]
        let held = hold.clone();
        #[cfg(test)]
        let written = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        #[cfg(test)]
        let done_count = written.clone();
        let worker = std::thread::Builder::new()
            .name("exact texture uploads".into())
            .spawn(move || {
                for job in todo {
                    #[cfg(test)]
                    while held.load(std::sync::atomic::Ordering::Acquire) {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    let digest = exact_game::hash::of(&job.data);
                    let written =
                        write(&device, &queue, &job.data, digest).map_err(|e| e.to_string());
                    if finished.send((job.name, job.serial, written)).is_err() {
                        break;
                    }
                    #[cfg(test)]
                    done_count.fetch_add(1, std::sync::atomic::Ordering::Release);
                }
            })
            .ok()?;
        Some(Self {
            jobs: Some(jobs),
            done,
            worker: Some(worker),
            latest: BTreeMap::new(),
            serial: 0,
            in_flight: 0,
            #[cfg(test)]
            hold,
            #[cfg(test)]
            written,
        })
    }
    /// Queue `data` for `name`. A worker that has stopped hands the texture
    /// back, to upload inline.
    pub(crate) fn send(
        &mut self,
        name: String,
        data: TextureData,
    ) -> Result<(), (String, TextureData)> {
        let Some(jobs) = &self.jobs else {
            return Err((name, data));
        };
        let serial = self.serial + 1;
        let job = Job {
            name: name.clone(),
            serial,
            data,
        };
        jobs.send(job).map_err(|e| (e.0.name, e.0.data))?;
        self.serial = serial;
        self.latest.insert(name, serial);
        self.in_flight += 1;
        Ok(())
    }
    /// `name` was delivered again: its upload in flight installs nothing.
    pub(crate) fn supersede(&mut self, name: &str) {
        self.latest.remove(name);
    }
    /// The uploads finished since the last call, in the order sent, each
    /// name's latest delivery only; with `wait`, every upload sent, waiting
    /// for the worker (the agent's clock, which never leaves one in flight).
    pub(crate) fn finished(&mut self, wait: bool) -> Vec<(String, Result<Written, String>)> {
        let mut done: Vec<Done> = self.done.try_iter().collect();
        while wait && self.in_flight > done.len() {
            match self.done.recv() {
                Ok(next) => done.push(next),
                Err(_) => break,
            }
        }
        let mut out = Vec::new();
        for (name, serial, written) in done {
            self.in_flight -= 1;
            if self.latest.get(&name) == Some(&serial) {
                self.latest.remove(&name);
                out.push((name, written));
            }
        }
        // A worker that panicked answers nothing more: what it held is lost,
        // its names unprepared, as a failed upload's would be.
        if self.worker.as_ref().is_some_and(|w| w.is_finished()) {
            self.jobs = None;
            self.latest.clear();
            self.in_flight = 0;
        }
        out
    }
    /// Whether an upload is still on the worker.
    pub(crate) fn busy(&self) -> bool {
        self.in_flight != 0
    }
}

impl Drop for Uploads {
    fn drop(&mut self) {
        // The module may be unloaded next: its code must have stopped running.
        self.jobs = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
