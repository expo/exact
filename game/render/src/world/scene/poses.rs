use super::decompose;
use exact_game::{Components, Entity, Globals, Offset, Parent, Transform, Visible, World};

#[cfg(test)]
thread_local! {
    static RESOLUTIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
pub(crate) fn resolution_count() -> usize {
    RESOLUTIONS.with(|n| n.get())
}

// One feed resolves columns once; each row read still observes live leases.
// LLP 1172.022.004 F3.
pub(crate) struct PoseRows<'w> {
    world: &'w World,
    pub(super) parents: Components<'w, Parent>,
    pub(super) transforms: Components<'w, Transform>,
    pub(super) offsets: Components<'w, Offset>,
    visible: Components<'w, Visible>,
    globals: Globals<'w>,
}
impl<'w> PoseRows<'w> {
    pub(crate) fn new(world: &'w World) -> Self {
        #[cfg(test)]
        RESOLUTIONS.with(|n| n.set(n.get() + 1));
        Self {
            world,
            parents: world.components(),
            transforms: world.components(),
            offsets: world.components(),
            visible: world.components(),
            globals: world.globals(),
        }
    }
    pub(crate) fn global(&self, entity: Entity) -> Option<glam::Affine3A> {
        self.globals.get(entity)
    }
    pub(crate) fn has_parent(&self, entity: Entity) -> bool {
        self.parents.has(entity)
    }
    pub(crate) fn is_visible(&self, mut entity: Entity) -> bool {
        for _ in 0..self.world.len() {
            if !self.world.contains(entity) || self.visible.get(entity).is_some_and(|v| !v.0) {
                return false;
            }
            match self.parents.get(entity) {
                Some(parent) => entity = parent.0,
                None => return true,
            }
        }
        false
    }
    /// The displayed global: drawn(parent) · local · offset. Presentation offsets
    /// (exact_game::Offset) move an entity and everything under it; without one on
    /// the chain this is the simulated global, exactly.
    pub(crate) fn drawn(&self, e: Entity) -> Option<glam::Affine3A> {
        let affine = |t: Transform| {
            glam::Affine3A::from_scale_rotation_translation(t.scale, t.rotation, t.position)
        };
        let offset = |e: Entity| self.offsets.get(e).map(|o| affine(o.0));
        // Most chains carry no offset: no allocation for them.
        let mut at = e;
        let mut any = false;
        for _ in 0..=self.world.len() {
            any |= self.offsets.has(at);
            match self.parents.get(at) {
                Some(p) if !any => at = p.0,
                _ => break,
            }
        }
        if !any {
            return self.globals.get(e);
        }
        // The parent chain, bounded even if tools edit a cycle.
        let mut chain = vec![e];
        while let Some(p) = self.parents.get(*chain.last().unwrap()) {
            if chain.len() > self.world.len() {
                break;
            }
            chain.push(p.0);
        }
        // The offset nearest the root: above it the simulated global holds.
        let Some(first) = (0..chain.len()).rev().find(|&i| offset(chain[i]).is_some()) else {
            return self.globals.get(e);
        };
        let mut global = self.globals.get(chain[first])? * offset(chain[first]).unwrap();
        for &below in chain[..first].iter().rev() {
            let local = affine(*self.transforms.get(below)?);
            global = global * local * offset(below).unwrap_or(glam::Affine3A::IDENTITY);
        }
        Some(global)
    }
    pub(crate) fn pose(&self, e: Entity) -> Option<Transform> {
        self.drawn(e).map(decompose)
    }
    // Roots use their authored quaternion and signed scale in the opaque buffers.
    // Decomposing even an unparented affine can move a reflection into the rotation.
    pub(crate) fn primitive_endpoint(
        &self,
        e: Entity,
        global: impl FnOnce() -> Option<glam::Affine3A>,
    ) -> Option<Transform> {
        if !self.parents.has(e) && !self.offsets.has(e) {
            self.transforms.get(e).map(|t| *t)
        } else {
            global().map(decompose)
        }
    }
    pub(crate) fn primitive_pose(&self, e: Entity) -> Option<Transform> {
        self.primitive_endpoint(e, || self.drawn(e))
    }
    // Resolve an explicit start through the same local/offset chain as current poses.
    // No override means the caller keeps its ordinary retained history.
    pub(crate) fn interpolation_origin(&self, e: Entity) -> Option<Transform> {
        if self.world.fresh().is_empty() {
            return None;
        }
        if !self.parents.has(e) && !self.offsets.has(e) {
            return self.world.interpolation_origin(e);
        }
        let mut chain = vec![e];
        while let Some(p) = self.parents.get(*chain.last()?) {
            if chain.len() > self.world.len() {
                return None;
            }
            chain.push(p.0);
        }
        let first = (0..chain.len())
            .rev()
            .find(|&i| self.world.interpolation_origin(chain[i]).is_some())?;
        let mut global = match chain.get(first + 1) {
            Some(&parent) => self.drawn(parent)?,
            None => glam::Affine3A::IDENTITY,
        };
        for &entity in chain[..=first].iter().rev() {
            let local = self
                .world
                .interpolation_origin(entity)
                .or_else(|| self.transforms.get(entity).map(|t| *t))?;
            let offset = self
                .offsets
                .get(entity)
                .map_or(Transform::default(), |o| o.0);
            for t in [local, offset] {
                global *= glam::Affine3A::from_scale_rotation_translation(
                    t.scale, t.rotation, t.position,
                );
            }
        }
        Some(decompose(global))
    }
    /// A changed parent edge resets only its own subtree's interpolation.
    pub(crate) fn snap_in(&self, e: Entity, chains: Option<&super::ParentChanges>) -> bool {
        match chains {
            Some(chains) => chains.get(self.world, e),
            None => self.snap(e, false),
        }
    }
    // Bounded even if tools edit a cycle before propagation.
    pub(crate) fn snap(&self, mut e: Entity, parent_changed: bool) -> bool {
        if parent_changed {
            return true;
        }
        // Without a fresh pose, no ancestor can require an interpolation reset.
        if self.world.fresh().is_empty() {
            return false;
        }
        for _ in 0..=self.world.len() {
            if self.world.is_fresh(e) {
                return true;
            }
            let Some(parent) = self.parents.get(e) else {
                return false;
            };
            e = parent.0;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_game::{Quat, Vec3};

    #[test]
    fn visibility_and_pose_reads_follow_current_rows_and_keep_signed_roots() {
        let mut w = World::new(60, 7);
        let root_pose = Transform {
            position: Vec3::new(1., 2., 3.),
            rotation: Quat::from_rotation_y(0.75),
            scale: Vec3::new(-2., 3., 4.),
        };
        let root = w.spawn(root_pose);
        let child = w.spawn((Transform::at(4., 0., 0.), Parent(root)));
        let empty = w.spawn(());
        let stale = w.spawn(());
        w.despawn(stale);
        let live = w.spawn(());
        w.propagate();
        let saved = w.save();
        let rows = PoseRows::new(&w);
        let pose = rows.primitive_pose(root).unwrap();
        assert_eq!(pose.rotation, root_pose.rotation);
        assert_eq!(pose.scale, root_pose.scale);
        for e in [root, child, empty, stale, live] {
            assert_eq!(rows.is_visible(e), w.is_visible(e));
            assert_eq!(rows.global(e), w.global(e));
        }
        assert_eq!(w.save(), saved);
        drop(rows);
        w.insert(root, Visible(false));
        w.insert(root, Offset(Transform::at(0., 5., 0.)));
        w.propagate();
        let rows = PoseRows::new(&w);
        assert!(!rows.is_visible(child));
        w.get_mut::<Visible>(root).unwrap().0 = true;
        assert!(rows.is_visible(child));
        let expected = w.drawn(child).unwrap().pose;
        assert_eq!(rows.drawn(child), Some(expected));
        w.get_mut::<Offset>(root).unwrap().0.position.y = 7.;
        assert_eq!(rows.drawn(child), Some(w.drawn(child).unwrap().pose));
        w.get_mut::<Parent>(child).unwrap().0 = stale;
        assert!(!rows.is_visible(child));
        w.get_mut::<Parent>(child).unwrap().0 = child;
        assert!(!rows.is_visible(child));
    }
}
