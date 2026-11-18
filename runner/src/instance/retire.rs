//! The perf counter's walk over a destroyed subtree (@ref LLP 1142 §13.4):
//! each retired row is counted against its list's site.
use super::*;

pub(in crate::instance) fn retire_node(u: &mut Update<'_>, node: &NodeInst) {
    if u.ids.work.sites.is_empty() {
        return;
    }
    if let Some(c) = node.collection() {
        c.retire_rows(u);
    }
    retire_children(u, &node.children);
}

pub(in crate::instance) fn retire_children(u: &mut Update<'_>, children: &[Child]) {
    for child in children {
        match child {
            Child::Node(n) => retire_node(u, n),
            Child::Region(r) => match &r.active {
                Active::Arm { roots, .. } => retire_children(u, roots),
                Active::Rows { rows } => {
                    for row in rows {
                        retire_children(u, &row.roots);
                    }
                }
            },
        }
    }
}
