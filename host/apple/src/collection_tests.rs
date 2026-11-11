//! Apple collection transport and host commits, using the actual runner window.
use super::*;
use exact_runner::{CollectionFeedback, CollectionSnapshot, DataError, RowMeasurement, Value};
use std::cell::Cell as Counter;

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
fn collection_boot_and_carried_boot_fill_the_supplied_viewport_in_the_first_batch() {
    let source = r#"component App
  view
    list appearance="none" virtualized=true estimated-item-height=100 scroll-start="end" width="100%" height="100%"
      for row in range(1000) key=row
        text `${row}` height=100
"#;
    let plan = contract::compile(source).unwrap().encode();
    let boot = |height| {
        Host::boot(
            &plan,
            (),
            Box::new(MonospaceMeasurer::default()),
            200.,
            height,
        )
        .unwrap()
    };
    let (small, small_batch) = boot(300.);
    let (large, large_batch) = boot(900.);
    let (replacement, replacement_batch) = Host::boot_with(
        &plan,
        (),
        Box::new(MonospaceMeasurer::default()),
        200.,
        450.,
        Some(&large.carry()),
    )
    .unwrap();
    for (host, batch, count) in [
        (&small, small_batch, 3),
        (&large, large_batch, 9),
        (&replacement, replacement_batch, 5),
    ] {
        assert_eq!(host.runner().kernel().epoch(), 1);
        let snapshot = host.runner().collections().remove(0);
        assert_eq!(snapshot.rows.len(), count);
        assert_eq!(snapshot.rows[0].index, 1000 - count);
        assert_eq!(snapshot.rows.last().unwrap().index, 999);
        assert!(snapshot
            .rows
            .iter()
            .all(|row| { host.runner().kernel().node(row.view).unwrap().frame.height == 100. }));
        let envelope: serde_json::Value = serde_json::from_str(&batch).unwrap();
        assert!(envelope["error"].is_null(), "{batch}");
        let collections = envelope["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| op["op"] == "collections")
            .collect::<Vec<_>>();
        assert_eq!(collections.len(), 1);
        assert_eq!(
            collections[0]["items"][0]["rows"].as_array().unwrap().len(),
            count
        );
    }
}

#[test]
fn collection_feedback_submits_edge_request_without_another_bridge_call() {
    use exact_runner::{Answer, Outcome, Request, Store};
    use std::sync::mpsc::{channel, Sender};
    use std::time::Duration;

    // This fixture needs an admitted executor; unrelated parallel Bridge tests
    // can exhaust the process-wide worker cap before feedback submits its work.
    const CHILD: &str = "EXACT_COLLECTION_FEEDBACK_TEST";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "abi::collection_tests::collection_feedback_submits_edge_request_without_another_bridge_call"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

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
                Ok(Answer::Later(Request::continuation(7)))
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

/// Each mounted row of `snapshot` starts where the layout put it in its list.
fn assert_rows_where_laid_out(bridge: &Bridge<Rows>, snapshot: &CollectionSnapshot) {
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let list = kernel.node(snapshot.view).unwrap().frame;
    for row in &snapshot.rows {
        let laid = kernel.node(row.view).unwrap().frame;
        assert!(row.measured, "{row:?}");
        assert!(
            (f64::from(laid.y - list.y) - row.start).abs() < 0.01,
            "{row:?} laid out at {laid:?} in a list at {list:?}"
        );
    }
}

/// LLP 1072 §8.7: the batch that lays rows out at sizes other than the
/// list's estimate carries those sizes, so a list opening at its end is told
/// the end they make in that batch, not after the presenter's next report.
#[test]
fn a_batch_that_lays_rows_out_carries_the_end_their_sizes_make() {
    let source = SOURCE.replace(
        "list virtualized=true",
        "list virtualized=true scroll-start=\"end\" estimated-item-height=40",
    );
    let (mut bridge, _, _) = fixture(&source);
    let opened = snapshot(&bridge);
    let end = opened.correction.expect("told to start at the end").offset;
    let batch = send(&mut bridge, &facts(&opened, end), 1.);
    assert!(batch.contains("\"error\":null"), "{batch}");
    let after = snapshot(&bridge);
    assert_rows_where_laid_out(&bridge, &after);
    // The port's 160 points end at the rows' end
    let correction = after.correction.expect("the end the sizes make");
    assert!(
        (correction.offset - (after.total_extent - 160.)).abs() < 0.01,
        "{correction:?} for an extent of {}",
        after.total_extent
    );
    assert!(
        correction.from.is_none(),
        "an end is absolute: {correction:?}"
    );
}

/// The same for a row the reader is at: rows mounted above it at their own
/// sizes move it in the layout, and the batch's correction keeps it at the
/// port's top, relative to where the port was.
#[test]
fn a_batch_that_lays_rows_out_above_the_anchor_keeps_it_in_place() {
    let source = SOURCE.replace(
        "list virtualized=true",
        "list virtualized=true estimated-item-height=40",
    );
    let (mut bridge, _, _) = fixture(&source);
    // Row 400 at the port's top by the estimate
    let report = facts(&snapshot(&bridge), 16_000.);
    let batch = send(&mut bridge, &report, 1.);
    assert!(batch.contains("\"error\":null"), "{batch}");
    let after = snapshot(&bridge);
    assert_rows_where_laid_out(&bridge, &after);
    let anchor = after
        .rows
        .iter()
        .find(|row| row.index == 400)
        .expect("row 400 mounted");
    assert!(
        after.rows.iter().any(|row| row.index < 400),
        "rows mounted above it"
    );
    let correction = after.correction.expect("the anchor's correction");
    assert!(
        (correction.offset - anchor.start).abs() < 0.01,
        "{correction:?} for row 400 at {}",
        anchor.start
    );
    assert_eq!(
        correction.from,
        Some(16_000.),
        "relative to where the port was"
    );
}

/// The same for a row a commit adds and takes into view (a sent message
/// at a followed end): the batch aims at the end the row's laid-out size
/// makes, not its estimate, which the presenter would scroll to first and
/// then come back from on the next report.
#[test]
fn a_batch_that_lays_a_row_out_aims_its_scroll_into_view_at_its_size() {
    aims_a_new_row_into_view_at_its_size("");
}

/// The same on a list that follows its end smoothly: the commit plans the
/// end's smooth correction before the request, and that correction, which
/// no host has seen, is not the host animating, so the request aims at once.
#[test]
fn a_smooth_end_the_same_commit_planned_does_not_hold_its_scroll_into_view() {
    aims_a_new_row_into_view_at_its_size(" scrollFollowEnd=true scroll-behavior=\"smooth\"");
}

fn aims_a_new_row_into_view_at_its_size(list: &str) {
    let source = SOURCE
        .replace(
            "action toggle",
            "state count = 30\n  action add\n    let n = count\n    count = n + 1\n    scrollIntoView(\"history\", n, block=\"end\")\n  action toggle",
        )
        .replace(
            "      if showing\n",
            "      button press=add testId=\"add\"\n        text \"add\"\n      if showing\n",
        )
        .replace(
            "list virtualized=true",
            &format!("list id=\"history\" virtualized=true scroll-start=\"end\" estimated-item-height=40{list}"),
        )
        .replace("for row in rows key=row", "for row in range(count) key=row");
    let (mut bridge, _, _) = fixture(&source);
    // Reports until the window rests at the end
    let mut top = 0.;
    for now in 1..=8 {
        let before = snapshot(&bridge);
        top = before.correction.map_or(top, |c| c.offset);
        send(&mut bridge, &facts(&before, top), f64::from(now) / 10.);
        if snapshot(&bridge).revision == before.revision {
            break;
        }
    }
    let host = bridge.host.as_ref().unwrap();
    let kernel = host.runner().kernel();
    let add = kernel
        .node_by_key(kernel.find_by_test_id("add")[0])
        .unwrap()
        .id;
    let len = bridge.dispatch(add, 0, 0, 1.);
    let added = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(added.contains("\"error\":null"), "{added}");
    let after = snapshot(&bridge);
    assert_eq!(after.count, 31);
    assert_eq!(after.rows.last().map(|r| r.index), Some(30), "{after:?}");
    assert_rows_where_laid_out(&bridge, &after);
    let correction = after.correction.expect("the new row's end");
    assert!(
        (correction.offset - (after.total_extent - 160.)).abs() < 0.01,
        "{correction:?} for an extent of {}",
        after.total_extent
    );
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
    // Reports until the window rests: the last one moves no row
    let mut rested = String::new();
    for now in 1..=8 {
        let before = snapshot(&bridge);
        rested = send(&mut bridge, &facts(&before, 0.), f64::from(now) / 10.);
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
fn a_sequence_only_report_does_not_resend_collections_on_a_scroll_commit() {
    let source = SOURCE.replace("action toggle", "state scrolled = 0\n  action scroll(left: number, top: number)\n    scrolled = scrolled + 1\n  action toggle")
        .replace("list virtualized=true", "list scroll=scroll virtualized=true");
    let (mut bridge, _, _) = fixture(&source);
    // Reports until the window rests: each measures the rows it mounts, and
    // rows shorter than the estimate leave the window more rows to build
    let mut before = snapshot(&bridge);
    for now in 1..=8 {
        send(&mut bridge, &facts(&before, 0.), f64::from(now) / 10.);
        let next = snapshot(&bridge);
        let rested = next.revision == before.revision;
        before = next;
        if rested {
            break;
        }
    }
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
    // The batch that laid the rows out measured them at those sizes
    let row = &after.rows[0];
    let laid = host.runner().kernel().node(row.view).unwrap().frame.height;
    assert!(row.measured, "{row:?}");
    assert!(
        (row.size - f64::from(laid)).abs() < 1e-4,
        "{row:?} laid out at {laid}"
    );
    // The presenter's report of that size, as it reads it, changes nothing
    let mut measurement = facts(&after, 16_000.);
    measurement.measurements.push(RowMeasurement {
        view: row.view,
        epoch: row.epoch,
        size: row.size,
    });
    let batch = send(&mut bridge, &measurement, 20.);
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(snapshot(&bridge).rows[0].size, row.size);
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

/// A list opening at its end rides none of the moves its rows make while it
/// opens, a report's or a commit's: they land where the end puts them, as
/// its corrections do. Once it has opened, a commit's move rides.
#[test]
fn a_list_opening_at_its_end_rides_no_move_until_it_has_opened() {
    let source = r#"
component App
  state start = 950
  resource rows = rows() as shape number[]
  action older
    start = start - 50
  view
    column
      button press=older testId="older"
        text "older"
      list appearance="none" virtualized=true estimated-item-height=68 scroll-start="end" height=160 width=240 testId="history"
        for row in rows.slice(start) key=row
          text `${row}` testId=`row-${row}`
"#;
    let (mut bridge, _, _) = fixture(source);
    let list = snapshot(&bridge).view;
    let rides = format!("\"rides\":[{list}]");
    let older = {
        let kernel = bridge.host.as_ref().unwrap().runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id("older")[0])
            .unwrap()
            .id
    };
    let opened = snapshot(&bridge);
    assert!(opened.opening);
    let at = opened.correction.expect("the end").offset;
    let reported = send(&mut bridge, &facts(&opened, at), 0.1);
    assert!(
        !reported.contains("\"rides\""),
        "a report as it opens: {reported}"
    );
    let len = bridge.dispatch(older, 0, 0, 0.2);
    let pressed = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(snapshot(&bridge).opening);
    assert!(!pressed.contains(&rides), "a commit as it opens: {pressed}");
    // Reports at the end until it has opened
    for now in 3..=12 {
        let before = snapshot(&bridge);
        if !before.opening {
            break;
        }
        let end = before
            .correction
            .map_or(before.total_extent - 160., |c| c.offset);
        send(&mut bridge, &facts(&before, end), f64::from(now) / 10.);
    }
    assert!(!snapshot(&bridge).opening);
    let len = bridge.dispatch(older, 0, 0, 1.5);
    let pressed = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(
        pressed.contains(&rides),
        "a commit once it has opened: {pressed}"
    );
}
