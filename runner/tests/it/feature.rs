//! A self-contained feature as a component (LLP 1035.005.004): each instance
//! owns its resources, mutation and task under names the root also uses, and a
//! tab's screen asks only while its gate holds.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    agent, Answer, DataError, DataSource, Event, Outcome, Request, Response, Runner, Store,
};

/// Every source answers later; its answer's one line names what was asked.
#[derive(Default)]
struct Echo {
    asks: Vec<String>,
}

fn arg(args: &[Value]) -> String {
    args.first()
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

impl DataSource for Echo {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, name: &str, args: &[Value]) -> Result<Answer, DataError> {
        let a = arg(args);
        self.asks.push(format!("{name}({a})"));
        Ok(Answer::Later(Request::get(&format!(
            "https://echo.test/{name}/{a}"
        ))))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        name: &str,
        args: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::record(vec![Value::list(vec![
            Value::str(&format!("{name}:{}", arg(args))),
        ])])))
    }
}

fn boot(source: &str) -> Runner<Echo> {
    let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
    Runner::boot(
        plan,
        Echo::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn text<D: DataSource>(r: &Runner<D>, test_id: &str) -> Option<String> {
    let tree = agent::tree(r);
    let key = format!("\"testId\":\"{test_id}\"");
    let node = tree.split("{\"id\":").find(|n| n.contains(&key))?;
    let at = node.find("\"text\":\"")? + 8;
    Some(node[at..].split('"').next().unwrap().to_string())
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

/// What was asked since the last call, sorted.
fn asked(r: &mut Runner<Echo>) -> Vec<String> {
    let mut asks = std::mem::take(&mut r.data().asks);
    asks.sort();
    asks
}

fn land<D: DataSource>(r: &mut Runner<D>) -> usize {
    let requests = r.take_requests();
    for q in &requests {
        let ok = Outcome::Response(Response {
            status: 200,
            headers: vec![],
            body: b"{}".to_vec(),
        });
        r.fulfill(q.ticket, ok).unwrap();
    }
    requests.len()
}

/// A canvas feature with a feed, a graph, a save and an autosave, used outside
/// every region and in a stack entry; the root declares a `selection`, a
/// `feed`, a `saved` and a `save` of its own.
const FEATURE: &str = "routes nav
  home \"/\"
  canvas \"/canvas/:id\"

shape Lines
  lines: string[]

component App
  state selection = \"root-sel\"
  resource feed = load(\"root\") as shape Lines else empty()
  mutation saved as shape Lines
  action open(id: string)
    nav = push(nav, path(\"canvas\", id))
  action back
    nav = back(nav)
  action save
    send saved = store(\"root\")
  view
    main
      text `${selection} ${feed.lines.join(\",\")}${pending(saved) ? \" saving\" : \"\"}` testId=\"root\"
      Canvas(doc=\"main\")
      for e in stack(nav) key=e.id
        if e.name == \"canvas\"
          Canvas(doc=e.params.id)

component Canvas
  props
    doc: string
  state selection = \"\"
  state dirty = false
  resource feed = load(`feed-${doc}`) as shape Lines else empty()
  resource graph = load(`graph-${doc}`) as shape Lines else empty()
  mutation saved as shape Lines refreshes graph
  task autosave when dirty
    after(500, save)
  action edit
    selection = doc
    dirty = true
  action save
    dirty = false
    send saved = store(doc)
  view
    column
      text `${doc}|${selection}|${feed.lines.join(\",\")}|${graph.lines.join(\",\")}|${pending(saved) ? \"saving\" : \"\"}` testId=`canvas-${doc}`
      button press=edit testId=`edit-${doc}`
        text \"Edit\"
";

#[test]
fn a_feature_owns_its_requests_apart_from_the_root_and_its_other_instance() {
    let mut r = boot(FEATURE);
    // The root's feed and the outside-every-region instance's two resources.
    assert_eq!(
        asked(&mut r),
        ["load(feed-main)", "load(graph-main)", "load(root)"]
    );
    assert_eq!(land(&mut r), 3);
    assert_eq!(text(&r, "root").as_deref(), Some("root-sel load:root"));
    let main = "main||load:feed-main|load:graph-main|";
    assert_eq!(text(&r, "canvas-main").as_deref(), Some(main));
    // A stack entry's instance asks for its own; nothing else asks.
    r.act("open", vec![Value::str("x")]).unwrap();
    assert_eq!(asked(&mut r), ["load(feed-x)", "load(graph-x)"]);
    land(&mut r);
    // Each instance's edit arms its own autosave, which sends its own
    // document, pends its own `saved` and refreshes its own graph; the root's
    // `selection` and `saved` are untouched.
    for (doc, other, at) in [("main", "x", 500.), ("x", "main", 1000.)] {
        let other_text = text(&r, &format!("canvas-{other}"));
        press(&mut r, &format!("edit-{doc}"));
        assert_eq!(r.timer_due_ms(), Some(at));
        r.advance(at).unwrap();
        assert_eq!(asked(&mut r), [format!("store({doc})")]);
        let saving = format!("{doc}|{doc}|load:feed-{doc}|load:graph-{doc}|saving");
        assert_eq!(text(&r, &format!("canvas-{doc}")), Some(saving));
        assert_eq!(text(&r, &format!("canvas-{other}")), other_text);
        assert_eq!(text(&r, "root").as_deref(), Some("root-sel load:root"));
        assert_eq!(land(&mut r), 1);
        assert_eq!(asked(&mut r), [format!("load(graph-{doc})")]);
        land(&mut r);
        assert_eq!(text(&r, &format!("canvas-{other}")), other_text);
        assert_eq!(r.timer_due_ms(), None);
    }
    // Popping x with its autosave armed sends nothing and drops the timer.
    press(&mut r, "edit-x");
    assert_eq!(r.timer_due_ms(), Some(1500.));
    r.act("back", vec![]).unwrap();
    assert_eq!(text(&r, "canvas-x"), None);
    assert_eq!(r.timer_due_ms(), None);
    r.advance(3000.).unwrap();
    assert!(asked(&mut r).is_empty());
    // The root's own `save` sends and pends the root's `saved`.
    r.act("save", vec![]).unwrap();
    assert_eq!(asked(&mut r), ["store(root)"]);
    assert_eq!(
        text(&r, "root").as_deref(),
        Some("root-sel load:root saving")
    );
    assert!(text(&r, "canvas-main").unwrap().ends_with('|'));
}

/// Each tab's screen, a row of `for e in t.stack`, owns a resource keyed on
/// the signed-in user and gated on its tab showing; the feed's `live` is gated
/// on its entry being on top, so a pushed post that covers the mounted feed
/// idles it.
const TABS: &str = "routes nav
  tab feed \"/\"
    post \"/post/:id\"
  tab profile \"/profile\"

shape Lines
  lines: string[]

component App
  state user = \"ann\"
  action pick(name: string)
    nav = select(nav, name)
  action open(id: string)
    nav = push(nav, path(\"post\", id))
  action back
    nav = back(nav)
  action signIn(u: string)
    user = u
  view
    main navigationKey=`${top(nav).id}` navigationBack=\"back\"
      column
        for t in nav.tabs key=t.name
          column role=\"tabpanel\" id=`panel-${t.name}`
            for e in t.stack key=e.id
              column navigationKey=`${e.id}`
                if e.name == \"profile\"
                  Profile(user=user, shown=(nav.tab == t.name))
                else if e.name == \"post\"
                  text `post ${e.params.id}` testId=\"post\"
                else
                  Feed(user=user, shown=(nav.tab == t.name), top=(top(nav).id == e.id))
      row role=\"tablist\"
        button press=pick(\"feed\") role=\"tab\" aria-controls=\"panel-feed\" aria-selected=(nav.tab == \"feed\")
          text \"Feed\"
        button press=pick(\"profile\") role=\"tab\" aria-controls=\"panel-profile\" aria-selected=(nav.tab == \"profile\")
          text \"Profile\"

component Feed
  props
    user: string
    shown: bool
    top: bool
  resource posts = load(`posts-${user}`) when shown as shape Lines else empty()
  resource live = load(`live-${user}`) when top as shape Lines else empty()
  view
    text `${posts.lines.join(\",\")}|${live.lines.join(\",\")}` testId=\"feed\"

component Profile
  props
    user: string
    shown: bool
  resource me = load(`me-${user}`) when shown as shape Lines else empty()
  view
    text me.lines.join(\",\") testId=\"me\"
";

#[test]
fn a_hidden_tab_and_a_covered_screen_ask_only_through_their_gates() {
    let mut r = boot(TABS);
    // Launch: only the showing tab's screen asks; the profile tab is unbuilt.
    assert_eq!(asked(&mut r), ["load(live-ann)", "load(posts-ann)"]);
    land(&mut r);
    assert_eq!(text(&r, "me"), None);
    // Built ahead (a prerendered page, iOS's More), the gated tab asks nothing.
    let panels = r.unopened_tabs().to_vec();
    assert_eq!(panels.len(), 1);
    r.build_tabs(&panels).unwrap();
    assert_eq!(text(&r, "me").as_deref(), Some(""));
    assert!(asked(&mut r).is_empty());
    // A pushed post covers the feed, which stays mounted. A sign-in then asks
    // for `posts`, gated on its tab showing, but not `live`, gated on its
    // entry being on top.
    r.act("open", vec![Value::str("7")]).unwrap();
    assert_eq!(text(&r, "post").as_deref(), Some("post 7"));
    assert!(asked(&mut r).is_empty());
    r.act("signIn", vec![Value::str("bob")]).unwrap();
    assert_eq!(asked(&mut r), ["load(posts-bob)"]);
    land(&mut r);
    assert_eq!(
        text(&r, "feed").as_deref(),
        Some("load:posts-bob|load:live-ann")
    );
    // On the profile tab, a sign-in asks for the profile only.
    r.act("pick", vec![Value::str("profile")]).unwrap();
    assert_eq!(asked(&mut r), ["load(me-bob)"]);
    land(&mut r);
    r.act("signIn", vec![Value::str("cat")]).unwrap();
    assert_eq!(asked(&mut r), ["load(me-cat)"]);
    land(&mut r);
    // Back to the feed, then off the post: each gate opens and asks.
    r.act("pick", vec![Value::str("feed")]).unwrap();
    assert_eq!(asked(&mut r), ["load(posts-cat)"]);
    land(&mut r);
    r.act("back", vec![]).unwrap();
    assert_eq!(asked(&mut r), ["load(live-cat)"]);
}
