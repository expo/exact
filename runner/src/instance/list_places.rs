//! Carry a list by its authored place, including the keys of enclosing rows.
//! Within one settled candidate its plan node and enclosing row indices name
//! that instance before its first window or kernel node exists.
use super::{collection::ListReading, place::Part, Frame, Tree};
use exact_kernel::Kernel;
use exact_plan::{NodesId, Plan};
use std::collections::{btree_map::Entry, BTreeMap};
use std::rc::Rc;

/// The bounded reading window at each virtualized list's authored place.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RetainedLists(BTreeMap<Vec<Part<String>>, Option<Rc<ListReading>>>);

impl RetainedLists {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Tree {
    pub(crate) fn carried_lists(&self, plan: &Plan, kernel: &Kernel) -> RetainedLists {
        let mut lists = RetainedLists::default();
        if self.has_collections {
            self.visit_places(plan, kernel, |node, path, _| {
                if let Some(list) = node.list() {
                    lists
                        .0
                        .entry(path.iter().map(Part::key).collect())
                        .and_modify(|entry| *entry = None)
                        .or_insert_with(|| list.reading().map(Rc::new));
                }
            });
        }
        lists
    }
}

type Instance = (u32, Vec<usize>);
#[derive(Debug, Default)]
pub(crate) struct CandidateLists(BTreeMap<Instance, Rc<ListReading>>);

impl CandidateLists {
    pub(super) fn get(&self, node: NodesId, frames: &[Frame]) -> Option<&ListReading> {
        if self.0.is_empty() {
            return None;
        }
        let rows = frames.iter().filter_map(|frame| frame.index).collect();
        self.0.get(&(node.0, rows)).map(Rc::as_ref)
    }

    pub(crate) fn match_places(
        &mut self,
        before: &RetainedLists,
        tree: &Tree,
        plan: &Plan,
        kernel: &Kernel,
    ) -> bool {
        if before.is_empty() || !tree.has_collections {
            return false;
        }
        let mut found = BTreeMap::new();
        tree.visit_places(plan, kernel, |node, path, rows| {
            if node.list().is_some() {
                let key: Vec<_> = path.iter().map(Part::key).collect();
                if let Some(Some(reading)) = before.0.get(&key) {
                    found
                        .entry(key)
                        .and_modify(|entry| *entry = None)
                        .or_insert_with(|| Some(((node.node.0, rows.to_vec()), reading)));
                }
            }
        });
        let mut changed = false;
        for (instance, reading) in found.into_values().flatten() {
            if let Entry::Vacant(entry) = self.0.entry(instance) {
                entry.insert(reading.clone());
                changed = true;
            }
        }
        changed
    }
}
