//! A child's sends (LLP 1035.005.004 D6): each instance of a component that
//! declares a mutation sends on its own, its `then` runs in its frames, and
//! destroying it forgets what it sent.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    agent, Answer, DataError, DataSource, Event, Outcome, Request, Response, Runner, Store,
};

/// `detect(id)` and `save(id)` answer later, `notes(id)` later too, and
/// `quick(id)` at once; every answer's lines name what was asked.
#[derive(Default)]
struct Books {
    asks: Vec<String>,
}

fn lines(args: &[Value], prefix: &str) -> Value {
    let id = args.first().and_then(Value::as_str).unwrap_or("");
    Value::record(vec![Value::list(vec![Value::str(&format!(
        "{prefix}{id}"
    ))])])
}

impl DataSource for Books {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, name: &str, args: &[Value]) -> Result<Answer, DataError> {
        let id = args.first().and_then(Value::as_str).unwrap_or("");
        self.asks.push(format!("{name}({id})"));
        match name {
            "detect" | "save" | "notes" => Ok(Answer::Later(Request::get(&format!(
                "https://books.test/{name}/{id}"
            )))),
            "quick" => Ok(Answer::Now(lines(args, "quick "))),
            "bad" if id == "reject" => Err(DataError::BadArguments("seats full".into())),
            "bad" if id == "closed" => Err(DataError::Unavailable("closed".into())),
            "bad" => Ok(Answer::Now(Value::Number(1.0))),
            _ => Err(DataError::UnknownSource(name.into())),
        }
    }
    fn parse(
        &mut self,
        _: &mut Store,
        name: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(lines(args, &format!("{name} "))))
    }
}

/// A stack of book screens. Each owns its detection, whose `then` hands the
/// lines to the root's `showLines` (Shelf's quote), and a queued save that
/// refreshes its own notes.
const BOOKS: &str = "routes nav
  list \"/\"
  book \"/book/:id\"

shape Lines
  lines: string[]

component App
  state shown = \"\"
  action open(id: string)
    nav = push(nav, path(\"book\", id))
  action back
    nav = back(nav)
  action showLines(lines: string[])
    shown = lines.join(\",\")
  view
    main
      text shown testId=\"shown\"
      for e in stack(nav) key=e.id
        if e.name == \"book\"
          Book(id=e.params.id, show=showLines)
        else
          text \"List\" testId=\"list\"

component Book
  props
    id: string
    show: action
  resource notes = notes(id) as shape Lines else empty()
  mutation detected as shape Lines then got
  mutation saved as shape Lines queue refreshes notes
  action detect
    send detected = detect(id)
  action detectNow
    send detected = quick(id)
  action broken
    send detected = bad(id)
  action clear
    detected = none
  action got
    match detected
      case some(d)
        show(d.lines)
      case none
        show([])
  action save
    send saved = save(id)
  view
    column
      text (pending(detected) ? \"detecting\" : match detected { case some(d) => d.lines.join(\",\"), case none => \"idle\" }) testId=`state-${id}`
      text (pending(saved) ? \"saving\" : \"saved\") testId=`saving-${id}`
      text notes.lines.join(\",\") testId=`notes-${id}`
      button press=detect testId=`detect-${id}`
        text \"Detect\"
      button press=detectNow testId=`now-${id}`
        text \"Now\"
      button press=broken testId=`bad-${id}`
        text \"Bad\"
      button press=clear testId=`clear-${id}`
        text \"Clear\"
      button press=save testId=`save-${id}`
        text \"Save\"
";

fn boot() -> Runner<Books> {
    let plan = contract::compile(BOOKS).unwrap_or_else(|e| panic!("{e}"));
    Runner::boot(
        plan,
        Books::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// The text of the node with `test_id`, or `None` when none shows.
fn text<D: DataSource>(r: &Runner<D>, test_id: &str) -> Option<String> {
    let tree = agent::tree(r);
    let key = format!("\"testId\":\"{test_id}\"");
    let node = tree.split("{\"id\":").find(|n| n.contains(&key))?;
    let at = node.find("\"text\":\"")? + 8;
    Some(node[at..].split('"').next()?.to_string())
}

fn press<D: DataSource>(r: &mut Runner<D>, test_id: &str) {
    let found = agent::handle(
        r,
        &format!("{{\"op\":\"tree\",\"target\":\"{test_id}\",\"shallow\":true}}"),
    );
    let id: u32 = found
        .split("{\"id\":")
        .nth(1)
        .unwrap_or_else(|| panic!("{test_id} in {found}"))
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    r.dispatch(id, Event::Press).unwrap();
}

fn ok() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: b"{}".to_vec(),
    })
}

/// Opens book `id` and lands its notes.
fn open(r: &mut Runner<Books>, id: &str) {
    r.act("open", vec![Value::str(id)]).unwrap();
    for q in r.take_requests() {
        r.fulfill(q.ticket, ok()).unwrap();
    }
}

#[test]
fn a_screens_then_runs_in_its_frame_and_hands_the_lines_to_the_root() {
    let mut r = boot();
    open(&mut r, "1");
    assert_eq!(text(&r, "notes-1").as_deref(), Some("notes 1"));
    press(&mut r, "detect-1");
    assert_eq!(text(&r, "state-1").as_deref(), Some("detecting"));
    let sent = r.take_requests();
    assert_eq!(sent.len(), 1);
    r.fulfill(sent[0].ticket, ok()).unwrap();
    assert_eq!(text(&r, "state-1").as_deref(), Some("detect 1"));
    // The `then` is its own commit: it reads the instance's answer and calls
    // the root's action with it.
    assert_eq!(text(&r, "shown").as_deref(), Some(""));
    assert!(r.land_then().error.is_none());
    assert_eq!(text(&r, "shown").as_deref(), Some("detect 1"));

    // An answer at once lands in the instance, and its `then` follows.
    press(&mut r, "now-1");
    assert_eq!(text(&r, "state-1").as_deref(), Some("quick 1"));
    assert!(r.land_then().error.is_none());
    assert_eq!(text(&r, "shown").as_deref(), Some("quick 1"));
    // The instance's own assignment clears its answer.
    press(&mut r, "clear-1");
    assert_eq!(text(&r, "state-1").as_deref(), Some("idle"));
}

#[test]
fn leaving_the_screen_before_its_answer_forgets_the_send_and_runs_no_then() {
    let mut r = boot();
    open(&mut r, "1");
    press(&mut r, "detect-1");
    let sent = r.take_requests();
    assert!(r.holds(sent[0].ticket));
    r.act("back", vec![]).unwrap();
    // The send was made and stays made; its ticket is forgotten.
    assert!(!r.holds(sent[0].ticket));
    assert_eq!(r.fulfill(sent[0].ticket, ok()).unwrap(), None);
    assert!(r.land_then().receipts.is_empty());
    assert_eq!(text(&r, "shown").as_deref(), Some(""));

    // The same book opened again is a new instance: nothing of its
    // predecessor's send reaches it.
    open(&mut r, "1");
    assert_eq!(text(&r, "state-1").as_deref(), Some("idle"));
    assert_eq!(r.fulfill(sent[0].ticket, ok()).unwrap(), None);
    assert_eq!(text(&r, "state-1").as_deref(), Some("idle"));
}

#[test]
fn a_covered_screen_keeps_its_instance_and_its_then_runs() {
    let mut r = boot();
    open(&mut r, "1");
    press(&mut r, "detect-1");
    let sent = r.take_requests();
    // A push covers the screen; it stays, so its send's `then` still runs.
    open(&mut r, "2");
    assert_eq!(text(&r, "state-2").as_deref(), Some("idle"));
    assert_eq!(text(&r, "state-1").as_deref(), Some("detecting"));
    r.fulfill(sent[0].ticket, ok()).unwrap();
    assert!(r.land_then().error.is_none());
    assert_eq!(text(&r, "shown").as_deref(), Some("detect 1"));
    assert_eq!(text(&r, "state-2").as_deref(), Some("idle"));
}

#[test]
fn two_screens_send_apart() {
    let mut r = boot();
    open(&mut r, "1");
    open(&mut r, "2");
    press(&mut r, "detect-1");
    press(&mut r, "detect-2");
    let sent = r.take_requests();
    assert_eq!(sent.len(), 2, "one send each: neither replaces the other");
    assert_eq!(text(&r, "state-1").as_deref(), Some("detecting"));
    assert_eq!(text(&r, "state-2").as_deref(), Some("detecting"));
    let two = sent.iter().find(|q| q.request.url.ends_with("/2")).unwrap();
    r.fulfill(two.ticket, ok()).unwrap();
    assert_eq!(text(&r, "state-1").as_deref(), Some("detecting"));
    assert_eq!(text(&r, "state-2").as_deref(), Some("detect 2"));
    assert!(r.land_then().error.is_none());
    assert_eq!(text(&r, "shown").as_deref(), Some("detect 2"));
}

#[test]
fn a_screens_queue_refreshes_its_own_notes_and_is_dropped_with_it() {
    let mut r = boot();
    open(&mut r, "1");
    open(&mut r, "2");
    assert_eq!(
        r.data()
            .asks
            .iter()
            .filter(|a| a.starts_with("notes"))
            .count(),
        2
    );
    // Two saves: one in flight, one waiting its turn.
    press(&mut r, "save-2");
    press(&mut r, "save-2");
    assert_eq!(text(&r, "saving-2").as_deref(), Some("saving"));
    assert_eq!(text(&r, "saving-1").as_deref(), Some("saved"));
    let first = r.take_requests();
    assert_eq!(first.len(), 1);
    r.fulfill(first[0].ticket, ok()).unwrap();
    // Its reply refreshes this instance's notes only.
    let asked: Vec<_> = r.take_requests();
    assert_eq!(asked.len(), 1, "{:?}", r.data().asks);
    assert_eq!(r.data().asks.last().unwrap(), "notes(2)");
    // Its next waits for the clock; back destroys the screen first, and the
    // waiting save goes with it.
    r.act("back", vec![]).unwrap();
    assert!(r.land_then().receipts.is_empty());
    assert_eq!(
        r.data()
            .asks
            .iter()
            .filter(|a| a.starts_with("save"))
            .count(),
        1
    );
    assert!(r.take_requests().is_empty());
    assert_eq!(text(&r, "saving-1").as_deref(), Some("saved"));
}

#[test]
fn a_use_outside_every_region_sends_as_the_root_does() {
    let source = BOOKS.replace(
        "      for e in stack(nav) key=e.id
        if e.name == \"book\"
          Book(id=e.params.id, show=showLines)
        else
          text \"List\" testId=\"list\"
",
        "      Book(id=\"9\", show=showLines)\n",
    );
    let plan = contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
    let mut r = Runner::boot(
        plan,
        Books::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for q in r.take_requests() {
        r.fulfill(q.ticket, ok()).unwrap();
    }
    press(&mut r, "detect-9");
    assert_eq!(text(&r, "state-9").as_deref(), Some("detecting"));
    for q in r.take_requests() {
        r.fulfill(q.ticket, ok()).unwrap();
    }
    assert!(r.land_then().error.is_none());
    assert_eq!(text(&r, "shown").as_deref(), Some("detect 9"));
}

#[test]
fn a_refused_send_leaves_the_instance_as_it_was() {
    let mut r = boot();
    open(&mut r, "1");
    open(&mut r, "2");
    // An answer outside its shape refuses the action that sent it, after
    // its first send gave the instance a row.
    let bad = id_of(&r, "bad-2");
    assert!(r.dispatch(bad, Event::Press).is_err());
    assert_eq!(text(&r, "state-2").as_deref(), Some("idle"));
    // Both screens still send, each on its own.
    press(&mut r, "detect-2");
    r.act("back", vec![]).unwrap();
    press(&mut r, "detect-1");
    assert_eq!(text(&r, "state-1").as_deref(), Some("detecting"));
    let sent = r.take_requests();
    assert_eq!(sent.len(), 2);
    assert!(
        !r.holds(sent[0].ticket),
        "the popped screen's send is forgotten"
    );
    r.fulfill(sent[1].ticket, ok()).unwrap();
    assert_eq!(text(&r, "state-1").as_deref(), Some("detect 1"));
}

#[test]
fn a_source_refusal_fails_only_its_owning_screens_receipt() {
    let source = BOOKS.replace("send detected = bad(id)", "show([id])\n    send detected = bad(id)")
        .replace("text notes.lines.join", r#"text (match failure(detected) { case some(e) => e.code + " " + e.message, case none => "ready" }) testId=`failure-${id}`
      text notes.lines.join"#);
    let mut r = Runner::boot(
        contract::compile(&source).unwrap(),
        Books::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for (id, failure) in [("reject", "error seats full"), ("closed", "error closed")] {
        open(&mut r, id);
        press(&mut r, &format!("bad-{id}"));
        assert_eq!(text(&r, "shown").as_deref(), Some(id));
        assert_eq!(text(&r, &format!("failure-{id}")).as_deref(), Some(failure));
        press(&mut r, &format!("now-{id}"));
        assert_eq!(text(&r, &format!("failure-{id}")).as_deref(), Some("ready"));
    }
}

fn id_of<D: DataSource>(r: &Runner<D>, test_id: &str) -> u32 {
    let found = agent::handle(
        r,
        &format!("{{\"op\":\"tree\",\"target\":\"{test_id}\",\"shallow\":true}}"),
    );
    found
        .split("{\"id\":")
        .nth(1)
        .unwrap_or_else(|| panic!("{test_id} in {found}"))
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
