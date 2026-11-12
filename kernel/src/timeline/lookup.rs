//! Named timeline lookup (LLP 1057.003 D4, D5; Scroll-driven Animations 1 §4.2).
//!
//! A named timeline is referenceable by the node that declares it and its
//! descendants; `timeline-scope: <name>` on a node declares the name in
//! scope for its subtree, attached to the one descendant that declares it
//! and is not already captured by a nearer `timeline-scope`, or else to an
//! inactive timeline; `all` does that for every name below it. So a
//! consumer's lookup walks up from itself, and the first node that declares
//! the name or scopes it decides; nothing found is no timeline. Chrome 154
//! agrees case by case (the phase 3 probe), `all` aside: it parses none.
//!
//! The kernel keeps a registry of the few nodes that bear a timeline row
//! (sources, consumers, scopes), kept from each commit's receipt, and after
//! a commit re-resolves every consumer; the receipt names those whose
//! answer changed, and `motion_sync` sends them (`MotionSync::timelines`).
//! A name that finds a scroll or view timeline is bound once layout has
//! placed it (`place.rs`); until then its consumer keeps the last placement.

use super::place::{Binding, TimelineScroller};
use super::{Axis, TimelineScope, LINKED};
use crate::arena::NodeArena;
use crate::generated::StyleProps;
use crate::id::NodeKey;
use crate::motion::{motion_node, TimelineRows};
use crate::sorted::SlotSet;
use crate::txn::CommitReceipt;
use exact_motion::NamedTimeline;

/// What a consumer's `animation-timeline` resolved to at the last commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// A drag timeline: its source's engine node.
    Drag(u64),
    /// The scroll progress timeline this node declares.
    Scroll(NodeKey),
    /// The view progress timeline whose subject is this node.
    View(NodeKey),
    /// `scroll()` or `view()`: the consumer's own, which layout places.
    Anonymous,
    /// An inactive timeline: a scope with no declaration below it, or several.
    Inactive,
    /// No timeline of that name in scope.
    Missing,
}

/// The linked lookup's signature: [`resolve`].
pub(super) type Resolve = fn(&mut NodeArena, &CommitReceipt) -> Vec<NodeKey>;

/// The timeline-bearing nodes: each slot whose style declares a timeline,
/// scopes a name or follows one, and what each follower's timeline
/// resolved to at the last commit; and, from the last layout, each scroll
/// or view timeline's binding and the scrollers they read. A page has a
/// handful.
#[derive(Debug, Clone, Default)]
pub(crate) struct Registry {
    slots: SlotSet,
    pub(super) resolved: Vec<(NodeKey, Found)>,
    pub(super) placed: Vec<(NodeKey, Binding)>,
    pub(super) scrollers: Vec<TimelineScroller>,
}

impl Registry {
    fn resolved(&self, consumer: NodeKey) -> Option<Found> {
        self.resolved
            .iter()
            .find(|(k, _)| *k == consumer)
            .map(|(_, t)| *t)
    }

    pub(super) fn placed(&self, consumer: NodeKey) -> Option<Binding> {
        self.placed
            .iter()
            .find(|(k, _)| *k == consumer)
            .map(|(_, b)| *b)
    }
}

impl crate::kernel::Kernel {
    /// What `consumer`'s `animation-timeline` resolved to at the last
    /// commit (LLP 1057.003 D4); `None` for a node that follows no
    /// timeline, or while the lookup is unlinked.
    pub fn timeline_found(&self, consumer: NodeKey) -> Option<Found> {
        self.arena().timelines.resolved(consumer)
    }

    /// The timeline `consumer`'s animations follow, as the engine binds
    /// them: a drag's source, a scroller's offset once layout has placed a
    /// scroll or view timeline (inactive before), or an inactive or
    /// missing one; `None` as for [`Self::timeline_found`].
    pub fn timeline_of(&self, consumer: NodeKey) -> Option<NamedTimeline> {
        let timelines = &self.arena().timelines;
        Some(match timelines.resolved(consumer)? {
            Found::Drag(n) => NamedTimeline::Source(n),
            Found::Inactive => NamedTimeline::Inactive,
            Found::Missing => NamedTimeline::Missing,
            Found::Scroll(_) | Found::View(_) | Found::Anonymous => timelines
                .placed(consumer)
                .map_or(NamedTimeline::Inactive, |(t, _)| t),
        })
    }

    /// Whether any node bears a timeline row: a source, a scope or a
    /// consumer. A host whose page seeks consumers itself (the web's glue)
    /// follows every commit while one does.
    pub fn has_timelines(&self) -> bool {
        !self.arena().timelines.slots.is_empty()
    }
}

/// After a commit, the consumers whose timeline now resolves differently,
/// leaving out any the commit created or touched (their rows reach the
/// engine anyway). Nothing while unlinked.
pub(crate) fn refresh(arena: &mut NodeArena, receipt: &CommitReceipt) -> Vec<NodeKey> {
    match LINKED.get() {
        Some(grammar) => (grammar.resolve)(arena, receipt),
        None => Vec::new(),
    }
}

fn bears(s: &StyleProps) -> bool {
    s.rare.drag_timeline.name.is_some()
        || s.rare.scroll_timeline_name.0.is_some()
        || s.rare.view_timeline_name.0.is_some()
        || follows(s)
        || s.rare.timeline_scope != TimelineScope::None
}

/// Whether `s`'s animations follow a named or an anonymous timeline.
fn follows(s: &StyleProps) -> bool {
    s.rare.animation_timeline.name().is_some() || s.rare.animation_timeline.anonymous().is_some()
}

/// [`refresh`], linked: keep the registry, then re-resolve every consumer.
/// Any change can move a scope (a node inserted, removed or moved, a row
/// set), so any commit that finds the registry in use re-resolves; each
/// lookup walks one consumer's ancestors and, at a scope, the registry's
/// declarations, never the tree.
pub(super) fn resolve(arena: &mut NodeArena, receipt: &CommitReceipt) -> Vec<NodeKey> {
    for key in &receipt.destroyed {
        arena.timelines.slots.remove(key.index);
    }
    for key in receipt.created.iter().chain(&receipt.touched) {
        let Some(slot) = arena.resolve(*key) else {
            continue;
        };
        if bears(arena.style(slot)) {
            arena.timelines.slots.insert(slot);
        } else {
            arena.timelines.slots.remove(slot);
        }
    }
    if arena.timelines.slots.is_empty() && arena.timelines.resolved.is_empty() {
        return Vec::new();
    }
    let at = &*arena;
    let resolved: Vec<(NodeKey, Found)> = at
        .timelines
        .slots
        .iter()
        .filter_map(|slot| {
            let timeline = &at.style(slot).rare.animation_timeline;
            let found = match timeline.name() {
                Some(name) => lookup(at, slot, name),
                None => {
                    timeline.anonymous()?;
                    Found::Anonymous
                }
            };
            Some((at.key(slot), found))
        })
        .collect();
    let changed = resolved
        .iter()
        .filter(|(key, timeline)| {
            at.timelines.resolved(*key) != Some(*timeline)
                && !receipt.created.contains(key)
                && receipt.touched.binary_search(key).is_err()
        })
        .map(|(key, _)| *key)
        .collect();
    arena.timelines.resolved = resolved;
    let live = &arena.timelines.resolved;
    let placed = std::mem::take(&mut arena.timelines.placed);
    arena.timelines.placed = (placed.into_iter())
        .filter(|(key, _)| live.iter().any(|(k, _)| k == key))
        .collect();
    changed
}

/// What `name` resolves to for the consumer at `slot`: walking up from it,
/// the first node that declares the name (itself first) or scopes it
/// (`timeline-scope`) decides.
fn lookup(arena: &NodeArena, consumer: u32, name: &str) -> Found {
    let slots = &arena.timelines.slots;
    let declares = |slot: u32| declared(arena, slot, name).is_some();
    let mut at = Some(consumer);
    while let Some(slot) = at {
        if slots.contains(slot) {
            if let Some(found) = declared(arena, slot, name) {
                return found;
            }
            let scope = &arena.style(slot).rare.timeline_scope;
            if scope.scopes(name) {
                // What this scope captures: the declarations whose nearest
                // scope of the name, themselves included, is this one. One
                // is the timeline; none or several are an inactive one, but
                // `all` declares only the names below it.
                let mut captured = slots
                    .iter()
                    .filter(|&s| declares(s) && scope_of(arena, s, name) == Some(slot));
                match (captured.next(), captured.next()) {
                    (Some(source), None) => {
                        return declared(arena, source, name).unwrap_or(Found::Inactive)
                    }
                    (None, _) if *scope == TimelineScope::All => {}
                    _ => return Found::Inactive,
                }
            }
        }
        at = arena.parent(slot);
    }
    Found::Missing
}

/// The timeline `slot` declares as `name`. On one node a scroll timeline
/// comes before a view timeline, as CSS orders them (Scroll-driven
/// Animations 1 §4.2), and a drag timeline, Exact's, after both.
fn declared(arena: &NodeArena, slot: u32, name: &str) -> Option<Found> {
    let rare = &arena.style(slot).rare;
    let named = |n: &Option<String>| n.as_deref() == Some(name);
    let key = arena.key(slot);
    if named(&rare.scroll_timeline_name.0) {
        Some(Found::Scroll(key))
    } else if named(&rare.view_timeline_name.0) {
        Some(Found::View(key))
    } else if named(&rare.drag_timeline.name) {
        Some(Found::Drag(motion_node(key)))
    } else {
        None
    }
}

/// The nearest node at or above `slot` whose `timeline-scope` names `name`.
fn scope_of(arena: &NodeArena, slot: u32, name: &str) -> Option<u32> {
    let mut at = Some(slot);
    while let Some(s) = at {
        if arena.timelines.slots.contains(s) && arena.style(s).rare.timeline_scope.scopes(name) {
            return Some(s);
        }
        at = arena.parent(s);
    }
    None
}

/// One node's timeline rows as the engine takes them: whether the timeline
/// it drives reads `x`, and what its animations follow over their range.
/// A drag's consumer takes two lengths, or stays on the clock; a scroll or
/// view timeline's takes the binding layout last placed for it, inactive
/// until the first.
pub(crate) fn rows(arena: &NodeArena, slot: u32) -> TimelineRows {
    let key = arena.key(slot);
    let style = arena.style(slot);
    let source = style
        .rare
        .drag_timeline
        .name
        .as_ref()
        .map(|_| style.rare.drag_timeline.axis == Axis::X);
    let lengths = || style.animation_range.lengths().map(|r| r.map(f64::from));
    let whole = [0.0, 1.0];
    let binding = match arena.timelines.resolved(key) {
        _ if !follows(style) => None,
        // `normal`, a percentage or a name: no length range to map a drag onto.
        Some(Found::Drag(source)) => lengths().map(|r| (NamedTimeline::Source(source), r)),
        Some(Found::Inactive) => Some((NamedTimeline::Inactive, lengths().unwrap_or(whole))),
        Some(Found::Missing) | None => Some((NamedTimeline::Missing, lengths().unwrap_or(whole))),
        Some(Found::Scroll(_) | Found::View(_) | Found::Anonymous) => Some(
            arena
                .timelines
                .placed(key)
                .unwrap_or((NamedTimeline::Inactive, whole)),
        ),
    };
    (motion_node(key), source, binding)
}
