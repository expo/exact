//! What the emitted program says about a child's resources (LLP 1035.005.004).
use exact_runner::{Answer, DataError, DataSource, Store, Value};

/// Answers `subjectWorks(s)` at once with `s` baked, as a build's data does.
struct Built;

impl DataSource for Built {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, name: &str, args: &[Value]) -> Result<Answer, DataError> {
        let arg = args.first().and_then(Value::as_str).unwrap_or("");
        match name {
            "subjectWorks" => Ok(Answer::Now(Value::record(vec![
                Value::Bool(true),
                Value::list(vec![Value::str(&format!("baked {arg}"))]),
            ]))),
            _ => Err(DataError::UnknownSource(name.into())),
        }
    }
}

const SCREENS: &str = "routes nav
  home \"/\"
  discover \"/discover\"

shape Subject
  ok: bool
  works: string[]

component App
  view
    main
      for e in stack(nav) key=e.id
        if e.name == \"discover\"
          Discover()
        else
          text \"Home\"

component Discover
  state subject = \"sf\"
  resource works = subjectWorks(subject) as shape Subject else empty(ok=true)
  action reload
    refresh works
  view
    column
      text works.works.join(\",\") testId=\"works\"
      button press=reload testId=\"reload\"
        text \"Reload\"
";

#[test]
fn grouped_subtitle_can_read_state_owned_by_its_arm() {
    let plan = contract::compile(
        r#"component App
  state show = true
  view
    list appearance="auto"
      section
        column
          text "Title"
          if show
            Detail()

component Detail
  state expanded = true
  view
    if expanded
      text "Supporting text"
"#,
    )
    .unwrap();
    crate::emit::emit(&plan, false, false).expect("the grouped list builds");
}

/// The row is an aggregate at the root; each instance makes its own request,
/// which its action refreshes, and its path follows the runner's.
#[test]
fn each_instance_makes_and_refreshes_its_own_request() {
    let plan = contract::compile(SCREENS).unwrap();
    let works = plan.str(plan.resources[0].name);
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(js.contains(&format!("const r_0=owned({works:?})")), "{js}");
    assert!(
        js.contains(&format!(
            ".r0=own(r_0,$path=>res({works:?},\"subjectWorks\""
        )),
        "{js}"
    );
    assert!(js.contains(r#"R($r["r0"])"#), "{js}");
    assert!(js.contains("\"/0=\"+rowPath($k)"), "{js}");
    assert!(js.contains("+\"/1:0\""), "{js}");
    assert_eq!(js.matches("const $q").count(), 2, "{js}");
}

/// A launch instance takes the bake's answer only while the boot builds it
/// (the runner's settlement.rs `launch`): an instance made later asks with
/// nothing compiled.
#[test]
fn a_launch_instance_takes_the_bakes_answer() {
    // At `/` the boot render makes no Discover, so the bake has nothing for it.
    let plan = contract::bake(contract::compile(SCREENS).unwrap(), Built).unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(!js.contains("baked sf"), "{js}");
    // Where it does, the instance takes the answer while the boot builds it.
    let first = SCREENS.replace(
        "home \"/\"\n  discover \"/discover\"",
        "discover \"/\"\n  home \"/home\"",
    );
    let plan = contract::bake(contract::compile(&first).unwrap(), Built).unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        js.contains("(booting()?[!0,[\"baked sf\"]]:void 0)"),
        "{js}"
    );
    assert!(js.contains("(booting()?[\"sf\"]:void 0)"), "{js}");
}

/// A store reader's instance takes the bake's answer only while the boot
/// makes it, though the emitter writes no arguments for it. The bake gives a
/// store-reading instance no answer (`bake_launch_answers`), so the test marks
/// a baked row a reader.
#[test]
fn a_store_readers_launch_instance_takes_the_bakes_answer_only_at_boot() {
    let first = SCREENS.replace(
        "home \"/\"\n  discover \"/discover\"",
        "discover \"/\"\n  home \"/home\"",
    );
    let mut plan = contract::bake(contract::compile(&first).unwrap(), Built).unwrap();
    let i = (plan.resources.iter())
        .position(|r| plan.str(r.name).starts_with("works"))
        .unwrap();
    plan.resources[i].reader = true;
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        js.contains("(booting()?[!0,[\"baked sf\"]]:void 0)"),
        "{js}"
    );
}

/// A plan with no child's resource emits no instance paths.
#[test]
fn a_plan_without_owners_names_no_instances() {
    let source = SCREENS.replace(
        "  resource works = subjectWorks(subject) as shape Subject else empty(ok=true)\n  action reload\n    refresh works\n",
        "",
    )
    .replace("works.works.join(\",\")", "subject")
    .replace("press=reload ", "");
    let js = crate::emit::emit(&contract::compile(&source).unwrap(), false, false)
        .unwrap()
        .js;
    assert!(!js.contains("$q"), "{js}");
    assert!(!js.contains("rowPath"), "{js}");
}

/// Each book screen owns its detection, whose `then` hands the lines to the
/// root's action, and a queued save that refreshes its own notes
/// (runner/tests/it/owned_sends.rs).
const BOOKS: &str = "routes nav
  list \"/\"
  book \"/book/:id\"

shape Lines
  lines: string[]

component App
  state shown = \"\"
  action showLines(lines: string[])
    shown = lines.join(\",\")
  view
    main
      text shown testId=\"shown\"
      for e in stack(nav) key=e.id
        if e.name == \"book\"
          Book(id=e.params.id, show=showLines)
        else
          text \"List\"

component Book
  props
    id: string
    show: action
  resource notes = notes(id) as shape Lines else empty()
  mutation detected as shape Lines then got
  mutation saved as shape Lines queue refreshes notes
  action detect
    send detected = detect(id)
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
      text (pending(detected) ? \"detecting\" : \"idle\") testId=`state-${id}`
      button press=detect testId=`detect-${id}`
        text \"Detect\"
      button press=save testId=`save-${id}`
        text \"Save\"
";

/// A child's mutation is a stub at the root; each instance makes its own,
/// after its state and requests, its slot, refreshes and `then` its own.
#[test]
fn each_instance_makes_its_own_mutations() {
    let plan = contract::compile(BOOKS).unwrap();
    let detected = plan.str(plan.mutations[0].name);
    let saved = plan.str(plan.mutations[1].name);
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        js.contains(&format!("const m_0=ownedMut({detected:?})")),
        "{js}"
    );
    assert!(
        js.contains(&format!("const m_1=ownedMut({saved:?})")),
        "{js}"
    );
    assert!(!js.contains("m_0.then"), "{js}");
    let (before, _) = js.split_once(".m0=ownMut(").unwrap();
    let owner = before.rsplit_once(';').unwrap().1;
    // Its `then` takes the instance and reads the captured `id` there.
    assert!(
        js.contains(&format!(
            ".m0=ownMut(m_0,n=>mut(n,{owner}[2],[],\"{{[s}}\"),()=>a_2({owner},"
        )),
        "{js}"
    );
    // The queue refreshes the instance's own notes.
    assert!(
        js.contains(&format!(
            "{owner}.m1=ownMut(m_1,n=>mut(n,{owner}[3],[{owner}.r0],\"{{[s}}\",1),0)"
        )),
        "{js}"
    );
    assert!(js.contains(r#"M($r["m0"],"detect""#), "{js}");
    assert!(js.contains(&format!(r#"{owner}["m0"].p()"#)), "{js}");
}

/// A use outside every region answers into a late root slot: its mutation is
/// the root's, declared after that slot, its `then` reading the root.
#[test]
fn a_use_outside_every_region_sends_as_the_root() {
    let source = BOOKS.replace(
        "      for e in stack(nav) key=e.id
        if e.name == \"book\"
          Book(id=e.params.id, show=showLines)
        else
          text \"List\"
",
        "      Book(id=\"9\", show=showLines)\n",
    );
    let plan = contract::compile(&source).unwrap();
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(!js.contains("ownMut"), "{js}");
    let slot = plan.mutations[0].slot.0;
    let declared = js.find(&format!("const s_{slot}=")).expect("the late slot");
    let detected = plan.str(plan.mutations[0].name);
    let made = js.find(&format!("const m_0=mut({detected:?}")).expect(&js);
    assert!(declared < made, "{js}");
    assert!(js.contains("m_0.then=()=>a_2(\"9\");"), "{js}");
}

/// A virtualized list inside a child component reads its enclosing instance's
/// owned resources and mutations in `inputs`, never the throwing root stubs.
#[test]
fn virtualized_list_row_inputs_read_owned_resources_on_their_instance() {
    let single = r#"shape Feed
  prefix: string

component App
  state show = true
  view
    column
      if show
        Screen()

component Screen
  resource feed = getFeed() as shape Feed else empty(prefix="")
  mutation save as shape Feed
  state ids = [1, 2]
  action run
    send save = saveFeed()
  view
    list virtualized=true height=200
      for x in ids key=x
        text `${feed.prefix}:${pending(feed)}:${failed(feed)}:${pending(save)}:${x}`
"#;
    let two_uses = single
        .replace("state show = true", "state a = true\n  state b = false")
        .replace(
            "      if show\n        Screen()",
            "      if a\n        Screen()\n      if b\n        Screen()",
        );
    for source in [single, &two_uses] {
        let plan = contract::compile(source).unwrap();
        for dev_reload in [false, true] {
            let js = crate::emit::emit(&plan, false, dev_reload).unwrap().js;
            let inputs_at = js.find("inputs:()=>[").unwrap_or_else(|| panic!("{js}"));
            let inputs_end = js[inputs_at..].find("],opaque:").unwrap() + inputs_at + 1;
            let inputs = &js[inputs_at..inputs_end];
            assert!(!inputs.contains("r_0"), "must not read root stub: {inputs}");
            assert!(!inputs.contains("r_1"), "must not read root stub: {inputs}");
            assert!(
                inputs.contains(r#"["r0"]()"#)
                    || inputs.contains(".r0()")
                    || inputs.contains("]()"),
                "expected owned resource read: {inputs}"
            );
            assert!(
                inputs.contains(".p()") && inputs.contains(".e()"),
                "expected pending/failed reads on owned resource/mutation: {inputs}"
            );
        }
    }
}
