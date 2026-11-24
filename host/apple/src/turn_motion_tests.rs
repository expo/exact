//! The turn's motion link (LLP 1047.001 D3, `TurnLinks::motion`): exits,
//! layout and height transitions and list moves run only where something can
//! move. A still plan links none of them and runs the same; a plan that moves
//! links them and runs as an archive that links everything does.
use super::*;
use crate::link::{Links, TurnLinks};
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

fn id(host: &Host<NoData>, test_id: &str) -> u32 {
    let k = host.runner.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// Every batch of a boot, two presses and the frames after each, with `turn`.
fn run(source: &str, turn: TurnLinks<NoData>) -> Vec<String> {
    run_linked(source, Links { turn, ..Links::ALL })
}

/// [`run`] with the whole link set `links`.
fn run_linked(source: &str, links: Links<NoData>) -> Vec<String> {
    let plan = contract::compile(source).unwrap().encode();
    let (mut host, boot) = Host::boot_stored_after_decode(
        PlanBytes::Copied(&plan),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        scale::LaunchLayout::NONE,
        exact_runner::Viewport::sized(390.0, 844.0),
        None,
        Vec::new(),
        None,
        None,
        None,
        None,
        "/",
        false,
        None,
        links,
        None,
        |_| Ok(()),
    )
    .unwrap();
    host.commit_boot();
    let mut batches = vec![boot];
    for at in [100.0, 2_000.0] {
        batches.push(host.dispatch_at(id(&host, "go"), Event::Press, at));
        for dt in [100.0, 300.0, 1_200.0] {
            batches.push(host.tick(at + dt));
        }
    }
    batches
}

fn uses(source: &str) -> crate::link::Uses {
    exact_runner::uses::apple_uses(&contract::compile(source).unwrap())
}

const STILL: &str = r##"component A
  state on = false
  action go
    on = not on
  view
    column
      button press=go testId="go"
        text "Go"
      box width=(on ? 80 : 60) height=(on ? 40 : 60) opacity=(on ? 0.5 : 1) transform=(on ? "translateX(4px)" : "none")
        text (on ? "b" : "a")
      if on
        text "shown"
"##;

#[test]
fn a_still_plan_links_no_turn_motion_and_runs_the_same() {
    assert!(TurnLinks::<NoData>::of(uses(STILL)).motion.is_none());
    // Its whole by-use link set (the settle-only engine too) against everything.
    assert_eq!(
        run_linked(STILL, Links::of(uses(STILL))),
        run_linked(STILL, Links::ALL)
    );
}

#[test]
fn a_plan_that_moves_links_the_turn_motion_and_runs_as_everything_linked() {
    use crate::link::Capability;
    // Each part, the keyframes it needs, and the one capability that links it.
    let moving = [
        // A height transition.
        (
            "box testId=\"h\" height=(on ? 120 : 40) transition=\"height 1s linear\" overflow=\"hidden\"\n        text \"a\"",
            "",
            Capability::Transitions,
        ),
        // A layout transition.
        (
            "box width=(on ? 200 : 60) height=40 -exact-layout-transition=\"1s linear\"",
            "",
            Capability::Transitions,
        ),
        // An exit.
        (
            "if not on\n        box width=40 height=40 -exact-exit-animation=\"fade 1s linear\"",
            "keyframes fade\n  from opacity=1\n  to opacity=0\n",
            Capability::Animations,
        ),
    ];
    let motion = [
        Capability::Motion,
        Capability::Drag,
        Capability::Animations,
        Capability::Timelines,
        Capability::Transitions,
    ];
    for (part, keyframes, links) in moving {
        let source = format!(
            "{keyframes}component A\n  state on = false\n  action go\n    on = not on\n  view\n    column\n      button press=go testId=\"go\"\n        text \"Go\"\n      {part}\n"
        );
        let used = uses(&source);
        assert_eq!(
            motion.iter().filter(|c| used.has(**c)).collect::<Vec<_>>(),
            [&links],
            "{part}"
        );
        assert!(TurnLinks::<NoData>::of(used).motion.is_some(), "{part}");
        let linked = run(&source, TurnLinks::of(uses(&source)));
        assert_eq!(
            linked,
            run(&source, TurnLinks::of(crate::link::ALL)),
            "{part}"
        );
        // And the link is what plays it.
        assert_ne!(
            linked,
            run(&source, TurnLinks::of(crate::link::Uses::NONE)),
            "{part}"
        );
    }
}

const HEIGHT_DRAG: &str = r#"component App
  state open = true
  action release(height: number, velocity: number)
    open = false
  view
    box id="sheet" testId="sheet" height=200 box-sizing="border-box"
      box testId="header" heightDragFor="sheet" heightrelease=release
"#;

/// The drags link with `drag` (`TurnLinks::drag`), which every handler that
/// registers a drag handle selects; a still plan links none of it. An
/// archive without it registers no handle and refuses the drag entry points,
/// and one with it begins the drag.
#[test]
fn drags_link_with_drag_and_an_archive_without_refuses_their_entry_points() {
    assert!(TurnLinks::<NoData>::of(uses(STILL)).drag.is_none());
    let used = uses(HEIGHT_DRAG);
    assert!(used.has(crate::link::Capability::Drag));
    assert!(TurnLinks::<NoData>::of(used).drag.is_some());
    let begin = |turn: TurnLinks<NoData>| {
        let plan = contract::compile(HEIGHT_DRAG).unwrap().encode();
        let (mut host, _) = Host::boot_stored_after_decode(
            PlanBytes::Copied(&plan),
            NoData,
            Box::new(MonospaceMeasurer::default()),
            scale::LaunchLayout::NONE,
            exact_runner::Viewport::sized(800.0, 900.0),
            None,
            Vec::new(),
            None,
            None,
            None,
            None,
            "/",
            false,
            None,
            Links { turn, ..Links::ALL },
            None,
            |_| Ok(()),
        )
        .unwrap();
        host.commit_boot();
        let header = host.runner.kernel().find_by_test_id("header")[0];
        let sheet = host.runner.kernel().find_by_test_id("sheet")[0];
        let begun = host.height_drag_begin(header, sheet, 5_000.0);
        let entries = [
            host.transform_motion(&[0; 120]).finish(),
            host.height_drag_update(1, 120.0, 5_016.0, 5_016.0),
            host.dispatch_height_measured(1, 120.0, 5_032.0, 5_032.0),
            host.dispatch_height_held(1, 120.0, 0.0, 5_048.0),
        ];
        (host.height_handles.len(), begun, entries)
    };
    let (handles, linked, _) = begin(TurnLinks::of(used));
    assert_eq!(handles, 1);
    assert!(linked.contains("\"token\""), "{linked}");
    let (handles, unlinked, entries) = begin(TurnLinks::of(crate::link::Uses::NONE));
    assert_eq!(handles, 0, "no handle is registered without the link");
    for refused in std::iter::once(&unlinked).chain(&entries) {
        assert!(refused.contains("links no drag"), "{refused}");
    }
}
