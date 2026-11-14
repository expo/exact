use super::*;
#[path = "../../../../../logic/tests/fixtures/activation.rs"]
mod fixture;
use exact_painted::presenter::PainterChoice;
use fixture::{Fixture, HANG};
use std::cell::Cell;
use std::io::Write;
use std::os::{fd::AsRawFd, unix::net::UnixStream};
use std::sync::mpsc::{channel, Receiver, Sender};

enum Report {
    Frame(Arc<Pixmap>),
    Idle,
    Failed(String),
}

struct Headless {
    close: UnixStream,
    report: Sender<Report>,
    idle_sent: Cell<bool>,
}

impl Output for Headless {
    fn wait(&self, fds: &[i32], timeout: i32) -> Result<bool, String> {
        if timeout == -1 && !self.idle_sent.replace(true) {
            let _ = self.report.send(Report::Idle);
        }
        poll(fds, self.fd(), timeout)
    }
    fn refresh(&self) -> u32 {
        60
    }
    fn period_ms(&self) -> f64 {
        1000. / 60.
    }
    fn pending(&self) -> bool {
        false
    }
    fn sequence(&self) -> Option<u32> {
        None
    }
    #[cfg(target_os = "linux")]
    fn last_flip(&self) -> Option<Duration> {
        None
    }
    fn fd(&self) -> i32 {
        self.close.as_raw_fd()
    }
    fn submit(&mut self, frame: SubmittedFrame) -> Result<Option<SubmittedFrame>, String> {
        Ok(Some(frame))
    }
    fn ready(&mut self) -> Result<Option<SubmittedFrame>, String> {
        Err("headless display closed".into())
    }
}

struct Observer {
    report: Sender<Report>,
    input: UnixStream,
    failure_sent: bool,
}

impl Events<fixture::Source> for Observer {
    fn fds(&self) -> Vec<i32> {
        vec![self.input.as_raw_fd()]
    }
    fn idle(&self) -> bool {
        false
    }
    fn publish(&mut self, pixels: Arc<Pixmap>) {
        let _ = self.report.send(Report::Frame(pixels));
    }
    fn turn(&mut self, p: &mut Presenter<fixture::Source>, _: impl Fn() -> f64) {
        if let Some(error) = p.activation_error().filter(|_| !self.failure_sent) {
            self.failure_sent = true;
            let _ = self.report.send(Report::Failed(error.into()));
        }
    }
    fn date_jumped(&self) -> bool {
        false
    }
    fn trace_requested(&self) -> bool {
        false
    }
}

struct Running {
    report: Receiver<Report>,
    close: UnixStream,
    thread: Option<std::thread::JoinHandle<i32>>,
}

impl Running {
    fn new(image: &Fixture, plan: &[u8]) -> Self {
        let source = image.source_factory(plan);
        let plan = plan.to_vec();
        let (close, closed) = UnixStream::pair().unwrap();
        let (input, input_writer) = UnixStream::pair().unwrap();
        let (report, reports) = channel();
        let thread = std::thread::spawn(move || {
            let _input_writer = input_writer;
            let (p, error) = Presenter::boot_with(
                &plan,
                source(),
                (40., 40.),
                1.,
                Path::new("").to_path_buf(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            let mut config = Config::from_env(&plan, "{}");
            config.dev_plan = None;
            config.dev_url = None;
            let mut output = Headless {
                close: closed,
                report: report.clone(),
                idle_sent: Cell::new(false),
            };
            let mut events = Observer {
                report,
                input,
                failure_sent: false,
            };
            // With no launch deadline or input, the loop waits indefinitely
            // after its one post-pixel update check. A timer cannot rescue a lost wake.
            run(
                &config,
                Instant::now(),
                p,
                &mut output,
                &mut events,
                LaunchMarks::default(),
                || exact_logic::Swappable::native(()),
            )
        });
        Self {
            report: reports,
            close,
            thread: Some(thread),
        }
    }

    fn receive(&self) -> Report {
        self.report
            .recv_timeout(HANG)
            .expect("the display loop made no progress")
    }

    fn loading_frame(&self) {
        let Report::Frame(pixels) = self.receive() else {
            panic!("expected the loading frame")
        };
        assert_eq!(pixels.pixel(39, 39).unwrap().blue(), 0);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.close.write_all(&[1]);
        if let Some(thread) = self.thread.take() {
            assert_eq!(thread.join().unwrap(), 1);
        }
    }
}

#[test]
#[ignore = "async lane: builds and loads a native library"]
fn native_loader_completion_presents_in_the_display_loop_after_session_retirement() {
    complete(false);
}

#[test]
#[ignore = "async lane: builds and loads a native library"]
fn native_loader_failure_reaches_the_display_loop_without_another_input() {
    complete(true);
}

fn complete(refuse: bool) {
    let mut image = Fixture::new(refuse);
    let plan = fixture::plan();
    let retired = Running::new(&image, &plan);
    retired.loading_frame();
    image.entered();
    drop(retired);

    let running = Running::new(&image, &plan);
    running.loading_frame();
    assert!(matches!(running.receive(), Report::Idle));
    let (finished, completion) = channel();
    image.source(&plan).when_preloaded(Box::new(move || {
        let _ = finished.send(());
    }));
    image.release();
    match running.receive() {
        Report::Failed(error) if refuse => {
            assert!(error.contains("Rust native export ABI differs"))
        }
        Report::Frame(pixels) if !refuse => assert_eq!(pixels.pixel(39, 39).unwrap().blue(), 255),
        _ => panic!("expected the activated frame or the loader's refusal"),
    }
    completion.recv_timeout(HANG).unwrap();
}
