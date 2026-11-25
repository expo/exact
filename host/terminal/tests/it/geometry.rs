//! The geometry facts (LLP 1123 D6) on the terminal: answered from its layout, with no owner, no
//! safe area and no native targets.

use exact_terminal::host::{Host, Mode};
use exact_terminal::vt::Vt;

const APP: &str = "shape Rect
  x: number
  y: number
  width: number
  height: number
shape Insets
  top: number
  right: number
  bottom: number
  left: number
shape Observed
  safeArea: option<Insets>
shape G
  available: bool
  frame: option<Rect>
  observations: option<Observed>
shape Chain
  available: bool
component App
  resource g = exactGeometry(\"box\") as shape G
  resource c = exactNativeChain(\"box\") as shape Chain
  derive width = match g.frame { case some(f) => f.width, case none => -1 }
  derive top = match g.observations { case some(o) => match o.safeArea { case some(s) => s.top, case none => -1 }, case none => -1 }
  view
    column width=\"100%\" height=\"100%\"
      column id=\"box\" width=\"50%\" height=\"2lh\"
      text `box ${g.available} ${width} top ${top} chain ${c.available}`
";

#[test]
fn the_facts_answer_from_the_terminals_layout() {
    // After the first layout the box's fact is available with its laid-out width and a safe area
    // of 0, and the native chain, which a terminal has none of, is unavailable (LLP 1123: the
    // terminal has no safe areas, every band is 0 and native targets are unavailable)
    let plan = contract::compile_terminal(APP).unwrap();
    let mut host = Host::boot(plan, (), Mode::Fullscreen, 40, 6).unwrap();
    host.arm = false;
    let mut vt = Vt::new(&mut host);
    vt.render(&mut host);
    // Half of 40 columns of 8
    let screen = vt.text(false);
    assert!(
        screen.contains("box true 160 top 0 chain false"),
        "{screen}"
    );
    // A resize lays the box out again, and its fact answers again
    host.resize(60, 6);
    vt.render(&mut host);
    let screen = vt.text(false);
    assert!(screen.contains("box true 240 top 0"), "{screen}");
}
