//! A navigation tab built when it is first selected (LLP 1075.003 §3.7).
//!
//! A tab panel ([`panel_sites`]) is made empty. Its contents — their views,
//! rows and the state their components declare — are made in the update
//! whose tree first selects it, as `UITabBarController` loads a tab's view
//! controller when the tab is first selected and SwiftUI's `TabView` builds
//! a tab's content when it is first shown. Once made they stay, as a visited
//! tab's stack does. The inliner makes a panel's contents one `if true` arm
//! (`contract_syntax::inline`), which owns the state of a component used
//! directly in it, so that state too starts at the first selection.
//!
//! Which panel is selected is what every host's projection reads
//! (`head.rs` `unselected`): the panel whose routes hold the navigation
//! root's `navigationKey`. While no made panel holds it, the panel whose tab
//! is `aria-selected` is made, else the first waiting one in tab order,
//! until one does or none waits; a root that names no route of any panel
//! shows all of them, as the projection does.

use super::*;
use crate::head::{tab_panels, HeadNode};
use exact_kernel::{PropId, PropList, PropValue};
use std::collections::HashMap;

/// The props the selection reads.
const READ: [PropId; 6] = [
    PropId::AccessibilityRole,
    PropId::Id,
    PropId::NavigationKey,
    PropId::NavigationBack,
    PropId::AccessibilityControls,
    PropId::AccessibilitySelected,
];

/// The plan's tab panels, sorted: each node whose `role` is the literal
/// `"tabpanel"` and whose nearest ancestor with a `navigationKey` binding is
/// a navigation root (it binds `navigationBack` too), not a route — the
/// panels a root's tablist names (`head.rs` `tab_panels`) when the app
/// writes them as the recipe does. The web JS target defers the same sites,
/// and the inliner makes their contents one arm (`contract_syntax::inline`),
/// but for a virtualized list's, whose one `for` owns its rows' state.
pub fn panel_sites(plan: &Plan) -> Vec<NodesId> {
    (0..plan.nodes.len() as u32)
        .map(NodesId)
        .filter(|&n| literal_str(plan, n, PropId::AccessibilityRole) == Some("tabpanel"))
        .filter(|&n| {
            let mut at = enclosing(plan, n);
            while let Some((a, _)) = at {
                if binds(plan, a, PropId::NavigationKey) {
                    return binds(plan, a, PropId::NavigationBack);
                }
                at = enclosing(plan, a);
            }
            false
        })
        .collect()
}

/// Whether each of `sites`, the plan's tab panels ([`panel_sites`]), needs a
/// navigation controller: its panel can hold more than one route, or its
/// route can show a header, which the platform shows as its navigation bar
/// (LLP 1115.000 D5.2). A panel holds more than one route when a route of
/// its own (a node with a `navigationKey` and no route between it and the
/// panel) stands inside a `for` below it, as `for e in t.stack` draws one
/// per entry, or when its template holds more than one route. A route can
/// show a header when a `header` is its child, shown or not. A panel with
/// one route outside any `for` and no `header` shows one fixed route with
/// no bar, which a host shows without a navigation controller (LLP 1075.003
/// §3.7). It is read from the plan, so it holds for a panel still waiting
/// for its first selection. One template serves every tab a `for t in
/// nav.tabs` draws, so a template that needs one for some tabs answers
/// `true` for all of them: a navigation controller can show one route with
/// no bar, and a plain one shows no bar and no pushed route.
pub fn sites_need_navigation(plan: &Plan, sites: &[NodesId]) -> Vec<bool> {
    let mut headed: Vec<NodesId> = (0..plan.nodes.len() as u32)
        .map(NodesId)
        .filter(|&n| literal_str(plan, n, PropId::SemanticTag) == Some("header"))
        .filter_map(|n| enclosing(plan, n).map(|(parent, _)| parent))
        .filter(|&parent| binds(plan, parent, PropId::NavigationKey))
        .collect();
    headed.sort_unstable();
    headed.dedup();
    let mut routes = vec![0usize; sites.len()];
    let mut navigation = vec![false; sites.len()];
    for route in (0..plan.nodes.len() as u32).map(NodesId) {
        if !binds(plan, route, PropId::NavigationKey) {
            continue;
        }
        let (mut at, mut each) = (enclosing(plan, route), false);
        while let Some((a, crossed)) = at {
            each |= crossed;
            if let Ok(i) = sites.binary_search(&a) {
                routes[i] += 1;
                navigation[i] |= each || routes[i] > 1 || headed.binary_search(&route).is_ok();
                break;
            }
            if binds(plan, a, PropId::NavigationKey) {
                break;
            }
            at = enclosing(plan, a);
        }
    }
    navigation
}

/// The string node `n`'s `prop` is bound to, when that binding is a literal.
fn literal_str(plan: &Plan, n: NodesId, prop: PropId) -> Option<&str> {
    plan.node(n).bindings.iter().find_map(|b| {
        let b = plan.binding(b);
        if b.kind != BindingKind::Prop || b.id != prop as u16 {
            return None;
        }
        match crate::vm::instructions(plan.read_code(plan.code(b.expr)).0)
            .collect::<Result<Vec<_>, _>>()
            .as_deref()
        {
            Ok([v, r]) if r.op == exact_plan::Opcode::Return && v.op == exact_plan::Opcode::Str => {
                Some(plan.str(exact_plan::StrId(v.args[0] as u32)))
            }
            _ => None,
        }
    })
}

/// Whether node `n` binds `prop`.
fn binds(plan: &Plan, n: NodesId, prop: PropId) -> bool {
    plan.node(n)
        .bindings
        .iter()
        .any(|b| plan.binding(b).kind == BindingKind::Prop && plan.binding(b).id == prop as u16)
}

/// A site's enclosing node, its parent, else its arm's region's, and
/// whether a `for` lies between them.
fn enclosing(plan: &Plan, n: NodesId) -> Option<(NodesId, bool)> {
    let node = plan.node(n);
    if let Some(parent) = node.parent {
        return Some((parent, false));
    }
    let (mut arm, mut each) = (node.arm, false);
    while let Some(a) = arm {
        let region = plan.region(plan.arm(a).region);
        each |= region.kind == RegionKind::Each;
        if let Some(parent) = region.parent {
            return Some((parent, each));
        }
        arm = region.arm;
    }
    None
}

/// The instance tree as the selection reads it: each node's [`READ`] props
/// as its bindings last left them, its children as its kernel view has them,
/// and its parent.
#[derive(Default)]
struct Index {
    nodes: HashMap<ViewId, (NodeType, PropList, Vec<ViewId>)>,
    parent: HashMap<ViewId, ViewId>,
}

impl Index {
    fn of(plan: &Plan, sites: &SiteIndex, children: &[Child]) -> Index {
        let mut index = Index::default();
        index.visit(plan, sites, children, None);
        index
    }

    fn visit(
        &mut self,
        plan: &Plan,
        sites: &SiteIndex,
        children: &[Child],
        parent: Option<ViewId>,
    ) {
        for c in children {
            match c {
                Child::Node(n) => {
                    let row = plan.node(n.node);
                    let mut props = PropList::new();
                    for (i, b) in row.bindings.iter().enumerate() {
                        let b = plan.binding(b);
                        let read = READ.iter().any(|p| *p as u16 == b.id);
                        if let (BindingKind::Prop, true) = (b.kind, read) {
                            let Some(v) = n.emitted(plan, sites, i) else {
                                continue;
                            };
                            if let Ok((prop, value)) = bridge::prop_value(b.id, &v) {
                                props.set(prop, bridge::icon_source(plan, prop, value));
                            }
                        }
                    }
                    let kids = match n.collection() {
                        Some(_) => Vec::new(),
                        None => roots_of(&n.children),
                    };
                    let node_type = NodeType::from_wire(row.node_type).unwrap_or(NodeType::View);
                    self.nodes.insert(n.view, (node_type, props, kids));
                    if let Some(p) = parent {
                        self.parent.insert(n.view, p);
                    }
                    self.visit(plan, sites, &n.children, Some(n.view));
                }
                Child::Region(r) => match &r.active {
                    Active::Arm { roots, .. } => self.visit(plan, sites, roots, parent),
                    Active::Rows { rows } => {
                        for row in rows {
                            self.visit(plan, sites, &row.roots, parent);
                        }
                    }
                },
            }
        }
    }

    fn node(&self, id: ViewId) -> Option<HeadNode<'_>> {
        self.nodes
            .get(&id)
            .map(|(t, p, c)| (*t, p, crate::head::Children::ids(c)))
    }

    /// The navigation root whose tab panel `panel` may be: its nearest
    /// ancestor with a `navigationKey`, when that has a `navigationBack`.
    fn root_of(&self, panel: ViewId) -> Option<ViewId> {
        let mut at = self.parent.get(&panel).copied();
        while let Some(a) = at {
            let props = &self.nodes.get(&a)?.1;
            if props.str(PropId::NavigationKey).is_some() {
                return props.str(PropId::NavigationBack).is_some().then_some(a);
            }
            at = self.parent.get(&a).copied();
        }
        None
    }
}

/// The instance tree's tab state, kept in [`Ids`], which lasts the
/// runner's life and every update reaches.
#[derive(Debug, Default)]
pub(crate) struct Tabs {
    /// The plan's [`panel_sites`], found when first asked; none once every
    /// tab is built ([`Tabs::build_every_tab`]).
    sites: Option<Vec<NodesId>>,
    /// The panels the update under way made empty.
    made: Vec<ViewId>,
    /// The panels whose contents wait for their first selection.
    panels: Vec<ViewId>,
    /// Each waiting panel's navigation root and what lies between it and its
    /// routes (its tablist and tabs, its panels), sorted: a child list one
    /// of them changes, or one of them destroyed, can change which tab is
    /// a navigation tab or selected.
    zone: Vec<ViewId>,
    /// What a reload carried ([`CarriedPanels`]), until each panel is built.
    pub(crate) carried: CarriedPanels,
}

impl Tabs {
    /// The panels whose contents wait for their first selection.
    pub(crate) fn waiting(&self) -> &[ViewId] {
        &self.panels
    }

    /// From now on no tab panel waits for its selection.
    pub(crate) fn build_every_tab(&mut self) {
        self.sites = Some(Vec::new());
    }
}

/// Whether node `node`'s new view `view` is a tab panel whose contents
/// wait: then it is noted, and they are not made with it.
pub(super) fn wait(u: &mut Update<'_>, node: NodesId, view: ViewId) -> bool {
    let plan = u.env.plan;
    let tabs = &mut u.ids.tabs;
    let panel = tabs
        .sites
        .get_or_insert_with(|| panel_sites(plan))
        .binary_search(&node)
        .is_ok();
    if panel {
        tabs.made.push(view);
    }
    panel
}

impl Tree {
    /// Build every waiting panel now; with [`Tabs::build_every_tab`], none
    /// waits again (a page rendered ahead of its runtime, which adopts it
    /// whole).
    pub(crate) fn open_every_tab(&mut self, u: &mut Update<'_>) -> Result<(), InstanceError> {
        let tabs = &mut u.ids.tabs;
        tabs.panels.append(&mut tabs.made);
        while !u.ids.tabs.panels.is_empty() {
            let open = std::mem::take(&mut u.ids.tabs.panels);
            let mut left = open.len();
            open_in(u, &mut self.children, &[], &open, &mut left)?;
            let tabs = &mut u.ids.tabs;
            tabs.panels.append(&mut tabs.made);
        }
        Ok(())
    }

    /// Build the waiting panels among `panels` now, selecting none; a tab
    /// panel inside their contents waits like any other, unless its root
    /// selects it.
    pub(crate) fn open_panels(
        &mut self,
        u: &mut Update<'_>,
        panels: &[ViewId],
    ) -> Result<(), InstanceError> {
        let tabs = &mut u.ids.tabs;
        tabs.panels.append(&mut tabs.made);
        let (open, waiting): (Vec<ViewId>, Vec<ViewId>) = std::mem::take(&mut tabs.panels)
            .into_iter()
            .partition(|w| panels.contains(w));
        tabs.panels = waiting;
        if open.is_empty() {
            return Ok(());
        }
        let mut left = open.len();
        open_in(u, &mut self.children, &[], &open, &mut left)?;
        self.open_tabs(u)
    }

    /// Make the contents of each waiting panel its root now selects, in
    /// this update's batch; a panel no root's tablist names is made at
    /// once. Asked when this update made a panel, set a prop the selection
    /// reads, or changed the children of or destroyed a view of a waiting
    /// panel's root above its routes.
    pub(super) fn open_tabs(&mut self, u: &mut Update<'_>) -> Result<(), InstanceError> {
        let tabs = &mut u.ids.tabs;
        let made = !tabs.made.is_empty();
        tabs.panels.append(&mut tabs.made);
        let around = &tabs.zone;
        let read = |op: &Op| match op {
            Op::SetProp { prop, .. } | Op::ClearProp { prop, .. } => READ.contains(prop),
            Op::SetChildren { id, .. } | Op::DestroyView { id } => around.binary_search(id).is_ok(),
            _ => false,
        };
        // A detached kernel's update writes no prop ops to read.
        if tabs.panels.is_empty() || !(made || u.discard || u.ops.iter().any(read)) {
            return Ok(());
        }
        loop {
            let index = Index::of(u.env.plan, u.sites, &self.children);
            let tabs = &mut u.ids.tabs;
            tabs.panels.retain(|w| index.nodes.contains_key(w));
            let open = to_open(&index, &tabs.panels);
            if open.is_empty() {
                tabs.zone = zone(&index, &tabs.panels);
                return Ok(());
            }
            tabs.panels.retain(|w| !open.contains(w));
            let mut left = open.len();
            open_in(u, &mut self.children, &[], &open, &mut left)?;
            let tabs = &mut u.ids.tabs;
            tabs.panels.append(&mut tabs.made);
        }
    }
}

/// [`Tabs::zone`] for the waiting `panels`, over `index`.
fn zone(index: &Index, panels: &[ViewId]) -> Vec<ViewId> {
    let mut zone = Vec::new();
    let mut stack: Vec<ViewId> = panels.iter().filter_map(|p| index.root_of(*p)).collect();
    while let Some(id) = stack.pop() {
        let Some((_, props, kids)) = index.nodes.get(&id) else {
            continue;
        };
        let route = props.str(PropId::NavigationKey).is_some()
            && props.str(PropId::NavigationBack).is_none();
        if route || zone.contains(&id) {
            continue;
        }
        zone.push(id);
        stack.extend(kids);
    }
    zone.sort_unstable();
    zone
}

/// Of the waiting `waiting`, the panels to make now: at most one per
/// navigation root.
fn to_open(index: &Index, waiting: &[ViewId]) -> Vec<ViewId> {
    let node = |id| index.node(id);
    let key_of = |id| {
        index
            .nodes
            .get(&id)
            .and_then(|n| n.1.str(PropId::NavigationKey))
    };
    let named: Vec<&str> = index
        .nodes
        .values()
        .filter(|(_, p, _)| p.get(PropId::AccessibilitySelected) == Some(&PropValue::Bool(true)))
        .filter_map(|(_, p, _)| p.str(PropId::AccessibilityControls))
        .collect();
    let (mut open, mut roots) = (Vec::new(), Vec::new());
    for &w in waiting {
        let at = index.root_of(w).and_then(|r| Some((r, node(r)?)));
        let panels = at
            .as_ref()
            .map(|(_, at)| tab_panels(&node, &at.2))
            .unwrap_or_default();
        let Some((root, at)) = at.filter(|_| panels.contains(&w)) else {
            open.push(w);
            continue;
        };
        if roots.contains(&root) {
            continue;
        }
        roots.push(root);
        let key = at.1.str(PropId::NavigationKey);
        let held = panels.iter().any(|p| {
            index
                .nodes
                .get(p)
                .is_some_and(|(_, _, kids)| kids.iter().any(|r| key_of(*r) == key))
        });
        if held {
            continue;
        }
        let waits = || panels.iter().filter(|p| waiting.contains(p));
        let chosen = waits()
            .find(|p| {
                let id = index.nodes.get(p).and_then(|n| n.1.str(PropId::Id));
                id.is_some_and(|id| named.contains(&id))
            })
            .or_else(|| waits().next());
        open.extend(chosen);
    }
    open
}

/// The state of components in tab panels a reload carries (LLP 1075.003
/// §9.14): each value with its panel's `id` and its slot's name, until that
/// panel is built again. A panel's contents are one arm, which owns that
/// state (`contract_syntax::inline`); no other arm's state is carried.
pub type CarriedPanels = Vec<(String, String, Value)>;

/// A panel's contents, made now: its one arm, whose state is what a reload
/// carried for this panel where that still conforms, else its initializers'.
fn open_panel(
    u: &mut Update<'_>,
    n: &NodeInst,
    frames: &[Frame],
) -> Result<Vec<Child>, InstanceError> {
    let plan = u.env.plan;
    let arm = plan.node(n.node).arm;
    let (region, contents) = match u.sites.children(Some(n.node), arm) {
        &[(_, Site::Region(r))] if plan.region(r).kind == RegionKind::When => {
            match plan.region(r).arms.iter().next() {
                Some(contents) => (r, contents),
                None => return realize(u, Some(n.node), arm, frames),
            }
        }
        _ => return realize(u, Some(n.node), arm, frames),
    };
    let id = n
        .bound_prop(plan, u.sites, PropId::Id)
        .as_ref()
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut frame = ArmFrame {
        region: Some(region.0),
        ..ArmFrame::default()
    };
    if plan.slots.iter().any(|s| s.owner == Some(contents)) {
        let slots: RowSlots = Rc::new(RefCell::new(BTreeMap::new()));
        frame.region = Some(region.0);
        frame.row = Some(slots.clone());
        let inner = with_frame(frames, frame.scope());
        for (i, s) in plan.slots.iter().enumerate() {
            if s.owner != Some(contents) {
                continue;
            }
            let name = plan.str(s.name);
            let carried = id.as_deref().and_then(|id| {
                let carried = &mut u.ids.tabs.carried;
                let at = carried.iter().position(|(p, n, _)| p == id && n == name)?;
                Some(carried.remove(at).2)
            });
            let v = match carried.filter(|v| v.conforms(plan, s.ty)) {
                Some(v) => v,
                None => u.eval_named(s.init, &inner, || {
                    format!("state {:?} initializer", plan.str(s.name))
                })?,
            };
            if !v.conforms(plan, s.ty) {
                return Err(InstanceError::slot_type(plan, &v, s.ty, name));
            }
            slots.borrow_mut().insert(i as u32, v);
        }
    }
    let roots = realize(u, None, Some(contents), &with_frame(frames, frame.scope()))?;
    Ok(vec![Child::Region(RegionInst {
        region,
        subject: None,
        active: Active::Arm {
            arm: Some(0),
            frame,
            roots,
        },
    })])
}

impl Tree {
    /// The state of each built tab panel's contents, for a reload to carry,
    /// with what an earlier reload carried for panels not built since.
    pub(crate) fn carry_panels(
        &self,
        plan: &Plan,
        index: &SiteIndex,
        left: &CarriedPanels,
    ) -> CarriedPanels {
        let sites = panel_sites(plan);
        let mut out = Vec::new();
        let mut stack: Vec<&Child> = self.children.iter().collect();
        while let Some(c) = stack.pop() {
            match c {
                Child::Node(n) => {
                    let id = n.bound_prop(plan, index, PropId::Id);
                    let id = id.as_ref().and_then(Value::as_str);
                    if let ([Child::Region(r)], Some(id)) = (n.children.as_slice(), id) {
                        if let Active::Arm {
                            frame:
                                ArmFrame {
                                    row: Some(slots), ..
                                },
                            ..
                        } = &r.active
                        {
                            if sites.binary_search(&n.node).is_ok() {
                                for (i, v) in slots.borrow().iter() {
                                    let name = plan.str(plan.slots[*i as usize].name);
                                    out.push((id.to_owned(), name.to_owned(), v.clone()));
                                }
                            }
                        }
                    }
                    stack.extend(&n.children);
                }
                Child::Region(r) => match &r.active {
                    Active::Arm { roots, .. } => stack.extend(roots),
                    Active::Rows { rows } => stack.extend(rows.iter().flat_map(|r| &r.roots)),
                },
            }
        }
        let built: Vec<String> = out.iter().map(|(p, _, _)| p.clone()).collect();
        out.extend(left.iter().filter(|(p, _, _)| !built.contains(p)).cloned());
        out
    }
}

/// Make the contents of the panels `open` names, found under `children`
/// in `frames`, as [`NodeInst::create`] would have made them.
fn open_in(
    u: &mut Update<'_>,
    children: &mut [Child],
    frames: &[Frame],
    open: &[ViewId],
    left: &mut usize,
) -> Result<(), InstanceError> {
    for c in children {
        if *left == 0 {
            return Ok(());
        }
        match c {
            Child::Node(n) => {
                let saved = n.enter(u, n.tuple);
                let done = if open.contains(&n.view) {
                    *left -= 1;
                    // A virtualized list that is a panel makes its window's
                    // rows now, as it would have when made.
                    match u.env.lists {
                        Some(lists) => (lists.create)(u, n.node, n.view, frames)
                            .map(|made| n.set_collection(made)),
                        None => Ok(()),
                    }
                    .and_then(|()| match n.collection() {
                        Some(_) => Ok(()),
                        None => open_panel(u, n, frames).map(|made| {
                            n.children = made;
                            n.emit_children(u);
                        }),
                    })
                } else if n.collection().is_none() {
                    open_in(u, &mut n.children, frames, open, left)
                } else {
                    Ok(())
                };
                u.theme = saved;
                done.map_err(|e| e.at_view(n.view, n.node))?;
            }
            Child::Region(r) => match &mut r.active {
                Active::Arm { roots, frame, .. } => {
                    u.with_frame(frames, frame.scope(), |u, inner| {
                        open_in(u, roots, inner, open, left)
                    })?;
                }
                Active::Rows { rows } => {
                    for row in rows {
                        u.with_frame(frames, row.frame.clone(), |u, inner| {
                            open_in(u, &mut row.roots, inner, open, left)
                        })?;
                    }
                }
            },
        }
    }
    Ok(())
}
