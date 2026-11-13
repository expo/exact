use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

/// Requests the sink received: path, row count and JSON body.
type Seen = Arc<Mutex<Vec<(String, usize, Value)>>>;

/// A local HTTP sink on a free port. It replies with the status and headers
/// that `answer(row count)` returns.
fn sink(answer: impl Fn(usize) -> (u16, &'static str) + Send + 'static) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split(' ').nth(1).unwrap_or("").to_string();
            let mut length = 0;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body: Value = serde_json::from_slice(&body).unwrap();
            let rows = body.to_string().matches("\"timeUnixNano\"").count();
            log.lock().unwrap().push((path, rows, body));
            let (status, headers) = answer(rows);
            let mut out = stream;
            let _ = write!(
                out,
                "HTTP/1.1 {status} X\r\n{headers}Content-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
    });
    (url, seen)
}

fn service(endpoint: &str, name: &str) -> Service {
    let dir =
        std::env::temp_dir().join(format!("exact-observe-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let config = json!({ "projectId": "p1", "endpoint": endpoint, "dispatchInDebug": true });
    let app = json!({ "id": "com.example.test", "name": "Test", "version": "1.2" });
    Service::new(
        config.as_object().unwrap().clone(),
        &app,
        true,
        dir,
        "s-1".into(),
        1_700_000_000.0,
    )
}

fn startup(s: &mut Service, n: usize) {
    for i in 0..n {
        let e = json!({ "kind": "startup", "wall": 1_700_000_001.0 + i as f64, "tti": "settled",
            "metrics": { "coldLaunchTime": 0.004, "timeToFirstRender": 0.1, "timeToInteractive": 0.2 },
            "marks": { "process": 0.0, "boot": 4.0, "commit": 50.0, "present": 104.0, "activated": 60.0, "interactive": 204.0 } });
        s.event(&e);
    }
}

#[test]
fn startup_becomes_observes_metrics_on_its_wire() {
    let (url, seen) = sink(|_| (200, ""));
    let mut s = service(&url, "wire");
    startup(&mut s, 2);
    s.dispatch();
    assert!(s.metrics.is_empty(), "acknowledged rows leave the queue");
    let seen = seen.lock().unwrap();
    let (path, rows, body) = &seen[0];
    assert_eq!((path.as_str(), *rows), ("/p1/v1/metrics", 6));
    let resource = &body["resourceMetrics"][0];
    assert_eq!(resource["schemaUrl"], SCHEMA_URL);
    assert_eq!(resource["scopeMetrics"][0]["scope"]["name"], "expo-observe");
    let attrs = resource["resource"]["attributes"].as_array().unwrap();
    let attr = |k: &str| {
        attrs
            .iter()
            .find(|a| a["key"] == k)
            .map(|a| a["value"]["stringValue"].clone())
    };
    assert_eq!(attr("service.name"), Some(json!("com.example.test")));
    assert_eq!(attr("telemetry.sdk.name"), Some(json!("exact-observe")));
    assert_eq!(attr("os.type"), Some(json!("linux")));
    let metrics = resource["scopeMetrics"][0]["metrics"].as_array().unwrap();
    let names: Vec<&str> = metrics
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    // A second launch of the same build in the same boot is warm. Warm
    // launches are detected through /proc's boot_id, so without it every launch is cold.
    let second = if std::path::Path::new("/proc/sys/kernel/random/boot_id").exists()
        && !std::io::IsTerminal::is_terminal(&std::io::stdin())
    {
        "expo.app_startup.warm_launch_time"
    } else {
        "expo.app_startup.cold_launch_time"
    };
    assert_eq!(
        names[..4],
        [
            "expo.app_startup.cold_launch_time",
            "expo.app_startup.ttr",
            "expo.app_startup.tti",
            second
        ]
    );
    assert!(metrics.iter().all(|m| m["unit"] == "s"));
    let tti = &metrics[2]["gauge"]["dataPoints"][0];
    assert_eq!(tti["timeUnixNano"], 1_700_000_001_000_000_000u64);
    let params = tti["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["key"] == "expo.custom_params")
        .unwrap();
    let params: Value =
        serde_json::from_str(params["value"]["stringValue"].as_str().unwrap()).unwrap();
    assert_eq!(params["exact.tti.reason"], "settled");
    assert_eq!(params["exact.phase.present"], json!(0.054));
    assert_eq!(params["exact.since_process_start.tti"], json!(0.204));
    assert!(
        params.get("expo.network.connected").is_some(),
        "TTI carries the device state"
    );
}

#[test]
fn a_413_halves_the_chunk_and_drops_a_lone_row() {
    // The sink rejects any request with more than two rows.
    let (url, seen) = sink(|rows| if rows > 2 { (413, "") } else { (200, "") });
    let mut s = service(&url, "large");
    startup(&mut s, 2); // six rows
    s.dispatch();
    assert!(s.metrics.is_empty());
    let sizes: Vec<usize> = seen.lock().unwrap().iter().map(|r| r.1).collect();
    assert_eq!(
        sizes,
        [6, 3, 1, 5, 2, 3, 1, 2],
        "halved until accepted; a success resets to the full chunk"
    );
    let (url, seen) = sink(|_| (413, ""));
    let mut s = service(&url, "lone");
    startup(&mut s, 1);
    s.dispatch();
    assert!(
        s.metrics.is_empty(),
        "a single row that is still too large is dropped"
    );
    assert_eq!(
        seen.lock().unwrap().iter().map(|r| r.1).collect::<Vec<_>>(),
        [3, 1, 2, 1, 1]
    );
}

#[test]
fn a_retryable_status_keeps_the_rows_and_waits_retry_after() {
    let (url, seen) = sink(|_| (503, "Retry-After: 5\r\n"));
    let mut s = service(&url, "retry");
    startup(&mut s, 1);
    s.dispatch();
    assert_eq!(s.metrics.len(), 3, "kept for later");
    let wait = s
        .gate
        .0
        .unwrap()
        .saturating_duration_since(Instant::now())
        .as_secs_f64();
    assert!(
        (55.0..=60.0).contains(&wait),
        "Retry-After clamped to at least 60 s, got {wait}"
    );
    s.dispatch();
    assert_eq!(
        seen.lock().unwrap().len(),
        1,
        "nothing is sent before the gate opens"
    );
    let (url, _) = sink(|_| (400, ""));
    let mut s = service(&url, "fatal");
    startup(&mut s, 1);
    s.dispatch();
    assert!(
        s.metrics.is_empty(),
        "a status Observe does not retry drops the chunk"
    );
}

#[test]
fn a_failed_export_retries_while_idle() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let count = AtomicUsize::new(0);
    let (attempts, received) = mpsc::channel();
    let (url, seen) = sink(move |_| {
        let attempt = count.fetch_add(1, Ordering::SeqCst) + 1;
        attempts.send(attempt).unwrap();
        if attempt == 1 {
            (503, "Retry-After: 60\r\n")
        } else {
            (200, "")
        }
    });
    let mut s = service(&url, "idle-retry");
    s.event(&json!({ "kind": "app.event", "name": "idle.retry" }));
    s.due = None;
    s.dispatch();
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 1);
    assert!(s.gate.0.is_some());
    assert_eq!(s.due, s.gate.0, "a failed attempt schedules its retry");

    // Advance both deadlines to exercise the idle worker without waiting a minute
    let now = Instant::now();
    s.gate.0 = Some(now);
    s.due = Some(now);
    let (events, idle) = mpsc::channel();
    let worker = std::thread::spawn(move || s.run(idle));
    assert_eq!(received.recv_timeout(Duration::from_secs(5)).unwrap(), 2);
    drop(events);
    worker.join().unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn custom_events_follow_observes_rules() {
    let (url, seen) = sink(|_| (200, ""));
    let mut s = service(&url, "events");
    s.event(
        &json!({ "kind": "app.attributes", "attributes": { "tier": "pro", "expo.reserved": 1 } }),
    );
    s.event(&json!({ "kind": "app.event", "name": "  checkout.done ", "severity": "warn", "body": "x".repeat(5000),
        "attributes": { "items": 3, "total": 4.5, "session.id": "spoof", "none": null } }));
    s.event(&json!({ "kind": "app.event", "name": "expo.mine" }));
    s.event(&json!({ "kind": "app.error", "message": "boom", "type": "CartError" }));
    s.dispatch();
    let seen = seen.lock().unwrap();
    let (path, _, body) = seen.iter().find(|r| r.0.ends_with("/logs")).unwrap();
    assert_eq!(path, "/p1/v1/logs");
    let records = body["resourceLogs"][0]["scopeLogs"][0]["logRecords"]
        .as_array()
        .unwrap();
    assert_eq!(records.len(), 2, "an `expo.` name is refused");
    let event = &records[0];
    let attrs: Vec<(&str, &Value)> = event["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["key"].as_str().unwrap(), &a["value"]))
        .collect();
    let keys: Vec<&str> = attrs.iter().map(|a| a.0).collect();
    assert_eq!(keys, ["session.id", "event.name", "items", "tier", "total"]);
    assert_eq!(attrs[1].1["stringValue"], "checkout.done");
    assert_eq!(attrs[2].1, &json!({ "intValue": 3 }));
    assert_eq!(attrs[4].1, &json!({ "doubleValue": 4.5 }));
    assert_eq!(
        event["droppedAttributesCount"], 2,
        "the reserved key and the null"
    );
    assert_eq!(
        (
            event["severityNumber"].clone(),
            event["severityText"].clone()
        ),
        (json!(13), json!("WARN"))
    );
    let body_text = event["body"]["stringValue"].as_str().unwrap();
    assert_eq!(
        (body_text.chars().count(), body_text.ends_with('…')),
        (4096, true)
    );
    let error: Vec<&str> = records[1]["attributes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["key"].as_str().unwrap())
        .collect();
    assert!(error.contains(&"exception.type") && records[1]["severityText"] == "ERROR");
}

#[test]
fn sampling_matches_eas_client_ids_uniform_value() {
    // The value web/service.js's BigInt splitmix64 gives for the same id.
    assert_eq!(
        uniform("f0e0ebad-d595-40fe-aa76-5ac43756f758"),
        0.2509529660831642
    );
}

#[test]
fn launch_defers_subscription_and_recovers_a_panic() {
    let state =
        std::env::temp_dir().join(format!("exact-observe-test-panic-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&state);
    std::env::set_var("XDG_STATE_HOME", &state);
    let (attempts, received) = mpsc::channel();
    let (url, seen) = sink(move |rows| {
        attempts.send(rows).unwrap();
        (200, "")
    });
    let config = json!({ "projectId": "p1", "endpoint": url, "dispatchInDebug": true });
    let ctx = LaunchContext {
        module: "observe",
        config: Box::leak(config.to_string().into_boxed_str()),
        app: r#"{"id":"com.example.crash"}"#,
        development: true,
    };
    let dir = ctx.state_dir();
    launch(ctx);
    let _ = std::panic::catch_unwind(|| panic!("index out of bounds"));
    let pending: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(pending.len(), 1);
    let record: Value = serde_json::from_slice(&std::fs::read(&pending[0]).unwrap()).unwrap();
    assert_eq!(record["message"], "index out of bounds");
    let mut next = Service::new(
        Map::new(),
        &json!({}),
        true,
        dir,
        "next-session".into(),
        0.0,
    );
    assert_eq!(next.logs.len(), 1);
    let row = &next.logs[0];
    assert_eq!(
        (row["severity"].as_str(), row["name"].as_str()),
        (Some("fatal"), Some("native.exception"))
    );
    assert_eq!(
        row["session"], record["session"],
        "against the session that crashed"
    );
    assert_eq!(row["attributes"]["expo.error.is_fatal"], true);
    next.dispatch(); // no project id: the rows are dropped, as Observe out of its gate
    assert!(next.logs.is_empty());
    assert_eq!(
        next.sessions.len(),
        1,
        "refused rows leave no orphan session metadata"
    );
    assert!(next.sessions.contains_key(&next.session));

    journal::record("app.event", object(json!({ "name": "evicted" })));
    for _ in 0..4096 {
        journal::record("test.ignored", Map::new());
    }
    journal::record("app.event", object(json!({ "name": "retained" })));
    journal::record("startup", Map::new());
    assert_eq!(received.recv_timeout(Duration::from_secs(10)).unwrap(), 1);
    let seen = seen.lock().unwrap();
    let attributes = &seen[0].2["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["attributes"];
    assert!(attributes
        .as_array()
        .unwrap()
        .iter()
        .any(|a| { a["key"] == "event.name" && a["value"]["stringValue"] == "retained" }));
}

/// The event names of every logs request the sink received, sorted.
fn posted_names(seen: &Seen) -> Vec<String> {
    let mut names: Vec<String> = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|r| r.0.ends_with("/logs"))
        .flat_map(|r| r.2["resourceLogs"].as_array().unwrap().clone())
        .flat_map(|r| r["scopeLogs"][0]["logRecords"].as_array().unwrap().clone())
        .map(|l| {
            let attrs = l["attributes"].as_array().unwrap();
            let name = attrs.iter().find(|a| a["key"] == "event.name").unwrap();
            name["value"]["stringValue"].as_str().unwrap().to_string()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn two_processes_add_to_one_queue_and_send_each_row_once() {
    // The sink answers after a pause, so the two dispatches below overlap.
    let (url, seen) = sink(|_| {
        std::thread::sleep(Duration::from_millis(100));
        (200, "")
    });
    let dir = std::env::temp_dir().join(format!(
        "exact-observe-test-processes-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let config = json!({ "projectId": "p1", "endpoint": url, "dispatchInDebug": true });
    let open = |session: &str| {
        Service::new(
            config.as_object().unwrap().clone(),
            &json!({ "id": "com.example.processes" }),
            true,
            dir.clone(),
            session.into(),
            0.0,
        )
    };
    let event =
        |s: &mut Service, name: &str| s.event(&json!({ "kind": "app.event", "name": name }));
    let stored = || -> Vec<Value> {
        let logs = read_json(&dir.join("queue.json"))["logs"].clone();
        logs.as_array()
            .unwrap()
            .iter()
            .map(|r| r["name"].clone())
            .collect()
    };
    let mut a = open("s-a");
    event(&mut a, "first");
    let mut b = open("s-b");
    event(&mut b, "second");
    event(&mut a, "third");
    assert_eq!(stored(), ["first", "second", "third"]);

    let threads = [a, b].map(|mut s| std::thread::spawn(move || s.dispatch()));
    for t in threads {
        t.join().unwrap();
    }
    assert_eq!(posted_names(&seen), ["first", "second", "third"]);
    assert!(stored().is_empty());
}
