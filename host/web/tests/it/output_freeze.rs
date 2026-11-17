use exact_runner::{DataError, DataSource, Event, Page, Value};
use exact_web::Host;
use serde_json::{json, Value as Json};

struct Granted;

impl DataSource for Granted {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        if source == "flood" {
            let n = args.first().and_then(Value::as_number).unwrap_or(0.0) as usize;
            return Ok(Value::list(
                [144.0, 62.0, 100.0]
                    .map(Value::Number)
                    .into_iter()
                    .cycle()
                    .take(3 * n)
                    .collect(),
            ));
        }
        Err(DataError::UnknownSource(source.into()))
    }

    fn grants(&self) -> &str {
        "device.midi purpose.midi"
    }
}

fn late_source() -> String {
    source("empty(0)", 0.0, 120.0, "", false).replace(
        "  resource midi = exactMIDI() as shape MIDI\n  action ask\n    requestMIDIAccess()\n  action start\n    playing = true\n  sequence sequence when playing\n    play([Track(beats=4, loops=1, offset=0, output=\"synth\", steps=[empty(0)])], bpm=120, clockOut=\"\")",
        r#"  resource midi = exactMIDI() as shape MIDI
  resource many = flood(1024) as shape number[]
  action ask
    requestMIDIAccess()
  action fill
    sendMIDI("", many, at=performanceNow() + 1000)
  action start
    playing = true
  sequence sequence when playing
    play([Track(beats=10, loops=1, offset=0, output="synth", steps=[Step(beat=2.5, src="", gain=1, group="", note=61, velocity=100, channel=0, duration=0.5, haptic="")]), Track(beats=10, loops=1, offset=-500, output="synth", steps=[note(6, 0.8)])], bpm=600)"#,
    )
}

fn standalone_source() -> &'static str {
    r#"sound "assets/tick.wav"
keyframes leave
  to opacity=0

shape Port
  id: string
  name: string
  state: string

shape MIDI
  state: string
  inputs: Port[]
  outputs: Port[]

component App
  state shown = true
  resource midi = exactMIDI() as shape MIDI
  action ask
    requestMIDIAccess()
  action output
    shown = false
    playSound("assets/tick.wav", at=performanceNow() + 20)
    sendMIDI("synth", [144, 64, 100], at=performanceNow() + 20)
    sendMIDI("synth", [128, 64, 0], at=performanceNow() + 180)
    haptic("impact-light")
  view
    column
      button "Output" press=output testId="output"
      if shown
        text "output" -exact-exit-animation="leave 10s linear both"
      else
        text "done"
"#
}

fn source(steps: &str, offset: f64, bpm: f64, clock: &str, sound: bool) -> String {
    let declaration = if sound {
        "sound \"assets/tick.wav\"\n"
    } else {
        ""
    };
    let midi = if sound {
        ""
    } else {
        "  resource midi = exactMIDI() as shape MIDI\n  action ask\n    requestMIDIAccess()\n"
    };
    format!(
        r#"{declaration}shape Step
  beat: number
  src: string
  gain: number
  group: string
  note: number
  velocity: number
  channel: number
  duration: number
  haptic: string

shape Track
  beats: number
  loops: number
  offset: number
  output: string
  steps: Step[]

shape Port
  id: string
  name: string
  state: string

shape MIDI
  state: string
  inputs: Port[]
  outputs: Port[]

fn note(beat: number, length: number): Step = Step(beat=beat, src="", gain=1, group="", note=60, velocity=100, channel=0, duration=length, haptic="")
fn hit(beat: number): Step = Step(beat=beat, src="assets/tick.wav", gain=1, group="", note=-1, velocity=100, channel=0, duration=0, haptic="impact-light")
fn empty(beat: number): Step = Step(beat=beat, src="", gain=1, group="", note=-1, velocity=100, channel=0, duration=0, haptic="")

component App
  state playing = false
{midi}  action start
    playing = true
  sequence sequence when playing
    play([Track(beats=4, loops=1, offset={offset}, output="synth", steps=[{steps}])], bpm={bpm}, clockOut="{clock}")
  view
    text "output"
"#
    )
}

fn wav(path: &std::path::Path, ms: u32) {
    let frames = 48 * ms;
    let data = frames * 2;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    for value in [1u16, 1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&48_000u32.to_le_bytes());
    bytes.extend_from_slice(&96_000u32.to_le_bytes());
    for value in [2u16, 16] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    bytes.resize(bytes.len() + data as usize, 0);
    std::fs::write(path, bytes).unwrap();
}

fn boot(source: &str, sound: bool) -> Host<Granted> {
    exact_web::link(exact_web_capabilities::ALL);
    let plan = if sound {
        let dir = super::TestDir::new("exact-output-freeze");
        std::fs::create_dir_all(dir.join("assets")).unwrap();
        wav(&dir.join("assets/tick.wav"), 20);
        std::fs::write(dir.join("app.contract"), source).unwrap();
        contract::compile_path(&dir.join("app.contract")).unwrap()
    } else {
        contract::compile(source).unwrap()
    };
    let (mut host, _) = Host::boot(&plan.encode(), Granted, Default::default(), "/").unwrap();
    if source.contains("exactMIDI()") {
        let (_, reply) = host
            .runner_mut()
            .midi_agent(r#"{"op":"prefer","midi":{"outputs":["synth"]}}"#)
            .unwrap();
        reply.unwrap();
        host.runner_mut().act("ask", vec![]).unwrap();
        host.runner_mut().land_then();
        host.advance(0.0);
    }
    host
}

fn start(host: &mut Host<Granted>, at: f64) -> Json {
    action(host, "start", at)
}

fn action(host: &mut Host<Granted>, name: &str, at: f64) -> Json {
    host.advance(at);
    host.runner_mut().act(name, vec![]).unwrap();
    output(&host.advance(at))
}

fn press(host: &mut Host<Granted>, test_id: &str, at: f64) -> Json {
    let view = {
        let kernel = host.runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(test_id)[0])
            .unwrap()
            .id
    };
    output(&host.dispatch_at(view, Event::Press, at))
}

fn live_refill(host: &mut Host<Granted>) -> Json {
    host.runner_mut().act("fill", vec![]).unwrap();
    host.advance(0.0);
    host.runner_mut().act("start", vec![]).unwrap();
    output(&host.advance(200.0))
}

fn output(batch: &str) -> Json {
    let batch: Json = serde_json::from_str(batch).unwrap();
    let ops: Vec<Json> = batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|op| {
            matches!(
                op["op"].as_str(),
                Some("midi" | "sound" | "exit" | "destroy")
            ) || op["op"] == "command" && op["name"] == "haptic"
        })
        .cloned()
        .collect();
    let mut out = json!({ "ops": ops, "clock": batch["clock"] });
    if let Some(freeze) = batch.get("freeze") {
        out["freeze"] = freeze.clone();
    }
    out
}

fn freeze(host: &mut Host<Granted>, at: f64) -> Json {
    host.advance(at);
    output(&host.set_page(Page {
        frozen: true,
        ..host.runner().page()
    }))
}

#[test]
fn a_freeze_batch_is_marked_without_a_page_reader_or_sequence() {
    exact_web::link(exact_web_capabilities::ALL);
    let plan = contract::compile("component App\n  view\n    text \"page\"\n").unwrap();
    let (mut host, _) = Host::boot(&plan.encode(), Granted, Default::default(), "/").unwrap();
    let batch: Json = serde_json::from_str(&host.set_page(Page {
        frozen: true,
        ..host.runner().page()
    }))
    .unwrap();
    assert_eq!(batch.get("freeze"), Some(&Json::Bool(true)));

    let unchanged: Json = serde_json::from_str(&host.set_page(host.runner().page())).unwrap();
    assert_eq!(unchanged.get("freeze"), None);
}

#[test]
fn the_output_freeze_fixture_matches_the_web_runner_output() {
    let mut overlap = boot(
        &source("note(0, 0.8), note(0.2, 0.8)", 100.0, 120.0, "", false),
        false,
    );
    let mut late = boot(&late_source(), false);
    let mut short = boot(&source("note(0, 0.16)", 100.0, 120.0, "", false), false);
    let mut clock = boot(
        &source("empty(0)", 0.0, 120.0, "synth", false).replace("beats=4", "beats=0.5"),
        false,
    );
    let mut sound = boot(&source("hit(0)", 100.0, 120.0, "", true), true);
    let mut standalone = boot(standalone_source(), true);
    let standalone_scheduled = press(&mut standalone, "output", 100.0);
    let standalone_freeze = freeze(&mut standalone, 150.0);
    let standalone_resume = {
        standalone.advance(200.0);
        output(&standalone.set_page(Page {
            frozen: false,
            ..standalone.runner().page()
        }))
    };
    let standalone_fresh = press(&mut standalone, "output", 200.0);
    let actual = json!({
        "overlap": start(&mut overlap, 0.0),
        "late": live_refill(&mut late),
        "short": { "scheduled": start(&mut short, 0.0), "freeze": freeze(&mut short, 150.0) },
        "clock": { "scheduled": start(&mut clock, 0.0), "freeze": freeze(&mut clock, 150.0) },
        "sound": { "scheduled": start(&mut sound, 0.0), "freeze": freeze(&mut sound, 150.0) },
        "standalone": {
            "scheduled": standalone_scheduled,
            "freeze": standalone_freeze,
            "resume": standalone_resume,
            "fresh": standalone_fresh,
        },
    });
    let expected: Json =
        serde_json::from_str(include_str!("../fixtures/output-freeze.json")).unwrap();
    assert_eq!(actual, expected);
}
