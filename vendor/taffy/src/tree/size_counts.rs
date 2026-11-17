//! EXACT PATCH 72: size computations, separate from stores made by row replay.

use super::{CollapsibleMarginSet, LayoutInput, LayoutOutput, NodeId, RunMode};
use std::collections::{HashMap, HashSet};

/// Counts since the caller last reset them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SizeCacheCounts {
    /// Engine entry points called.
    pub passes: usize,
    /// Computed size answers eligible for storage.
    pub computed: usize,
    /// Computations whose question was already answered since the last clear in this pass.
    pub repeated: usize,
    /// Replays stopped after writing at least one answer.
    pub stopped_replays: usize,
    /// Most answers held at a node visited by a successful row replay.
    pub replayed_max_answers: usize,
    /// Most size answers held by one node.
    pub max_answers: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SizeCounts {
    pub enabled: bool,
    pub counts: SizeCacheCounts,
    answered: HashMap<NodeId, Vec<LayoutInput>>,
    pub peaks: HashMap<NodeId, usize>,
    pub most_held: usize,
    pub over_nine: HashSet<NodeId>,
}

pub(crate) fn stores_size(input: &LayoutInput, output: &LayoutOutput) -> bool {
    input.run_mode == RunMode::ComputeSize
        && !output.margins_can_collapse_through
        && output.top_margin == CollapsibleMarginSet::ZERO
        && output.bottom_margin == CollapsibleMarginSet::ZERO
}

impl SizeCounts {
    #[inline]
    pub fn begin(&mut self) {
        if !self.enabled { return; }
        self.counts.passes += 1;
        self.answered.clear();
    }

    #[inline]
    pub fn computed(&mut self, node: NodeId, input: &LayoutInput, output: &LayoutOutput) {
        if !self.enabled { return; }
        if !stores_size(input, output) {
            return;
        }
        self.counts.computed += 1;
        self.counts.repeated += self.answered.get(&node).is_some_and(|qs| qs.contains(input)) as usize;
    }

    #[inline]
    pub fn stored(&mut self, node: NodeId, input: &LayoutInput, output: &LayoutOutput, held: usize) {
        if !self.enabled { return; }
        self.counts.max_answers = self.counts.max_answers.max(held);
        let peak = self.peaks.entry(node).or_default();
        *peak = (*peak).max(held);
        self.most_held = self.most_held.max(held);
        if held > 9 { self.over_nine.insert(node); }
        if stores_size(input, output) {
            let questions = self.answered.entry(node).or_default();
            if !questions.contains(input) {
                questions.push(*input);
            }
        }
    }

    #[inline]
    pub fn clear(&mut self, node: NodeId) {
        if !self.enabled { return; }
        self.answered.remove(&node);
    }
}
