//! Keyframe animations on the web (LLP 1055 D7): the browser plays them. The
//! host sends each `@keyframes` rule once, by its name, ahead of the
//! `animation` declaration that uses it; nothing runs per frame. A projected
//! document carries the rules in its head.

use exact_runner::{DataError, DataSource, Event, Value};
use exact_web::Host;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, s: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(s.into()))
    }
}

fn boot() -> (Host<NoData>, String) {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../contract/corpus/keyframes.contract"
    ))
    .unwrap();
    let plan = contract::compile(&src).unwrap();
    exact_web::link(exact_web_capabilities::ALL);
    Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap()
}

/// Each `keyframes` op's name and rule, in batch order. Neither holds a
/// quote or a backslash, so the JSON text is the value.
fn rules(batch: &str) -> Vec<(String, String)> {
    batch
        .split("{\"op\":\"keyframes\",\"name\":\"")
        .skip(1)
        .map(|op| {
            let (name, rest) = op.split_once("\",\"css\":\"").unwrap();
            let (css, _) = rest.split_once("\"}").unwrap();
            (name.to_string(), css.to_string())
        })
        .collect()
}

#[test]
fn each_rule_reaches_the_page_once_before_the_declaration_naming_it() {
    let (mut host, first) = boot();
    let sent = rules(&first);
    let [(breathe, breathe_rule), (appear, appear_rule)] = sent.as_slice() else {
        panic!("{first}")
    };
    // By the authored name: keyframes are global by name (LLP 1055 D5, D7).
    assert_eq!((breathe.as_str(), appear.as_str()), ("breathe", "appear"));
    assert_eq!(
        breathe_rule,
        "0%{opacity:0.4;scale:0.9;}50%{opacity:1;}100%{opacity:0.4;}"
    );
    assert_eq!(
        appear_rule,
        "0%{animation-timing-function:ease-out;opacity:0;translate:0px 8px;}"
    );
    let declaration = "animation:1.6s ease-in-out 0s infinite normal none running breathe;";
    assert!(first.contains(declaration), "{first}");
    assert!(
        first.find(breathe_rule.as_str()) < first.find(declaration),
        "the rule precedes its first use"
    );
    assert!(first.contains("animation:0.3s ease 0s 1 normal both running appear;"));
    assert!(
        !first.contains("\"op\":\"animate\""),
        "the browser plays it; no frames"
    );

    // A projected document carries the same rules in its head.
    let document = host.document().unwrap();
    assert_eq!(
        document.keyframes,
        format!("@keyframes appear{{{appear_rule}}}@keyframes breathe{{{breathe_rule}}}")
    );
    assert!(document.root.contains(declaration));

    // `none` takes the declaration away; back again, the rule is already there.
    let k = host.runner().kernel();
    let toggle = k.node_by_key(k.find_by_test_id("toggle")[0]).unwrap().id;
    let off = host.dispatch_at(toggle, Event::Press, 100.0);
    assert!(!off.contains(declaration), "{off}");
    assert!(rules(&off).is_empty());
    let on = host.dispatch_at(toggle, Event::Press, 200.0);
    assert!(on.contains(declaration), "{on}");
    assert!(rules(&on).is_empty(), "sent once: {on}");
}

#[test]
fn prefetched_animations_are_paused_until_a_reveal_restates_their_css() {
    use exact_runner::{CollectionFeedback, CollectionFill, RowMeasurement};

    exact_web::link(exact_web_capabilities::ALL);
    let source = r#"keyframes Draw
  from opacity=0.2
  to opacity=1
component App
  view
    list virtualized=true height=120 width=240 estimated-item-height=40
      for item in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9] key=item
        row height=40
          box testId=`box-${item}` animation="Draw 1s linear both"
          box testId=`none-${item}` animation="Draw 1s linear both" -exact-animation-trigger="none"
          box testId=`paused-${item}` animation="Draw 1s linear both paused"
          svg width=20 height=20
            circle testId=`svg-${item}` r=8 animation="Draw 1s linear both"
"#;
    let (mut host, first) = Host::boot(
        &contract::compile(source).unwrap().encode(),
        NoData,
        Default::default(),
        "/",
    )
    .unwrap();
    let id = |name| {
        let k = host.runner().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let ids = [id("box-5"), id("svg-5"), id("none-5"), id("paused-5")];
    let css = |batch: &str, id| -> Option<String> {
        let batch: serde_json::Value = serde_json::from_str(batch).unwrap();
        batch["ops"].as_array().unwrap().iter().find_map(|op| {
            (op["id"].as_u64() == Some(u64::from(id)))
                .then(|| op["css"].as_str().map(str::to_owned))
                .flatten()
        })
    };
    for (i, id) in ids.iter().enumerate() {
        let css = css(&first, *id).unwrap();
        assert!(
            css.contains(if i == 2 {
                " running Draw"
            } else {
                " paused Draw"
            }),
            "{css}"
        );
    }
    let document = host.document().unwrap();
    for name in ["box-5", "svg-5"] {
        let at = document
            .root
            .find(&format!("data-testid=\"{name}\""))
            .unwrap();
        let start = document.root[..at].rfind('<').unwrap();
        let end = at + document.root[at..].find('>').unwrap();
        let tag = &document.root[start..end];
        assert!(tag.contains(" paused Draw"), "{tag}");
    }
    let snapshot = host.runner().collections().remove(0);
    let feedback = CollectionFeedback {
        view: snapshot.view,
        revision: snapshot.revision,
        scroll_sequence: 1,
        offset: 160.0,
        port_main: 120.0,
        port_cross: 240.0,
        cross: 240.0,
        padding: [0.0; 2],
        focus_view: None,
        interaction_view: None,
        measurements: snapshot
            .rows
            .iter()
            .map(|row| RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: 40.0,
            })
            .collect(),
    };
    let waiting = host.collection_feedback(
        &feedback
            .encode_with(CollectionFill {
                animating: true,
                view_offset: Some(0.0),
                ..Default::default()
            })
            .unwrap(),
    );
    for name in ["box-5", "svg-5"] {
        let k = host.runner().kernel();
        assert!(
            k.awaits_view(&k.node_by_key(k.find_by_test_id(name)[0]).unwrap()),
            "{waiting}"
        );
    }
    let mut feedback = feedback;
    let snapshot = host.runner().collections().remove(0);
    feedback.revision = snapshot.revision;
    feedback.measurements = snapshot
        .rows
        .iter()
        .map(|row| RowMeasurement {
            view: row.view,
            epoch: row.epoch,
            size: 40.0,
        })
        .collect();
    let revealed = host.collection_feedback(
        &feedback
            .encode_with(CollectionFill {
                animating: true,
                view_offset: Some(160.0),
                ..Default::default()
            })
            .unwrap(),
    );
    for id in &ids[..2] {
        let css = css(&revealed, *id).expect("a reveal must resend animation CSS");
        assert!(css.contains(" running Draw"), "{css}");
    }
    assert!(css(&revealed, ids[3]).is_none_or(|css| css.contains(" paused Draw")));
}
