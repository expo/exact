use super::*;
use std::collections::HashMap;

// Typed reads skip names; ordered walks retain save, hash and diagnostic order.
// LLP 1172.022.004 F3.
#[derive(Default)]
pub(super) struct Storages {
    names: BTreeMap<&'static str, TypeId>,
    types: HashMap<TypeId, Box<dyn Erased>>,
}

impl Storages {
    pub(super) fn typed<T: Data>(&self) -> Option<&dyn Erased> {
        self.types.get(&TypeId::of::<T>()).map(Box::as_ref)
    }

    pub(super) fn typed_mut<T: Data>(&mut self) -> Option<&mut Box<dyn Erased>> {
        self.types.get_mut(&TypeId::of::<T>())
    }

    pub(super) fn get(&self, name: &str) -> Option<&dyn Erased> {
        self.types.get(self.names.get(name)?).map(Box::as_ref)
    }

    pub(super) fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn Erased>> {
        self.types.get_mut(self.names.get(name)?)
    }

    pub(super) fn contains_key(&self, name: &str) -> bool {
        self.names.contains_key(name)
    }

    pub(super) fn insert(
        &mut self,
        name: &'static str,
        id: TypeId,
        storage: Box<dyn Erased>,
    ) -> Option<Box<dyn Erased>> {
        self.names.insert(name, id);
        self.types.insert(id, storage)
    }

    pub(super) fn get_or_insert<T: Data>(
        &mut self,
        name: &'static str,
        make: impl FnOnce() -> Box<dyn Erased>,
    ) -> &mut Box<dyn Erased> {
        let id = TypeId::of::<T>();
        self.types.entry(id).or_insert_with(|| {
            let storage = make();
            self.names.insert(name, id);
            storage
        })
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&&'static str, &dyn Erased)> {
        self.names
            .iter()
            .map(|(name, id)| (name, self.types[id].as_ref()))
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &dyn Erased> {
        self.iter().map(|(_, storage)| storage)
    }

    pub(super) fn remove_entity(&mut self, index: usize) {
        for id in self.names.values() {
            self.types.get_mut(id).unwrap().remove(index);
        }
    }
}
