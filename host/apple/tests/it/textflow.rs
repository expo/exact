//! @ref LLP 1043.000 §3 D4, D7 — real batches carry geometry independently of frames.
use exact_apple::{content_region::ContentRegionRegistration, Host};
use exact_kernel::{MonospaceMeasurer, TextMeasurer};
use exact_runner::{DataError, DataSource, Event, Value};
struct Data;
impl DataSource for Data {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}
const SOURCE: &str = r#"component App
  state x = 80
  action move
    x = x + 20
  action leave
    x = 900
  view
    column width=400 height=500
      view id="owner" width=400 height=300 overflow-x="hidden" overflow-y="hidden"
        view id="content" position="relative" width="100%" height="100%"
          text "The river carries its quiet story through the garden and beyond the trees." testId="prose" width=400 height=300
          box position="absolute" left=x top=40 width=60 height=60 wrap-flow="both" shape-outside="circle()"
        text "Waiting" id="pending"
      button testId="move" press=move
        text "Move"
      button testId="leave" press=leave
        text "Leave"
"#;
fn id<D: DataSource>(h: &Host<D>, name: &str) -> u32 {
    let key = h.runner().kernel().find_by_test_id(name)[0];
    h.runner().kernel().node_by_key(key).unwrap().id
}
#[test]
fn unchanged_frame_gets_flow_and_leaving_shape_gets_empty_clear() {
    let plan = contract::compile(SOURCE).unwrap().encode();
    let (mut h, boot) = Host::boot(
        &plan,
        Data,
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
    )
    .unwrap();
    let prose = id(&h, "prose");
    assert!(
        boot.contains(&format!("\"op\":\"flow\",\"id\":{prose}")),
        "{boot}"
    );
    let frame = h.runner().kernel().node(prose).unwrap().frame;
    let move_id = id(&h, "move");
    let moved = h.dispatch_at(move_id, Event::Press, 16.);
    assert!(
        moved.contains(&format!("\"op\":\"flow\",\"id\":{prose}")),
        "{moved}"
    );
    assert!(
        !moved.contains(&format!("\"op\":\"frame\",\"id\":{prose},")),
        "{moved}"
    );
    assert_eq!(frame, h.runner().kernel().node(prose).unwrap().frame);
    let leave_id = id(&h, "leave");
    let cleared = h.dispatch_at(leave_id, Event::Press, 32.);
    assert!(
        cleared.contains(&format!("\"op\":\"flow\",\"id\":{prose},\"shapes\":[]")),
        "{cleared}"
    );
}
#[test]
fn raster_region_keeps_flowed_publications_and_rejects_superseded_geometry() {
    for padded in [false, true] {
        region_flow(padded);
    }
}
#[test]
fn painted_exclusions_keep_native_presentation() {
    let box_line = SOURCE
        .lines()
        .find(|line| line.contains("box position="))
        .unwrap();
    let shell = SOURCE
        .replace(box_line, "")
        .replace(
            "      button testId=\"move\"",
            &format!(
                "{}\n      button testId=\"move\"",
                box_line.replacen("          ", "      ", 1)
            ),
        )
        .replace("column width=", "column position=\"relative\" width=");
    for source in [SOURCE, &shell] {
        painted_exclusion(source);
    }
}
fn painted_exclusion(source: &str) {
    let source = source.replace(
        "wrap-flow=\"both\"",
        "background-color=\"red\" wrap-flow=\"both\"",
    );
    let plan = contract::compile(&source).unwrap().encode();
    let (mut h, mut batch) = Host::boot_region(
        &plan,
        Data,
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
        exact_runner::time::Launch::NONE,
    )
    .unwrap();
    for _ in 0..20 {
        let Some((id, request)) = h.pending_region_request() else {
            break;
        };
        let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
        batch = h.complete_region_text(id, metrics, std::rc::Rc::new(()));
    }
    assert!(
        batch.contains("\"disabled\":"),
        "an opaque text raster hides the red exclusion: {batch}"
    );
    assert!(h.region_publication_id().is_none());
    assert!(h.pending_region_request().is_none());
    let moved = h.dispatch_at(id(&h, "move"), Event::Press, 16.);
    assert!(!moved.contains("\"op\":\"region\""));
    assert!(!h
        .runner()
        .kernel()
        .node(id(&h, "prose"))
        .unwrap()
        .flow_shapes()
        .is_empty());
}

#[test]
fn unsupported_content_paint_keeps_native_presentation() {
    for (target, paint) in [
        ("testId=\"prose\"", "background-color=\"red\""),
        ("id=\"content\"", "background-color=\"red\""),
        ("id=\"owner\"", "box-shadow=\"2px 2px 3px red\""),
        ("testId=\"prose\"", "border-width=2 border-style=\"solid\""),
        ("testId=\"prose\"", "box-shadow=\"2px 2px 3px red\""),
        ("testId=\"prose\"", "opacity=0.5"),
        ("testId=\"prose\"", "-webkit-text-stroke=\"2px red\""),
        ("id=\"content\"", "-webkit-text-stroke=\"2px red\""),
        (
            "testId=\"prose\" width=400 height=300",
            "overflow-x=\"hidden\" overflow-y=\"hidden\"",
        ),
    ] {
        let source = SOURCE.replace(target, &format!("{target} {paint}"));
        let source = source.replace(
            "testId=\"prose\" width=400 height=300 overflow",
            "testId=\"prose\" width=400 height=60 overflow",
        );
        // Long enough to overflow the clipped box beside the exclusion.
        let source = source.replace(
            "beyond the trees.\" testId=\"prose\" width=400 height=60",
            "beyond the trees, past the mill, under the bridge and out to the wide grey sea.\" testId=\"prose\" width=400 height=60",
        );
        let plan = contract::compile(&source).unwrap().encode();
        let (mut h, mut batch) = Host::boot_region(
            &plan,
            Data,
            Box::new(MonospaceMeasurer::default()),
            400.,
            500.,
            ContentRegionRegistration {
                activate: None,
                owner: "owner",
                content: "content",
                pending: "pending",
            },
            exact_runner::time::Launch::NONE,
        )
        .unwrap();
        for _ in 0..20 {
            let Some((id, request)) = h.pending_region_request() else {
                break;
            };
            let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
            batch.push_str(&h.complete_region_text(id, metrics, std::rc::Rc::new(())));
        }
        assert!(batch.contains("\"disabled\":"), "{paint}: {batch}");
        assert!(h.region_publication_id().is_none(), "{paint}");
        assert!(h.pending_region_request().is_none(), "{paint}");
        let moved = h.dispatch_at(id(&h, "move"), Event::Press, 16.);
        assert!(!moved.contains("\"op\":\"region\""), "{moved}");
    }
}

#[test]
fn native_projections_return_flow_to_ordinary_views() {
    for initially_flowed in [true, false] {
        let source = SOURCE
            .replace(
                "state x = 80",
                if initially_flowed {
                    "state x = 80\n  state margin = 0"
                } else {
                    "state x = 420\n  state margin = 0"
                },
            )
            .replace("x = x + 20", "margin = 80")
            .replace(
                "wrap-flow=\"both\"",
                "wrap-flow=\"both\" shape-margin=margin",
            );
        let plan = contract::compile(&source).unwrap().encode();
        let (mut h, mut history) = Host::boot_native_region(
            &plan,
            Data,
            Box::new(MonospaceMeasurer::default()),
            400.,
            500.,
            ContentRegionRegistration {
                activate: None,
                owner: "owner",
                content: "content",
                pending: "pending",
            },
            exact_apple::content_region::NativeProjectionLimits::default(),
            exact_runner::time::Launch::NONE,
        )
        .unwrap();
        for _ in 0..20 {
            let Some((id, request)) = h.pending_region_request() else {
                break;
            };
            let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
            history.push_str(&h.complete_region_text(id, metrics, std::rc::Rc::new(())));
        }
        if !initially_flowed {
            assert!(
                h.region_publication_id().is_some(),
                "unflowed A is selected"
            );
            history.push_str(&h.dispatch_at(id(&h, "move"), Event::Press, 16.));
        }
        assert!(history.contains("\"disabled\":"), "{history}");
        assert!(h.region_publication_id().is_none());
        assert!(h.pending_region_request().is_none());
        let prose = id(&h, "prose");
        assert!(
            history.contains(&format!("\"op\":\"create\",\"id\":{prose},")),
            "{history}"
        );
        assert!(
            history.contains(&format!("\"op\":\"frame\",\"id\":{prose},")),
            "{history}"
        );
        assert!(!h
            .runner()
            .kernel()
            .node(prose)
            .unwrap()
            .flow_shapes()
            .is_empty());
        let cleared = h.dispatch_at(id(&h, "leave"), Event::Press, 32.);
        assert!(
            cleared.contains(&format!("\"op\":\"flow\",\"id\":{prose},\"shapes\":[]")),
            "{cleared}"
        );
    }
}

#[test]
fn region_wire_carries_every_font_metric() {
    let source = SOURCE.replace("testId=\"prose\"", r#"testId="prose" font-stretch="50%" font-variant-numeric="tabular-nums" font-variant-ligatures="no-common-ligatures" font-variation-settings="\"wght\" 650" font-feature-settings="\"liga\" 1" font-optical-sizing="none""#);
    let plan = contract::compile(&source).unwrap().encode();
    let (h, _) = Host::boot_region(
        &plan,
        Data,
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
        exact_runner::time::Launch::NONE,
    )
    .unwrap();
    let request = h.pending_region_request().unwrap().0;
    let wire: serde_json::Value =
        serde_json::from_str(&h.region_request_json(request).unwrap()).unwrap();
    for row in [&wire["strut"], &wire["runs"][0]] {
        assert_eq!(row["stretch"], 50.);
        assert_eq!(row["numeric"], 1);
        assert_eq!(row["ligatures"], 2);
        assert_eq!(row["variations"], serde_json::json!([["wght", 650]]));
        assert_eq!(row["features"], serde_json::json!([["liga", 1]]));
        assert_eq!(row["opticalSizing"], "none");
    }
}

#[test]
fn only_a_current_incomplete_flow_returns_the_registration_to_native_views() {
    let plan = contract::compile(SOURCE).unwrap().encode();
    let (mut h, _) = Host::boot_region(
        &plan,
        Data,
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
        exact_runner::time::Launch::NONE,
    )
    .unwrap();
    for _ in 0..20 {
        let (id, request) = h.pending_region_request().expect("flow request");
        if !request.exclusions().is_empty() {
            break;
        }
        let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
        h.complete_region_text(id, metrics, std::rc::Rc::new(()));
    }
    let (old, request) = h.pending_region_request().unwrap();
    assert!(!request.exclusions().is_empty());
    let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
    h.dispatch_at(id(&h, "move"), Event::Press, 16.);
    let current = h.pending_region_request().unwrap().0;
    assert_ne!(old, current);
    let stale = h.fallback_region_text(old);
    assert!(!stale.contains("\"disabled\":"));
    assert_eq!(h.pending_region_request().unwrap().0, current);
    let fallback = h.fallback_region_text(current);
    assert!(fallback.contains("\"disabled\":"), "{fallback}");
    assert!(fallback.contains("\"error\":null"), "{fallback}");
    assert!(h.pending_region_request().is_none());
    assert!(h.region_publication_id().is_none());
    assert!(!h
        .complete_region_text(old, metrics, std::rc::Rc::new(()))
        .contains("\"op\":\"region\""));
    h.dispatch_at(id(&h, "leave"), Event::Press, 32.);
    assert!(h
        .runner()
        .kernel()
        .node(id(&h, "prose"))
        .unwrap()
        .flow_shapes()
        .is_empty());
    assert!(h.pending_region_request().is_none());
}
fn region_flow(padded: bool) {
    let source = SOURCE
        .replace("id=\"owner\"", "id=\"owner\" background-color=\"red\"")
        .replace(
            "testId=\"prose\" width=400 height=300",
            if padded {
                "testId=\"prose\" width=400 height=300 padding-left=10 padding-top=12"
            } else {
                "testId=\"prose\" width=400 height=300"
            },
        );
    let plan = contract::compile(&source).unwrap().encode();
    let (mut h, mut batch) = Host::boot_region(
        &plan,
        Data,
        Box::new(MonospaceMeasurer::default()),
        400.,
        500.,
        ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        },
        exact_runner::time::Launch::NONE,
    )
    .unwrap();
    let mut flowed_source = None;
    for _ in 0..20 {
        let Some((id, request)) = h.pending_region_request() else {
            break;
        };
        if !request.exclusions().is_empty() {
            let wire: serde_json::Value =
                serde_json::from_str(&h.region_request_json(id).unwrap()).unwrap();
            assert_eq!(wire["exclusions"][0]["kind"], "Circle");
            assert_eq!(
                wire["exclusions"][0]["cx"],
                if padded { 100. } else { 110. }
            );
            assert_eq!(wire["exclusions"][0]["cy"], if padded { 58. } else { 70. });
            flowed_source = Some(wire["source"].clone());
        }
        let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
        batch = h.complete_region_text(id, metrics, std::rc::Rc::new(()));
    }
    assert!(!batch.contains("\"disabled\":"), "{batch}");
    assert!(h.pending_region_request().is_none());
    assert!(h.region_publication_id().is_some());
    assert!(flowed_source.is_some(), "no flowed worker request");
    let prose = id(&h, "prose");
    let wire: serde_json::Value = serde_json::from_str(&batch).unwrap();
    let region = wire["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["op"] == "region")
        .unwrap();
    let frame = region["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == prose)
        .unwrap();
    assert_eq!(frame["box"][2], 400.);
    assert_eq!(frame["box"][0], if padded { 10. } else { 0. });
    assert_eq!(frame["box"][1], if padded { 12. } else { 0. });
    assert!(
        batch.contains(&format!("\"op\":\"flow\",\"id\":{prose}")),
        "{batch}"
    );
    assert!(!h
        .runner()
        .kernel()
        .node(prose)
        .unwrap()
        .flow_shapes()
        .is_empty());
    let accepted = h.region_publication_id();
    let move_id = id(&h, "move");
    let moved = h.dispatch_at(move_id, Event::Press, 16.);
    assert!(!moved.contains("\"disabled\":"), "{moved}");
    let (stale_id, stale) = h.pending_region_request().unwrap();
    let metrics = stale.with_request(|r| MonospaceMeasurer::default().measure(r));
    let wire: serde_json::Value =
        serde_json::from_str(&h.region_request_json(stale_id).unwrap()).unwrap();
    assert_eq!(wire["source"], flowed_source.unwrap());
    assert_eq!(
        wire["exclusions"][0]["cx"],
        if padded { 120. } else { 130. }
    );
    assert_eq!(h.region_publication_id(), accepted);
    h.dispatch_at(move_id, Event::Press, 32.);
    let current = h.pending_region_request().unwrap().0;
    assert_ne!(current, stale_id);
    h.complete_region_text(stale_id, metrics, std::rc::Rc::new(()));
    assert_eq!(h.pending_region_request().unwrap().0, current);
    assert_eq!(h.region_publication_id(), accepted);
    for _ in 0..20 {
        let Some((id, request)) = h.pending_region_request() else {
            break;
        };
        let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
        h.complete_region_text(id, metrics, std::rc::Rc::new(()));
    }
    assert!(h.pending_region_request().is_none());
    assert_ne!(h.region_publication_id(), accepted);
    let leave = id(&h, "leave");
    h.dispatch_at(leave, Event::Press, 48.);
    for _ in 0..20 {
        let Some((id, request)) = h.pending_region_request() else {
            break;
        };
        let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
        h.complete_region_text(id, metrics, std::rc::Rc::new(()));
    }
    assert!(h.pending_region_request().is_none());
    assert!(h.region_publication_id().is_some());
    assert!(h
        .runner()
        .kernel()
        .node(prose)
        .unwrap()
        .flow_shapes()
        .is_empty());
}

#[test]
fn list_settlement_publishes_final_mounted_flow_and_clears_disappearing_shapes() {
    struct Rows;
    impl DataSource for Rows {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::list(
                (0..100).map(|n| Value::Number(n as f64)).collect(),
            ))
        }
    }
    let plan = contract::compile(r#"component App
  state holes = true
  resource rows = rows() as shape number[]
  action clear
    holes = false
  view
    column width=240 height=200
      button testId="clear" press=clear
        text "Clear"
      list testId="list" virtualized=true estimated-item-height=80 width=240 height=160 overflow-x="hidden"
        for row in rows key=row
          view position="relative" width=240 height=80
            text `Paragraph ${row} around the circle` testId=`row-${row}` width=240 height=80
            if holes
              box position="absolute" left=80 top=10 width=40 height=40 wrap-flow="both" shape-outside="circle()"
"#).unwrap().encode();
    let (mut h, _) = Host::boot(
        &plan,
        Rows,
        Box::new(MonospaceMeasurer::default()),
        240.,
        200.,
    )
    .unwrap();
    let list = id(&h, "list");
    let old: std::collections::BTreeSet<_> = h
        .runner()
        .kernel()
        .arena()
        .iter_live()
        .map(|slot| h.runner().kernel().arena().key(slot))
        .collect();
    let snapshot = h.runner().collections().remove(0);
    assert_eq!(snapshot.view, list);
    let feedback = exact_runner::CollectionFeedback {
        view: list,
        revision: snapshot.revision,
        scroll_sequence: snapshot.scroll_sequence + 1,
        offset: 400.,
        port_cross: 240.,
        port_main: 160.,
        cross: 240.,
        padding: [0.0; 2],
        measurements: snapshot
            .rows
            .iter()
            .map(|r| exact_runner::RowMeasurement {
                view: r.view,
                epoch: r.epoch,
                size: 80.,
            })
            .collect(),
        focus_view: None,
        interaction_view: None,
    };
    let batch = h.collection_feedback(&feedback.encode().unwrap(), 0.);
    assert!(!batch.contains("\"error\":\""), "{batch}");
    let mut flowed = Vec::new();
    let mut mounted = 0;
    for slot in h.runner().kernel().arena().iter_live() {
        let key = h.runner().kernel().arena().key(slot);
        let node = h.runner().kernel().node_by_key(key).unwrap();
        if node.flow_shapes().is_empty() {
            continue;
        }
        flowed.push(node.id);
        if old.contains(&key) {
            continue;
        }
        mounted += 1;
        let mut expected = format!("{{\"op\":\"flow\",\"id\":{},\"shapes\":[", node.id);
        for (i, shape) in node.flow_shapes().iter().enumerate() {
            if i > 0 {
                expected.push(',');
            }
            shape.write_json(&mut expected);
        }
        expected.push_str("]}");
        assert!(
            batch.contains(&expected),
            "missing final flow {expected}: {batch}"
        );
        let prefix = format!("{{\"op\":\"flow\",\"id\":{},", node.id);
        assert_eq!(
            batch.matches(&prefix).count(),
            1,
            "intermediate geometry must not escape"
        );
    }
    assert!(
        mounted > 0,
        "the report must mount paragraphs under exclusions"
    );
    let clear = id(&h, "clear");
    let batch = h.dispatch_at(clear, Event::Press, 16.);
    for view in flowed {
        let Some(node) = h.runner().kernel().node(view) else {
            continue;
        };
        assert!(node.flow_shapes().is_empty());
        assert!(
            batch.contains(&format!("{{\"op\":\"flow\",\"id\":{view},\"shapes\":[]}}")),
            "{batch}"
        );
    }
    let repeated = h.resize(240., 200.);
    assert!(
        !repeated.contains("\"op\":\"flow\""),
        "unchanged clears are not repeated: {repeated}"
    );
}

#[test]
fn retired_column_fragments_are_not_cleared_and_whole_new_rows_need_no_record() {
    struct Rows;
    impl DataSource for Rows {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::list(
                (0..100).map(|n| Value::Number(n as f64)).collect(),
            ))
        }
    }
    let sentence = "The river carries its quiet story through the garden and beyond the trees. The birds follow the water past the bridge and into the fields at the edge of the village.";
    let plan = contract::compile(&format!(
        r#"component App
  resource rows = rows() as shape number[]
  view
    list testId="list" virtualized=true estimated-item-height=140 width=320 height=400
      for row in rows key=row
        view column-count=2 column-gap=20 width=320
          text "{sentence}" testId=`first-${{row}}`
          text (row % 2 == 0 ? "A short note." : "{sentence} {sentence} {sentence}")
"#
    ))
    .unwrap()
    .encode();
    let (mut h, boot) = Host::boot(
        &plan,
        Rows,
        Box::new(MonospaceMeasurer::default()),
        320.,
        400.,
    )
    .unwrap();
    let old = id(&h, "first-0");
    let kernel = h.runner().kernel();
    let old_key = kernel.find_by_test_id("first-0")[0];
    let old_fragments = kernel.fragments(old_key).unwrap();
    assert_eq!(old_fragments.len(), 2);
    let old_clip_height = old_fragments[0].height;
    assert!(boot.contains(&format!("\"op\":\"fragments\",\"id\":{old},")));
    let whole = id(&h, "first-1");
    assert!(kernel
        .fragments(kernel.find_by_test_id("first-1")[0])
        .is_none());
    assert!(!boot.contains(&format!("\"op\":\"fragments\",\"id\":{whole},")));
    let snapshot = h.runner().collections().remove(0);
    let feedback = exact_runner::CollectionFeedback {
        view: snapshot.view,
        revision: snapshot.revision,
        scroll_sequence: snapshot.scroll_sequence + 1,
        offset: 6000.,
        port_cross: 320.,
        port_main: 400.,
        cross: 320.,
        padding: [0.; 2],
        measurements: vec![],
        focus_view: None,
        interaction_view: None,
    };
    let batch = h.collection_feedback(&feedback.encode().unwrap(), 0.);
    assert!(batch.contains(&format!("\"op\":\"destroy\",\"id\":{old}}}")));
    assert!(!batch.contains(&format!("\"op\":\"fragments\",\"id\":{old},")));
    let kernel = h.runner().kernel();
    let next_key = kernel.find_by_test_id("first-41")[0];
    let next = kernel.node_by_key(next_key).unwrap();
    assert!(kernel.fragments(next_key).is_none());
    assert!(next.frame.height > old_clip_height);
    assert!(batch.contains(&format!("\"op\":\"create\",\"id\":{},", next.id)));
    assert!(!batch.contains(&format!("\"op\":\"fragments\",\"id\":{},", next.id)));
}

// @ref LLP 1043.000 §8 — the Reader's drop cap: an auto-height paragraph is
// measured through the seam around the shape it is painted around.
#[test]
fn a_drop_cap_measures_its_auto_height_paragraph_around_the_shape() {
    use std::{cell::RefCell, rc::Rc};
    struct Seen(Rc<RefCell<Vec<usize>>>);
    impl TextMeasurer for Seen {
        fn measure(
            &mut self,
            r: &exact_kernel::TextMeasureRequest<'_>,
        ) -> exact_kernel::TextMetrics {
            self.0.borrow_mut().push(r.exclusions.len());
            MonospaceMeasurer::default().measure(r)
        }
    }
    let boot = |wrap: &str| {
        let plan = contract::compile(&format!(
            r#"component App
  view
    box width=360 padding=20 position="relative"
      box position="absolute" left=20 top=20 width=48 height=48 wrap-flow="{wrap}" shape-outside="inset(0)" shape-margin=6
        text "T" font-size=48 line-height=1
      text "There is an hour when the garden belongs to neither day nor night. The visitors have gone, but the birds have not yet settled." testId="lede"
      text "After." testId="after"
"#
        ))
        .unwrap()
        .encode();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (h, batch) = Host::boot(&plan, Data, Box::new(Seen(seen.clone())), 400., 600.).unwrap();
        let seen = seen.borrow().clone();
        (h, batch, seen)
    };
    let (plain, _, _) = boot("auto");
    let (h, batch, seen) = boot("both");
    let (lede, after) = (id(&h, "lede"), id(&h, "after"));
    let kernel = h.runner().kernel();
    let (l, a) = (
        kernel.node(lede).unwrap().frame,
        kernel.node(after).unwrap().frame,
    );
    assert!(seen.contains(&1), "no request carried the shape: {seen:?}");
    assert!(
        batch.contains(&format!("\"op\":\"flow\",\"id\":{lede}")),
        "{batch}"
    );
    assert_eq!(kernel.node(lede).unwrap().flow_refusal(), None);
    assert!(l.height > plain.runner().kernel().node(lede).unwrap().frame.height);
    assert_eq!(a.y, l.y + l.height);
}
