//! Shared-element handoffs (LLP 1013.000 D3): a `sharedElement` name that
//! leaves one node and arrives at another in one commit.
//!
//! A name leaves with the node the commit destroys, directly or with an
//! ancestor; it arrives with a node the commit creates. A name that leaves
//! and arrives exactly once pairs. Two leavers or two arrivers pair nothing:
//! which would fly is not the kernel's guess. Names need not be unique in
//! the tree, since a pair is a change, not a lookup: two mounted routes can
//! hold the same name.

use crate::arena::NodeArena;
use crate::id::NodeKey;
use crate::PropId;
use exact_motion::{Property, Transition};
use std::collections::HashMap;

/// One name handed from a destroyed node to a created one.
#[derive(Debug, Clone, PartialEq)]
pub struct Handoff {
    /// The `sharedElement` name.
    pub name: String,
    /// The destroyed node; it resolves to nothing after the commit.
    pub from: NodeKey,
    /// The created node, live after the commit.
    pub to: NodeKey,
    /// The arriver's `-exact-layout-transition`, else the leaver's (D2): the curve
    /// the flight runs on. A handoff with neither is not reported.
    pub transition: Transition,
}

/// A node leaving with its name, recorded at its destroy.
#[derive(Debug)]
pub(crate) struct Leaver {
    name: String,
    key: NodeKey,
    transition: Option<Transition>,
}

/// The leaver a destroyed slot is, if it carries a name.
pub(crate) fn leaver(arena: &NodeArena, slot: u32) -> Option<Leaver> {
    let name = arena.props(slot).str(PropId::SharedElement)?;
    if name.is_empty() {
        return None;
    }
    Some(Leaver {
        name: name.to_owned(),
        key: arena.key(slot),
        transition: layout_curve(arena, slot),
    })
}

fn layout_curve(arena: &NodeArena, slot: u32) -> Option<Transition> {
    arena
        .style(slot)
        .rare
        .layout_transition
        .matching(Property::Layout)
        .filter(|t| t.starts())
        .cloned()
}

/// Pair the commit's leavers with the named nodes it created: each name
/// counted once on each side, so a commit that replaces many named nodes
/// pairs in linear time.
pub(crate) fn pair(arena: &NodeArena, created: &[NodeKey], leavers: Vec<Leaver>) -> Vec<Handoff> {
    if leavers.is_empty() {
        return Vec::new();
    }
    let mut left: HashMap<&str, (usize, &Leaver)> = HashMap::new();
    for leaver in &leavers {
        left.entry(leaver.name.as_str()).or_insert((0, leaver)).0 += 1;
    }
    let mut arrived: HashMap<&str, (usize, NodeKey, u32)> = HashMap::new();
    for key in created {
        let Some(slot) = arena.resolve(*key) else {
            continue;
        };
        let Some(name) = arena.props(slot).str(PropId::SharedElement) else {
            continue;
        };
        if left.contains_key(name) {
            arrived.entry(name).or_insert((0, *key, slot)).0 += 1;
        }
    }
    let mut out = Vec::new();
    for leaver in &leavers {
        let (Some(&(1, _)), Some(&(1, to, slot))) = (
            left.get(leaver.name.as_str()),
            arrived.get(leaver.name.as_str()),
        ) else {
            continue;
        };
        let Some(transition) = layout_curve(arena, slot).or_else(|| leaver.transition.clone())
        else {
            continue;
        };
        out.push(Handoff {
            name: leaver.name.clone(),
            from: leaver.key,
            to,
            transition,
        });
    }
    out
}

/// The names a route change pairs (LLP 1013.004 D1): each held by exactly
/// one rendered node in the leaving route's subtree and one in the
/// arriving route's, as `(from, to)` in the leaving route's tree order.
/// A subtree under `display: none` holds none.
pub fn pair_routes(
    kernel: &crate::Kernel,
    leaving: NodeKey,
    arriving: NodeKey,
) -> Vec<(NodeKey, NodeKey)> {
    if !kernel.has_prop(PropId::SharedElement) {
        return Vec::new();
    }
    let names = |route: NodeKey| {
        // By name: how many hold it, the first, and that one's tree order.
        let mut found: HashMap<&str, (usize, NodeKey, usize)> = HashMap::new();
        let mut stack: Vec<_> = kernel
            .node_by_key(route)
            .map(|n| n.id)
            .into_iter()
            .collect();
        while let Some(node) = stack.pop().and_then(|id| kernel.node(id)) {
            if node.style.display == crate::Display::None {
                continue;
            }
            if let Some(name) = node
                .props
                .str(PropId::SharedElement)
                .filter(|s| !s.is_empty())
            {
                let order = found.len();
                found.entry(name).or_insert((0, node.key, order)).0 += 1;
            }
            stack.extend(node.children().into_iter().rev());
        }
        found
    };
    let arrived = names(arriving);
    let mut pairs: Vec<_> = names(leaving)
        .into_iter()
        .filter_map(|(name, left)| match (left, arrived.get(name)) {
            ((1, from, order), Some(&(1, to, _))) => Some((order, from, to)),
            _ => None,
        })
        .collect();
    pairs.sort_unstable_by_key(|p| p.0);
    pairs.into_iter().map(|(_, from, to)| (from, to)).collect()
}
