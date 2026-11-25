//! A transform pair's release velocity is measured over the times its input
//! was sampled, not the clock's (LLP 1057.001 §3).
use exact_apple::Host;
use exact_kernel::MonospaceMeasurer;
use exact_runner::{DataError, DataSource};

struct NoData;
impl DataSource for NoData {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

/// The v3 packet: version/op, runtime/handle/target/clip/sequence and the two
/// tokens, six values, the clock and the sample's input time.
fn packet(op: u32, identity: [u64; 7], values: [f64; 6], now: f64, at: f64) -> Vec<u8> {
    let mut bytes = [3u32, op].map(u32::to_le_bytes).concat();
    for word in identity {
        bytes.extend(word.to_le_bytes());
    }
    for value in values.into_iter().chain([now, at]) {
        bytes.extend(value.to_le_bytes());
    }
    bytes
}

#[test]
fn transform_release_velocity_follows_the_input_time_not_the_clock() {
    let plan = contract::compile(
        r#"component App
  state vx = 0
  action geometry(w: number, h: number, pw: number, ph: number)
    vx = 0
  action finish(px: number, py: number, s: number, x: number, y: number, vs: number)
    vx = x
  view
    column testId="clip" width=320 height=200 overflow="hidden" padding=0 border-width=0
      column id="photo" testId="photo" width="100%" height="100%" box-sizing="border-box" padding=0 border-width=0 translate="0px 0px" scale=1 transition="translate -exact-spring(180, 12, 1), scale -exact-spring(180, 12, 1)"
        column testId="handle" transformDragFor="photo" transformgeometry=geometry transformrelease=finish
"#,
    )
    .unwrap()
    .encode();
    let clocks: [fn(f64) -> f64; 2] = [|_| 1000., |at| 1000. + 3. * at];
    for clock in clocks {
        let (mut h, batch) = Host::boot(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            400.,
            800.,
        )
        .unwrap();
        let batch: serde_json::Value = serde_json::from_str(&batch).unwrap();
        let binding = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["op"] == "transform-drag")
            .unwrap_or_else(|| panic!("{batch}"));
        let [runtime, handle, target, clip] = ["runtime", "handleKey", "targetKey", "clipKey"]
            .map(|key| binding[key].as_str().unwrap().parse::<u64>().unwrap());
        let mut send = |op, tokens: [u64; 2], values, now, at| -> serde_json::Value {
            let identity = [runtime, handle, target, clip, 1, tokens[0], tokens[1]];
            let reply = h
                .transform_motion(&packet(op, identity, values, now, at))
                .finish();
            let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
            assert_eq!(reply["accepted"], true, "{reply}");
            reply
        };
        send(10, [0; 2], [320., 200., 320., 200., 0., 0.], 1000., 1000.);
        let begun = send(11, [0; 2], [0., 0., 1., 0., 0., 0.], 1000., 1000.);
        let tokens = ["translateToken", "scaleToken"]
            .map(|key| begun[key].as_str().unwrap().parse::<u64>().unwrap());
        // 60 points across in 40 ms, sampled every 10 ms from where the drag began.
        for i in 1..=4 {
            let at = 10. * i as f64;
            send(
                12,
                tokens,
                [1.5 * at, 0., 1., 0., 0., 0.],
                clock(at),
                1000. + at,
            );
        }
        let released = send(13, tokens, [60., 0., 1., 0., 0., 0.], clock(40.), 1040.);
        let vx = released["velocity"][0].as_f64().unwrap();
        assert!((vx - 1500.).abs() < 1e-6, "{released}");
        assert_eq!(
            h.runner().slot("vx"),
            Some(&exact_runner::Value::Number(vx))
        );
    }
}
