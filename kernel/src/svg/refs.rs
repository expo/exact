//! SVG references by id (LLP 1055.000 D3).

use crate::id::ViewId;
use crate::kernel::Kernel;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;

impl Kernel {
    /// The node an SVG reference `#id` from `from` names (LLP 1055.000 D3):
    /// the one live node with that `id`; with several, the one sharing the
    /// deepest common ancestor with `from`, ties in tree order, so a
    /// component's instance finds its own definitions.
    pub fn resolve_id(&self, from: ViewId, id: &str) -> Option<ViewId> {
        let arena = self.arena();
        self.selectors
            .references
            .resolve(
                id,
                arena.slot_of(from),
                || {
                    self.selectors
                        .lookup_id(id)
                        .iter()
                        .copied()
                        .filter(|s| arena.is_live(*s))
                },
                |slot| arena.parent(slot),
                |slot| arena.children(slot).iter().copied(),
            )
            .map(|slot| arena.local_id(slot))
    }
}

/// [`Kernel::resolve_id`]'s rule over any tree, including a kernel-free
/// document (LLP 1048.004). An entry holds the first candidate in tree order
/// at or below each ancestor. A reference walks up to the first indexed ancestor,
/// its deepest common ancestor with any candidate, in O(depth) time.
/// With no shared ancestor it uses the first candidate globally. Tree order
/// compares child positions from the root, then keys; roots have no position.
///
/// Call [`Self::clear`] when candidates, parentage or sibling order change.
#[derive(Debug, Default, Clone)]
pub struct IdIndex<K> {
    entries: RefCell<HashMap<String, Nearest<K>>>,
}

#[derive(Debug, Clone)]
struct Nearest<K> {
    first: Option<K>,
    multiple: bool,
    ancestors: HashMap<K, K>,
}

impl<K: Copy + Ord + Hash> IdIndex<K> {
    /// Forget entries before changing their tree or ids.
    pub fn clear(&mut self) {
        self.entries.get_mut().clear();
    }

    /// Resolve an id. Nonempty candidate sets and their tree positions are
    /// cached until invalidation; sibling lists are read once per parent
    /// while building an entry, even when many candidates share it.
    pub fn resolve<C, I>(
        &self,
        id: &str,
        from: Option<K>,
        candidates: impl FnOnce() -> C,
        parent: impl Fn(K) -> Option<K>,
        children: impl Fn(K) -> I,
    ) -> Option<K>
    where
        C: IntoIterator<Item = K>,
        I: IntoIterator<Item = K>,
    {
        let mut entries = self.entries.borrow_mut();
        if !entries.contains_key(id) {
            let candidates: Vec<_> = candidates().into_iter().collect();
            if candidates.is_empty() {
                return None;
            }
            let mut entry = Nearest {
                first: candidates.first().copied(),
                multiple: candidates.len() > 1,
                ancestors: HashMap::new(),
            };
            if entry.multiple {
                let mut positions: HashMap<K, HashMap<K, usize>> = HashMap::new();
                let mut ranked = Vec::with_capacity(candidates.len());
                for node in candidates {
                    let mut chain = vec![node];
                    let mut order = Vec::new();
                    let mut child = node;
                    while let Some(p) = parent(child) {
                        let siblings = positions.entry(p).or_insert_with(|| {
                            children(p)
                                .into_iter()
                                .enumerate()
                                .map(|(i, c)| (c, i))
                                .collect()
                        });
                        order.push(*siblings.get(&child).unwrap_or(&usize::MAX));
                        chain.push(p);
                        child = p;
                    }
                    order.reverse();
                    ranked.push((order, node, chain));
                }
                ranked.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
                entry.first = ranked.first().map(|(_, node, _)| *node);
                for (_, node, chain) in ranked {
                    for ancestor in chain {
                        entry.ancestors.entry(ancestor).or_insert(node);
                    }
                }
            }
            entries.insert(id.to_owned(), entry);
        }
        let entry = &entries[id];
        if !entry.multiple {
            return entry.first;
        }
        let mut node = from?;
        loop {
            if let Some(candidate) = entry.ancestors.get(&node) {
                return Some(*candidate);
            }
            match parent(node) {
                Some(p) => node = p,
                None => return entry.first,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn successive_missing_ids_do_not_grow_the_index() {
        let index = IdIndex::<u32>::default();
        assert_eq!(
            index.resolve("fade", Some(2), || [1], |_| None, |_| []),
            Some(1)
        );
        for i in 0..512 {
            assert_eq!(
                index.resolve(&format!("missing-{i}"), Some(2), || [], |_| None, |_| []),
                None
            );
            assert_eq!(index.entries.borrow().len(), 1);
        }
    }

    #[test]
    fn repeated_references_walk_candidates_once() {
        for n in [128, 256] {
            // Root 0 has n components, each holding a definition and a use.
            let candidates: Vec<_> = (1..=n).map(|i| n + 2 * i).collect();
            let walks = Cell::new(0);
            let siblings = Cell::new(0);
            let candidates_read = Cell::new(0);
            let index = IdIndex::default();
            let parent = |node| {
                walks.set(walks.get() + 1);
                match node {
                    0 => None,
                    i if i <= n => Some(0),
                    i => Some((i - n) / 2),
                }
            };
            for i in 1..=n {
                assert_eq!(
                    index.resolve(
                        "fade",
                        Some(n + 2 * i + 1),
                        || {
                            candidates_read.set(candidates_read.get() + candidates.len());
                            candidates.iter().copied()
                        },
                        parent,
                        |p| {
                            let children: Vec<_> = if p == 0 {
                                (1..=n).collect()
                            } else {
                                vec![n + 2 * p, n + 2 * p + 1]
                            };
                            siblings.set(siblings.get() + children.len());
                            children
                        },
                    ),
                    Some(n + 2 * i)
                );
            }
            eprintln!(
                "{n} references: {} parent reads, {} children read",
                walks.get(),
                siblings.get()
            );
            assert!(walks.get() <= 12 * n, "candidate walks must be shared");
            assert_eq!(siblings.get(), 3 * n as usize);
            assert_eq!(candidates_read.get(), n as usize);
        }
    }
}
