use super::*;
use exact_runner::{Dispatch, Outcome, Reply, Response, Work};
use std::sync::{atomic::AtomicUsize, Arc, Mutex};

#[derive(Default)]
struct Package {
    open: Arc<Mutex<std::collections::BTreeSet<u64>>>,
    calls: Arc<AtomicUsize>,
    registration: Arc<Mutex<Option<u64>>>,
}

fn activation(r: &mut Runner<Source>, registration: Option<u64>) {
    let mut args = vec![
        json!("record"),
        json!({"id":"record","keys":"Control+Alt+Meta+F"}),
    ];
    if let Some(id) = registration {
        args.push(json!(id));
    }
    r.raise_module_event(
        "global-shortcuts",
        "globalshortcut",
        &json!(args).to_string(),
        CallOrigin::Device,
    )
    .unwrap();
    settle(r);
}

impl Package {
    fn activation(&self, r: &mut Runner<Source>) {
        activation(r, *self.registration.lock().unwrap());
    }

    fn handler(&self) -> exact_runner::NativeHandler {
        let open = self.open.clone();
        let generations = open.clone();
        let calls = self.calls.clone();
        let registration = self.registration.clone();
        let native_calls = calls.clone();
        let package: exact_runner::NativeHandler = Arc::new(move |body, reply| {
            let request: Json = serde_json::from_slice(&body).unwrap();
            if request["source"] == "globalPointer" || request["release"] == "globalPointer" {
                assert!(reply.owns_pointer_observation());
                native_calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let answer = if request["source"] == "globalPointer" {
                generations
                    .lock()
                    .unwrap()
                    .insert(request["args"]["observation"].as_u64().unwrap());
                json!({"listening":true})
            } else if request["source"] == "globalShortcut" {
                if request["args"]["id"] == "record" {
                    *registration.lock().unwrap() = request["args"]["registration"].as_u64();
                }
                json!({"id":request["args"]["id"],"keys":request["args"]["keys"],"registered":true,"reason":null})
            } else {
                if request["release"] == "globalPointer" {
                    generations
                        .lock()
                        .unwrap()
                        .remove(&request["args"]["observation"].as_u64().unwrap());
                }
                json!({})
            };
            reply.send(Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: answer.to_string().into_bytes(),
            }));
        });
        package
    }
    fn boot(&self, app: &App, live: bool) -> Runner<Source> {
        let mut r = boot_with_grants(app, "macos", BOTH);
        r.native_slot().host_packages(Some(self.handler()));
        if live {
            r.set_live_clock(Some(Box::new(|| 0.)));
        }
        r.bind_extensions(&["global-shortcuts"], &[]);
        settle(&mut r);
        drain(&mut r);
        r
    }
}

fn drain(r: &mut Runner<Source>) {
    for ask in r.take_requests() {
        let outcome = match r.native_work(&ask.request) {
            Dispatch::Run(Work::Later(work)) => {
                let (send, receive) = std::sync::mpsc::channel();
                work(Reply::new(move |answer| send.send(answer).unwrap()));
                receive.try_recv().unwrap()
            }
            Dispatch::Run(Work::Now(work)) => work(),
            _ => panic!("recorder request was not dispatched"),
        };
        r.fulfill(ask.ticket, outcome).unwrap();
    }
    settle(r);
}

#[test]
fn a_fresh_recorder_releases_its_bound_reader_and_discards_old_callbacks() {
    let app = recording_app();
    let fixture = Package::default();
    let boot = |live| fixture.boot(&app, live);
    let open = &fixture.open;
    let calls = &fixture.calls;
    let app_chord = r#"globalShortcut(id="record", keys="Control+Alt+Meta+F")"#;
    let wrapped = RECORDER.replace(
        &format!("resource chord = {app_chord} as shape GlobalShortcut"),
        &format!("resource declared = {app_chord} when false as shape GlobalShortcut\n  resource chord = chordFromData() as shape GlobalPointer"),
    ).replace("chord.registered", "chord.listening");
    std::fs::write(app.0.join("app.contract"), wrapped).unwrap();
    let mut from_data = boot(true);
    drain(&mut from_data);
    drain(&mut from_data);
    assert_eq!(text(&from_data, "chord"), "true");
    from_data
        .set_page(exact_runner::Page {
            hidden: true,
            ..from_data.page()
        })
        .unwrap();
    fixture.activation(&mut from_data);
    drain(&mut from_data);
    assert_eq!(
        text(&from_data, "listening"),
        "true",
        "{}",
        logs(&from_data)
    );
    drop(from_data);
    assert!(open.lock().unwrap().is_empty());
    std::fs::write(app.0.join("app.contract"), RECORDER).unwrap();
    let mut first = boot(true);
    fixture.activation(&mut first);
    drain(&mut first);
    assert_eq!(text(&first, "listening"), "true", "{}", logs(&first));
    assert_eq!(open.lock().unwrap().len(), 1);
    let stale = *open.lock().unwrap().first().unwrap();
    let edge = json!([{"screenX":1,"screenY":2,"button":0,"timeStamp":3,
        "altKey":false,"ctrlKey":false,"metaKey":false,"shiftKey":false}, stale])
    .to_string();
    first
        .raise_module_event(
            "global-shortcuts",
            "globalpointerdown",
            &edge,
            CallOrigin::Device,
        )
        .unwrap();
    let mut second = boot(true);
    press(&mut second, "mount");
    drain(&mut second);
    assert_eq!(open.lock().unwrap().len(), 2);
    drop(first);
    assert_eq!(open.lock().unwrap().len(), 1);
    let mut fresh = boot(true);
    press(&mut fresh, "start");
    drain(&mut fresh);
    fresh
        .raise_module_event(
            "global-shortcuts",
            "globalpointerdown",
            &edge,
            CallOrigin::Device,
        )
        .unwrap();
    settle(&mut fresh);
    assert_eq!(text(&fresh, "count"), "0");
    assert_eq!(text(&fresh, "listening"), "true");
    let current = *open.lock().unwrap().last().unwrap();
    let mut valid: Json = serde_json::from_str(&edge).unwrap();
    valid[1] = json!(current);
    fresh
        .raise_module_event(
            "global-shortcuts",
            "globalpointerup",
            &valid.to_string(),
            CallOrigin::Device,
        )
        .unwrap();
    settle(&mut fresh);
    assert_eq!(text(&fresh, "count"), "1");

    // The agent can take control after acquisition, with a real callback still queued.
    fresh.set_live_clock(None);
    assert!(!open.lock().unwrap().contains(&current));
    fresh
        .raise_module_event(
            "global-shortcuts",
            "globalpointerup",
            &valid.to_string(),
            CallOrigin::Device,
        )
        .unwrap();
    settle(&mut fresh);
    assert_eq!(text(&fresh, "count"), "1");
    pointer_edge(&mut fresh);
    assert_eq!(text(&fresh, "count"), "2");
    act(&mut second, "unmount");
    act(&mut fresh, "stop");
    assert!(open.lock().unwrap().is_empty());
    let mut cancelled = boot(true);
    press(&mut cancelled, "start");
    let ask = cancelled.take_requests().pop().unwrap();
    let Dispatch::Run(Work::Later(delayed)) = cancelled.native_work(&ask.request) else {
        panic!("expected bound read")
    };
    drop(cancelled);
    delayed(Reply::new(|outcome| {
        let Outcome::Response(answer) = outcome else {
            panic!("missing cancellation")
        };
        assert_eq!(
            serde_json::from_slice::<Json>(&answer.body).unwrap()["error"],
            "AbortError"
        );
    }));
    assert!(open.lock().unwrap().is_empty());
    let mut delayed = boot(true);
    press(&mut delayed, "start");
    drain(&mut delayed);
    press(&mut delayed, "mount");
    let held = delayed.take_requests();
    assert!(!held.is_empty());
    act(&mut delayed, "stop");
    delayed
        .act_from(CallOrigin::Device, "start", vec![])
        .unwrap();
    settle(&mut delayed);
    assert!(
        logs(&delayed).contains("listener failed: global-shortcuts.globalPointer: NotAllowedError")
    );
    act(&mut delayed, "unmount");
    assert!(open.lock().unwrap().is_empty());
    let mut denied = boot_with_grants(&app, "macos", "device.global-shortcuts");
    denied.native_slot().host_packages(Some(fixture.handler()));
    denied.set_live_clock(Some(Box::new(|| 0.)));
    denied.bind_extensions(&["global-shortcuts"], &[]);
    settle(&mut denied);
    drain(&mut denied);
    press(&mut denied, "start");
    assert!(denied.take_requests().is_empty());
    assert!(logs(&denied).contains("grant-missing (device.global-pointer)"));
    assert!(open.lock().unwrap().is_empty());
    let before = calls.load(std::sync::atomic::Ordering::Relaxed);
    let mut driven = boot(false);
    press(&mut driven, "start");
    assert!(driven.take_requests().is_empty());
    pointer_edge(&mut driven);
    act(&mut driven, "stop");
    pointer_edge(&mut driven);
    assert_eq!(text(&driven, "count"), "1");
    assert_eq!(before, calls.load(std::sync::atomic::Ordering::Relaxed));
    assert!(open.lock().unwrap().is_empty());
}
#[test]
fn an_activation_belongs_to_the_confirmed_registration() {
    let app = recording_app();
    let source = RECORDER.replace("  state recording", "  state installed = true\n  state recording")
        .replace(r#"keys="Control+Alt+Meta+F") as shape GlobalShortcut"#, r#"keys="Control+Alt+Meta+F") when installed as shape GlobalShortcut"#)
        .replace("  state mounted", "  state presses = 0\n  state mounted")
        .replace("  action toggle", "  action toggle(id: string)\n    presses = presses + 1")
        .replace(r#"      text testId="chord""#, "      text testId=\"presses\" `${presses}`\n      text testId=\"chord\"")
        .replace("  action start", "  action install\n    installed = true\n  action retire\n    installed = false\n  action start");
    std::fs::write(app.0.join("app.contract"), source).unwrap();
    let package = Package::default();
    let mut r = package.boot(&app, true);
    let old = *package.registration.lock().unwrap();
    assert!(old.is_some());
    assert_eq!(text(&r, "chord"), "true");
    act(&mut r, "retire");
    act(&mut r, "install");
    let pending = r.take_requests();
    assert!(!pending.is_empty());
    r.set_page(exact_runner::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();
    activation(&mut r, old);
    drain(&mut r);
    assert_eq!(text(&r, "presses"), "1");
    assert!(
        package.open.lock().unwrap().is_empty(),
        "a pending registration admitted an old activation"
    );
    drop(r);
    let mut fresh = package.boot(&app, true);
    activation(&mut fresh, old);
    drain(&mut fresh);
    assert_eq!(text(&fresh, "presses"), "1");
    assert!(
        package.open.lock().unwrap().is_empty(),
        "a replacement admitted an old activation"
    );
    act(&mut fresh, "stop");
    package.activation(&mut fresh);
    drain(&mut fresh);
    assert_eq!(package.open.lock().unwrap().len(), 1);
    assert_eq!(text(&fresh, "presses"), "2");
}

#[test]
fn the_last_restricted_registrar_leaves_the_remaining_reader_a_new_stamp() {
    let app = recording_app();
    shared_chord_app(&app);
    let contract = app.0.join("app.contract");
    let fixture = std::fs::read_to_string(&contract).unwrap().replace(
        r#"sharedChord = globalShortcut(id="record""#,
        r#"sharedChord = globalShortcut(id="other""#,
    );
    std::fs::write(contract, fixture).unwrap();
    let package = Package::default();
    let mut r = package.boot(&app, true);
    let old = *package.registration.lock().unwrap();
    r.set_page(exact_runner::Page {
        hidden: true,
        ..r.page()
    })
    .unwrap();
    let digest = state(&r)["controls"][0]["digest"]
        .as_str()
        .unwrap()
        .to_string();
    r.press_control("share", &digest, Default::default())
        .unwrap();
    settle(&mut r);
    drain(&mut r);
    package.activation(&mut r);
    drain(&mut r);
    assert!(package.open.lock().unwrap().is_empty());
    act(&mut r, "unshare");
    drain(&mut r);
    assert_ne!(old, *package.registration.lock().unwrap());
    activation(&mut r, old);
    drain(&mut r);
    assert!(package.open.lock().unwrap().is_empty());
    act(&mut r, "stop");
    package.activation(&mut r);
    drain(&mut r);
    assert_eq!(package.open.lock().unwrap().len(), 1);
}

#[test]
fn a_logical_pointer_read_keeps_its_own_opener() {
    let app = recording_app();
    std::fs::write(
        app.0.join("app.contract"),
        RECORDER.replace("listener = globalPointer()", "listener = yieldPointer()"),
    )
    .unwrap();
    use exact_runner::{DataSource, Placement};
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false).unwrap();
    for native in [false, true] {
        for placement in [Placement::Main, Placement::Worker] {
            for person in [true, false] {
                let mut data = exact_data::Placed::new(Grants(BOTH), placement);
                data.bind(&plan);
                data.activate().unwrap();
                let mut r = Runner::boot(
                    plan.clone(),
                    boxed(data),
                    exact_kernel::Kernel::with_monospace(),
                    Default::default(),
                    "/",
                )
                .unwrap();
                r.set_module_target("macos");
                let package = Package::default();
                if native {
                    r.native_slot().host_packages(Some(package.handler()));
                    r.set_live_clock(Some(Box::new(|| 0.)));
                    r.bind_extensions(&["global-shortcuts"], &[]);
                }
                settle(&mut r);
                if person {
                    press(&mut r, "start");
                } else {
                    r.act_from(CallOrigin::Device, "start", vec![]).unwrap();
                    settle(&mut r);
                }
                // Hiding the window before a worker runs cannot change who opened this read.
                r.set_page(exact_runner::Page {
                    hidden: true,
                    ..r.page()
                })
                .unwrap();
                drive_yielded_read(&mut r);
                if person {
                    r.act_from(CallOrigin::Device, "refreshListener", vec![])
                        .unwrap();
                    settle(&mut r);
                    drive_yielded_read(&mut r);
                }
                assert_eq!(
                    package.open.lock().unwrap().len(),
                    usize::from(native && person),
                    "{placement:?}: {}",
                    logs(&r)
                );
                assert_eq!(
                    text(&r, "listening"),
                    person.to_string(),
                    "{placement:?}: {}",
                    logs(&r)
                );
            }
        }
    }
}

fn drive_yielded_read(r: &mut Runner<Source>) {
    for _ in 0..12 {
        let requests = r.take_requests();
        if requests.is_empty() {
            break;
        }
        for ask in requests {
            let work = if let Some(turn) = ask.request.continuation {
                r.dispatch_work(turn)
            } else if ask.request.is_native() {
                r.native_work(&ask.request)
            } else {
                Dispatch::Run(Work::Now(Box::new(|| {
                    Outcome::Response(Response {
                        status: 200,
                        headers: vec![],
                        body: br#""continue""#.to_vec(),
                    })
                })))
            };
            let reply = match work {
                Dispatch::Run(Work::Now(run)) => run(),
                Dispatch::Run(Work::Later(run)) => {
                    let (send, receive) = std::sync::mpsc::channel();
                    run(Reply::new(move |v| send.send(v).unwrap()));
                    receive
                        .recv_timeout(std::time::Duration::from_secs(10))
                        .unwrap()
                }
                _ => panic!("the recorder has one read in flight"),
            };
            r.fulfill(ask.ticket, reply).unwrap();
            settle(r);
        }
    }
}

#[test]
fn a_carried_recorder_needs_a_new_start() {
    let app = recording_app();
    let package = Package::default();
    let mut r = package.boot(&app, true);
    press(&mut r, "start");
    drain(&mut r);
    assert_eq!(text(&r, "listening"), "true");
    let old = *package.registration.lock().unwrap();
    let carried = r.carry();
    drop(r);
    assert!(package.open.lock().unwrap().is_empty());
    let (plan, _) = contract::compile_path_all(&app.0.join("app.contract"), false).unwrap();
    let mut fresh = Runner::boot_carrying(
        plan,
        boxed(Grants(BOTH)),
        exact_kernel::Kernel::with_monospace(),
        &carried,
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(text(&fresh, "listening"), "false");
    fresh.set_module_target("macos");
    fresh.native_slot().host_packages(Some(package.handler()));
    fresh.set_live_clock(Some(Box::new(|| 0.)));
    fresh.bind_extensions(&["global-shortcuts"], &[]);
    settle(&mut fresh);
    drain(&mut fresh);
    assert_eq!(text(&fresh, "listening"), "false");
    assert!(package.open.lock().unwrap().is_empty());
    act(&mut fresh, "stop");
    assert_ne!(old, *package.registration.lock().unwrap());
    fresh
        .set_page(exact_runner::Page {
            hidden: true,
            ..fresh.page()
        })
        .unwrap();
    package.activation(&mut fresh);
    drain(&mut fresh);
    assert_eq!(package.open.lock().unwrap().len(), 1);
}
