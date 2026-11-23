//! Each session's canvases back at its own display scale (LLP 1056 D4): a
//! window on another display, or at another zoom, leaves the others' alone.
use super::{Bridge, Hooks};
use exact_plan::Value;
use exact_runner::exact_canvas::{Context2d, DrawError, Frame};
use exact_runner::{DataError, DataSource};

struct Tile {
    wants_frame: bool,
}
impl DataSource for Tile {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        match (source, args) {
            ("save", [value]) => Ok(value.clone()),
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("tile".into(), 0)]
    }
    fn draw_2d(
        &mut self,
        _: &str,
        _: &[Value],
        ctx: &Context2d,
        frame: &Frame,
    ) -> Result<bool, DrawError> {
        ctx.fill_rect(0., 0., frame.width, frame.height);
        Ok(self.wants_frame)
    }
}

const TILE: &str = r#"component App
  view
    canvas surface=tile() width=100 height=50
"#;

/// The canvas ops of the bridge's last reply: (scale, width, generation).
fn canvases(bridge: &Bridge<Tile>, len: u32) -> Vec<(f64, u64, u64)> {
    let batch: serde_json::Value =
        serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
    batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| op["op"] == "canvas2d")
        .map(|op| {
            let n = |k: &str| op[k].as_f64().unwrap();
            (n("scale"), n("w") as u64, n("generation") as u64)
        })
        .collect()
}

/// Some canvas op, and each is `want`: (scale, width, generation).
fn drawn(ops: &[(f64, u64, u64)], want: (f64, u64, u64)) {
    assert!(
        !ops.is_empty() && ops.iter().all(|&op| op == want),
        "{ops:?}, not {want:?}"
    );
}

/// No canvas op, or each at `scale` and `generation`.
fn kept(ops: &[(f64, u64, u64)], scale: f64, generation: u64) {
    assert!(
        ops.iter().all(|&(s, _, g)| s == scale && g == generation),
        "{ops:?}"
    );
}

/// `f` on `bridge` within its session's role scope, as `with_runtime` runs a call.
fn on(bridge: &mut Bridge<Tile>, f: impl FnOnce(&mut Bridge<Tile>) -> u32) -> Vec<(f64, u64, u64)> {
    let roles = bridge.roles();
    let _roles = roles.as_ref().map(exact_kernel::style::roles::bind);
    let len = f(bridge);
    canvases(bridge, len)
}

const MEMORY: f64 = (8u64 << 30) as f64;

#[test]
fn a_sessions_display_scale_leaves_another_sessions_canvases_alone() {
    let plan = contract::compile(TILE).unwrap().encode();
    let boot = |scale: f64| {
        let mut bridge = Bridge::<Tile>::new();
        bridge.canvas_display(scale, MEMORY);
        let ops = on(&mut bridge, |b| {
            b.boot(
                &plan,
                Tile { wants_frame: false },
                Hooks::none(),
                390.0,
                844.0,
            )
        });
        (bridge, ops)
    };
    let (mut a, ops) = boot(2.0);
    drawn(&ops, (2.0, 200, 0));
    let (mut b, ops) = boot(1.0);
    drawn(&ops, (1.0, 100, 0));

    // A's next draw is at its own scale: no new generation.
    kept(&on(&mut a, Bridge::canvas_draw), 2.0, 0);

    // A moves to a 3× display: only A's canvas is backed again.
    drawn(
        &on(&mut a, |a| a.canvas_display(3.0, MEMORY)),
        (3.0, 300, 1),
    );
    kept(&on(&mut b, Bridge::canvas_draw), 1.0, 0);

    // And B's change leaves A at 3×.
    drawn(
        &on(&mut b, |b| b.canvas_display(1.5, MEMORY)),
        (1.5, 150, 1),
    );
    kept(&on(&mut a, Bridge::canvas_draw), 3.0, 1);
}

#[test]
fn draining_a_queue_leaves_the_canvass_next_frame_for_the_display() {
    let plan = contract::compile(
        r#"component App
  mutation saved as shape number queue
  action saveBoth
    send saved = save(1)
    send saved = save(2)
  view
    column
      button press=saveBoth testId="save"
        text "Save"
      canvas surface=tile() width=100 height=50
"#,
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Tile { wants_frame: true }, Hooks::none(), 390., 844.);
    let runner = bridge.host.as_ref().unwrap().runner();
    assert!(runner.canvas_wants_frame());
    let view = runner.kernel().find_by_test_id("save")[0];
    let view = runner.kernel().node_by_key(view).unwrap().id;
    assert!(on(&mut bridge, |b| b.dispatch(view, 0, 0, 10.)).is_empty());
    let answer = |bridge: &Bridge<Tile>| {
        bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .slot("saved")
            .cloned()
    };
    assert_eq!(answer(&bridge), Some(Value::some(Value::Number(1.))));
    let drained = on(&mut bridge, |b| b.advance(0., 4));
    assert_eq!(answer(&bridge), Some(Value::some(Value::Number(2.))));
    assert!(drained.is_empty(), "the queue drain drew a canvas frame");
    assert!(
        !on(&mut bridge, |b| b.frame(10.)).is_empty(),
        "the display still owes the frame"
    );
}
