//! The command line every terminal app shares (LLP 1101.003):
//! `<app> [entry.contract | app.plan] [/location | word …] [--name value …]
//! [--print | --json] [--columns N] [--timeout S] [--color=auto|always|never]
//! [--size COLSxROWS] [--inline | --fullscreen] [--drive op …]`
//!
//! The arguments are the launch URL (D3, [`crate::argv`]). Which mode runs
//! is D1's table: `--drive` runs the agent's operations headless; `--print`,
//! `--json`, or a stdin or stdout that is not a terminal prints once and
//! exits ([`crate::print`]); otherwise the app runs in this terminal (LLP
//! 1101 D10).
//!
//! Under `--drive`, at `--size` (80×24 by default), the operations run in
//! order, the agent's verbs:
//! `tap <testId>`, `type <testId> <text>`, `key <Name>`, `paste <text>`,
//! `wheel <testId> <rows>`, `resize <COLSxROWS>`, `wait <ms>`,
//! `until <text>` (wait up to 15 s for the screen to show it), `tree`,
//! `screenshot <file.txt|file.ans>`, `print [--all]` (the screen; `--all`
//! puts the scrollback above it), `document` (the whole laid-out document),
//! `layout` (every node's box as JSON: px, cells, testId, corner radii),
//! `plan` (the running plan's SHA-256, as JSON), and the runner's own reads
//! the agent's `tree` and `layout <id>` make elsewhere: `runner-tree` (every
//! node, its id, parent, children and props, as JSON) and `node <id>` (one
//! node's every row with its value, token and source, as `{"node":…}`).
//!
//! Headless, frames go into a terminal emulator, and those verbs read its
//! screen: what a person would see (LLP 1101.002 §0 P1). `tap`, `type` and
//! `wheel` refuse a node that is not on it.

use crate::argv::{self, parse_size, Color, Line};
use crate::color::ColorDepth;
use crate::ending::{self, Outcome, Reason};
use crate::host::{Host, Key, Mode, Start};
use crate::print::{self, Print};
use crate::term;
use exact_runner::DataSource;
use std::io::{IsTerminal, Write};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// `--timeout`'s default, GNU `timeout`'s kind of deadline (D2).
const TIMEOUT: f64 = 120.0;

/// A key named as the agent names it, in Playwright's spelling: `Enter`,
/// `Control+c`, `Shift+Enter`, `a`, with the modifiers in any order. It is
/// the key the terminal's decoder makes of the same chord, or `None` for a
/// name that is no key.
pub fn key_of(name: &str) -> Option<Key> {
    const NAMED: [&str; 13] = [
        "Enter",
        "Tab",
        "Escape",
        "Backspace",
        "Delete",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "Home",
        "End",
        "PageUp",
        "PageDown",
    ];
    let (mut mods, mut rest) = (0, name);
    // `+` itself is a key: `+`, `Shift++`.
    while let Some((word, key)) = rest.split_once('+').filter(|(_, key)| !key.is_empty()) {
        mods |= match word {
            "Shift" => 1,
            "Alt" => 2,
            "Control" => 4,
            "Meta" => 8,
            _ => return None,
        };
        rest = key;
    }
    if rest == "Space" {
        return Some(term::text_with_mods(' ', mods));
    }
    if let Some(n) = NAMED.iter().find(|n| **n == rest) {
        return Some(term::with_mods(n, mods));
    }
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(term::text_with_mods(c, mods)),
        _ => None,
    }
}

fn tree<D: DataSource>(host: &Host<D>) -> String {
    let mut out = String::new();
    let kernel = host.kernel();
    fn walk<D: DataSource>(
        host: &Host<D>,
        id: exact_kernel::ViewId,
        depth: usize,
        out: &mut String,
    ) {
        let kernel = host.kernel();
        let Some(n) = kernel.node(id) else { return };
        if n.is_inline_run() || n.style.display == exact_kernel::Display::None {
            return;
        }
        let r = host.cells_of(id).expect("a node");
        let test_id = n.props.str(exact_kernel::PropId::TestId).unwrap_or("");
        let text = n
            .text_runs()
            .iter()
            .map(|r| r.text.to_string())
            .collect::<String>();
        out.push_str(&format!(
            "{}{:?}{} @{},{} {}x{}{}\n",
            "  ".repeat(depth),
            n.node_type,
            if test_id.is_empty() {
                String::new()
            } else {
                format!(" #{test_id}")
            },
            r.x,
            r.y,
            r.w,
            r.h,
            if text.is_empty() {
                String::new()
            } else {
                format!(" {text:?}")
            }
        ));
        if n.node_type != exact_kernel::NodeType::Text {
            for c in n.children() {
                walk(host, c, depth + 1, out);
            }
        } else {
            runs(host, id, depth + 1, out);
        }
    }
    /// A paragraph's inline runs that carry a `testId` (a link the agent
    /// taps): named, with their text, where the paragraph's own line is.
    fn runs<D: DataSource>(
        host: &Host<D>,
        id: exact_kernel::ViewId,
        depth: usize,
        out: &mut String,
    ) {
        let kernel = host.kernel();
        let Some(n) = kernel.node(id) else { return };
        for c in n.children() {
            let Some(run) = kernel.node(c) else { continue };
            if let Some(test_id) = run.props.str(exact_kernel::PropId::TestId) {
                let text: String = run.text_runs().iter().map(|r| r.text.to_string()).collect();
                let shown = if host.is_presented_run(c) {
                    ""
                } else {
                    " (off the screen)"
                };
                out.push_str(&format!(
                    "{}Run #{test_id} {text:?}{shown}\n",
                    "  ".repeat(depth)
                ));
            }
            runs(host, c, depth + 1, out);
        }
    }
    for root in kernel.roots() {
        walk(host, root, 0, &mut out);
    }
    out
}

/// Every displayed node's box, as one JSON line in the agent's shape (the
/// `layout` verb, the terminal carrier's own: LLP 1121 stage 11 C, for
/// scripts/scale-check.mjs): the viewport and each node's frame in the
/// kernel's px, the cell (`COLUMN`×`ROW` px), each node's cells as the
/// painter rounds them, its `testId`, and its corners' resolved radii in px
/// (the terminal draws no corner a picture could be measured by), as a host
/// sizes one against the box.
fn layout<D: DataSource>(host: &Host<D>) -> String {
    use exact_kernel::style::cells::{COLUMN, ROW};
    use std::fmt::Write;
    let kernel = host.kernel();
    let (cols, rows) = host.size();
    let num = |n: f64| format!("{n}");
    let mut nodes = Vec::new();
    let mut stack: Vec<exact_kernel::ViewId> = kernel.roots();
    stack.reverse();
    while let Some(id) = stack.pop() {
        let Some(n) = kernel.node(id) else { continue };
        if n.is_inline_run() || n.style.display == exact_kernel::Display::None {
            continue;
        }
        let (f, c) = (n.frame, host.cells_of(id).expect("a node"));
        let radius = [
            n.style.border_radius_top_left.clone(),
            n.style.border_radius_top_right.clone(),
            n.style.border_radius_bottom_right.clone(),
            n.style.border_radius_bottom_left.clone(),
        ]
        // As a host sizes a corner: a percentage of the box's width across
        // and of its height down (`at_basis`), none below 0, one length where
        // the two agree, else the pair `[across, down]`.
        .map(|d| {
            let env = kernel.env_of(n.id);
            let (across, down) = (
                d.at_basis(&env, f.width).max(0.0),
                d.at_basis(&env, f.height).max(0.0),
            );
            match across == down {
                true => num(f64::from(across)),
                false => format!("[{},{}]", num(f64::from(across)), num(f64::from(down))),
            }
        });
        let mut node = format!(
            "{{\"id\":{},\"x\":{},\"y\":{},\"w\":{},\"h\":{},\"cells\":{{\"x\":{},\"y\":{},\"w\":{},\"h\":{}}},\"radius\":[{}]",
            id,
            num(f64::from(f.x)),
            num(f64::from(f.y)),
            num(f64::from(f.width)),
            num(f64::from(f.height)),
            c.x,
            c.y,
            c.w,
            c.h,
            radius.join(",")
        );
        if let Some(t) = n
            .props
            .str(exact_kernel::PropId::TestId)
            .filter(|t| !t.is_empty())
        {
            let _ = write!(node, ",\"testId\":{}", json_string(t));
        }
        node.push('}');
        nodes.push(node);
        if n.node_type != exact_kernel::NodeType::Text {
            stack.extend(n.children().into_iter().rev());
        }
    }
    format!(
        "{{\"viewport\":{{\"w\":{},\"h\":{}}},\"cell\":{{\"w\":{},\"h\":{}}},\"nodes\":[{}]}}\n",
        num(cols as f64 * f64::from(COLUMN)),
        num(rows as f64 * f64::from(ROW)),
        num(f64::from(COLUMN)),
        num(f64::from(ROW)),
        nodes.join(",")
    )
}

/// `s` as a JSON string.
fn json_string(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The whole laid-out document, as the host sees it rather than as a
/// terminal shows it: the `document` verb.
fn document<D: DataSource>(host: &mut Host<D>) -> String {
    let grid = match host.mode {
        Mode::Fullscreen => host.frame().grid.clone(),
        Mode::Inline => host.render(0, host.document_rows().max(1)).grid,
    };
    grid.text()
}

/// Run an app: `entry` is the default `.contract` (or `.plan`) when the
/// arguments name none, `mode` the default occupancy. `data` makes the data
/// module once the plan is known to be a terminal one (D9).
pub fn run<D: DataSource>(
    mut args: Vec<String>,
    data: impl FnOnce() -> D,
    entry: Option<&str>,
    mode: Mode,
) -> ExitCode {
    let start = Instant::now();
    let path = match args.first() {
        Some(a) if a.ends_with(".contract") || a.ends_with(".plan") => args.remove(0),
        _ => match entry {
            Some(e) => e.to_string(),
            None => {
                eprintln!("usage: exact-terminal <app.contract | app.plan> [location] [--print | --json] [--drive op …] (--help for more)");
                return ExitCode::from(2);
            }
        },
    };
    let dir = std::path::Path::new(&path)
        .parent()
        .map(|d| match d.as_os_str().is_empty() {
            true => std::path::PathBuf::from("."),
            false => d.to_path_buf(),
        });
    let plan = || -> Result<exact_plan::Plan, String> {
        if path.ends_with(".contract") {
            return contract::compile_path_terminal(std::path::Path::new(&path)).map_err(|all| {
                let all: Vec<String> = all.iter().map(ToString::to_string).collect();
                all.join("\n")
            });
        }
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        exact_plan::Plan::decode_terminal(&bytes).map_err(|e| format!("{path}: {e}"))
    };
    launch(args, start, mode, dir, true, data, plan)
}

/// Run an app whose terminal entry was compiled at build time. The bytes
/// are its encoded plan, typically embedded with `include_bytes!`. No
/// source files or compiler work are needed when the app starts.
pub fn run_embedded<D: DataSource>(
    args: Vec<String>,
    data: impl FnOnce() -> D,
    bytes: &[u8],
    mode: Mode,
) -> ExitCode {
    let start = Instant::now();
    let plan = || {
        exact_plan::Plan::decode_terminal(bytes).map_err(|e| format!("embedded terminal plan: {e}"))
    };
    launch(args, start, mode, None, false, data, plan)
}

/// A run that ends before boot: a usage error (2) or a failure (1).
fn refuse(json: bool, reason: Reason, message: String) -> ExitCode {
    let status = if reason == Reason::Usage { 2 } else { 1 };
    let outcome = Outcome {
        status,
        reason,
        head: None,
        message: Some(message),
    };
    ExitCode::from(print::before_boot(json, &outcome))
}

/// D1's table, then the plan, the location and the boot, then the mode.
/// `entry`: the command line names an entry first (`exact-terminal`).
fn launch<D: DataSource>(
    args: Vec<String>,
    start: Instant,
    default_mode: Mode,
    assets: Option<std::path::PathBuf>,
    entry: bool,
    data: impl FnOnce() -> D,
    plan: impl FnOnce() -> Result<exact_plan::Plan, String>,
) -> ExitCode {
    let line = match argv::scan(args) {
        Ok(line) => line,
        Err(refused) => return refuse(refused.json, Reason::Usage, refused.message),
    };
    let usage = |message: &str| refuse(line.json, Reason::Usage, message.into());
    let (tty_in, tty_out) = (
        std::io::stdin().is_terminal(),
        std::io::stdout().is_terminal(),
    );
    if line.drive.is_some() && line.print_flag() {
        return usage("a drive and print mode don't combine: --drive runs headless, --print and --json print once");
    }
    let print = line.drive.is_none() && (line.print_flag() || !tty_in || !tty_out);
    if print && line.size.is_some() {
        return usage("--size is a terminal's or a drive's: print mode lays out at --columns");
    }
    if print && line.mode.is_some() {
        return usage(
            "--inline and --fullscreen are a terminal's or a drive's: print mode writes rows once",
        );
    }
    let json = print && line.json;
    let timeout = line.timeout.unwrap_or(TIMEOUT);
    if print {
        ending::arm(start, timeout, json);
    }
    let plan = match plan() {
        Ok(plan) => plan,
        Err(message) => return refuse(json, Reason::Error, message),
    };
    // A signal or the deadline while it compiled ends the run here.
    if print && ending::cut_short() {
        let outcome = ending::outcome(&ending::Causes::default(), None);
        return ExitCode::from(print::before_boot(json, &outcome));
    }
    if line.help {
        return match say(&help(&plan, entry)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(code) => code,
        };
    }
    let location = line.location();
    match exact_runner::launch_matches(&plan, &location) {
        Ok(true) => {}
        Ok(false) if plan.router.is_none() => {
            return usage(&format!("{location}: this app has one location, `/`"));
        }
        Ok(false) => return usage(&format!("no route for {location}")),
        Err(e) => return refuse(json, Reason::Error, format!("{e:?}")),
    }
    let interactive = !print && line.drive.is_none();
    let (mode, (cols, rows)) = if print {
        (Mode::Inline, print_size(&line, tty_out))
    } else {
        let size = line.size.or_else(|| interactive.then(term::size).flatten());
        (line.mode.unwrap_or(default_mode), size.unwrap_or((80, 24)))
    };
    let resolved = exact_route::canonical(&location);
    let boot = Start {
        person: interactive || print,
        assets,
        location,
        update: match print {
            true => exact_runner::Update::None,
            false => exact_runner::Update::Fast,
        },
    };
    let mut host = match Host::boot_with_clock(plan, data(), mode, cols, rows, boot) {
        Ok(h) => h,
        Err(e) => return refuse(json, Reason::Error, format!("boot: {e}")),
    };
    if print {
        ending::booted();
        // D6: SGR to a terminal, plain otherwise; `always` and `never`
        // force it. Plain draws a picture as its alt text.
        let sgr = match line.color {
            None | Some(Color::Auto) => tty_out,
            Some(Color::Depth(depth)) => depth != ColorDepth::Monochrome,
        };
        host.set_colors(match (sgr, line.color) {
            (false, _) => ColorDepth::Monochrome,
            (true, Some(Color::Depth(depth))) => depth,
            (true, _) => ColorDepth::detect(),
        });
        let print = Print {
            json,
            sgr,
            timeout,
            location: resolved,
        };
        return ExitCode::from(print::run(&mut host, print));
    }
    host.set_colors(match line.color {
        Some(Color::Depth(depth)) => depth,
        None | Some(Color::Auto) => ColorDepth::detect(),
    });
    if interactive {
        return match term::run(&mut host) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    drive(host, line.drive.unwrap_or_default())
}

/// Print mode's width and the height fact (D1): `--columns`, else the
/// terminal's, else `COLUMNS`, else 80; the terminal's rows when stdout is
/// one, else 24.
fn print_size(line: &Line, tty_out: bool) -> (usize, usize) {
    let terminal = tty_out.then(term::size).flatten();
    let cols = line
        .columns
        .or(terminal.map(|(c, _)| c))
        .or_else(|| {
            std::env::var("COLUMNS")
                .ok()?
                .trim()
                .parse()
                .ok()
                .filter(|c| *c > 0)
        })
        .unwrap_or(80);
    (cols, terminal.map_or(24, |(_, r)| r))
}

/// `--help`: the host's flags and the entry's route patterns (D3).
fn help(plan: &exact_plan::Plan, entry: bool) -> String {
    let app = ending::app();
    let entry = if entry {
        " <entry.contract | app.plan>"
    } else {
        ""
    };
    let mut out = format!(
        "usage: {app}{entry} [/location | word …] [--name value …] [flags]

A `/…` location, or words (each one path segment), say where the app opens;
`--name value` adds the query parameter name=value.

  --print               print once and exit (also when stdin or stdout is not a terminal)
  --json                print the settled state as JSON and exit
  --columns N           print at N columns (default: the terminal's, $COLUMNS, else 80)
  --timeout S           stop after S seconds, exit 124 (default 120; 0 for none)
  --color=auto|always|never
                        SGR to a terminal (auto), always, or never
  --size COLSxROWS      the screen's size, in a terminal or under --drive
  --inline, --fullscreen
                        how the app occupies a terminal
  --drive op …          run the agent's headless operations
  --                    every word after it is a path segment
  -h, --help            this help

Locations:
"
    );
    if plan.router.is_none() {
        out.push_str("  /\n");
    }
    for route in &plan.routes {
        if route.notfound {
            out.push_str("  (any other: not found)\n");
        } else {
            out.push_str(&format!("  {}\n", plan.str(route.pattern)));
        }
    }
    out
}

/// Write a verb's answer to stdout. A closed stdout ends the run quietly
/// with 141, as a shell's pipeline expects, never a panic (LLP 1101.003 D8).
fn say(text: &str) -> Result<(), ExitCode> {
    let mut out = std::io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Err(ExitCode::from(141)),
        Err(e) => {
            eprintln!("{e}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// The agent's headless operations, in order (LLP 1101 D8).
fn drive<D: DataSource>(host: Host<D>, args: Vec<String>) -> ExitCode {
    match drive_ops(host, args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => code,
    }
}

fn drive_ops<D: DataSource>(mut host: Host<D>, mut args: Vec<String>) -> Result<(), ExitCode> {
    // Headless: the frames go into a terminal emulator, which the verbs
    // read. Keys stay armed: each verb is one read, against the screen as
    // the last verb left it (LLP 1101.002 §0 P1).
    let mut vt = crate::vt::Vt::new(&mut host);
    // Announcements land at the next wait.
    let woken = Arc::new(AtomicBool::new(true));
    let flag = woken.clone();
    host.listen(Arc::new(move || flag.store(true, Ordering::SeqCst)));
    let start = Instant::now();
    host.live_from(start);
    let pump = |host: &mut Host<D>, vt: &mut crate::vt::Vt| {
        // Local files decode in a moment; the screen a verb reads has
        // its pictures in, as the Linux host's headless boot waits.
        host.wait_images(std::time::Duration::from_millis(500));
        if woken.swap(false, Ordering::SeqCst) {
            host.announced();
        }
        host.tick(start.elapsed().as_secs_f64() * 1000.0);
        vt.render(host);
    };
    pump(&mut host, &mut vt);
    if args.is_empty() {
        args.push("print".into());
    }
    let mut ops = args.into_iter().peekable();
    while let Some(op) = ops.next() {
        let mut arg = || ops.next().unwrap_or_default();
        host.read_at = host.frames;
        if matches!(op.as_str(), "tap" | "type" | "wheel") {
            host.advance_for_input();
        }
        match op.as_str() {
            "tap" => {
                let id = arg();
                match host.by_test_id(&id) {
                    // An inline run (a link in a paragraph) as a click on it.
                    Some(v) if host.is_presented_run(v) => {
                        host.press_run(v);
                    }
                    Some(v) if host.on_screen(v) => host.press(v),
                    Some(_) => {
                        eprintln!("tap: #{id} is not on the screen");
                        return Err(ExitCode::FAILURE);
                    }
                    None => {
                        eprintln!("tap: no node #{id}");
                        return Err(ExitCode::FAILURE);
                    }
                }
            }
            "type" => {
                let (id, text) = (arg(), arg());
                match host.by_test_id(&id) {
                    Some(v) if !host.on_screen(v) => {
                        eprintln!("type: #{id} is not on the screen");
                        return Err(ExitCode::FAILURE);
                    }
                    Some(v) if host.input_eligible(v) => {
                        host.press(v);
                        for c in text.chars() {
                            host.key(Key::Char(c));
                        }
                    }
                    Some(_) => {
                        eprintln!("type: #{id} is not available for input");
                        return Err(ExitCode::FAILURE);
                    }
                    None => {
                        eprintln!("type: no node #{id}");
                        return Err(ExitCode::FAILURE);
                    }
                }
            }
            "key" => {
                let name = arg();
                let Some(key) = key_of(&name) else {
                    eprintln!("key: {name:?} is not a key");
                    return Err(ExitCode::FAILURE);
                };
                if host.key(key) == crate::host::After::Quit {
                    break;
                }
            }
            "paste" => host.paste(&arg()),
            "wheel" => {
                let (id, n) = (arg(), arg());
                let Ok(rows) = n.parse::<i32>() else {
                    eprintln!("wheel: {n:?} is not a number of rows");
                    return Err(ExitCode::FAILURE);
                };
                // The named node is on the screen (a scroller, or a control
                // a person could point at), and a scroller holds it.
                let moved = host
                    .by_test_id(&id)
                    .filter(|v| host.on_screen(*v))
                    .is_some_and(|v| host.wheel_on(v, rows));
                if !moved {
                    eprintln!("wheel: #{id} is not on the screen in a scroller");
                    return Err(ExitCode::FAILURE);
                }
            }
            "resize" => {
                let size = arg();
                let Some((c, r)) = parse_size(&size) else {
                    eprintln!("resize: {size:?} is not COLSxROWS");
                    return Err(ExitCode::FAILURE);
                };
                host.resize(c, r);
            }
            "wait" => {
                let until = Instant::now() + Duration::from_millis(arg().parse().unwrap_or(100));
                while Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(5));
                    pump(&mut host, &mut vt);
                }
            }
            "until" => {
                // The screen, and what scrolls off it while this waits: a
                // fast reply can pass through between two looks.
                let want = arg();
                let from = vt.scrolled();
                let until = Instant::now() + Duration::from_secs(15);
                loop {
                    pump(&mut host, &mut vt);
                    if vt.text_since(from).contains(&want) {
                        break;
                    }
                    if Instant::now() > until {
                        eprintln!("until: the screen never showed {want:?}");
                        return Err(ExitCode::FAILURE);
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            "tree" => say(&tree(&host))?,
            "print" => {
                let all = ops.next_if(|a| a == "--all").is_some();
                say(&vt.text(all))?;
            }
            "document" => say(&document(&mut host))?,
            "layout" => say(&layout(&host))?,
            "runner-tree" => say(&format!("{}\n", exact_runner::agent::tree(&host.runner)))?,
            "node" => {
                let id = arg();
                let Ok(id) = id.parse::<u32>() else {
                    eprintln!("node: {id:?} is not a view id");
                    return Err(ExitCode::FAILURE);
                };
                let node = exact_runner::agent::node(&host.runner, id);
                say(&format!("{{\"node\":{node}}}\n"))?;
            }
            "plan" => say(&format!("{{\"plan\":\"{}\"}}\n", host.plan_digest()))?,
            "screenshot" => {
                let file = arg();
                let body = if file.ends_with(".ans") {
                    vt.ansi()
                } else {
                    vt.text(false)
                };
                if let Err(e) = std::fs::write(&file, body) {
                    eprintln!("screenshot: {file}: {e}");
                    return Err(ExitCode::FAILURE);
                }
            }
            other => {
                eprintln!("unknown operation {other:?}");
                return Err(ExitCode::FAILURE);
            }
        }
        pump(&mut host, &mut vt);
        // Headless, nothing opens: say what would have.
        for url in host.outbound.drain(..) {
            eprintln!("open {url}");
        }
        if vt.unanswered() > 0 {
            eprintln!("{op}: the writer is waiting on a position answer it never got");
            return Err(ExitCode::FAILURE);
        }
    }
    Ok(())
}
