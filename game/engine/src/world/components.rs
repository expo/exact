use super::{Component, Entity, Target, World};
use crate::storage::{At, Storage};
use crate::Ref;
use std::panic::Location;

/// A component column resolved once for indexed reads. Holding this handle does
/// not lease any rows; each `get` leases only its returned row, just as `World::get`.
/// Structural changes require dropping the handle's shared world borrow first.
pub struct Components<'w, C> {
    world: &'w World,
    storage: Option<&'w Storage<C>>,
}
impl<'w, C: Component> Components<'w, C> {
    pub(super) fn new(world: &'w World) -> Self {
        Self {
            world,
            storage: world.storage::<C>(),
        }
    }
    /// Visit members in slot order without leasing their values.
    pub(crate) fn entities(&self) -> impl Iterator<Item = Entity> + '_ {
        self.world.sim_reads::<C>();
        self.storage
            .into_iter()
            .flat_map(|s| s.indices(None))
            .map(|i| self.world.entity_at(i))
    }
    /// Test membership without borrowing values, rejecting stale entity handles.
    pub fn has(&self, entity: Entity) -> bool {
        self.world.sim_reads::<C>();
        self.world.contains(entity) && self.storage.is_some_and(|s| s.has(entity.index() as usize))
    }
    /// Borrow a current row. Other rows stay free, and an exclusive lease on this
    /// row is refused with the component, entity and both callers in the diagnostic.
    #[track_caller]
    pub fn get(&self, target: impl Target) -> Option<Ref<'w, C>> {
        self.get_at(target, Location::caller())
    }
    pub(super) fn get_at(&self, target: impl Target, at: At) -> Option<Ref<'w, C>> {
        let w = self.world;
        w.sim_reads::<C>();
        let entity = target.entity(w)?;
        if !w.contains(entity) {
            return None;
        }
        let (storage, index) = (self.storage?, entity.index() as usize);
        storage
            .get(index, &w.leases, at)
            .unwrap_or_else(|_| w.refuse(storage.row_conflict(index, false, &w.leases, at)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Parent, Transform};
    use std::panic::{catch_unwind, AssertUnwindSafe};

    #[test]
    fn rows_remain_live_and_release_individually() {
        let mut w = World::new(60, 7);
        let a = w.spawn_named("a", Transform::at(1., 0., 0.));
        let b = w.spawn(Transform::at(2., 0., 0.));
        let empty = w.spawn(());
        let saved = w.save();
        let hash = w.hash();
        let rows = w.components::<Transform>();
        assert_eq!(rows.get("a"), w.get::<Transform>(a));
        assert!(rows.has(a));
        assert!(!rows.has(empty));
        assert!(rows.get(empty).is_none());
        assert!(w.components::<Parent>().get(a).is_none());
        assert!(!w.components::<Parent>().has(a));
        assert_eq!(w.save(), saved);
        assert_eq!(w.hash(), hash);
        let read = rows.get(a).unwrap();
        w.get_mut::<Transform>(b).unwrap().position.x = 9.;
        assert_eq!(rows.get(b).unwrap().position.x, 9.);
        assert!(catch_unwind(AssertUnwindSafe(|| w.get_mut::<Transform>(a))).is_err());
        drop(read);
        let write = w.get_mut::<Transform>(a).unwrap();
        assert!(rows.has(a));
        assert!(catch_unwind(AssertUnwindSafe(|| rows.get(a))).is_err());
        assert_eq!(rows.get(b).unwrap().position.x, 9.);
        drop(write);
        let mut query = w.query::<&mut Transform>();
        let _ = query.iter().next().unwrap();
        assert!(catch_unwind(AssertUnwindSafe(|| rows.get(b))).is_err());
        drop(query);
        assert!(rows.get(b).is_some());
    }

    #[test]
    fn presentation_reads_are_refused_inside_a_tick() {
        let mut w = World::new(60, 7);
        let e = w.spawn(());
        w.in_tick = true;
        let rows = w.components::<crate::Offset>();
        assert!(catch_unwind(AssertUnwindSafe(|| rows.get(e))).is_err());
        assert!(catch_unwind(AssertUnwindSafe(|| rows.has(e))).is_err());
    }

    #[test]
    fn recycling_restore_and_separate_worlds_resolve_current_columns() {
        let mut w = World::new(60, 7);
        let stale = w.spawn(Transform::at(1., 0., 0.));
        w.despawn(stale);
        let live = w.spawn(Transform::at(2., 0., 0.));
        assert_eq!(stale.index(), live.index());
        let saved = w.save();
        let check = |w: &World| {
            let rows = w.components::<Transform>();
            assert!(!rows.has(stale));
            assert!(rows.get(stale).is_none());
            assert_eq!(rows.get(live).unwrap().position.x, 2.);
        };
        check(&w);
        w.load(&saved).unwrap();
        check(&w);
        let mut other = World::new(60, 7);
        let e = other.spawn(Transform::at(3., 0., 0.));
        assert_eq!(
            other.components::<Transform>().get(e).unwrap().position.x,
            3.
        );
        check(&w);
    }
}
