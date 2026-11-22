//! The Linux display loop over a pixel output and its input descriptors.
//! Hardware opens outside this loop so headless tests use the same wake dispatch.
use exact_painted::app::Config;
use exact_painted::launch_marks::LaunchMarks;
use exact_painted::presenter::{PanelImage, Presenter, SubmittedFrame};
use exact_runner::DataSource;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tiny_skia::Pixmap;

pub(crate) type PanelFrame = SubmittedFrame<PanelImage>;

pub(crate) enum Completion {
    Pending,
    Shown(PanelFrame),
    Retry(PanelFrame),
}

pub(crate) trait Output {
    fn direct(&self) -> bool {
        false
    }
    fn refresh(&self) -> u32;
    fn period_ms(&self) -> f64;
    fn pending(&self) -> bool;
    fn sequence(&self) -> Option<u32>;
    #[cfg(target_os = "linux")]
    fn observed(&self) -> Option<Instant> {
        None
    }
    #[cfg(target_os = "linux")]
    fn last_flip(&self) -> Option<Duration>;
    fn fd(&self) -> i32;
    fn submit(&mut self, frame: PanelFrame) -> Result<Completion, String>;
    fn ready(&mut self) -> Result<Completion, String>;
    fn wait(&self, fds: &[i32], timeout: i32) -> Result<bool, String> {
        poll(fds, self.fd(), timeout)
    }
}

pub(crate) trait Events<D: DataSource> {
    fn fds(&self) -> Vec<i32>;
    fn idle(&self) -> bool;
    fn mirror(&self) -> bool {
        true
    }
    fn publish(&mut self, pixels: Arc<Pixmap>);
    fn turn(&mut self, p: &mut Presenter<D>, wall: impl Fn() -> f64);
    fn date_jumped(&self) -> bool;
    fn trace_requested(&self) -> bool;
}

type ReloadStamp = Vec<(std::path::PathBuf, SystemTime, u64)>;

fn mtime(path: &Path) -> Option<ReloadStamp> {
    fn collect(path: &Path, rows: &mut ReloadStamp) {
        let Ok(metadata) = std::fs::metadata(path) else {
            return;
        };
        if metadata.is_dir() {
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    collect(&entry.path(), rows);
                }
            }
        } else if let Ok(modified) = metadata.modified() {
            rows.push((path.to_path_buf(), modified, metadata.len()));
        }
    }
    let mut rows = Vec::new();
    collect(path, &mut rows);
    collect(&path.parent()?.join("rust"), &mut rows);
    rows.sort();
    Some(rows)
}

pub(super) struct LocalPlan {
    path: std::path::PathBuf,
    seen: Option<ReloadStamp>,
}

impl LocalPlan {
    pub(super) fn new(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            seen: mtime(path),
        }
    }

    pub(super) fn poll<D: DataSource>(&mut self, p: &mut Presenter<D>, data: fn() -> D) {
        let current = mtime(&self.path);
        if current.is_none() || current == self.seen {
            return;
        }
        self.seen = current;
        let Ok(bytes) = std::fs::read(&self.path) else {
            return;
        };
        let started = Instant::now();
        let name = self
            .path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        let module = exact_painted::delivery::Module::local(&self.path, p.compat());
        // A held candidate is the presenter's until its hold clears.
        let later = name.clone();
        match module
            .map_err(exact_painted::host::HostError::Asset)
            .and_then(|module| {
                p.reload_with_outcome(&bytes, data(), module, false, move |outcome| {
                    println!("reload of {later}, held: {outcome:?}")
                })
            }) {
            Ok(exact_painted::presenter::ReloadResult::Held(reason)) => {
                println!("reload of {name} held: {reason}")
            }
            Ok(exact_painted::presenter::ReloadResult::Applied(warning)) => println!(
                "reloaded {name} in {:.1} ms{}",
                started.elapsed().as_secs_f64() * 1000.0,
                warning.map(|e| format!(" — {e}")).unwrap_or_default()
            ),
            Err(error) => eprintln!("exact: reload: {error}"),
        }
    }
}

pub(crate) fn run<D: DataSource>(
    config: &Config,
    started: Instant,
    mut p: Presenter<D>,
    display: &mut impl Output,
    events: &mut impl Events<D>,
    mut marks: LaunchMarks,
    data: fn() -> D,
) -> i32 {
    let wall = || started.elapsed().as_secs_f64() * 1000.0;
    let observe_log = std::env::var("EXACT_OBSERVE_LOG").as_deref() == Ok("1");
    // A development display samples its flips, and writes a trace on
    // SIGUSR1 (LLP 1079 D3, D5); a production binary does neither.
    let measured = !exact_runner::delivery::production(p.compat());
    let mut frames = exact_painted::frames::Frames::new(display.refresh());
    // A GPU canvas's `Frame::period_ms` (LLP 1009).
    p.set_frame_period(display.period_ms());
    let mut local_plan = config.dev_plan.as_deref().map(LocalPlan::new);
    // First pixel is the first frame presented (LLP 1026 D11). It asks for
    // the launch's update check; the presenter's cadence asks for the rest
    // (LLP 1030.006 D5). The window counts as in the foreground throughout:
    // this loop is not told its focus.
    let mut first_pixel: Option<Instant> = None;
    loop {
        if measured {
            // A replaced host's flip and samples are the old runner's.
            frames.host(p.hosts());
            if p.dirty() {
                frames.dirty(wall());
            }
        }
        if p.dirty() && !display.pending() {
            let Some(frame) = p.panel_frame(display.direct(), events.mirror()) else {
                eprintln!("exact: display/presenter submission ownership mismatch");
                return 1;
            };
            if measured {
                frames.submitted(wall(), p.host().runner().seq(), p.hosts());
            }
            let submitted = display.submit(frame);
            if !marks.done() {
                marks.submitted(first_pixel.is_none());
            }
            match submitted {
                // The first picture's synchronous modeset has no flip event:
                // it is presented when `set_crtc` returns (LLP 1003.001 D9).
                Ok(Completion::Shown(frame)) => {
                    presented(&mut p, frame, wall(), events, &mut first_pixel)
                }
                Ok(Completion::Pending) => {}
                Ok(Completion::Retry(frame)) => p.panel_lost(&frame),
                Err(e) => {
                    eprintln!("exact: {e}");
                    return 1;
                }
            }
            // What the frame moved under the resting pointer is hovered (#139).
            p.follow_pointer();
        }
        if p.module_pending() {
            p.first_pixel();
        }
        if first_pixel.is_some() && !marks.done() {
            if !p.module_pending() {
                marks.activated();
            }
            marks.turn(&exact_painted::launch_marks::items(
                &p.host().runner().outstanding(),
            ));
        }
        if marks.take_done() {
            exact_painted::journal::record("startup", marks.report());
            if observe_log {
                eprintln!("observe: startup {}", marks.line());
            }
        }
        let now = wall();
        let mut timeout = work_timeout(
            display.pending(),
            p.needs_animation_frame(),
            p.wake_due_ms(),
            now,
        );
        if let Some(wait) = marks.timeout_ms() {
            timeout = if timeout < 0 { wait } else { timeout.min(wait) };
        }
        // An activation after this turn's paint left a commit to show: don't sleep.
        if p.dirty() && !display.pending() {
            timeout = 0;
        }
        if config.dev_plan.is_some() || config.dev_url.is_some() {
            timeout = if timeout < 0 { 100 } else { timeout.min(100) };
        }
        if p.delivery_pending() {
            timeout = if timeout < 0 { 50 } else { timeout.min(50) };
        }
        if let Some(due) = p.update_check_due() {
            let wait = due.min(i32::MAX as f64) as i32;
            timeout = if timeout < 0 { wait } else { timeout.min(wait) };
        }
        if events.idle() && timeout < 0 {
            timeout = 1000;
        }
        let mut fds = events.fds();
        fds.push(p.executor_fd());
        fds.push(p.image_fd());
        if let Some(fd) = p.content_region_fd() {
            fds.push(fd);
        }
        if let Some(fd) = p.update_fd() {
            fds.push(fd);
        }
        fds.push(display.fd());
        match display.wait(&fds, timeout) {
            Ok(true) => match display.ready() {
                Ok(Completion::Shown(frame)) => {
                    // The flip's kernel timestamp, else when it was heard.
                    #[cfg(target_os = "linux")]
                    let at = display.last_flip().map_or_else(
                        || {
                            display.observed().map_or_else(wall, |t| {
                                t.duration_since(started).as_secs_f64() * 1000.
                            })
                        },
                        |at| monotonic_ms(at, wall()),
                    );
                    #[cfg(not(target_os = "linux"))]
                    let at = wall();
                    #[cfg(target_os = "linux")]
                    if !marks.done() {
                        if let Some(at) = display.last_flip() {
                            marks.flipped(
                                at,
                                &exact_painted::launch_marks::items(
                                    &p.host().runner().outstanding(),
                                ),
                            );
                        }
                    }
                    presented(&mut p, frame, at, events, &mut first_pixel);
                    let paint = p.last_frame_ms().map(|(ms, _)| ms);
                    if measured {
                        if let Some(line) = frames.flipped(display.sequence(), at, paint) {
                            p.host_mut().log(line);
                        }
                    }
                }
                Ok(Completion::Pending) => {}
                Ok(Completion::Retry(frame)) => p.panel_lost(&frame),
                Err(e) => {
                    eprintln!("exact: {e}");
                    return 1;
                }
            },
            Ok(false) => {}
            Err(e) => {
                eprintln!("exact: {e}");
                return 1;
            }
        }
        // Work this turn does (input, completions, timers, frame tasks) is
        // wanted from when the loop woke, however long it takes.
        let woke = measured.then(wall);
        // SIGUSR1's, or a hatch's `diagnostics().save_trace()`.
        let asked = events.trace_requested();
        if measured && (asked || p.hatch_trace_asked()) {
            match exact_painted::frames::save_trace(
                &mut p,
                &frames,
                &exact_painted::frames::app_name(),
            ) {
                Ok(path) => println!("exact: trace saved to {}", path.display()),
                Err(e) => eprintln!("exact: {e}"),
            }
        }
        if let Some(e) = p.pump(wall()) {
            eprintln!("exact: {e}");
        }
        p.poll_update();
        p.run_commands(data);
        events.turn(&mut p, wall);

        if let Some(e) = p.admit_idle_timer(wall()) {
            eprintln!("exact: {e}");
        }
        // The hatches' turn (LLP 1075.003.000.001 §4.1): what they asked of
        // elements runs, and the moments this turn's commits caused.
        p.hatch_turn();
        let now = wall();
        let due = p.wake_due_ms().is_some_and(|due| due <= now);
        let date_jumped = events.date_jumped();
        if due || p.wants_frames() || date_jumped {
            if let Some(e) = p.follow_local_offset() {
                eprintln!("exact: {e}");
            }
        }
        // A frame task (LLP 1073 D5): each frame the display can take is
        // `frame` (timers, then frame tasks), never caught up.
        if !display.pending() && p.wants_frames() {
            if let Some(e) = p.animation_frame(now) {
                eprintln!("exact: {e}");
            }
        } else if due {
            if let Some(e) = p.advance(now) {
                eprintln!("exact: {e}");
            }
        }
        if !display.pending() && p.needs_animation_frame() {
            p.tick(now);
        }
        p.poll_images();
        p.poll_development(data);
        if let Some(woke) = woke.filter(|_| p.dirty()) {
            frames.dirty(woke);
        }
        if let Some(plan) = &mut local_plan {
            plan.poll(&mut p, data);
        }
    }
}

/// Wait for any of the descriptors to be readable, or `timeout` ms (-1
/// forever).
#[allow(unsafe_code)]
pub(super) fn poll(fds: &[i32], display_fd: i32, timeout: i32) -> Result<bool, String> {
    let mut pfds: Vec<libc::pollfd> = fds
        .iter()
        .map(|fd| libc::pollfd {
            fd: *fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    // SAFETY: the array is ours and sized by its length; poll reads and
    // writes only inside it.
    let result = unsafe { libc::poll(pfds.as_mut_ptr(), pfds.len() as libc::nfds_t, timeout) };
    if result < 0 {
        let e = std::io::Error::last_os_error();
        return if e.kind() == std::io::ErrorKind::Interrupted {
            Ok(false)
        } else {
            Err(format!("poll: {e}"))
        };
    }
    if let Some(fd) = pfds.iter().find(|fd| {
        fd.fd == display_fd && fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0
    }) {
        return Err(format!("poll fd {} flags {}", fd.fd, fd.revents));
    }
    Ok(pfds
        .iter()
        .any(|fd| fd.fd == display_fd && fd.revents & libc::POLLIN != 0))
}

pub(super) fn work_timeout(pending: bool, animation: bool, due: Option<f64>, now: f64) -> i32 {
    if animation && !pending {
        0
    } else if let Some(due) = due {
        (due - now).max(0.0).ceil().min(f64::from(i32::MAX)) as i32
    } else {
        -1
    }
}

/// A `CLOCK_MONOTONIC` instant (a DRM flip's) on the loop's clock, `wall`
/// being that clock now: `Instant` reads `CLOCK_MONOTONIC` on Linux.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
fn monotonic_ms(at: Duration, wall: f64) -> f64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid timespec for the call to fill.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    let now = now.tv_sec as f64 + now.tv_nsec as f64 / 1e9;
    wall - (now - at.as_secs_f64()) * 1000.0
}

fn presented<D: DataSource>(
    p: &mut Presenter<D>,
    frame: PanelFrame,
    at: f64,
    events: &mut impl Events<D>,
    first: &mut Option<Instant>,
) {
    // Pixels belong to this receipt, even if input/reload changed the live Host.
    if let Some(pixels) = frame.mirror() {
        events.publish(pixels.clone());
    }
    p.panel_complete(&frame);
    // What the picture carried starts at its present (LLP 1003.001 D9).
    if let Some(e) = p.display_presented(&frame, at) {
        eprintln!("exact: {e}");
    }
    // Its boxes are the ones the pointer rests on now: their hover, and
    // their `:hover` looks (LLP 1121 D8), follow (#139).
    p.follow_pointer();
    if first.is_none() {
        *first = Some(Instant::now());
        p.update_launched();
    }
}

#[cfg(test)]
mod timer_tests {
    use super::work_timeout;

    #[test]
    fn timer_deadlines_do_not_request_intermediate_frame_wakes() {
        for (due, now, expected) in [
            (Some(16.0), 0.0, 16),
            (Some(2.0), 0.0, 2),
            (Some(1000.0), 0.0, 1000),
            (None, 0.0, -1),
            (Some(16.0), 500.0, 0),
            (Some(1000.0), 1000.0, 0),
            (Some(501.1), 501.0, 1),
            (Some(f64::from(u32::MAX)), 0.0, i32::MAX),
        ] {
            assert_eq!(work_timeout(false, false, due, now), expected);
            assert_eq!(work_timeout(true, false, due, now), expected);
            assert_eq!(work_timeout(true, true, due, now), expected);
            assert_eq!(work_timeout(false, true, due, now), 0);
        }
    }
}

#[cfg(test)]
#[path = "event_loop/activation_tests.rs"]
mod activation_tests;
