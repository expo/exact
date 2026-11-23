//! A popover's default surface (LLP 1021; theme2 2026-10-21 18:45, 19:55):
//! the compiler marks a `popover` whose author wrote no background, border,
//! shadow or material, on it or its class, in any state (`defaultSurface`), for a host
//! that paints the popover to draw the platform's surface under it. A menu is
//! marked as any popover is, for a host that paints its rows (expo/exact#551).

use crate::source::{boxed, NoData};
use exact_kernel::{Kernel, PropId};
use exact_runner::Runner;

const APP: &str = r##"style Card
  border-width=1
  border-style="solid"

component App
  view
    column
      button popovertarget="bare" testId="open"
        text "Open"
      column id="bare" popover="auto" padding=12 width="19rem" testId="bare"
        text "Popover text"
      column id="round" popover="auto" border-radius=12 testId="round"
        text "Rounded"
      column id="filled" popover="auto" background-color="white" testId="filled"
        text "Filled"
      column id="classed" popover="auto" class=Card testId="classed"
        text "Classed"
      column id="shadowed" popover="auto" box-shadow="0 2px 8px black" testId="shadowed"
        text "Shadowed"
      column id="glass" popover="auto" backgroundMaterial="glass" testId="glass"
        text "Glass"
      column id="hovered" popover="auto" testId="hovered"
        :hover
          background-color = "#dddddd"
        text "Hovered"
      column id="pressed" popover="auto" testId="pressed"
        :active
          background-color="#dddddd"
        text "Pressed"
      column id="clipped" popover="auto" background-clip="padding-box" testId="clipped"
        text "Clipped"
      column id="menu" popover="auto" role="menu" testId="menu"
        button testId="item"
          text "Item"
      column id="menu-inked" popover="auto" role="menu" border-radius=8 padding=4 testId="menu-inked"
        :hover
          color="#0000ff"
        :active
          opacity=0.5
      column id="menu-filled" popover="auto" role="menu" background-color="#0000ff" testId="menu-filled"
      column id="menu-flat" popover="auto" role="menu" box-shadow="none" testId="menu-flat"
      column id="menu-classed" popover="auto" role="menu" class=Card testId="menu-classed"
      column id="menu-hovered" popover="auto" role="menu" testId="menu-hovered"
        :hover
          background-color="transparent"
      column id="menu-focused" popover="auto" role="menu" testId="menu-focused"
        :focus-visible
          box-shadow="none"
      column id="menu-pressed" popover="auto" role="menu" testId="menu-pressed"
        :active
          background-color="transparent"
      column testId="plain"
        text "No popover"
"##;

#[test]
fn a_popover_whose_author_wrote_no_surface_is_marked_for_the_platforms() {
    let plan = contract::bake(
        contract::compile(APP).unwrap_or_else(|e| panic!("{e}")),
        boxed(NoData),
    )
    .unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    let marked = |t: &str| {
        let node = k.node_by_key(k.find_by_test_id(t)[0]).unwrap();
        node.props.bool(PropId::DefaultSurface) == Some(true)
    };
    assert!(marked("bare"), "no surface: the platform's");
    assert!(marked("round"), "a corner draws no surface");
    assert!(marked("clipped"), "nor does a background's clip alone");
    assert!(marked("menu"), "a menu with no surface: the platform's");
    assert!(marked("menu-inked"), "a state's ink draws no surface");
    for t in [
        "filled",
        "classed",
        "shadowed",
        "glass",
        "hovered",
        "pressed",
        "plain",
        "open",
        "menu-filled",
        "menu-flat",
        "menu-classed",
        "menu-hovered",
        "menu-focused",
        "menu-pressed",
    ] {
        assert!(!marked(t), "{t}");
    }
}

#[test]
fn default_surface_is_the_compilers() {
    let e = contract::compile(
        "component App\n  view\n    column popover=\"auto\" defaultSurface=true\n      text \"x\"\n",
    )
    .unwrap_err();
    assert_eq!(e.id, "lower-default-surface", "{}", e.message);
}

#[test]
fn a_class_choices_active_arm_decides_whether_the_surface_is_authored() {
    for surface in [
        "background-color=\"transparent\"",
        "box-shadow=\"none\"",
        "border-width=0",
    ] {
        let source = format!(
            r#"style Card
  {surface}
style Plain
  border-radius=12
component App
  state on = false
  action flip
    on = not on
  view
    column
      button testId="flip" press=flip
        text "Flip"
      column popover="auto" testId="yes" class=(on ? Card : Plain)
      column popover="auto" testId="no" class=(on ? Plain : Card)
      column popover="auto" testId="own" class=(on ? Card : Plain) background-color="transparent"
      column popover="auto" testId="both" class=(on ? Card : Card)
"#
        );
        let plan = contract::bake(
            contract::compile(&source).unwrap_or_else(|e| panic!("{surface}: {e}")),
            boxed(NoData),
        )
        .unwrap();
        let mut r = Runner::boot(
            plan,
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        for on in [false, true, false] {
            for (id, expected) in [("yes", !on), ("no", on), ("own", false), ("both", false)] {
                let k = r.kernel();
                let node = k.node_by_key(k.find_by_test_id(id)[0]).unwrap();
                assert_eq!(
                    node.props.bool(PropId::DefaultSurface) == Some(true),
                    expected,
                    "{surface}: {id}, on={on}"
                );
            }
            let k = r.kernel();
            let flip = k.node_by_key(k.find_by_test_id("flip")[0]).unwrap().id;
            r.dispatch(flip, exact_runner::Event::Press).unwrap();
        }
    }
}
