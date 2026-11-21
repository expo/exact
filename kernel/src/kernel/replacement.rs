//! Tree operations for a plan replacement, once the runner has assigned view ids.
//! LLP 1029.000.002 D3.

use super::{Kernel, NodeRef, Op, StyleProps, ViewId};
use crate::id::IdSet;

#[cfg(test)]
mod tests;

impl Kernel {
    /// The operations that change the live nodes of `before` into this tree.
    /// Common ids keep their nodes when their type, parent and attached root
    /// permit it. The runner assigns fresh ids to views at new places.
    pub fn replacement_ops(&self, before: &Kernel) -> Vec<Op> {
        let kept = self.kept_nodes(before);
        let mut ops = Vec::new();
        // Destroying a parent also destroys its descendants.
        for slot in before.arena.iter_live() {
            let old = before.node_at(slot);
            if !kept.contains(&old.id) && old.parent.is_none_or(|id| kept.contains(&id)) {
                ops.push(Op::DestroyView { id: old.id });
            }
        }

        let mut children = Vec::new();
        for slot in self.arena.iter_live() {
            let new = self.node_at(slot);
            let id = new.id;
            let old = if kept.contains(&id) {
                before.node(id)
            } else {
                ops.push(Op::CreateView {
                    id,
                    node_type: new.node_type,
                });
                None
            };
            declarations(old, new, &mut ops);
            let old_children = old
                .into_iter()
                .flat_map(|node| node.child_nodes())
                .filter(|node| kept.contains(&node.id))
                .map(|node| node.id);
            if !old_children.eq(new.child_nodes().map(|node| node.id)) {
                children.push(Op::SetChildren {
                    id,
                    children: new.children(),
                });
            }
        }
        // Every new child must exist before a parent can name it.
        ops.extend(children);
        ops.extend(
            self.roots()
                .into_iter()
                .filter(|id| !kept.contains(id))
                .map(|id| Op::AttachRoot { id }),
        );
        ops
    }

    fn kept_nodes(&self, before: &Kernel) -> IdSet<ViewId> {
        // AttachRoot appends. An initial run of the new roots can keep its
        // place if its members already occur in that order in the old roots.
        let mut old_roots = before.roots().into_iter();
        let mut stack: Vec<_> = self
            .arena
            .roots()
            .iter()
            .copied()
            .take_while(|&s| {
                let node = self.node_at(s);
                before
                    .node(node.id)
                    .is_some_and(|old| old.node_type == node.node_type)
                    && old_roots.any(|id| id == node.id)
            })
            .collect();
        stack.extend(
            self.arena
                .iter_live()
                .filter(|&s| self.arena.parent(s).is_none() && !self.arena.is_root(s)),
        );
        let mut kept = IdSet::default();
        while let Some(slot) = stack.pop() {
            let new = self.node_at(slot);
            if before.node(new.id).is_some_and(|old| {
                old.node_type == new.node_type
                    && old.parent == new.parent
                    && old.is_root == new.is_root
            }) {
                kept.insert(new.id);
                stack.extend(self.arena.children(slot));
            }
        }
        kept
    }
}

fn declarations(old: Option<NodeRef<'_>>, new: NodeRef<'_>, ops: &mut Vec<Op>) {
    let id = new.id;
    if let Some(old) = old {
        for (prop, _) in old.props.iter() {
            if !new.props.contains(prop) {
                ops.push(Op::ClearProp { id, prop });
            }
        }
    }
    for (prop, value) in new.props.iter() {
        if old.and_then(|old| old.props.get(prop)) != Some(value) {
            ops.push(Op::SetProp {
                id,
                prop,
                value: value.clone(),
            });
        }
    }
    let declared = new.style.mask.union(new.style.written);
    let changed = if let Some(old) = old {
        let gone = old.style.mask.union(old.style.written).minus(declared);
        if !gone.is_empty() {
            ops.push(Op::ClearStyle {
                id,
                mask: gone,
                written: false,
            });
        }
        old.style.changed_mask(new.style)
    } else {
        declared
    };
    if !changed.is_empty() {
        let mut patch = StyleProps::default();
        patch.copy_rows(new.style, changed.intersect(new.style.mask));
        patch.written = changed.intersect(new.style.written);
        ops.push(Op::SetStyle {
            id,
            patch: Box::new(patch),
        });
    }
    if old.map_or(&[][..], |node| node.looks()) != new.looks() {
        ops.push(Op::SetLooks {
            id,
            looks: new.looks().to_vec(),
        });
    }
}
