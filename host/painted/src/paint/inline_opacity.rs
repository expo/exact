//! Inline opacity composites the box's paints and descendants as one group.
//! The paragraph's own opacity belongs to its existing box layer.
use crate::text::RunPaint;
use exact_kernel::{Kernel, ViewId};
use std::collections::HashMap;
use std::ops::Range;

#[derive(Clone)]
struct Group {
    source: ViewId,
    opacity: f32,
    range: Range<usize>,
    children: Vec<usize>,
}

#[derive(Clone)]
pub(crate) struct Groups(Vec<Group>);

pub(crate) enum Paint {
    Begin(usize, f32),
    Text(Range<usize>),
    End(usize, f32),
}

/// A run's inline boxes, outermost first, each with its opacity.
pub(crate) type Chain = Box<[(ViewId, f32)]>;

/// Each run's chain of inline boxes, from the run up to the paragraph.
pub(crate) fn chains(
    kernel: &Kernel,
    palette: &[RunPaint],
    opacity: impl Fn(ViewId) -> f32,
) -> Box<[Chain]> {
    palette
        .iter()
        .map(|run| {
            let mut chain = Vec::new();
            let mut at = Some(run.source);
            while let Some(node) = at.and_then(|id| kernel.node(id)) {
                if !node.is_inline_run() {
                    break;
                }
                chain.push((node.id, opacity(node.id).clamp(0., 1.)));
                at = node.parent;
            }
            chain.reverse();
            chain.into_boxed_slice()
        })
        .collect()
}

impl Groups {
    pub(crate) fn capture(
        kernel: &Kernel,
        owner: ViewId,
        palette: &[RunPaint],
        opacity: impl Fn(ViewId) -> f32,
    ) -> Self {
        Self::new(owner, &chains(kernel, palette, opacity))
    }

    /// The groups of the boxes in `chains` that are not opaque.
    pub(crate) fn new(owner: ViewId, chains: &[Chain]) -> Self {
        let mut groups = vec![Group {
            source: owner,
            opacity: 1.,
            range: 0..chains.len(),
            children: Vec::new(),
        }];
        let mut by_id = HashMap::new();
        for (index, chain) in chains.iter().enumerate() {
            let mut parent = 0;
            for &(source, opacity) in chain.iter().filter(|(_, opacity)| *opacity < 1.) {
                let child = *by_id.entry(source).or_insert_with(|| {
                    let child = groups.len();
                    groups.push(Group {
                        source,
                        opacity,
                        range: index..index + 1,
                        children: Vec::new(),
                    });
                    groups[parent].children.push(child);
                    child
                });
                groups[child].range.end = index + 1;
                parent = child;
            }
        }
        Self(groups)
    }

    pub(super) fn owner(&self, kernel: &Kernel, source: ViewId) -> usize {
        let mut at = Some(source);
        while let Some(node) = at.and_then(|id| kernel.node(id)) {
            if let Some(index) = self.0.iter().position(|g| g.source == node.id) {
                return index;
            }
            if !node.is_inline_run() {
                break;
            }
            at = node.parent;
        }
        0
    }

    pub(crate) fn visit(&self, paint: &mut impl FnMut(Paint)) {
        self.group(0, paint);
    }

    fn group(&self, index: usize, paint: &mut impl FnMut(Paint)) {
        let group = &self.0[index];
        if group.opacity <= 0. {
            return;
        }
        paint(Paint::Begin(index, group.opacity));
        let mut from = group.range.start;
        for &child in &group.children {
            let range = &self.0[child].range;
            if from < range.start {
                paint(Paint::Text(from..range.start));
            }
            self.group(child, paint);
            from = range.end;
        }
        if from < group.range.end {
            paint(Paint::Text(from..group.range.end));
        }
        paint(Paint::End(index, group.opacity));
    }
}
