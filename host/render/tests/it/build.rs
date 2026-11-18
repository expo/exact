//! The render entry's build (`<app>-render --build`, LLP 1048.000 D7).

use exact_plan::Value;
use exact_runner::{Answer, DataError, DataSource, Store};
use std::process::ExitCode;

/// A source whose note for the routes `drawn` and `notfound` draws randomness, as `crypto.getRandomValues` does.
#[derive(Default)]
struct Notes;

impl DataSource for Notes {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if source != "note" {
            return Err(DataError::UnknownSource(source.into()));
        }
        if matches!(
            args.first().and_then(Value::as_str),
            Some("drawn" | "notfound")
        ) {
            store.observe_entropy();
        }
        Ok(Answer::Now(Value::record(vec![Value::str("a note")])))
    }
}

/// A draw of randomness is each reader's own (LLP 1069.005 D2), so a build gives no page for a render that drew: its
/// line names the route, the not-found route's at `/404.html` too, and the run fails once every page has rendered. A
/// render of a location the command line names, without `--build`, still makes its page.
#[test]
fn a_build_gives_no_page_for_a_render_that_drew_randomness() {
    let plan = contract::compile(
        r#"
shape Note
  text: string

routes nav
  home "/" render=build
  drawn "/drawn" render=build
  notfound render=build

component App
  resource note = note(top(nav).name) as shape Note
  view
    text note.text
"#,
    )
    .unwrap()
    .encode();
    let mut out = Vec::new();
    let status = exact_render::main_with(&plan, Notes::default, ["--build".to_owned()], &mut out);
    let lines: Vec<serde_json::Value> = String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let refusal = |route: &str| {
        format!("route `{route}` is render=build, but its render drew randomness, and a page made at build would give every reader the same random values (LLP 1069.005 D2). Declare the route render=request, rendered for each request, or render=client, rendered in the browser")
    };
    let got: Vec<_> = lines
        .iter()
        .map(|line| {
            (
                line["location"].as_str().unwrap(),
                line["root"]
                    .as_str()
                    .is_some_and(|root| root.contains("a note")),
                line["error"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("/", true, None),
            ("/drawn", false, Some(refusal("drawn"))),
            ("/404.html", false, Some(refusal("notfound"))),
        ]
    );
    assert_eq!(format!("{status:?}"), format!("{:?}", ExitCode::FAILURE));
    // A location named on the command line, without `--build`, writes no file, so its page is made whatever it drew
    let mut out = Vec::new();
    let status = exact_render::main_with(&plan, Notes::default, ["/drawn".to_owned()], &mut out);
    let line: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert!(line["error"].is_null(), "{line}");
    assert!(
        line["root"]
            .as_str()
            .is_some_and(|root| root.contains("a note")),
        "{line}"
    );
    assert_eq!(format!("{status:?}"), format!("{:?}", ExitCode::SUCCESS));
}
