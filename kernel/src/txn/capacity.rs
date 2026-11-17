use super::{NodeArena, Op};

/// Initial capacities without scratch sets to deduplicate operations. Style
/// and prop updates need none; destruction reserves its subtrees as it walks.
pub(super) struct Batch {
    pub created: usize,
    pub destroyed: usize,
    pub parents: usize,
    pub children: usize,
    pub roots_added: usize,
    pub roots_removed: usize,
}

impl Batch {
    pub fn new(arena: &NodeArena, ops: &[Op]) -> Self {
        let mut created = 0usize;
        let mut destroyed = 0usize;
        let mut destroyed_roots = 0usize;
        let mut lists = 0usize;
        let mut parents = 0usize;
        let mut roots = 0usize;
        for op in ops {
            match op {
                Op::CreateView { .. } => created += 1,
                Op::DestroyView { id } => {
                    destroyed += 1;
                    destroyed_roots +=
                        usize::from(arena.slot_of(*id).is_some_and(|s| arena.is_root(s)));
                }
                Op::AttachRoot { .. } => roots += 1,
                Op::SetChildren { id, children } => {
                    lists += 1;
                    // Earlier lists in this batch are already counted. The
                    // arena's list also accounts for children detached here.
                    let old = arena.slot_of(*id).map_or(0, |s| arena.children(s).len());
                    parents = parents.saturating_add(children.len()).saturating_add(old);
                }
                _ => {}
            }
        }
        // Repeated operations cannot introduce more IDs than this. In
        // particular, replacing one wide list many times must not reserve
        // its width once per replacement.
        let nodes = arena.live_count().saturating_add(created);
        Self {
            created,
            destroyed: destroyed.min(nodes),
            parents: parents
                .saturating_add(created)
                .saturating_add(destroyed)
                .min(nodes),
            children: created.saturating_add(lists).min(nodes),
            roots_added: roots.min(nodes),
            roots_removed: destroyed
                .min(destroyed_roots.saturating_add(roots))
                .min(nodes),
        }
    }
}
