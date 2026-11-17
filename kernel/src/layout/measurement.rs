//! Measured-leaf facts and pending geometry (LLP 1051.000 D5).
use super::*;

// @ref LLP 1043.000 §3 D3 — keep the proof with Taffy's measured leaf.
#[derive(Default, Clone, Debug)]
pub(super) struct MeasureContext {
    pub(super) slot: u32,
    pub(super) pending: bool,
    pub(super) pending_pass: u64,
    pub(super) asynchronous: bool,
    pub(super) kind: alert::Kind,
    pub(super) pass: u64,
    pub(super) height_measured: bool,
    pub(super) text_item: Option<Frame>,
    // At most LEAF_OFFERS per live leaf; text/style invalidation clears them.
    pub(super) measurements: Vec<Measurement>,
    // @ref LLP 1043.000 §8 — the shapes an admitted auto-height leaf is
    // measured around (border-box coordinates), written only by settle_flow;
    // changing them clears `measurements`, whose key does not include them.
    pub(super) flow: Vec<exact_textflow::FlowShape>,
    // A fitted paragraph's unscaled min-content width (`fit`), cleared with
    // `measurements`.
    pub(super) fit_natural: Option<f32>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Measurement {
    pub(super) width: AxisOffer,
    pub(super) height: AxisOffer,
    pub(super) metrics: TextMetrics,
    // The scale a fitted paragraph was measured at; 1 for any other.
    pub(super) scale: f32,
}

impl MeasureContext {
    pub(super) fn begin(&mut self, pass: u64) {
        if self.pending_pass != pass {
            self.pending_pass = pass;
            self.pending = false;
        }
    }

    pub(super) fn remember(&mut self, measurement: Measurement) {
        if measurement.metrics.pending {
            self.asynchronous = true;
            self.measurements.clear();
        }
        // A worker's current accepted width supersedes historical scalar widths.
        if self.asynchronous {
            return;
        }
        if self.measurements.len() == LEAF_OFFERS {
            self.measurements.remove(0);
        }
        self.measurements.push(measurement);
    }
}

impl LayoutTree {
    /// A height-auto query publishes nothing, including pending flags.
    pub(crate) fn restore_text_pending(&mut self) {
        for (node, pending) in self.pending_text_changes.drain() {
            if let Some(context) = self.taffy.get_node_context_mut(node) {
                context.pending = pending;
            }
        }
    }

    pub(crate) fn text_pending(&self, arena: &NodeArena, slot: u32) -> bool {
        arena
            .taffy(slot)
            .into_iter()
            .chain(self.alert_prefixes.get(&slot).copied())
            .any(|node| self.taffy.get_node_context(node).is_some_and(|c| c.pending))
    }
}
