//! A live activity's send goes to the shell with its ticket, and the shell's
//! report answers it and lists what it shows (LLP 1126 D13).
use super::*;
use exact_runner::{DataError, Value};

struct Nothing;
impl DataSource for Nothing {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

const APP: &str = r#"shape State
  title: string
shape ID
  id: string
shape Activity
  id: string
  kind: string
  state: string
  activityState: string
  token: option<string>
  promoted: bool
shape Activities
  activities: Activity[]
  frequentPushesEnabled: bool
component App
  resource activities = exactActivities() as shape Activities
  mutation started as shape ID
  action start
    send started = startActivity("patrol", State(title="Ronda"))
  view
    column
      button press=start testId="start"
        text "Start"
      text activities.activities.length.toString() testId="count"
      if match started { case some(a) => true, case none => false }
        text "started" testId="id"
"#;

const MAPPED: &str = r#"{"reach":{"androidActivities":[{"kind":"patrol","android":{"channel":"default","title":"title"}}]}}"#;

fn boot(compat: &str) -> Presenter<Nothing> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(APP),
        Nothing,
        (400., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.compat = compat.into();
    p.serve_requests();
    p
}

fn shown(p: &Presenter<Nothing>, name: &str) -> bool {
    !p.host().kernel().find_by_test_id(name).is_empty()
}

/// How many activities `exactActivities` answers.
fn listed(p: &Presenter<Nothing>) -> usize {
    match p.host().runner().resource("activities") {
        Some(Value::Record(fields)) => match &fields[0] {
            Value::List(items) => items.len(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

/// The bodies of the queued requests of kind 24.
fn activity_requests(p: &mut Presenter<Nothing>) -> Vec<serde_json::Value> {
    let words = p.take_requests().unwrap_or_default();
    let (mut out, mut at) = (Vec::new(), 0);
    while at < words.len() {
        let n = words[at + 2] as usize;
        if words[at] == 24 {
            let body = &words[at + 3..at + 3 + n];
            let len = body[0] as usize;
            let bytes: Vec<u8> = body[1..]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .take(len)
                .collect();
            out.push(serde_json::from_slice(&bytes).unwrap());
        }
        at += 3 + n;
    }
    out
}

fn press(p: &mut Presenter<Nothing>, name: &str) {
    let k = p.host().kernel();
    let view = k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
    p.tap(view).unwrap();
}

#[test]
fn a_send_reaches_the_shell_with_its_ticket_and_its_answer_lands() {
    let mut p = boot(MAPPED);
    // The source's first read asks the shell for its list.
    let first = activity_requests(&mut p);
    assert_eq!(first, [serde_json::json!({"enumerate": true})]);
    p.activities(r#"{"activities":[],"frequentPushesEnabled":false}"#);
    press(&mut p, "start");
    let sends = activity_requests(&mut p);
    assert_eq!(sends.len(), 1, "{sends:?}");
    assert_eq!(sends[0]["command"], "startActivity");
    assert_eq!(
        sends[0]["args"],
        serde_json::json!(["patrol", {"title": "Ronda"}])
    );
    let ticket = sends[0]["ticket"].as_u64().unwrap();
    assert_eq!(
        p.activities(&format!(
            r#"{{"ticket":{ticket},"status":200,"body":{{"id":"n-1"}}}}"#
        )),
        None
    );
    assert!(shown(&p, "id"));
    p.activities(r#"{"activities":[{"id":"n-1","kind":"patrol","state":"{\"title\":\"Ronda\"}","activityState":"active","token":null,"promoted":true}],"frequentPushesEnabled":false}"#);
    assert_eq!(listed(&p), 1);
}

#[test]
fn a_refused_send_is_the_mutations_error_and_an_unmapped_app_stays_unlinked() {
    let mut p = boot(MAPPED);
    activity_requests(&mut p);
    press(&mut p, "start");
    let ticket = activity_requests(&mut p)[0]["ticket"].as_u64().unwrap();
    p.activities(&format!(
        r#"{{"ticket":{ticket},"status":400,"error":"notifications are off for this app"}}"#
    ));
    assert!(!shown(&p, "id"));
    // No mapping: the runner refuses the send itself, and the shell sees none.
    let mut p = boot(r#"{"reach":{}}"#);
    press(&mut p, "start");
    assert!(activity_requests(&mut p).is_empty());
    assert!(p
        .host()
        .runner()
        .journal()
        .any(|l| l.contains("startActivity is not linked into this artifact")));
}

#[test]
fn a_send_body_that_is_not_an_object_is_refused_not_forwarded() {
    let mut p = boot(MAPPED);
    activity_requests(&mut p);
    p.activity_send(7, b"[1]");
    assert!(activity_requests(&mut p).is_empty());
}
