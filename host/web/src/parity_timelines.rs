//! Scroll and view timelines against the browser's own (LLP 1057.003 D5;
//! Scroll-driven Animations 1). [`SOURCE`] declares them on fixed boxes in
//! five scrollers; `parity.mjs --timelines` builds it for the web, where the
//! browser runs every timeline itself, scrolls each scroller through
//! [`STEPS`]' offsets and records each consumer's computed opacity
//! (`tests/fixtures/browser-timelines.json`). [`check`] lays the same app
//! out in the kernel, places its timelines and holds the engine, given the
//! same offsets, to every recorded sample: what each native host computes
//! from its scroll callback.

use exact_kernel::{Kernel, MotionSync, Offer};
use exact_motion::{Engine, Property};
use exact_plan::{Plan, Value};
use exact_runner::{DataError, DataSource, Runner};

/// The page the recorder opens, in CSS px.
pub const VIEWPORT: [f32; 2] = [420.0, 900.0];

/// Fixed boxes with no text, so layout does not depend on fonts. Each
/// consumer's own opacity is 0.25, which shows when its timeline is
/// inactive; on an active one `both` fills every offset with 0 to 1.
pub const SOURCE: &str = include_str!("../tests/fixtures/timelines.contract");

/// Each scroller, its axis, the offsets it is scrolled to (whole points,
/// which every browser shows exactly) and the consumers read at each.
pub const STEPS: &str = include_str!("../tests/fixtures/timelines-steps.json");

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// The kernel and engine with [`SOURCE`] laid out in [`VIEWPORT`] and its
/// timelines placed, as a native host boots it.
fn boot() -> (Runner<NoData>, Engine) {
    let plan = contract::compile(SOURCE).expect("the timelines fixture compiles");
    let mut runner = Runner::boot(
        Plan::decode(&plan.encode()).expect("a plan"),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .expect("the timelines fixture boots");
    let [w, h] = VIEWPORT;
    let k = runner.kernel_mut();
    let env = exact_kernel::style::Env {
        viewport_width: w,
        viewport_height: h,
        ..k.env()
    };
    k.set_env(env).expect("a viewport");
    let root = k.roots()[0];
    k.compute_layout(root, Offer::definite(w, h))
        .expect("the timelines fixture lays out");
    let mut engine = Engine::new();
    let mut sync = MotionSync::default();
    let keys: Vec<_> = (k.arena().iter_live()).map(|s| k.arena().key(s)).collect();
    for key in keys {
        k.motion_sync_node(key, &mut sync);
    }
    sync.apply(&mut engine)
        .expect("kernel rows are engine input");
    for (node, binding) in k.place_timelines() {
        engine.set_animation_timeline(node, binding);
    }
    (runner, engine)
}

/// Every recorded sample the engine disagrees with by more than
/// [`crate::parity::TOLERANCE`], as `scroller@offset consumer: engine, browser`.
pub fn check(fixture: &str) -> Vec<String> {
    let recorded: serde_json::Value = serde_json::from_str(fixture).expect("a JSON fixture");
    let steps: serde_json::Value = serde_json::from_str(STEPS).expect("JSON steps");
    let (runner, mut engine) = boot();
    let k = runner.kernel();
    let node = |id: &str| exact_kernel::motion_node(k.find_by_test_id(id)[0]);
    let mut wrong = Vec::new();
    for sample in recorded["samples"].as_array().expect("samples") {
        let scroller = sample["scroller"].as_str().expect("a scroller");
        let at = sample["at"].as_f64().expect("an offset");
        let step = (steps.as_array().expect("steps").iter())
            .find(|s| s["scroller"] == scroller)
            .expect("a step for each sample");
        let (x, y) = if step["axis"] == "x" {
            (at, 0.0)
        } else {
            (0.0, at)
        };
        engine.set_scroll_offset(node(scroller), x, y);
        engine.frame();
        for (id, browser) in sample["values"].as_object().expect("values") {
            let browser = browser.as_f64().expect("a number");
            let shown = engine
                .sampled_value(node(id), Property::Opacity)
                .map_or(f64::NAN, |v| v.x);
            if (shown - browser).abs() > crate::parity::TOLERANCE || shown.is_nan() {
                wrong.push(format!("{scroller}@{at} {id}: {shown}, browser {browser}"));
            }
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    /// The engine, placed by the kernel's layout, against Chrome's own
    /// scroll and view timelines on the same app.
    #[test]
    fn scroll_and_view_timelines_match_the_browsers() {
        let fixture = include_str!("../tests/fixtures/browser-timelines.json");
        let wrong = super::check(fixture);
        assert!(wrong.is_empty(), "{wrong:#?}");
    }
}
