// Observe on Linux, compiled into the app's executable. The entry that
// `contract::linux_entry` generates calls `launch_parts`, which runs `launch`.
// `launch` does no I/O. It names the session and installs a panic hook. After the
// startup report, a background thread subscribes to journal events, keeps the queue
// (one bounded JSON file), records earlier crashes and sends rows in expo-observe's
// OTLP/JSON, matching `apple/service/ObserveWire.swift` and `web/service.js`.
// Every process of the app shares the queue file. A process changes it only while
// holding `queue.lock`, reading it, changing it and writing it back, and sends only
// while holding `send.lock`, so each row is sent once.

use exact_painted::journal::{self, serde_json, LaunchContext};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const SCHEMA_URL: &str = "https://opentelemetry.io/schemas/1.27.0";
const CLIENT_VERSION: &str = "0.1.0";
const CHUNK: usize = 200;
const MAX_ROWS: usize = 2000;
const DEBOUNCE: Duration = Duration::from_secs(5);

/// Observe's `metricNameMap`. Exact emits only these names.
fn metric_name(category: &str, name: &str) -> String {
    match (category, name) {
        ("appStartup", "timeToInteractive") => "expo.app_startup.tti",
        ("appStartup", "timeToFirstRender") => "expo.app_startup.ttr",
        ("appStartup", "coldLaunchTime") => "expo.app_startup.cold_launch_time",
        ("appStartup", "warmLaunchTime") => "expo.app_startup.warm_launch_time",
        ("appStartup", "bundleLoadTime") => "expo.app_startup.bundle_load_time",
        ("updates", "updateDownloadTime") => "expo.updates.download_time",
        ("navigation", "cold_ttr") => "expo.navigation.cold_ttr",
        ("navigation", "warm_ttr") => "expo.navigation.warm_ttr",
        ("navigation", "tti") => "expo.navigation.tti",
        _ => return format!("expo.unknown.{name}"),
    }
    .into()
}

fn severity_number(s: &str) -> Option<u8> {
    Some(match s {
        "trace" => 1,
        "debug" => 5,
        "info" => 9,
        "warn" => 13,
        "error" => 17,
        "fatal" => 21,
        _ => return None,
    })
}

fn log_enabled() -> bool {
    std::env::var("EXACT_OBSERVE_LOG").as_deref() == Ok("1")
}

/// Formats 16 bytes as a lowercase v4 UUID.
fn uuid_v4(mut b: [u8; 16]) -> String {
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    )
}

/// A random v4 UUID.
fn uuid() -> String {
    let mut b = [0u8; 16];
    use std::io::Read;
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .is_err()
    {
        let t = journal::wall().to_bits() ^ u64::from(std::process::id()).rotate_left(32);
        b[..8].copy_from_slice(&t.to_le_bytes());
        b[8..].copy_from_slice(&t.rotate_left(17).to_le_bytes());
    }
    uuid_v4(b)
}

/// A v4-format session id hashed from the clocks and the pid.
/// It avoids reading `/dev/urandom`, since a launch part must do no I/O.
fn session_id() -> String {
    let mix = |mut z: u64| {
        z = z.wrapping_add(0x9e37_79b9_7f4a_7c15);
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    };
    let seed = exact_painted::launch_marks::boottime().to_bits()
        ^ u64::from(std::process::id()).rotate_left(40);
    let (a, b) = (mix(seed), mix(seed ^ journal::wall().to_bits()));
    uuid_v4((u128::from(a) << 64 | u128::from(b)).to_be_bytes())
}

/// The metadata that names the app's build: its identity and environment. A crash
/// record carries it, so a crash before this session's row is saved is counted
/// against the build that crashed, not the one that opens next.
fn app_meta(app: &Value, config: &Value, development: bool) -> Map<String, Value> {
    let environment = config["environment"].as_str().unwrap_or(if development {
        "development"
    } else {
        "production"
    });
    object(json!({
        "appIdentifier": app["id"], "appName": app["name"], "appVersion": app["version"],
        "environment": environment,
    }))
}

/// The launch part: no I/O, no parsing, no thread until startup is reported.
pub fn launch(ctx: LaunchContext) {
    let session = session_id();
    let start = journal::wall();
    // A panic writes `pending-<session>.json`. The next launch sends it as a
    // fatal `native.exception` for this session.
    let (module, config, app, development) = (ctx.module, ctx.config, ctx.app, ctx.development);
    let context = move || LaunchContext {
        module,
        config,
        app,
        development,
    };
    let crash_session = session.clone();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let parse = |text: &str| serde_json::from_str::<Value>(text).unwrap_or_default();
        let meta = app_meta(&parse(app), &parse(config), development);
        let record = json!({
            "session": crash_session, "sessionStart": start, "time": journal::wall(),
            "meta": meta, "type": "panic", "message": message,
            "location": info.location().map(|l| format!("{}:{}", l.file(), l.line())),
            "thread": std::thread::current().name().map(str::to_string),
        });
        let dir = context().state_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join(format!("pending-{crash_session}.json")),
            record.to_string(),
        );
        previous(info);
    }));
    journal::after_startup(move || {
        let (tx, rx) = mpsc::channel::<Value>();
        journal::subscribe(move |e| {
            let _ = tx.send(e.json());
        });
        let dir = context().state_dir();
        let config: Map<String, Value> = serde_json::from_str(config).unwrap_or_default();
        let app: Value = serde_json::from_str(app).unwrap_or_default();
        Service::new(config, &app, development, dir, session, start).run(rx);
    });
}

/// Observe's custom-event validation, copied from expo-app-metrics (`LogEvents`).
mod rules {
    use super::*;

    pub fn name(raw: &str) -> Option<String> {
        let n = raw.trim();
        (!n.is_empty() && !n.starts_with("expo.") && n.chars().count() <= 256)
            .then(|| n.to_string())
    }

    pub fn truncate(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            return s.into();
        }
        s.chars().take(max - 1).chain(['…']).collect()
    }

    fn reserved(k: &str) -> bool {
        (k.starts_with("expo.") && k.len() > 5) || k == "session.id" || k == "event.name"
    }

    /// Keeps up to 128 attributes in key order. Returns the kept ones and how many were dropped.
    pub fn attributes(raw: &Value) -> (Map<String, Value>, u64) {
        let mut kept = Map::new();
        let mut dropped = 0;
        let mut keys: Vec<&String> = raw
            .as_object()
            .map(|o| o.keys().collect())
            .unwrap_or_default();
        keys.sort();
        for key in keys {
            let k = key.trim();
            if k.is_empty() || reserved(k) || kept.len() >= 128 {
                dropped += 1;
                continue;
            }
            kept.insert(k.into(), raw[key].clone());
        }
        (kept, dropped)
    }
}

/// Same as expo's `EASClientID.deterministicUniformValue`: a stable value in [0, 1) per install.
fn uniform(id: &str) -> f64 {
    let hex: String = id.chars().filter(|c| *c != '-').collect();
    let half = |r: std::ops::Range<usize>| {
        hex.get(r)
            .and_then(|h| u64::from_str_radix(h, 16).ok())
            .unwrap_or(0)
    };
    let mut z = half(0..16) ^ half(16..32);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// Observe's jittered exponential backoff, in seconds.
fn backoff(attempt: u32) -> f64 {
    let random = u64::from_str_radix(&uuid().replace('-', "")[..13], 16).unwrap_or(0) as f64
        / (1u64 << 52) as f64;
    (60.0 * 2f64.powi(attempt as i32 - 1)).min(900.0) * random
}

/// The server's Retry-After in seconds, clamped to 60-900. An HTTP-date value is ignored.
fn retry_after(h: Option<&str>) -> Option<f64> {
    let s: f64 = h?.trim().parse().ok()?;
    s.is_finite().then(|| s.clamp(60.0, 900.0))
}

/// An OTLP attribute with a string value.
fn string_attr(key: &str, value: &str) -> Value {
    json!({ "key": key, "value": { "stringValue": value } })
}

/// A JSON number, as expo-observe sends it.
fn nanos(seconds: f64) -> u64 {
    (seconds * 1000.0).round() as u64 * 1_000_000
}

/// Observe's `otAnyValue`: a value it can't represent is dropped and counted.
fn any_value(v: &Value) -> Option<Value> {
    Some(match v {
        Value::Bool(b) => json!({ "boolValue": b }),
        Value::Number(n) if n.is_i64() => json!({ "intValue": n.as_i64() }),
        Value::Number(n) => {
            let f = n.as_f64()?;
            if f.fract() == 0.0 && f.abs() < 9.0e15 {
                json!({ "intValue": f as i64 })
            } else {
                json!({ "doubleValue": f })
            }
        }
        Value::String(s) => json!({ "stringValue": s }),
        Value::Array(a) => {
            json!({ "arrayValue": { "values": a.iter().map(any_value).collect::<Option<Vec<_>>>()? } })
        }
        Value::Object(o) => {
            json!({ "kvlistValue": { "values": o.iter().map(|(k, x)| any_value(x).map(|m| json!({ "key": k, "value": m }))).collect::<Option<Vec<_>>>()? } })
        }
        Value::Null => return None,
    })
}

fn read_json(path: &std::path::Path) -> Value {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The device state Observe attaches to TTI, read when the startup report arrives.
/// Low-power mode comes from the ACPI platform profile. Missing hardware is left out.
fn device() -> Map<String, Value> {
    let mut d = Map::new();
    if let Some(profile) = read("/sys/firmware/acpi/platform_profile") {
        d.insert(
            "expo.device.lowPowerMode".into(),
            (profile == "low-power" || profile == "quiet").into(),
        );
    }
    for entry in std::fs::read_dir("/sys/class/power_supply")
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = entry.path();
        if read(&format!("{}/type", p.display())).as_deref() != Some("Battery") {
            continue;
        }
        if let Some(c) =
            read(&format!("{}/capacity", p.display())).and_then(|c| c.parse::<f64>().ok())
        {
            d.insert("expo.device.batteryLevel".into(), json!(c / 100.0));
        }
        if let Some(s) = read(&format!("{}/status", p.display())) {
            d.insert(
                "expo.device.batteryCharging".into(),
                (s == "Charging" || s == "Full").into(),
            );
        }
        break;
    }
    let mut kind = None;
    for entry in std::fs::read_dir("/sys/class/net")
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = entry.path();
        if entry.file_name() == "lo"
            || read(&format!("{}/operstate", p.display())).as_deref() != Some("up")
        {
            continue;
        }
        let wifi = p.join("wireless").exists();
        if kind.is_none() || wifi {
            kind = Some(if wifi {
                "wifi"
            } else if p.join("device").exists() {
                "ethernet"
            } else {
                "other"
            });
        }
    }
    d.insert("expo.network.connected".into(), kind.is_some().into());
    d.insert("expo.network.type".into(), kind.unwrap_or("none").into());
    d
}

type Row = Map<String, Value>;

/// Removes from `queue` one row equal to each of `sent`, the earliest first.
fn remove_sent(queue: &mut Vec<Row>, sent: &[Row]) {
    let key = |row: &Row| serde_json::to_string(row).unwrap_or_default();
    let mut left = std::collections::HashMap::<String, usize>::new();
    for row in sent {
        *left.entry(key(row)).or_default() += 1;
    }
    queue.retain(|row| match left.get_mut(&key(row)) {
        Some(n) if *n > 0 => {
            *n -= 1;
            false
        }
        _ => true,
    });
}

/// Appends a row, dropping the oldest past `MAX_ROWS`.
fn push_capped(queue: &mut Vec<Row>, row: Row) {
    queue.push(row);
    if queue.len() > MAX_ROWS {
        queue.remove(0);
    }
}

/// The JSON object `v` builds, as a map.
fn object(v: Value) -> Map<String, Value> {
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

struct Service {
    /// `moduleConfig.observe`. Indexing a missing key gives `null`.
    config: Value,
    development: bool,
    dir: PathBuf,
    session: String,
    /// This session's entry in `sessions`: its start and metadata.
    entry: Value,
    client_id: String,
    /// The queue file as this process last read or wrote it.
    sessions: Map<String, Value>,
    metrics: Vec<Row>,
    logs: Vec<Row>,
    globals: Map<String, Value>,
    launch_route: Option<Value>,
    navigated: bool,
    /// Reads the device state at TTI. Tests replace it.
    device: fn() -> Map<String, Value>,
    due: Option<Instant>,
    gate: (Option<Instant>, u32),
}

impl Service {
    fn new(
        config: Map<String, Value>,
        app: &Value,
        development: bool,
        dir: PathBuf,
        session: String,
        start: f64,
    ) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let id_file = dir.join("eas-client-id");
        let client_id = read(&id_file.to_string_lossy()).unwrap_or_else(|| {
            let id = uuid();
            let _ = std::fs::write(&id_file, &id);
            id
        });
        let mut s = Service {
            development,
            dir,
            client_id,
            entry: Value::Null,
            sessions: Map::new(),
            metrics: Vec::new(),
            logs: Vec::new(),
            globals: Map::new(),
            launch_route: None,
            navigated: false,
            device,
            due: Some(Instant::now() + DEBOUNCE),
            gate: (None, 0),
            session,
            config: Value::Object(config),
        };
        let mut meta = object(json!({
            "osName": "Linux",
            "osVersion": read("/proc/sys/kernel/osrelease"),
            "deviceModel": read("/sys/devices/virtual/dmi/id/product_name"),
            // `en_US.UTF-8` becomes `en-US`. `C` and `POSIX` mean no language.
            "language": std::env::var("LANG").ok().map(|l| l.split('.').next().unwrap_or("").replace('_', "-"))
                .filter(|l| !l.is_empty() && l != "C" && l != "POSIX"),
            "clientVersion": CLIENT_VERSION,
        }));
        meta.extend(app_meta(app, &s.config, development));
        s.entry = json!({ "start": start, "meta": meta });
        if log_enabled() {
            eprintln!("observe: service started, session {}", s.session);
        }
        let _queue = s.hold("queue.lock");
        s.reload();
        let records = s.ingest_crashes();
        // A pending record is the only copy of its panic: it goes only once the queue
        // that holds the panic is on disk, and stays for the next launch otherwise.
        if s.commit(true).is_ok() {
            for path in records {
                let _ = std::fs::remove_file(path);
            }
        }
        s
    }

    fn run(mut self, rx: mpsc::Receiver<Value>) {
        loop {
            let wait = self.due.map_or(Duration::from_secs(3600), |d| {
                d.saturating_duration_since(Instant::now())
            });
            match rx.recv_timeout(wait) {
                Ok(e) => self.event(&e),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return self.dispatch(),
            }
            if self.due.is_some_and(|d| Instant::now() >= d) {
                self.due = None;
                self.dispatch();
            }
        }
    }

    fn schedule(&mut self) {
        self.due.get_or_insert_with(|| Instant::now() + DEBOUNCE);
    }

    /// An exclusive lock on the file `name` in the state directory, held until the file
    /// is dropped. When the directory cannot be written there is none, and the process
    /// goes on without it.
    fn hold(&self, name: &str) -> Option<std::fs::File> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(name))
            .ok()?;
        file.lock().ok()?;
        Some(file)
    }

    /// Reads the queue file, which other processes of the app may have changed. Its
    /// session metadata gets this session's entry, which another process drops while
    /// no queued row refers to it.
    fn reload(&mut self) {
        let queue = read_json(&self.dir.join("queue.json"));
        let rows = |k: &str| -> Vec<Row> {
            queue[k]
                .as_array()
                .map(|a| a.iter().filter_map(|r| r.as_object().cloned()).collect())
                .unwrap_or_default()
        };
        self.metrics = rows("metrics");
        self.logs = rows("logs");
        self.sessions = queue["sessions"].as_object().cloned().unwrap_or_default();
        self.sessions
            .insert(self.session.clone(), self.entry.clone());
    }

    /// Writes the queue file, forgetting sessions no queued row refers to, except this one.
    fn save(&mut self) {
        let _ = self.commit(false);
    }

    /// Replaces the queue file with this process's copy; `durable` also syncs the new
    /// file and the rename to disk before returning.
    fn commit(&mut self, durable: bool) -> std::io::Result<()> {
        let live: std::collections::HashSet<&str> = self
            .metrics
            .iter()
            .chain(&self.logs)
            .filter_map(|r| r["session"].as_str())
            .collect();
        let current = self.session.as_str();
        self.sessions
            .retain(|s, _| s == current || live.contains(s.as_str()));
        let q = json!({ "sessions": self.sessions, "metrics": self.metrics, "logs": self.logs });
        let tmp = self.dir.join("queue.json.tmp");
        let mut file = std::fs::File::create(&tmp)?;
        std::io::Write::write_all(&mut file, q.to_string().as_bytes())?;
        if durable {
            file.sync_all()?;
        }
        std::fs::rename(&tmp, self.dir.join("queue.json"))?;
        if durable {
            std::fs::File::open(&self.dir)?.sync_all()?;
        }
        Ok(())
    }

    fn metric(
        &mut self,
        category: &str,
        name: &str,
        value: f64,
        wall: f64,
        route: Option<&Value>,
        params: Map<String, Value>,
    ) {
        let mut r = object(json!({
            "session": self.session, "time": wall, "category": category, "name": name, "value": value,
        }));
        if let Some(route) = route.filter(|r| r.is_string()) {
            r.insert("route".into(), route.clone());
        }
        if !params.is_empty() {
            r.insert("params".into(), Value::Object(params).to_string().into());
        }
        push_capped(&mut self.metrics, r);
    }

    fn log(
        &mut self,
        session: String,
        wall: f64,
        severity: &str,
        name: &str,
        body: Option<String>,
        (attributes, dropped): (Map<String, Value>, u64),
    ) {
        let mut r = object(json!({
            "session": session, "time": wall, "severity": severity, "name": name,
            "attributes": attributes, "dropped": dropped,
        }));
        if let Some(b) = body {
            r.insert("body".into(), b.into());
        }
        push_capped(&mut self.logs, r);
    }

    fn event(&mut self, e: &Value) {
        let _queue = self.hold("queue.lock");
        self.reload();
        let wall = e["wall"].as_f64().unwrap_or_else(journal::wall);
        match e["kind"].as_str().unwrap_or("") {
            "startup" => self.startup(e, wall),
            "navigation.launch" => {
                self.launch_route = Some(e.clone());
                return;
            }
            "navigation" => self.navigation(e, wall),
            "app.attributes" => {
                self.globals = rules::attributes(&e["attributes"]).0;
                return;
            }
            "app.event" => {
                let Some(name) = rules::name(e["name"].as_str().unwrap_or("")) else {
                    eprintln!("observe: dropped an event with an invalid name");
                    return;
                };
                let (kept, dropped) = rules::attributes(&e["attributes"]);
                let mut attrs = self.globals.clone();
                attrs.extend(kept);
                if let Some(d) = e["displayName"]
                    .as_str()
                    .map(str::trim)
                    .filter(|d| !d.is_empty())
                {
                    attrs.insert(
                        "expo.log.display_name".into(),
                        rules::truncate(d, 128).into(),
                    );
                }
                let severity = e["severity"]
                    .as_str()
                    .filter(|s| severity_number(s).is_some())
                    .unwrap_or("info");
                let body = e["body"].as_str().map(|b| rules::truncate(b, 4096));
                self.log(
                    self.session.clone(),
                    wall,
                    severity,
                    &name,
                    body,
                    (attrs, dropped),
                );
            }
            "app.error" => {
                let mut attrs = object(json!({
                    "expo.error.source": e["source"].as_str().unwrap_or("reportedByUser"),
                    "expo.error.is_fatal": false,
                    "exception.type": e["type"].as_str().unwrap_or("Error"),
                    "exception.message": e["message"].as_str().unwrap_or(""),
                }));
                if let Some(stack) = e["stack"].as_str() {
                    attrs.insert("exception.stacktrace".into(), stack.into());
                }
                self.log(
                    self.session.clone(),
                    wall,
                    "error",
                    "js.exception",
                    None,
                    (attrs, 0),
                );
            }
            _ => return,
        }
        self.save();
        self.schedule();
    }

    /// Stores the launch metrics from the host's startup report under Observe's names.
    fn startup(&mut self, e: &Value, wall: f64) {
        let marks = &e["marks"];
        let ms = |a: &str, b: &str| {
            Some(json!(
                ((marks[a].as_f64()? - marks[b].as_f64()?) * 10.0).round() / 10_000.0
            ))
        };
        let mut phases = Map::new();
        for (k, v) in [
            ("exact.phase.boot", ms("commit", "boot")),
            ("exact.phase.present", ms("present", "commit")),
            ("exact.phase.activate", ms("activated", "commit")),
            ("exact.since_process_start.ttr", ms("present", "process")),
            (
                "exact.since_process_start.tti",
                ms("interactive", "process"),
            ),
        ] {
            if let Some(v) = v {
                phases.insert(k.into(), v);
            }
        }
        // The first frame counts as shown when its synchronous modeset returns.
        phases.insert("exact.present.method".into(), "modeset".into());
        if let Some(path) = e["bootPath"].as_str() {
            phases.insert("exact.boot.path".into(), path.into());
        }
        if let Some(r) = e["processResolutionMs"].as_f64() {
            phases.insert("exact.launch.resolution_ms".into(), json!(r));
        }
        let device = (self.device)();
        let launch = self.launch_type();
        for (name, value) in e["metrics"].as_object().into_iter().flatten() {
            let Some(value) = value.as_f64() else {
                continue;
            };
            let name = &if name == "coldLaunchTime" {
                format!("{launch}LaunchTime")
            } else {
                name.clone()
            };
            let mut params = self.globals.clone();
            params.extend(phases.clone());
            if name == "timeToInteractive" {
                params.extend(device.clone());
                params.insert("exact.tti.reason".into(), e["tti"].clone());
                // What was outstanding over time, ms from process start: shows what held TTI.
                if let Some(trace) = e["trace"].as_array().filter(|t| !t.is_empty()) {
                    let steps: Vec<&str> = trace.iter().filter_map(Value::as_str).collect();
                    params.insert("exact.tti.trace".into(), steps.join(" → ").into());
                }
            }
            self.metric("appStartup", name, value, wall, None, params);
        }
        // The launch route's `cold_ttr` and `tti`, measured from boot. Observe measures
        // from its integration start, which has no Exact equivalent.
        if let (Some(route), Some(boot)) = (self.launch_route.clone(), marks["boot"].as_f64()) {
            let mut params = self.nav_params(&route, true);
            params.insert("exact.nav.anchor".into(), "boot".into());
            if let Some(present) = marks["present"].as_f64() {
                self.metric(
                    "navigation",
                    "cold_ttr",
                    (present - boot) / 1000.0,
                    wall,
                    Some(&route["route"]),
                    params.clone(),
                );
            }
            if let (Some(i), false, true) = (
                marks["interactive"].as_f64(),
                self.navigated,
                e["metrics"]["timeToInteractive"].is_number(),
            ) {
                self.metric(
                    "navigation",
                    "tti",
                    (i - boot) / 1000.0,
                    wall,
                    Some(&route["route"]),
                    params,
                );
            }
        }
    }

    /// Warm when the last launch was the same build in the same OS boot and no terminal
    /// is attached, otherwise cold (Observe's iOS rule). The build is the executable's
    /// size and mtime. Called after TTI because it does file I/O.
    fn launch_type(&self) -> &'static str {
        use std::io::IsTerminal;
        let boot = read("/proc/sys/kernel/random/boot_id");
        let build = std::fs::metadata("/proc/self/exe").ok().map(|m| {
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            format!("{}:{}", m.len(), mtime.map_or(0, |d| d.as_nanos()))
        });
        let path = self.dir.join("launch.json");
        let previous = read_json(&path);
        let _ = std::fs::write(&path, json!({ "boot": boot, "build": build }).to_string());
        let same = boot.is_some()
            && previous["boot"] == json!(boot)
            && build.is_some()
            && previous["build"] == json!(build);
        if same && !std::io::stdin().is_terminal() {
            "warm"
        } else {
            "cold"
        }
    }

    /// Observe's navigation params, on top of the global attributes.
    fn nav_params(&self, e: &Value, launch: bool) -> Map<String, Value> {
        let mut params = self.globals.clone();
        let route_params = match &e["routeParams"] {
            Value::Null => json!({}),
            p => p.clone(),
        };
        params.extend(object(
            json!({ "isAppLaunch": launch, "routeParams": route_params }),
        ));
        if !e["url"].is_null() {
            params.insert("url".into(), e["url"].clone());
        }
        params
    }

    fn navigation(&mut self, e: &Value, wall: f64) {
        let (Some(name), Some(value)) = (e["name"].as_str(), e["value"].as_f64()) else {
            return;
        };
        self.navigated = true;
        let mut params = self.nav_params(e, false);
        for (k, v) in e
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.starts_with("exact."))
        {
            params.insert(k.clone(), v.clone());
        }
        self.metric("navigation", name, value, wall, Some(&e["route"]), params);
    }

    /// Turns each pending panic record from an earlier process into a `native.exception`
    /// log, and returns the records' paths, which the caller removes once the queue is saved.
    fn ingest_crashes(&mut self) -> Vec<PathBuf> {
        let mut records = Vec::new();
        for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with("pending-") || name.contains(&self.session) {
                continue;
            }
            let path = entry.path();
            let o = read_json(&path);
            records.push(path);
            let Some(crashed) = o["session"].as_str() else {
                continue;
            };
            if !self.sessions.contains_key(crashed) {
                // The device's metadata is this process's; the build's is the record's.
                let mut meta = self.sessions[&self.session]["meta"].clone();
                if let (Some(meta), Some(build)) = (meta.as_object_mut(), o["meta"].as_object()) {
                    meta.extend(build.clone());
                }
                self.sessions.insert(
                    crashed.into(),
                    json!({ "start": o["sessionStart"], "meta": meta }),
                );
            }
            let mut attrs = object(json!({
                "exception.type": o["type"].as_str().unwrap_or("panic"),
                "exception.message": o["message"].as_str().unwrap_or(""),
                "expo.error.source": "nativeCrash",
                "expo.error.is_fatal": true,
            }));
            if let Some(at) = o["location"].as_str() {
                attrs.insert("exception.stacktrace".into(), format!("at {at}").into());
            }
            let time = o["time"].as_f64().unwrap_or_else(journal::wall);
            self.log(
                crashed.into(),
                time,
                "fatal",
                "native.exception",
                None,
                (attrs, 0),
            );
        }
        records
    }

    fn resource(&self, meta: &Value) -> Value {
        let mut a = vec![string_attr("os.type", "linux")];
        let mut s = |k: &str, v: &Value| a.extend(v.as_str().map(|v| string_attr(k, v)));
        s("os.name", &meta["osName"]);
        s("os.version", &meta["osVersion"]);
        s("device.model.name", &meta["deviceName"]);
        s("device.model.identifier", &meta["deviceModel"]);
        s("browser.language", &meta["language"]);
        s("telemetry.sdk.name", &json!("exact-observe"));
        s("telemetry.sdk.version", &meta["clientVersion"]);
        s("telemetry.sdk.language", &json!("rust"));
        s("expo.eas_client.id", &json!(self.client_id));
        s("service.name", &meta["appIdentifier"]);
        s("service.version", &meta["appVersion"]);
        s("expo.app.name", &meta["appName"]);
        s("expo.environment", &meta["environment"]);
        json!({ "attributes": a })
    }

    /// `{"resourceMetrics":[…]}` or `{"resourceLogs":[…]}`, one resource per session.
    fn body(&self, signal: &str, rows: &[Row]) -> Value {
        let mut by: Vec<(String, Vec<Value>)> = Vec::new();
        for r in rows {
            let session = r["session"].as_str().unwrap_or("").to_string();
            let time = r["time"].as_f64().unwrap_or(0.0);
            let mut attrs = vec![string_attr("session.id", &session)];
            let item = if signal == "metrics" {
                let text = |k: &str| r.get(k).and_then(Value::as_str);
                attrs.extend(text("route").map(|v| string_attr("expo.route_name", v)));
                attrs.extend(text("params").map(|v| string_attr("expo.custom_params", v)));
                let name = metric_name(
                    r["category"].as_str().unwrap_or(""),
                    r["name"].as_str().unwrap_or(""),
                );
                json!({ "unit": "s", "name": name, "gauge": { "dataPoints": [{ "timeUnixNano": nanos(time), "asDouble": r["value"], "attributes": attrs }] } })
            } else {
                let name = r["name"].as_str().unwrap_or("");
                attrs.push(string_attr("event.name", name));
                let mut dropped = r.get("dropped").and_then(Value::as_u64).unwrap_or(0);
                let mut keys: Vec<(&String, &Value)> = r
                    .get("attributes")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                    .collect();
                keys.sort_by(|a, b| a.0.cmp(b.0));
                for (k, v) in keys {
                    match any_value(v) {
                        Some(m) => attrs.push(json!({ "key": k, "value": m })),
                        None => dropped += 1,
                    }
                }
                let severity = r["severity"].as_str().unwrap_or("info");
                let mut rec = json!({
                    "timeUnixNano": nanos(time), "observedTimeUnixNano": nanos(time),
                    "severityNumber": severity_number(severity).unwrap_or(9), "severityText": severity.to_uppercase(),
                    "body": { "stringValue": r.get("body").and_then(Value::as_str).unwrap_or("") }, "attributes": attrs,
                });
                if dropped > 0 {
                    rec["droppedAttributesCount"] = dropped.into();
                }
                rec
            };
            match by.iter_mut().find(|(s, _)| *s == session) {
                Some((_, items)) => items.push(item),
                None => by.push((session, vec![item])),
            }
        }
        let (outer, inner, list) = if signal == "metrics" {
            ("resourceMetrics", "scopeMetrics", "metrics")
        } else {
            ("resourceLogs", "scopeLogs", "logRecords")
        };
        let resources: Vec<Value> = by
            .into_iter()
            .filter_map(|(s, items)| {
                let meta = &self.sessions.get(&s)?["meta"];
                let scope = json!({ "name": "expo-observe", "version": meta["clientVersion"].as_str().unwrap_or("0") });
                Some(json!({ "resource": self.resource(meta), inner: [{ "scope": scope, list: items }], "schemaUrl": SCHEMA_URL }))
            })
            .collect();
        json!({ outer: resources })
    }

    /// Observe's gate: dispatching enabled, this install in the sample, and not a development
    /// build unless `dispatchInDebug`. When it fails, rows are dropped, not kept for later.
    fn should_dispatch(&self) -> bool {
        let c = &self.config;
        let rate = c["sampleRate"].as_f64().unwrap_or(1.0).clamp(0.0, 1.0);
        c["dispatchingEnabled"].as_bool().unwrap_or(true)
            && uniform(&self.client_id) < rate
            && (!self.development || c["dispatchInDebug"] == true)
    }

    fn dispatch(&mut self) {
        if self.gate.0.is_some_and(|after| Instant::now() < after) {
            self.due = self.gate.0;
            return;
        }
        let _sending = self.hold("send.lock");
        let project = self.config["projectId"].as_str().map(str::to_string);
        let Some(project) = project.filter(|_| self.should_dispatch()) else {
            let _queue = self.hold("queue.lock");
            self.reload();
            self.metrics.clear();
            self.logs.clear();
            return self.save();
        };
        let base = self.config["endpoint"]
            .as_str()
            .unwrap_or("https://o.expo.dev")
            .trim_end_matches('/')
            .to_string();
        if self.send("metrics", &format!("{base}/{project}/v1/metrics")) {
            self.send("logs", &format!("{base}/{project}/v1/logs"));
        }
    }

    /// Sends one signal's rows chunk by chunk, as expo-observe's `DispatchLoop.swift` does.
    /// Returns false when it has to back off.
    fn send(&mut self, signal: &str, url: &str) -> bool {
        let mut limit = CHUNK;
        loop {
            let rows: Vec<Row> = {
                let _queue = self.hold("queue.lock");
                self.reload();
                let queue = self.queue(signal);
                queue[..limit.min(queue.len())].to_vec()
            };
            if rows.is_empty() {
                return true;
            }
            let body = self.body(signal, &rows).to_string().into_bytes();
            let headers = [
                ("content-type", "application/json"),
                ("expo-appmetrics-skip", "1"),
            ];
            let result = journal::post(url, &headers, body);
            if log_enabled() {
                let status = result
                    .as_ref()
                    .map_or_else(|e| format!("transport error ({e})"), |(s, _)| s.to_string());
                eprintln!("observe: {signal} {} rows → {status}", rows.len());
            }
            match result {
                Ok((413, _)) if rows.len() > 1 => {
                    self.gate.1 = 0;
                    limit = rows.len() / 2;
                }
                // Success, a lone 413 row, or a status Observe does not retry: drop the chunk,
                // and only its rows. During the request other processes may have added rows,
                // and the capacity trim dropped old ones.
                Ok((status, _)) if !matches!(status, 429 | 502 | 503 | 504) => {
                    self.gate.1 = 0;
                    let _queue = self.hold("queue.lock");
                    self.reload();
                    remove_sent(self.queue(signal), &rows);
                    self.save();
                    limit = CHUNK;
                }
                retryable => {
                    self.gate.1 += 1;
                    let after = retryable.ok().and_then(|(_, after)| after);
                    let wait =
                        retry_after(after.as_deref()).unwrap_or_else(|| backoff(self.gate.1));
                    self.gate.0 = Some(Instant::now() + Duration::from_secs_f64(wait));
                    self.due = self.gate.0;
                    return false;
                }
            }
        }
    }

    fn queue(&mut self, signal: &str) -> &mut Vec<Row> {
        if signal == "metrics" {
            &mut self.metrics
        } else {
            &mut self.logs
        }
    }
}
