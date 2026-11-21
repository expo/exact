//! Match a replacement's first frame under parents that keep their native view.
use super::place::Part;
use super::{Ids, Tree};
use exact_kernel::{Appearance, Kernel, NodeRef, NodeType, PropId, ViewId};
use exact_plan::Plan;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
struct View {
    id: ViewId,
    shape: Shape,
}

#[derive(Debug, Clone, PartialEq)]
struct Shape {
    tag: String,
    kind: NodeType,
    events: Vec<u8>,
    literal_props: [Option<String>; 4],
    appearance: Option<Appearance>,
    virtualized: bool,
}

impl View {
    fn new(node: NodeRef<'_>, plan: &Plan, site: exact_plan::NodesId) -> Self {
        let source = plan.node(site);
        let tag = &plan.place_steps[source.place.expect("authored place").0 as usize];
        let mut events: Vec<_> = source
            .handlers
            .iter()
            .map(|handler| plan.handler(handler).event as u8)
            .collect();
        events.sort_unstable();
        events.dedup();
        Self {
            id: node.id,
            shape: Shape {
                tag: plan.str(tag.name).into(),
                kind: node.node_type,
                events,
                literal_props: [
                    PropId::Type,
                    PropId::Hatch,
                    PropId::Markup,
                    PropId::ListStyle,
                ]
                .map(|prop| node.props.str(prop).map(str::to_owned)),
                appearance: matches!(
                    node.node_type,
                    NodeType::List | NodeType::TextInput | NodeType::Control | NodeType::Pressable
                )
                .then_some(node.style.appearance),
                virtualized: node.props.bool(PropId::Virtualized).unwrap_or(false),
            },
        }
    }
}

type Children = BTreeMap<Vec<Part<String>>, Option<View>>;

/// Views indexed by their physical parent and the source steps within it.
/// A place with more than one live claimant cannot identify either view.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RetainedViews(BTreeMap<Option<ViewId>, Children>);

impl Tree {
    pub(crate) fn retainable_views(
        &self,
        plan: &Plan,
        kernel: &Kernel,
        ids: &Ids,
    ) -> RetainedViews {
        let mut views = RetainedViews::default();
        let mut depths = BTreeMap::new();
        self.visit_places(plan, kernel, |instance, path, _| {
            let id = instance.view;
            let (Some(node), Some(site)) = (kernel.node(id), ids.site(id)) else {
                return;
            };
            let depth = match node.parent {
                None if node.is_root => 0,
                Some(parent) => match depths.get(&parent) {
                    Some(&depth) => depth,
                    None => return,
                },
                None => return,
            };
            let Some(local) = path.get(depth..) else {
                return;
            };
            depths.insert(id, path.len());
            let step = local.iter().map(Part::key).collect();
            views
                .0
                .entry(node.parent)
                .or_default()
                .entry(step)
                .and_modify(|ambiguous| *ambiguous = None)
                .or_insert_with(|| Some(View::new(node, plan, site)));
        });
        views
    }
}

impl RetainedViews {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn assign(&self, candidate: &Self, count: ViewId, ids: &mut Ids) {
        let mut kept = vec![None; count as usize];
        let mut parents = vec![(None, None)];
        while let Some((old, new)) = parents.pop() {
            let (Some(old), Some(new)) = (self.0.get(&old), candidate.0.get(&new)) else {
                continue;
            };
            for (step, view) in new {
                let (Some(view), Some(Some(before))) = (view, old.get(step)) else {
                    continue;
                };
                if before.shape == view.shape {
                    kept[view.id as usize - 1] = Some(before.id);
                    parents.push((Some(before.id), Some(view.id)));
                }
            }
        }
        ids.kept = kept.into_iter();
    }
}

impl Ids {
    pub(crate) fn finish_first_frame(&mut self) {
        debug_assert!(self.kept.as_slice().is_empty());
        self.kept = Default::default();
    }
}
