//! The Linux display loop over a pixel output and its input descriptors.
//! Hardware opens outside this loop so headless tests use the same wake dispatch.
use exact_painted::app::Config;
use exact_painted::launch_marks::LaunchMarks;
use exact_painted::presenter::{Presenter, SubmittedFrame};
use exact_runner::DataSource;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tiny_skia::Pixmap;

pub(crate) trait Output {
    fn refresh(&self) -> u32;
    fn period_ms(&self) -> f64;
    fn pending(&self) -> bool;
    fn sequence(&self) -> Option<u32>;
    #[cfg(target_os = "linux")]
    fn last_flip(&self) -> Option<Duration>;
    fn fd(&self) -> i32;
    fn submit(&mut self, frame: SubmittedFrame) -> Result<Option<SubmittedFrame>, String>;
    fn ready(&mut self) -> Result<Option<SubmittedFrame>, String>;
    fn wait(&self, fds: &[i32], timeout: i32) -> Result<bool, String> {
        poll(fds, self.fd(), timeout)
    }
}

pub(crate) trait Events<D: DataSource> {
    fn fds(&self) -> Vec<i32>;
    fn idle(&self) -> bool;
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
    let mut plan_seen = config.dev_plan.as_deref().and_then(mtime);
    // The dev plan's modification time and since when it has waited on requests in flight.
    let mut held: Option<(Option<ReloadStamp>, Instant)> = None;
    // First pixel is the first frame presented (LLP 1026 D11); the update
    // check follows two seconds after it, off the boot path.
    let mut first_pixel: Option<Instant> = None;
    let mut check_due: Option<Instant> = None;
    loop {
        if measured {
            // A replaced host's flip and samples are the old runner's.
            frames.host(p.hosts());
            if p.dirty() {
                frames.dirty(wall());
            }
        }
        if p.dirty() && !display.pending() {
            let Some(frame) = p.display_frame() else {
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
                Ok(Some(frame)) => {
                    presented(&mut p, frame, events, &mut first_pixel, &mut check_due)
                }
                Ok(None) => {}
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
        if let Some(due) = check_due {
            let wait = due.saturating_duration_since(Instant::now()).as_millis() as i32;
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
                Ok(Some(frame)) => {
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
                    presented(&mut p, frame, events, &mut first_pixel, &mut check_due);
                    let paint = p.last_frame_ms().map(|(ms, _)| ms);
                    if let Some(line) = display
                        .sequence()
                        .filter(|_| measured)
                        .and_then(|s| frames.flipped(s, wall(), paint))
                    {
                        p.host_mut().log(line);
                    }
                }
                Ok(None) => {}
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
        if check_due.is_some_and(|due| Instant::now() >= due) {
            check_due = None;
            p.check_update();
        }
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
        if let Some(path) = &config.dev_plan {
            let m = mtime(path);
            if m.is_some() && m != plan_seen {
                plan_seen = m.clone();
                if let Ok(bytes) = std::fs::read(path) {
                    let t = Instant::now();
                    let module = exact_painted::delivery::Module::local(path, &config.compat);
                    let let_go = held.as_ref().is_some_and(|(at, t)| {
                        *at == m && t.elapsed() >= exact_painted::host::LET_GO_AFTER
                    });
                    match module
                        .map_err(exact_painted::host::HostError::Asset)
                        .and_then(|module| p.reload_module(&bytes, data(), module, let_go))
                    {
                        Ok(e) => println!(
                            "reloaded {} in {:.1} ms{}",
                            path.file_name()
                                .map(|f| f.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            t.elapsed().as_secs_f64() * 1000.0,
                            e.map(|e| format!(" — {e}")).unwrap_or_default()
                        ),
                        Err(exact_painted::host::HostError::PreparingModule) => plan_seen = None,
                        // Asked again each turn; after a second it lets the requests go (LLP 1007 §6).
                        Err(exact_painted::host::HostError::RequestsPending) => {
                            plan_seen = None;
                            if held.as_ref().is_none_or(|(at, _)| *at != m) {
                                held = Some((m.clone(), Instant::now()));
                            }
                        }
                        Err(e) => eprintln!("exact: reload: {e}"),
                    }
                    if plan_seen.is_some() {
                        held = None;
                    }
                }
            }
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

fn presented<D: DataSource>(
    p: &mut Presenter<D>,
    frame: SubmittedFrame,
    events: &mut impl Events<D>,
    first: &mut Option<Instant>,
    check_due: &mut Option<Instant>,
) {
    // Pixels belong to this receipt, even if input/reload changed the live Host.
    events.publish(frame.pixels().clone());
    p.display_complete(&frame);
    // Its boxes are the ones the pointer rests on now: their hover, and
    // their `:hover` looks (LLP 1121 D8), follow (#139).
    p.follow_pointer();
    if first.is_none() {
        *first = Some(Instant::now());
        *check_due = Some(Instant::now() + Duration::from_secs(2));
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
