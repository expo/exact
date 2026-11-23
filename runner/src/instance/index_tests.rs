//! A computed type or family token that varies by an app axis is refused on
//! a node whose children inherit it (stage 6 C). Both facts come from the
//! site index, so a tree's first build costs the same in a large plan as in
//! a small one.

use crate::{DataError, DataSource, Event, Runner};
use exact_kernel::Kernel;
use exact_plan::Value;

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// `family.alt` varies by the `density` axis; `family.text` does not.
const DESIGN: &str = "forma t\n\naxes\n  density = regular | compact\n\ntheme\n  type.caption = -exact-caption1 \"Helvetica Neue\"\n    when density=compact: -exact-caption2 \"Helvetica Neue\"\n  family.text = \"Helvetica Neue\", system-ui\n  family.alt = \"Helvetica Neue\", system-ui\n    when density=compact: \"Menlo\", ui-monospace\n";

fn plan(name: &str, contract: &str) -> exact_plan::Plan {
    let dir = crate::TempDir(
        std::env::temp_dir().join(format!("exact2-varying-{name}-{}", std::process::id())),
    );
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("app.forma"), DESIGN).unwrap();
    std::fs::write(dir.join("app.contract"), contract).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap()
}

fn boot(plan: exact_plan::Plan) -> Runner<NoData> {
    Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn view(r: &Runner<NoData>, test_id: &str) -> u32 {
    let key = r.kernel().find_first_by_test_id(test_id).unwrap();
    r.kernel().node_by_key(key).unwrap().id
}

fn refused(r: &Runner<NoData>, test_id: &str) -> bool {
    let line = format!("view {}: invalid font-family", view(r, test_id));
    r.journal().any(|l| l.contains(&line))
}

#[test]
fn a_varying_family_is_refused_where_children_inherit_it() {
    let r = boot(plan(
        "refused",
        "component App\n  state family = \"family.alt\"\n  view\n    column testId=\"outer\" font-family=family\n      text \"a\" testId=\"leaf\" font-family=family\n      column testId=\"arm\" font-family=family\n        if family == \"\"\n          text \"b\"\n",
    ));
    assert!(refused(&r, "outer"), "a node with child nodes");
    assert!(
        refused(&r, "arm"),
        "a node whose one child is a region, its arm not shown"
    );
    assert!(!refused(&r, "leaf"), "a leaf's own text takes it");

    let r = boot(plan(
        "fixed",
        "component App\n  state family = \"family.text\"\n  view\n    column testId=\"outer\" font-family=family\n      text \"a\" font-family=family\n",
    ));
    assert!(!refused(&r, "outer"), "a token that does not vary");
}

/// A page of `font="type.caption"` texts, behind `if show`, in a plan with
/// `padding` other sites that never show.
fn page(padding: usize) -> String {
    let mut s = String::from(
        "component App\n  state show = false\n  action open\n    show = true\n  view\n    column\n      button press=open testId=\"open\"\n        text \"Open\"\n      if show\n        column\n          for i in range(200) key=i\n            text \"t\" font=\"type.caption\"\n      if show and not show\n        column\n",
    );
    for _ in 0..padding {
        s.push_str("          text \"p\"\n");
    }
    s
}

/// One build of the page: its seconds and the nodes it made.
fn build(plan: &exact_plan::Plan) -> (f64, usize) {
    let mut r = boot(plan.clone());
    let open = view(&r, "open");
    let at = std::time::Instant::now();
    r.dispatch(open, Event::Press).unwrap();
    let took = at.elapsed().as_secs_f64();
    (took, r.last_instance_work().nodes_visited)
}

#[test]
fn a_first_build_costs_the_same_in_a_large_plan() {
    // Each text's type rows name a varying token. Read from a scan of the
    // plan, 8,000 more sites made the build ten times as long.
    let plans = [plan("small", &page(0)), plan("large", &page(8000))];
    // The fastest of five builds of each, taken in turns.
    let mut best = [f64::INFINITY; 2];
    for _ in 0..5 {
        for (plan, best) in plans.iter().zip(&mut best) {
            let (took, nodes) = build(plan);
            assert!(nodes > 200, "the page was built: {nodes} nodes");
            *best = best.min(took);
        }
    }
    let [small, large] = best;
    assert!(
        large < small * 2.0,
        "200 texts: {:.1} ms in a small plan, {:.1} ms in a large one",
        small * 1e3,
        large * 1e3
    );
}
