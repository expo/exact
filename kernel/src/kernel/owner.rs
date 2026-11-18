//! The environment a box's lengths resolve against (`NodeArena::env_of`,
//! @ref LLP 1123 D9), and two things it resolves: runtime custom properties
//! and captures (D5), and main's local insets (expo/exact#36).
//!
//! The local insets stay until LLP 1123 is activated, which replaces them
//! with D9's observations and D10's leaf, and their authored overrides with
//! D5's captures (§"What it replaces"). A native container that hosts Exact
//! content (a route under a navigation bar, a tab panel above a tab bar, a
//! sheet under its grabber) says its owner's insets on the owner's root
//! (`Kernel::set_local_insets`); `var(--exact-safe-area-inset-<edge>)` reads
//! the nearest one at or above a node, as an inherited custom property reads
//! its nearest declaration, and the session root's ([`crate::Env::local`])
//! where no owner holds the node. An author may declare the property on any
//! node, spelled as it is read (`--exact-safe-area-inset-bottom="0px"`); its
//! subtree then reads that, as CSS Variables 1 inherits a custom property's
//! computed value, and on an owner's root the authored value stands over the
//! host's, which stands over the parent's.
//!
//! Neither changes layout by itself: a length that reads one is resolved per
//! node where its engine style is derived. An owner's own clearance of its
//! chrome is D3's content root (`block.rs`).

use super::readers::InsetReaders;
use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::generated::{StyleId, StyleMask, StyleProps};
use crate::id::ViewId;
use crate::layout::LayoutMirror;
use crate::style::calc::{self, simplify, Node};
use crate::style::safe_area_properties::{Resolved, SafeAreaProperties};
use crate::style::{Dimension, Edge, Env, RowValue};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The rows an author overrides the four insets with, top right bottom left.
pub(crate) const OVERRIDES: [StyleId; 4] = [
    StyleId::LocalInsetTop,
    StyleId::LocalInsetRight,
    StyleId::LocalInsetBottom,
    StyleId::LocalInsetLeft,
];

/// Whether a style's lengths read what only the node's own environment
/// has: a local inset or an author's custom property (LLP 1123 D5, D9).
pub(crate) fn reads_node(style: &StyleProps) -> bool {
    !node_rows(style).is_empty()
}

/// The rows of `style` whose lengths read a local inset, its own overrides
/// aside (they read their parent's, [`local_at`]), or an author's custom
/// property.
fn node_rows(style: &StyleProps) -> StyleMask {
    rows_where(style, |c| {
        c.reads_local() || c.reads_custom() || c.reads_observed()
    })
}

fn rows_where(style: &StyleProps, reads: impl Fn(&calc::Comparison) -> bool) -> StyleMask {
    let mut rows = StyleMask::EMPTY;
    for id in style.mask.iter().filter(|id| !OVERRIDES.contains(id)) {
        if matches!(style.get(id), RowValue::Dimension(Dimension::Compare(c)) if reads(&c)) {
            rows.set(id);
        }
    }
    rows
}

/// Derive `slot`'s lengths again after their local insets or custom
/// properties changed, through what a change of the rows that read them
/// does (`txn::style_changed`): a paragraph is measured again where one is
/// a text row, an inline image's or an inline edge, whatever the value. An
/// engine style that comes out the same lays the box out no more
/// (`TaffyMirror::write_style`); `dirty` lays it out regardless, as an inset
/// change does.
pub(crate) fn rederive(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    dirty: bool,
    receipt: &mut crate::CommitReceipt,
) {
    let rows = node_rows(&arena.effective_style(slot));
    crate::txn::style_changed(arena, layout, slot, rows, receipt);
    if let Some(node) = arena.taffy(slot).filter(|_| dirty) {
        layout.mark_dirty(node);
    }
}

/// What a change of `slot`'s rows `changed` can reach, taken before it:
/// its declarations, where it changes them (LLP 1123 D5), and the readers
/// of its inset overrides with the insets they read, where it changes one
/// (D9).
pub(crate) struct Before {
    declared: Option<SafeAreaProperties>,
    insets: Option<InsetReaders>,
}

pub(crate) fn before_style(arena: &NodeArena, slot: u32, changed: StyleMask) -> Before {
    Before {
        declared: changed
            .has(StyleId::SafeAreaProperties)
            .then(|| own_declarations(arena, slot)),
        insets: (OVERRIDES.iter().any(|&id| changed.has(id)))
            .then(|| InsetReaders::before(arena, slot)),
    }
}

/// Derive again, and touch, what a change of `slot`'s rows moved
/// ([`before_style`]): the readers of each changed declaration, and each
/// reader whose inset an override's change moved, a capture's reader below
/// a nearer override among them, which the override's inheritance alone
/// does not reach.
pub(crate) fn after_style(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    before: Before,
    touched: &mut Vec<crate::id::NodeKey>,
    receipt: &mut crate::CommitReceipt,
) {
    if let Some(old) = before.declared {
        declarations_changed(arena, layout, slot, &old, touched, receipt);
    }
    if let Some(insets) = before.insets {
        insets_moved(arena, layout, insets, touched, receipt);
    }
}

/// `slot`'s rows `rows` resolved again under a new root font size: where
/// one is an inset override, every reader of it is derived again, the
/// insets it gave before being gone.
pub(crate) fn resolved_again(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    rows: StyleMask,
    touched: &mut Vec<crate::id::NodeKey>,
    receipt: &mut crate::CommitReceipt,
) {
    if OVERRIDES.iter().any(|&id| rows.has(id)) {
        let insets = InsetReaders::unknown(arena, slot);
        insets_moved(arena, layout, insets, touched, receipt);
    }
}

fn insets_moved(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    insets: InsetReaders,
    touched: &mut Vec<crate::id::NodeKey>,
    receipt: &mut crate::CommitReceipt,
) {
    for s in insets.moved(arena) {
        rederive(arena, layout, s, true, receipt);
        touched.push(arena.key(s));
    }
}

/// `slot`'s custom properties changed from `old` (LLP 1123 D5): each reader
/// of a name whose value changed is derived again, in `slot`'s subtree,
/// below the nodes that declare that name again, and through the
/// declarations that read it.
pub(crate) fn declarations_changed(
    arena: &mut NodeArena,
    layout: &mut dyn LayoutMirror,
    slot: u32,
    old: &SafeAreaProperties,
    touched: &mut Vec<crate::id::NodeKey>,
    receipt: &mut crate::CommitReceipt,
) {
    let new = own_declarations(arena, slot);
    let mut changed: Vec<Arc<str>> = Vec::new();
    for (name, _) in old.iter().chain(new.iter()) {
        if old.get(name) != new.get(name) && !changed.iter().any(|n| &**n == name) {
            changed.push(name.into());
        }
    }
    if changed.is_empty() {
        return;
    }
    for s in super::readers::of_declarations(arena, slot, changed) {
        rederive(arena, layout, s, false, receipt);
        touched.push(arena.key(s));
    }
}

/// The names whose values change at a node that declares `own`, when those
/// in `above` changed: the changed ones it does not declare again, and each
/// it declares that reads a changed one. `at_root`: `own` is what changed,
/// so `above` names its own too.
pub(crate) fn reaching(
    own: &SafeAreaProperties,
    above: &[Arc<str>],
    at_root: bool,
) -> Vec<Arc<str>> {
    let mut out: Vec<Arc<str>> = match at_root {
        true => above.to_vec(),
        false => above
            .iter()
            .filter(|n| own.get(n).is_none())
            .cloned()
            .collect(),
    };
    // What each declaration reads, collected once
    let reads: Vec<(&str, Vec<Arc<str>>)> = own
        .iter()
        .map(|(name, value)| {
            let mut names = Vec::new();
            if let Some(t) = calc::tree_of(value) {
                t.names(&mut names);
            }
            (name, names)
        })
        .collect();
    loop {
        let mut added = false;
        for (name, names) in &reads {
            if out.iter().any(|n| &**n == *name) {
                continue;
            }
            // A name it declares is its own; any other, its parent's
            let moved = names.iter().any(|m| match own.get(m) {
                Some(_) => out.contains(m),
                None => above.contains(m),
            });
            if moved {
                out.push((*name).into());
                added = true;
            }
        }
        if !added {
            return out;
        }
    }
}

/// The custom properties `slot` declares itself.
pub(crate) fn own_declarations(arena: &NodeArena, slot: u32) -> SafeAreaProperties {
    declarations(arena.style(slot))
}

/// The custom properties `style` declares.
pub(crate) fn declarations(style: &StyleProps) -> SafeAreaProperties {
    match style.get(StyleId::SafeAreaProperties) {
        RowValue::SafeAreaProperties(own) if style.mask.has(StyleId::SafeAreaProperties) => {
            own.clone()
        }
        _ => SafeAreaProperties::default(),
    }
}

/// The environment `slot`'s lengths, `style`, resolve against: the
/// arena's, unless they read a local inset ([`env_at`]).
pub(crate) fn env_for<'a>(arena: &'a NodeArena, slot: u32, style: &StyleProps) -> Cow<'a, Env> {
    match reads_node(style) {
        true => env_at(arena, slot),
        false => Cow::Borrowed(arena.env()),
    }
}

/// The environment `slot`'s lengths resolve against: the arena's, with the
/// local insets `slot` inherits ([`local_at`]) and the custom properties its
/// lengths read ([`properties_at`]).
pub(crate) fn env_at(arena: &NodeArena, slot: u32) -> Cow<'_, Env> {
    let mut env = arena.env().clone();
    env.local = Some(local_at(arena, slot));
    env.properties = Some(properties_at(arena, slot, &arena.effective_style(slot)));
    env.observed = Some(arena.observations.input(slot));
    env.targets = arena.observations.targets.scope(arena, slot);
    Cow::Owned(env)
}

/// The values of the custom properties `style`'s lengths read at `slot`
/// (LLP 1123 D5), each resolved where it is declared.
pub(crate) fn properties_at(arena: &NodeArena, slot: u32, style: &StyleProps) -> Resolved {
    let names = read_names(style);
    if names.is_empty() {
        return Resolved::default();
    }
    // Every declaration first, so a walk past the bound invalidates every
    // value, whatever order the rows read them in
    let mut resolver = Resolver::new(arena);
    let keys: Vec<_> = names.iter().map(|n| resolver.reach(slot, n)).collect();
    if resolver.exhausted {
        return Resolved::default();
    }
    let values = names
        .into_iter()
        .zip(keys)
        .filter_map(|(name, key)| Some((name, resolver.value(&key?)?)))
        .collect();
    Resolved(values)
}

/// The custom properties `style`'s lengths read, fallbacks included.
fn read_names(style: &StyleProps) -> Vec<Arc<str>> {
    let mut names = Vec::new();
    read_names_into(style, &mut names);
    names
}

/// [`read_names`], into `out`.
pub(crate) fn read_names_into(style: &StyleProps, out: &mut Vec<Arc<str>>) {
    for id in style.mask.iter() {
        if let RowValue::Dimension(Dimension::Compare(c)) = style.get(id) {
            c.properties(out);
        }
    }
}

/// The most declarations one node's lengths reach: past it every value is
/// invalid, as CSS Variables 1 §3.3 has an over-long expansion.
const MAX_DECLARATIONS: usize = 1024;

/// A declaration: the node that declares a name, and the name.
pub(crate) type Key = (u32, Arc<str>);

/// Whether the values a node's lengths take depend on any of `changed`,
/// declarations whose values moved: read where each read is taken, past
/// the fallback of a property the node has, and each declaration resolved
/// where it is declared (LLP 1123 D5), its answer kept for the next node.
pub(crate) struct Depends<'a> {
    arena: &'a NodeArena,
    changed: Vec<Key>,
    known: std::collections::HashMap<Key, bool>,
}

impl<'a> Depends<'a> {
    pub(crate) fn new(arena: &'a NodeArena, mut changed: Vec<Key>) -> Self {
        changed.sort_unstable();
        Depends {
            arena,
            changed,
            known: Default::default(),
        }
    }

    /// Whether a length of `slot`, its inset overrides aside, takes a value
    /// that depends on a changed declaration.
    pub(crate) fn reader(&mut self, slot: u32) -> bool {
        let style = self.arena.style(slot);
        style.mask.iter().any(|id| {
            let RowValue::Dimension(Dimension::Compare(c)) = style.get(id) else {
                return false;
            };
            if OVERRIDES.contains(&id) {
                return false;
            }
            // A text reads what its `env()` calls leave, as its row reads it
            c.tree(|t| self.takes(slot, t)).unwrap_or_else(|| {
                match c.substituted(self.arena.env()) {
                    Some(Dimension::Compare(s)) => s.tree(|t| self.takes(slot, t)).unwrap_or(false),
                    _ => false,
                }
            })
        })
    }

    /// Whether `node`, read at `at`, takes a value that depends on one.
    fn takes(&mut self, at: u32, node: &Node) -> bool {
        let Node::Var(v) = node else {
            return node.children().any(|c| self.takes(at, c));
        };
        let fallback = |this: &mut Self| v.fallback.as_deref().is_some_and(|f| this.takes(at, f));
        match declaring(self.arena, at, &v.name) {
            Some(slot) => {
                let key = (slot, v.name.clone());
                self.declaration(&key) || !defined_at(self.arena, at, &v.name) && fallback(self)
            }
            None => fallback(self),
        }
    }

    /// Whether the declaration `key`'s value depends on a changed one.
    fn declaration(&mut self, key: &Key) -> bool {
        if let Some(&known) = self.known.get(key) {
            return known;
        }
        // A cycle reads as moving nothing while its walk is open
        self.known.insert(key.clone(), false);
        let reads = match self.changed.binary_search(key) {
            Ok(_) => true,
            Err(_) => {
                let own = own_declarations(self.arena, key.0);
                let tree = own.get(&key.1).and_then(calc::tree_of);
                tree.is_some_and(|t| self.takes(key.0, &t))
            }
        };
        // An invalid declaration moves nothing, its reader taking a fallback:
        // no inset makes a declaration valid or invalid
        let depends = reads && defined_at(self.arena, key.0, &key.1);
        self.known.insert(key.clone(), depends);
        depends
    }
}

/// The node that declares `name` nearest at or above `from`.
fn declaring(arena: &NodeArena, from: u32, name: &str) -> Option<u32> {
    let mut at = Some(from);
    while let Some(slot) = at {
        if own_declarations(arena, slot).get(name).is_some() {
            return Some(slot);
        }
        at = arena.parent(slot);
    }
    None
}

/// Whether the custom property `name` has a value at `slot`, read where it
/// is declared: a valid declaration at or above `slot`.
pub(crate) fn defined_at(arena: &NodeArena, slot: u32, name: &str) -> bool {
    let mut resolver = Resolver::new(arena);
    let name: Arc<str> = name.into();
    match resolver.reach(slot, &name) {
        Some(key) => resolver.value(&key).is_some(),
        None => false,
    }
}

/// Resolves custom properties where they are declared (CSS Variables 1
/// §2.3). It first finds every declaration a reader depends on, fallbacks
/// included, and the cycles among them (§2.3, by Tarjan's algorithm), whose
/// every member is invalid; then it substitutes the rest, each `var()` in a
/// declaration reading the declaring node's own values and a capture its
/// local inset.
struct Resolver<'a> {
    arena: &'a NodeArena,
    found: BTreeMap<Key, Declared>,
    /// Tarjan's stack of declarations whose component is open.
    stack: Vec<Key>,
    /// Whether the walk passed [`MAX_DECLARATIONS`].
    exhausted: bool,
}

struct Declared {
    /// Its value's tree; `None` where it holds none.
    tree: Option<Node>,
    /// Tarjan's visit index and low link.
    index: usize,
    low: usize,
    open: bool,
    cyclic: bool,
}

impl<'a> Resolver<'a> {
    fn new(arena: &'a NodeArena) -> Self {
        Resolver {
            arena,
            found: BTreeMap::new(),
            stack: Vec::new(),
            exhausted: false,
        }
    }

    /// `name`'s declaration as `from` has it, the nearest at or above it,
    /// visited with all it depends on; `None` where nothing declares it.
    fn reach(&mut self, from: u32, name: &Arc<str>) -> Option<Key> {
        let (slot, declared) = self.declaring(from, name)?;
        let key = (slot, name.clone());
        if !self.found.contains_key(&key) {
            self.visit(key.clone(), &declared);
        }
        Some(key)
    }

    fn declaring(&self, from: u32, name: &str) -> Option<(u32, Dimension)> {
        let mut at = Some(from);
        while let Some(slot) = at {
            if let RowValue::SafeAreaProperties(own) =
                self.arena.style(slot).get(StyleId::SafeAreaProperties)
            {
                if let Some(d) = own.get(name) {
                    return Some((slot, d.clone()));
                }
            }
            at = self.arena.parent(slot);
        }
        None
    }

    fn visit(&mut self, key: Key, declared: &Dimension) {
        let index = self.found.len();
        if index >= MAX_DECLARATIONS {
            self.exhausted = true;
        }
        let tree = calc::tree_of(declared);
        let mut names = Vec::new();
        if let Some(t) = &tree {
            t.names(&mut names);
        }
        self.found.insert(
            key.clone(),
            Declared {
                tree,
                index,
                low: index,
                open: true,
                cyclic: false,
            },
        );
        self.stack.push(key.clone());
        let mut low = index;
        let mut looped = false;
        for name in names {
            if self.exhausted {
                break;
            }
            let Some((slot, d)) = self.declaring(key.0, &name) else {
                continue;
            };
            let dep = (slot, name);
            looped |= dep == key;
            match self.found.get(&dep) {
                None => {
                    self.visit(dep.clone(), &d);
                    low = low.min(self.found[&dep].low);
                }
                Some(seen) if seen.open => low = low.min(seen.index),
                Some(_) => {}
            }
        }
        self.found.get_mut(&key).expect("visited").low = low;
        if low == index {
            let at = self
                .stack
                .iter()
                .rposition(|k| *k == key)
                .expect("on the stack");
            let component = self.stack.split_off(at);
            let cyclic = component.len() > 1 || looped;
            for k in component {
                let d = self.found.get_mut(&k).expect("visited");
                d.open = false;
                d.cyclic = cyclic;
            }
        }
    }

    /// A visited declaration's value, substituted where it is declared;
    /// `None` where it is invalid: in a cycle, reading an undefined
    /// environment value, or too long.
    fn value(&mut self, key: &Key) -> Option<Node> {
        if self.exhausted {
            return None;
        }
        if let Some(kept) = self.arena.readers.value(key) {
            return kept;
        }
        let d = self.found.get(key)?;
        let tree = d.tree.clone().filter(|_| !d.cyclic);
        let local = local_at(self.arena, key.0);
        let value = tree
            .and_then(|tree| {
                // A declaration holds no observation until captures are built
                // (LLP 1123 D5): one here, a keyboard reading included, takes
                // its fallback
                let leaves = calc::Leaves {
                    local: &|edge| local[edge as usize],
                    observed: &|_| None,
                };
                calc::substitute(&tree, &leaves, &mut |name| {
                    let dep = self.reach(key.0, name)?;
                    self.value(&dep)
                })
            })
            .map(simplify::simplify)
            .filter(|v| !calc::undefined_in(v, self.arena.env()) && calc::within_expansion(v));
        self.arena.readers.keep(key.clone(), value.clone());
        value
    }
}

impl NodeArena {
    /// The environment a box's lengths resolve against (LLP 1123 D9): the
    /// document's, with the box's own local insets where its style reads
    /// one. Every reader of a box's style rows takes this, never
    /// [`NodeArena::env`]: the document's has no local insets, and a
    /// length that reads one there would take the session root's under a
    /// route's bar (a debug build stops at it, `Env::local_inset`).
    pub fn env_of(&self, slot: u32) -> Cow<'_, Env> {
        env_for(self, slot, &self.effective_style(slot))
    }
}

/// The four local insets at `slot`, top right bottom left (main's model,
/// which LLP 1123 retires): an
/// override `slot` declares, else its owner's host value when `slot` is an
/// owner's root, else its parent's; at a root, the session root's. An
/// override's `var()` reads the edge as `slot` has it: that edge's override
/// on `slot`, else what `slot` inherits. Overrides that read themselves,
/// directly or through one another, form a cycle, invalid at computed-value
/// time, and so unset (CSS Variables 1 §3).
pub(crate) fn local_at(arena: &NodeArena, slot: u32) -> [f32; 4] {
    let base = match arena.blocks.local(slot) {
        Some(host) => host,
        None => match arena.parent(slot) {
            Some(parent) => local_at(arena, parent),
            None => Edge::ALL.map(|e| arena.env().root_inset(e)),
        },
    };
    with_overrides(arena, slot, base)
}

/// [`local_at`] for many nodes: those of `keep` are kept for the nodes
/// below them, so a chain of them is worked out once.
pub(crate) struct LocalInsets<'a> {
    keep: &'a crate::sorted::SlotSet,
    known: crate::id::IdMap<u32, [f32; 4]>,
    path: Vec<u32>,
}

impl<'a> LocalInsets<'a> {
    pub(crate) fn new(keep: &'a crate::sorted::SlotSet) -> Self {
        LocalInsets {
            keep,
            known: Default::default(),
            path: Vec::new(),
        }
    }

    pub(crate) fn at(&mut self, arena: &NodeArena, slot: u32) -> [f32; 4] {
        // The nodes up to the first whose insets are known or given
        let (path, mut at) = (&mut self.path, slot);
        path.clear();
        let base = loop {
            // Only a kept node is known, so only one is looked up
            if self.keep.contains(at) {
                if let Some(&known) = self.known.get(&at) {
                    break known;
                }
            }
            path.push(at);
            if let Some(host) = arena.blocks.local(at) {
                break host;
            }
            match arena.parent(at) {
                Some(parent) => at = parent,
                None => break Edge::ALL.map(|e| arena.env().root_inset(e)),
            }
        };
        // The topmost node takes `base`: what is known above it, its host's or the root's
        let mut insets = base;
        for &s in path.iter().rev() {
            insets = with_overrides(arena, s, insets);
            if self.keep.contains(s) {
                self.known.insert(s, insets);
            }
        }
        insets
    }
}

/// `slot`'s insets, `base` those it inherits, with its own overrides.
fn with_overrides(arena: &NodeArena, slot: u32, base: [f32; 4]) -> [f32; 4] {
    let style = arena.style(slot);
    if !OVERRIDES.iter().any(|&id| style.mask.has(id)) {
        return base;
    }
    let declared = OVERRIDES.map(|id| match style.get(id) {
        RowValue::Dimension(d) if style.mask.has(id) => Some(d),
        _ => None,
    });
    // Which declared edge reads which, and so, closed, which reads itself
    // through others: a cycle, whose members are unset (CSS Variables 1 §3)
    let mut reads = [[false; 4]; 4];
    for (i, d) in declared.iter().enumerate() {
        if let Some(Dimension::Compare(c)) = d {
            reads[i] = Edge::ALL.map(|e| declared[e as usize].is_some() && c.reads_local_edge(e));
        }
    }
    for k in 0..4 {
        for i in 0..4 {
            for j in 0..4 {
                reads[i][j] |= reads[i][k] && reads[k][j];
            }
        }
    }
    let mut out: [Option<f32>; 4] = [None; 4];
    for i in 0..4 {
        resolve(i, &declared, &reads, base, arena.env(), &mut out);
    }
    out.map(|v| v.unwrap_or(0.0))
}

/// Edge `i`'s value at a node: its override, read against the node's own
/// values of the edges it references (resolved first, the graph being
/// acyclic past its cycles), else what the node inherits (`base`), which a
/// cycle's member takes too.
fn resolve(
    i: usize,
    declared: &[Option<Dimension>; 4],
    reads: &[[bool; 4]; 4],
    base: [f32; 4],
    env: &Env,
    out: &mut [Option<f32>; 4],
) -> f32 {
    if let Some(v) = out[i] {
        return v;
    }
    let value = match &declared[i] {
        Some(d) if !reads[i][i] => {
            let mut local = base;
            for (j, v) in local.iter_mut().enumerate() {
                if j != i && reads[i][j] {
                    *v = resolve(j, declared, reads, base, env, out);
                }
            }
            let mut env = env.clone();
            env.local = Some(local);
            // An override reads no custom property, so a `var()` of one takes its fallback
            env.properties = Some(Resolved::default());
            d.at_basis(&env, 0.0)
        }
        _ => base[i],
    };
    out[i] = Some(value);
    value
}

/// A box moved to another parent, created ones included, may be in another
/// owner, and below other declarations, now: what in its subtree reads a
/// local inset or a custom property is derived again. Whether anything was.
pub(crate) fn moved(arena: &mut NodeArena, layout: &mut dyn LayoutMirror, slot: u32) -> bool {
    let readers = &arena.readers;
    // A host's insets or an override make insets differ from place to place
    let differ = !arena.blocks.local_slots().is_empty() || readers.overrides();
    let local = differ && !readers.local.is_empty();
    // With nothing declared, a property reader takes its fallback anywhere
    let properties = !readers.properties.is_empty() && readers.declares();
    // With no owner's or session's targets, a raw reader reads none anywhere
    let targets = !readers.targets.is_empty() && arena.observations.targets.placed_any();
    if !local && !properties && !targets {
        return false;
    }
    let mut receipt = crate::CommitReceipt::default();
    let mut any = false;
    for s in arena.subtree(slot) {
        let reads = local && arena.readers.local.contains(s)
            || properties && arena.readers.properties.contains(s)
            || targets && arena.readers.targets.contains_key(&s);
        if reads {
            rederive(arena, layout, s, false, &mut receipt);
            any = true;
        }
    }
    any
}

impl Kernel {
    /// Set or clear (`None`) the local insets of the content owner whose
    /// root is `view`, top right bottom left in points: what its container's
    /// chrome takes of its box, which `var(--exact-safe-area-inset-*)`
    /// reads in its subtree (main's model, which LLP 1123 replaces with D9's
    /// observations). Re-derives and dirties exactly the
    /// nodes there whose style reads one; whether any did (a layout is owed
    /// then). Nonfinite or negative insets are refused.
    pub fn set_local_insets(
        &mut self,
        view: ViewId,
        insets: Option<[f32; 4]>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if !insets
            .unwrap_or([0.0; 4])
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
        {
            return Err(LayoutError::InvalidEnv.into());
        }
        if self.arena.blocks.local(slot) == insets {
            return Ok(false);
        }
        let before = super::readers::InsetReaders::before(&self.arena, slot);
        self.arena.blocks.set_local(slot, insets);
        self.arena.readers.insets_changed();
        let readers = before.moved(&self.arena);
        self.restyle_readers(&readers);
        Ok(!readers.is_empty())
    }

    /// Set the session root's local insets (`None`: the safe-area insets,
    /// `env()`'s), which a node outside every owner reads: the insets of
    /// the area the document covers (main's model, which LLP 1123 replaces
    /// with D9's session band). A host whose document
    /// covers its view while `env()` is 0 sets them to the view's. Returns
    /// whether a layout is owed, as [`Kernel::set_env`] does.
    pub fn set_root_local_insets(&mut self, insets: Option<[f32; 4]>) -> Result<bool, KernelError> {
        let mut next = self.arena.env().clone();
        next.root_local = insets;
        if !next.is_finite() || insets.is_some_and(|l| l.iter().any(|v| *v < 0.0)) {
            return Err(LayoutError::InvalidEnv.into());
        }
        self.replace_env(next)
    }

    /// What `var(--exact-safe-area-inset-*)` reads at `view`, top right
    /// bottom left: its owner's local insets, or the session root's.
    pub fn local_insets(&self, view: ViewId) -> Option<[f32; 4]> {
        let slot = self.arena.slot_of(view)?;
        let env = env_at(&self.arena, slot);
        Some(Edge::ALL.map(|e| env.local_inset(e)))
    }

    /// The environment `view`'s lengths resolve against: the page's, with
    /// its owner's local insets (LLP 1123 D9). A host that resolves a row
    /// itself reads this one.
    pub fn env_of(&self, view: ViewId) -> Env {
        match self.arena.slot_of(view) {
            Some(slot) => env_at(&self.arena, slot).into_owned(),
            // A node gone (a leaving view's last style) reads the root's
            None => Env {
                local: Some(Edge::ALL.map(|e| self.arena.env().root_inset(e))),
                properties: Some(Default::default()),
                ..self.env()
            },
        }
    }

    pub(super) fn restyle_readers(&mut self, readers: &[u32]) {
        if readers.is_empty() {
            return;
        }
        if let Some(r) = &mut self.region {
            r.invalidate();
        }
        let mut unmirrored = crate::layout::Unmirrored;
        let layout = self.layout.as_deref_mut().unwrap_or(&mut unmirrored);
        let mut receipt = crate::CommitReceipt::default();
        for &slot in readers {
            rederive(&mut self.arena, layout, slot, true, &mut receipt);
        }
        // A replayed row checks its styles, not what they now resolve to
        if let Some(tree) = self.layout.as_deref_mut().and_then(|l| l.tree()) {
            tree.forget_memo();
        }
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "property_tests.rs"]
mod property_tests;
