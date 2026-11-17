//! The canvas seam (LLP 1009 D2/D3): a `canvas` node reaches the page as a
//! `<canvas>` with the web's default size, and its surface's inputs —
//! arguments evaluated against state — arrive as `surface` ops, at boot
//! and whenever they change, never otherwise.

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
        "/../../contract/corpus/canvas.contract"
    ))
    .unwrap();
    let plan = contract::compile(&src).unwrap();
    exact_web::link(exact_web_capabilities::ALL);
    Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap()
}

fn view(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner().kernel();
    let key = k.find_by_test_id(test_id)[0];
    k.node_by_key(key).unwrap().id
}

#[test]
fn a_canvas_is_a_canvas_element_with_the_webs_default_size() {
    let (host, batch) = boot();
    let bare = view(&host, "bare");
    assert!(
        batch.contains(&format!(
            "\"op\":\"create\",\"id\":{bare},\"tag\":\"canvas\""
        )),
        "{batch}"
    );
    let at = batch
        .find(&format!("\"id\":{bare},\"tag\":\"canvas\""))
        .unwrap();
    let create = &batch[at..at + 400];
    // Its `div` sizes as a <canvas>: the natural 300×150 and 2:1, not rows.
    assert!(!create.contains("width:300px"), "{create}");
    assert!(
        create.contains("contain:size;justify-self:start;contain-intrinsic-size:300px 150px;aspect-ratio:auto 2/1;min-height:min-content;"),
        "a bare canvas is 300×150, as on the web: {create}"
    );
    let map = view(&host, "map");
    assert!(
        batch.contains(&format!(
            "\"op\":\"surface\",\"id\":{map},\"name\":\"map\",\"values\":[1,\"caltrain\"]}}"
        )),
        "{batch}"
    );
    // The sky (LLP 1014): a canvas with children, its surface's one input
    // a string from state.
    let sky = view(&host, "sky");
    assert!(
        batch.contains(&format!(
            "\"op\":\"surface\",\"id\":{sky},\"name\":\"aurora\",\"values\":[\"1\"]}}"
        )),
        "{batch}"
    );
    assert_eq!(
        batch.matches("\"op\":\"surface\"").count(),
        2,
        "the bare canvas has no surface; `map` and `sky` do"
    );
}

#[test]
fn surface_inputs_are_published_when_they_change_and_only_then() {
    let (mut host, _) = boot();
    let map = view(&host, "map");
    let zoom = view(&host, "zoom");
    let quiet = host.advance(1000.0);
    assert!(
        !quiet.contains("\"op\":\"surface\""),
        "nothing changed: {quiet}"
    );
    let batch = host.dispatch(zoom, Event::Press);
    assert!(
        batch.contains(&format!(
            "\"op\":\"surface\",\"id\":{map},\"name\":\"map\",\"values\":[2,\"caltrain\"]}}"
        )),
        "{batch}"
    );
    assert_eq!(host.runner().plan().surfaces.len(), 2);
}

#[test]
fn an_absolute_canvas_restyles_when_its_parents_justify_items_changes() {
    let plan = contract::compile(
        r#"component App
  state centered = false
  action toggle
    centered = not centered
  view
    box testId="parent" press=toggle position="relative" width=100 height=20 justify-items=(centered ? "center" : "normal")
      canvas testId="canvas" position="absolute" width=20 height=10
"#,
    )
    .unwrap();
    exact_web::link(exact_web_capabilities::ALL);
    let (mut host, first) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
    let (parent, canvas) = (view(&host, "parent"), view(&host, "canvas"));
    for (op, batch, forced) in std::iter::once(("create", first, true)).chain(
        [false, true]
            .into_iter()
            .map(|forced| ("style", host.dispatch(parent, Event::Press), forced)),
    ) {
        let batch: serde_json::Value = serde_json::from_str(&batch).unwrap();
        let style = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["op"] == op && row["id"] == canvas)
            .unwrap_or_else(|| panic!("no canvas {op}: {batch}"));
        let css = style["css"].as_str().unwrap();
        assert!(css.contains("position:absolute;"), "{css}");
        assert_eq!(css.contains("justify-self:start;"), forced, "{css}");
    }
}

#[test]
fn the_caltrain_app_boots_with_its_gpu_surface_and_a_2d_map() {
    let plan = caltrain::build().unwrap();
    exact_web::link(exact_web_capabilities::ALL);
    let (_, batch) = Host::boot(
        &plan.encode(),
        caltrain_data::Caltrain,
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(
        batch.contains("\"name\":\"glass\",\"values\":[\"glass\",\"mv\"]"),
        "{}",
        &batch[batch.len().saturating_sub(600)..]
    );
    // The line map is a Canvas 2D surface (LLP 1056): never a GPU surface
    // op; the page watches its box, and it draws once the box is known.
    assert!(
        batch.contains("\"op\":\"canvas2d\"") && batch.contains("\"watch\":true"),
        "the map's canvas is watched"
    );
    assert!(!batch.contains("\"name\":\"map\""));
    assert_eq!(batch.matches("\"op\":\"surface\"").count(), 1);
}

#[test]
fn r13_named_empty_and_short_contract_calls_survive_web_batch() {
    for (call, values) in [
        (
            "world(restart=false, seed=7, paused=true)",
            r#"{"restart":false,"seed":7,"paused":true}"#,
        ),
        ("world()", "{}"),
        ("world(7)", "[7]"),
    ] {
        let plan = contract::compile(&format!(
            "component App\n  view\n    canvas surface={call}\n"
        ))
        .unwrap();
        exact_web::link(exact_web_capabilities::ALL);
        let (_, batch) =
            exact_web::Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
        assert!(batch.contains(&format!("\"values\":{values}")), "{batch}");
    }
}

#[test]
fn a_same_action_rust_capture_waits_for_the_resized_browser_box() {
    use exact_runner::{Answer, Request, Store};
    struct Ink;
    impl DataSource for Ink {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Later(Request::capture_canvas(
                "ink",
                "app:/tmp/shot.png",
            )))
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("ink".into(), 0)]
        }
        fn draw_2d(
            &mut self,
            _: &str,
            _: &[Value],
            ctx: &exact_runner::exact_canvas::Context2d,
            frame: &exact_runner::exact_canvas::Frame,
        ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
            ctx.set_fill_style_str("red");
            ctx.fill_rect(0., 0., frame.width, frame.height);
            Ok(false)
        }
    }
    for superseded in [false, true] {
        let plan = contract::compile(
            r#"component App
  state width = 100
  mutation image as shape string
  action capture
    width = 200
    send image = capture()
  action resize
    width = 300
  view
    column
      button press=capture testId="capture"
        text "Capture"
      button press=resize testId="resize"
        text "Resize"
      canvas surface=ink() width=width height=50 testId="ink"
"#,
        )
        .unwrap();
        exact_web::link(exact_web_capabilities::ALL);
        let (mut host, _) = Host::boot(&plan.encode(), Ink, Default::default(), "/").unwrap();
        host.data_ready();
        let id = |host: &Host<Ink>, name: &str| {
            let k = host.runner().kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        };
        let canvas = id(&host, "ink");
        host.canvas_geometry(canvas, 100., 50., 2.);
        let button = id(&host, "capture");
        let batch: serde_json::Value =
            serde_json::from_str(&host.dispatch(button, Event::Press)).unwrap();
        assert!(
            !batch["ops"]
                .as_array()
                .unwrap()
                .iter()
                .any(|op| op["op"] == "canvasCapture"),
            "the old geometry cannot decide the capture: {batch}"
        );
        let prepare = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["op"] == "canvasCapturePrepare")
            .unwrap();
        let ticket = prepare["ticket"].as_u64().unwrap();
        if superseded {
            let resize = id(&host, "resize");
            host.dispatch(resize, Event::Press);
        }
        let draw: serde_json::Value = serde_json::from_str(&host.canvas_geometry(
            canvas,
            if superseded { 300. } else { 200. },
            50.,
            2.,
        ))
        .unwrap();
        let fresh = draw["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["op"] == "canvas2d" && op["fresh"] == true)
            .unwrap();
        assert_eq!(fresh["generation"], 1);
        assert_eq!(fresh["w"], if superseded { 600 } else { 400 });
        let answer: serde_json::Value =
            serde_json::from_str(&host.canvas_capture_ready(ticket)).unwrap();
        let capture = answer["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["op"] == "canvasCapture")
            .unwrap();
        assert_eq!(capture["generation"], 1);
        assert_eq!(capture["w"], if superseded { 600 } else { 400 });
        assert_eq!(capture["h"], 100);
        assert_eq!(capture["scale"], 2);
    }
}

#[test]
fn a_frames_capture_survives_its_animation_draw() {
    struct Animated;
    impl DataSource for Animated {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(
            &mut self,
            _: &mut exact_runner::Store,
            _: &str,
            _: &[Value],
        ) -> Result<exact_runner::Answer, DataError> {
            Ok(exact_runner::Answer::Later(
                exact_runner::Request::capture_canvas("ink", "app:/tmp/frame.png"),
            ))
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("ink".into(), 0)]
        }
        fn draw_2d(
            &mut self,
            _: &str,
            _: &[Value],
            ctx: &exact_runner::exact_canvas::Context2d,
            _: &exact_runner::exact_canvas::Frame,
        ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
            ctx.fill_rect(0., 0., 4., 4.);
            Ok(true)
        }
    }
    let plan = contract::compile("component App\n  mutation image as shape string\n  action capture\n    send image = capture()\n  task save mount\n    every(frame, capture)\n  view\n    canvas surface=ink() width=40 height=40 testId=\"ink\"\n").unwrap();
    exact_web::link(exact_web_capabilities::ALL);
    let (mut host, _) = Host::boot(&plan.encode(), Animated, Default::default(), "/").unwrap();
    host.data_ready();
    let k = host.runner().kernel();
    let id = k.node_by_key(k.find_by_test_id("ink")[0]).unwrap().id;
    host.canvas_geometry(id, 40., 40., 1.);
    let batch: serde_json::Value = serde_json::from_str(&host.frame(16.)).unwrap();
    let ticket = batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["op"] == "canvasCapturePrepare")
        .unwrap()["ticket"]
        .as_u64()
        .unwrap();
    let capture = host.canvas_capture_ready(ticket);
    assert!(capture.contains("\"op\":\"canvasCapture\""), "{capture}");
}

#[test]
fn a_capture_retries_canvases_skipped_by_the_draw_byte_budget() {
    struct Budget;
    impl DataSource for Budget {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(
            &mut self,
            _: &mut exact_runner::Store,
            _: &str,
            _: &[Value],
        ) -> Result<exact_runner::Answer, DataError> {
            Ok(exact_runner::Answer::Later(
                exact_runner::Request::capture_canvas("ink", "app:/tmp/ink.png"),
            ))
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("large".into(), 1), ("ink".into(), 0)]
        }
        fn draw_2d(
            &mut self,
            name: &str,
            _: &[Value],
            ctx: &exact_runner::exact_canvas::Context2d,
            _: &exact_runner::exact_canvas::Frame,
        ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
            for _ in 0..if name == "large" { 220_000 } else { 1 } {
                ctx.fill_rect(0., 0., 1., 1.);
            }
            Ok(false)
        }
    }
    for count in [2, 3, 17] {
        let canvases = (0..count)
            .map(|i| {
                format!("      canvas surface=large(n) width=10 height=10 testId=\"large{i}\"\n")
            })
            .collect::<String>();
        let plan = contract::compile(&format!("component App\n  state n = 0\n  mutation image as shape string\n  action capture\n    n = 1\n    send image = capture()\n  view\n    column\n      button press=capture testId=\"capture\"\n        text \"Capture\"\n{canvases}      canvas surface=ink() width=10 height=10 testId=\"ink\"\n")).unwrap();
        exact_web::link(exact_web_capabilities::ALL);
        let (mut host, _) = Host::boot(&plan.encode(), Budget, Default::default(), "/").unwrap();
        host.data_ready();
        let id = |host: &Host<Budget>, name: &str| {
            let k = host.runner().kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        };
        let mut views = (0..count)
            .map(|i| id(&host, &format!("large{i}")))
            .collect::<Vec<_>>();
        views.push(id(&host, "ink"));
        for view in &views {
            host.canvas_geometry(*view, 10., 10., 1.);
        }
        let button = id(&host, "capture");
        let first: serde_json::Value =
            serde_json::from_str(&host.dispatch(button, Event::Press)).unwrap();
        let ops = first["ops"].as_array().unwrap();
        let draws = ops
            .iter()
            .filter(|op| op["op"] == "canvas2d")
            .collect::<Vec<_>>();
        assert_eq!(
            draws.len(),
            1,
            "the first canvas exhausts the turn's budget"
        );
        assert!(
            draws[0]["lists"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| l[1].as_u64().unwrap())
                .sum::<u64>()
                > 8 << 20
        );
        let ticket = ops
            .iter()
            .find(|op| op["op"] == "canvasCapturePrepare")
            .unwrap()["ticket"]
            .as_u64()
            .unwrap();
        // Unchanged browser geometry produces no extra turn before this callback.
        let mut draws = Vec::new();
        loop {
            let ready: serde_json::Value =
                serde_json::from_str(&host.canvas_capture_ready(ticket)).unwrap();
            let ops = ready["ops"].as_array().unwrap();
            let capture = ops.iter().position(|op| op["op"] == "canvasCapture");
            let turn = ops[..capture.unwrap_or(ops.len())]
                .iter()
                .filter(|op| op["op"] == "canvas2d")
                .map(|op| op["id"].as_u64().unwrap() as u32)
                .collect::<Vec<_>>();
            assert!(turn.len() <= 16, "one callback keeps its retry bound");
            draws.extend(turn);
            if capture.is_some() {
                break;
            }
            assert!(
                ops.iter()
                    .any(|op| op["op"] == "canvasCapturePrepare" && op["ticket"] == ticket),
                "budget-deferred capture schedules the next callback without another event"
            );
            assert!(draws.len() < views.len(), "every callback makes progress");
        }
        assert_eq!(
            draws,
            views[1..],
            "every retry is replayed before the snapshot"
        );
        assert!(host.canvas_snapshotted(ticket));
    }
}

#[test]
fn a_rejected_capture_draw_answers_instead_of_parking() {
    use exact_runner::{Answer, DrawReply, DrawRequest, Drawn, Request, Store};
    struct Rejected(bool);
    impl DataSource for Rejected {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            self.0 = true;
            Ok(Answer::Later(Request::capture_canvas(
                "ink",
                "app:/tmp/shot.png",
            )))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: exact_runner::Outcome,
        ) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::str(
                &exact_runner::canvas_capture::captured_image(&outcome).unwrap_err(),
            )))
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("ink".into(), 0)]
        }
        fn draw(
            &mut self,
            _: &DrawRequest<'_>,
            _: &exact_runner::exact_canvas::Context2d,
        ) -> Drawn {
            Drawn::Now(DrawReply {
                lists: if self.0 { vec![vec![0xff; 8]] } else { vec![] },
                ..Default::default()
            })
        }
    }
    let plan = contract::compile("component App\n  mutation image as shape string\n  action capture\n    send image = capture()\n  view\n    column\n      button press=capture testId=\"capture\"\n        text \"Capture\"\n      canvas surface=ink() width=10 height=10 testId=\"ink\"\n").unwrap();
    exact_web::link(exact_web_capabilities::ALL);
    let (mut host, _) =
        Host::boot(&plan.encode(), Rejected(false), Default::default(), "/").unwrap();
    host.data_ready();
    let id = |host: &Host<Rejected>, name: &str| {
        let k = host.runner().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    host.canvas_geometry(id(&host, "ink"), 10., 10., 1.);
    let batch: serde_json::Value =
        serde_json::from_str(&host.dispatch(id(&host, "capture"), Event::Press)).unwrap();
    let ticket = batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .find(|op| op["op"] == "canvasCapturePrepare")
        .unwrap()["ticket"]
        .as_u64()
        .unwrap();
    let ready: serde_json::Value =
        serde_json::from_str(&host.canvas_capture_ready(ticket)).unwrap();
    assert!(
        !host.runner().holds(ticket),
        "a rejected drawing must answer its capture: {ready}"
    );
    assert!(exact_runner::agent::state(host.runner()).contains("EncodingError"));
    assert!(
        !ready["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "canvasCapture"),
        "no PNG may replace the destination"
    );
}
