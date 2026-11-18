//! The reader index (@ref LLP 1123 D9 "Cost", D5 "Captures"): which nodes
//! read what only their own environment has, so a change of an owner's
//! insets, an inset override, a declaration or a content root derives again
//! the boxes whose inputs it moves, found from these sets rather than from a
//! walk of every node it might reach.
//!
//! The sets follow each node's own style. A platform default never reads a
//! local inset, a custom property or a container unit (`arena/defaults.rs`
//! builds each from the schema's metrics), so a node's effective style reads
//! one exactly where its own does.
//!
//! The index also keeps each declaration's substituted value, which every
//! reader of it shares, since a value is resolved where it is declared and
//! not where it is read (D5). They are all dropped at any change a value
//! can depend on: a declaration or an inset override, a move while any node
//! declares, an owner's insets while a capture reads them, a capture's
//! observation when the solve moves it or a content region withholds it,
//! and the document's environment. A slot a declaring node leaves keeps its values
//! until the next node there declares, which drops them.

use super::owner::{self, OVERRIDES};
use crate::arena::NodeArena;
use crate::generated::{StyleCodec, StyleId, StyleProps};
use crate::id::IdHasher;
use crate::sorted::SlotSet;
use crate::style::calc::{self, EnvTerm, Node};
use crate::style::targets::Target;
use crate::style::{Dimension, Edge, RowValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub(crate) struct Readers {
    /// Nodes with a length that reads a local inset.
    pub(crate) local: SlotSet,
    /// Nodes with a length that reads an author's custom property.
    pub(crate) properties: SlotSet,
    /// Nodes with a length that reads an observation, of its safe area or
    /// the keyboard.
    pub(crate) observers: SlotSet,
    /// The components each observer reads, as
    /// [`calc::Comparison::observed_components`] gives them.
    pub(crate) observed: crate::id::IdMap<u32, crate::style::calc::Components>,
    /// Nodes with a length that reads a native target's raw insets, and
    /// the targets each reads, each with the edges it reads as bits (LLP
    /// 1123 D9, "The native chain").
    pub(crate) targets: crate::id::IdMap<u32, Vec<(Target, u8)>>,
    /// Nodes with a length that reads a container unit (LLP 1123 D16), which
    /// a content root they read moves (`block::clear_block`).
    pub(crate) container: SlotSet,
    /// Nodes that declare a custom property.
    declaring: SlotSet,
    /// Nodes that declare one whose value reads a local inset: captures.
    captures: SlotSet,
    /// Nodes that declare one whose value reads the node's own observation,
    /// a keyboard reading or a layout region (LLP 1123 D5): captures that the readers'
    /// solve measures, as it measures a length that reads one.
    observing: SlotSet,
    /// Nodes that override a local inset.
    overriding: SlotSet,
    /// Each declaration's value, by its node and name, as
    /// [`owner::properties_at`] substitutes it; `None` where it is invalid.
    values: RefCell<HashMap<owner::Key, Option<Node>>>,
}

impl Readers {
    /// Record what `slot`'s own style, now `style` and before `old`, reads
    /// and declares.
    pub(crate) fn update(&mut self, slot: u32, old: &StyleProps, style: &StyleProps) {
        let differs = |id| {
            old.mask.has(id) != style.mask.has(id)
                || style.mask.has(id) && old.get(id) != style.get(id)
        };
        if differs(StyleId::SafeAreaProperties) || OVERRIDES.into_iter().any(differs) {
            self.invalidate();
        }
        let (mut local, mut properties, mut container, mut observes) = (false, false, false, 0);
        let mut targets = Vec::new();
        for id in style.mask.iter() {
            // Skip the inset overrides: `local_at` resolves them for the subtree,
            // whose readers are indexed
            if id.codec() != StyleCodec::Dimension || OVERRIDES.contains(&id) {
                continue;
            }
            let RowValue::Dimension(d) = style.get(id) else {
                continue;
            };
            container |= d.reads_container();
            if let Dimension::Compare(c) = d {
                local |= c.reads_local();
                properties |= c.reads_custom();
                observes |= c.observed_components();
                targets.extend(c.targets_read());
            }
        }
        let own = owner::declarations(style);
        let captures = own.iter().any(|(_, value)| reads_local(value));
        let capture_components = own
            .iter()
            .fold(0, |c, (_, value)| c | observed_components(value));
        observes |= capture_components;
        put(&mut self.local, slot, local);
        put(&mut self.properties, slot, properties);
        put(&mut self.container, slot, container);
        put(&mut self.observers, slot, observes != 0);
        match observes {
            0 => self.observed.remove(&slot),
            components => self.observed.insert(slot, components),
        };
        match targets.is_empty() {
            true => self.targets.remove(&slot),
            false => self.targets.insert(slot, merged(targets)),
        };
        put(&mut self.declaring, slot, own.iter().next().is_some());
        put(&mut self.captures, slot, captures);
        put(&mut self.observing, slot, capture_components != 0);
        let overrides = OVERRIDES.iter().any(|&id| style.mask.has(id));
        put(&mut self.overriding, slot, overrides);
    }

    pub(crate) fn forget(&mut self, slot: u32) {
        for set in [
            &mut self.local,
            &mut self.properties,
            &mut self.container,
            &mut self.observers,
            &mut self.declaring,
            &mut self.captures,
            &mut self.observing,
            &mut self.overriding,
        ] {
            set.remove(slot);
        }
        self.observed.remove(&slot);
        self.targets.remove(&slot);
    }

    pub(crate) fn clear(&mut self) {
        *self = Readers::default();
    }

    /// Whether any node declares a custom property.
    pub(crate) fn declares(&self) -> bool {
        !self.declaring.is_empty()
    }

    /// Whether any node overrides a local inset.
    pub(crate) fn overrides(&self) -> bool {
        !self.overriding.is_empty()
    }

    /// A move: which declaration a name reads at a node can change.
    pub(crate) fn moved(&mut self) {
        if !self.declaring.is_empty() {
            self.invalidate();
        }
    }

    /// An owner's local insets changed, which a capture reads.
    pub(crate) fn insets_changed(&mut self) {
        if !self.captures.is_empty() {
            self.invalidate();
        }
    }

    /// The nodes to derive again after the solve, or a content region, gave
    /// the readers `moved` new inputs: below each of `moved` that captures an
    /// observation, each node whose lengths take a value that depends on a
    /// declaration of the capture that reads one ([`owner::Depends`]). Drops
    /// every kept value when any of `moved` is such a capture.
    pub(crate) fn captured(arena: &mut NodeArena, moved: &[u32]) -> Vec<u32> {
        let readers = &arena.readers;
        let captures: Vec<u32> = (moved.iter().copied())
            .filter(|&s| readers.observing.contains(s))
            .collect();
        if captures.is_empty() {
            return Vec::new();
        }
        arena.readers.invalidate();
        let arena = &*arena;
        let mut out = Vec::new();
        for capture in captures {
            let own = owner::own_declarations(arena, capture);
            let changed: Vec<owner::Key> = (own.iter())
                .filter(|(_, value)| observed_components(value) != 0)
                .map(|(name, _)| (capture, name.into()))
                .collect();
            let mut depends = owner::Depends::new(arena, changed, true);
            let readers = below(arena, capture, &arena.readers.properties);
            out.extend(readers.into_iter().filter(|&s| depends.reader(s)));
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Drop every kept value.
    pub(crate) fn invalidate(&mut self) {
        self.values.get_mut().clear();
    }

    /// The kept value of a declaration; `None` where none is kept.
    pub(crate) fn value(&self, key: &owner::Key) -> Option<Option<Node>> {
        self.values.borrow().get(key).cloned()
    }

    pub(crate) fn keep(&self, key: owner::Key, value: Option<Node>) {
        self.values.borrow_mut().insert(key, value);
    }
}

fn put(set: &mut SlotSet, slot: u32, member: bool) {
    match member {
        true => set.insert(slot),
        false => set.remove(slot),
    };
}

/// The components of its node's own observations a declared value reads
/// (LLP 1123 D5): a capture of them.
fn observed_components(value: &Dimension) -> crate::style::calc::Components {
    match value {
        Dimension::Compare(c) => c.observed_components(),
        _ => 0,
    }
}

/// Whether a declared value reads a local inset.
fn reads_local(value: &Dimension) -> bool {
    match value {
        Dimension::Compare(c) => c.reads_local(),
        other => calc::tree_of(other)
            .is_some_and(|t| t.any(&|n| matches!(n, Node::Env(EnvTerm::Local(_))))),
    }
}

/// `read`, each target once, its edges merged.
fn merged(mut read: Vec<(Target, u8)>) -> Vec<(Target, u8)> {
    read.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    let mut out: Vec<(Target, u8)> = Vec::with_capacity(read.len());
    for (target, edges) in read {
        match out.last_mut() {
            Some((last, merged)) if *last == target => *merged |= edges,
            _ => out.push((target, edges)),
        }
    }
    out
}

/// What a change of the local insets at `top` can reach, taken before it:
/// each local-inset reader and capture in `top`'s region, with the insets
/// it reads there. The region is `top`'s subtree, less the subtrees of
/// owners below it whose hosts set insets of their own, which their nodes
/// read instead ([`owner::local_at`]).
pub(crate) struct InsetReaders {
    top: u32,
    local: Vec<(u32, [f32; 4])>,
    captures: Vec<(u32, [f32; 4])>,
}

impl InsetReaders {
    /// Each reader in `top`'s region, its inputs to be read as moved: what
    /// gave them before is gone.
    pub(crate) fn unknown(arena: &NodeArena, top: u32) -> Self {
        let (local, captures) = region(arena, top);
        let unknown = |s: u32| (s, [f32::NAN; 4]);
        InsetReaders {
            top,
            local: local.into_iter().map(unknown).collect(),
            captures: captures.into_iter().map(unknown).collect(),
        }
    }

    pub(crate) fn before(arena: &NodeArena, top: u32) -> Self {
        let (local, captures) = region(arena, top);
        let mut insets = owner::LocalInsets::new(&arena.readers.captures);
        let mut at = |slots: Vec<u32>| -> Vec<(u32, [f32; 4])> {
            slots
                .into_iter()
                .map(|s| (s, insets.at(arena, s)))
                .collect()
        };
        let captures = at(captures);
        InsetReaders {
            top,
            local: at(local),
            captures,
        }
    }

    /// The readers whose inputs the change moved: each local-inset reader
    /// whose lengths read an edge that changed where it is, and each reader
    /// of a capture's names that read one, which change as a changed
    /// declaration's do, read where each read is taken ([`owner::Depends`]).
    pub(crate) fn moved(self, arena: &NodeArena) -> Vec<u32> {
        let mut insets = owner::LocalInsets::new(&arena.readers.captures);
        let mut changed = |s: u32, old: [f32; 4]| {
            let now = insets.at(arena, s);
            [0, 1, 2, 3].map(|e| now[e] != old[e])
        };
        let mut out: Vec<u32> = (self.local.into_iter())
            .filter(|&(s, old)| {
                let changed = changed(s, old);
                changed.contains(&true) && reads_edges(arena, s, changed)
            })
            .map(|(s, _)| s)
            .collect();
        let mut moved = Vec::new();
        for (capture, old) in self.captures {
            let changed = changed(capture, old);
            let defined = |name: &str| owner::defined_at(arena, capture, name);
            for (name, value) in owner::own_declarations(arena, capture).iter() {
                if value_reads_edges(value, changed, &defined) {
                    moved.push((capture, name.into()));
                }
            }
        }
        if !moved.is_empty() {
            let mut depends = owner::Depends::new(arena, moved, false);
            let readers = below(arena, self.top, &arena.readers.properties);
            out.extend(readers.into_iter().filter(|&s| depends.reader(s)));
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn reads_edge(n: &Node, edges: [bool; 4]) -> bool {
    matches!(n, Node::Env(EnvTerm::Local(e)) if edges[*e as usize])
}

/// Whether a declared value reads one of `edges` where it is read, a
/// `var()` whose property is `defined` standing in for its fallback.
fn value_reads_edges(value: &Dimension, edges: [bool; 4], defined: &dyn Fn(&str) -> bool) -> bool {
    match value {
        Dimension::Compare(c) => c
            .tree(|t| read_where_taken(t, edges, defined))
            .unwrap_or_else(|| (0..4).any(|e| edges[e] && c.reads_local_edge(Edge::ALL[e]))),
        other => calc::tree_of(other).is_some_and(|t| read_where_taken(&t, edges, defined)),
    }
}

/// Whether a length of `slot` reads one of `edges` of its local insets,
/// where it reads them: not in the fallback of a custom property `slot`
/// has, since the property's value stands there instead.
fn reads_edges(arena: &NodeArena, slot: u32, edges: [bool; 4]) -> bool {
    let style = arena.style(slot);
    let resolved = match arena.readers.properties.contains(slot) {
        true => owner::properties_at(arena, slot, style),
        false => Default::default(),
    };
    let defined = |name: &str| resolved.get(name).is_some();
    style.mask.iter().any(|id| {
        if id.codec() != StyleCodec::Dimension || OVERRIDES.contains(&id) {
            return false;
        }
        let RowValue::Dimension(Dimension::Compare(c)) = style.get(id) else {
            return false;
        };
        let any = |e: usize| edges[e] && c.reads_local_edge(Edge::ALL[e]);
        c.tree(|t| read_where_taken(t, edges, &defined))
            .unwrap_or_else(|| (0..4).any(any))
    })
}

/// Whether `node` reads one of `edges` where it is read: past the fallback
/// of a `var()` whose property is `defined`.
fn read_where_taken(node: &Node, edges: [bool; 4], defined: &dyn Fn(&str) -> bool) -> bool {
    match node {
        Node::Var(v) if defined(&v.name) => false,
        n if reads_edge(n, edges) => true,
        n => n.children().any(|c| read_where_taken(c, edges, defined)),
    }
}

/// The nodes in `top`'s subtree whose lengths read a container unit, which a
/// change to `top`'s content root moves.
pub(crate) fn container_readers(arena: &NodeArena, top: u32) -> Vec<u32> {
    match arena.readers.container.is_empty() {
        true => Vec::new(),
        false => below(arena, top, &arena.readers.container),
    }
}

/// The members of `set` in `top`'s subtree: walking down, past no more than
/// two nodes for each member, else checking each member's ancestors.
fn below(arena: &NodeArena, top: u32, set: &SlotSet) -> Vec<u32> {
    let budget = 2 * set.len() + 1;
    match walk_down(arena, top, budget, |_| true) {
        Some(nodes) => nodes.into_iter().filter(|&s| set.contains(s)).collect(),
        None => (set.iter())
            .filter(|&s| in_region(arena, &SlotSet::default(), top, s))
            .collect(),
    }
}

/// The local-inset readers and captures in `top`'s region. A walk down
/// finds them where the region is small; it stops past two nodes for each
/// member of either set, about what checking each member's ancestors
/// costs, and then each member's ancestors are checked.
fn region(arena: &NodeArena, top: u32) -> (Vec<u32>, Vec<u32>) {
    let readers = &arena.readers;
    region_within(
        arena,
        top,
        2 * (readers.local.len() + readers.captures.len()) + 1,
    )
}

/// [`region`], walking down past no more than `budget` nodes.
fn region_within(arena: &NodeArena, top: u32, budget: usize) -> (Vec<u32>, Vec<u32>) {
    let readers = &arena.readers;
    let owners = arena.blocks.local_slots();
    let member = |s: u32| (readers.local.contains(s), readers.captures.contains(s));
    let walked = walk_down(arena, top, budget, |c| !owners.contains(c));
    if let Some(nodes) = walked {
        let (mut local, mut captures) = (Vec::new(), Vec::new());
        for s in nodes {
            let (l, c) = member(s);
            if l {
                local.push(s);
            }
            if c {
                captures.push(s);
            }
        }
        return (local, captures);
    }
    let inside = |s: &u32| in_region(arena, owners, top, *s);
    (
        readers.local.iter().filter(inside).collect(),
        readers.captures.iter().filter(inside).collect(),
    )
}

/// The nodes of `top`'s subtree, `top` first, that `enter` lets the walk
/// into; `None` past `budget` nodes.
pub(crate) fn walk_down(
    arena: &NodeArena,
    top: u32,
    mut budget: usize,
    enter: impl Fn(u32) -> bool,
) -> Option<Vec<u32>> {
    budget = budget.checked_sub(1)?;
    let (mut out, mut stack) = (Vec::new(), vec![top]);
    while let Some(s) = stack.pop() {
        out.push(s);
        for &c in arena.children(s) {
            if arena.parent(c) != Some(s) {
                continue;
            }
            budget = budget.checked_sub(1)?;
            if enter(c) {
                stack.push(c);
            }
        }
    }
    Some(out)
}

/// Whether `slot` is in the region of the local insets at `top`, `owners`
/// the nodes whose hosts set insets.
fn in_region(arena: &NodeArena, owners: &SlotSet, top: u32, slot: u32) -> bool {
    let mut at = Some(slot);
    while let Some(s) = at {
        if s == top {
            return true;
        }
        if owners.contains(s) {
            return false;
        }
        at = arena.parent(s);
    }
    false
}

/// The property readers in `slot`'s subtree whose values change when the
/// names `changed` change there: each that reads a changed name. A nearer
/// declaration stops a name unless its value reads another changed name.
///
/// A walk down from `slot` finds them where the change stays near it: in a
/// small subtree, or one whose nodes declare the names again. It stops past
/// two nodes for each property reader, about what walking up from each
/// costs, and then the walk up from each finds them, at a cost of their
/// number times their depth however far the change reaches.
pub(crate) fn of_declarations(arena: &NodeArena, slot: u32, changed: Vec<Arc<str>>) -> Vec<u32> {
    declared_at(arena, slot, &changed).find(arena)
}

/// The changes below `slot` when the names `changed` change there.
fn declared_at(arena: &NodeArena, slot: u32, changed: &[Arc<str>]) -> Changes {
    let own = owner::own_declarations(arena, slot);
    Changes::new(slot, owner::reaching(&own, changed, true))
}

/// The names whose values change at each node of `top`'s subtree: those
/// that change at `top`, carried down through the declaring nodes below
/// it. Walking up from a reader, each declaring node's names are kept for
/// the readers below it.
struct Changes {
    top: u32,
    /// The names that change at `top`.
    at_top: Rc<[Arc<str>]>,
    memo: HashMap<u32, Rc<[Arc<str>]>, BuildHasherDefault<IdHasher>>,
    /// One empty list of names, which every node no change reaches shares.
    no_names: Rc<[Arc<str>]>,
    /// The declaring nodes between a reader and `top`, nearest first.
    path: Vec<u32>,
    /// The names a reader reads.
    read: Vec<Arc<str>>,
}

impl Changes {
    fn new(top: u32, at_top: Vec<Arc<str>>) -> Self {
        Changes {
            top,
            at_top: at_top.into(),
            memo: HashMap::default(),
            no_names: Rc::from([]),
            path: Vec::new(),
            read: Vec::new(),
        }
    }

    /// The readers of a changed name below `top`. A walk down finds them
    /// where the change stays near `top`; it stops past two nodes for each
    /// property reader, about what walking up from each costs, and then the
    /// walk up from each finds them, at a cost of their number times their
    /// depth however far the change reaches.
    fn find(&mut self, arena: &NodeArena) -> Vec<u32> {
        let budget = 2 * arena.readers.properties.len() + 1;
        self.down(arena, budget).unwrap_or_else(|| self.up(arena))
    }

    /// The readers of a changed name below `top`, walking down; `None` past
    /// `budget` nodes, each child counted before its declarations are read.
    fn down(&mut self, arena: &NodeArena, mut budget: usize) -> Option<Vec<u32>> {
        let readers = &arena.readers;
        budget = budget.checked_sub(1)?;
        let mut out = Vec::new();
        let mut stack = vec![(self.top, self.at_top.clone())];
        while let Some((s, names)) = stack.pop() {
            if readers.properties.contains(s) && self.reads(arena, s, &names) {
                out.push(s);
            }
            for &c in arena.children(s) {
                if arena.parent(c) != Some(s) {
                    continue;
                }
                budget = budget.checked_sub(1)?;
                let names = match readers.declaring.contains(c) {
                    true => self.shared(owner::reaching(
                        &owner::own_declarations(arena, c),
                        &names,
                        false,
                    )),
                    false => names.clone(),
                };
                if !names.is_empty() {
                    stack.push((c, names));
                }
            }
        }
        Some(out)
    }

    /// The readers of a changed name below `top`, walking up from each
    /// property reader.
    fn up(&mut self, arena: &NodeArena) -> Vec<u32> {
        (arena.readers.properties.iter())
            .filter(|&s| self.read_at(arena, s))
            .collect()
    }

    /// Whether a length of `slot` reads a name whose value changes there.
    fn read_at(&mut self, arena: &NodeArena, slot: u32) -> bool {
        let Some(changed) = self.at(arena, slot).filter(|c| !c.is_empty()) else {
            return false;
        };
        self.reads(arena, slot, &changed)
    }

    fn shared(&self, names: Vec<Arc<str>>) -> Rc<[Arc<str>]> {
        match names.is_empty() {
            true => self.no_names.clone(),
            false => names.into(),
        }
    }

    /// Whether a length of `slot` reads one of `names`.
    fn reads(&mut self, arena: &NodeArena, slot: u32, names: &[Arc<str>]) -> bool {
        self.read.clear();
        owner::read_names_into(arena.style(slot), &mut self.read);
        self.read.iter().any(|n| names.contains(n))
    }

    /// The names that change at `slot`; `None` where it is not in `top`'s
    /// subtree.
    fn at(&mut self, arena: &NodeArena, slot: u32) -> Option<Rc<[Arc<str>]>> {
        self.path.clear();
        let mut at = Some(slot);
        let mut names = loop {
            let s = at?;
            if s == self.top {
                break self.at_top.clone();
            }
            // Only a declaring node changes the names, so only one is kept
            if arena.readers.declaring.contains(s) {
                if let Some(known) = self.memo.get(&s) {
                    break known.clone();
                }
                self.path.push(s);
            }
            at = arena.parent(s);
        };
        for i in (0..self.path.len()).rev() {
            let s = self.path[i];
            let own = owner::own_declarations(arena, s);
            names = self.shared(owner::reaching(&own, &names, false));
            self.memo.insert(s, names.clone());
        }
        Some(names)
    }
}

#[cfg(test)]
#[path = "readers_tests.rs"]
mod tests;
