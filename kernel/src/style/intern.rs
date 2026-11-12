//! Weak indexes: only returned values own entries; their last drop removes both keys.
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex, Weak};

pub(super) struct Table<K: Clone + Eq + Hash + 'static, V: 'static> {
    keys: HashMap<K, Weak<Value<K, V>>>,
    ids: HashMap<usize, Weak<Value<K, V>>>,
    next: usize,
}

impl<K: Clone + Eq + Hash, V> Table<K, V> {
    pub(super) fn new() -> Self {
        Self {
            keys: HashMap::new(),
            ids: HashMap::new(),
            next: 0,
        }
    }

    pub(super) fn intern(table: &'static Mutex<Self>, key: K, value: V) -> Arc<Value<K, V>> {
        let mut t = table.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = t.keys.get(&key).and_then(Weak::upgrade) {
            return value;
        }
        // IDs are never reused: a released raw handle cannot alias a new value.
        t.next = t.next.checked_add(1).expect("calculation handle space");
        assert!(t.next <= usize::MAX >> 3, "calculation handle space");
        let value = Arc::new(Value {
            id: t.next,
            key: key.clone(),
            value,
            table,
        });
        t.keys.insert(key, Arc::downgrade(&value));
        t.ids.insert(value.id, Arc::downgrade(&value));
        value
    }

    pub(super) fn get(&self, id: usize) -> Option<Arc<Value<K, V>>> {
        self.ids.get(&id).and_then(Weak::upgrade)
    }

    #[cfg(test)]
    pub(super) fn contains(&self, key: &K) -> bool {
        self.keys.contains_key(key)
    }
}

pub(super) struct Value<K: Clone + Eq + Hash + 'static, V: 'static> {
    pub(super) id: usize,
    pub(super) key: K,
    pub(super) value: V,
    table: &'static Mutex<Table<K, V>>,
}

impl<K: Clone + Eq + Hash, V> Drop for Value<K, V> {
    fn drop(&mut self) {
        let mut t = self.table.lock().unwrap_or_else(|e| e.into_inner());
        // Another thread may have interned the key after our last owner dropped.
        if t.keys
            .get(&self.key)
            .is_some_and(|v| std::ptr::eq(v.as_ptr(), self))
        {
            t.keys.remove(&self.key);
        }
        t.ids.remove(&self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hasher;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::LazyLock;

    static COMPARISONS: AtomicUsize = AtomicUsize::new(0);
    #[derive(Clone, Eq)]
    struct Counted(u32);
    impl PartialEq for Counted {
        fn eq(&self, other: &Self) -> bool {
            COMPARISONS.fetch_add(1, Ordering::Relaxed);
            self.0 == other.0
        }
    }
    impl Hash for Counted {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.0.hash(state);
        }
    }

    #[test]
    fn interning_uses_hash_indexes_and_releases_both_keys() {
        static TABLE: LazyLock<Mutex<Table<Counted, ()>>> =
            LazyLock::new(|| Mutex::new(Table::new()));
        let mut values = Vec::new();
        for n in 0..4096 {
            values.push(Table::intern(&TABLE, Counted(n), ()));
        }
        for n in [4095, 0, 2000] {
            COMPARISONS.store(0, Ordering::Relaxed);
            let found = Table::intern(&TABLE, Counted(n), ());
            assert_eq!(found.id, values[n as usize].id);
            assert!(COMPARISONS.load(Ordering::Relaxed) < 64);
        }
        drop(values);
        let table = TABLE.lock().unwrap();
        assert!(table.keys.is_empty());
        assert!(table.ids.is_empty());
    }

    #[test]
    fn concurrent_last_drops_do_not_remove_a_new_owner_of_the_same_key() {
        static TABLE: LazyLock<Mutex<Table<u32, ()>>> = LazyLock::new(|| Mutex::new(Table::new()));
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..2000 {
                        let value = Table::intern(&TABLE, 1, ());
                        let copy = Table::intern(&TABLE, 1, ());
                        assert!(Arc::ptr_eq(&value, &copy));
                        assert!(TABLE.lock().unwrap().get(value.id).is_some());
                    }
                });
            }
        });
        assert!(TABLE.lock().unwrap().keys.is_empty());
        assert!(TABLE.lock().unwrap().ids.is_empty());
    }
}
