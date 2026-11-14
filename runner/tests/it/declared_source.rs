//! A source declared in Contract (LLP 1162.001): the runner writes its
//! request and reads its reply, and the app's data module is never asked.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, FailureKind, Outcome, RequestClass, RequestOut, Response,
    Runner, Store,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const SOURCE: &str = r##"shape Current
  temperature_2m: number
  wind: option<number>

shape Ack
  id: string

shape Repo
  name: string
  stars: number

source weather(latitude: number, place: string) as shape Current
  url "https://api.example.test/v1/forecast?latitude={latitude}&place={place}"
  pointer "/current"

source save(title: string, done: bool) as shape Ack
  url "https://api.example.test/items"
  method "POST"
  body json

source repo(owner: string) as shape Repo
  url "https://graph.example.test/graphql"
  header "Authorization" "Bearer {secret:graph.token}"
  graphql "query($owner: String!) { repo(owner: $owner) { name stars } }"
  pointer "/data/repo"

component App
  state place = "Palo Alto & Co"
  resource now = weather(52.5, place) as shape Current
  resource gh = repo("expo") as shape Repo
  mutation saved as shape Ack
  derive code = match failure(now) { case some(f) => f.code, case none => "" }
  derive why = match failure(now) { case some(f) => f.message, case none => "" }
  derive ghCode = match failure(gh) { case some(f) => f.code, case none => "" }
  derive ghReason = match failure(gh) { case some(f) => f.reason, case none => "" }
  derive savedId = match saved { case some(a) => a.id, case none => "" }
  action again
    refresh now
  action add
    send saved = save("Milk", false)
  view
    text `${now.temperature_2m}`
"##;

/// A data module that answers nothing and is never asked.
struct Nothing(&'static str);

impl DataSource for Nothing {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("the data module was asked for {source}")
    }
    fn grants(&self) -> &str {
        self.0
    }
}

fn boot(grants: &'static str, secrets: Vec<(String, Vec<u8>)>) -> Runner<Nothing> {
    Runner::boot_stored(
        contract::compile(SOURCE).unwrap(),
        Nothing(grants),
        Kernel::with_monospace(),
        secrets,
        Default::default(),
        "/",
    )
    .unwrap()
}

fn json(status: u16, body: &str) -> Outcome {
    Outcome::Response(Response {
        status,
        headers: vec![("content-type".into(), "application/json".into())],
        body: body.as_bytes().to_vec(),
    })
}

fn named<'a>(requests: &'a [RequestOut], target: &str) -> &'a RequestOut {
    (requests.iter().find(|r| r.target == target))
        .unwrap_or_else(|| panic!("no request for {target}: {requests:?}"))
}

fn header<'a>(out: &'a RequestOut, name: &str) -> Option<&'a str> {
    let headers = &out.request.headers;
    (headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))).map(|(_, v)| v.as_str())
}

#[test]
fn a_read_is_written_from_its_declaration_and_read_into_its_shape() {
    let mut r = boot("", Vec::new());
    let requests = r.take_requests();
    let now = named(&requests, "now");
    assert_eq!(now.request.method, "GET");
    assert_eq!(
        now.request.url,
        "https://api.example.test/v1/forecast?latitude=52.5&place=Palo%20Alto%20%26%20Co"
    );
    assert_eq!(header(now, "accept"), Some("application/json"));
    assert_eq!(
        now.request.grants.as_deref(),
        Some("net.fetch https://api.example.test")
    );
    assert_eq!(now.class, RequestClass::Read);
    // The declarations are the app's grants, beside the module's.
    let grants = r.grants();
    assert!(
        grants.contains("net.fetch https://api.example.test"),
        "{grants}"
    );
    assert!(
        grants.contains("net.fetch https://graph.example.test"),
        "{grants}"
    );
    // Extra members are left out; an absent option is `none`.
    let ticket = now.ticket;
    r.fulfill(
        ticket,
        json(
            200,
            r#"{"latitude":52.5,"current":{"temperature_2m":12.5,"time":"now"}}"#,
        ),
    )
    .unwrap();
    assert_eq!(
        r.resource("now"),
        Some(&Value::record(vec![
            Value::Number(12.5),
            Value::Option(None)
        ]))
    );
    // A refresh asks again, forced, past the cache.
    r.act("again", vec![]).unwrap();
    let again = r.take_requests();
    assert!(named(&again, "now").forced);
}

#[test]
fn a_failure_is_what_a_typescript_sources_would_be() {
    let cases: [(Outcome, &str, &str); 4] = [
        (
            json(503, "{}"),
            "error",
            "HTTP 503 from https://api.example.test",
        ),
        (
            Outcome::Failed {
                kind: FailureKind::Network,
                message: "no route".into(),
            },
            "offline",
            "no route",
        ),
        (json(200, "<html>"), "error", "not JSON"),
        (
            json(200, r#"{"current":{"wind":3}}"#),
            "shape",
            "temperature_2m",
        ),
    ];
    for (outcome, code, why) in cases {
        let mut r = boot("", Vec::new());
        let ticket = named(&r.take_requests(), "now").ticket;
        r.fulfill(ticket, outcome).unwrap();
        assert_eq!(r.derive("code"), Some(&Value::str(code)));
        let message = r.derive("why").unwrap().as_str().unwrap().to_string();
        assert!(message.contains(why), "{message}");
    }
    let mut r = boot("", Vec::new());
    let ticket = named(&r.take_requests(), "now").ticket;
    r.fulfill(ticket, json(200, r#"{"other":1}"#)).unwrap();
    assert_eq!(r.derive("code"), Some(&Value::str("shape")));
}

#[test]
fn a_send_posts_its_parameters_as_json() {
    let mut r = boot("", Vec::new());
    r.take_requests();
    r.act("add", vec![]).unwrap();
    let requests = r.take_requests();
    let saved = named(&requests, "saved");
    assert_eq!(saved.request.method, "POST");
    assert_eq!(saved.class, RequestClass::Ordered);
    assert_eq!(header(saved, "content-type"), Some("application/json"));
    let body: serde_json::Value = serde_json::from_slice(&saved.request.body).unwrap();
    assert_eq!(body, serde_json::json!({"title": "Milk", "done": false}));
    let ticket = saved.ticket;
    r.fulfill(ticket, json(201, r#"{"id":"7","title":"Milk"}"#))
        .unwrap();
    assert_eq!(r.derive("savedId"), Some(&Value::str("7")));
}

#[test]
fn a_graphql_operation_posts_its_document_and_reads_its_errors() {
    let token = vec![("graph.token".to_string(), b"t0k".to_vec())];
    let mut r = boot("secret.keep graph.token", token);
    let requests = r.take_requests();
    let gh = named(&requests, "gh");
    assert_eq!(gh.request.method, "POST");
    assert_eq!(header(gh, "authorization"), Some("Bearer t0k"));
    let body: serde_json::Value = serde_json::from_slice(&gh.request.body).unwrap();
    assert_eq!(body["variables"], serde_json::json!({"owner": "expo"}));
    assert!(body["query"].as_str().unwrap().starts_with("query($owner"));
    // A resource's operation is a read, whatever its method (LLP 1162 D1).
    assert_eq!(gh.class, RequestClass::Read);
    let ticket = gh.ticket;
    r.fulfill(
        ticket,
        json(
            200,
            r#"{"data":null,"errors":[{"message":"no such owner","extensions":{"code":"NOT_FOUND"}}]}"#,
        ),
    )
    .unwrap();
    assert_eq!(r.derive("ghReason"), Some(&Value::str("NOT_FOUND")));

    let mut r = boot(
        "secret.keep graph.token",
        vec![("graph.token".into(), b"t".to_vec())],
    );
    let ticket = named(&r.take_requests(), "gh").ticket;
    r.fulfill(
        ticket,
        json(
            200,
            r#"{"data":{"repo":{"name":"exact","stars":9,"forks":1}}}"#,
        ),
    )
    .unwrap();
    assert_eq!(
        r.resource("gh"),
        Some(&Value::record(vec![Value::str("exact"), Value::Number(9.)]))
    );
}

#[test]
fn a_header_whose_secret_is_absent_sends_nothing() {
    // Not granted, and granted but not kept: neither is sent.
    for grants in ["", "secret.keep graph.token"] {
        let mut r = boot(grants, Vec::new());
        let requests = r.take_requests();
        assert!(requests.iter().all(|o| o.target != "gh"), "{requests:?}");
        assert_eq!(r.derive("ghCode"), Some(&Value::str("refused")));
    }
}

const MIXED: &str = r##"shape Current
  temperature_2m: number

shape Ack
  id: string

source weather(place: string) as shape Current
  url "https://api.example.test/forecast?place={place}"
  pointer "/current"

source save(title: string) as shape Ack
  url "https://api.example.test/items"
  method "POST"
  body json

component App
  resource now = weather("here") as shape Current
  resource notes = loadNotes() as shape string[] else empty()
  mutation saved as shape Ack queue
  action add
    send saved = save("Milk")
  view
    text `${now.temperature_2m} ${notes.length}`
"##;

/// A data module that has not loaded, and answers nothing when asked.
struct Loading;

impl DataSource for Loading {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::Unavailable(source.into()))
    }
    fn ready(&self) -> bool {
        false
    }
}

/// In an app with a data module too, a declared source asks at once: no
/// module answers it, so none it waits for (LLP 1162.001 D4). The module's
/// own resource waits for it, as before.
#[test]
fn a_declared_source_does_not_wait_for_the_data_module() {
    let mut r = Runner::boot(
        contract::compile(MIXED).unwrap(),
        Loading,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let requests = r.take_requests();
    named(&requests, "now");
    assert!(requests.iter().all(|o| o.target != "notes"), "{requests:?}");
    // A queued send to one goes out, and the one after it once it lands.
    r.act("add", vec![]).unwrap();
    r.act("add", vec![]).unwrap();
    let mut sent = 0;
    for _ in 0..4 {
        r.advance(r.now_ms() + 1.0).unwrap();
        let requests = r.take_requests();
        let Some(out) = requests.iter().find(|o| o.target == "saved") else {
            continue;
        };
        sent += 1;
        let ticket = out.ticket;
        r.fulfill(ticket, json(201, r#"{"id":"7"}"#)).unwrap();
    }
    assert_eq!(
        sent, 2,
        "both queued sends went out before the module loaded"
    );
}

const READS: &str = r##"shape Current
  temperature_2m: number

source weather(place: string) as shape Current
  url "https://api.example.test/forecast?place={place}"
  pointer "/current"

source nearby(place: string, city: string) as shape Current
  url "https://api.example.test/near?place={place}&city={city}"
  pointer "/current"

component App
  state places: string[] = ["harbor"]
  resource city = savedCity() as shape string
  resource now = weather(city) as shape Current
  resource near = nearby(p, city) for p in places as shape Current
  derive temperature = now.temperature_2m
  view
    text `${now.temperature_2m}`
"##;

/// A data module whose `savedCity()` answers the city its store keeps, or
/// Berlin, and `dress(t)` what to wear at `t` degrees, once it has loaded
/// (`loaded`).
#[derive(Clone, Default)]
struct Module {
    loaded: Rc<Cell<bool>>,
    asked: Rc<RefCell<Vec<String>>>,
}

impl Module {
    fn loaded() -> Module {
        let module = Module::default();
        module.loaded.set(true);
        module
    }
}

impl DataSource for Module {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        assert!(self.loaded.get(), "the module was asked before it loaded");
        self.asked.borrow_mut().push(source.into());
        Ok(Answer::Now(match source {
            "savedCity" => Value::str(store.get("city").unwrap_or("Berlin")),
            "dress" if args[0].as_number() > Some(10.0) => Value::str("shorts"),
            "dress" => Value::str("coat"),
            other => return Err(DataError::UnknownSource(other.into())),
        }))
    }
    fn ready(&self) -> bool {
        self.loaded.get()
    }
    fn grants(&self) -> &str {
        "secret.keep city\n"
    }
}

/// The URLs the host was handed.
fn urls(r: &mut Runner<Module>) -> Vec<String> {
    (r.take_requests().into_iter())
        .map(|o| o.request.url)
        .collect()
}

/// The checkpoint of a page whose module read no `city`, and whose
/// forecasts answered 12.
fn rendered(plan: &exact_plan::Plan) -> exact_runner::Checkpoint {
    let mut render = Runner::boot(
        plan.clone(),
        Module::loaded(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for out in render.take_requests() {
        let reply = json(200, r#"{"current":{"temperature_2m":12}}"#);
        render.fulfill(out.ticket, reply).unwrap();
    }
    render.document_checkpoint("/")
}

/// A render holds no secret, so a declared source's page answer that read
/// the store read it only through its arguments: the page doesn't list it in
/// `readers` (LLP 1048.000 D6), and a device that boots from the page asks it
/// when its arguments change, once the module's reader they read answers,
/// never at the same arguments. This holds for a declared resource and for a
/// list's elements.
#[test]
fn a_declared_source_s_page_answer_is_asked_when_its_arguments_change() {
    let plan = contract::compile(READS).unwrap();
    let checkpoint = rendered(&plan);
    let readers: Vec<&str> = (checkpoint.readers.iter())
        .map(|&k| checkpoint.answers[k].0.as_str())
        .collect();
    assert_eq!(readers, ["city"]);

    // The module loads after the boot, on a device whose store keeps `city`
    let launch = |city: &str| {
        let module = Module::default();
        let mut r = Runner::boot_checkpoint(
            plan.clone(),
            module.clone(),
            Kernel::with_monospace(),
            &checkpoint,
            vec![("city".into(), city.as_bytes().to_vec())],
            Default::default(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert!(urls(&mut r).is_empty(), "nothing is asked at boot");
        assert_eq!(r.derive("temperature"), Some(&Value::Number(12.0)));
        module.loaded.set(true);
        r.data_ready().unwrap();
        assert_eq!(*module.asked.borrow(), ["savedCity"]);
        urls(&mut r)
    };
    assert_eq!(
        launch("Paris"),
        [
            "https://api.example.test/forecast?place=Paris",
            "https://api.example.test/near?place=harbor&city=Paris",
        ]
    );
    assert!(launch("Berlin").is_empty());
}

/// A page's answer that the launch asks again is asked at boot when its
/// source can answer, before the module loads, as the launch's own ask (LLP
/// 1048.000 D6). A JavaScript render lists a request still pending at its
/// end in `open`, a declared one too.
#[test]
fn a_page_answer_asked_again_is_asked_at_boot_when_its_source_can_answer() {
    let plan = contract::compile(READS).unwrap();
    let mut checkpoint = rendered(&plan);
    checkpoint.open = (0..checkpoint.answers.len())
        .filter(|&k| checkpoint.answers[k].0 != "city")
        .collect();
    let mut r = Runner::boot_checkpoint(
        plan,
        Module::default(),
        Kernel::with_monospace(),
        &checkpoint,
        Vec::new(),
        Default::default(),
        Default::default(),
        "/",
    )
    .unwrap();
    let requests = r.take_requests();
    assert!(requests.iter().all(|o| !o.forced), "{requests:?}");
    let urls: Vec<&str> = (requests.iter()).map(|o| o.request.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://api.example.test/forecast?place=Berlin",
            "https://api.example.test/near?place=harbor&city=Berlin",
        ]
    );
}

/// A declared source never waits for the data module, so a launch asks a
/// declared resource at boot and keeps no answer of it for the next launch
/// (LLP 1027 D4, LLP 1162.001 D4). Nor does it keep one of a resource whose
/// identifying arguments read a declared resource: the declared resource
/// shows its placeholder until it answers, so such a kept answer would not
/// fit (LLP 1027.005 D3). A launch whose module loads after the boot forgets what an
/// earlier build kept for both, asks the declared resource at boot, at the
/// arguments the module's kept answer gives, and not again when the module
/// loads, and keeps neither reply.
#[test]
fn a_declared_reader_and_a_resource_it_identifies_keep_no_answer() {
    let src = READS.replace(
        "  derive temperature",
        "  resource outfit = dress(now.temperature_2m) as shape string\n  derive temperature",
    );
    let plan = contract::bake(contract::compile(&src).unwrap(), Module::loaded()).unwrap();
    let kept = |args: Vec<Value>, value: Value| {
        let mut w = exact_plan::bytes::Writer::default();
        Value::list(args).encode(&mut w);
        value.encode(&mut w);
        w.into_vec()
    };
    let snapshot = vec![
        ("city".to_string(), b"Paris".to_vec()),
        (
            "exact.kept.city".into(),
            kept(Vec::new(), Value::str("Paris")),
        ),
        (
            "exact.kept.now".into(),
            kept(
                vec![Value::str("Paris")],
                Value::record(vec![Value::Number(3.0)]),
            ),
        ),
        (
            "exact.kept.outfit".into(),
            kept(vec![Value::Number(3.0)], Value::str("coat")),
        ),
    ];
    let module = Module::default();
    let mut r = Runner::boot_stored(
        plan,
        module.clone(),
        Kernel::with_monospace(),
        snapshot,
        Default::default(),
        "/",
    )
    .unwrap();
    let requests = r.take_requests();
    let now = named(&requests, "now");
    assert_eq!(
        now.request.url,
        "https://api.example.test/forecast?place=Paris"
    );
    assert!(!now.forced, "the launch's own ask");
    assert_eq!(
        r.derive("temperature"),
        Some(&Value::Number(0.0)),
        "the placeholder, not an answer kept for it"
    );
    module.loaded.set(true);
    r.data_ready().unwrap();
    assert!((r.take_requests().iter()).all(|o| o.target != "now"));
    let reply = json(200, r#"{"current":{"temperature_2m":12}}"#);
    r.fulfill(now.ticket, reply).unwrap();
    assert_eq!(r.resource("outfit"), Some(&Value::str("shorts")));
    let writes = r.take_store_writes();
    for name in ["exact.kept.now", "exact.kept.outfit"] {
        let kept: Vec<_> = (writes.iter())
            .filter(|w| w.name == name)
            .map(|w| &w.value)
            .collect();
        assert_eq!(kept, [&None], "{name}: forgotten, never kept");
    }
}
