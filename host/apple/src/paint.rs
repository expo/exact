//! Paint motion (LLP 1055.000 D6, LLP 1062): colour and shadow transitions
//! and keyframes, sampled by the engine like every other property.
//!
//! Ownership, `currentcolor` and per-view appearance are shared in
//! `PaintMotion`; this host presents the values as styles over the rows.

use super::Host;
use crate::batch::Batch;
use crate::style::Shown;
use exact_kernel::motion::{node_key, PaintMotion};
use exact_kernel::{CommitReceipt, NodeKey, ViewId};
use exact_motion::Property;
use exact_runner::DataSource;
use std::collections::BTreeMap;

/// The host's paint-motion state.
#[derive(Debug, Default)]
pub(super) struct Paint {
    pub(super) motion: PaintMotion,
    /// Inline runs painting an inherited `color` that moves, by run: their
    /// paragraph paints them (LLP 1062 D5).
    pub(super) runs: BTreeMap<ViewId, exact_motion::Value>,
    /// Inline runs' own `opacity` other than 1, as their paragraph last
    /// painted it: a frame that moves it paints the paragraph again.
    pub(super) opacities: BTreeMap<ViewId, f64>,
}

impl<D: DataSource> Host<D> {
    /// Adopt a commit's paint, after its `motion_sync` set the rows.
    pub(super) fn sync_paint(&mut self, receipt: &CommitReceipt, batch: &mut Batch) {
        let retired = self
            .paint
            .motion
            .sync(self.runner.kernel(), receipt, &mut self.engine);
        self.retire_paint(retired, batch);
    }

    /// Adopt the whole tree's paint at boot.
    pub(super) fn boot_paint(&mut self, order: &[ViewId]) {
        let kernel = self.runner.kernel();
        let keys: Vec<NodeKey> = order
            .iter()
            .filter_map(|id| kernel.node(*id).map(|n| n.key))
            .collect();
        self.paint.motion.adopt(kernel, keys, &mut self.engine);
    }

    fn retire_paint(&mut self, retired: Vec<(u64, Property)>, batch: &mut Batch) {
        let mut views: BTreeMap<ViewId, bool> = BTreeMap::new();
        for (node, property) in retired {
            if let Some(view) = self.keys.get(&node_key(node)).copied() {
                *views.entry(view).or_default() |= property == Property::Color;
            }
        }
        for (view, inherits) in views {
            self.present_colors(view, batch, inherits);
        }
    }

    /// What a node's paint shows now, over its rows: each owned property's
    /// value while it differs from its target. A `currentcolor` side that
    /// stays one follows the presented `color` through its row.
    pub(super) fn shown_paint(&self, node: u64) -> Shown {
        let mut shown = Shown::default();
        for property in Property::PAINT {
            shown.set(
                property,
                self.paint.motion.shown(&self.engine, node, property),
            );
        }
        shown
    }

    /// The presenter's appearance: a `light-dark()` colour an owner shows
    /// resolves by it. A change re-targets every owner, transitioning under
    /// its row; the first report only corrects boot's guess, without motion.
    pub fn set_scheme(&mut self, dark: bool) -> String {
        let mut batch = Batch::new();
        if let Some(retired) = self.paint.motion.set_scheme(
            self.runner.kernel(),
            &mut self.engine,
            dark,
            self.now_ms / 1000.0,
        ) {
            self.retire_paint(retired, &mut batch);
            self.svg.all_dirty();
            self.present(&mut batch, false);
        }
        self.finish(batch, None)
    }

    /// The platform's colours resolve differently (LLP 1095 D1): paint
    /// motion re-targets, transitioning under each owner's row (D6).
    /// `first`: the session's first report, a correction without motion.
    pub(super) fn repaint_colors(&mut self, batch: &mut Batch, first: bool) {
        if let Some(retired) = self.paint.motion.colors_changed(
            self.runner.kernel(),
            &mut self.engine,
            self.now_ms / 1000.0,
            first,
        ) {
            self.retire_paint(retired, batch);
        }
    }

    /// A view's own appearance, when the presenter finds it differs from the
    /// session's (LLP 1062 D4): its node's colours resolve by it. The first
    /// report for a view corrects what was presented without motion, as the
    /// session's first does; a later change transitions, and so does a view
    /// that agrees with the session again.
    pub fn set_view_scheme(&mut self, view: ViewId, dark: bool) -> String {
        let mut batch = Batch::new();
        let key = self.runner.kernel().node(view).map(|n| n.key);
        if let Some(key) = key {
            if let Some(retired) = self.paint.motion.set_view_scheme(
                self.runner.kernel(),
                &mut self.engine,
                key,
                dark,
                self.now_ms / 1000.0,
            ) {
                self.retire_paint(retired, &mut batch);
                self.svg.all_dirty();
                self.present(&mut batch, false);
            }
        }
        self.finish(batch, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_runner::{DataError, Event};

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

    #[test]
    fn destroying_an_inline_run_releases_its_inherited_paint() {
        let plan = contract::compile(
            "component App\n  state on = false\n  state shown = true\n  action go\n    on = true\n  action hide\n    shown = false\n  view\n    column color=(on ? \"#ffffff\" : \"#000000\") transition=\"color 1s linear\"\n      button \"Go\" testId=\"go\" press=go\n      button \"Hide\" testId=\"hide\" press=hide\n      text\n        if shown\n          text \"Run\" testId=\"run\"\n",
        ).unwrap();
        let (mut host, _) = Host::boot(
            &plan.encode(),
            NoData,
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            390.0,
            844.0,
        )
        .unwrap();
        let view = |host: &Host<NoData>, name| {
            let kernel = host.runner().kernel();
            kernel
                .node_by_key(kernel.find_by_test_id(name)[0])
                .unwrap()
                .id
        };
        let run = view(&host, "run");
        host.dispatch_at(view(&host, "go"), Event::Press, 0.0);
        host.tick(500.0);
        assert!(host.paint.runs.contains_key(&run));
        host.dispatch_at(view(&host, "hide"), Event::Press, 500.0);
        assert!(!host.paint.runs.contains_key(&run));
    }

    /// The `opacity` the last `paragraph` op in `batch` gives run `id`:
    /// `None` when no op in it names the run.
    fn run_opacity(batch: &str, id: u32) -> Option<f64> {
        let ops: serde_json::Value = serde_json::from_str(batch).unwrap();
        let ops = ops.get("ops").unwrap_or(&ops).as_array().unwrap();
        ops.iter()
            .filter(|op| op["op"] == "paragraph")
            .flat_map(|op| op["runs"].as_array().unwrap())
            .filter(|run| run["id"] == id)
            .map(|run| run["opacity"].as_f64().unwrap_or(1.0))
            .next_back()
    }

    fn opacity_host(source: &str) -> (Host<NoData>, String) {
        let plan = contract::compile(source).unwrap();
        Host::boot(
            &plan.encode(),
            NoData,
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            390.0,
            844.0,
        )
        .unwrap()
    }

    fn test_view(host: &Host<NoData>, name: &str) -> ViewId {
        let kernel = host.runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(name)[0])
            .unwrap()
            .id
    }

    /// CSS Color 3 §3.2: `opacity` applies to every element, a span in a
    /// paragraph included, and a nested one's composes with its own.
    #[test]
    fn an_inline_runs_opacity_reaches_its_paragraph() {
        let (host, boot) = opacity_host(
            "component App\n  view\n    column\n      text\n        text \"Posted \" testId=\"plain\"\n        text \"yesterday\" opacity=0.3 testId=\"faded\"\n        text opacity=0.5\n          text \"at noon\" opacity=0.5 testId=\"nested\"\n",
        );
        assert_eq!(run_opacity(&boot, test_view(&host, "plain")), Some(1.0));
        assert_eq!(run_opacity(&boot, test_view(&host, "faded")), Some(0.3));
        assert_eq!(run_opacity(&boot, test_view(&host, "nested")), Some(0.25));
        assert_eq!(boot.matches("\"op\":\"paragraph\"").count(), 1, "{boot}");
    }

    #[test]
    fn a_button_title_with_opacity_keeps_its_paint_node() {
        let (host, boot) = opacity_host(
            "component App\n  view\n    button testId=\"button\" opacity=0.5\n      text \"Title\" opacity=0.25 testId=\"title\"\n",
        );
        let title = test_view(&host, "title");
        let ops: serde_json::Value = serde_json::from_str(&boot).unwrap();
        assert!(
            ops["ops"]
                .as_array()
                .unwrap()
                .iter()
                .any(|op| op["op"] == "create" && op["id"] == title),
            "a faded title needs its own paint node: {boot}"
        );
    }

    #[test]
    fn native_button_titles_refuse_opacity_instead_of_dropping_it() {
        for opacity in [0., 0.25, 1.] {
            let error = contract::compile(&format!(
                "component App\n  view\n    button appearance=\"auto\" opacity=0.5\n      text \"Title\" opacity={opacity}\n"
            )).unwrap_err();
            assert_eq!(error.id, "lower-button-style-attr");
            assert!(error.message.contains("takes no `opacity`"), "{error}");
        }
    }

    #[test]
    fn an_inline_runs_opacity_transition_repaints_its_paragraph() {
        let (mut host, _) = opacity_host(
            "component App\n  state on = false\n  action go\n    on = true\n  view\n    column\n      button \"Go\" testId=\"go\" press=go\n      text\n        text \"Posted \"\n        text \"yesterday\" testId=\"run\" opacity=(on ? 0.2 : 1) transition=\"opacity 1s linear\"\n",
        );
        let run = test_view(&host, "run");
        host.dispatch_at(test_view(&host, "go"), Event::Press, 0.0);
        let shown = run_opacity(&host.tick(500.0), run).expect("a frame repaints the run");
        assert!((shown - 0.6).abs() < 0.01, "half way: {shown}");
        let shown = run_opacity(&host.tick(1500.0), run).expect("the end repaints the run");
        assert!((shown - 0.2).abs() < 1e-9, "at rest: {shown}");
    }

    #[test]
    fn an_inline_runs_opacity_animation_is_sampled_into_its_paragraph() {
        let (mut host, boot) = opacity_host(
            "keyframes pulse\n  from opacity=1\n  to opacity=0.2\ncomponent App\n  view\n    column\n      text\n        text \"Posted \"\n        text \"yesterday\" testId=\"run\" animation=\"pulse 1000ms linear infinite alternate\"\n",
        );
        let run = test_view(&host, "run");
        assert!(
            !boot.contains(&format!("\"op\":\"animations\",\"id\":{run}")),
            "a run has no view to play a spec on: {boot}"
        );
        host.tick(0.0);
        let shown = run_opacity(&host.tick(500.0), run).expect("a frame repaints the run");
        assert!((shown - 0.6).abs() < 0.01, "half way: {shown}");
    }
}
