//! Apple collection transport and host commits, using the actual runner window.
use super::*;
use exact_runner::{CollectionFeedback, CollectionSnapshot, DataError, RowMeasurement, Value};
use std::cell::Cell as Counter;

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

const SOURCE: &str = r#"
component App
  state showing = true
  resource rows = rows() as shape number[]
  action toggle
    showing = !showing
  view
    column
      button press=toggle testId="toggle"
        text "toggle"
      if showing
        list virtualized=true height=160 width=240 testId="history"
          for row in rows key=row
            text `${row}` testId=`row-${row}`
"#;

#[derive(Default)]
struct Rows {
    queries: Rc<Counter<usize>>,
}

impl DataSource for Rows {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        self.queries.set(self.queries.get() + 1);
        Ok(Value::list(
            (0..1000).map(|i| Value::Number(i as f64)).collect(),
        ))
    }
}

fn fixture(source: &str) -> (Bridge<Rows>, String, Rc<Counter<usize>>) {
    let data = Rows::default();
    let queries = data.queries.clone();
    let plan = contract::compile(source).unwrap().encode();
    let (host, batch) = Host::boot(
        &plan,
        data,
        Box::new(MonospaceMeasurer::default()),
        400.,
        600.,
    )
    .unwrap();
    // This synchronous source issues no requests. Keep these fixtures independent
    // of worker admission and the scheduler's process-wide worker budget.
    let mut bridge = Bridge::new();
    bridge.host = Some(host);
    (bridge, batch, queries)
}

fn snapshot(bridge: &Bridge<Rows>) -> CollectionSnapshot {
    bridge
        .host
        .as_ref()
        .unwrap()
        .runner()
        .collections()
        .remove(0)
}

fn facts(snapshot: &CollectionSnapshot, top: f64) -> CollectionFeedback {
    CollectionFeedback {
        view: snapshot.view,
        revision: snapshot.revision,
        scroll_sequence: snapshot.scroll_sequence + 1,
        offset: top,
        port_cross: 240.,
        port_main: 160.,
        cross: 240.,
        padding: [0.0; 2],
        measurements: Vec::new(),
        focus_view: None,
        interaction_view: None,
    }
}

fn send(bridge: &mut Bridge<Rows>, facts: &CollectionFeedback, now_ms: f64) -> String {
    let bytes = facts.encode().unwrap();
    bridge.input_write(&bytes);
    let len = bridge.collection_feedback(bytes.len(), now_ms);
    String::from_utf8(bridge.output_bytes(len as usize).to_vec()).unwrap()
}

fn state(bridge: &Bridge<Rows>) -> (String, String, f64, f64) {
    let host = bridge.host.as_ref().unwrap();
    (
        host.runner().collections_json(),
        host.agent(r#"{"op":"tree"}"#),
        host.engine().now(),
        host.runner().now_ms(),
    )
}

#[test]
fn collection_feedback_submits_edge_request_without_another_bridge_call() {
    use exact_runner::{Answer, Outcome, Request, Store};
    use std::sync::mpsc::{channel, Sender};
    use std::time::Duration;

    struct DeferredRows(Sender<u64>);
    impl DataSource for DeferredRows {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::list(vec![Value::Number(0.), Value::Number(1.)]))
        }
        fn answer(
            &mut self,
            _: &mut Store,
            name: &str,
            args: &[Value],
        ) -> Result<Answer, DataError> {
            if args == [Value::Number(1.)] {
                Ok(Answer::Later(Request::continuation::<TestWork>(7)))
            } else {
                self.query(name, args).map(Answer::Now)
            }
        }
        fn continuation(&mut self, token: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
            let submitted = self.0.clone();
            Some(Box::new(move || {
                submitted.send(token).unwrap();
                Outcome::Storage(Vec::new())
            }))
        }
    }
    let source = r#"component App
  state cursor = 0
  resource rows = rows(cursor) as shape number[]
  action next
    cursor = 1
  view
    list virtualized=true height=160 width=240 reachend=next
      for row in rows key=row
        text `${row}`
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (submitted, received) = channel();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, DeferredRows(submitted), Hooks::none(), 400., 600.);
    let snapshot = bridge
        .host
        .as_ref()
        .unwrap()
        .runner()
        .collections()
        .remove(0);
    assert!(bridge.host.as_mut().unwrap().take_requests().is_empty());
    assert!(received.try_recv().is_err());
    let bytes = facts(&snapshot, 0.).encode().unwrap();
    bridge.input_write(&bytes);
    let len = bridge.collection_feedback(bytes.len(), 0.);
    let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize));
    assert!(batch.contains("\"error\":null"), "{batch}");
    let host = bridge.host.as_mut().unwrap();
    assert_eq!(host.runner().slot("cursor"), Some(&Value::Number(1.)));
    assert_eq!(host.runner().pending().len(), 1);
    assert!(
        host.take_requests().is_empty(),
        "feedback must submit its request"
    );
    // No pump, tick, event or other emit call may be needed to start the work.
    assert_eq!(received.recv_timeout(Duration::from_secs(2)).unwrap(), 7);
}

#[test]
fn collection_boot_uses_common_json_after_frames_and_ticks_do_not_emit_it() {
    let (mut bridge, batch, queries) = fixture(SOURCE);
    let snapshot = snapshot(&bridge);
    assert_eq!(snapshot.count, 1000);
    assert!(snapshot.rows.len() < 64);
    let host = bridge.host.as_ref().unwrap();
    let op = format!(
        "{{\"op\":\"collections\",\"items\":{}}}",
        host.runner().collections_json()
    );
    assert!(batch.contains(&op), "{batch}");
    assert!(batch.rfind("\"op\":\"frame\"").unwrap() < batch.find(&op).unwrap());
    let queries_before = queries.get();
    for time in [1.0, 10.0, 20.0] {
        let len = bridge.tick(time);
        let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize));
        assert!(!batch.contains("\"op\":\"collections\""), "{batch}");
    }
    // An unchanged layout also avoids retransmitting the identical metadata.
    let len = bridge.resize(400., 600.);
    assert!(!String::from_utf8_lossy(bridge.output_bytes(len as usize))
        .contains("\"op\":\"collections\""));
    assert_eq!(queries.get(), queries_before);
}

#[test]
fn a_sequence_only_report_does_not_resend_collections_on_a_scroll_commit() {
    let source = SOURCE.replace("action toggle", "state scrolled = 0\n  action scroll(left: number, top: number)\n    scrolled = scrolled + 1\n  action toggle")
        .replace("list virtualized=true", "list scroll=scroll virtualized=true");
    let (mut bridge, _, _) = fixture(&source);
    let initial = snapshot(&bridge);
    send(&mut bridge, &facts(&initial, 0.), 1.);
    let before = snapshot(&bridge);
    let report = facts(&before, 0.);
    send(&mut bridge, &report, 2.);
    let after = snapshot(&bridge);
    assert_eq!(before.revision, after.revision);
    assert!(after.scroll_sequence > before.scroll_sequence);
    let host = bridge.host.as_mut().unwrap();
    let batch = host.dispatch_at(
        after.view,
        Event::scroll_payload("0,0,240,32000,240,160").unwrap(),
        3.,
    );
    assert!(!batch.contains("\"op\":\"collections\""), "{batch}");
    assert!(batch.contains("\"error\":null"), "{batch}");
}

#[test]
fn collection_scroll_and_measurements_commit_without_requerying_resources() {
    let (mut bridge, _, queries) = fixture(SOURCE);
    let before = snapshot(&bridge);
    let queries_before = queries.get();
    let batch = send(&mut bridge, &facts(&before, 16_000.), 10.);
    assert!(batch.contains("\"error\":null"), "{batch}");
    for op in ["destroy", "create", "children", "frame", "collections"] {
        assert!(
            batch.contains(&format!("\"op\":\"{op}\"")),
            "missing {op}: {batch}"
        );
    }
    let after = snapshot(&bridge);
    assert!(after.rows.iter().any(|row| row.index == 500));
    assert!(after.rows.iter().all(|row| row.index > 0));
    assert!(after.rows.len() < 64);
    let host = bridge.host.as_ref().unwrap();
    assert!(host.runner().kernel().find_by_test_id("row-0").is_empty());
    assert_eq!(host.runner().last_instance_work().rows_keyed, 0);
    assert_eq!(host.engine().now(), 0.01);
    let mut measurement = facts(&after, 16_000.);
    measurement.measurements.push(RowMeasurement {
        view: after.rows[0].view,
        epoch: after.rows[0].epoch,
        size: 43.,
    });
    let batch = send(&mut bridge, &measurement, 20.);
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert!(snapshot(&bridge)
        .rows
        .iter()
        .any(|row| row.size == 43. && row.measured));
    assert_eq!(queries.get(), queries_before);
}

#[test]
fn collection_stale_feedback_changes_neither_tree_nor_motion_clock() {
    let (mut bridge, _, queries) = fixture(SOURCE);
    let old = facts(&snapshot(&bridge), 16_000.);
    send(&mut bridge, &old, 10.);
    let before = state(&bridge);
    let count = queries.get();
    let batch = send(&mut bridge, &old, 1000.);
    assert!(batch.starts_with("{\"ops\":[]"), "{batch}");
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(state(&bridge), before);
    let mut unknown = facts(&snapshot(&bridge), 0.);
    unknown.view = u32::MAX;
    assert!(send(&mut bridge, &unknown, 1000.).starts_with("{\"ops\":[]"));
    assert_eq!(state(&bridge), before);
    assert_eq!(queries.get(), count);
}

#[test]
fn collection_input_bounds_and_malformed_bytes_are_nonmutating() {
    let (mut bridge, _, queries) = fixture(SOURCE);
    let bytes = facts(&snapshot(&bridge), 16_000.).encode().unwrap();
    let before = state(&bridge);
    let count = queries.get();
    for len in [0, bytes.len() - 1, bytes.len() + 1, usize::MAX] {
        bridge.input_write(&bytes);
        let len = bridge.collection_feedback(len, 1000.);
        let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize));
        assert!(batch.starts_with("{\"ops\":[]"), "{batch}");
        assert!(batch.contains("malformed collection feedback"), "{batch}");
        assert_eq!(state(&bridge), before);
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    let mut nonfinite = bytes.clone();
    nonfinite[24..32].copy_from_slice(&f64::NAN.to_le_bytes());
    for malformed in [trailing, nonfinite] {
        let len = bridge.input_write(&malformed);
        let len = bridge.collection_feedback(len, 1000.);
        let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize));
        assert!(batch.contains("malformed collection feedback"), "{batch}");
        assert_eq!(state(&bridge), before);
    }
    assert_eq!(queries.get(), count);
    assert!(!bridge.host.as_ref().unwrap().runner().is_poisoned());
    // Rejection never consumes otherwise-valid feedback.
    let feedback = facts(&snapshot(&bridge), 16_000.);
    let batch = send(&mut bridge, &feedback, 20.);
    assert!(batch.contains("\"op\":\"collections\""), "{batch}");
    assert_eq!(bridge.host.as_ref().unwrap().engine().now(), 0.02);
}

#[test]
fn collection_feedback_rejects_invalid_time_before_mutation() {
    let (mut bridge, _, _) = fixture(SOURCE);
    let feedback = facts(&snapshot(&bridge), 16_000.);
    let before = state(&bridge);
    for time in [
        f64::NAN,
        f64::INFINITY,
        -1.0,
        exact_runner::MAX_CLOCK_MS + 1.,
    ] {
        let batch = send(&mut bridge, &feedback, time);
        assert!(batch.starts_with("{\"ops\":[]"), "{batch}");
        assert!(
            batch.contains("invalid collection feedback time"),
            "{batch}"
        );
        assert_eq!(state(&bridge), before);
    }
}

/// A commit that moves a list's rows has the presenter ride the move
/// (`"rides"`), as a collection view's batch update moves its cells; a
/// report that moves nothing and a resize, which takes places without
/// motion, do not.
#[test]
fn a_commit_that_moves_a_lists_rows_has_them_ride_the_move() {
    let source = SOURCE
        .replace("action toggle", "state tall = false\n  action grow\n    tall = true\n  action toggle")
        .replace(
            "      if showing\n",
            "      button press=grow testId=\"grow\"\n        text \"grow\"\n      if showing\n",
        )
        .replace(
            "            text `${row}` testId=`row-${row}`",
            "            column height=(tall and row == 0 ? 80 : 20)\n              text `${row}` testId=`row-${row}`",
        );
    let (mut bridge, _, _) = fixture(&source);
    let list = snapshot(&bridge).view;
    let rides = format!("\"rides\":[{list}]");
    // Reports measuring the mounted rows until the window rests: the last one moves no row
    let mut rested = String::new();
    for now in 1..=8 {
        let before = snapshot(&bridge);
        rested = send(&mut bridge, &measured(&before, 0.), f64::from(now) / 10.);
        if snapshot(&bridge).revision == before.revision {
            break;
        }
    }
    assert!(!rested.contains("\"rides\""), "nothing moved: {rested}");
    let host = bridge.host.as_ref().unwrap();
    let kernel = host.runner().kernel();
    let grow = kernel
        .node_by_key(kernel.find_by_test_id("grow")[0])
        .unwrap()
        .id;
    let len = bridge.dispatch(grow, 0, 0, 3.);
    let grown = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(grown.contains(&rides), "the first row grew: {grown}");
    let len = bridge.resize(400., 600.);
    let resized = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(!resized.contains("\"rides\""), "{resized}");
}

#[test]
fn a_list_rides_only_once_its_first_rows_have_settled() {
    // A list at the top and one that opens at its end, each with a row its
    // commits grow and shrink in its first window
    for end in [false, true] {
        let source = SOURCE
            .replace(
                "action toggle",
                "state tall = false\n  action grow\n    tall = true\n  action shrink\n    tall = false\n  action toggle",
            )
            .replace(
                "      if showing\n",
                "      button press=grow testId=\"grow\"\n        text \"grow\"\n      button press=shrink testId=\"shrink\"\n        text \"shrink\"\n      if showing\n",
            )
            .replace(
                "list virtualized=true",
                if end { "list virtualized=true scroll-start=\"end\"" } else { "list virtualized=true" },
            )
            .replace(
                "            text `${row}` testId=`row-${row}`",
                "            column height=(tall and (row == 0 or row == 999) ? 80 : 20)\n              text `${row}` testId=`row-${row}`",
            );
        let (mut bridge, _, _) = fixture(&source);
        let list = snapshot(&bridge).view;
        let press = |bridge: &mut Bridge<Rows>, id: &str, now: f64| {
            let kernel = bridge.host.as_ref().unwrap().runner().kernel();
            let node = kernel
                .node_by_key(kernel.find_by_test_id(id)[0])
                .unwrap()
                .id;
            let len = bridge.dispatch(node, 0, 0, now);
            String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned()
        };
        let rides = |out: &str| out.contains(&format!("\"rides\":[{list}]"));
        // Before any report the list is opening
        assert!(
            !rides(&press(&mut bridge, "grow", 0.1)),
            "end {end}: before any report"
        );
        // A report that measured nothing has not settled its rows
        let before = snapshot(&bridge);
        let top = before.correction.as_ref().map_or(0., |c| c.offset);
        send(&mut bridge, &facts(&before, top), 0.2);
        assert!(
            !rides(&press(&mut bridge, "shrink", 0.3)),
            "end {end}: rows not yet measured"
        );
        // Reports measuring the rows, at the corrections they ask for, until they rest
        for now in 4..=16 {
            let before = snapshot(&bridge);
            let top = before.correction.as_ref().map_or(top, |c| c.offset);
            send(&mut bridge, &measured(&before, top), f64::from(now) / 10.);
            if snapshot(&bridge).revision == before.revision && snapshot(&bridge).live {
                break;
            }
        }
        assert!(snapshot(&bridge).live, "end {end}: settled");
        assert!(
            rides(&press(&mut bridge, "grow", 2.0)),
            "end {end}: the first settled edit rides"
        );
    }
}

#[test]
fn a_list_settles_on_the_report_whose_measurements_grew_it() {
    // A list at its top whose rows measure 24 points, not the estimate: each
    // report measures the rows it has not measured yet, as a host's does. A
    // host sends nothing more for a port that has not moved once every row is
    // measured, so the list must be live by the report that measured its last
    // row, not wait for one more
    let (mut bridge, _, _) = fixture(SOURCE);
    // (The first report sets the list's width, and takes no measurement.)
    let width = facts(&snapshot(&bridge), 0.);
    send(&mut bridge, &width, 0.05);
    let extent = snapshot(&bridge).total_extent;
    for now in 1..=8 {
        let before = snapshot(&bridge);
        let mut report = facts(&before, 0.);
        report.measurements = before
            .rows
            .iter()
            .filter(|row| !row.measured)
            .map(|row| RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: 24.,
            })
            .collect();
        assert!(
            !report.measurements.is_empty(),
            "every row measured, and still not live"
        );
        send(&mut bridge, &report, f64::from(now) / 10.);
        if snapshot(&bridge).live {
            break;
        }
    }
    let after = snapshot(&bridge);
    assert!(
        (after.total_extent - extent).abs() > 0.5,
        "the measurements changed the extent"
    );
    assert!(
        after.live,
        "settled on the report that measured the last row"
    );
}

#[test]
fn a_list_settles_only_on_a_report_of_its_port_at_rest() {
    use exact_runner::CollectionFill;
    // A host animating a smooth correction reports where the port is headed,
    // not where it shows, so rows measured then have not shown settled. The
    // report at rest settles the list though it measures nothing new: it
    // only changes the host's motion, a report the runner takes by its
    // travel alone
    let (mut bridge, _, _) = fixture(SOURCE);
    let width = facts(&snapshot(&bridge), 0.);
    send(&mut bridge, &width, 0.05);
    let moving = |bridge: &mut Bridge<Rows>, report: &CollectionFeedback, now: f64| {
        let fill = CollectionFill {
            animating: true,
            ..CollectionFill::default()
        };
        let bytes = report.encode_with(fill).unwrap();
        bridge.input_write(&bytes);
        bridge.collection_feedback(bytes.len(), now);
    };
    for now in 1..=8 {
        let before = snapshot(&bridge);
        moving(&mut bridge, &measured(&before, 0.), f64::from(now) / 10.);
        assert!(!snapshot(&bridge).live, "report {now}: the port is moving");
        if snapshot(&bridge).revision == before.revision {
            break;
        }
    }
    let rest = facts(&snapshot(&bridge), 0.);
    send(&mut bridge, &rest, 1.0);
    assert!(snapshot(&bridge).live, "the port at rest settles it");
}

/// `facts` measuring every mounted row at its laid-out size: 20 points, or
/// 80 for a row the fixture's `tall` grows.
fn measured(snapshot: &CollectionSnapshot, top: f64) -> CollectionFeedback {
    let mut f = facts(snapshot, top);
    f.measurements = snapshot
        .rows
        .iter()
        .map(|row| RowMeasurement {
            view: row.view,
            epoch: row.epoch,
            size: 20.,
        })
        .collect();
    f
}

#[test]
fn collection_removal_clears_snapshot_and_recreation_rejects_old_feedback() {
    let (mut bridge, _, _) = fixture(SOURCE);
    let old = facts(&snapshot(&bridge), 16_000.);
    let host = bridge.host.as_ref().unwrap();
    let kernel = host.runner().kernel();
    let toggle = kernel
        .node_by_key(kernel.find_by_test_id("toggle")[0])
        .unwrap()
        .id;
    let len = bridge.dispatch(toggle, 0, 0, 10.);
    let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize));
    assert!(
        batch.contains("{\"op\":\"collections\",\"items\":[]}"),
        "{batch}"
    );
    assert!(bridge
        .host
        .as_ref()
        .unwrap()
        .runner()
        .collections()
        .is_empty());
    bridge.dispatch(toggle, 0, 0, 20.);
    assert_ne!(snapshot(&bridge).view, old.view);
    let before = state(&bridge);
    assert!(send(&mut bridge, &old, 1000.).starts_with("{\"ops\":[]"));
    assert_eq!(state(&bridge), before);
}

#[test]
fn ordinary_eager_list_has_no_collection_metadata() {
    let (mut bridge, batch, _) = fixture(&SOURCE.replace("virtualized=true", "virtualized=false"));
    assert!(!batch.contains("\"op\":\"collections\""));
    let host = bridge.host.as_ref().unwrap();
    assert!(host.runner().collections().is_empty());
    assert_eq!(host.runner().kernel().find_by_test_id("row-999").len(), 1);
    let before = state(&bridge);
    let feedback = CollectionFeedback {
        view: 1,
        revision: 0,
        scroll_sequence: 1,
        offset: 10.,
        port_cross: 240.,
        port_main: 160.,
        cross: 240.,
        padding: [0.0; 2],
        measurements: vec![],
        focus_view: None,
        interaction_view: None,
    };
    assert!(send(&mut bridge, &feedback, 1000.).starts_with("{\"ops\":[]"));
    assert_eq!(state(&bridge), before);
}

#[test]
fn collection_end_follow_is_runner_owned_while_eager_lists_keep_the_native_prop() {
    let source = SOURCE.replace("height=160", "scrollFollowEnd=true height=160");
    let (bridge, batch, _) = fixture(&source);
    let host = bridge.host.as_ref().unwrap();
    let node = host.runner().kernel().node(snapshot(&bridge).view).unwrap();
    assert_eq!(
        node.props.bool(exact_kernel::PropId::ScrollFollowEnd),
        Some(true)
    );
    assert!(!batch.contains("\"scrollFollowEnd\""), "{batch}");
    let (_, eager_batch, _) = fixture(&source.replace("virtualized=true", "virtualized=false"));
    assert!(
        eager_batch.contains("\"scrollFollowEnd\":\"true\""),
        "{eager_batch}"
    );
}

// Instantiate the actual export, including the runtime identity/reentrancy guard.
#[allow(unsafe_code)]
mod exports {
    use super::*;
    crate::host!(Rows, &[], "{}");

    #[test]
    fn collection_export_uses_the_runtime_buffer_and_refuses_dead_or_busy_handles() {
        let rt = exact_create();
        let (bridge, _, _) = fixture(SOURCE);
        let bytes = facts(&snapshot(&bridge), 16_000.).encode().unwrap();
        with_entry(&EXACT_RUNTIMES, rt, |entry| {
            entry.bridge = bridge;
            entry.bridge.input_write(&bytes);
        });
        let len = exact_collection_feedback(rt, bytes.len(), 10.);
        let entry = EXACT_RUNTIMES.with(|r| r.borrow().get(rt)).unwrap();
        let guard = entry.borrow();
        assert!(
            String::from_utf8_lossy(guard.bridge.output_bytes(len as usize))
                .contains("\"op\":\"collections\"")
        );
        let before = state(&guard.bridge);
        exact_collection_feedback(rt, bytes.len(), 1000.);
        REFUSAL.with(|r| assert!(String::from_utf8_lossy(&r.borrow()).contains("busy")));
        assert_eq!(state(&guard.bridge), before);
        drop(guard);
        exact_destroy(rt);
        exact_collection_feedback(rt, bytes.len(), 1000.);
        REFUSAL.with(|r| assert!(String::from_utf8_lossy(&r.borrow()).contains("no such runtime")));
    }

    /// `exact_press_writes_router` (LLP 1127 D16): a row that pushes, a
    /// button that only counts, and a text with no press answer 1, 0 and 2;
    /// a busy or gone runtime answers 2, never its refusal's length.
    #[test]
    fn press_writes_router_export_answers_each_press_and_two_when_refused() {
        const APP: &str = r#"routes nav
  list "/"
  item "/item/:id"

component App
  state refreshes = 0
  action open(id: string)
    nav = push(nav, path("item", id))
  action refresh
    refreshes = refreshes + 1
  view
    column navigationBack="back"
      for e in stack(nav) key=e.id
        if e.name == "list"
          column navigationKey=`${e.id}`
            button press=open("a") testId="row"
              text "A"
            button press=refresh testId="refresh"
              text "Refresh"
            text `${refreshes}` testId="count"
"#;
        let mut bridge = Bridge::<Rows>::new();
        bridge.boot(
            &contract::compile(APP).unwrap().encode(),
            Rows::default(),
            Hooks::none(),
            400.,
            800.,
        );
        let kernel = bridge.host.as_ref().unwrap().runner().kernel();
        let views = ["row", "refresh", "count"]
            .map(|t| kernel.node_by_key(kernel.find_by_test_id(t)[0]).unwrap().id);
        let rt = exact_create();
        with_entry(&EXACT_RUNTIMES, rt, |entry| entry.bridge = bridge);
        assert_eq!(
            views.map(|view| exact_press_writes_router(rt, view)),
            [1, 0, 2]
        );
        let entry = EXACT_RUNTIMES.with(|r| r.borrow().get(rt)).unwrap();
        let busy = entry.borrow();
        assert_eq!(exact_press_writes_router(rt, views[0]), 2, "busy");
        drop(busy);
        exact_destroy(rt);
        assert_eq!(exact_press_writes_router(rt, views[0]), 2, "gone");
    }
}

/// Rows a list report measures move the rows after them; a row's
/// `-exact-layout-transition` does not play that move (the list's offset keeps what
/// shows still), as UIKit's self-sizing does not animate.
#[test]
fn rows_a_report_moves_do_not_play_their_layout_transition() {
    let source = SOURCE.replace(
        "text `${row}` testId=`row-${row}`",
        "column -exact-layout-transition=\"300ms ease-in-out\"\n              text `${row}` testId=`row-${row}`",
    );
    let (mut bridge, _, _) = fixture(&source);
    let before = snapshot(&bridge);
    send(&mut bridge, &facts(&before, 16_000.), 10.);
    let after = snapshot(&bridge);
    let mut measurement = facts(&after, 16_000.);
    for (i, row) in after.rows.iter().enumerate().take(4) {
        measurement.measurements.push(RowMeasurement {
            view: row.view,
            epoch: row.epoch,
            size: 43. + i as f64,
        });
    }
    let batch = send(&mut bridge, &measurement, 20.);
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert!(
        bridge.host.as_ref().unwrap().engine().quiescent(),
        "no row is moving: {batch}"
    );
    assert!(
        !batch.contains("\"layout\""),
        "no layout presented: {batch}"
    );
}

/// Rows a list's data or its author moves still play their
/// `-exact-layout-transition`: a row put first, two rows swapped (the count kept),
/// a row with no transition of its own growing, a row's box nudged in its
/// wrapper, the list's padding swapped. Another list on the page, whose data did not change, keeps
/// still throughout.
#[test]
fn rows_the_data_moves_still_play_their_layout_transition() {
    struct Shifted;
    impl DataSource for Shifted {
        fn query(&mut self, _: &str, args: &[Value]) -> Result<Value, DataError> {
            let step = match args.first() {
                Some(Value::Number(n)) => *n as i64,
                _ => 0,
            };
            let rows: Vec<i64> = match step {
                0 => (0..40).collect(),
                1 => (-1..40).collect(),
                _ => [0, -1].into_iter().chain(1..40).collect(),
            };
            Ok(Value::list(
                rows.into_iter().map(|i| Value::Number(i as f64)).collect(),
            ))
        }
    }
    let source = r#"component App
  state step = 0
  state tall = false
  state nudged = false
  state padded = false
  resource rows = rows(step) as shape number[]
  resource others = rows(0) as shape number[]
  action next
    step = step + 1
  action grow
    tall = true
  action nudge
    nudged = true
  action pad
    padded = true
  view
    column
      button press=next testId="next"
        text "next"
      button press=grow testId="grow"
        text "grow"
      button press=nudge testId="nudge"
        text "nudge"
      button press=pad testId="pad"
        text "pad"
      list virtualized=true height=200 width=240 padding-left=(padded ? 20 : 0) padding-right=(padded ? 0 : 20) testId="edited"
        for row in rows key=row
          column height=(tall and row == 0 ? 80 : 20) width=200 position="relative" left=(nudged and row == 1 ? 20 : 0) -exact-layout-transition=(row == 0 ? "none" : "300ms ease-in-out")
            text `${row}`
      list virtualized=true height=200 width=240 testId="other"
        for row in others key=row
          column -exact-layout-transition="300ms ease-in-out"
            text `${row}`
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        Shifted,
        Box::new(MonospaceMeasurer::default()),
        400.,
        600.,
    )
    .unwrap();
    for list in host.runner().collections() {
        let mut first = facts(&list, 0.);
        first.port_main = 200.;
        host.collection_feedback(&first.encode().unwrap(), 10.);
    }
    let id = |host: &Host<Shifted>, test: &str| {
        let kernel = host.runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(test)[0])
            .unwrap()
            .id
    };
    // The row box of a list's mounted row at `index`.
    let row_box = |host: &Host<Shifted>, list: &str, index: usize| {
        let snapshot = host.runner().collection(id(host, list)).unwrap();
        let row = snapshot.rows.iter().find(|r| r.index == index).unwrap();
        exact_kernel::motion::motion_node(host.runner().kernel().node(row.root).unwrap().key)
    };
    let layout = exact_motion::Property::Layout;
    let (other, mut now) = (row_box(&host, "other", 1), 20.);
    // Each step: the button, the row (by index after it) that must move, what it is.
    for (button, index, what) in [
        ("next", 2, "a row put first"),
        ("next", 1, "two rows swapped"),
        (
            "grow",
            1,
            "the row before it, which has no transition, grown",
        ),
        ("nudge", 2, "its box nudged in its wrapper"),
        ("pad", 1, "the list's padding swapped"),
    ] {
        let generation = |host: &Host<Shifted>| host.runner().collection_data();
        let before = generation(&host);
        let batch = host.dispatch_at(id(&host, button), Event::Press, now);
        assert!(batch.contains("\"error\":null"), "{batch}");
        // Only a change of keys moves a list's data generation: a body that
        // reads changing state (a clock) every commit does not.
        assert_eq!(
            generation(&host) != before,
            button == "next",
            "{what}: the generation"
        );
        let engine = host.engine();
        assert!(
            engine.is_active(row_box(&host, "edited", index), layout),
            "{what}: the row moves: {batch}"
        );
        assert!(
            !engine.is_active(other, layout),
            "{what}: the other list keeps still"
        );
        now += 1000.;
        host.advance(now);
    }
}

/// A row rebound to another item (LLP 1078) keeps its views: the batch says
/// `renew` for each before any props or style for it, and creates none of
/// them; off, as by default, the same scroll says `renew` for none.
#[test]
fn a_rebound_row_is_renewed_before_its_props_and_never_created() {
    let ops = |batch: &str| -> Vec<(String, u64)> {
        let v: serde_json::Value = serde_json::from_str(batch).unwrap();
        v["ops"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| {
                let id = op["id"].as_u64().unwrap_or(0);
                (op["op"].as_str().unwrap().to_owned(), id)
            })
            .collect()
    };
    for reuse in [false, true] {
        let (mut bridge, _, _) = fixture(SOURCE);
        bridge.set_row_reuse(reuse);
        let top = facts(&snapshot(&bridge), 4_000.);
        let batch = send(&mut bridge, &top, 10.);
        assert!(batch.contains("\"error\":null"), "{batch}");
        let ops = ops(&batch);
        let renewed: Vec<u64> = ops
            .iter()
            .filter(|(op, _)| op == "renew")
            .map(|(_, id)| *id)
            .collect();
        if !reuse {
            assert!(renewed.is_empty(), "off: {batch}");
            continue;
        }
        assert!(renewed.len() > 10, "rows were rebound: {batch}");
        for id in &renewed {
            let at = ops.iter().position(|o| o.0 == "renew" && o.1 == *id);
            for (i, (op, of)) in ops.iter().enumerate() {
                if of != id {
                    continue;
                }
                assert_ne!(op, "create", "a renewed view is not created: {batch}");
                assert_ne!(op, "destroy", "nor destroyed: {batch}");
                if op == "props" || op == "style" {
                    assert!(at < Some(i), "renew comes before its {op}: {batch}");
                }
            }
        }
        let kernel = bridge.host.as_ref().unwrap().runner().kernel();
        assert!(kernel.find_by_test_id("row-0").is_empty());
        assert!(snapshot(&bridge).rows.iter().any(|row| row.index == 125));
    }
}

/// A row rebound while the colour its inline text inherits is moving: the
/// text is the same for the new item, so nothing restates its paragraph but
/// the renewal, which paints it again in the colour the new row starts at.
#[test]
fn a_rebound_paragraph_is_painted_again_without_the_old_rows_moving_colour() {
    const SOURCE: &str = r##"
component App
  state on = false
  resource rows = rows() as shape number[]
  action toggle
    on = not on
  view
    column
      button press=toggle testId="toggle"
        text "toggle"
      list virtualized=true height=160 width=240 testId="history"
        for row in rows key=row
          column color=(on ? "#ffffff" : "#000000") transition="color 10s linear"
            text testId="para"
              text "plain"
"##;
    let (mut bridge, _, _) = fixture(SOURCE);
    bridge.set_row_reuse(true);
    let host = bridge.host.as_mut().unwrap();
    let toggle = {
        let k = host.runner().kernel();
        k.node_by_key(k.find_by_test_id("toggle")[0]).unwrap().id
    };
    host.dispatch_at(toggle, Event::Press, 0.);
    let mid = host.advance(500.);
    assert!(
        mid.contains("\"paragraph\""),
        "the runs' colour moves: {mid}"
    );
    let top = facts(&snapshot(&bridge), 4_000.);
    let batch = send(&mut bridge, &top, 600.);
    let v: serde_json::Value = serde_json::from_str(&batch).unwrap();
    let ops = v["ops"].as_array().unwrap();
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let paragraphs: Vec<u64> = ops
        .iter()
        .filter(|op| op["op"] == "renew")
        .filter_map(|op| op["id"].as_u64())
        .filter(|id| {
            kernel
                .node(*id as u32)
                .and_then(|n| n.props.str(exact_kernel::PropId::TestId).map(str::to_owned))
                .is_some_and(|t| t == "para")
        })
        .collect();
    assert!(!paragraphs.is_empty(), "rows were rebound: {batch}");
    for id in paragraphs {
        let painted = ops
            .iter()
            .find(|op| op["op"] == "paragraph" && op["id"].as_u64() == Some(id))
            .unwrap_or_else(|| panic!("paragraph {id} is painted again: {batch}"));
        let colour = &painted["runs"][0]["style"]["text_color"];
        assert_eq!(
            colour,
            &serde_json::json!([255, 255, 255, 255]),
            "{painted}"
        );
    }
}

#[test]
fn fixed_collection_geometry_reports_the_shared_flow_extents() {
    let source = include_str!("../../../contract/corpus/list-fixed.contract");
    let (mut bridge, _, _) = fixture(source);
    let snapshot = snapshot(&bridge);
    let lead = snapshot.fixed[0].roots.len() as u32;
    let trail = snapshot.fixed[1].roots.len() as u32;
    let sizes = [0, 1].map(|side| bridge.collection_fixed_size(snapshot.view, lead, trail, side));
    assert_eq!(sizes[0], 95.0);
    assert!(sizes[1] >= 300.0);
    assert!(bridge
        .collection_fixed_size(snapshot.view, lead, trail, 2)
        .is_nan());
    assert!(bridge
        .collection_fixed_size(snapshot.view, u32::MAX, trail, 0)
        .is_nan());
    let mut report = facts(&snapshot, 0.0);
    report.measurements.extend(
        snapshot
            .fixed
            .iter()
            .enumerate()
            .map(|(side, f)| RowMeasurement {
                view: f.view,
                epoch: 1,
                size: sizes[side],
            }),
    );
    let batch = send(&mut bridge, &report, 1.0);
    assert!(batch.contains("\"error\":null"), "{batch}");
}

#[test]
fn a_build_waits_for_validated_host_measurements_without_retirement() {
    use exact_runner::CollectionFill;
    let (mut bridge, _, _) = fixture(SOURCE);
    let first = snapshot(&bridge);
    send(&mut bridge, &facts(&first, 0.), 0.);
    let first = snapshot(&bridge);
    let before: Vec<_> = first.rows.iter().map(|r| r.view).collect();
    let bytes = facts(&first, 1600.)
        .encode_with(CollectionFill {
            create_only: true,
            limit: None,
            ..CollectionFill::default()
        })
        .unwrap();
    let writes = bridge.host.as_ref().unwrap().batch_writes;
    bridge.input_write(&bytes);
    let len = bridge.collection_feedback(bytes.len(), 0.);
    let reply = String::from_utf8_lossy(bridge.output_bytes(len as usize));
    assert!(reply.contains("\"error\":null"), "{reply}");
    let built = snapshot(&bridge);
    let added: Vec<_> = built
        .rows
        .iter()
        .filter(|row| !before.contains(&row.view))
        .collect();
    assert!(!added.is_empty());
    assert!(added.iter().all(|row| !row.measured));
    assert!(built.window.is_none(), "host measurement is still owed");
    assert!(
        before
            .iter()
            .all(|view| built.rows.iter().any(|r| r.view == *view)),
        "a build cannot retire"
    );
    assert_eq!(
        bridge.host.as_ref().unwrap().batch_writes - writes,
        1,
        "one published commit"
    );
    let row = added[0];
    let mut measurement = facts(&built, 1600.);
    measurement.measurements.push(RowMeasurement {
        view: row.view,
        epoch: row.epoch,
        size: 43.,
    });
    let before = state(&bridge);
    for (revision, sequence) in [
        (built.revision - 1, measurement.scroll_sequence),
        (built.revision, built.scroll_sequence - 1),
    ] {
        let mut stale = measurement.clone();
        stale.revision = revision;
        stale.scroll_sequence = sequence;
        let reply = send(&mut bridge, &stale, 1000.);
        assert!(reply.contains("\"error\":null"), "{reply}");
        assert_eq!(state(&bridge), before);
    }
    let mut bytes = measurement.encode().unwrap();
    let flags = u32::from_le_bytes(bytes[92..96].try_into().unwrap()) | 1024;
    bytes[92..96].copy_from_slice(&flags.to_le_bytes());
    bridge.input_write(&bytes);
    let len = bridge.collection_feedback(bytes.len(), 1000.);
    let reply = String::from_utf8_lossy(bridge.output_bytes(len as usize));
    assert!(reply.contains("malformed collection feedback"), "{reply}");
    assert_eq!(
        state(&bridge),
        before,
        "unknown flags cannot bypass validation"
    );
    let reply = send(&mut bridge, &measurement, 0.);
    assert!(reply.contains("\"error\":null"), "{reply}");
    assert!(snapshot(&bridge)
        .rows
        .iter()
        .any(|r| r.view == row.view && r.measured && r.size == 43.));
}

#[test]
fn leaving_an_edge_publishes_its_rearmed_window_without_a_view_commit() {
    let source = include_str!("../tests/fixtures/quiet-edge.contract");
    let (mut bridge, _, _) = fixture(source);
    for _ in 0..8 {
        let current = snapshot(&bridge);
        if current.window.is_some() {
            break;
        }
        let mut initial = facts(&current, 0.);
        initial.port_main = 300.;
        initial.measurements = current
            .rows
            .iter()
            .map(|row| RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: 100.,
            })
            .collect();
        send(&mut bridge, &initial, 0.);
    }
    assert!(snapshot(&bridge).window.is_some(), "all rows are measured");
    for (offset, reaches) in [(201., 1.), (200., 1.), (201., 2.)] {
        let before = snapshot(&bridge);
        let mut report = facts(&before, offset);
        report.port_main = 300.;
        let batch = send(&mut bridge, &report, 0.);
        let host = bridge.host.as_ref().unwrap();
        assert_eq!(host.runner().slot("reaches"), Some(&Value::Number(reaches)));
        if offset == 200. {
            let batch: serde_json::Value = serde_json::from_str(&batch).unwrap();
            let ops = batch["ops"].as_array().unwrap();
            assert_eq!(ops.len(), 1, "only the edge metadata changes: {batch}");
            assert_eq!(ops[0]["op"], "collections");
            let mask = ops[0]["items"][0]["window"]["mask"].as_u64().unwrap();
            assert_ne!(mask & 8, 0, "the presenter can report the next entry");
            assert_eq!(snapshot(&bridge).revision, before.revision);
            assert_eq!(snapshot(&bridge).rows, before.rows);
        }
    }
}

#[test]
fn quiet_travel_consumes_no_feedback_and_preserves_the_next_batch() {
    let (mut bridge, _, _) = fixture(SOURCE);
    let first = facts(&snapshot(&bridge), 320.0);
    send(&mut bridge, &first, 0.0);
    let before = snapshot(&bridge);
    bridge
        .host
        .as_mut()
        .unwrap()
        .runner_mut()
        .log("kept across quiet travel");
    let writes = bridge.host.as_ref().unwrap().batch_writes;
    let output = bridge.output_bytes(bridge.output.len()).to_vec();
    let travel = facts(&before, 325.0);
    bridge.input_write(
        &[
            travel.revision.to_le_bytes(),
            travel.scroll_sequence.to_le_bytes(),
        ]
        .concat(),
    );
    bridge.scrolled(2, travel.view, 0.0, travel.offset);
    let after = snapshot(&bridge);
    assert_eq!(after.rows, before.rows);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.scroll_sequence, travel.scroll_sequence);
    assert_eq!(bridge.host.as_ref().unwrap().batch_writes, writes);
    assert_eq!(bridge.output_bytes(output.len()), output);
    bridge.input_write(&[0; 15]);
    bridge.scrolled(2, travel.view, 0.0, 900.0);
    assert_eq!(snapshot(&bridge).scroll_sequence, travel.scroll_sequence);
    let batch = send(&mut bridge, &facts(&after, 325.0), 0.0);
    assert!(batch.contains("kept across quiet travel"), "{batch}");
}

#[test]
fn a_backward_build_waits_for_native_sizes_before_anchoring() {
    for horizontal in [true, false] {
        backward_build_waits_for_intrinsic("input type=\"checkbox\" role=\"switch\"", horizontal);
    }
}

#[test]
fn a_backward_build_waits_for_replaced_sizes_before_anchoring() {
    for item in [
        "image",
        "image min-width=40",
        "image min-height=40",
        "image width=\"max-content\" height=\"min-content\"",
        "video",
        "svg",
        "canvas",
        "iframe",
    ] {
        for horizontal in [true, false] {
            backward_build_waits_for_intrinsic(item, horizontal);
        }
    }
    backward_build_waits_for_intrinsic("image aspect-ratio=1", true);
}

fn backward_build_waits_for_intrinsic(item: &str, horizontal: bool) {
    use exact_runner::CollectionFill;
    let layout = if horizontal {
        r#"display="flex" width=240 height=240 overflow-x="scroll" overflow-y="hidden" estimated-item-width=100"#
    } else {
        r#"width=240 height=240 overflow-y="scroll" estimated-item-height=100"#
    };
    let source = format!(
        r#"component App
  view
    list virtualized=true {layout}
      for n in range(100) key=n
        {item} flex-shrink=0 align-self="start"
"#
    );
    let (mut bridge, _, _) = fixture(&source);
    let mut report = facts(&snapshot(&bridge), 3000.);
    report.port_main = 240.;
    report.port_cross = 240.;
    report.cross = 240.;
    send(&mut bridge, &report, 0.);
    let before = snapshot(&bridge);
    let sizes: Vec<_> = before
        .rows
        .iter()
        .map(|r| (r.root, Some((100., 100.))))
        .collect();
    let reply = bridge.host.as_mut().unwrap().set_intrinsics(&sizes);
    assert!(reply.contains("\"error\":null"), "{reply}");
    let measurement = |host: &Host<Rows>, row: &exact_runner::CollectionRow| {
        let frame = host.runner().kernel().node(row.view).unwrap().frame;
        RowMeasurement {
            view: row.view,
            epoch: row.epoch,
            size: f64::from(if horizontal {
                frame.width
            } else {
                frame.height
            }),
        }
    };
    // A fresh report acknowledges only rows already on main.
    report = CollectionFeedback {
        measurements: before
            .rows
            .iter()
            .map(|r| measurement(bridge.host.as_ref().unwrap(), r))
            .collect(),
        ..facts(&before, 3000.)
    };
    report.port_main = 240.;
    report.port_cross = 240.;
    report.cross = 240.;
    send(&mut bridge, &report, 0.);
    let before = snapshot(&bridge);
    let offset = 2700.;
    let anchor = before
        .rows
        .iter()
        .find(|r| r.start + r.size > offset)
        .unwrap()
        .view;
    let main = |frame: exact_kernel::Frame| if horizontal { frame.x } else { frame.y } as f64;
    let start = main(
        bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .kernel()
            .node(anchor)
            .unwrap()
            .frame,
    );
    let mut build = facts(&before, offset);
    build.port_main = 240.;
    build.port_cross = 240.;
    build.cross = 240.;
    let bytes = build
        .encode_with(CollectionFill {
            create_only: true,
            limit: None,
            ..CollectionFill::default()
        })
        .unwrap();
    bridge.input_write(&bytes);
    bridge.collection_feedback(bytes.len(), 0.);
    let built = snapshot(&bridge);
    let added: Vec<_> = built
        .rows
        .iter()
        .filter(|r| !before.rows.iter().any(|old| old.view == r.view))
        .collect();
    assert!(
        !added.is_empty(),
        "the reversal builds rows before the visible window"
    );
    assert!(
        added.iter().all(|r| !r.measured),
        "{item}, horizontal={horizontal}: a host fallback is not a measurement"
    );
    assert!(
        added.iter().all(|r| r.size > 0.),
        "unresolved rows keep positive estimates"
    );
    assert!(
        before
            .rows
            .iter()
            .all(|r| built.rows.iter().any(|now| now.view == r.view)),
        "a backward build cannot retire"
    );
    let host = bridge.host.as_mut().unwrap();
    let sizes: Vec<_> = added.iter().map(|r| (r.root, Some((80., 80.)))).collect();
    let reply = host.set_intrinsics(&sizes);
    assert!(reply.contains("\"error\":null"), "{reply}");
    let sized = snapshot(&bridge);
    for row in &added {
        let current = sized.rows.iter().find(|r| r.view == row.view).unwrap();
        assert!(!current.measured);
        assert_eq!(
            current.size, row.size,
            "layout cannot acknowledge a row size"
        );
    }
    let mut measured = build;
    measured.revision = built.revision;
    measured.scroll_sequence = built.scroll_sequence + 1;
    measured.measurements = added
        .iter()
        .map(|r| measurement(bridge.host.as_ref().unwrap(), r))
        .collect();
    let reply = send(&mut bridge, &measured, 0.);
    assert!(reply.contains("\"error\":null"), "{reply}");
    let after = snapshot(&bridge);
    let host = bridge.host.as_ref().unwrap();
    let end = main(host.runner().kernel().node(anchor).unwrap().frame);
    let corrected = after.correction.as_ref().map_or(offset, |c| c.offset);
    assert!(
        (end - corrected - (start - offset)).abs() < 0.01,
        "{item}, horizontal={horizontal}: measurements moved the visible row: {start} -> {end}, correction {corrected}"
    );
}
