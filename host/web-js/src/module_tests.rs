//! Declared substitutes on the runner and JS target, from the same compiled
//! third-party interface (LLP 1133.030 D12).
use exact_runner::{agent, Dispatch, Outcome, Reply, Request, Runner, Value, Work};
use serde_json::{json, Value as Json};
use std::sync::{Arc, Mutex};

const EPOCH: u64 = 1_800_000_000_000;
const EXTRA: &str = r#"  state heard = 0
  action heardChime(tag: string)
    heard = heard + 1
  state timerOn = false
  action arm
    timerOn = true
  task pauseOnSeek when timerOn
    after(500, hold)
  view
"#;

struct Granted;
impl exact_runner::DataSource for Granted {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "device.alarms timer"
    }
}

fn plan() -> exact_plan::Plan {
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!(
        "target/module-substitute-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let package = dir.join("node_modules/@fixture/chime");
    std::fs::create_dir_all(&package).unwrap();
    let fixture = root.join("contract/cli/tests/chime");
    let mut interface: Json =
        serde_json::from_slice(&std::fs::read(fixture.join("exact.json")).unwrap()).unwrap();
    interface["permission"][0]["requires"] = json!({"web": ["ping"]});
    for name in interface["names"].as_array_mut().unwrap() {
        name["substitute"]
            .as_object_mut()
            .unwrap()
            .remove("unsupported");
        if name["source"] == "chimeStatus" {
            name["substitute"]["targets"]["web"] =
                json!({"android": {"exact": false, "ping": "prompt"}});
        }
        if name["send"] == "setChime" {
            name["substitute"]["list"]["tap"]["choices"]["ack"] =
                json!({"event": "chimeclick", "args": ["tag"]});
        }
    }
    interface["names"].as_array_mut().unwrap().push(
        json!({"event": "chimeclick", "on": "root", "raised": "activation", "args": ["string"], "hold": 4}),
    );
    std::fs::write(package.join("exact.json"), interface.to_string()).unwrap();
    std::fs::copy(fixture.join("package.json"), package.join("package.json")).unwrap();
    let source = std::fs::read_to_string(fixture.join("app/app.contract")).unwrap();
    let source = source
        .replace("use chimeStatus,", "use chimeclick, chimeStatus,")
        .replace("  view\n", EXTRA)
        .replace("    column\n", "    column chimeclick=heardChime\n");
    std::fs::write(dir.join("app.contract"), source).unwrap();
    let plan = contract::compile_path_all(&dir.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"))
        .0;
    std::fs::remove_dir_all(dir).unwrap();
    plan
}

fn call(send: &str, args: Json) -> Json {
    json!({"call": {"extension": "chime", "send": send, "args": args}})
}

fn schedule(args: Json) -> Json {
    let mut fields =
        json!({"tag": "t", "title": "Tea", "at": null, "duration": null, "snooze": 30000});
    fields
        .as_object_mut()
        .unwrap()
        .extend(args.as_object().unwrap().clone());
    call("setChime", fields)
}

fn prefer(name: &str, state: &str) -> Json {
    json!({"prefer": {"permission": {name: state}}})
}

type Replies = Arc<Mutex<Vec<Json>>>;

fn data_call(r: &mut Runner<Granted>, body: &Json, replies: &Replies) {
    let Dispatch::Run(work) = r.native_work(&Request::native(body.to_string().into_bytes())) else {
        panic!("a module call is work");
    };
    let replies = replies.clone();
    let done = move |outcome| {
        let Outcome::Response(response) = outcome else {
            panic!("an answer")
        };
        let mut answer: Json = serde_json::from_slice(&response.body).unwrap();
        if answer == json!({}) {
            answer = Json::Null;
        }
        replies.lock().unwrap().push(answer);
    };
    match work {
        Work::Now(work) => done(work()),
        Work::Later(work) => work(Reply::new(done)),
    }
}

fn snapshot(r: &mut Runner<Granted>, replies: &Replies) -> Json {
    let mut entries = r
        .declared_state()
        .into_iter()
        .find(|(s, _)| s == "chimes")
        .unwrap()
        .1;
    for entry in entries.as_array_mut().unwrap() {
        entry.as_object_mut().unwrap().remove("ref");
    }
    let ext: Json = serde_json::from_str(&r.extensions_json().unwrap()).unwrap();
    let held: Vec<Json> = r
        .device_holds()
        .iter()
        .map(|h| serde_json::from_str::<Json>(&h.args).unwrap()["capability"].clone())
        .collect();
    let status = Arc::new(Mutex::new(Vec::new()));
    data_call(
        r,
        &json!({"extension": "chime", "source": "chimeStatus"}),
        &status,
    );
    let listed = Arc::new(Mutex::new(Vec::new()));
    data_call(
        r,
        &json!({"extension": "chime", "source": "chimes"}),
        &listed,
    );
    json!({"entries": entries, "permissions": ext["chime"]["permissions"], "held": held,
        "listed": listed.lock().unwrap().last(),
        "status": status.lock().unwrap().last(), "replies": replies.lock().unwrap().clone(),
        "heard": r.slot("heard").and_then(Value::as_number).map(|n| n as u64)})
}

fn runner(plan: &exact_plan::Plan, steps: &[Json]) -> Vec<Json> {
    let mut r = exact_runner::time::launching(
        Some(exact_runner::time::WallTime {
            epoch_at_zero: EPOCH as f64,
            utc_offset: 0.0,
        }),
        || {
            Runner::boot(
                plan.clone(),
                Granted,
                exact_kernel::Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap()
        },
    );
    r.set_module_target("web");
    let replies = Replies::default();
    let mut out = Vec::new();
    for step in steps {
        if step["arm"] == true {
            r.act("arm", Vec::new()).unwrap();
        }
        if let Some(body) = step.get("call") {
            data_call(&mut r, body, &replies);
        }
        if let Some(body) = step.get("prefer") {
            let mut body = body.clone();
            body["op"] = json!("prefer");
            r.extension_agent(&body.to_string()).unwrap().1.unwrap();
        }
        if let Some(ms) = step["clock"].as_f64() {
            r.advance(ms).unwrap();
        }
        if let Some(choice) = step["answer"].as_str() {
            let ticket = r.device_holds()[0].ticket;
            agent::answer(
                &mut r,
                &json!({"op": "tap", "ticket": ticket, "choice": choice}).to_string(),
            )
            .unwrap();
        }
        if let Some(choice) = step["choice"].as_str() {
            let entries = r
                .declared_state()
                .into_iter()
                .find(|(s, _)| s == "chimes")
                .unwrap()
                .1;
            let ticket = entries[0]["ref"].clone();
            let answer = agent::answer(
                &mut r,
                &json!({"op": "tap", "ticket": ticket, "choice": choice}).to_string(),
            )
            .unwrap()
            .0;
            let answer: Json = serde_json::from_str(&answer).unwrap();
            assert!(answer.get("error").is_none(), "{answer}");
        }
        assert!(r.apply_announced().1.is_none());
        out.push(snapshot(&mut r, &replies));
    }
    out
}

fn js(plan: &exact_plan::Plan, steps: &[Json]) -> Json {
    let mut rows = serde_json::Map::new();
    for m in &plan.modules {
        let source = &plan.sources[m.source.0 as usize];
        let options: Json = if source.params.len == 0 {
            Json::Null
        } else {
            serde_json::from_str(&crate::emit::surface_type_json(
                plan,
                plan.source_params[source.params.start as usize].ty,
            ))
            .unwrap()
        };
        rows.insert(
            plan.str(source.name).into(),
            json!([
                format!("{}.{}", plan.str(m.extension), plan.str(m.declared)),
                m.send,
                serde_json::from_str::<Json>(plan.str(m.substitute)).unwrap(),
                serde_json::from_str::<Json>(&crate::emit::surface_type_json(plan, source.ty))
                    .unwrap(),
                plan.str(m.request),
                options,
                m.live,
            ]),
        );
    }
    let script = format!(
        "const rows={}; const steps={}; const epoch={EPOCH};\n{DRIVE}",
        Json::Object(rows),
        json!(steps)
    );
    let out = std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
        .args(["-e", &script])
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

const DRIVE: &str = r#"
import {mock} from 'bun:test';
mock.module('./host/web-js/navigation.js', () => import('./host/web/navigation.js'));
for (const n of ['prompt-glue', 'alert-message', 'menu-rows']) mock.module(`./host/web-js/${n}.js`, () => ({}));
mock.module('./host/web-js/wakelock-glue.js', () => ({displayed: () => true}));
mock.module('./host/web-js/names.js', () => ({sourceTypes: {}}));
globalThis.location = new URL(`http://fixture.invalid/?agent&epoch=${epoch}`);
const {modules} = await import('./host/web-js/modules.js');
const rt = await import('./host/web-js/rt.js');
rt.clock.agent = true;
rt.data.grants = 'device.alarms timer';
let heard = 0;
modules(rows, {}, {}, {'chime.chimeclick': [() => () => { heard++; }, 'activation', 4, ['s'], 'heardChime']});
const ext = exact.extensions, replies = [], out = [];
for (const step of steps) {
  if (step.arm) rt.every(500, () => { rt.Hosts.holdChime('app', ['t']); return true; }, true, 'pauseOnSeek');
  if (step.call) ext.call(step.call).then(v => replies.push(v));
  if (step.prefer) { const v = ext.prefer(step.prefer); if (v.error) throw Error(v.error); }
  if (step.clock != null) rt.advance(step.clock);
  if (step.answer) { const v = ext.answer({op:'tap', ticket:ext.holds()[0].ticket, choice:step.answer}); if (v.error) throw Error(v.error); }
  if (step.choice) { const v = ext.answer({op:'tap', ticket:ext.declared().chimes[0].ref, choice:step.choice}); if (v.error) throw Error(v.error); }
  await Promise.resolve();
  const entries = ext.declared().chimes.map(({ref, ...e}) => e);
  out.push({entries, permissions:ext.state().chime.permissions, held:ext.holds().map(h=>h.device.args.capability),
    status:await ext.call({extension:'chime',source:'chimeStatus'}),
    listed:await ext.call({extension:'chime',source:'chimes'}), replies:[...replies], heard});
}
console.log(JSON.stringify(out));
"#;

fn compare(plan: &exact_plan::Plan, steps: &[Json]) {
    let js = js(plan, steps);
    let runner = runner(plan, steps);
    for (i, (got, want)) in js.as_array().unwrap().iter().zip(&runner).enumerate() {
        assert_eq!(got, want, "step {i}: {}", steps[i]);
    }
    assert_eq!(js.as_array().unwrap().len(), runner.len());
}

#[test]
fn declared_substitute_permissions_match_the_runner() {
    let plan = plan();
    let steps = vec![
        schedule(json!({"duration": 60000})),
        json!({"answer": "allow"}),
        json!({"answer": "deny"}),
        prefer("chime", "prompt"),
        call("requestChime", json!({})),
        json!({"answer": "deny"}),
        prefer("chime", "granted"),
        prefer("ping", "denied"),
        schedule(json!({"duration": 60000})),
        prefer("ping", "prompt"),
        schedule(json!({"duration": 60000})),
        json!({"answer": "cancel"}),
        call("requestChime", json!({})),
        json!({"answer": "allow"}),
    ];
    compare(&plan, &steps);
}

#[test]
fn declared_substitute_due_times_and_updates_match_the_runner() {
    let plan = plan();
    let steps = vec![
        prefer("chime", "granted"),
        prefer("ping", "granted"),
        schedule(json!({"at": EPOCH - 1})),
        schedule(json!({"at": EPOCH})),
        schedule(json!({"at": EPOCH + 1})),
        json!({"clock": 1}),
        json!({"clock": 2}),
        json!({"choice": "ack"}),
        json!({"choice": "snooze"}),
        json!({"clock": 10002}),
        call("holdChime", json!({"tag":"t"})),
        json!({"clock": 100000}),
        call("holdChime", json!({"tag":"t"})),
        call("goChime", json!({"tag":"t"})),
        json!({"clock": 120000}),
        json!({"choice": "stop"}),
        call("goChime", json!({"tag":"t"})),
        schedule(json!({"at": EPOCH + 120000})),
        schedule(json!({"at": EPOCH + 119999})),
        schedule(json!({"duration": 60000})),
        schedule(json!({"duration": 1000})),
        json!({"clock": 121000}),
        call("stopChime", json!({"tag":"t"})),
    ];
    compare(&plan, &steps);
    compare(
        &plan,
        &[
            prefer("chime", "granted"),
            prefer("ping", "granted"),
            schedule(json!({"duration": 1000})),
            json!({"arm": true}),
            json!({"clock": 1000}),
        ],
    );
}

#[test]
fn declared_list_identity_matches_the_runner() {
    for scope in ["session", "app"] {
        let mut plan = plan();
        let second = exact_plan::StrId(plan.strings.len() as u32);
        plan.strings.push("second");
        let i = plan
            .modules
            .iter()
            .position(|m| plan.str(m.declared) == "stopChime")
            .unwrap();
        plan.modules[i].extension = second;
        plan.set_module_substitute(
            i,
            &json!({"effect": true, "list": {"state": "chimes", "key": "tag", "scope": scope}})
                .to_string(),
        );
        let row = plan.modules.remove(i);
        plan.modules.push(row);
        let i = plan
            .modules
            .iter()
            .position(|m| plan.str(m.declared) == "setChime")
            .unwrap();
        let mut sub: Json = serde_json::from_str(plan.str(plan.modules[i].substitute)).unwrap();
        sub["list"]["scope"] = json!(scope);
        plan.set_module_substitute(i, &sub.to_string());
        compare(
            &plan,
            &[
                prefer("chime", "granted"),
                prefer("ping", "granted"),
                schedule(json!({"tag": "a", "duration": 1000})),
                json!({"call": {"extension": "second", "send": "stopChime", "args": {"tag": "b"}}}),
                json!({"call": {"extension": "second", "send": "stopChime", "args": {"tag": "a"}}}),
            ],
        );
    }
}
