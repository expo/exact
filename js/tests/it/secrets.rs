//! Real OAuth package refreshes through Hermes and the worker envelope.
#![cfg(exact_js_engine)]
use exact_data::placed::Placed;
use exact_js::Module;
use exact_plan::Value;
use exact_runner::{
    Answer, DataSource, Dispatch, Outcome, Placement, Reply, Request, Response, SecretLease,
    SecretWait, SharedSecrets, Store, Target, Work,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/context.hbc"));
const GRANTS: &str = "net.fetch https://context.test\nsecret.keep token\n";
const OLD: &str = r#"{"accessToken":"old","refreshToken":"rotate-0","tokenEndpoint":"https://context.test/token","scopes":[]}"#;

#[derive(Default)]
struct Disk {
    value: Option<String>,
    held: bool,
    read_error: bool,
    write_error: bool,
    writes: usize,
    acquires: usize,
}
#[derive(Clone)]
struct Backend(Arc<Mutex<Disk>>);
struct Wait(Backend);
struct Lease(Backend);
impl SharedSecrets for Backend {
    fn acquire(&self, name: &str, _: Arc<dyn Fn() + Send + Sync>) -> Box<dyn SecretWait> {
        assert_eq!(name, "token");
        self.0.lock().unwrap().acquires += 1;
        Box::new(Wait(self.clone()))
    }
}
impl SecretWait for Wait {
    fn take(&mut self) -> Option<Result<Arc<dyn SecretLease>, String>> {
        let mut disk = self.0 .0.lock().unwrap();
        if disk.held {
            return None;
        }
        disk.held = true;
        Some(Ok(Arc::new(Lease(self.0.clone()))))
    }
}
impl SecretLease for Lease {
    fn read(&self) -> Result<Option<String>, String> {
        let disk = self.0 .0.lock().unwrap();
        if disk.read_error {
            Err("read failed".into())
        } else {
            Ok(disk.value.clone())
        }
    }
    fn write(&self, value: Option<&str>) -> Result<(), String> {
        let mut disk = self.0 .0.lock().unwrap();
        if disk.write_error {
            disk.value = None;
            return Err("write failed".into());
        }
        disk.value = value.map(str::to_owned);
        disk.writes += 1;
        Ok(())
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.0 .0.lock().unwrap().held = false;
    }
}

fn module(placement: Placement, disk: &Backend) -> Placed<Module> {
    module_over(placement, Arc::new(disk.clone()))
}
fn module_over(placement: Placement, secrets: Arc<dyn SharedSecrets>) -> Placed<Module> {
    let plan = contract::compile("shape Result\n  text: string\ncomponent App\n  mutation output as shape Result\n  action go(name: string)\n    send output = secret(name)\n  view\n    text \"secret\"\n").unwrap();
    let mut module = Module::new(HBC.to_vec(), "test.context", GRANTS);
    module.set_budget_ms(f64::INFINITY);
    let mut module = module.placed(placement);
    module.configure_secrets(secrets).unwrap();
    module.bind(&plan);
    module.activate().unwrap();
    module
}
fn store() -> Store {
    Store::new(GRANTS, [("token".into(), OLD.as_bytes().to_vec())])
}
fn args(id: usize) -> [Value; 1] {
    [Value::str(&id.to_string())]
}
fn response(status: u16, body: &str) -> Outcome {
    Outcome::Response(Response {
        status,
        headers: vec![],
        body: body.as_bytes().to_vec(),
    })
}
fn run(dispatch: Dispatch) -> Outcome {
    match dispatch {
        Dispatch::Again => Dispatch::again_outcome(),
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let (tx, rx) = std::sync::mpsc::channel();
            work(Reply::new(move |reply| {
                tx.send(reply).unwrap();
            }));
            rx.recv_timeout(Duration::from_secs(20))
                .expect("worker turn completed")
        }
        _ => panic!("a runnable continuation"),
    }
}
fn drain(m: &mut impl DataSource, s: &mut Store, id: usize, mut answer: Answer) -> Answer {
    for _ in 0..20 {
        let Answer::Later(request) = &answer else {
            return answer;
        };
        let Some(token) = request.continuation else {
            return answer;
        };
        if m.host_continuation(token) {
            return answer;
        }
        let outcome = run(m.dispatch(token, s));
        answer = m
            .parse_for(Target::Mutation(id), s, "secret", &args(id), outcome)
            .unwrap();
    }
    panic!("a bounded owner turn")
}
fn begin(m: &mut impl DataSource, s: &mut Store, id: usize) -> Answer {
    let answer = m
        .answer_for(Target::Mutation(id), s, "secret", &args(id))
        .unwrap();
    drain(m, s, id, answer)
}
fn land(m: &mut impl DataSource, s: &mut Store, id: usize, outcome: Outcome) -> Answer {
    let answer = m
        .parse_for(Target::Mutation(id), s, "secret", &args(id), outcome)
        .unwrap();
    drain(m, s, id, answer)
}
fn request(answer: &Answer) -> &Request {
    let Answer::Later(r) = answer else {
        panic!("{answer:?}")
    };
    r
}
/// `answer`'s lease acquisition, landed once the lease is granted. A backend that grants on another thread, as the data
/// crate's broker does, wakes `woke` through the module's `abort_wake`.
fn acquire(
    m: &mut impl DataSource,
    s: &mut Store,
    id: usize,
    answer: &Answer,
    woke: Option<&std::sync::mpsc::Receiver<()>>,
) -> Answer {
    let token = request(answer).continuation.expect("secret acquisition");
    assert!(m.host_continuation(token));
    let mut dispatch = m.dispatch(token, s);
    while matches!(dispatch, Dispatch::Held) {
        woke.expect("a held lease is waited for on `woke`")
            .recv_timeout(Duration::from_secs(20))
            .expect("the backend granted the lease");
        if let Some((_, again)) = m.release(s).into_iter().find(|(t, _)| *t == token) {
            dispatch = again;
        }
    }
    land(m, s, id, run(dispatch))
}
fn text(answer: Answer) -> String {
    let Answer::Now(Value::Record(fields)) = answer else {
        panic!("{answer:?}")
    };
    fields[0].as_str().unwrap().into()
}
fn backend() -> Backend {
    Backend(Arc::new(Mutex::new(Disk {
        value: Some(OLD.into()),
        ..Default::default()
    })))
}
fn rotated() -> Outcome {
    response(
        200,
        r#"{"access_token":"fresh","refresh_token":"rotate-1","token_type":"Bearer"}"#,
    )
}

#[test]
fn independent_native_realms_reload_under_the_lease_before_refreshing() {
    for placement in [Placement::Main, Placement::Worker] {
        let disk = backend();
        let mut a = module(placement, &disk);
        let mut b = module(placement, &disk);
        let (mut sa, mut sb) = (store(), store());
        assert_eq!(request(&begin(&mut a, &mut sa, 0)).method, "GET");
        assert_eq!(request(&begin(&mut b, &mut sb, 0)).method, "GET");
        let aw = land(&mut a, &mut sa, 0, response(401, "expired"));
        let post = acquire(&mut a, &mut sa, 0, &aw, None);
        assert_eq!(request(&post).method, "POST");
        assert!(String::from_utf8_lossy(&request(&post).body).contains("rotate-0"));
        let bw = land(&mut b, &mut sb, 0, response(401, "expired"));
        let bt = request(&bw).continuation.unwrap();
        assert!(matches!(b.dispatch(bt, &sb), Dispatch::Held));
        assert!(b.release(&sb).is_empty());
        let fresh = land(&mut a, &mut sa, 0, rotated());
        assert_eq!(request(&fresh).method, "GET");
        let released = b.release(&sb);
        assert_eq!(released.len(), 1);
        assert_eq!(released[0].0, bt);
        let fresh_b = land(
            &mut b,
            &mut sb,
            0,
            run(released.into_iter().next().unwrap().1),
        );
        assert_eq!(request(&fresh_b).method, "GET");
        assert!(request(&fresh_b)
            .headers
            .iter()
            .any(|(_, v)| v == "Bearer fresh"));
        assert_eq!(text(land(&mut a, &mut sa, 0, response(200, "A"))), "A");
        assert_eq!(text(land(&mut b, &mut sb, 0, response(200, "B"))), "B");
        assert_eq!(disk.0.lock().unwrap().writes, 1);
        assert!(sa.take_writes().iter().all(|w| w.durable));
        assert!(sb.take_writes().iter().all(|w| w.durable));
    }
}

#[test]
fn callers_in_one_native_owner_do_not_block_the_lease_holders_reply() {
    for placement in [Placement::Main, Placement::Worker] {
        let disk = backend();
        let mut m = module(placement, &disk);
        let mut s = store();
        begin(&mut m, &mut s, 0);
        begin(&mut m, &mut s, 1);
        let a = land(&mut m, &mut s, 0, response(401, "expired"));
        acquire(&mut m, &mut s, 0, &a, None);
        let b = land(&mut m, &mut s, 1, response(401, "expired"));
        let token = request(&b).continuation.unwrap();
        assert!(matches!(m.dispatch(token, &s), Dispatch::Held));
        assert_eq!(request(&land(&mut m, &mut s, 0, rotated())).method, "GET");
        let released = m.release(&s);
        assert_eq!(released.len(), 1);
        assert_eq!(
            request(&land(
                &mut m,
                &mut s,
                1,
                run(released.into_iter().next().unwrap().1)
            ))
            .method,
            "GET"
        );
        assert_eq!(disk.0.lock().unwrap().writes, 1);
    }
}

#[test]
fn native_secret_read_and_write_failures_release_the_lease() {
    for placement in [Placement::Main, Placement::Worker] {
        for write in [false, true] {
            let disk = backend();
            let mut m = module(placement, &disk);
            let mut s = store();
            begin(&mut m, &mut s, 0);
            let wait = land(&mut m, &mut s, 0, response(401, "expired"));
            if !write {
                disk.0.lock().unwrap().read_error = true;
            }
            let answer = acquire(&mut m, &mut s, 0, &wait, None);
            let answer = if write {
                disk.0.lock().unwrap().write_error = true;
                land(&mut m, &mut s, 0, rotated())
            } else {
                answer
            };
            assert!(text(answer).contains(if write { "write failed" } else { "read failed" }));
            assert!(!disk.0.lock().unwrap().held);
            assert!(!s.take_writes().iter().any(|w| w
                .value
                .as_ref()
                .is_some_and(|v| String::from_utf8_lossy(v).contains("rotate-1"))));
        }
    }
}

/// The secret a native host's Keychain keeps, behind the data crate's leases
struct Keychain(Mutex<Option<String>>);
impl exact_data::secrets::Backend for Keychain {
    fn read(&self, _: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn write(&self, _: &str, value: Option<&str>) -> Result<(), String> {
        *self.0.lock().unwrap() = value.map(str::to_owned);
        Ok(())
    }
}

// A refresh the token endpoint refuses keeps the session (LLP 1146 §5, "Refresh") through the next lease, which forgets
// a secret whose work threw after its POST
#[test]
fn a_refresh_the_token_endpoint_refuses_keeps_the_session_through_the_next_lease() {
    for placement in [Placement::Main, Placement::Worker] {
        let keychain = Arc::new(Keychain(Mutex::new(Some(OLD.into()))));
        let leases = exact_data::secrets::Coordinator::new(
            exact_data::secrets::Namespace::memory(),
            keychain.clone(),
        );
        let mut m = module_over(placement, Arc::new(leases));
        let (wake, woke) = std::sync::mpsc::channel();
        m.abort_wake(Arc::new(move || {
            let _ = wake.send(());
        }));
        let mut s = store();
        for id in 0..2 {
            begin(&mut m, &mut s, id);
            let wait = land(&mut m, &mut s, id, response(401, "expired"));
            let post = acquire(&mut m, &mut s, id, &wait, Some(&woke));
            assert_eq!(request(&post).method, "POST");
            assert!(String::from_utf8_lossy(&request(&post).body).contains("rotate-0"));
            // A 403 that is not an OAuth reply, as Lexus's token endpoint answers from another region
            let failed = land(&mut m, &mut s, id, response(403, "<html>Forbidden</html>"));
            assert_eq!(text(failed), "failed:The provider's reply was unreadable.");
            assert_eq!(keychain.0.lock().unwrap().as_deref(), Some(OLD));
        }
    }
}

struct Other;
impl DataSource for Other {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Ok(Value::Unit)
    }
    fn app_id(&self) -> &str {
        "test.context"
    }
    fn grants(&self) -> &str {
        "secret.keep token"
    }
}

fn kept_refresh<D: DataSource>(mut m: D, disk: Backend) {
    use exact_runner::InFlight;
    let (wake, woke) = std::sync::mpsc::channel();
    m.abort_wake(Arc::new(move || {
        let _ = wake.send(());
    }));
    let mut s = store();
    begin(&mut m, &mut s, 0);
    begin(&mut m, &mut s, 1);
    let a = land(&mut m, &mut s, 0, response(401, "expired"));
    let sent = acquire(&mut m, &mut s, 0, &a, None);
    assert_eq!(request(&sent).method, "POST");
    let b = land(&mut m, &mut s, 1, response(401, "expired"));
    let bt = request(&b).continuation.unwrap();
    assert!(matches!(m.dispatch(bt, &s), Dispatch::Held));
    let a_args = args(0);
    let b_args = args(1);
    m.forgotten(
        &s,
        &[InFlight {
            target: Target::Mutation(1),
            source: "secret",
            args: &b_args,
            fetch: None,
            continuation: Some(bt),
        }],
    );
    let kept = m.keep_sent(
        &s,
        &[InFlight {
            target: Target::Mutation(0),
            source: "secret",
            args: &a_args,
            fetch: request(&sent).fetch,
            continuation: None,
        }],
        false,
    );
    let token = kept[0].expect("the sent refresh retains its reply");
    let mut outcome = rotated();
    let mut released = Vec::new();
    for _ in 0..10 {
        let Some(request) = m.module_delivery(&mut s, token, outcome).unwrap() else {
            break;
        };
        let next = request.continuation.unwrap();
        let mut dispatch = m.dispatch(next, &s);
        while matches!(dispatch, Dispatch::Held) {
            woke.recv_timeout(Duration::from_secs(20))
                .expect("owner completion woke the host");
            released.extend(m.release(&s));
            if let Some(at) = released.iter().position(|(token, _)| *token == next) {
                dispatch = released.remove(at).1;
            }
        }
        outcome = run(dispatch);
    }
    assert_eq!(disk.0.lock().unwrap().writes, 1);
    released.extend(m.release(&s));
    let at = released
        .iter()
        .position(|(token, _)| *token == bt)
        .expect("waiting refresh released");
    let fresh = land(&mut m, &mut s, 1, run(released.remove(at).1));
    assert_eq!(request(&fresh).method, "GET");
    assert!(!disk.0.lock().unwrap().held);
    assert!(request(&fresh)
        .headers
        .iter()
        .any(|(_, v)| v == "Bearer fresh"));
    assert!(s.take_writes().iter().all(|w| w.durable));
}

#[test]
fn concurrent_protected_posts_keep_their_own_replies_after_let_go() {
    use exact_runner::InFlight;
    fn check(mut m: impl DataSource, disk: Backend, order: [usize; 2]) {
        let (wake, woke) = std::sync::mpsc::channel();
        m.abort_wake(Arc::new(move || {
            let _ = wake.send(());
        }));
        let mut s = store();
        let target = Target::Resource(0);
        let args = [Value::str("protected")];
        let mut answer = m.answer_for(target, &mut s, "secret", &args).unwrap();
        for _ in 0..10 {
            let r = request(&answer);
            let Some(token) = r.continuation else {
                break;
            };
            let fetch = r.fetch;
            let outcome = run(m.dispatch(token, &s));
            answer = m
                .parse_fetch_for(target, fetch, &mut s, "secret", &args, outcome)
                .unwrap()
                .unwrap();
        }
        let mut posts = vec![request(&answer).clone()];
        posts.extend(m.more_for(target));
        assert_eq!(posts.len(), 2);
        for (i, post) in posts.iter().enumerate() {
            assert_eq!(post.method, "POST");
            assert_eq!(
                post.url,
                format!("https://context.test/protected/{}", ["a", "b"][i])
            );
        }
        m.forgotten(&s, &[]);
        let view: Vec<_> = order
            .iter()
            .map(|&i| InFlight {
                target,
                source: "secret",
                args: &args,
                fetch: posts[i].fetch,
                continuation: None,
            })
            .collect();
        let kept = m.keep_sent(&s, &view, false);
        assert!(
            kept.iter().all(Option::is_some),
            "both protected requests keep their replies"
        );
        assert_ne!(kept[0], kept[1]);
        let mut expected = OLD.to_string();
        for (step, i) in order.into_iter().enumerate() {
            let mut outcome = response(200, ["A", "B"][i]);
            let token = kept[step].unwrap();
            let mut landed = false;
            for _ in 0..10 {
                let Some(request) = m.module_delivery(&mut s, token, outcome).unwrap() else {
                    landed = true;
                    break;
                };
                let next = request.continuation.unwrap();
                let mut dispatch = m.dispatch(next, &s);
                while matches!(dispatch, Dispatch::Held) {
                    woke.recv_timeout(Duration::from_secs(20))
                        .expect("the owner completed its turn");
                    if let Some((_, ready)) = m.release(&s).into_iter().find(|(t, _)| *t == next) {
                        dispatch = ready;
                    }
                }
                outcome = run(dispatch);
            }
            assert!(landed, "the kept reply completed its module turn");
            expected.push_str(["/a:A", "/b:B"][i]);
            assert_eq!(s.get("token"), Some(expected.as_str()));
            let disk = disk.0.lock().unwrap();
            assert_eq!(disk.value.as_deref(), Some(expected.as_str()));
            assert_eq!(disk.writes, step + 1);
            assert_eq!(disk.held, step == 0, "the lease covers both replies");
        }
    }
    for placement in [Placement::Main, Placement::Worker] {
        for order in [[0, 1], [1, 0]] {
            let disk = backend();
            check(module(placement, &disk), disk, order);
            let disk = backend();
            let js = module(placement, &disk);
            let rust = Placed::new(Other, Placement::Worker);
            let mut mixed = exact_data::Mixed::new(js, rust, &["secret"], &["unused"]).unwrap();
            mixed.activate().unwrap();
            check(exact_data_host::Storage::new(mixed), disk, order);
        }
    }
}

#[test]
fn superseded_refresh_persists_before_releasing_main_worker_and_ordered_sets() {
    for placement in [Placement::Main, Placement::Worker] {
        let disk = backend();
        kept_refresh(module(placement, &disk), disk);
        let disk = backend();
        let js = module(placement, &disk);
        let rust = Placed::new(Other, Placement::Worker);
        let mut mixed = exact_data::Mixed::new(js, rust, &["secret"], &["unused"]).unwrap();
        mixed.activate().unwrap();
        kept_refresh(mixed, disk);
    }
}

#[test]
fn releasing_before_a_workers_acquire_envelope_is_applied_does_not_consume_it() {
    let disk = backend();
    let mut m = module(Placement::Worker, &disk);
    let mut s = store();
    begin(&mut m, &mut s, 0);
    let answer = m
        .parse_for(
            Target::Mutation(0),
            &mut s,
            "secret",
            &args(0),
            response(401, "expired"),
        )
        .unwrap();
    let token = request(&answer).continuation.unwrap();
    let envelope = run(m.dispatch(token, &s));
    assert!(m.release(&s).is_empty());
    let wait = m
        .parse_for(Target::Mutation(0), &mut s, "secret", &args(0), envelope)
        .unwrap();
    assert_eq!(
        request(&acquire(&mut m, &mut s, 0, &wait, None)).method,
        "POST"
    );
}

#[test]
fn an_interrupted_or_over_budget_lease_turn_does_not_strand_other_realms() {
    for interrupt in [false, true] {
        let disk = backend();
        let plan=contract::compile("shape Result\n  text: string\ncomponent App\n  mutation output as shape Result\n  action go(name: string)\n    send output = secret(name)\n  view\n    text \"secret\"\n").unwrap();
        let mut m = Module::new(HBC.to_vec(), "test.context", GRANTS);
        m.set_budget_ms(f64::INFINITY);
        m.configure_secrets(Arc::new(disk.clone())).unwrap();
        m.bind(&plan);
        m.activate().unwrap();
        let mut s = store();
        begin(&mut m, &mut s, 0);
        let wait = land(&mut m, &mut s, 0, response(401, "expired"));
        let token = request(&wait).continuation.unwrap();
        let outcome = run(m.dispatch(token, &s));
        assert!(disk.0.lock().unwrap().held);
        if interrupt {
            m.interrupt().unwrap().trigger();
        } else {
            m.set_budget_ms(0.0);
        }
        assert!(m
            .parse_for(Target::Mutation(0), &mut s, "secret", &args(0), outcome)
            .is_err());
        assert!(!disk.0.lock().unwrap().held);
        let mut next = module(Placement::Main, &disk);
        let mut next_store = store();
        begin(&mut next, &mut next_store, 0);
        let wait = land(&mut next, &mut next_store, 0, response(401, "expired"));
        assert_eq!(
            request(&acquire(&mut next, &mut next_store, 0, &wait, None)).method,
            "POST"
        );
    }
}

#[test]
fn retiring_a_queued_acquire_leaves_the_holder_and_next_caller_live() {
    use exact_runner::InFlight;
    for placement in [Placement::Main, Placement::Worker] {
        let disk = backend();
        let mut m = module(placement, &disk);
        let mut s = store();
        begin(&mut m, &mut s, 0);
        begin(&mut m, &mut s, 1);
        let a = land(&mut m, &mut s, 0, response(401, "expired"));
        acquire(&mut m, &mut s, 0, &a, None);
        let b = land(&mut m, &mut s, 1, response(401, "expired"));
        assert!(matches!(
            m.dispatch(request(&b).continuation.unwrap(), &s),
            Dispatch::Held
        ));
        let a_args = args(0);
        m.forgotten(
            &s,
            &[InFlight {
                target: Target::Mutation(0),
                source: "secret",
                args: &a_args,
                fetch: None,
                continuation: None,
            }],
        );
        m.keep_sent(&s, &[], false);
        assert_eq!(request(&land(&mut m, &mut s, 0, rotated())).method, "GET");
        assert!(m.release(&s).is_empty());
        assert!(!disk.0.lock().unwrap().held);
        let mut next = module(placement, &disk);
        let mut next_store = store();
        begin(&mut next, &mut next_store, 0);
        let next_wait = land(&mut next, &mut next_store, 0, response(401, "expired"));
        assert_eq!(
            request(&acquire(&mut next, &mut next_store, 0, &next_wait, None)).method,
            "GET"
        );
        assert_eq!(disk.0.lock().unwrap().writes, 1);
    }
}

#[test]
fn validation_never_acquires_the_live_secret_backend() {
    // A painted host loads the validation pass before its boot offers the
    // session's secrets, so the offer comes after activation too.
    for offered_after_activation in [false, true] {
        let disk = backend();
        let plan = contract::compile("shape Result\n  text: string\ncomponent App\n  mutation output as shape Result\n  action go(name: string)\n    send output = secret(name)\n  view\n    text \"secret\"\n").unwrap();
        let mut m = Module::new(HBC.to_vec(), "test.context", GRANTS);
        m.set_budget_ms(f64::INFINITY);
        if !offered_after_activation {
            m.configure_secrets(Arc::new(disk.clone())).unwrap();
        }
        m.bind(&plan);
        m.activate_for_validation().unwrap();
        if offered_after_activation {
            m.configure_secrets(Arc::new(disk.clone())).unwrap();
        }
        let mut s = store();
        begin(&mut m, &mut s, 0);
        let failed = land(&mut m, &mut s, 0, response(401, "expired"));
        assert!(text(failed).contains("validation"));
        assert_eq!(disk.0.lock().unwrap().acquires, 0);
    }
}

#[test]
fn two_read_only_resources_on_an_absent_secret_settle_without_reasking_each_other() {
    use std::collections::{HashMap, VecDeque};
    for placement in [Placement::Main, Placement::Worker] {
        let disk = Backend(Arc::new(Mutex::new(Disk::default())));
        let module = module(placement, &disk);
        let plan=contract::compile("shape Result\n  text: string\ncomponent App\n  resource a = secret(\"read-a\") as shape Result\n  resource b = secret(\"read-b\") as shape Result\n  view\n    column\n      text a.text\n      text b.text\n").unwrap();
        let mut runner = exact_runner::Runner::boot(
            plan,
            module,
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let mut held = HashMap::new();
        let mut work = VecDeque::new();
        for _ in 0..20 {
            for request in runner.take_requests() {
                let token = request
                    .request
                    .continuation
                    .expect("local lease or owner turn");
                match runner.dispatch_work(token) {
                    Dispatch::Held => {
                        held.insert(token, request.ticket);
                    }
                    dispatch => work.push_back((request.ticket, dispatch)),
                }
            }
            for (token, dispatch) in runner.release_work() {
                let ticket = held.remove(&token).expect("previously held work");
                work.push_back((ticket, dispatch));
            }
            let Some((ticket, dispatch)) = work.pop_front() else {
                break;
            };
            runner.fulfill(ticket, run(dispatch)).unwrap();
        }
        assert!(
            work.is_empty() && held.is_empty(),
            "resources did not settle"
        );
        assert!(
            runner.take_requests().is_empty(),
            "absence synchronization reasked a reader"
        );
        for name in ["a", "b"] {
            let Some(Value::Record(fields)) = runner.resource(name) else {
                panic!("resource {name} has no answer")
            };
            assert_eq!(fields[0].as_str(), Some("empty"));
        }
        assert_eq!(
            disk.0.lock().unwrap().acquires,
            2,
            "each resource acquired once"
        );
        let writes: Vec<_> = runner
            .take_store_writes()
            .into_iter()
            .filter(|write| write.name == "token")
            .collect();
        assert_eq!(
            writes.len(),
            2,
            "each durable absence still acknowledges old retries"
        );
        assert!(writes
            .iter()
            .all(|write| write.durable && write.value.is_none()));
    }
}
