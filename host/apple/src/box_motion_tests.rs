//! LLP 1055.001 on iOS: a box's `translate`, `scale`, `rotate` and
//! `background-color` keyframes go to Core Animation where it plays them as
//! CSS does, and the engine keeps no frames busy for them; the rest are
//! sampled as before. The host here is flipped to iOS's lowering after boot.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::{DataError, Event};

#[derive(Clone, Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const APP: &str = r##"keyframes sweep
  from translate="-24px 0px"
  to translate="64px 8px"
keyframes spin
  from rotate=0
  to rotate=360
keyframes pulse
  from scale=0.5
  to scale=1.2
keyframes glow
  from background-color="#16a34a"
  to background-color="#2563eb"
keyframes fade
  from background-color="#16a34a"
  to background-color="rgba(37, 99, 235, 0.5)"
keyframes drop
  from cy=10
  to cy=30
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="sweep" width=24 height=64 animation=(on ? "sweep 1s linear infinite" : "none")
      box testId="spin" width=32 height=12 animation=(on ? "spin 1200ms linear infinite" : "none")
      box testId="pulse" width=32 height=32 animation=(on ? "pulse 800ms ease-in-out infinite alternate" : "none")
      box testId="glow" width=64 height=64 background-color="#f1f5f9" animation=(on ? "glow 1s ease infinite alternate" : "none")
      box testId="corner" width=32 height=8 transform-origin="0% 0%" animation=(on ? "spin 2s linear infinite" : "none")
      box testId="pressed" width=32 height=32 press=go animation=(on ? "sweep 1s linear infinite" : "none")
      box testId="holder" width=64 height=64 animation=(on ? "spin 1s linear infinite" : "none")
        button press=go testId="inner"
          text "In"
      box testId="framed" width=64 height=64 border-width=2 border-style="solid" border-color="#0f172a" animation=(on ? "glow 1s ease infinite" : "none")
      box testId="fading" width=64 height=64 animation=(on ? "fade 1s ease infinite" : "none")
      svg width=40 height=40 viewBox="0 0 40 40"
        rect testId="shape" x=10 y=10 width=20 height=20 fill="#0ea5e9" transform-box="fill-box" transform-origin="center" animation=(on ? "spin 1s linear infinite" : "none")
        circle testId="ball" cx=20 cy=10 r=4 fill="#f97316" animation=(on ? "drop 1s linear infinite" : "none")
        circle testId="clipped" cx=20 cy=10 r=4 filter="blur(1px)" animation=(on ? "drop 1s linear infinite" : "none")
        defs
          filter id="soft"
            feGaussianBlur stdDeviation=1
        g filter="url(#soft)"
          rect testId="inside" x=10 y=10 width=8 height=8 transform-box="fill-box" transform-origin="center" animation=(on ? "spin 1s linear infinite" : "none")
        g filter="blur(1px)"
          rect testId="followed" x=10 y=10 width=8 height=8 transform-box="fill-box" transform-origin="center" animation=(on ? "spin 1s linear infinite" : "none")
"##;

fn id(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// The `animations` op's specs for `view` in a batch, if any.
fn specs(batch: &str, view: u32) -> Option<Vec<serde_json::Value>> {
    let v: serde_json::Value = serde_json::from_str(batch).unwrap();
    v["ops"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|op| op["op"] == "animations" && op["id"] == view)
        .map(|op| op["specs"].as_array().cloned().unwrap_or_default())
}

fn keys(batch: &str, view: u32) -> Vec<String> {
    specs(batch, view)
        .unwrap_or_default()
        .iter()
        .map(|s| s["k"].as_str().unwrap().to_string())
        .collect()
}

fn ios(host: &mut Host<NoData>) {
    host.svg.box_motion = true;
    host.engine.set_lowered_properties(&svg::lowered(true));
}

#[test]
fn a_boxs_transform_and_colour_keyframes_play_in_core_animation() {
    let plan = contract::compile(APP).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let go = id(&host, "go");
    let on = host.dispatch_at(go, Event::Press, 100.0);
    assert_eq!(
        keys(&on, id(&host, "sweep")),
        ["transform.translation.x", "transform.translation.y"],
        "{on}"
    );
    assert_eq!(keys(&on, id(&host, "spin")), ["transform.rotation.z"]);
    assert_eq!(keys(&on, id(&host, "pulse")), ["transform.scale"]);
    assert_eq!(keys(&on, id(&host, "glow")), ["backgroundColor"]);
    // A turn in radians, over one linear interval; the y axis its own track.
    let spin = specs(&on, id(&host, "spin")).unwrap();
    let v: Vec<f64> = spin[0]["v"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect();
    assert_eq!(v.len(), 2);
    assert!((v[1] - std::f64::consts::TAU).abs() < 1e-4, "{v:?}");
    let sweep = specs(&on, id(&host, "sweep")).unwrap();
    assert_eq!(
        sweep[1]["v"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_f64().unwrap())
            .collect::<Vec<_>>(),
        [0.0, 8.0]
    );
    // An SVG element's transform plays on its transform pair's outer
    // layer, and a circle's centre as its layer's position.
    let scene = |on: &str| -> String {
        let v: serde_json::Value = serde_json::from_str(on).unwrap();
        v["ops"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|op| op["op"] == "svg")
            .map(|op| op.to_string())
            .unwrap_or_default()
    };
    let svg_op = scene(&on);
    assert!(svg_op.contains("transform.rotation.z"), "{svg_op}");
    assert!(svg_op.contains("position.y"), "{svg_op}");
    // A CSS function filter's picture follows its content on the GPU on
    // iOS, so what turns inside it lowers too.
    for lowered in ["shape", "ball", "followed"] {
        assert!(
            !host.engine.node_sampled(exact_kernel::motion::motion_node(
                host.runner.kernel().node(id(&host, lowered)).unwrap().key
            )),
            "{lowered}"
        );
    }
    // What Core Animation cannot say as CSS does is sampled: a corner
    // origin, a box that takes input or holds one that does, a bordered
    // box's colour, two alphas, a filtered circle's centre (its picture is
    // made for one place), and anything drawn inside a `filter` element's
    // picture.
    for sampled in [
        "corner", "pressed", "holder", "framed", "fading", "clipped", "inside",
    ] {
        assert!(keys(&on, id(&host, sampled)).is_empty(), "{sampled}: {on}");
        assert!(
            host.engine.node_sampled(exact_kernel::motion::motion_node(
                host.runner.kernel().node(id(&host, sampled)).unwrap().key
            )),
            "{sampled}"
        );
    }
    assert!(
        on.contains("\"motion\":true"),
        "the sampled ones keep frames coming"
    );
}

#[test]
fn lowered_box_keyframes_keep_no_frames_coming() {
    let app = r##"keyframes sweep
  from translate="-24px 0px"
  to translate="64px 0px"
keyframes glow
  from background-color="#16a34a"
  to background-color="#2563eb"
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="bar" width=64 height=64 overflow="hidden"
        box testId="sweep" width=24 height=64 animation=(on ? "sweep 1s linear infinite" : "none")
      box testId="glow" width=64 height=64 animation=(on ? "glow 1s ease infinite alternate" : "none")
"##;
    let plan = contract::compile(app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    assert!(on.contains("\"motion\":false"), "{on}");
    let later = host.tick(1400.0);
    assert!(!later.contains("\"op\":\"present\""), "{later}");
    // Off again: the specs go, the row's own values stay.
    let off = host.dispatch_at(id(&host, "go"), Event::Press, 1500.0);
    assert_eq!(specs(&off, id(&host, "sweep")), Some(Vec::new()), "{off}");
}

/// A modern colour keyframe moves in Oklab (CSS Color 4 §12.1); a lowered
/// track interpolates in sRGB, so the engine samples it.
#[test]
fn a_modern_colour_keyframe_is_sampled() {
    let app = r##"keyframes glow
  from background-color="color(srgb 1 0 0)"
  to background-color="color(srgb 0 0 1)"
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="glow" width=64 height=64 animation=(on ? "glow 1s linear infinite" : "none")
"##;
    let plan = contract::compile(app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    assert!(on.contains("\"motion\":true"), "{on}");
}

#[test]
fn hosts_without_box_lowering_sample_them() {
    let plan = contract::compile(APP).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    // LLP 1126 D10: watchOS samples box motion; it has no Core Animation presenter.
    assert!(!host.svg.box_motion || cfg!(any(target_os = "ios", target_os = "tvos")));
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    if !cfg!(any(target_os = "ios", target_os = "tvos")) {
        assert!(keys(&on, id(&host, "sweep")).is_empty(), "{on}");
        assert!(on.contains("\"motion\":true"));
    }
}

/// LLP 1053.000.000 D4: a glass group ignores the opacity Core Animation
/// plays between it and its glass, so an opacity animation inside a group is
/// sampled and reaches the host as values; outside one, it is lowered.
#[test]
fn opacity_inside_a_glass_group_is_sampled() {
    let app = r##"keyframes dim
  from opacity=1
  to opacity=0.2
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      row glassGroup=12
        box testId="grouped" width=40 height=40 backgroundMaterial="glass" animation=(on ? "dim 1s linear infinite" : "none")
      box testId="free" width=40 height=40 backgroundMaterial="glass" animation=(on ? "dim 1s linear infinite" : "none")
"##;
    let plan = contract::compile(app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    assert_eq!(keys(&on, id(&host, "free")), ["opacity"], "{on}");
    assert!(keys(&on, id(&host, "grouped")).is_empty(), "{on}");
    let sampled = |host: &Host<NoData>, t: &str| {
        host.engine.node_sampled(exact_kernel::motion::motion_node(
            host.runner.kernel().node(id(host, t)).unwrap().key,
        ))
    };
    assert!(sampled(&host, "grouped"));
    assert!(!sampled(&host, "free"));
    let grouped = id(&host, "grouped");
    let later = host.tick(600.0);
    let v: serde_json::Value = serde_json::from_str(&later).unwrap();
    assert!(
        v["ops"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "present"
                && op["id"] == grouped
                && op.to_string().contains("opacity")),
        "{later}"
    );
}

/// LLP 1053.000.000 D4: an opacity animation still lowered when a group is
/// above it (a group set after it started) is switched to sampling by the
/// pass over every running animation, and its Core Animation spec withdrawn.
#[test]
fn a_lowered_opacity_animation_under_a_group_is_switched_to_sampling() {
    let app = r##"keyframes dim
  from opacity=1
  to opacity=0.2
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      row glassGroup=12
        box testId="grouped" width=40 height=40 backgroundMaterial="glass" animation=(on ? "dim 1s linear infinite" : "none")
"##;
    let plan = contract::compile(app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let view = id(&host, "grouped");
    let node = exact_kernel::motion::motion_node(host.runner.kernel().node(view).unwrap().key);
    host.engine.set_node_sampled(node, false);
    assert_eq!(
        svg_lower::resample(host.runner.kernel(), &mut host.engine, false),
        [view]
    );
    assert!(host.engine.node_sampled(node));
    assert!(svg_lower::resample(host.runner.kernel(), &mut host.engine, false).is_empty());
}

/// A filtered box is drawn as a picture that Core Animation's animations
/// do not reach (`BoxFilter.swift`), as is an SVG group with a filter
/// reference. An animation running when an ancestor gains such a filter is
/// switched to sampling in that batch, its Core Animation specs withdrawn,
/// though the commit did not touch the animated node.
#[test]
fn a_lowered_animation_is_sampled_when_an_ancestor_gains_a_filter() {
    let app = r##"keyframes pulse
  from opacity=1
  to opacity=0.2
keyframes spin
  from rotate=0
  to rotate=360
component A
  state on = false
  state blurred = false
  action go
    on = not on
  action blur
    blurred = not blurred
  view
    column
      button press=go testId="go"
        text "Go"
      button press=blur testId="blur"
        text "Blur"
      box filter=(blurred ? "blur(0.5px)" : "none")
        box testId="square" width=40 height=40 background-color="#ff0000" animation=(on ? "pulse 1s linear infinite alternate" : "none")
        box testId="turning" width=40 height=40 animation=(on ? "spin 1s linear infinite" : "none")
      svg width=40 height=40 viewBox="0 0 40 40"
        defs
          filter id="soft"
            feGaussianBlur stdDeviation=1
        g filter=(blurred ? "url(#soft)" : "none")
          rect testId="shape" x=10 y=10 width=8 height=8 animation=(on ? "pulse 1s linear infinite" : "none")
"##;
    for ios_motion in [false, true] {
        let plan = contract::compile(app).unwrap().encode();
        let (mut host, _) = Host::boot(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            390.0,
            844.0,
        )
        .unwrap();
        if ios_motion {
            ios(&mut host);
        }
        let names: &[&str] = if ios_motion {
            &["square", "turning", "shape"]
        } else {
            &["square", "shape"]
        };
        let sampled = |host: &Host<NoData>, name: &str| {
            host.engine.node_sampled(exact_kernel::motion::motion_node(
                host.runner.kernel().node(id(host, name)).unwrap().key,
            ))
        };
        // A box's specs come in its `animations` op, a shape's in its scene.
        let shape = id(&host, "shape");
        let lowered = |batch: &str, view: u32| {
            fn find(v: &serde_json::Value, view: u32) -> Option<usize> {
                if v["id"] == view && v["a"].is_array() {
                    return v["a"].as_array().map(Vec::len);
                }
                let items = v.as_array().into_iter().flatten();
                let fields = v.as_object().into_iter().flat_map(|o| o.values());
                items.chain(fields).find_map(|c| find(c, view))
            }
            if view == shape {
                find(&serde_json::from_str(batch).unwrap(), view)
            } else {
                specs(batch, view).map(|s| s.len())
            }
        };
        let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
        for name in names {
            assert!(!sampled(&host, name), "{name}, ios: {ios_motion}");
            assert!(
                lowered(&on, id(&host, name)).is_some_and(|n| n > 0),
                "{name}: {on}"
            );
        }
        let blurred = host.dispatch_at(id(&host, "blur"), Event::Press, 300.0);
        for name in names {
            assert!(sampled(&host, name), "{name}, ios: {ios_motion}");
            assert_eq!(
                lowered(&blurred, id(&host, name)),
                Some(0),
                "{name}: {blurred}"
            );
        }
        let later = host.tick(600.0);
        let square = id(&host, "square");
        let v: serde_json::Value = serde_json::from_str(&later).unwrap();
        assert!(
            v["ops"]
                .as_array()
                .unwrap()
                .iter()
                .any(|op| op["op"] == "present"
                    && op["id"] == square
                    && op["property"] == "opacity"),
            "ios: {ios_motion}: {later}"
        );
    }
}

/// LLP 1115.003 D4: a box's `translate` given as a percentage of itself
/// lowers too, resolved against its border box when its specs are made, and
/// its specs are made again when it resizes.
#[test]
fn a_percentage_translate_lowers_against_the_box_and_follows_a_resize() {
    let app = "keyframes lift\n  from translate=\"0px 100%\"\n  to translate=\"0px 0px\"\ncomponent A\n  state on = false\n  state tall = false\n  action go\n    on = true\n  action grow\n    tall = true\n  view\n    column\n      button press=go testId=\"go\"\n        text \"Go\"\n      button press=grow testId=\"grow\"\n        text \"Grow\"\n      box testId=\"lane\" width=40 height=(tall ? 120 : 90) animation=(on ? \"lift 500ms ease-out both\" : \"none\")\n";
    let plan = contract::compile(app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let lane = id(&host, "lane");
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let y = |batch: &str| -> Vec<f64> {
        specs(batch, lane)
            .unwrap_or_default()
            .iter()
            .find(|s| s["k"] == "transform.translation.y")
            .map(|s| {
                s["v"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_f64().unwrap())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(y(&on), [90.0, 0.0], "{on}");
    let v: serde_json::Value = serde_json::from_str(&on).unwrap();
    assert_eq!(v["motion"], false, "Core Animation plays it: {on}");
    let grown = host.dispatch_at(id(&host, "grow"), Event::Press, 200.0);
    assert_eq!(y(&grown), [120.0, 0.0], "{grown}");
}

/// @ref LLP 1121 D8 — an opacity animation on a node whose press (`:active`
/// lowered to the press paint) or `:hover` look paints its opacity stays
/// sampled by the engine on both platforms (Core Animation would add the
/// host's opacity layers to its own sample); one on a node without either
/// is Core Animation's.
#[test]
fn a_press_or_a_look_keeps_an_opacity_animation_sampled() {
    let app = r##"keyframes blink
  from opacity=1
  to opacity=0.4
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="pressed" press=go width=20 height=20 animation=(on ? "blink 1s linear infinite" : "none")
        :active
          opacity = 0.6
      box testId="hovered" width=20 height=20 animation=(on ? "blink 1s linear infinite" : "none")
        :hover
          opacity = 0.8
      box testId="plain" width=20 height=20 animation=(on ? "blink 1s linear infinite" : "none")
"##;
    for ios_motion in [false, true] {
        let plan = contract::compile(app).unwrap().encode();
        let (mut host, _) = Host::boot(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            390.0,
            844.0,
        )
        .unwrap();
        if ios_motion {
            ios(&mut host);
        }
        host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
        let sampled = |name: &str| {
            host.engine.node_sampled(exact_kernel::motion::motion_node(
                host.runner.kernel().node(id(&host, name)).unwrap().key,
            ))
        };
        assert!(
            sampled("pressed") && sampled("hovered"),
            "ios: {ios_motion}"
        );
        assert!(!sampled("plain"), "ios: {ios_motion}");
    }
}

/// CSS Animations 1: an animation of zero duration, or of zero iterations,
/// still occurs, and its fill shows its last (or first) frame. Core
/// Animation plays neither, so the engine samples the node and presents the
/// filled frame, on both platforms and for an SVG shape too.
#[test]
fn an_animation_of_no_time_is_sampled_and_presents_its_fill() {
    let app = r##"keyframes dim
  from opacity=1
  to opacity=0.25
keyframes rise
  from opacity=0.25
  to opacity=1
keyframes spin
  from rotate=0
  to rotate=90
component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      box testId="instant" width=40 height=40 animation=(on ? "dim 0ms linear forwards" : "none")
      box testId="none" width=40 height=40 animation=(on ? "rise 600ms linear 0 forwards" : "none")
      box testId="timed" width=40 height=40 animation=(on ? "dim 300ms linear forwards" : "none")
      box testId="turned" width=40 height=40 animation=(on ? "spin 0ms linear forwards" : "none")
      svg width=40 height=40 viewBox="0 0 40 40"
        circle testId="dot" cx=20 cy=20 r=4 fill="#f97316" animation=(on ? "dim 0ms linear forwards" : "none")
"##;
    for ios_motion in [false, true] {
        let plan = contract::compile(app).unwrap().encode();
        let (mut host, _) = Host::boot(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            390.0,
            844.0,
        )
        .unwrap();
        if ios_motion {
            ios(&mut host);
        }
        let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
        let sampled = |name: &str| {
            host.engine.node_sampled(exact_kernel::motion::motion_node(
                host.runner.kernel().node(id(&host, name)).unwrap().key,
            ))
        };
        for name in ["instant", "none", "dot"] {
            assert!(sampled(name), "{name}, ios: {ios_motion}");
            assert!(keys(&on, id(&host, name)).is_empty(), "{name}: {on}");
        }
        assert!(!sampled("timed"), "ios: {ios_motion}");
        if ios_motion {
            assert!(sampled("turned"));
            assert!(keys(&on, id(&host, "turned")).is_empty(), "{on}");
        }
        let later = host.tick(200.0);
        let presented = |name: &str| -> Option<f64> {
            let view = id(&host, name);
            [&on, &later].iter().rev().find_map(|batch| {
                let v: serde_json::Value = serde_json::from_str(batch).unwrap();
                v["ops"].as_array().unwrap().iter().rev().find_map(|op| {
                    (op["op"] == "present" && op["id"] == view && op["property"] == "opacity")
                        .then(|| op["x"].as_f64().unwrap())
                })
            })
        };
        // The last frame for a forwards fill; the first for zero iterations.
        for name in ["instant", "none"] {
            let x = presented(name);
            assert!(
                x.is_some_and(|x| (x - 0.25).abs() < 1e-6),
                "{name} presented {x:?}, ios: {ios_motion}: {on} | {later}"
            );
        }
    }
}

/// `linear()` lowers to keys Core Animation plays as Chrome 155 computes the
/// curve (the engine's `piecewise_linear_matches_chrome_at_its_edges` has
/// the same samples): key times from 0 to 1, never decreasing; a jump held
/// to a hair before its input; extrapolation before the first stop and past
/// the last. `translate` is read because, unlike opacity, it does not clamp.
#[test]
fn linear_easings_lower_to_what_chrome_plays() {
    let cases: &[(&str, &[(f64, f64)])] = &[
        (
            "linear(1 0% 25%, 0 25% 100%)",
            &[(0.0, 1.0), (0.2, 1.0), (0.25, 0.0), (1.0, 0.0)],
        ),
        (
            "linear(0 0% 25%, 1 50% 75%)",
            &[
                (0.0, 0.0),
                (0.25, 0.0),
                (0.375, 0.5),
                (0.9, 1.0),
                (1.0, 1.0),
            ],
        ),
        (
            "linear(0, 0.2 100%, 0.5 100%, 1 100%)",
            &[(0.5, 0.1), (0.9998, 0.2), (1.0, 1.0)],
        ),
        (
            "linear(0 25%, 1 25%, 1)",
            &[(0.0, 1.0), (0.1, 1.0), (0.25, 1.0), (1.0, 1.0)],
        ),
        (
            "linear(0 25%, 1 25%, 0.5 25%, 1)",
            &[(0.1, 1.0), (0.25, 0.5), (0.5, 0.6667)],
        ),
        (
            "linear(0 25%, 1 75%)",
            &[(0.0, -0.5), (0.1, -0.3), (0.5, 0.5), (0.9, 1.3), (1.0, 1.5)],
        ),
        ("linear(0, 1 0%, 0)", &[(0.0, 1.0), (0.5, 0.5), (1.0, 0.0)]),
        (
            "linear(0, 0.5 50%, 1 50%, 0.2 50%)",
            &[(0.25, 0.25), (0.5, 0.2), (0.9, 0.2)],
        ),
    ];
    let mut app = String::from(
        "keyframes slide\n  from translate=\"0px 0px\"\n  to translate=\"100px 0px\"\ncomponent A\n  state on = false\n  action go\n    on = not on\n  view\n    column\n      button press=go testId=\"go\"\n        text \"Go\"\n",
    );
    for (i, (easing, _)) in cases.iter().enumerate() {
        app.push_str(&format!(
            "      box testId=\"l{i}\" width=8 height=8 animation=(on ? \"slide 1s {easing} infinite\" : \"none\")\n"
        ));
    }
    let plan = contract::compile(&app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    for (i, (easing, samples)) in cases.iter().enumerate() {
        let specs = specs(&on, id(&host, &format!("l{i}"))).expect("lowered");
        let x = specs
            .iter()
            .find(|s| s["k"] == "transform.translation.x")
            .expect("an x track");
        let nums = |k: &str| -> Vec<f64> {
            x[k].as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap())
                .collect()
        };
        let (t, v) = (nums("t"), nums("v"));
        assert_eq!(t.first(), Some(&0.0), "{easing}: {t:?}");
        assert_eq!(t.last(), Some(&1.0), "{easing}: {t:?}");
        assert!(t.windows(2).all(|w| w[0] <= w[1]), "{easing}: {t:?}");
        // Core Animation's linear calculation mode, between the keys.
        let play = |at: f64| -> f64 {
            let k = t.iter().rposition(|x| *x <= at).unwrap();
            if k + 1 == t.len() || t[k + 1] == t[k] {
                return v[k];
            }
            v[k] + (v[k + 1] - v[k]) * (at - t[k]) / (t[k + 1] - t[k])
        };
        for &(at, chrome) in *samples {
            let got = play(at) / 100.0;
            assert!(
                (got - chrome).abs() < 1e-3,
                "{easing} at {at}: Chrome {chrome}, lowered {got} ({t:?} {v:?})"
            );
        }
    }
}

/// A colour's `linear()` lowers as a length's does, its jump held and its
/// start the curve's, with a key every sixteenth between: the red channel
/// Chrome 155 computes for a paused CSS animation from rgb(0, 0, 0) to
/// rgb(200, 0, 0), extrapolation clamped to the channel's range.
#[test]
fn a_colours_linear_easing_lowers_to_what_chrome_plays() {
    let cases: &[(&str, &[(f64, f64)])] = &[
        (
            "linear(1 0% 25%, 0 25% 100%)",
            &[(0.0, 200.0), (0.2, 200.0), (0.25, 0.0), (0.5, 0.0)],
        ),
        (
            "linear(0 25%, 1 25%, 1)",
            &[(0.0, 200.0), (0.1, 200.0), (0.25, 200.0)],
        ),
        (
            "linear(0, 1 50%)",
            &[(0.25, 100.0), (0.75, 255.0), (1.0, 255.0)],
        ),
    ];
    let mut app = String::from(
        "keyframes paint\n  from background-color=\"rgb(0, 0, 0)\"\n  to background-color=\"rgb(200, 0, 0)\"\ncomponent A\n  state on = false\n  action go\n    on = not on\n  view\n    column\n      button press=go testId=\"go\"\n        text \"Go\"\n",
    );
    for (i, (easing, _)) in cases.iter().enumerate() {
        app.push_str(&format!(
            "      box testId=\"c{i}\" width=8 height=8 animation=(on ? \"paint 1s {easing} infinite\" : \"none\")\n"
        ));
    }
    let plan = contract::compile(&app).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    for (i, (easing, samples)) in cases.iter().enumerate() {
        let specs = specs(&on, id(&host, &format!("c{i}"))).expect("lowered");
        assert_eq!(specs[0]["k"], "backgroundColor", "{easing}");
        let t: Vec<f64> = specs[0]["t"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_f64().unwrap())
            .collect();
        let red: Vec<f64> = specs[0]["v"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c[0].as_f64().unwrap())
            .collect();
        assert_eq!(t.first(), Some(&0.0), "{easing}: {t:?}");
        assert_eq!(t.last(), Some(&1.0), "{easing}: {t:?}");
        assert!(t.windows(2).all(|w| w[0] <= w[1]), "{easing}: {t:?}");
        assert!(t.len() >= 17, "{easing}: a key every sixteenth, {t:?}");
        let play = |at: f64| -> f64 {
            let k = t.iter().rposition(|x| *x <= at).unwrap();
            if k + 1 == t.len() || t[k + 1] == t[k] {
                return red[k];
            }
            red[k] + (red[k + 1] - red[k]) * (at - t[k]) / (t[k + 1] - t[k])
        };
        for &(at, chrome) in *samples {
            let got = play(at);
            assert!(
                (got - chrome).abs() <= 1.0,
                "{easing} at {at}: Chrome red {chrome}, lowered {got} ({t:?} {red:?})"
            );
        }
    }
}

/// A small deterministic generator for the property tests below.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn unit(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
}

/// A random easing for one interval: `linear()` with two to six stops on a
/// coarse grid, so stops often share an input and often leave the ends to
/// extrapolation; `steps()`; or a cubic.
fn random_easing(r: &mut Rng) -> String {
    match r.below(6) {
        0..=2 => {
            let n = 2 + r.below(5) as usize;
            let mut inputs: Vec<u64> = (0..n).map(|_| r.below(11) * 10).collect();
            inputs.sort();
            let stops: Vec<String> = inputs
                .iter()
                .map(|i| format!("{} {i}%", (r.below(21) as f64 - 5.0) / 10.0))
                .collect();
            format!("linear({})", stops.join(", "))
        }
        3 => {
            let at = ["jump-start", "jump-end", "jump-none", "jump-both"][r.below(4) as usize];
            format!("steps({}, {at})", 2 + r.below(5))
        }
        4 => "ease-in-out".into(),
        _ => "cubic-bezier(0.3, 0.1, 0.6, 1.2)".into(),
    }
}

/// Core Animation's linear calculation mode over a spec's keys, each
/// segment under its own cubic (`c`, empty when every one is linear).
fn play_spec(t: &[f64], v: &[f64], c: &[[f64; 4]], at: f64) -> f64 {
    let k = t.iter().rposition(|x| *x <= at).unwrap_or(0);
    if k + 1 >= t.len() || t[k + 1] <= t[k] {
        return v[k];
    }
    let u = (at - t[k]) / (t[k + 1] - t[k]);
    let eased = match c.get(k) {
        Some(&[x1, y1, x2, y2]) => {
            let bez = |a: f64, b: f64, s: f64| {
                3.0 * (1.0 - s) * (1.0 - s) * s * a + 3.0 * (1.0 - s) * s * s * b + s * s * s
            };
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..60 {
                let mid = (lo + hi) / 2.0;
                if bez(x1, x2, mid) < u {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            bez(y1, y2, (lo + hi) / 2.0)
        }
        None => u,
    };
    v[k] + (v[k + 1] - v[k]) * eased
}

/// One random keyframes track per box, lowered, played back and compared
/// with the engine's own value (`Animation::value_at`) at many points.
/// Points within 2e-3 of a keyframe or of a stop's input are left out: a
/// jump there is held to a hair before it, which the engine is not; so are
/// a colour's clamped spans. The reviews' cases lead the translate set: a
/// steep ramp beside a held stop, an extrapolated end before a plain
/// interval, and a ramp a hundredth of a percent long.
fn lowered_matches_engine(colour: bool, seed: u64, boxes: usize) {
    let mut r = Rng(seed);
    let property = if colour {
        "background-color"
    } else {
        "translate"
    };
    let literal = |v: f64| {
        if colour {
            format!("rgb({}, 0, 0)", v.round())
        } else {
            format!("{v}px 0px")
        }
    };
    let mut app = String::new();
    let mut cases = Vec::new();
    for i in 0..boxes {
        // Keyframe offsets: 0, up to two inside, 1; the two review cases first.
        let (offsets, values, easings): (Vec<f64>, Vec<f64>, Vec<Option<String>>) =
            match (colour, i) {
                (false, 0) => (
                    vec![0.0, 1.0],
                    vec![0.0, 100.0],
                    vec![Some("linear(0 0% 50%, 1 50.005% 100%)".into())],
                ),
                (false, 1) => (
                    vec![0.0, 0.5, 1.0],
                    vec![0.0, 100.0, 200.0],
                    vec![Some("linear(0 25%, 1 75%)".into()), Some("linear".into())],
                ),
                (false, 2) => (
                    vec![0.0, 1.0],
                    vec![0.0, 100.0],
                    vec![Some("linear(0, 1 0.01%)".into())],
                ),
                _ => {
                    let mut offsets = vec![0.0];
                    for _ in 0..r.below(3) {
                        offsets.push((1 + r.below(19)) as f64 * 5.0 / 100.0);
                    }
                    offsets.push(1.0);
                    offsets.sort_by(f64::total_cmp);
                    offsets.dedup();
                    let values = offsets.iter().map(|_| (r.below(201)) as f64).collect();
                    let easings = (1..offsets.len())
                        .map(|_| (r.below(4) > 0).then(|| random_easing(&mut r)))
                        .collect();
                    (offsets, values, easings)
                }
            };
        let default = random_easing(&mut r);
        let name = format!("k{i}");
        let mut contract = format!("keyframes {name}\n");
        let mut css = String::new();
        let mut breaks: Vec<f64> = offsets.clone();
        for (k, off) in offsets.iter().enumerate() {
            let easing = easings.get(k).cloned().flatten();
            contract.push_str(&format!(
                "  {}% {property}=\"{}\"",
                off * 100.0,
                literal(values[k])
            ));
            css.push_str(&format!(
                "{}%{{{property}:{}",
                off * 100.0,
                literal(values[k])
            ));
            if let Some(e) = &easing {
                contract.push_str(&format!(" animation-timing-function=\"{e}\""));
                css.push_str(&format!(";animation-timing-function:{e}"));
            }
            contract.push('\n');
            css.push('}');
            if let (Some(next), Some(e)) = (offsets.get(k + 1), easing.as_ref().or(Some(&default)))
            {
                let span = next - off;
                for stop in e
                    .split(['(', ',', ')'])
                    .filter_map(|f| f.split_whitespace().nth(1))
                {
                    if let Some(pct) = stop.strip_suffix('%').and_then(|n| n.parse::<f64>().ok()) {
                        breaks.push(off + span * pct / 100.0);
                    }
                }
                if let Some(rest) = e.strip_prefix("steps(") {
                    let n: f64 = rest.split(',').next().unwrap().trim().parse().unwrap();
                    breaks.extend((1..n as usize).map(|j| off + span * j as f64 / n));
                }
            }
        }
        app.push_str(&contract);
        cases.push((name, default, css, breaks));
    }
    app.push_str("component A\n  state on = false\n  action go\n    on = not on\n  view\n    column\n      button press=go testId=\"go\"\n        text \"Go\"\n");
    for (name, default, _, _) in &cases {
        app.push_str(&format!(
            "      box testId=\"{name}\" width=8 height=8 animation=(on ? \"{name} 1s {default} infinite\" : \"none\")\n"
        ));
    }
    let plan = contract::compile(&app)
        .unwrap_or_else(|e| panic!("{e:?}\n{app}"))
        .encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    ios(&mut host);
    let on = host.dispatch_at(id(&host, "go"), Event::Press, 100.0);
    let prop = exact_motion::Property::from_name(property).unwrap();
    let mut checked = 0;
    for (name, default, css, breaks) in &cases {
        let mut anim = exact_motion::Animations::parse(&format!("{name} 1s {default} infinite"))
            .unwrap()
            .0
            .remove(0);
        anim.keyframes = exact_motion::Keyframes::parse(css).unwrap();
        let engine = |at: f64| {
            let v = anim.value_at(at, prop, exact_motion::Value::ZERO, false);
            if colour {
                v.to_rgba8()[0] as f64
            } else {
                v.x
            }
        };
        let specs = specs(&on, id(&host, name)).unwrap_or_else(|| panic!("{name} lowered: {css}"));
        let spec = specs
            .iter()
            .find(|s| {
                s["k"]
                    == if colour {
                        "backgroundColor"
                    } else {
                        "transform.translation.x"
                    }
            })
            .unwrap();
        let t: Vec<f64> = spec["t"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_f64().unwrap())
            .collect();
        let v: Vec<f64> = spec["v"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                if colour {
                    n[0].as_f64().unwrap()
                } else {
                    n.as_f64().unwrap()
                }
            })
            .collect();
        let c: Vec<[f64; 4]> = spec["c"]
            .as_array()
            .map(|cs| {
                cs.iter()
                    .map(|q| std::array::from_fn(|j| q[j].as_f64().unwrap()))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(t.first(), Some(&0.0), "{name} {default} {css}: {t:?}");
        assert_eq!(t.last(), Some(&1.0), "{name} {default} {css}: {t:?}");
        assert!(
            t.windows(2).all(|w| w[0] <= w[1]),
            "{name} {default} {css}: {t:?}"
        );
        for j in 0..400 {
            let at = (j as f64 + r.unit()) / 400.0;
            if breaks.iter().any(|b| (at - b).abs() < 2e-3) {
                continue;
            }
            let (want, got) = (engine(at), play_spec(&t, &v, &c, at));
            // A colour extrapolated out of gamut clamps between its keys,
            // where Core Animation draws straight (`stepped_keys`).
            let k = t.iter().rposition(|x| *x <= at).unwrap_or(0);
            if colour
                && v[k..(k + 2).min(v.len())]
                    .iter()
                    .any(|c| *c <= 0.0 || *c >= 255.0)
            {
                continue;
            }
            let tolerance = if colour { 2.0 } else { 0.05 };
            assert!(
                (want - got).abs() <= tolerance,
                "{name} at {at}: engine {want}, Core Animation {got}\n  {default} | {css}\n  t {t:?}\n  v {v:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > boxes * 300, "too few points checked: {checked}");
}

/// Translate tracks: lengths do not clamp, so a `linear()`'s extrapolated
/// ends show.
#[test]
fn random_lowered_translate_tracks_play_as_the_engine_samples() {
    lowered_matches_engine(false, 0x9e37_79b9_7f4a_7c15, 80);
}

/// Colour tracks, sampled every sixteenth between their keys.
#[test]
fn random_lowered_colour_tracks_play_as_the_engine_samples() {
    lowered_matches_engine(true, 0x2545_f491_4f6c_dd1d, 60);
}

#[test]
fn an_unsupported_filter_transition_is_named() {
    let literal = "component App\n  view\n    box transition=\"opacity 200ms, filter 200ms\"\n";
    let diagnostics = contract::compile(literal).unwrap_err();
    assert!(format!("{diagnostics:?}").contains("`filter` is not a supported transition property"));
    let source = "component App\n  state transitions = \"opacity 200ms, filter 200ms\"\n  view\n    box transition=transitions\n";
    let plan = contract::compile(source).unwrap().encode();
    let (host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    let logs = host.agent("{\"op\":\"logs\"}");
    assert!(logs.contains("invalid transition value"), "{logs}");
    assert!(logs.contains("filter 200ms"), "{logs}");
}

#[test]
fn hover_scale_transitions_without_layout_and_returns_to_the_changed_base() {
    let source = r#"component App
  state large = false
  action grow
    large = true
  view
    box testId="card" press=grow width=100 height=40 scale=(large ? 1.1 : 1) transition="scale 150ms linear"
      :hover
        scale=1.2
      :focus-visible
        scale=1.3
      text "Hover"
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        400.,
    )
    .unwrap();
    let view = id(&host, "card");
    let key = host.runner.kernel().node(view).unwrap().key;
    let motion = motion_node(key);
    let frame = host.runner.kernel().node(view).unwrap().frame;
    let scale = |host: &Host<NoData>| host.engine.value(motion, Property::Scale).unwrap().x;
    host.look_motion(view, 1, 0.);
    host.advance(75.);
    assert!((scale(&host) - 1.1).abs() < 1e-6);
    assert_eq!(host.runner.kernel().node(view).unwrap().frame, frame);
    host.dispatch_at(view, Event::Press, 75.);
    host.advance(150.);
    assert!(
        (scale(&host) - 1.2).abs() < 1e-6,
        "base edits stay under hover"
    );
    host.look_motion(view, 3, 150.);
    host.advance(300.);
    assert!((scale(&host) - 1.3).abs() < 1e-6, "focus wins over hover");
    host.look_motion(view, 0, 300.);
    host.advance(450.);
    assert!(
        (scale(&host) - 1.1).abs() < 1e-6,
        "leave returns to the new base"
    );
}

#[test]
fn hover_scale_clears_on_disable_and_destruction() {
    let source = r#"component App
  state blocked = false
  state shown = true
  action disable
    blocked = true
  action remove
    shown = false
  view
    column
      button testId="disable" press=disable
        text "Disable"
      button testId="remove" press=remove
        text "Remove"
      if shown
        box testId="card" disabled=blocked width=100 height=40 transition="scale 150ms linear"
          :hover
            scale=1.1
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        400.,
    )
    .unwrap();
    let view = id(&host, "card");
    let motion = motion_node(host.runner.kernel().node(view).unwrap().key);
    host.look_motion(view, 1, 0.);
    host.advance(150.);
    assert!((host.engine.value(motion, Property::Scale).unwrap().x - 1.1).abs() < 1e-6);
    host.dispatch_at(id(&host, "disable"), Event::Press, 150.);
    host.advance(300.);
    assert_eq!(host.engine.value(motion, Property::Scale).unwrap().x, 1.);
    host.dispatch_at(id(&host, "remove"), Event::Press, 300.);
    assert_eq!(host.engine.value(motion, Property::Scale), None);
}

#[test]
fn authored_translate_velocity_reaches_native_springs_from_panrelease() {
    let source = r#"component App
  state x = 0
  state y = 0
  state vx = 0
  state vy = 0
  state dragging = true
  action release(sx: number, sy: number)
    vx = sx
    vy = sy
    dragging = false
    x = 100
    y = 100
  view
    box testId="panel" panrelease=release width=100 height=100 translate=`${x}px ${y}px` -exact-translate-velocity=`${vx} ${vy}` transition=(dragging ? "none" : "translate -exact-spring(260, 24, 1)")
"#;
    for (target, vx, vy) in [
        (100, 800., -600.),
        (100, -800., 600.),
        (100, 0., 0.),
        (0, 800., -600.),
        (0, -800., 600.),
    ] {
        let source = source
            .replace("x = 100", &format!("x = {target}"))
            .replace("y = 100", &format!("y = {target}"));
        let plan = contract::compile(&source).unwrap().encode();
        let (mut host, _) = Host::boot(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            400.,
            400.,
        )
        .unwrap();
        let view = id(&host, "panel");
        let key = motion_node(host.runner.kernel().node(view).unwrap().key);
        host.dispatch_at(view, Event::PanRelease(vx, vy), 0.);
        let spring = host
            .engine
            .spring_descriptor(key, Property::Translate)
            .unwrap();
        assert_eq!(spring.velocity, exact_motion::Value::new(vx, vy));
        host.advance(1.);
        let shown = host.engine.value(key, Property::Translate).unwrap();
        if vx > 0. {
            assert!(shown.x > 0. && shown.y < 0.);
        }
        if vx < 0. {
            assert!(shown.x < 0. && shown.y > 0.);
        }
        if vx == 0. {
            assert!(shown.x > 0. && (shown.x - shown.y).abs() < 1e-8);
        }
    }
}

#[test]
fn clearing_or_refusing_authored_velocity_restores_interruption_inheritance() {
    let source = r#"component App
  state x = 0
  state supplied = true
  state speed = "800 -600"
  action release
    x = 100
  action clear
    supplied = false
    x = -100
  action refuse
    supplied = true
    speed = "bad"
    x = -200
  view
    column
      button testId="release" press=release
        text "Release"
      button testId="clear" press=clear
        text "Clear"
      button testId="refuse" press=refuse
        text "Refuse"
      box testId="panel" translate=`${x}px 0px` -exact-translate-velocity=(supplied ? speed : none) transition="translate -exact-spring(260, 24, 1)"
"#;
    let plan = contract::compile(source).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        400.,
        400.,
    )
    .unwrap();
    let panel = id(&host, "panel");
    let key = motion_node(host.runner.kernel().node(panel).unwrap().key);
    host.dispatch_at(id(&host, "release"), Event::Press, 0.);
    assert_eq!(
        host.engine
            .spring_descriptor(key, Property::Translate)
            .unwrap()
            .velocity,
        exact_motion::Value::new(800., -600.)
    );
    for (action, time) in [("clear", 30.), ("refuse", 60.)] {
        host.dispatch_at(id(&host, action), Event::Press, time);
        assert!(
            !host
                .runner
                .kernel()
                .node(panel)
                .unwrap()
                .style
                .mask
                .has(exact_kernel::StyleId::TranslateVelocity),
            "{action} clears the row"
        );
        let velocity = host
            .engine
            .spring_descriptor(key, Property::Translate)
            .unwrap()
            .velocity;
        assert_ne!(
            velocity,
            exact_motion::Value::ZERO,
            "{action} preserves current velocity"
        );
        assert_ne!(
            velocity,
            exact_motion::Value::new(800., -600.),
            "{action} does not repeat old release"
        );
    }
}
