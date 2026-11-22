//! Source places at the current row keys and element ids (LLP 1029.000.002 D1).
use super::{Active, Child, NodeInst, Row, Tree};
use exact_kernel::{Kernel, PropId};
use exact_plan::{PlaceKind, PlaceStepsId, Plan, RegionsId, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Part<K = Value> {
    Component(String, u32),
    Element(String, u32),
    Id(String),
    Children(u32),
    Each(u32, String),
    Row(K, u32),
}

impl Part {
    pub(super) fn key(&self) -> Part<String> {
        match self {
            Self::Component(name, i) => Part::Component(name.clone(), *i),
            Self::Element(name, i) => Part::Element(name.clone(), *i),
            Self::Id(id) => Part::Id(id.clone()),
            Self::Children(i) => Part::Children(*i),
            Self::Each(i, key) => Part::Each(*i, key.clone()),
            Self::Row(value, i) => {
                Part::Row(super::key_text(value).expect("validated row key"), *i)
            }
        }
    }
}

enum Peer<'a> {
    Element(&'a NodeInst, PlaceStepsId),
    Each(RegionsId, &'a [Row]),
}

// Both arms' ordinal positions came from the source. Only the active arm has
// live ids. A compiler-generated wrapper adds no step to an authored place.
fn peers<'a>(plan: &Plan, children: &'a [Child], visit: &mut impl FnMut(Peer<'a>)) {
    for child in children {
        match child {
            Child::Node(node) => match plan.node(node.node).place {
                Some(step) => visit(Peer::Element(node, step)),
                None => peers(plan, &node.children, visit),
            },
            Child::Region(region) => match &region.active {
                Active::Arm { roots, .. } => peers(plan, roots, visit),
                Active::Rows { rows } => visit(Peer::Each(region.region, rows)),
            },
        }
    }
}

struct Walk<'a, F> {
    plan: &'a Plan,
    kernel: &'a Kernel,
    path: Vec<Part>,
    rows: Vec<usize>,
    visit: F,
}

impl<F: FnMut(&NodeInst, &[Part], &[usize])> Walk<'_, F> {
    fn append(&mut self, step: PlaceStepsId) {
        let start = self.path.len();
        let mut next = Some(step);
        while let Some(step) = next {
            let row = &self.plan.place_steps[step.0 as usize];
            let ordinal = row.ordinal;
            self.path.push(match row.kind {
                PlaceKind::Component => Part::Component(self.plan.str(row.name).into(), ordinal),
                PlaceKind::Element => Part::Element(self.plan.str(row.name).into(), ordinal),
                PlaceKind::Children => Part::Children(ordinal),
                PlaceKind::Each => Part::Each(ordinal, self.plan.str(row.name).into()),
            });
            next = row.outer;
        }
        self.path[start..].reverse();
    }

    fn siblings(&mut self, children: &[Child]) {
        let plan = self.plan;
        let kernel = self.kernel;
        let id = |node: &NodeInst| kernel.node(node.view)?.props.str(PropId::Id);
        let mut counts = BTreeMap::new();
        peers(plan, children, &mut |peer| {
            if let Peer::Element(node, step) = peer {
                if let Some(id) = id(node) {
                    let outer = plan.place_steps[step.0 as usize].outer;
                    *counts.entry((outer, id)).or_insert(0usize) += 1;
                }
            }
        });
        peers(plan, children, &mut |peer| {
            let parent = self.path.len();
            match peer {
                Peer::Element(node, step) => {
                    let source = &plan.place_steps[step.0 as usize];
                    let outer = source.outer;
                    if let Some(id) = id(node)
                        .filter(|id| !source.shared_id && counts.get(&(outer, *id)) == Some(&1))
                    {
                        if let Some(outer) = outer {
                            self.append(outer);
                        }
                        self.path.push(Part::Id(id.into()));
                    } else {
                        self.append(step);
                    }
                    (self.visit)(node, &self.path, &self.rows);
                    self.siblings(&node.children);
                    if let Some(list) = node.list() {
                        list.visit_rows(&mut |region, row| self.row(region, row));
                    }
                }
                Peer::Each(region, rows) => {
                    for row in rows {
                        self.row(region, row);
                    }
                }
            }
            self.path.truncate(parent);
        });
    }

    fn row(&mut self, region: RegionsId, row: &Row) {
        if let Some(step) = self.plan.region(region).place {
            let parent = self.path.len();
            self.append(step);
            self.path.push(Part::Row(row.key.clone(), row.dup));
            self.rows.push(row.frame.index.expect("row position"));
            self.siblings(&row.roots);
            self.rows.pop();
            self.path.truncate(parent);
        }
    }
}

impl Tree {
    pub(crate) fn visit_places(
        &self,
        plan: &Plan,
        kernel: &Kernel,
        visit: impl FnMut(&NodeInst, &[Part], &[usize]),
    ) {
        Walk {
            plan,
            kernel,
            path: Vec::new(),
            rows: Vec::new(),
            visit,
        }
        .siblings(&self.children);
    }
}
