//! Realized node children shared with a render checkpoint until updated.
use super::{Child, Frame, InstanceError, NodeInst, Rc, Update};
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, Default)]
pub(super) struct Children(Option<Rc<[Child]>>);

impl From<Vec<Child>> for Children {
    fn from(children: Vec<Child>) -> Self {
        Self((!children.is_empty()).then(|| Rc::from(children)))
    }
}

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
            let mut children = Vec::new();
            let collection = (lists.create)(u, self.node, self.view, frames, &mut children)?;
            self.children = children.into();
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
