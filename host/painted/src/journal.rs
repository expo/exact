//! The launch journal and launch parts, the Linux counterpart of ExactKit's `LaunchParts.swift`.
//!
//! Each module listed under `launch` in `app.json` provides `modules/<name>/linux/launch.rs`,
//! compiled into the app and run at the start of `main`, before the display opens.
//! A launch part only subscribes and installs hooks. File and network work goes
//! through [`after_startup`] so it cannot delay the startup marks.
//! Subscribers run synchronously on the thread draining the journal and must only
//! hand events off. Retained replay can run on the subscribing thread.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub use serde_json;
use serde_json::{Map, Value};

/// Events kept for a subscriber that arrives later.
const CAPACITY: usize = 4096;

/// One journal event.
#[derive(Debug, Clone)]
pub struct Event {
    /// `startup`, `app.event`, `app.attributes`, `app.error`, …
    pub kind: String,
    /// Seconds on the launch marks' clock ([`crate::launch_marks::boottime`]).
    pub at: f64,
    /// Seconds since the Unix epoch.
    pub wall: f64,
    /// Kind-specific fields.
    pub fields: Map<String, Value>,
}

impl Event {
    /// The fields plus `kind`, `at` and `wall` in one object, the shape the
    /// Apple and web journals produce.
    pub fn json(&self) -> Value {
        let mut o = self.fields.clone();
        o.insert("kind".into(), self.kind.clone().into());
        o.insert("at".into(), self.at.into());
        o.insert("wall".into(), self.wall.into());
        Value::Object(o)
    }
}

type Subscriber = Arc<dyn Fn(&Event) + Send + Sync>;
type Deferred = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Journal {
    events: VecDeque<Arc<Event>>,
    subscribers: Vec<Subscriber>,
    deferred: Vec<Deferred>,
    started: bool,
    pending: VecDeque<(Arc<Event>, Vec<Subscriber>)>,
    draining: bool,
}

static JOURNAL: Mutex<Option<Journal>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Journal) -> R) -> R {
    let mut j = JOURNAL.lock().unwrap_or_else(|e| e.into_inner());
    f(j.get_or_insert_with(Journal::default))
}

/// Unix seconds now.
pub fn wall() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// Records an event. The first `startup` event starts the [`after_startup`] work.
pub fn record(kind: &str, fields: Map<String, Value>) {
    let e = Arc::new(Event {
        kind: kind.into(),
        at: crate::launch_marks::boottime(),
        wall: wall(),
        fields,
    });
    let deferred = with(|j| {
        if j.events.len() == CAPACITY {
            j.events.pop_front();
        }
        j.events.push_back(e.clone());
        let deferred = if kind == "startup" && !j.started {
            j.started = true;
            std::mem::take(&mut j.deferred)
        } else {
            Vec::new()
        };
        j.pending.push_back((e, j.subscribers.clone()));
        deferred
    });
    drain();
    for d in deferred {
        std::thread::spawn(d);
    }
}

/// Calls `f` with the retained events, then with each new one.
pub fn subscribe(f: impl Fn(&Event) + Send + Sync + 'static) {
    let f: Subscriber = Arc::new(f);
    with(|j| {
        for event in &j.events {
            j.pending.push_back((event.clone(), vec![f.clone()]));
        }
        j.subscribers.push(f);
    });
    drain();
}

fn drain() {
    let owns = with(|j| {
        if j.draining {
            return false;
        }
        j.draining = true;
        true
    });
    if !owns {
        return;
    }
    loop {
        let next = with(|j| {
            let next = j.pending.pop_front();
            if next.is_none() {
                j.draining = false;
            }
            next
        });
        let Some((event, subscribers)) = next else {
            return;
        };
        for subscriber in subscribers {
            subscriber(&event);
        }
    }
}

/// Runs `f` on its own thread once startup is recorded, or at once if it already was.
pub fn after_startup(f: impl FnOnce() + Send + 'static) {
    let f: Deferred = Box::new(f);
    let ready = with(|j| {
        if j.started {
            Some(f)
        } else {
            j.deferred.push(f);
            None
        }
    });
    if let Some(f) = ready {
        std::thread::spawn(f);
    }
}

/// The time all launch parts together should take, as on Apple. Overruns are
/// logged to stderr, not enforced.
const LAUNCH_BUDGET: std::time::Duration = std::time::Duration::from_micros(500);

/// Records how long the generated `launch_parts()` took. Logs it when over
/// budget or when `EXACT_OBSERVE_LOG=1`.
pub fn launch_parts_ran(took: std::time::Duration) {
    let micros = took.as_secs_f64() * 1e6;
    if took > LAUNCH_BUDGET || std::env::var("EXACT_OBSERVE_LOG").as_deref() == Ok("1") {
        eprintln!(
            "exact: launch parts took {micros:.0} µs (budget {} µs)",
            LAUNCH_BUDGET.as_micros()
        );
    }
    let mut f = Map::new();
    f.insert("micros".into(), micros.into());
    record("launch.parts", f);
}

/// What a launch part receives, like ExactKit's `ExactLaunchContext`.
pub struct LaunchContext {
    /// The module's name in `app.json`'s `launch` list.
    pub module: &'static str,
    /// `app.json`'s `moduleConfig[module]`, JSON (`{}` when none).
    pub config: &'static str,
    /// The app's identity: `{"id","name","version"}` from `app.json`.
    pub app: &'static str,
    /// Whether this is a development build. Observe sends nothing from one
    /// unless configured to.
    pub development: bool,
}

impl LaunchContext {
    /// The context for `module`. `compat` is the baked compatibility id.
    pub fn new(
        module: &'static str,
        config: &'static str,
        app: &'static str,
        compat: &str,
    ) -> Self {
        LaunchContext {
            module,
            config,
            app,
            development: !exact_runner::delivery::production(compat),
        }
    }

    /// `$XDG_STATE_HOME/exact/<app id>/<module>`, defaulting to `~/.local/state`.
    /// Not created here, because launch does no I/O.
    pub fn state_dir(&self) -> std::path::PathBuf {
        let app: Value = serde_json::from_str(self.app).unwrap_or_default();
        let id = app["id"].as_str().unwrap_or("app");
        let base = std::env::var_os("XDG_STATE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state"))
            })
            .unwrap_or_else(std::env::temp_dir);
        base.join("exact").join(id).join(self.module)
    }
}

/// A Contract value as JSON, for the `observe` commands' arguments.
fn value_json(v: &exact_plan::Value) -> Value {
    if let Some(s) = v.as_str() {
        return s.into();
    }
    match v {
        exact_plan::Value::Number(n) => {
            serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number)
        }
        exact_plan::Value::Bool(b) => (*b).into(),
        exact_plan::Value::Unit => Value::Null,
        other => format!("{other:?}").into(),
    }
}

/// Records Contract's `observe`, `observeAttributes` and `observeError`.
/// Leading positional arguments are followed by alternating keys and values.
pub fn host_command(name: &str, args: &[exact_plan::Value]) {
    let pairs = |from: usize| {
        let pairs = args.get(from..).unwrap_or_default().chunks_exact(2);
        let o = pairs.filter_map(|kv| Some((kv[0].as_str()?.into(), value_json(&kv[1]))));
        Value::Object(o.collect())
    };
    let text = |i: usize, or: &str| {
        args.get(i)
            .and_then(|v| v.as_str())
            .unwrap_or(or)
            .to_string()
    };
    let mut f = Map::new();
    match name {
        "observe" => {
            f.insert("name".into(), text(0, "").into());
            f.insert("severity".into(), text(1, "info").into());
            f.insert("attributes".into(), pairs(2));
            record("app.event", f);
        }
        "observeAttributes" => {
            f.insert("attributes".into(), pairs(0));
            record("app.attributes", f);
        }
        _ => {
            f.insert("source".into(), "reportedByUser".into());
            f.insert("message".into(), text(0, "").into());
            f.insert("type".into(), text(1, "ContractError").into());
            record("app.error", f);
        }
    }
}

/// POSTs `body` to `url` with the host's existing transport. Returns the
/// status and the `Retry-After` header.
pub fn post(
    url: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> Result<(u16, Option<String>), String> {
    let transport = ibex2::transport::default_transport();
    let mut req = ibex2::stdlib::fetch::Request::get(url);
    req.method = "POST".into();
    for (k, v) in headers {
        req.headers.set(k, v);
    }
    req.body = Some(body);
    req.max_body = Some(64 * 1024);
    req.timeout = Some(std::time::Duration::from_secs(30));
    let r = transport.send(&req).map_err(|e| format!("{url}: {e}"))?;
    Ok((r.status, r.headers.get("retry-after").map(str::to_string)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn a_subscriber_can_record_another_event() {
        let seen = Arc::new(AtomicBool::new(false));
        let callback_seen = seen.clone();
        subscribe(move |event| {
            if event.kind == "test.reentrant.parent" {
                record("test.reentrant.child", Map::new());
            } else if event.kind == "test.reentrant.child" {
                callback_seen.store(true, Ordering::SeqCst);
            }
        });
        let order = Arc::new(Mutex::new(Vec::new()));
        let callback_order = order.clone();
        subscribe(move |event| {
            if event.kind.starts_with("test.reentrant.") {
                callback_order.lock().unwrap().push(event.kind.clone());
            }
        });
        record("test.reentrant.parent", Map::new());
        assert!(seen.load(Ordering::SeqCst));
        assert_eq!(
            *order.lock().unwrap(),
            ["test.reentrant.parent", "test.reentrant.child"]
        );
    }
}
