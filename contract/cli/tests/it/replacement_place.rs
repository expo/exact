//! A release plan keeps the source boundaries needed to identify its live
//! views across edits (LLP 1029.000.002 D1).
use exact_kernel::Kernel;
use exact_plan::Plan;
use exact_runner::{agent, Runner};
use serde_json::{json, Value};

fn boot(source: &str) -> Runner<()> {
    let bytes = contract::compile(source).unwrap().encode();
    Runner::boot(
        Plan::decode(&bytes).unwrap(),
        (),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn places(runner: &Runner<()>, target: &str) -> Vec<Value> {
    let kernel = runner.kernel();
    kernel
        .find_by_test_id(target)
        .into_iter()
        .map(|key| {
            let id = kernel.node_by_key(key).unwrap().id;
            let detail: Value = serde_json::from_str(&agent::node(runner, id)).unwrap();
            detail["place"].clone()
        })
        .collect()
}

#[test]
fn editor_places_follow_source_siblings_including_hidden_branches_and_fills() {
    let source = r#"component App
  state field = "compose"
  view
    main
      if false
        input aria-label="Hidden"
      input aria-label="Draft" id=field testId="named"
      input aria-label="Later" testId="later"
      Twice()
        Note()
component Twice
  slot
  view
    column
      children
      children
component Note
  view
    input aria-label="Note" testId="note"
"#;
    let app = boot(source);
    let main = [
        json!({"component":"App","index":0}),
        json!({"element":"main","index":0}),
    ];
    let at = |tail: Value| json!([main[0], main[1], tail]);
    assert_eq!(places(&app, "named"), [at(json!({"id":"compose"}))]);
    assert_eq!(
        places(&app, "later"),
        [at(json!({"element":"input","index":2}))]
    );
    let notes = places(&app, "note");
    let note = |index| json!([main[0],main[1],{"component":"Twice","index":0},{"element":"column","index":0},{"children":index},{"component":"Note","index":0},{"element":"input","index":0}]);
    assert_eq!(notes, [note(0), note(1)]);
    let inserted = source.replace(
        "      Twice()",
        "      text \"label\"\n      Note()\n      Twice()",
    );
    let app = boot(&inserted);
    assert_eq!(places(&app, "note")[1..], notes);
    assert_eq!(
        places(&app, "later"),
        [at(json!({"element":"input","index":2}))]
    );
    let hidden_duplicate = source.replace("id=field", "id=\"compose\"").replace(
        "aria-label=\"Hidden\"",
        "aria-label=\"Hidden\" id=\"compose\"",
    );
    assert_eq!(
        places(&boot(&hidden_duplicate), "named"),
        [at(json!({"element":"input","index":1}))]
    );
    let renamed = boot(&source.replace("Note", "Other"));
    assert_ne!(places(&renamed, "note"), notes);
    let duplicate = boot(&source.replace("testId=\"later\"", "id=field testId=\"later\""));
    assert_eq!(
        places(&duplicate, "named"),
        [at(json!({"element":"input","index":1}))]
    );
    assert_eq!(
        places(&duplicate, "later"),
        [at(json!({"element":"input","index":2}))]
    );
}

#[test]
fn row_places_include_the_loop_key_and_repeat_in_an_ordinary_or_virtualized_list() {
    let source = r#"component App
  view
    main
      for x in ["a","b","a"] key=x
        text x testId="row"
      for y in ["a"] key=y
        text y testId="row"
      list virtualized=true appearance="none" width=300 height=200 estimated-item-height=40
        for z in ["a","b"] key=z
          text z height=40 testId="cell"
"#;
    let app = boot(source);
    let rows = places(&app, "row");
    let keys = |rows: &[Value]| {
        rows.iter()
            .map(|path| {
                let path = path.as_array().unwrap();
                json!([{"for": path[path.len() - 3]["for"]}, path[path.len() - 2]])
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        keys(&rows),
        [
            json!([{"for":0},{"key":"a","repeat":0}]),
            json!([{"for":0},{"key":"b","repeat":0}]),
            json!([{"for":0},{"key":"a","repeat":1}]),
            json!([{"for":1},{"key":"a","repeat":0}]),
        ]
    );
    let cells = places(&app, "cell");
    assert_eq!(keys(&cells), keys(&rows[..2]));
    assert_ne!(cells[0], rows[0]);
    let changed = boot(&source.replace("[\"a\",\"b\",\"a\"]", "[\"b\",\"a\",\"a\"]"));
    assert_eq!(
        places(&changed, "row"),
        [
            rows[1].clone(),
            rows[0].clone(),
            rows[2].clone(),
            rows[3].clone()
        ]
    );
}

#[test]
fn a_new_view_cannot_receive_an_id_retired_in_an_earlier_generation() {
    let mut old = boot(
        r#"component App
  state shown = true
  action hide
    shown = false
  view
    main
      button "Hide" press=hide testId="hide"
      if shown
        text "Retired" testId="retired"
"#,
    );
    let id = |r: &Runner<()>, name: &str| {
        r.kernel()
            .node_by_key(r.kernel().find_by_test_id(name)[0])
            .unwrap()
            .id
    };
    let retired = id(&old, "retired");
    let hide = id(&old, "hide");
    old.dispatch(hide, exact_runner::Event::Press).unwrap();
    assert!(old.kernel().node(retired).is_none());
    let plan =
        contract::compile("component App\n  view\n    main\n      input testId=\"new\"\n").unwrap();
    for carry in [old.carry(), old.reboot_sequence()] {
        let mut exhausted = carry.clone();
        exhausted.last_view_id = u32::MAX;
        assert!(Runner::boot_carrying(
            plan.clone(),
            (),
            Kernel::with_monospace(),
            &exhausted,
            Default::default(),
            "/"
        )
        .is_err());
        let new = Runner::boot_carrying(
            plan.clone(),
            (),
            Kernel::with_monospace(),
            &carry,
            Default::default(),
            "/",
        )
        .unwrap();
        assert!(id(&new, "new") > retired);
    }
}

#[test]
fn a_replacement_keeps_a_compatible_view_and_requires_its_parent_to_match() {
    let source = r#"component App
  state draft = ""
  action edit(value: string)
    draft = value
  action flip
    draft = "edited"
  view
    main
      column id="group" testId="group"
        input id="draft" value=draft input=edit testId="editor"
      text "Before" testId="label"
"#;
    let old = boot(source);
    let id = |app: &Runner<()>, name| {
        let kernel = app.kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(name)[0])
            .unwrap()
            .id
    };
    let carry = old.carry();
    let replace = |source: &str| {
        let plan = contract::compile(source).unwrap();
        Runner::boot_carrying(
            plan,
            (),
            Kernel::with_monospace(),
            &carry,
            Default::default(),
            "/",
        )
        .unwrap()
    };
    let inserted = source
        .replace(
            "      column",
            "      text \"New\" id=\"new\" testId=\"inserted\"\n      column",
        )
        .replace("Before", "After")
        .replace("action edit(", "action edited(")
        .replace("input=edit ", "input=edited ");
    let new = replace(&inserted);
    assert!(id(&new, "inserted") > carry.last_view_id);
    for kept in ["group", "editor"] {
        assert_eq!(id(&old, kept), id(&new, kept), "{kept}");
    }
    for replaced in [
        inserted.replace("column id", "column press=flip id"),
        inserted.replace("input=edited ", ""),
        inserted.replace("input id", "textarea id"),
        inserted.replace("input id", "input type=\"password\" id"),
    ] {
        let new = replace(&replaced);
        assert!(id(&new, "editor") > carry.last_view_id);
    }
}

#[test]
fn reload_moves_keyed_row_views_with_their_data() {
    let source = r#"component App
  view
    main
      for x in ["a","b","a"] key=x
        input id="editor" testId=x
"#;
    let before = boot(source);
    let plan = contract::compile(&source.replace(r#"["a","b","a"]"#, r#"["a","a","b"]"#)).unwrap();
    let after = Runner::boot_carrying(
        plan,
        (),
        Kernel::with_monospace(),
        &before.carry(),
        Default::default(),
        "/",
    )
    .unwrap();
    let children = |app: &Runner<()>| app.kernel().node(app.roots()[0]).unwrap().children();
    let rows = children(&before);
    assert_eq!(rows.len(), 3);
    assert_eq!(children(&after), [rows[0], rows[2], rows[1]]);
}

#[test]
fn a_new_key_expression_starts_new_rows_even_when_the_keys_are_equal() {
    let source = r#"component App
  view
    main
      Items()
component Items
  view
    column
      for row in ["a"] key=row
        input testId="row"
component Other
  view
    text "Unrelated"
"#;
    let old = boot(source);
    let carry = old.carry();
    let inserted = source.replace("      Items()", "      Other()\n      Items()");
    for (key, kept) in [("(row)", true), (r#""a""#, false)] {
        let plan = contract::compile(&inserted.replace("key=row", &format!("key={key}"))).unwrap();
        let candidate = Runner::boot_carrying(
            plan,
            (),
            Kernel::with_monospace(),
            &carry,
            Default::default(),
            "/",
        )
        .unwrap();
        let current = candidate.kernel().find_by_test_id("row")[0];
        assert_eq!(
            candidate.kernel().node_by_key(current).unwrap().id <= carry.last_view_id,
            kept
        );
        assert_eq!(places(&old, "row") == places(&candidate, "row"), kept);
    }
}
