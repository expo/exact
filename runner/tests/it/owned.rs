//! A child's requests (LLP 1035.005.004): each instance of a component that
//! declares a resource owns its own request, asked after the render that
//! makes the instance and let go when the render destroys it.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    agent, Answer, DataError, DataSource, Event, Outcome, Request, Response, Runner, Store,
};

/// `subjectWorks(s)` answers later, with `s` as its one work; `isbn(x)`
/// answers at once.
#[derive(Default)]
struct Works {
    asks: Vec<String>,
}

impl DataSource for Works {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, name: &str, args: &[Value]) -> Result<Answer, DataError> {
        let arg = args
            .first()
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.asks.push(format!("{name}({arg})"));
        match name {
            "subjectWorks" => Ok(Answer::Later(Request::get(&format!(
                "https://works.test/{arg}"
            )))),
            "now" => Ok(Answer::Now(Value::str(&format!("now {arg}")))),
            "emptyWorks" => Ok(Answer::Now(Value::record(vec![
                Value::Bool(true),
                Value::list(vec![Value::str("ph")]),
            ]))),
            _ => Err(DataError::UnknownSource(name.into())),
        }
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        let arg = args.first().and_then(Value::as_str).unwrap_or("");
        Ok(Answer::Now(Value::record(vec![
            Value::Bool(true),
            Value::list(vec![Value::str(arg)]),
        ])))
    }
}

/// Shelf's Discover tab, reduced: the screen owns its subject, its works and
/// the actions that change them; `works` asks only while its tab shows.
const DISCOVER: &str = "routes nav
  tab home \"/\"
  tab discover \"/discover\"

shape Subject
  ok: bool
  works: string[]

component App
  action pick(name: string)
    nav = select(nav, name)
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\"
      column
        for t in nav.tabs key=t.name
          column role=\"tabpanel\" id=`panel-${t.name}` testId=`panel-${t.name}`
            for e in t.stack key=e.id
              column navigationKey=`${e.id}`
                if e.name == \"discover\"
                  Discover(shown=(nav.tab == t.name))
                else
                  text \"Home\" testId=\"home\"
      row role=\"tablist\"
        button press=pick(\"home\") testId=\"tab-home\" role=\"tab\" aria-controls=\"panel-home\" aria-selected=(nav.tab == \"home\")
          text \"Home\"
        button press=pick(\"discover\") testId=\"tab-discover\" role=\"tab\" aria-controls=\"panel-discover\" aria-selected=(nav.tab == \"discover\")
          text \"Discover\"

component Discover
  props
    shown: bool
  state subject = \"sf\"
  resource works = subjectWorks(subject) when shown as shape Subject else empty(ok=true)
  action choose(s: string)
    subject = s
  action reload
    refresh works
  view
    column
      text (pending(works) ? `loading ${works.works.join(\",\")}` : works.works.join(\",\")) testId=\"works\"
      button press=choose(\"fantasy\") testId=\"fantasy\"
        text \"Fantasy\"
      button press=reload testId=\"reload\"
        text \"Reload\"
";

fn boot<D: DataSource>(source: &str, data: D, url: &str) -> Runner<D> {
    let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
    Runner::boot(
        plan,
        data,
        Kernel::with_monospace(),
        Default::default(),
        url,
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

/// Lands every request out; how many there were.
fn land<D: DataSource>(r: &mut Runner<D>) -> usize {
    let requests = r.take_requests();
    for q in &requests {
        r.fulfill(q.ticket, ok()).unwrap();
    }
    requests.len()
}

#[test]
fn a_tab_screen_asks_while_shown_and_keeps_its_answer_while_hidden() {
    let mut r = boot(DISCOVER, Works::default(), "/");
    // The Discover tab is not built: nothing asked.
    assert!(r.data().asks.is_empty());
    assert_eq!(text(&r, "works"), None);

    // Its first selection makes the screen, which asks: pending, with the
    // placeholder showing, in the commit that made it.
    r.act("pick", vec![Value::str("discover")]).unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(sf)"]);
    assert_eq!(text(&r, "works").as_deref(), Some("loading "));
    assert_eq!(land(&mut r), 1);
    assert_eq!(text(&r, "works").as_deref(), Some("sf"));

    // Hidden, it asks nothing and keeps what it showed.
    r.act("pick", vec![Value::str("home")]).unwrap();
    assert_eq!(r.data().asks.len(), 1);
    assert_eq!(text(&r, "works").as_deref(), Some("sf"));
    assert_eq!(land(&mut r), 0);

    // Shown again, its gate opens: it asks again, showing the last answer.
    r.act("pick", vec![Value::str("discover")]).unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(sf)", "subjectWorks(sf)"]);
    assert_eq!(text(&r, "works").as_deref(), Some("loading sf"));
    land(&mut r);

    // Its own state changes its arguments; its own refresh asks again.
    press(&mut r, "fantasy");
    assert_eq!(r.data().asks.last().unwrap(), "subjectWorks(fantasy)");
    land(&mut r);
    assert_eq!(text(&r, "works").as_deref(), Some("fantasy"));
    press(&mut r, "reload");
    assert_eq!(r.data().asks.len(), 4);
    assert_eq!(text(&r, "works").as_deref(), Some("loading fantasy"));
    land(&mut r);
    assert_eq!(text(&r, "works").as_deref(), Some("fantasy"));
}

/// A stack of book screens, each owning its book's request; the list screen
/// owns none.
const BOOKS: &str = "routes nav
  list \"/\"
  book \"/book/:id\"

shape Subject
  ok: bool
  works: string[]

component App
  action open(id: string)
    nav = push(nav, path(\"book\", id))
  action swap(id: string)
    nav = replace(nav, path(\"book\", id))
  action back
    nav = back(nav)
  view
    main
      for e in stack(nav) key=e.id
        if e.name == \"book\"
          Book(id=e.params.id)
        else
          text \"List\" testId=\"list\"

component Book
  props
    id: string
  resource works = subjectWorks(id) as shape Subject else empty()
  action reload
    refresh works
  view
    column
      text `${id}: ${works.works.join(\",\")}` testId=`book-${id}`
      button press=reload testId=`reload-${id}`
        text \"Reload\"
";

#[test]
fn a_popped_screen_lets_its_request_go_and_its_late_answer_changes_nothing() {
    let mut r = boot(BOOKS, Works::default(), "/");
    r.act("open", vec![Value::str("1")]).unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(1)"]);
    let first = r.take_requests();
    assert_eq!(first.len(), 1);
    assert!(r.holds(first[0].ticket));
    // Back destroys the screen: its ticket is forgotten.
    r.act("back", vec![]).unwrap();
    assert!(!r.holds(first[0].ticket));
    assert_eq!(r.fulfill(first[0].ticket, ok()).unwrap(), None);
    // Opening the same book again is a new instance with a new ticket: the
    // predecessor's reply, even under the same instance path, is dropped.
    r.act("open", vec![Value::str("1")]).unwrap();
    assert_eq!(r.data().asks.len(), 2);
    assert_eq!(text(&r, "book-1").as_deref(), Some("1: "));
    assert_eq!(r.fulfill(first[0].ticket, ok()).unwrap(), None);
    assert_eq!(text(&r, "book-1").as_deref(), Some("1: "));
    assert_eq!(land(&mut r), 1);
    assert_eq!(text(&r, "book-1").as_deref(), Some("1: 1"));
}

#[test]
fn replace_keeps_the_screen_and_asks_with_the_new_parameters() {
    let mut r = boot(BOOKS, Works::default(), "/");
    r.act("open", vec![Value::str("1")]).unwrap();
    land(&mut r);
    r.act("swap", vec![Value::str("2")]).unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(1)", "subjectWorks(2)"]);
    // The same instance: it shows its last answer until the new one lands.
    assert_eq!(text(&r, "book-2").as_deref(), Some("2: 1"));
    land(&mut r);
    assert_eq!(text(&r, "book-2").as_deref(), Some("2: 2"));
}

#[test]
fn two_instances_ask_and_refresh_apart() {
    let mut r = boot(BOOKS, Works::default(), "/");
    for id in ["1", "2", "1"] {
        r.act("open", vec![Value::str(id)]).unwrap();
    }
    // Equal arguments, two instances: two asks, two tickets.
    let asks = ["subjectWorks(1)", "subjectWorks(2)", "subjectWorks(1)"];
    assert_eq!(r.data().asks, asks);
    assert_eq!(land(&mut r), 3);
    // The covered screen's reload reaches only it.
    let views = agent::handle(&r, "{\"op\":\"tree\",\"target\":\"reload-1\"}");
    let ids: Vec<u32> = views
        .split("{\"id\":")
        .skip(1)
        .map(|n| n.split(',').next().unwrap().parse().unwrap())
        .collect();
    assert_eq!(ids.len(), 2, "{views}");
    r.dispatch(ids[0], Event::Press).unwrap();
    assert_eq!(r.data().asks.len(), 4);
    assert_eq!(r.pending().len(), 1);
}

#[test]
fn an_unopened_tab_asks_nothing_and_a_built_one_asks_at_that_render() {
    let mut r = boot(&DISCOVER.replace("when shown ", ""), Works::default(), "/");
    assert!(r.data().asks.is_empty());
    let panels = r.unopened_tabs().to_vec();
    assert_eq!(panels.len(), 1);
    r.build_tabs(&panels).unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(sf)"]);
    assert_eq!(r.pending().len(), 1);
}

/// `now(x)` answers at once; `again(x)` reads that answer.
const CHAIN: &str = "routes nav
  home \"/\"
  scan \"/scan\"

component App
  action scan
    nav = push(nav, \"/scan\")
  view
    main
      for e in stack(nav) key=e.id
        if e.name == \"scan\"
          Scan()
        else
          text \"Home\"

component Scan
  state code = \"978\"
  resource scanned = now(code) as shape string
  derive isbn = scanned == \"\" ? \"\" : `${scanned}!`
  resource lookup = now(isbn) as shape string
  view
    text `${scanned} / ${lookup}` testId=\"scan\"
";

#[test]
fn an_immediate_answer_lands_in_a_later_render_and_what_reads_it_asks_then() {
    let mut r = boot(CHAIN, Works::default(), "/");
    r.act("scan", vec![]).unwrap();
    // `scanned` asks after the render that made the screen; `lookup` first
    // on the placeholder's `isbn`, then on the answer's.
    assert_eq!(r.data().asks, ["now(978)", "now()", "now(now 978!)"]);
    assert_eq!(text(&r, "scan").as_deref(), Some("now 978 / now now 978!"));
}

/// A child used outside every region: one instance, as long-lived as the root.
#[test]
fn a_use_outside_every_region_asks_with_the_root() {
    let src = "component App
  view
    main
      Feed(topic=\"t\")

component Feed
  props
    topic: string
  state n = 1
  resource got = now(`${topic}${n}`) as shape string
  action more
    n = n + 1
  view
    column
      text got testId=\"got\"
      button press=more testId=\"more\"
        text \"More\"
";
    let mut r = boot(src, Works::default(), "/");
    assert_eq!(r.data().asks, ["now(t1)"]);
    assert_eq!(text(&r, "got").as_deref(), Some("now t1"));
    press(&mut r, "more");
    assert_eq!(r.data().asks, ["now(t1)", "now(t2)"]);
    assert_eq!(text(&r, "got").as_deref(), Some("now t2"));
}

/// The build's data answers at once; a launch's module is not loaded yet.
struct Built {
    loaded: std::rc::Rc<std::cell::Cell<bool>>,
    asks: Vec<String>,
}

impl DataSource for Built {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, name: &str, args: &[Value]) -> Result<Answer, DataError> {
        if !self.loaded.get() {
            return Err(DataError::Unavailable("not loaded".into()));
        }
        let arg = args
            .first()
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        self.asks.push(format!("{name}({arg})"));
        Ok(Answer::Now(Value::record(vec![
            Value::Bool(true),
            Value::list(vec![Value::str(&format!("baked {arg}"))]),
        ])))
    }
    fn ready(&self) -> bool {
        self.loaded.get()
    }
}

#[test]
fn a_cold_launch_shows_the_baked_answer_of_its_first_screens_request() {
    let built = Built {
        loaded: std::rc::Rc::new(std::cell::Cell::new(true)),
        asks: Vec::new(),
    };
    // Discover is the launch tab, so the bake's first frame makes it.
    let first = DISCOVER.replace(
        "  tab home \"/\"\n  tab discover \"/discover\"",
        "  tab discover \"/\"\n  tab home \"/home\"",
    );
    assert_ne!(first, DISCOVER);
    let plan = contract::bake(contract::compile(&first).unwrap(), built).unwrap();
    let loaded = std::rc::Rc::new(std::cell::Cell::new(false));
    let launch = Built {
        loaded: loaded.clone(),
        asks: Vec::new(),
    };
    let url = "/";
    let mut r = Runner::boot(
        plan,
        launch,
        Kernel::with_monospace(),
        Default::default(),
        url,
    )
    .unwrap();
    // No kept answer, the module not loaded: the first frame shows the
    // bake's, waiting for the module.
    assert_eq!(text(&r, "works").as_deref(), Some("baked sf"));
    assert!(r.take_requests().is_empty());
    loaded.set(true);
    r.data_ready().unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(sf)"]);
    assert_eq!(text(&r, "works").as_deref(), Some("baked sf"));
}

/// An `else source()` placeholder answers in the render that makes the
/// instance, before its view reads it.
#[test]
fn an_owned_requests_source_placeholder_shows_in_the_making_render() {
    let src = DISCOVER.replace("else empty(ok=true)", "else emptyWorks()");
    let mut r = boot(&src, Works::default(), "/discover");
    assert_eq!(text(&r, "works").as_deref(), Some("loading ph"));
    assert_eq!(land(&mut r), 1);
    assert_eq!(text(&r, "works").as_deref(), Some("sf"));
}

/// A boot from a rendered page gives the first frame's instances the page's
/// answers by their arguments, asking nothing (LLP 1035.005.004 D8); an
/// instance made later asks.
#[test]
fn a_rendered_pages_instances_answer_the_boots_first_frame() {
    let plan = contract::compile(BOOKS).unwrap();
    let mut rendered = Runner::boot(
        plan.clone(),
        Works::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/book/1",
    )
    .unwrap();
    assert_eq!(land(&mut rendered), 1);
    let checkpoint = rendered.document_checkpoint("/book/1");
    assert_eq!(checkpoint.answers.len(), 1, "{:?}", checkpoint.answers);
    let mut r = Runner::boot_checkpoint(
        plan,
        Works::default(),
        Kernel::with_monospace(),
        &checkpoint,
        Vec::new(),
        Default::default(),
        Default::default(),
        "/book/1",
    )
    .unwrap();
    assert_eq!(text(&r, "book-1").as_deref(), Some("1: 1"));
    assert!(r.data().asks.is_empty(), "{:?}", r.data().asks);
    r.act("open", vec![Value::str("2")]).unwrap();
    assert_eq!(r.data().asks, ["subjectWorks(2)"]);
}

/// The agent's `state` names a child's resources and lists each instance's
/// under its path; `perf` counts every instance's asks under the declared
/// name (LLP 1035.005.004 D8).
#[test]
fn the_agent_lists_each_instances_requests_by_its_path() {
    let mut r = boot(BOOKS, Works::default(), "/");
    let state = agent::state(&r);
    assert!(state.contains("\"owned\":[\"works\"]"), "{state}");
    assert!(!state.contains("\"instances\""), "{state}");
    r.measure(true, false);
    for id in ["1", "2"] {
        r.act("open", vec![Value::str(id)]).unwrap();
    }
    land(&mut r);
    let state = agent::state(&r);
    let instances = &state[state.find("\"instances\":").expect("instances")..];
    assert_eq!(instances.matches("\"works\":{").count(), 2, "{instances}");
    let perf = agent::handle(&r, "{\"op\":\"perf\"}");
    assert!(perf.contains("{\"name\":\"works\",\"asked\":2}"), "{perf}");
}
