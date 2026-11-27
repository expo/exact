//! The Vulkan driver consumes DRM events. Its completion wakes the same poll
//! loop as a KMS flip without keeping the input thread in vkWaitForPresentKHR.
use super::event_loop::{Completion, PanelFrame};
use exact_painted::ink::scanout::{QueuedFrame, Scanout};
use exact_painted::presenter::PanelImage;
use std::collections::VecDeque;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixDatagram;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) struct Direct {
    // Join the waiter before dropping the surface and returning the card to KMS.
    presents: Presents<PanelFrame, QueuedFrame>,
    output: Scanout,
}

impl Direct {
    pub(super) fn new(output: Scanout) -> io::Result<Self> {
        let presents = Presents::new(output.pending_limit(), |frame: QueuedFrame| {
            match frame.wait(Duration::from_secs(2)) {
                Ok(true) => Ok(()),
                Ok(false) => Err("Vulkan presentation timed out".into()),
                Err(e) => Err(e.to_string()),
            }
        })?;
        Ok(Self { presents, output })
    }

    pub(super) fn fd(&self) -> i32 {
        self.presents.wake.as_raw_fd()
    }

    pub(super) fn pending(&self) -> bool {
        !self.presents.frames.is_empty()
    }

    pub(super) fn can_submit(&self) -> bool {
        self.presents.can_submit()
    }

    pub(super) fn observed(&self) -> Option<Instant> {
        self.presents.observed
    }

    pub(super) fn submit(&mut self, frame: PanelFrame) -> Result<Completion, String> {
        self.presents.submit(frame, |frame| {
            match frame.picture.image() {
                PanelImage::Pixels(pixels) => self.output.present_pixels(pixels),
                PanelImage::Ink { view, rotation, .. } => {
                    self.output.present_rotated(view, *rotation)
                }
            }
            .map_err(|e| e.to_string())
        })
    }

    pub(super) fn ready(&mut self) -> Result<Completion, String> {
        self.presents.ready()
    }
}

struct Presents<F, Q> {
    frames: VecDeque<F>,
    limit: usize,
    retiring: bool,
    jobs: Option<Sender<Result<Q, String>>>,
    wake: UnixDatagram,
    answers: Receiver<Result<Instant, String>>,
    observed: Option<Instant>,
    worker: Option<JoinHandle<()>>,
}

impl<F, Q: Send + 'static> Presents<F, Q> {
    fn new(
        limit: usize,
        wait: impl Fn(Q) -> Result<(), String> + Send + 'static,
    ) -> io::Result<Self> {
        let (wake, signal) = UnixDatagram::pair()?;
        wake.set_nonblocking(true)?;
        let (jobs, frames) = channel::<Result<Q, String>>();
        let (done, answers) = channel();
        let worker = thread::Builder::new()
            .name("exact-display".into())
            .spawn(move || {
                for frame in frames {
                    let result = frame.and_then(&wait).map(|()| Instant::now());
                    // Keep draining every already-queued present until `jobs`
                    // drops, so Drop waits all in-flight presents before
                    // destroying the swapchain and never triggers a premature
                    // POLLHUP on the wake socket.
                    let _ = done.send(result);
                    let _ = signal.send(&[1]);
                }
            })?;
        Ok(Self {
            frames: VecDeque::with_capacity(limit),
            limit,
            retiring: false,
            jobs: Some(jobs),
            wake,
            answers,
            observed: None,
            worker: Some(worker),
        })
    }

    fn can_submit(&self) -> bool {
        !self.retiring && self.frames.len() < self.limit
    }

    fn submit(
        &mut self,
        frame: F,
        present: impl FnOnce(&F) -> Result<Q, String>,
    ) -> Result<Completion<F>, String> {
        if !self.can_submit() {
            return Err("no Vulkan presentation slot is available".into());
        }
        let queued = present(&frame);
        self.frames.push_back(frame);
        if let Err(error) = &queued {
            if self.frames.len() == 1 {
                return Ok(self.failed(error));
            }
            // The earlier FIFO present can still acknowledge its own picture.
            // Queue the failure behind it and refuse further submissions.
            self.retiring = true;
        }
        if self.jobs.as_ref().unwrap().send(queued).is_err() {
            return Ok(self.failed("display worker stopped"));
        }
        Ok(Completion::Pending)
    }

    fn ready(&mut self) -> Result<Completion<F>, String> {
        match self.wake.recv(&mut [0]) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(Completion::Pending);
            }
            Err(e) => return Ok(self.failed(&e.to_string())),
            Ok(_) => {}
        }
        match self.answers.try_recv() {
            Ok(Ok(at)) => {
                let frame = self
                    .frames
                    .pop_front()
                    .ok_or("unsolicited Vulkan display completion")?;
                self.observed = Some(at);
                Ok(Completion::Shown(frame))
            }
            Ok(Err(error)) => Ok(self.failed(&error)),
            Err(e) => Ok(self.failed(&e.to_string())),
        }
    }

    fn failed(&mut self, error: &str) -> Completion<F> {
        eprintln!("exact: {error}; repainting on KMS dumb buffers");
        self.retiring = true;
        Completion::Retry(self.frames.drain(..).collect())
    }
}

impl<F, Q> Drop for Presents<F, Q> {
    fn drop(&mut self) {
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests;
