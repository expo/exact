//! Realized node children shared with a render checkpoint until updated.
use super::{realize_child, Child, Frame, InstanceError, NodeInst, Rc, Update};
use exact_plan::{ArmsId, NodesId};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Default)]
pub(super) struct Children(Option<Rc<[Child]>>);

impl Deref for Children {
    type Target = [Child];

    fn deref(&self) -> &[Child] {
        self.0.as_deref().unwrap_or(&[])
    }
}

impl DerefMut for Children {
    fn deref_mut(&mut self) -> &mut [Child] {
        match &mut self.0 {
            Some(children) => Rc::make_mut(children),
            None => &mut [],
        }
    }
}

impl Children {
    pub(super) fn one(child: Child) -> Self {
        Self(Some(Rc::new([child])))
    }

    pub(super) fn realize(
        u: &mut Update<'_>,
        node: NodesId,
        arm: Option<ArmsId>,
        frames: &[Frame],
    ) -> Result<Self, InstanceError> {
        let mut sites = u.sites.children(Some(node), arm).iter();
        Self::realize_n(u, sites.len(), |u| {
            let &(_, site) = sites.next().expect("child site");
            realize_child(u, site, frames)
        })
    }

    pub(super) fn realize_n(
        u: &mut Update<'_>,
        count: usize,
        mut make: impl FnMut(&mut Update<'_>) -> Result<Child, InstanceError>,
    ) -> Result<Self, InstanceError> {
        match count {
            0 => Ok(Self::default()),
            1 => Ok(Self::one(make(u)?)),
            2 => {
                let first = make(u)?;
                let second = make(u)?;
                Ok(Self(Some(Rc::new([first, second]))))
            }
            _ => {
                let base = u.child_buffer.len();
                u.child_buffer.reserve(count);
                for _ in 0..count {
                    match make(u) {
                        Ok(child) => u.child_buffer.push(child),
                        Err(error) => {
                            u.child_buffer.truncate(base);
                            return Err(error);
                        }
                    }
                }
                // Drain has an exact length, so Rc allocates its slice directly.
                Ok(Self(Some(u.child_buffer.drain(base..).collect())))
            }
        }
    }

    pub(super) fn update(
        &mut self,
        u: &mut Update<'_>,
        frames: &[Frame],
    ) -> Result<bool, InstanceError> {
        // Preserve the checkpoint's backing until a child will actually
        // update. Count skipped regions as the mutable walk would.
        let start = if self.0.as_ref().is_some_and(|c| Rc::strong_count(c) > 1) {
            let Some(first) = self.iter().position(|c| needs_update(c, u)) else {
                return Ok(false);
            };
            first
        } else {
            0
        };
        update_all(u, &mut self[start..], frames)
    }
}

impl NodeInst {
    pub(super) fn create_list(
        &mut self,
        u: &mut Update<'_>,
        frames: &[Frame],
    ) -> Result<(), InstanceError> {
        if let Some(lists) = u.env.lists {
            debug_assert!(self.children.is_empty());
            let mut children = Children::default();
            let collection = (lists.create)(u, self.node, self.view, frames, &mut children)?;
            self.children = children;
            self.set_list(collection);
        }
        Ok(())
    }
}

fn needs_update(child: &Child, u: &mut Update<'_>) -> bool {
    let deps = &u.sites.data.deps;
    match child {
        Child::Node(n) => u.stale(&deps.nodes[n.node.0 as usize]),
        Child::Region(r) => {
            let stale = u.stale(&deps.regions[r.region.0 as usize]);
            if !stale {
                u.work.regions_skipped += 1;
            }
            stale
        }
    }
}

/// Whether the children whose reads changed now contribute a different list of roots.
pub(super) fn update_all(
    u: &mut Update<'_>,
    children: &mut [Child],
    frames: &[Frame],
) -> Result<bool, InstanceError> {
    let mut roots = false;
    for child in children {
        if !needs_update(child, u) {
            continue;
        }
        match child {
            Child::Node(n) => n.update(u, frames)?,
            Child::Region(r) => roots |= r.update(u, frames, false)?,
        }
    }
    Ok(roots)
}
