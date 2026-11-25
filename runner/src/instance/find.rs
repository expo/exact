//! Locating a view's instance: its plan site and the lexical frames in force
//! there. The kernel's parent chain names every view between the root and
//! the target, so the search descends only along that path, comparing root
//! views among the siblings it meets — never walking the rest of the tree.

use super::*;

/// Whether `children` contribute `view` among their roots, directly or
/// through a nested region's arm or rows.
fn has_root(children: &[Child], view: ViewId) -> bool {
    children.iter().any(|c| match c {
        Child::Node(n) => n.view == view,
        Child::Region(r) => match &r.active {
            Active::Arm { roots, .. } => has_root(roots, view),
            Active::Rows { rows } => rows.iter().any(|row| has_root(&row.roots, view)),
        },
    })
}

/// The root among `children` whose view is `target`, pushing the frames
/// of the arms and rows crossed to reach it; `scanned` counts rows compared.
pub(super) fn locate<'a>(
    children: &'a [Child],
    target: ViewId,
    frames: &mut Vec<Frame>,
    scanned: &mut usize,
) -> Option<&'a NodeInst> {
    if let Some(n) = children.iter().find_map(|c| match c {
        Child::Node(n) if n.view == target => Some(n),
        _ => None,
    }) {
        return Some(n);
    }
    for c in children {
        let Child::Region(r) = c else { continue };
        match &r.active {
            Active::Arm { roots, frame, .. } => {
                if has_root(roots, target) {
                    frames.push(frame.scope());
                    return locate(roots, target, frames, scanned);
                }
            }
            Active::Rows { rows } => {
                if let Some(row) = rows.iter().find(|row| {
                    *scanned += 1;
                    has_root(&row.roots, target)
                }) {
                    frames.push(row.frame.clone());
                    return locate(&row.roots, target, frames, scanned);
                }
            }
        }
    }
    None
}

impl NodeInst {
    /// Whether `view` is this node's or a descendant's (a pin in a row).
    pub(super) fn contains(&self, view: ViewId) -> bool {
        self.view == view
            || self.list().is_some_and(|c| c.contains(view))
            || contains(&self.children, view)
    }
}

/// Whether `view` belongs to any instance under `children`.
pub(super) fn contains(children: &[Child], view: ViewId) -> bool {
    children.iter().any(|c| match c {
        Child::Node(n) => n.contains(view),
        Child::Region(r) => match &r.active {
            Active::Arm { roots, .. } => contains(roots, view),
            Active::Rows { rows } => rows.iter().any(|row| contains(&row.roots, view)),
        },
    })
}

impl Tree {
    /// The site owning `view` and the frames in force there, following
    /// `parent` (the kernel's) from the root down; `scanned` counts the
    /// rows compared on the way.
    pub fn find(
        &self,
        view: ViewId,
        parent: impl Fn(ViewId) -> Option<ViewId>,
        scanned: &mut usize,
    ) -> Option<(NodesId, Vec<Frame>)> {
        let mut frames = Vec::new();
        let node = self.find_instance(view, parent, &mut frames, scanned)?;
        Some((node.node, frames))
    }

    fn find_instance(
        &self,
        view: ViewId,
        parent: impl Fn(ViewId) -> Option<ViewId>,
        frames: &mut Vec<Frame>,
        scanned: &mut usize,
    ) -> Option<&NodeInst> {
        let mut chain = vec![view];
        // A moved view's path runs through its declaration (LLP 1013.002 D1).
        let up = |at| self.declared_parent(at).or_else(|| parent(at));
        while let Some(next) = up(*chain.last().expect("nonempty")) {
            chain.push(next);
        }
        let mut targets = chain.into_iter().rev().peekable();
        let mut here: &[Child] = &self.children;
        while let Some(target) = targets.next() {
            let n = locate(here, target, frames, scanned)?;
            if target == view {
                return Some(n);
            }
            match n.list() {
                Some(list)
                    if targets
                        .peek()
                        .is_some_and(|next| list.row_by_wrapper(*next).is_some()) =>
                {
                    let (frame, roots) = list.row_by_wrapper(targets.next()?)?;
                    frames.push(frame.clone());
                    here = roots;
                }
                _ => here = &n.children,
            }
        }
        None
    }
}

impl Tree {
    /// Only regions inside the card identify its rows. An outer region may
    /// repeat the whole section while the card itself holds static rows.
    pub(crate) fn keyed_children(
        &self,
        card: ViewId,
        parent: impl Fn(ViewId) -> Option<ViewId>,
    ) -> Vec<(ViewId, &str)> {
        fn visit<'a>(
            children: &'a [Child],
            key: Option<&'a str>,
            out: &mut Vec<(ViewId, &'a str)>,
        ) {
            for child in children {
                match child {
                    Child::Node(node) => {
                        if let Some(key) = key {
                            out.push((node.view, key));
                        }
                    }
                    Child::Region(region) => match &region.active {
                        Active::Arm { roots, .. } => visit(roots, key, out),
                        Active::Rows { rows } => {
                            for row in rows {
                                visit(&row.roots, row.key.as_str(), out);
                            }
                        }
                    },
                }
            }
        }
        let mut out = Vec::new();
        if let Some(node) = self.find_instance(card, parent, &mut Vec::new(), &mut 0) {
            visit(&node.children, None, &mut out);
        }
        out
    }
}
