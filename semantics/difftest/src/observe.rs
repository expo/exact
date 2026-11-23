//! The runner's half of a differential run: boot the plan, deliver each
//! event, and print what `Contract.Observe.lines` prints for the same
//! configuration (`semantics/Contract/Observe.lean` is the format's
//! definition).

use crate::oracle::Oracle;
use crate::script::Event;
use exact_kernel::{Kernel, PropId, ViewId};
use exact_plan::{Plan, Value};
use exact_runner::{ControlValue, Event as HostEvent, Runner};

/// A number as the observation prints it: its IEEE bits, NaN canonical.
pub fn number(n: f64) -> String {
    let bits = if n.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        n.to_bits()
    };
    format!("n{bits:016x}")
}

/// A string as the observation quotes it.
pub fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u00{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A value as the observation prints it.
pub fn value(v: &Value) -> String {
    match v {
        Value::Number(n) => number(*n),
        Value::Bool(b) => b.to_string(),
        v if v.is_str() => quote(v.as_str().unwrap_or_default()),
        Value::Unit => "()".into(),
        Value::Option(None) => "none".into(),
        Value::Option(Some(inner)) => format!("some({})", value(inner)),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(value).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Record(items) => {
            let parts: Vec<String> = items.iter().map(value).collect();
            format!("{{{}}}", parts.join(","))
        }
        _ => "?".into(),
    }
}

/// Every live element in structural preorder.
fn preorder(r: &Runner<Oracle>) -> Vec<ViewId> {
    fn walk(r: &Runner<Oracle>, id: ViewId, out: &mut Vec<ViewId>) {
        out.push(id);
        if let Some(n) = r.kernel().node(id) {
            for c in n.children() {
                walk(r, c, out);
            }
        }
    }
    let mut out = Vec::new();
    for root in r.roots() {
        walk(r, root, &mut out);
    }
    out
}

/// The elements an observation shows, in preorder: every live element but
/// its virtualized data rows. Fixed children remain observable
/// (`Contract.Observe.viewLines` uses the same boundary).
fn observed(r: &Runner<Oracle>) -> Vec<ViewId> {
    fn walk(
        r: &Runner<Oracle>,
        windowed: &[exact_runner::CollectionSnapshot],
        id: ViewId,
        out: &mut Vec<ViewId>,
    ) {
        out.push(id);
        if let Some(collection) = windowed.iter().find(|c| c.view == id) {
            for root in collection.fixed.iter().flat_map(|fixed| &fixed.roots) {
                walk(r, windowed, *root, out);
            }
            return;
        }
        if let Some(n) = r.kernel().node(id) {
            for c in n.children() {
                walk(r, windowed, c, out);
            }
        }
    }
    let windowed = r.collections();
    let mut out = Vec::new();
    for root in r.roots() {
        walk(r, &windowed, root, &mut out);
    }
    out
}

fn find(r: &Runner<Oracle>, test_id: &str) -> Option<ViewId> {
    preorder(r).into_iter().find(|&id| {
        r.kernel()
            .node(id)
            .is_some_and(|n| n.props.str(PropId::TestId) == Some(test_id))
    })
}

/// Boot `plan` against `oracle`, on the device store the oracle holds,
/// building every tab panel then when the host would (a prerendered page).
pub(crate) fn boot(
    plan: Plan,
    oracle: Oracle,
) -> Result<Runner<Oracle>, exact_runner::RunnerError> {
    let snapshot = oracle.snapshot();
    let every = oracle.prerendered;
    let launch = oracle.launch.clone();
    let mut r = exact_runner::time::launching_with(launch, || {
        Runner::boot_with_delivery_linked(
            exact_runner::RunnerLinks::ALL,
            exact_runner::ModuleLinks {
                source_answer: Some(Oracle::platform_answer),
                ..exact_runner::ModuleLinks::ALL
            },
            plan,
            oracle,
            Kernel::with_monospace(),
            None,
            snapshot,
            Default::default(),
            Default::default(),
            "/",
        )
    })?;
    // A module's per-target answers are the web's, as the JS target gives them, not those of the machine the
    // runner was built for: Photo Editor's wallet can pay on macOS and cannot on Linux, so a walk there never
    // reached its print sizes (#465). The answers boot asked for are asked again for the web.
    r.set_device_links(exact_runner::runner::DeviceLinks::ALL);
    r.set_module_target("web");
    if let (_, Some(error)) = r.apply_announced() {
        return Err(error);
    }
    if every {
        r.build_every_tab()?;
    }
    Ok(r)
}

/// The state, commands and view lines of one configuration.
fn state(r: &mut Runner<Oracle>, plan: &Plan, out: &mut Vec<String>) {
    for (i, row) in plan.slots.iter().enumerate() {
        let id = exact_plan::SlotsId(i as u32);
        if row.owner.is_some() || plan.locale == Some(id) || plan.format == Some(id) {
            continue;
        }
        let name = plan.str(row.name);
        if let Some(v) = r.slot(name) {
            out.push(format!("slot {name} {}", value(v)));
        }
    }
    // What the store keeps for each persisted state (LLP 1116 D5).
    for row in plan.slots.iter().filter(|s| s.is_persisted()) {
        let name = plan.str(row.name);
        let kept = r.persisted(name).map_or("-".to_string(), |v| value(&v));
        out.push(format!("store {name} {kept}"));
    }
    for d in &plan.derives {
        let name = plan.str(d.name);
        if let Some(v) = r.derive(name) {
            out.push(format!("derive {name} {}", value(v)));
        }
    }
    for res in &plan.resources {
        let name = plan.str(res.name);
        if let Some(v) = r.resource(name) {
            out.push(format!("resource {name} {}", value(v)));
        }
    }
    // Each queue's waiting sends (LLP 1092 D12), in declaration order.
    let queued = r.queued();
    for m in (plan.mutations.iter()).filter(|m| m.mode == exact_plan::MutationMode::Queue) {
        let name = plan.str(m.name);
        let n = queued
            .iter()
            .find(|(q, _)| q == name)
            .map_or(0, |(_, n)| *n);
        out.push(format!("queued {name} {n}"));
    }
    // The runner's own scroll to a fragment (LLP 1153 D5) is a host's effect
    // of a router change once its content settles, which the semantics leaves
    // out as it leaves out the change a host is sent: no app wrote it.
    let mut commands = std::mem::take(&mut r.data().commands);
    commands.extend(r.take_commands());
    for c in commands
        .into_iter()
        .filter(|c| c.name != "scrollToFragment")
    {
        let args: String = c.args.iter().map(|a| format!(" {}", value(a))).collect();
        out.push(format!("command {}{args}", c.name));
    }
    for id in observed(r) {
        let Some(n) = r.kernel().node(id) else {
            continue;
        };
        if let Some(t) = n.props.str(PropId::TestId) {
            // A `time`'s text is the runner's live time (LLP 1126 D6), a
            // presentation the semantics leaves out (`OMITTED_KINDS`).
            let live = n.props.str(PropId::SemanticTag) == Some("time")
                && n.props.get(PropId::Datetime).is_some();
            let text = (n.props.str(PropId::Text))
                .filter(|_| !live)
                .map_or("-".to_string(), quote);
            out.push(format!("view {} {text}", quote(t)));
        }
    }
}

/// Boot `plan` against `oracle`, deliver `events`, and return the
/// observation lines and the source (with its transcript).
///
/// An event is delivered as every host delivers one, at the current time
/// (`Runner::dispatch_at`): the work already due fires first, then the
/// event, which runs even when that work refused (`Contract.Observe.dispatchAt`).
pub fn run(plan: Plan, oracle: Oracle, events: &[Event]) -> (Vec<String>, Option<Oracle>) {
    let (mut driver, mut out) = Driver::new(plan, oracle);
    for event in events {
        out.extend(driver.step(event));
    }
    (out, Some(driver.oracle))
}

/// A booted runner. Each delivery returns its block of [`run`]'s observation,
/// after a `due` block when the input's arrival ran due work.
pub struct Driver {
    plan: Plan,
    runner: Option<Runner<Oracle>>,
    oracle: Oracle,
}

/// A rendered test target and the actions its input handlers name.
#[derive(Clone, Debug)]
pub struct Control {
    pub id: String,
    pub press: Option<String>,
    pub input: Option<String>,
    /// Keyboard handlers on a target whose default the verifier can deliver.
    pub key_actions: Vec<String>,
    /// Candidate key values from those handlers.
    pub keys: Vec<String>,
}

impl Driver {
    /// Boot once, retaining the oracle even when boot refuses.
    pub fn new(plan: Plan, oracle: Oracle) -> (Self, Vec<String>) {
        let kept = oracle.handle();
        let mut out = vec!["== boot".into()];
        let runner = match boot(plan.clone(), oracle) {
            Err(e) => {
                out.extend(["outcome refused".into(), format!("# {e:?}")]);
                None
            }
            Ok(r) if r.is_poisoned() => {
                out.push("outcome poisoned".into());
                None
            }
            Ok(mut r) => {
                out.push("outcome ok".into());
                trapped(&r, 0, &mut out);
                for res in &plan.resources {
                    // Only what the runner answers itself: one of the app's
                    // sources may be asked nothing at boot (a closed `when`,
                    // LLP 1161.002) and asked of the oracle later.
                    if answers_itself(&plan, res) && !r.data().scripted(plan.str(res.source)) {
                        let name = plan.str(res.name);
                        // Only an answer: a read held, refused or failed shows
                        // its placeholder, which `failed`, `pending` and
                        // `current` read as no answer; with no fact the
                        // semantics leaves a module's read out.
                        let unanswered = r.resource_is_placeholder(name)
                            || r.failed_resources().iter().any(|(n, _)| *n == name)
                            || r.pending().iter().any(|(n, _)| n == name);
                        if let Some(v) = r.resource(name).filter(|_| !unanswered).cloned() {
                            r.data()
                                .facts
                                .borrow_mut()
                                .push((name.to_string(), res.ty, v));
                        }
                    }
                }
                state(&mut r, &plan, &mut out);
                Some(r)
            }
        };
        (
            Self {
                plan,
                runner,
                oracle: kept,
            },
            out,
        )
    }

    /// Whether another event can be delivered.
    pub fn live(&self) -> bool {
        self.runner.as_ref().is_some_and(|r| !r.is_poisoned())
    }

    /// Deliver one event; a refused boot or poisoned runner returns no further blocks.
    /// The work due at an input's arrival, when it attempted any commit and
    /// did not poison the runner, is a `due` block before the input's, which
    /// then holds the handlers alone (LLP 1151 D1).
    pub fn step(&mut self, event: &Event) -> Vec<String> {
        if !self.live() {
            return Vec::new();
        }
        let r = self.runner.as_mut().expect("live runner");
        let mut before = Vec::new();
        state(r, &self.plan, &mut before);
        let mut out = Vec::new();
        let mut due = None;
        let mut journaled = r.journal_start() + r.journal().count();
        if let Some(arrival) = arrival(r, event) {
            let ((result, receipts), ran) = r.trace_commits(before_input);
            if ran.iter().any(|l| l.starts_with("ran ")) && !r.is_poisoned() {
                let (block, after) = self.block("due", journaled, &result, receipts, ran, &before);
                // The due work's commands are printed with it, not carried
                // as part of the configuration the handler starts from.
                before = after
                    .into_iter()
                    .filter(|l| !l.starts_with("command "))
                    .collect();
                out.extend(block);
                let r = self.runner.as_mut().expect("live runner");
                journaled = r.journal_start() + r.journal().count();
                due = Some((arrival, result, Vec::new(), 0));
            } else {
                // Nothing traced ran, or it poisoned the runner (the
                // semantics' `dispatchAt` ends there): it stays the input's.
                due = Some((arrival, result, ran, receipts));
            }
        }
        let r = self.runner.as_mut().expect("live runner");
        let ((result, receipts), mut ran) = match due {
            None => r.trace_commits(|r| apply(r, event)),
            Some((arrival, due, mut carried, carried_receipts)) => {
                let ((result, receipts), ran) = r.trace_commits(|r| input(r, arrival, due));
                carried.extend(ran);
                ((result, receipts + carried_receipts), carried)
            }
        };
        if let Event::Answer(source, _) = event {
            if receipts > 0 || result.is_err() {
                let outcome = if r.is_poisoned() {
                    "poisoned"
                } else if result.is_err() {
                    "refused"
                } else {
                    "ok"
                };
                ran.push(format!("ran source {source} {outcome}"));
            }
        }
        let label = event.label();
        out.extend(
            self.block(&label, journaled, &result, receipts, ran, &before)
                .0,
        );
        out
    }

    /// A block's lines (label, outcome, commit lines, configuration after it)
    /// and that configuration, for the next block's `before`.
    fn block(
        &mut self,
        label: &str,
        journaled: usize,
        result: &Result<(), exact_runner::RunnerError>,
        receipts: usize,
        ran: Vec<String>,
        before: &[String],
    ) -> (Vec<String>, Vec<String>) {
        let r = self.runner.as_mut().expect("live runner");
        let mut out = vec![format!("== {label}")];
        let traced = ran
            .iter()
            .filter(|l| l.starts_with("ran ") && l.ends_with(" ok"))
            .count();
        if r.is_poisoned() {
            out.push("outcome poisoned".into());
        } else {
            out.push(
                if result.is_ok() {
                    "outcome ok"
                } else {
                    "outcome refused"
                }
                .into(),
            );
        }
        if let Err(e) = result {
            out.push(format!("# {e:?}"));
        }
        out.extend(ran);
        trapped(r, journaled, &mut out);
        let mut after = Vec::new();
        if !r.is_poisoned() {
            state(r, &self.plan, &mut after);
        }
        let moved_without_trace = traced == 0 && !r.is_poisoned() && before != after;
        if (receipts > traced || moved_without_trace) && !out.iter().any(|l| l == OMITTED_COMMIT) {
            out.push(OMITTED_COMMIT.into());
        }
        if moved_without_trace && !out.iter().any(|l| l == "untraced configuration moved") {
            out.push("untraced configuration moved".into());
        }
        if !r.is_poisoned() {
            out.extend(module_moved(r, &self.plan));
        }
        out.extend(after.iter().cloned());
        (out, after)
    }

    pub(crate) fn mutation_requests(&self) -> u64 {
        self.runner.as_ref().map_or(0, Runner::mutation_requests)
    }

    pub(crate) fn soon(&self) -> Option<f64> {
        let r = self.runner.as_ref().filter(|_| self.live())?;
        let now = r.now_ms();
        r.tasks()
            .into_iter()
            .filter_map(|(_, at)| at)
            .filter(|at| at.is_finite() && now < *at && *at <= now + 1.0)
            .reduce(f64::max)
            .map(|at| at - now)
    }

    /// Controls among the same live elements whose `view` lines the observation prints.
    pub fn controls(&self) -> Vec<Control> {
        let Some(r) = self.runner.as_ref().filter(|_| self.live()) else {
            return Vec::new();
        };
        let mut seen = std::collections::BTreeSet::new();
        observed(r)
            .into_iter()
            .filter_map(|view| {
                let id = r
                    .kernel()
                    .node(view)?
                    .props
                    .str(PropId::TestId)?
                    .to_string();
                if !seen.insert(id.clone()) {
                    return None;
                }
                let (node, _) = r.site_of(view)?;
                let handler = |kind| {
                    self.plan
                        .node(node)
                        .handlers
                        .iter()
                        .map(|h| self.plan.handler(h))
                        .find(|h| h.event == kind)
                        .map(|h| self.plan.action(h.action))
                };
                let action = |kind| handler(kind).map(|a| self.plan.str(a.name).to_string());
                let mut key_actions = Vec::new();
                let mut keys = Vec::new();
                if host_default(r, view, "").is_none() {
                    use exact_plan::EventKind::{Key, Keycapture, Keyup};
                    for a in [Keycapture, Key, Keyup].into_iter().filter_map(handler) {
                        key_actions.push(self.plan.str(a.name).to_string());
                        for i in exact_runner::vm::instructions(self.plan.code(a.body)).flatten() {
                            if i.op == exact_plan::Opcode::Str {
                                let chord = self.plan.str(exact_plan::StrId(i.args[0] as u32));
                                if is_key_value(chord) && !keys.iter().any(|k| k == chord) {
                                    keys.push(chord.to_string());
                                }
                            }
                        }
                    }
                }
                Some(Control {
                    id,
                    press: action(exact_plan::EventKind::Press),
                    input: action(exact_plan::EventKind::Input)
                        .or_else(|| action(exact_plan::EventKind::Change)),
                    key_actions,
                    keys,
                })
            })
            .collect()
    }
}

/// The bindings that trapped since journal line `from` (counted from boot),
/// as `#` notes: each row was unset and the runner went on (LLP 1017.003
/// D3), which `compare` reads where the runner passed an evaluation bound.
fn trapped(r: &Runner<Oracle>, from: usize, out: &mut Vec<String>) {
    let skip = from.saturating_sub(r.journal_start());
    for line in r
        .journal()
        .skip(skip)
        .filter(|l| l.contains(" trapped Trap("))
    {
        out.push(format!("# {line}"));
    }
}

/// The plan node of the first element with `test_id` after `events`, by
/// the same boot and delivery as [`run`]: where a step's target or a
/// differing view line was declared (`contract::SourceMap::node`).
pub fn site(plan: Plan, oracle: Oracle, events: &[Event], test_id: &str) -> Option<usize> {
    let mut r = boot(plan, oracle).ok()?;
    for e in events {
        let _ = apply(&mut r, e);
        if r.is_poisoned() {
            return None;
        }
    }
    let view = find(&r, test_id)?;
    r.site_of(view).map(|(node, _)| node.0 as usize)
}

/// The host event a tap or typing delivers to `view`: typing reaches an
/// `input` handler where the element has one, else its `change` handler.
fn host_event(r: &Runner<Oracle>, view: ViewId, e: &Event) -> HostEvent {
    match e {
        Event::Type(_, s) if r.handlers_of(view).contains(&exact_plan::EventKind::Input) => {
            HostEvent::Input(ControlValue::Text(s.clone()))
        }
        Event::Type(_, s) => HostEvent::Change(ControlValue::Text(s.clone())),
        _ => HostEvent::Press,
    }
}

/// An input that runs the work due at its arrival before its handlers, with
/// its target and event chosen as it arrives.
enum Arrival {
    Dispatch(ViewId, HostEvent),
    Key(ViewId, String),
}

/// Whether delivering `e` first runs the work due at its arrival
/// (`Contract.Observe.dispatchAt`, `keyPress`): a host attaches a listener
/// only where the view handles the event (`Runner::handlers_of`), so a tap or
/// typing reaches the due work only through a handler; a key reaches it
/// whenever its target exists.
fn arrival(r: &Runner<Oracle>, e: &Event) -> Option<Arrival> {
    match e {
        Event::Tap(t) | Event::Type(t, _) => {
            let view = find(r, t)?;
            let event = host_event(r, view, e);
            let kind = match &event {
                HostEvent::Input(_) => exact_plan::EventKind::Input,
                HostEvent::Change(_) => exact_plan::EventKind::Change,
                _ => exact_plan::EventKind::Press,
            };
            r.handlers_of(view)
                .contains(&kind)
                .then_some(Arrival::Dispatch(view, event))
        }
        Event::Key(t, k) => Some(Arrival::Key(find(r, t)?, k.clone())),
        _ => None,
    }
}

/// An [`Arrival`] after [`before_input`] ran the work due: its handlers'
/// outcome where one failed, else the due work's (`due`), as
/// `Contract.Observe.dispatchAt` and `keyPress` combine them.
fn input(
    r: &mut Runner<Oracle>,
    arrival: Arrival,
    due: Result<(), exact_runner::RunnerError>,
) -> (Result<(), exact_runner::RunnerError>, usize) {
    match arrival {
        Arrival::Dispatch(view, event) => {
            let delivered = r.dispatch(view, event);
            let receipts = usize::from(delivered.is_ok());
            (delivered.map(drop).and(due), receipts)
        }
        Arrival::Key(view, chord) => key(r, view, &chord, due),
    }
}

/// Apply work due at the input's arrival. The handler can arm more change
/// actions for the host's next turn, so its receipt ends this delivery.
fn before_input(r: &mut Runner<Oracle>) -> (Result<(), exact_runner::RunnerError>, usize) {
    let now = r.now_ms();
    let mut result = Ok(());
    let mut receipts = 0;
    let mut firings = 0;
    loop {
        let advanced = r.advance_timed(now);
        receipts += advanced.receipts.len();
        firings += advanced.receipts.len() + usize::from(advanced.error.is_some());
        let refused = advanced.error.is_some();
        if let Some(error) = advanced.error {
            let stop = r.is_poisoned()
                || matches!(error, exact_runner::RunnerError::TimerFireLimit { .. });
            if result.is_ok() || stop {
                result = Err(error);
            }
            if stop {
                break;
            }
        }
        if !advanced.needs_apply && !refused {
            break;
        }
        if r.timer_due_ms().is_none_or(|at| at > now) {
            break;
        }
        if firings >= exact_runner::TIMER_FIRE_LIMIT {
            result = Err(exact_runner::RunnerError::TimerFireLimit {
                limit: exact_runner::TIMER_FIRE_LIMIT,
            });
            break;
        }
    }
    (result, receipts)
}

/// Each DOM event gets its own path. A keydown handler can remove the focus
/// before keyup, and stopping one event does not stop the other.
fn key_route(
    r: &Runner<Oracle>,
    target: ViewId,
    key: &exact_runner::KeyboardEvent,
    release: bool,
) -> Vec<(ViewId, HostEvent)> {
    use exact_plan::EventKind;
    let mut capture = Vec::new();
    let mut bubble = Vec::new();
    let mut at = Some(target);
    while let Some(id) = at {
        let handlers = r.handlers_of(id);
        if release {
            if handlers.contains(&EventKind::Keyup) {
                bubble.push((id, HostEvent::Keyup(key.clone())));
            }
        } else {
            if handlers.contains(&EventKind::Keycapture) {
                capture.push((id, HostEvent::Keycapture(key.clone())));
            }
            if handlers.contains(&EventKind::Key) {
                bubble.push((id, HostEvent::Key(key.clone())));
            }
        }
        at = r.kernel().node(id).and_then(|n| n.parent);
    }
    capture.reverse();
    capture.extend(bubble);
    capture
}

fn key(
    r: &mut Runner<Oracle>,
    view: ViewId,
    chord: &str,
    due: Result<(), exact_runner::RunnerError>,
) -> (Result<(), exact_runner::RunnerError>, usize) {
    // The work due fired before the keydown, so the route is the tree the
    // key meets.
    let (mut result, mut receipts) = (due, 0);
    let due_commands = r.take_commands();
    r.data().commands.extend(due_commands);
    for (release, key) in [false, true]
        .into_iter()
        .zip(crate::script::key_press(chord))
    {
        let target = r
            .kernel()
            .node(view)
            .map(|_| view)
            .or_else(|| r.roots().first().copied());
        let Some(target) = target else { break };
        for (id, event) in key_route(r, target, &key, release) {
            let outcome = r.dispatch(id, event);
            receipts += usize::from(outcome.is_ok());
            result = result.and(outcome.map(drop));
            let commands = r.take_commands();
            let stop = commands.iter().any(|c| c.name == "stopPropagation");
            r.data().commands.extend(commands);
            if stop {
                break;
            }
        }
    }
    (result, receipts)
}

/// Infer walk candidates from single characters, function keys and common
/// named keys in a handler. Tests can supply other keys explicitly.
fn is_key_value(chord: &str) -> bool {
    const NAMED: &[&str] = &[
        "Enter",
        "Escape",
        "Tab",
        "Backspace",
        "Delete",
        "Insert",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "ArrowUp",
        "ArrowDown",
        "ArrowLeft",
        "ArrowRight",
        "ContextMenu",
        "Shift",
        "Control",
        "Alt",
        "Meta",
    ];
    let event = exact_runner::KeyboardEvent::chord(chord);
    let key = event.key.as_str();
    let function = key
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=24).contains(&n));
    key.chars().count() == 1 || function || NAMED.contains(&key)
}

/// Why a press of `key` at `view` does something the runner leaves to the
/// host, if it does: the key's default there (docs/contract-grammar.md,
/// "Then the default"). A field takes typing and Enter's commit, a control
/// or an element with a `press` handler is pressed by Enter or Space, and
/// Tab moves the focus anywhere. Only a box or a text without `press` keeps
/// nothing for the host, so only there is a key delivered.
pub(crate) fn host_default(r: &Runner<Oracle>, view: ViewId, key: &str) -> Option<String> {
    let node = r.kernel().node(view)?;
    if exact_runner::KeyboardEvent::chord(key).key == "Tab" {
        return Some("Tab moves the focus".into());
    }
    if !matches!(
        node.node_type,
        exact_kernel::NodeType::View | exact_kernel::NodeType::Text
    ) {
        return Some(format!("a key at a {:?} node", node.node_type));
    }
    r.handlers_of(view)
        .contains(&exact_plan::EventKind::Press)
        .then(|| "Enter and Space press an element with a press handler".into())
}

impl Driver {
    /// [`host_default`] at the first element with `test_id`; none when no
    /// element has it, and the delivery says so.
    pub fn key_default(&self, test_id: &str, key: &str) -> Option<String> {
        let r = self.runner.as_ref()?;
        host_default(r, find(r, test_id)?, key)
    }
}

/// Whether the runner answers resource `res` itself, so that its boot value
/// is a host fact: one of its own sources (but a transport's or MIDI port's,
/// which change as they play), or a single read of a native module's name,
/// which the module's substitute answers (`runner/src/runner/modules.rs`).
/// A module read per element (`each`) has no one fact; the semantics leaves
/// it out (`module` in the oracle's text).
fn answers_itself(plan: &Plan, res: &exact_plan::ResourcesRow) -> bool {
    let source = plan.str(res.source);
    (exact_plan::runner_owned_source(source) && !matches!(source, "exactTransport" | "exactMIDI"))
        || (plan.module_named(source).is_some() && res.each.len == 0)
}

/// The start of the note a step carries when a module's read holds other
/// than at boot.
pub const MODULE_MOVED: &str = "# outside: a module's answer moved";

/// A [`MODULE_MOVED`] note for each module read whose value is no longer its
/// boot fact: the module's state is the runner's (a permission, a preferred
/// value, a send's answer), and the semantics holds the boot answer, so what
/// the step shows from here on is outside it.
fn module_moved(r: &mut Runner<Oracle>, plan: &Plan) -> Vec<String> {
    let facts = r.data().facts.clone();
    let facts = facts.borrow();
    facts
        .iter()
        .filter(|(name, _, boot)| {
            plan.resources.iter().any(|res| {
                let source = plan.str(res.source);
                plan.str(res.name) == name
                    && plan.module_named(source).is_some()
                    && !r.data().scripted(source)
            }) && r.resource(name).map(value) != Some(value(boot))
        })
        .map(|(name, _, _)| {
            format!("{MODULE_MOVED}: resource {name} no longer holds its boot answer")
        })
        .collect()
}

/// Commit kinds for which the semantics has no branch (LLP 1151 D3).
pub const OMITTED_KINDS: &[&str] = &[
    "a transport's or MIDI port's answer",
    "a re-ask",
    "a wake's delivery and expiry",
    "a drop deadline",
    "a transport-end action",
    "the advance tail's presence",
    "polls",
    "live times",
    "text-axis frames",
];

/// A receipt with no modeled commit. Property checks must exclude this step from `after`.
pub const OMITTED_COMMIT: &str = "# a commit the semantics has no branch for";

/// Deliver a primitive event, retaining successful receipts even when a later commit refuses.
pub(crate) fn apply(
    r: &mut Runner<Oracle>,
    e: &Event,
) -> (Result<(), exact_runner::RunnerError>, usize) {
    if let Some(arrival) = arrival(r, e) {
        let (due, receipts) = before_input(r);
        let (result, more) = input(r, arrival, due);
        return (result, receipts + more);
    }
    match e {
        // An event the view has no handler for reaches nothing and fires
        // nothing due.
        Event::Tap(t) | Event::Type(t, _) => match find(r, t) {
            Some(view) => {
                let event = host_event(r, view, e);
                let result = r.dispatch(view, event);
                let receipts = usize::from(result.is_ok());
                (result.map(drop), receipts)
            }
            None => (Err(exact_runner::RunnerError::UnknownView(0)), 0),
        },
        Event::Key(..) => (Err(exact_runner::RunnerError::UnknownView(0)), 0),
        Event::Clock(ms) | Event::TestClock(ms) => {
            let a = r.advance_timed(r.now_ms() + ms);
            (a.error.map_or(Ok(()), Err), a.receipts.len())
        }
        Event::Land | Event::InputLand => {
            let a = r.land_then();
            (a.error.map_or(Ok(()), Err), a.receipts.len())
        }
        Event::Answer(source, json) => {
            if let Err(why) = r.data().answer(source, json) {
                let error = exact_runner::DataError::Interface(why);
                let resource = source.clone();
                return (Err(exact_runner::RunnerError::Data { resource, error }), 0);
            }
            match r.changed(exact_runner::ResourceChange::Source(source)) {
                Ok(receipt) => (Ok(()), usize::from(receipt.is_some())),
                Err(e) => (Err(e), 0),
            }
        }
    }
}
