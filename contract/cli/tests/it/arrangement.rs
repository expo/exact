//! @ref LLP 1188 D3 — the compiler takes an `arrangement` of two children and
//! refuses one it can prove no host can show, leaving the rest to the host.

/// `BODY` in a route, under a `header`.
const SOURCE: &str = r#"routes nav
  home "/"
component App
  state mode = "split"
  state rows = ["a", "b"]
  state wide = true
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` position="absolute" inset=0
          header
            h2 "Song"
BODY
component Pair
  view
    text "One"
    text "Two"
"#;

fn compile(body: &str) -> Result<(), contract::CompileError> {
    let body: String = body.lines().map(|l| format!("          {l}\n")).collect();
    contract::compile(&SOURCE.replace("BODY\n", &body)).map(|_| ())
}

const TWO: &str = "\n  scroll height=\"100%\"\n    text \"Lyrics\"\n  column height=\"100%\"\n    text \"Player\"";

#[test]
fn two_children_are_taken_in_every_form_and_doubt_is_left_to_the_host() {
    let on = |value: &str| format!("row arrangement={value} flex=1 min-height=0{TWO}");
    for value in [
        "\"none\"",
        "\"split\"",
        "\"split horizontal\"",
        "\"split vertical\"",
        "\"overlay\"",
        "\"overlay horizontal\"",
        "\"overlay vertical\"",
        "mode",
        "(wide ? \"split\" : \"none\")",
    ] {
        compile(&on(value)).unwrap_or_else(|e| panic!("{value}: {e:?}"));
    }
    // How many children these have, only the running app says
    for body in [
        "row arrangement=\"split\"\n  text \"One\"\n  if wide\n    text \"Two\"",
        "row arrangement=\"split\"\n  for r in rows key=r\n    text r",
        // A word the app computes may be `none` there
        "scroll height=\"100%\"\n  row arrangement=mode\n    text \"One\"\n    text \"Two\"",
        "row arrangement=\"none\"\n  text \"One\"",
    ] {
        compile(body).unwrap_or_else(|e| panic!("{body}: {e:?}"));
    }
    let error = compile(&on("\"side-by-side\"")).unwrap_err();
    assert!(
        error.id == "lower-attr-value" && error.message.contains("\"overlay\""),
        "{error:?}"
    );
}

#[test]
fn what_the_source_proves_cannot_stand_is_refused_with_what_to_write() {
    let cases = [
        ("row arrangement=\"split\"\n  text \"One\"", "this `row` has one"),
        ("column arrangement=\"overlay\"\n  text \"One\"\n  text \"Two\"\n  text \"Three\"", "this `column` has 3"),
        ("row arrangement=\"split\"", "this `row` has none"),
        ("row id=\"pair\" arrangement=\"split\"\n  text \"One\"\n  text \"Two\"\n  text \"Three\"", "this `row` has 3"),
        ("scroll height=\"100%\"\n  row arrangement=\"split\"\n    text \"One\"\n    text \"Two\"", "inside a `scroll`"),
        ("list height=\"100%\" appearance=\"auto\"\n  section\n    row arrangement=\"split\"\n      text \"One\"\n      text \"Two\"", "inside a `list`"),
        ("row arrangement=\"split\"\n  text \"One\"\n  column arrangement=\"overlay\"\n    text \"Two\"\n    text \"Three\"", "inside another `arrangement`"),
        ("scroll height=\"100%\" arrangement=\"split\"\n  text \"One\"\n  text \"Two\"", "is on a `scroll`"),
    ];
    for (body, says) in cases {
        let error = compile(body).expect_err(body);
        assert!(
            error.id == "lower-arrangement" && error.message.contains(says),
            "{body}: {error:?}"
        );
    }
    // A sheet is shown over the scroller it is written in, and what a portal moves stands where it goes
    compile("scroll height=\"100%\"\n  dialog\n    row arrangement=\"split\"\n      text \"One\"\n      text \"Two\"").unwrap();
    compile("column\n  scroll height=\"100%\"\n    column\n      row portal=\"dock\" arrangement=\"split\"\n        text \"One\"\n        text \"Two\"\n  row id=\"dock\"").unwrap();
    // A portal can bring the second
    compile("column\n  row id=\"dock\" arrangement=\"split\"\n    text \"One\"\n  column\n    box portal=\"dock\"\n      text \"Two\"").unwrap();
    // A tablist of text is a segmented control, a pane's like any other
    compile("row arrangement=\"split\"\n  text \"One\"\n  row role=\"tablist\"\n    button role=\"tab\"\n      text \"Two\"").unwrap();
    // A component's own two are its box's children
    compile("row arrangement=\"split\"\n  Pair()").unwrap();
}
