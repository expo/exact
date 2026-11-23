//! Placement by `portal` (@ref LLP 1013.002 D1, D2, D3): an element declared
//! once is shown in the element its `portal` names by `id`, and the same live
//! node moves there. The instance tree stays the declaration tree; this pass
//! rewrites a batch's child lists so the kernel holds the shown tree, keeps
//! each moved node's declaration parent for event paths, and hides (detaches)
//! a mover whose destination is missing or cannot hold it.
//!
//! A batch is rewritten in three parts. First, each surviving mover whose
//! shown parent changes, and each moved mover that dies, is detached from
//! where it is shown, so no destroy takes it along and no later list makes a
//! cycle. Then the batch's own lists run without the movers. Last, every
//! list that differs from the final placement is set, adding movers to their
//! destinations; each intermediate tree is a part of the final one, so the
//! kernel's child, cycle and depth rules hold throughout. A moved mover whose
//! declaration died is destroyed by its own op, once.

use super::*;
use exact_kernel::{PropId, MAX_DEPTH};
use std::collections::{HashMap, HashSet};

/// What the last placement published.
#[derive(Debug, Clone, Default)]
pub(crate) struct Portals {
    /// Each mover not shown at its declaration: where it is shown, or `None`
    /// while it is hidden, and its declaration parent.
    moved: HashMap<ViewId, (Option<ViewId>, ViewId)>,
    /// The published child list of each parent a mover is shown in or
    /// declared in.
    lists: HashMap<ViewId, Vec<ViewId>>,
    /// Problems already journaled: one line per element and note.
    said: HashSet<(ViewId, String)>,
    /// The value each hidden mover was last journaled hiding it.
    hidden: HashMap<ViewId, String>,
    /// The surviving movers the last batch detached from where they were
    /// shown: a destroy in that batch takes none of their subtree.
    extracted: Vec<ViewId>,
}

/// One realized node of the final declaration tree, in preorder.
struct Entry {
    view: ViewId,
    /// The declaration parent; a list's row root names the list.
    parent: Option<usize>,
    /// Its top-level app root.
    root: usize,
    /// Levels between it and its parent in the kernel: 2 under a list's row
    /// wrapper, else 1.
    step: u32,
    id: Option<String>,
    /// A `portal` binding, and its value when nonempty.
    bears: bool,
    portal: Option<String>,
    /// Plain structure (D3): it may receive content, and it may move.
    receives: bool,
    movable: bool,
    /// It lays its children out as ordinary layout children (D3): a box or
    /// an svg's `foreignObject`, not a list, a table's or a prose list's
    /// part or a route's projected header.
    holds: bool,
    /// It is a list; it is a route.
    list: bool,
    keyed: bool,
}

struct Walk<'a> {
    plan: &'a Plan,
    sites: &'a SiteIndex,
    out: Vec<Entry>,
}

/// A prop's value as the kernel holds it: an optional binding's `none` is no
/// value.
fn bound(n: &NodeInst, plan: &Plan, sites: &SiteIndex, prop: PropId) -> Option<Value> {
    match n.bound_prop(plan, sites, prop)? {
        Value::Option(None) => None,
        Value::Option(Some(v)) => Some((*v).clone()),
        v => Some(v),
    }
}

fn said(n: &NodeInst, plan: &Plan, sites: &SiteIndex, prop: PropId) -> Option<String> {
    let v = bound(n, plan, sites, prop)?;
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

impl Walk<'_> {
    fn children(
        &mut self,
        children: &[Child],
        parent: Option<usize>,
        root: Option<usize>,
        step: u32,
    ) {
        for c in children {
            match c {
                Child::Node(n) => self.node(n, parent, root, step),
                Child::Region(r) => match &r.active {
                    Active::Arm { roots, .. } => self.children(roots, parent, root, step),
                    Active::Rows { rows } => {
                        for row in rows {
                            self.children(&row.roots, parent, root, step);
                        }
                    }
                },
            }
        }
    }

    fn node(&mut self, n: &NodeInst, parent: Option<usize>, root: Option<usize>, step: u32) {
        let i = self.out.len();
        let (plan, sites) = (self.plan, self.sites);
        let said = |prop| said(n, plan, sites, prop);
        let own = plan.node(n.node).node_type;
        let view_type = NodeType::View as u8;
        let tag = said(PropId::SemanticTag);
        let role = said(PropId::AccessibilityRole);
        let keyed = said(PropId::NavigationKey).is_some();
        // HTML: an empty `popover` is the `auto` state.
        let popover = bound(n, plan, sites, PropId::Popover).is_some();
        let up = parent.map(|p| &self.out[p]);
        // A list's cell is in a wrapper (`step` 2) or a grouped list's section.
        let plain_parent = step == 1 && up.is_some_and(|p| p.holds);
        let part = tag.as_deref().is_some_and(|t| is_part(t) || t == "dialog");
        // A route's header projects its children into the bar, a tablist its
        // tabs: they are slots. A dialog's and a panel's content is laid out.
        let bar = tag.as_deref() == Some("header") && up.is_some_and(|p| p.keyed);
        let projected = bar
            || matches!(
                role.as_deref(),
                Some("dialog" | "alertdialog" | "tab" | "tabpanel" | "tablist")
            );
        let bears = (plan.node(n.node).bindings.iter()).any(|b| {
            let b = plan.binding(b);
            b.kind == BindingKind::Prop && b.id == PropId::Portal as u16
        });
        let receives = own == view_type
            && matches!(
                tag.as_deref(),
                None | Some("main" | "section" | "article" | "aside" | "nav" | "footer")
            )
            && role.is_none()
            && !keyed
            && !popover
            && (parent.is_none() || plain_parent);
        let cells = own == NodeType::List as u8
            || (tag.as_deref() == Some("section") && up.is_some_and(|p| p.list));
        self.out.push(Entry {
            view: n.view,
            parent,
            root: root.unwrap_or(i),
            step,
            id: said(PropId::Id),
            bears,
            portal: bears.then(|| said(PropId::Portal)).flatten(),
            receives,
            movable: plain_parent && !keyed && !popover && !part && !projected,
            holds: (own == view_type || own == NodeType::SvgForeignObject as u8)
                && !cells
                && !bar
                && role.as_deref() != Some("tablist")
                && !tag.as_deref().is_some_and(holds_parts),
            list: own == NodeType::List as u8,
            keyed,
        });
        let root = Some(root.unwrap_or(i));
        let Some(list) = n.list() else {
            self.children(&n.children, Some(i), root, 1);
            return;
        };
        // A list's fixed children before its rows, the rows, then the rest.
        let split = list
            .fixed_split()
            .unwrap_or(n.children.len())
            .min(n.children.len());
        self.children(&n.children[..split], Some(i), root, 1);
        list.visit_rows(&mut |_, row| self.children(&row.roots, Some(i), root, 2));
        self.children(&n.children[split..], Some(i), root, 1);
    }
}

/// HTML's table, description list and prose list containers, whose children
/// are their parts.
fn holds_parts(tag: &str) -> bool {
    matches!(
        tag,
        "table" | "thead" | "tbody" | "tfoot" | "tr" | "dl" | "ul" | "ol"
    )
}

/// The parts, which only their container holds.
fn is_part(tag: &str) -> bool {
    matches!(
        tag,
        "caption" | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td" | "dt" | "dd" | "li"
    )
}

/// Why a mover is hidden, journaled once per element and value.
enum Hidden {
    Missing,
    Inside,
    Refuses,
    Cycle,
    Deep,
    Unreachable,
}

impl Hidden {
    fn why(&self) -> &'static str {
        match self {
            Hidden::Missing => "no element in its app root has that id",
            Hidden::Inside => "that element is this one or inside it",
            Hidden::Refuses => "that element cannot receive content (a plain box with no role, navigationKey or popover, not a slot of its parent, can)",
            Hidden::Cycle => "the placements make a cycle",
            Hidden::Deep => "the placement would nest past the kernel's depth limit",
            Hidden::Unreachable => "that element is itself hidden",
        }
    }
}

impl Tree {
    /// Whether the plan says `portal` anywhere: only then does placement run.
    pub(super) fn portals_for(plan: &Plan) -> Option<Box<Portals>> {
        crate::uses::portals(plan).then(Box::default)
    }

    /// The movers the last placement detached and kept alive: a destroy in
    /// the same batch does not reach their subtrees.
    pub(crate) fn extracted(&self) -> &[ViewId] {
        self.portals.as_ref().map_or(&[], |p| &p.extracted)
    }

    /// The declaration parent of a moved or hidden view; `None` for one shown
    /// where it is declared, whose kernel parent is its declaration parent.
    pub(crate) fn declared_parent(&self, view: ViewId) -> Option<ViewId> {
        self.portals
            .as_ref()?
            .moved
            .get(&view)
            .map(|(_, parent)| *parent)
    }

    /// Rewrite `ops`, a batch the instance tree made, to publish the final
    /// placement; journal each hidden mover once in `notes`.
    pub(crate) fn place(
        &mut self,
        plan: &Plan,
        sites: &SiteIndex,
        ops: &mut Vec<Op>,
        notes: &mut Vec<String>,
    ) {
        let Some(mut portals) = self.portals.take() else {
            return;
        };
        let structural = ops.iter().any(|op| match op {
            Op::CreateView { .. } | Op::DestroyView { .. } | Op::SetChildren { .. } => true,
            Op::SetProp { prop, .. } | Op::ClearProp { prop, .. } => matches!(
                prop,
                PropId::Portal
                    | PropId::Id
                    | PropId::AccessibilityRole
                    | PropId::NavigationKey
                    | PropId::Popover
                    | PropId::SemanticTag
            ),
            _ => false,
        });
        if structural {
            let mut walk = Walk {
                plan,
                sites,
                out: Vec::new(),
            };
            walk.children(&self.children, None, None, 1);
            portals.publish(&walk.out, ops, notes);
        }
        self.portals = Some(portals);
    }
}

impl Portals {
    fn publish(&mut self, entries: &[Entry], ops: &mut Vec<Op>, notes: &mut Vec<String>) {
        let index: HashMap<ViewId, usize> = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.view, i))
            .collect();
        // D2.1: the first element in declaration preorder with the id, in
        // the mover's app root.
        let named: HashSet<(usize, &str)> = (entries.iter())
            .filter_map(|e| Some((e.root, e.portal.as_deref()?)))
            .collect();
        let mut first: HashMap<(usize, &str), usize> = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            if let Some(id) = &e.id {
                if !named.contains(&(e.root, id.as_str())) {
                    continue;
                }
                if first.contains_key(&(e.root, id.as_str())) {
                    let note = format!("id \"{id}\" names more than one element; `portal` takes the first in declaration order");
                    if self.said.insert((entries[e.root].view, note.clone())) {
                        notes.push(note);
                    }
                } else {
                    first.insert((e.root, id), i);
                }
            }
        }
        let inside = |i: usize, of: usize| {
            let mut at = Some(i);
            while let Some(a) = at {
                if a == of {
                    return true;
                }
                at = entries[a].parent;
            }
            false
        };
        // D2.2: each mover's edge, or the reason it is hidden.
        let mut edge: HashMap<usize, Result<usize, Hidden>> = HashMap::new();
        for (i, e) in entries.iter().enumerate() {
            let Some(value) = &e.portal else { continue };
            if !e.movable {
                let note = format!("view {}: `portal` is ignored: this element is a slot of its parent, a route, a dialog or a popover (LLP 1013.002 D3)", e.view);
                if self.said.insert((e.view, note.clone())) {
                    notes.push(note);
                }
                continue;
            }
            edge.insert(
                i,
                match first.get(&(e.root, value.as_str())) {
                    None => Err(Hidden::Missing),
                    Some(&d) if inside(d, i) => Err(Hidden::Inside),
                    Some(&d) if !entries[d].receives => Err(Hidden::Refuses),
                    Some(&d) => Ok(d),
                },
            );
        }
        // The shown parent of each entry under the edges kept so far.
        let up =
            |i: usize, edge: &HashMap<usize, Result<usize, Hidden>>| -> Option<Option<usize>> {
                match edge.get(&i) {
                    Some(Ok(d)) => Some(Some(*d)),
                    Some(Err(_)) => None,
                    None => Some(entries[i].parent),
                }
            };
        // Drop every portal edge on a cycle, then the deepest edge on each
        // path past the depth limit, then each edge into hidden content,
        // until none changes.
        loop {
            let mut dropped: Vec<(usize, Hidden)> = Vec::new();
            let mut on_cycle: HashSet<usize> = HashSet::new();
            for (&m, kept) in &edge {
                let Ok(d) = kept else { continue };
                if on_cycle.contains(&m) {
                    continue;
                }
                let mut path = vec![m];
                let mut at = Some(*d);
                let mut steps = 0;
                while let Some(a) = at {
                    if a == m {
                        for p in path.iter().filter(|p| matches!(edge.get(p), Some(Ok(_)))) {
                            if on_cycle.insert(*p) {
                                dropped.push((*p, Hidden::Cycle));
                            }
                        }
                        break;
                    }
                    steps += 1;
                    if steps > entries.len() {
                        break;
                    }
                    path.push(a);
                    at = up(a, &edge).flatten();
                }
            }
            if dropped.is_empty() {
                // D2.3: a destination inside hidden content cannot receive.
                for (&m, kept) in &edge {
                    let Ok(d) = kept else { continue };
                    let mut at = *d;
                    let reached = loop {
                        match up(at, &edge) {
                            None => break false,
                            Some(None) => break true,
                            Some(Some(p)) => at = p,
                        }
                    };
                    if !reached {
                        dropped.push((m, Hidden::Unreachable));
                    }
                }
            }
            if dropped.is_empty() {
                let mut depth: HashMap<usize, Option<u32>> = HashMap::new();
                for i in 0..entries.len() {
                    if depth_of(i, &edge, entries, &up, &mut depth).is_some_and(|d| d > MAX_DEPTH) {
                        // The deepest kept portal edge on its path.
                        let mut at = Some(i);
                        while let Some(a) = at {
                            if matches!(edge.get(&a), Some(Ok(_))) {
                                dropped.push((a, Hidden::Deep));
                                break;
                            }
                            at = up(a, &edge).flatten();
                        }
                        if !dropped.is_empty() {
                            break;
                        }
                    }
                }
            }
            if dropped.is_empty() {
                break;
            }
            for (m, why) in dropped {
                edge.insert(m, Err(why));
            }
        }
        // In declaration order, so the journal reads the same on every run.
        for (m, e) in entries.iter().enumerate() {
            if let Some(Err(why)) = edge.get(&m) {
                let value = e.portal.clone().unwrap_or_default();
                if self.hidden.insert(e.view, value.clone()).as_ref() != Some(&value) {
                    notes.push(format!(
                        "view {}: `portal=\"{value}\"` hides it: {} (LLP 1013.002 D2)",
                        e.view,
                        why.why()
                    ));
                }
            }
        }
        self.apply(entries, &index, &edge, ops);
    }

    /// Rewrite `ops` to the placement `edge` decides, and remember it.
    fn apply(
        &mut self,
        entries: &[Entry],
        index: &HashMap<ViewId, usize>,
        edge: &HashMap<usize, Result<usize, Hidden>>,
        ops: &mut Vec<Op>,
    ) {
        let view = |i: usize| entries[i].view;
        let moved: HashMap<ViewId, (Option<ViewId>, ViewId)> = (edge.iter())
            .filter_map(|(&m, kept)| {
                let parent = view(entries[m].parent?);
                Some((view(m), (kept.as_ref().ok().map(|d| view(*d)), parent)))
            })
            .collect();
        // The final list of each parent placement touches: its declared
        // children without the movers, then the movers it receives in
        // declaration preorder.
        // A bearer's parent's list is placement's only where that parent lays
        // it out: a list's or a group's cells are the collection's.
        let placeable =
            |e: &Entry| e.bears && e.step == 1 && e.parent.is_some_and(|p| entries[p].holds);
        let mut finals: HashMap<ViewId, Vec<ViewId>> = HashMap::new();
        let mut touched: Vec<ViewId> = Vec::new();
        for e in entries.iter().filter(|e| placeable(e)) {
            touched.extend(e.parent.map(view));
        }
        let kept: HashSet<ViewId> = touched.iter().copied().collect();
        let mut incoming: Vec<(usize, usize)> = (edge.iter())
            .filter_map(|(&m, kept)| kept.as_ref().ok().map(|d| (*d, m)))
            .collect();
        incoming.sort_unstable_by_key(|&(_, m)| m);
        touched.extend(incoming.iter().map(|&(d, _)| view(d)));
        touched.extend(self.lists.keys().filter(|v| index.contains_key(v)));
        let kept: HashSet<ViewId> = kept
            .into_iter()
            .chain(incoming.iter().map(|&(d, _)| view(d)))
            .collect();
        touched.sort_unstable();
        touched.dedup();
        // Each touched parent's declared children, in one pass.
        let mut declared: HashMap<usize, Vec<ViewId>> =
            touched.iter().map(|x| (index[x], Vec::new())).collect();
        for e in entries.iter().filter(|e| e.step == 1) {
            if let Some(list) = e.parent.and_then(|p| declared.get_mut(&p)) {
                list.push(e.view);
            }
        }
        let mut arriving: HashMap<usize, Vec<ViewId>> = HashMap::new();
        for &(d, m) in &incoming {
            arriving.entry(d).or_default().push(view(m));
        }
        let authored: HashMap<ViewId, Vec<ViewId>> = declared
            .iter()
            .map(|(&i, list)| (view(i), list.clone()))
            .collect();
        for (&i, list) in &mut declared {
            list.retain(|c| !moved.contains_key(c));
            list.extend(arriving.remove(&i).unwrap_or_default());
            finals.insert(view(i), std::mem::take(list));
        }
        let created: HashSet<ViewId> = (ops.iter())
            .filter_map(|op| match op {
                Op::CreateView { id, .. } => Some(*id),
                _ => None,
            })
            .collect();
        let destroyed: HashSet<ViewId> = (ops.iter())
            .filter_map(|op| match op {
                Op::DestroyView { id } => Some(*id),
                _ => None,
            })
            .collect();
        // Where each mover is shown now, before this batch.
        let shown_before = |m: ViewId| -> Option<Option<ViewId>> {
            if created.contains(&m) {
                return None;
            }
            match self.moved.get(&m) {
                Some((at, _)) => Some(*at),
                None => index
                    .get(&m)
                    .and_then(|&i| entries[i].parent)
                    .map(|p| Some(view(p))),
            }
        };
        // 1. Detach each surviving mover that moves, and each moved mover
        // whose declaration died, from where it is shown.
        let mut leaving: HashMap<ViewId, Vec<ViewId>> = HashMap::new();
        for e in entries.iter().filter(|e| placeable(e)) {
            let now = match moved.get(&e.view) {
                Some((at, _)) => *at,
                None => e.parent.map(view),
            };
            if let Some(Some(was)) = shown_before(e.view) {
                if Some(was) != now {
                    leaving.entry(was).or_default().push(e.view);
                }
            }
        }
        self.extracted = leaving.values().flatten().copied().collect();
        let mut dying = Vec::new();
        for (&m, (at, _)) in &self.moved {
            if index.contains_key(&m) {
                continue;
            }
            dying.push(m);
            if let Some(was) = at {
                leaving.entry(*was).or_default().push(m);
            }
        }
        let mut detach = Vec::new();
        let mut state: HashMap<ViewId, Vec<ViewId>> = HashMap::new();
        for (was, gone) in leaving {
            let Some(list) = self.lists.get(&was) else {
                continue;
            };
            let gone: HashSet<ViewId> = gone.into_iter().collect();
            let remaining: Vec<ViewId> =
                list.iter().copied().filter(|c| !gone.contains(c)).collect();
            state.insert(was, remaining.clone());
            detach.push(Op::SetChildren {
                id: was,
                children: remaining,
            });
        }
        // 2. The batch's own lists, without the movers placement holds or
        // destroys: a list of a parent the batch later destroys too, so that
        // destroy does not take a mover along.
        let gone: HashSet<ViewId> = dying.iter().copied().collect();
        for op in ops.iter_mut() {
            if let Op::SetChildren { id, children } = op {
                children.retain(|c| !moved.contains_key(c) && !gone.contains(c));
                state.insert(*id, children.clone());
            }
        }
        // 3. Every list that differs from the final placement.
        let mut attach = Vec::new();
        let mut touched: Vec<&ViewId> = finals.keys().collect();
        touched.sort_unstable();
        for x in touched {
            let list = &finals[x];
            // A list placement never set, and the batch left, is the declared one.
            let before = (state.get(x).or_else(|| self.lists.get(x))).or_else(|| authored.get(x));
            if before != Some(list) {
                attach.push(Op::SetChildren {
                    id: *x,
                    children: list.clone(),
                });
            }
        }
        if !detach.is_empty() {
            ops.splice(0..0, detach);
        }
        ops.extend(attach);
        ops.extend(
            dying
                .into_iter()
                .filter(|m| !destroyed.contains(m))
                .map(|id| Op::DestroyView { id }),
        );
        self.moved = moved;
        finals.retain(|x, _| kept.contains(x));
        self.lists = finals;
        self.said.retain(|(v, _)| index.contains_key(v));
        // A mover shown again journals its next hiding afresh.
        (self.hidden).retain(|v, _| {
            index
                .get(v)
                .is_some_and(|i| matches!(edge.get(i), Some(Err(_))))
        });
    }
}

/// An entry's depth below its app root under `edge`, memoized; `None` for
/// one that is hidden. Iterative: a chain of placements may be long.
fn depth_of(
    i: usize,
    edge: &HashMap<usize, Result<usize, Hidden>>,
    entries: &[Entry],
    up: &dyn Fn(usize, &HashMap<usize, Result<usize, Hidden>>) -> Option<Option<usize>>,
    memo: &mut HashMap<usize, Option<u32>>,
) -> Option<u32> {
    let mut path = Vec::new();
    let mut at = i;
    let mut base = loop {
        if let Some(d) = memo.get(&at) {
            break *d;
        }
        if path.len() > entries.len() {
            break None;
        }
        path.push(at);
        match up(at, edge) {
            None => break None,
            Some(None) => break Some(0),
            Some(Some(p)) => at = p,
        }
    };
    // `base` is the depth of the first entry above the path, or of its top.
    let top_is_root = matches!(path.last().map(|&t| up(t, edge)), Some(Some(None)));
    for (k, &e) in path.iter().enumerate().rev() {
        let step = |e: usize| {
            if matches!(edge.get(&e), Some(Ok(_))) {
                1
            } else {
                entries[e].step
            }
        };
        let d = if k == path.len() - 1 && top_is_root {
            Some(0)
        } else {
            base.map(|b| b + step(e))
        };
        memo.insert(e, d);
        base = d;
    }
    memo.get(&i).copied().flatten()
}
