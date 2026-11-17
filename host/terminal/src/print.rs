//! Print mode (LLP 1101.003 D1, D2, D7): a terminal entry run once, from a
//! shell. The host lays the document out at the width, runs the app until
//! it is done, writes what it shows once and exits; a script reads the
//! output and the exit status.
//!
//! Done is LLP 1113 D3's settle procedure under its command-line column: the
//! host repeats passes (announcements applied, timers due by the wall clock
//! fired, commits laid out) until one commits nothing, then the app is done
//! when no request is pending, no one-shot work is armed and no node is
//! `aria-busy`. A `role="log"`'s settled children are written as they
//! settle, as inline mode prints them into scrollback, and the app is told
//! (`printed`), so its retire task runs; the rest is written at exit.

use crate::ending::{self, Causes, Mark, Outcome};
use crate::host::{Host, Mode};
use crate::image::Protocol;
use crate::paint::Painted;
use crate::term::record;
use exact_kernel::{NodeKey, PropId, ViewId};
use exact_plan::EventKind;
use exact_runner::{DataSource, Event};
use std::collections::HashSet;
use std::io::{ErrorKind, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Rows painted per strip, as inline mode bounds a burst.
const STRIP: usize = 128;

/// How a print-mode run writes and ends.
pub(crate) struct Print {
    /// `--json`: no rows, one `state` object at exit.
    pub json: bool,
    /// Rows as SGR text, else plain.
    pub sgr: bool,
    /// `--timeout`'s seconds; zero is none.
    pub timeout: f64,
    /// The launch location as resolved, for `navigation` and the head line.
    pub location: String,
}

/// What of the transcript is written, as inline mode's writer keeps it.
#[derive(Default)]
struct Stream {
    /// Log children written, by identity, and the `id` of the last.
    printed: HashSet<NodeKey>,
    through: String,
    /// One more for each new log node.
    logs: u64,
    log: Option<NodeKey>,
    /// The (log, through) last told to the app.
    published: (u64, String),
}

/// Why writing stopped.
enum Stop {
    /// A signal or the timeout was recorded.
    Cut,
    /// The write failed.
    Failed(std::io::Error),
}

/// Run `host` to done and write it. The process's exit status.
pub(crate) fn run<D: DataSource>(host: &mut Host<D>, print: Print) -> u8 {
    let mut out = std::io::stdout().lock();
    let woken = Arc::new(AtomicBool::new(true));
    let flag = woken.clone();
    host.listen(Arc::new(move || flag.store(true, Ordering::SeqCst)));
    let start = Instant::now();
    host.live_from(start);
    // The surface record before the first frame (D1), not the empty fill.
    host.publish(&record(
        Mode::Inline,
        Protocol::Blocks,
        host.colors(),
        0,
        "",
    ));
    let mut stream = Stream::default();
    let mut causes = Causes::default();
    loop {
        if let Some(error) = fixpoint(host, &woken, start) {
            causes.error = Some(error);
            break;
        }
        if !print.json {
            match stream.write_settled(host, &mut out, print.sgr) {
                // The app was told what is written: its retire task runs.
                Ok(true) => continue,
                Ok(false) => {}
                Err(Stop::Cut) => break,
                Err(Stop::Failed(e)) => {
                    failed(&mut causes, e);
                    break;
                }
            }
        }
        ending::check_deadline(print.timeout);
        if ending::cut_short() {
            break;
        }
        if host.quitting() {
            causes.closed = true;
            break;
        }
        if done(host) {
            break;
        }
        sleep(host, &woken, start);
    }
    shutdown(host, &woken, &mut causes);
    let head = host.runner.head().status;
    let outcome = ending::outcome(&causes, head);
    let mark = ending::mark();
    let written = if causes.epipe {
        Ok(())
    } else if print.json {
        write_json(host, &mut out, &print.location, &outcome)
    } else {
        let top = stream.top(host);
        write_live(host, &mut out, top, print.sgr, mark)
    };
    // A signal or the timeout that came during the write replaces the
    // captured status, even when the write then finished (D2).
    let status = match written {
        Ok(()) | Err(Stop::Cut) => ending::outcome(&causes, head).status,
        Err(Stop::Failed(e)) if e.kind() == ErrorKind::BrokenPipe => 141,
        Err(Stop::Failed(e)) => {
            crate::term::diagnostic(format_args!("{}: {e}", ending::app()));
            1
        }
    };
    if let Some(code) = outcome.head {
        crate::term::diagnostic(format_args!(
            "{}: status {code} at {}",
            ending::app(),
            print.location
        ));
    }
    status
}

/// A failed write before the final one: EPIPE ends quietly (141), another
/// error is said and exits 1 (D8).
fn failed(causes: &mut Causes, e: std::io::Error) {
    if e.kind() == ErrorKind::BrokenPipe {
        causes.epipe = true;
    } else {
        crate::term::diagnostic(format_args!("{}: {e}", ending::app()));
        causes.error = Some(e.to_string());
    }
}

/// Passes until one commits nothing (D2): announcements applied, the runner
/// advanced to the milliseconds since boot, the commits laid out, pictures
/// read in as each commit asks for them. A runner refusal ends it.
fn fixpoint<D: DataSource>(
    host: &mut Host<D>,
    woken: &AtomicBool,
    start: Instant,
) -> Option<String> {
    loop {
        if host.images.loading() {
            host.wait_images(Duration::from_millis(500));
        }
        woken.store(false, Ordering::SeqCst);
        let announced = host.announced();
        let ticked = host.tick(start.elapsed().as_secs_f64() * 1000.0);
        if let Some(e) = announced.error.or(ticked.error) {
            return Some(format!("{e:?}"));
        }
        if !announced.committed && !ticked.committed {
            return None;
        }
    }
}

/// Whether any node in the whole tree is `aria-busy`.
fn busy<D: DataSource>(host: &Host<D>) -> bool {
    host.find(|n| n.props.bool(PropId::AccessibilityBusy) == Some(true))
        .is_some()
}

/// Done: nothing holds the run (D2). A repeating timer holds nothing.
fn done<D: DataSource>(host: &Host<D>) -> bool {
    !host.runner.has_pending() && !host.runner.one_shot_armed() && !busy(host)
}

/// Sleep until an announcement, the next due time, or a short bound, so
/// the deadline and a signal are seen promptly.
fn sleep<D: DataSource>(host: &Host<D>, woken: &AtomicBool, start: Instant) {
    let now = start.elapsed().as_secs_f64() * 1000.0;
    let wait = host
        .timer_due_ms()
        .map_or(50.0, |due| (due - now).clamp(0.0, 50.0));
    let until = Instant::now() + Duration::from_secs_f64(wait / 1000.0);
    while Instant::now() < until && !woken.load(Ordering::SeqCst) && !ending::cut_short() {
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// D2's shutdown, for every cooperative exit after boot: `beforeunload` on
/// every node that declares it, with no confirmation to show; no further
/// action or timer; then announcements only, until no node is busy or a
/// second has passed.
fn shutdown<D: DataSource>(host: &mut Host<D>, woken: &AtomicBool, causes: &mut Causes) {
    let listeners: Vec<ViewId> = host
        .runner
        .handlers()
        .into_iter()
        .filter(|(_, kinds)| kinds.contains(&EventKind::Beforeunload))
        .map(|(view, _)| view)
        .collect();
    if listeners.is_empty() {
        return;
    }
    for view in listeners {
        if host.kernel().node(view).is_none() {
            continue;
        }
        if let Err(e) = host.runner.dispatch(view, Event::Beforeunload) {
            causes.error.get_or_insert_with(|| format!("{e:?}"));
        }
        host.after_commit();
    }
    let until = Instant::now() + Duration::from_secs(1);
    while busy(host) && Instant::now() < until {
        if woken.swap(false, Ordering::SeqCst) {
            if let Some(e) = host.announced().error {
                causes.error.get_or_insert_with(|| format!("{e:?}"));
            }
        } else {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Stream {
    /// Write the log's newly settled children, up to its first busy one,
    /// and tell the app (`printed`). Whether the app was told something new.
    fn write_settled<D: DataSource>(
        &mut self,
        host: &mut Host<D>,
        out: &mut impl Write,
        sgr: bool,
    ) -> Result<bool, Stop> {
        let log = host.log_key();
        if log != self.log {
            self.through.clear();
            self.printed.clear();
            self.logs += 1;
        }
        self.log = log;
        if log.is_some() {
            let children = host.transcript();
            self.printed
                .retain(|k| children.iter().any(|c| c.key == *k));
            let lead = self.lead(&children);
            // None settles while a picture is still being read.
            let settled = match host.images.loading() {
                true => lead,
                false => (children.iter().position(|c| c.busy)).unwrap_or(children.len()),
            };
            if settled > lead {
                let bottom = |n: usize| if n == 0 { 0 } else { children[n - 1].bottom };
                let (from, to) = (bottom(lead), bottom(settled));
                for top in (from..to).step_by(STRIP) {
                    let painted = host.render_settled(top, (to - top).min(STRIP));
                    write_rows(out, &painted, sgr, Mark::default())?;
                }
                self.printed
                    .extend(children[lead..settled].iter().map(|c| c.key));
                self.through = children[settled - 1].id.clone();
            }
        }
        let now = (self.logs, self.through.clone());
        if now == self.published {
            return Ok(false);
        }
        self.published = now;
        host.publish(&record(
            Mode::Inline,
            Protocol::Blocks,
            host.colors(),
            self.logs,
            &self.through,
        ));
        Ok(true)
    }

    /// How many of the log's first children are written.
    fn lead(&self, children: &[crate::host::LogChild]) -> usize {
        children
            .iter()
            .take_while(|c| self.printed.contains(&c.key))
            .count()
    }

    /// The first document row not yet written: below the written children.
    fn top<D: DataSource>(&self, host: &Host<D>) -> usize {
        if self.log.is_none() || self.log != host.log_key() {
            return 0;
        }
        let children = host.transcript();
        match self.lead(&children) {
            0 => 0,
            n => children[n - 1].bottom,
        }
    }
}

/// The rows from `top` to the document's end, once (D1).
fn write_live<D: DataSource>(
    host: &Host<D>,
    out: &mut impl Write,
    top: usize,
    sgr: bool,
    mark: Mark,
) -> Result<(), Stop> {
    let total = host.document_rows();
    for at in (top..total).step_by(STRIP) {
        let painted = host.render(at, (total - at).min(STRIP));
        write_rows(out, &painted, sgr, mark)?;
    }
    Ok(())
}

/// Each row as SGR text, or that row of the plain text, then `\n`; none of
/// inline mode's cursor moves or redraws (D1). A signal or the timeout
/// recorded since `mark` stops it between rows.
fn write_rows(out: &mut impl Write, painted: &Painted, sgr: bool, mark: Mark) -> Result<(), Stop> {
    let grid = &painted.grid;
    let plain = (!sgr).then(|| grid.text());
    let mut lines = plain.as_deref().map(str::lines);
    for y in 0..grid.rows {
        if ending::mark() != mark {
            return Err(Stop::Cut);
        }
        let row = match lines.as_mut() {
            Some(lines) => lines.next().unwrap_or("").to_string(),
            None => grid.row_sgr(y),
        };
        out.write_all(row.as_bytes())
            .and_then(|()| out.write_all(b"\n"))
            .map_err(Stop::Failed)?;
    }
    out.flush().map_err(Stop::Failed)
}

/// `--json`'s one object (D7): the runner's `state`, the launch location as
/// resolved, and the captured outcome, in one write. A signal or the
/// timeout during it changes the exit status, not the object; a write that
/// stays blocked is the watchdog's to end.
fn write_json<D: DataSource>(
    host: &Host<D>,
    out: &mut impl Write,
    location: &str,
    outcome: &Outcome,
) -> Result<(), Stop> {
    let state = exact_runner::agent::state(&host.runner);
    let mut object = state
        .trim_end()
        .strip_suffix('}')
        .unwrap_or("{")
        .to_string();
    if object.len() > 1 {
        object.push(',');
    }
    object.push_str("\"navigation\":{\"url\":");
    exact_runner::agent::quote(location, &mut object);
    object.push_str(&format!("}},\"exit\":{}}}\n", outcome.json()));
    out.write_all(object.as_bytes()).map_err(Stop::Failed)?;
    out.flush().map_err(Stop::Failed)
}

/// A run that ends before boot (usage, compile, decode or boot failure):
/// with `--json` the `exit` object alone on stdout (D7), else a line on
/// stderr. The status.
pub(crate) fn before_boot(json: bool, outcome: &Outcome) -> u8 {
    let message = outcome.message.as_deref().unwrap_or("");
    if json {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{{\"exit\":{}}}", outcome.json());
        let _ = out.flush();
    }
    for line in message.lines() {
        crate::term::diagnostic(format_args!("{line}"));
    }
    outcome.status
}
