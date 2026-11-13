//! `-exact-hit-slop` (Charlie, 2026-11-10): a press just past a small
//! button's box presses it, layout unmoved; a button painted over the slop
//! keeps its own presses.
use exact_kernel::PropId;
use exact_painted::{presenter::PainterChoice, Presenter};
use std::path::PathBuf;

fn app(slop: &str) -> String {
    format!(
        r#"component App
  state small = 0
  state next = 0
  action pressSmall
    small = small + 1
  action pressNext
    next = next + 1
  view
    box testId="page" position="relative" width=400 height=400
      button testId="small" appearance="none" position="absolute" left=100 top=100 width=24 height=24 padding=0 border-width=0 press=pressSmall{slop}
      button testId="next" appearance="none" position="absolute" left=130 top=100 width=40 height=24 padding=0 border-width=0 press=pressNext
      text `${{small}}` testId="smallCount" position="absolute" top=300
      text `${{next}}` testId="nextCount" position="absolute" top=330
"#
    )
}

fn boot(slop: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &contract::compile(&app(slop)).unwrap().encode(),
        (),
        (400., 400.),
        1.,
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn id(p: &Presenter<()>, name: &str) -> u32 {
    let key = p.host().kernel().find_by_test_id(name)[0];
    p.host().kernel().node_by_key(key).unwrap().id
}

fn count(p: &Presenter<()>, name: &str) -> String {
    let node = p.host().kernel().node(id(p, name)).unwrap();
    node.props.str(PropId::Text).unwrap().to_string()
}

fn click(p: &mut Presenter<()>, x: f32, y: f32) {
    let now = p.host().now();
    p.pointer_down(x, y, now).unwrap();
    p.pointer_up(x, y, now + 16.).unwrap();
}

#[test]
fn a_press_in_the_slop_presses_the_button_and_layout_is_unmoved() {
    let mut plain = boot("");
    let mut p = boot(r#" -exact-hit-slop="10px""#);
    let frame = |p: &Presenter<()>| p.host().kernel().node(id(p, "small")).unwrap().frame;
    assert_eq!(frame(&p), frame(&plain), "layout never sees the slop");
    let (small, next) = (id(&p, "small"), id(&p, "next"));
    assert_eq!(p.hit(95., 112.), Some(small));
    assert_eq!(p.hit(112., 92.), Some(small));
    assert_eq!(p.hit(127., 112.), Some(small));
    assert_ne!(p.hit(88., 112.), Some(small));
    assert_eq!(
        p.hit(132., 112.),
        Some(next),
        "the front button keeps its box"
    );
    assert_ne!(plain.hit(95., 112.), Some(id(&plain, "small")));
    click(&mut p, 95., 112.);
    assert_eq!(count(&p, "smallCount"), "1");
    click(&mut p, 132., 112.);
    assert_eq!(count(&p, "nextCount"), "1");
    click(&mut plain, 95., 112.);
    assert_eq!(count(&plain, "smallCount"), "0");
}
