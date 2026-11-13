//! LLP 1123 D3, D4, D9 as an app writes them: `-exact-safe-area` on a route,
//! `var(--exact-safe-area-inset-*)` in a length and `--exact-safe-area-inset-*`
//! declared on a box, through the compiler to the kernel's layout.

use exact_kernel::{HostCover, Kernel, Offer};
use exact_plan::Plan;
use exact_runner::Runner;

const APP: &str = r#"routes nav
  home "/"

component App
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` SAFE testId="route" position="absolute" inset=0
          column margin-top="calc(-1 * var(--exact-safe-area-inset-top))" padding-top="var(--exact-safe-area-inset-top)" testId="hero"
            text "Hero" testId="hero-text"
          column --exact-safe-area-inset-top="8px" testId="own"
            column height="var(--exact-safe-area-inset-top)" testId="own-probe"
"#;

fn boot(safe: &str) -> Runner<()> {
    let source = APP.replace("SAFE", safe);
    let plan = contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
    let plan = Plan::decode(&plan.encode()).unwrap();
    Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap()
}

/// The route as a host bar covers it: 106 points of clearance at its top,
/// its local inset.
fn covered(runner: &mut Runner<()>) -> exact_kernel::ViewId {
    let kernel = runner.kernel_mut();
    let route = kernel
        .node_by_key(kernel.find_by_test_id("route")[0])
        .unwrap()
        .id;
    kernel
        .set_local_insets(route, Some([106.0, 0.0, 0.0, 0.0]))
        .unwrap();
    kernel
        .set_host_cover(route, Some(HostCover::Edges([106.0, 0.0, 0.0, 0.0])))
        .unwrap();
    kernel
        .compute_layout(1, Offer::definite(390.0, 844.0))
        .unwrap();
    route
}

fn y(runner: &Runner<()>, test_id: &str) -> (f32, f32) {
    let kernel = runner.kernel();
    let node = kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap();
    let parent = node
        .parent
        .and_then(|p| kernel.node(p))
        .map_or(0.0, |p| p.frame.y);
    (node.frame.y + parent, node.frame.height)
}

#[test]
fn a_background_reaches_under_the_bar_and_a_box_reads_its_own_inset() {
    let mut runner = boot("");
    covered(&mut runner);
    assert_eq!(y(&runner, "hero").0, 0.0, "the background under the bar");
    assert_eq!(y(&runner, "hero-text").0, 106.0, "its text below it");
    assert_eq!(y(&runner, "own-probe").1, 8.0, "the box's own, declared");
}

#[test]
fn a_route_opted_out_of_its_top_starts_under_the_bar() {
    let mut runner = boot(r#"-exact-safe-area="top""#);
    covered(&mut runner);
    assert_eq!(
        y(&runner, "hero").0,
        -106.0,
        "nothing cleared: pulled above"
    );
    // The opt-out's idiom: the box pads the inset, and needs no margin
    let source = APP.replace("SAFE", r#"-exact-safe-area="top""#).replace(
        r#"margin-top="calc(-1 * var(--exact-safe-area-inset-top))" "#,
        "",
    );
    let plan = contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
    let plan = Plan::decode(&plan.encode()).unwrap();
    let mut runner =
        Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
    covered(&mut runner);
    assert_eq!(y(&runner, "hero").0, 0.0);
    assert_eq!(y(&runner, "hero-text").0, 106.0);
}

#[test]
fn the_opt_out_belongs_to_a_content_owner() {
    let source = r#"component App
  view
    column -exact-safe-area="top"
      text "x"
"#;
    let error = contract::compile(source).unwrap_err().to_string();
    assert!(error.contains("navigationKey"), "{error}");
    let tab = r#"component App
  view
    column role="tabpanel" -exact-safe-area="bottom left"
      text "x"
"#;
    contract::compile(tab).unwrap_or_else(|e| panic!("{e}"));
    for bad in [r#""top top""#, r#""middle""#] {
        let source = APP.replace("SAFE", &format!("-exact-safe-area={bad}"));
        assert!(contract::compile(&source).is_err(), "{bad}");
    }
    // Another `--exact-` custom property is no design token and no host's
    let other = APP.replace("SAFE", r#"padding-left="var(--exact-gutter)""#);
    assert!(contract::compile(&other).is_err());
}
