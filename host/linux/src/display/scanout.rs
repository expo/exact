//! The Vulkan driver consumes DRM events. Its completion wakes the same poll
//! loop as a KMS flip without keeping the input thread in vkWaitForPresentKHR.
use super::event_loop::{Completion, PanelFrame};
use exact_painted::ink::scanout::{QueuedFrame, Scanout};
use exact_painted::presenter::PanelImage;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixDatagram;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub(super) struct Direct {
    output: Scanout,
    pending: Option<PanelFrame>,
    jobs: Option<Sender<QueuedFrame>>,
    wake: UnixDatagram,
    answers: Receiver<Result<Instant, String>>,
    pub(super) observed: Option<Instant>,
    worker: Option<JoinHandle<()>>,
}

impl Direct {
    pub(super) fn new(output: Scanout) -> io::Result<Self> {
        let (wake, signal) = UnixDatagram::pair()?;
        let (jobs, frames) = channel::<QueuedFrame>();
        let (done, answers) = channel();
        let worker = thread::Builder::new()
            .name("exact-display".into())
            .spawn(move || {
                for frame in frames {
                    let result = match frame.wait(Duration::from_secs(2)) {
                        Ok(true) => Ok(Instant::now()),
                        // A stuck or disconnected panel gives ownership back to KMS.
                        Ok(false) => Err("Vulkan presentation timed out".into()),
                        Err(e) => Err(e.to_string()),
                    };
                    drop(frame);
                    if done.send(result).is_err() || signal.send(&[1]).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            output,
            pending: None,
            jobs: Some(jobs),
            wake,
            answers,
            observed: None,
            worker: Some(worker),
        })
    }

    pub(super) fn fd(&self) -> i32 {
        self.wake.as_raw_fd()
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn submit(&mut self, frame: PanelFrame) -> Completion {
        let result = match frame.image() {
            PanelImage::Pixels(pixels) => self.output.present_pixels(pixels),
            PanelImage::Ink { view, rotation, .. } => self.output.present_rotated(view, *rotation),
        }
        .map_err(|e| e.to_string())
        .and_then(|queued| {
            self.jobs
                .as_ref()
                .unwrap()
                .send(queued)
                .map_err(|_| "display worker stopped".into())
        });
        match result {
            Ok(()) => {
                self.pending = Some(frame);
                Completion::Pending
            }
            Err(error) => Self::failed(frame, error),
        }
    }

    pub(super) fn ready(&mut self) -> Result<Completion, String> {
        match self.wake.recv(&mut [0]) {
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(Completion::Pending)
            }
            Err(e) => return Err(e.to_string()),
            Ok(_) => {}
        }
        let frame = self
            .pending
            .take()
            .ok_or("unsolicited Vulkan display completion")?;
        Ok(match self.answers.recv() {
            Ok(Ok(at)) => {
                self.observed = Some(at);
                Completion::Shown(frame)
            }
            Ok(Err(error)) => Self::failed(frame, error),
            Err(error) => Self::failed(frame, error.to_string()),
        })
    }

    fn failed(frame: PanelFrame, error: String) -> Completion {
        eprintln!("exact: {error}; repainting on KMS dumb buffers");
        Completion::Retry(frame)
    }
}

impl Drop for Direct {
    fn drop(&mut self) {
        // The waiter must release its swapchain before the carrier drives the card again.
        self.jobs.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
