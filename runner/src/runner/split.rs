//! Which stacks a host shows as a list beside its detail (LLP 1115.002 D4;
//! the rule is LLP 1116.004 D1.2's, written once for every host).
//!
//! **A stack splits when a row of its root route's list pushes.** The rule
//! reads the push and never the picture: a row's disclosure chevron is paint
//! (the Mac's table draws none), and what says that a row shows a detail is
//! where its press goes.
//!
//! - *The list* is a `list` in the stack's root route whose rows go down the
//!   page. One that lays its items out sideways (a flex row, a virtualized
//!   row list) is a carousel, and a `list` hidden as authored is none.
//! - *A row's press* is the outermost thing in the row that a press reaches:
//!   a link, or a node that hears `press`. A button inside a row that
//!   presses (a star in a link) is not the row's.
//! - *It pushes* when it is a link to one of the app's routes (its `href`
//!   matches a pattern the route table declares), or when the action its
//!   `press` runs writes the router. An action that writes the router may
//!   also go back or open a sheet: the rule cannot see which, and the author
//!   says `navigationSplit="none"` for a list whose rows only do those.
//! - *`navigationSplit` on the root route* wins: `"none"` keeps the stack
//!   pushed, and `"split"`, a width (the list column's, a whole number of
//!   px from 1 to 999999, `split_width`) or a `"placeholder"` box that is the
//!   route's own child asks for the split, also of a stack whose list this
//!   rule does not find.
//!
//! **Asked or found.** The answer says whether the root route asked
//! (`StackSplit::asked`), and each host admits a stack by its own platform
//! (LLP 1127 D22): the Mac and the painted hosts split one that asks or that
//! this rule finds.
//!
//! **Whose row a route is.** A route above the root whose location a row of
//! the list links to is that row's detail: the row is the selected one while
//! the route shows, and a route that is some row's detail was not pushed
//! from inside the detail (no Back leads from it to the row's last detail).
//! A row whose `press` is an action names no location; its host knows which
//! row it pressed.
//!
//! When a window is wide enough for two columns, and what the columns look
//! like, are each host's: the Mac splits at every width (LLP 1115.002 D4),
//! the web at 900 px.

use super::{DataSource, Runner, RunnerError};
use crate::instance::InstanceError;
use exact_kernel::{
    CommitReceipt, Display, FlexDirection, NodeType, Op, PropId, PropValue, StyleId, StyleProps,
    ViewId,
};
use exact_plan::{EventKind, NodesId, Plan, Value};

/// The press handler's router write-set fact, shared with web projection.
/// A handler that writes the router may push, go back or open a sheet.
pub fn press_writes_router(plan: &Plan, site: NodesId) -> Option<bool> {
    let handler = plan
        .node(site)
        .handlers
        .iter()
        .map(|h| plan.handler(h))
        .find(|h| h.event == EventKind::Press)?;
    Some(plan.router.is_some_and(|slot| {
        plan.action(handler.action)
            .writes
            .iter()
            .any(|w| plan.write(w).slot == slot)
    }))
}

/// The complementary pane beside a route's content (LLP 1115.002, item 4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteInspector {
    /// The aside whose subtree goes in the pane.
    pub view: ViewId,
    /// Its laid-out border-box width in CSS pixels.
    pub width: f32,
    /// The authored minimum including the padding and border, when specified.
    pub minimum: Option<f32>,
    /// The authored maximum including the padding and border, when specified.
    pub maximum: Option<f32>,
}

/// A stack that splits, read from its root route.
#[derive(Debug, Clone, PartialEq)]
pub struct StackSplit {
    /// Whether the root route asks for the split: `navigationSplit` is
    /// `"split"` or a width there, or its own child box is the
    /// `"placeholder"` (LLP 1127 D1).
    pub asked: bool,
    /// The root route's own placeholder box, which asks, and which the split
    /// shows while nothing is selected: the first in tree order (LLP 1127 D1).
    pub placeholder: Option<ViewId>,
    /// The list whose row pushes; `None` where only the root route's ask (its
    /// `navigationSplit` or its placeholder box) splits the stack.
    pub list: Option<ViewId>,
    /// The first visible row whose outer press owner has a non-false `aria-current`.
    pub current: Option<ViewId>,
    /// The list column's width as `navigationSplit` names it, in CSS px.
    pub width: Option<f32>,
    /// Each route above the root in its stack whose location a row of the
    /// list links to, with that row's press: `(route, row)`, bottom to top.
    pub rows: Vec<(ViewId, ViewId)>,
    /// Each route above the root in its stack with its visit's location,
    /// bottom to top: a row's association with a route holds while the
    /// route keeps the location it had, as `replace` keeps a visit's id
    /// (LLP 1127 D3).
    pub locations: Vec<(ViewId, String)>,
}

#[derive(Default)]
struct ListRows {
    actionable: bool,
    pushes: bool,
    current: Option<ViewId>,
    links: Vec<(ViewId, String)>,
}

impl<D: DataSource> Runner<D> {
    /// A trailing aside is a window inspector when it fills the cross axis of the
    /// route's own flex row. A nested aside belongs to its section.
    pub fn route_inspector(&self, route: ViewId) -> Option<RouteInspector> {
        use exact_kernel::{AlignItems, AlignSelf, BoxSizing, Dimension, PositionType, Visibility};

        let route = self.kernel.node(route)?;
        route.props.str(PropId::NavigationKey)?;
        if route.style.display != Display::Flex
            || route.style.flex_direction != FlexDirection::Row
            || matches!(
                route.props.str(PropId::NavigationPresentation),
                Some("modal" | "fullscreen")
            )
        {
            return None;
        }
        let mut children = route.child_nodes().rev().filter(|n| {
            n.style.display != Display::None && n.layout_position() != PositionType::Absolute
        });
        let aside = children.next()?;
        let complementary = match aside.props.str(PropId::AccessibilityRole) {
            Some(role) => role == "complementary",
            None => aside.props.str(PropId::SemanticTag) == Some("aside"),
        };
        if !complementary || children.next().is_none() {
            return None;
        }

        if aside.style.visibility != Visibility::Visible {
            return None;
        }
        let kernel = &self.kernel;
        let padding = kernel.resolved_padding(route.key).unwrap_or_default();
        let border = kernel.resolved_border(route.key).unwrap_or_default();
        let height = route.frame.height - padding.1 - padding.3 - border.1 - border.3;
        let top = route.frame.y + padding.1 + border.1;
        let projected = kernel.host_block(aside.id).is_some();
        let env = kernel.env();
        let (x_inset, y_inset) = if aside.style.box_sizing == BoxSizing::ContentBox {
            let pad = kernel.resolved_padding(aside.key).unwrap_or_default();
            let border = kernel.resolved_border(aside.key).unwrap_or_default();
            (
                pad.0 + pad.2 + border.0 + border.2,
                pad.1 + pad.3 + border.1 + border.3,
            )
        } else {
            (0.0, 0.0)
        };
        let size = |dimension: &Dimension, basis, inset| match dimension.resolve(&env) {
            Dimension::Auto | Dimension::Keyword(_) => None,
            length => Some(length.at_basis(&env, basis).max(0.0) + inset),
        };
        // A native pane has its own containing block. Its frame no longer says
        // whether the authored flex item stretches to the height of the route.
        let fills = if projected {
            let stretches = aside.style.align_self == AlignSelf::Stretch
                || aside.style.align_self == AlignSelf::Auto
                    && matches!(
                        route.style.align_items,
                        AlignItems::Stretch | AlignItems::Normal
                    );
            let h = size(&aside.style.height, height, y_inset);
            h.map_or(stretches, |h| (h - height).abs() <= 1.0)
                && size(&aside.style.max_height, height, y_inset).is_none_or(|h| h >= height - 1.0)
                && size(&aside.style.min_height, height, y_inset).is_none_or(|h| h <= height + 1.0)
        } else {
            (aside.frame.height - height).abs() <= 1.0 && (aside.frame.y - top).abs() <= 1.0
        };
        if height <= 0.0 || aside.frame.width <= 0.0 || !fills {
            return None;
        }
        let basis = route.frame.width - padding.0 - padding.2 - border.0 - border.2
            + if projected { aside.frame.width } else { 0.0 };
        Some(RouteInspector {
            view: aside.id,
            width: aside.frame.width,
            minimum: size(&aside.style.min_width, basis, x_inset),
            maximum: size(&aside.style.max_width, basis, x_inset),
        })
    }

    /// Whether the stack whose root route is `route` asks for a split (LLP
    /// 1127 D1) and the list the shared rule finds beside the route above it,
    /// read from the tree as it now stands. `None` for a node that is no
    /// route, for a root route presented as a sheet or saying "none", and for
    /// one that does not ask and has no list whose rows push, nor an eligible
    /// `retained` list; each host admits
    /// a split from the answer by its own rule (LLP 1127 D22). A host may
    /// retain a previously split list while it has no actionable rows (static
    /// placeholders are allowed); its current eligibility is still this rule's.
    pub fn stack_split(&self, route: ViewId, retained: Option<ViewId>) -> Option<StackSplit> {
        let node = self.kernel.node(route)?;
        node.props.str(PropId::NavigationKey)?;
        if matches!(
            node.props.str(PropId::NavigationPresentation),
            Some("modal" | "fullscreen")
        ) {
            return None;
        }
        let said = node.props.str(PropId::NavigationSplit);
        if said == Some("none") {
            return None;
        }
        let width = said.and_then(exact_kernel::props::split_width);
        let placeholder = self.split_placeholder(route);
        let asked = said == Some("split") || width.is_some() || placeholder.is_some();
        let found = self.pushing_list(route, retained);
        if found.is_none() && !asked {
            return None;
        }
        let (list, found_rows) = match found {
            Some((list, rows)) => (Some(list), rows),
            None => (None, ListRows::default()),
        };
        let locations = self.pane_locations(route);
        // A route whose location a row's link names is that row's detail.
        let rows = locations
            .iter()
            .filter_map(|(route, url)| {
                let (row, _) = found_rows.links.iter().find(|(_, href)| href == url)?;
                Some((*route, *row))
            })
            .collect();
        Some(StackSplit {
            asked,
            placeholder,
            list,
            current: found_rows.current,
            width,
            rows,
            locations,
        })
    }

    /// @ref LLP 1116.006 D2 — show `view`, a box written
    /// `navigationSplit="placeholder"`, as a split shows it in its detail
    /// pane while nothing is selected, or hide it again: the lowerer's
    /// `display: none` gives way to the web shell's sheet for it
    /// (`host/web/index.html`). A flex box filling the containing block the
    /// host gives it, with no margin; and, where neither the author nor the
    /// tag set them, a column centred both ways inside 16 px. Shown, these
    /// rows win over the author's, as the shell's `!important` ones do, so a
    /// host asks again after a commit restyles the box. `None` where `view`
    /// is no placeholder or already is as asked.
    pub fn show_split_placeholder(
        &mut self,
        view: ViewId,
        shown: bool,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let Some(node) = self.kernel.node(view) else {
            return Ok(None);
        };
        if node.props.str(PropId::NavigationSplit) != Some("placeholder") {
            return Ok(None);
        }
        let zero = || Value::Number(0.0);
        let mut rows = vec![("display", Value::str(if shown { "flex" } else { "none" }))];
        if shown {
            rows.extend([
                ("position_type", Value::str("absolute")),
                ("box_sizing", Value::str("border-box")),
                ("width", Value::str("auto")),
                ("height", Value::str("auto")),
            ]);
            for side in ["top", "right", "bottom", "left"] {
                rows.push((side, zero()));
            }
            for side in ["margin_top", "margin_right", "margin_bottom", "margin_left"] {
                rows.push((side, zero()));
            }
            // Neither set nor written: a row the author cleared (`unset`) is said.
            let said = node.style.mask.union(node.style.written);
            let unsaid = |name: &str| StyleId::from_name(name).is_some_and(|id| !said.has(id));
            for (name, value) in [
                ("flex_direction", "column"),
                ("align_items", "center"),
                ("justify_content", "center"),
            ] {
                if unsaid(name) {
                    rows.push((name, Value::str(value)));
                }
            }
            for side in [
                "padding_top",
                "padding_right",
                "padding_bottom",
                "padding_left",
            ] {
                if unsaid(side) {
                    rows.push((side, Value::Number(16.0)));
                }
            }
        }
        let mut patch = StyleProps::default();
        for (name, value) in rows {
            let id = StyleId::from_name(name).expect("a kernel style");
            crate::bridge::set_plan_style(&mut patch, id as u16, &value, &self.plan)
                .map_err(|e| RunnerError::from(InstanceError::Bridge(e)))?;
        }
        let mut after = node.style.clone();
        after.apply_patch(&patch);
        if after == *node.style {
            return Ok(None);
        }
        let op = Op::SetStyle {
            id: view,
            patch: Box::new(patch),
        };
        self.apply(vec![op]).map(Some)
    }

    /// The first of `route`'s own children written
    /// `navigationSplit="placeholder"`: the compiler admits a placeholder
    /// only there (LLP 1127 D1), so the search reads one level and no
    /// subtree. It asks for the split, and the split shows it while nothing
    /// is selected.
    fn split_placeholder(&self, route: ViewId) -> Option<ViewId> {
        self.kernel
            .node(route)?
            .child_nodes()
            .find(|n| {
                n.node_type == NodeType::View
                    && n.props.str(PropId::NavigationKey).is_none()
                    && n.props.str(PropId::NavigationSplit) == Some("placeholder")
            })
            .map(|n| n.id)
    }

    /// The first list under `route`, in tree order, with a row that pushes,
    /// and that list's rows that are links to the app's routes.
    fn pushing_list(&self, route: ViewId, retained: Option<ViewId>) -> Option<(ViewId, ListRows)> {
        let mut empty = None;
        let mut stack: Vec<ViewId> = self.kernel.node(route)?.children();
        stack.reverse();
        while let Some(id) = stack.pop() {
            let Some(node) = self.kernel.node(id) else {
                continue;
            };
            // Another route's, a popover's, a hidden box's and the split's own
            // placeholder's (shown or not) are not this route's rows.
            if node.props.str(PropId::NavigationKey).is_some()
                || node.props.str(PropId::Popover).is_some()
                || node.props.str(PropId::NavigationSplit) == Some("placeholder")
                || node.style.display == Display::None
            {
                continue;
            }
            if node.node_type == NodeType::List && !sideways(&node) {
                let mut rows = ListRows::default();
                self.row_presses(id, id, &mut rows);
                if rows.pushes {
                    return Some((id, rows));
                }
                if retained == Some(id) && !rows.actionable {
                    empty = Some((id, rows));
                }
                continue;
            }
            stack.extend(node.children().into_iter().rev());
        }
        empty
    }

    /// A section's header or footer can contain a navigation command. Only its
    /// body supplies rows to the list (LLP 1115.002 §4).
    fn outside_split_rows(&self, list: ViewId, node: &exact_kernel::NodeRef<'_>) -> bool {
        node.style.display == Display::None
            || node.props.str(PropId::Popover).is_some()
            || node.props.str(PropId::NavigationSplit) == Some("placeholder")
            || matches!(
                node.props.str(PropId::SemanticTag),
                Some("header" | "footer")
            ) && node
                .parent
                .and_then(|p| self.kernel.node(p))
                .is_some_and(|section| {
                    section.props.str(PropId::SemanticTag) == Some("section")
                        && (section.parent == Some(list)
                            || section
                                .parent
                                .and_then(|p| self.kernel.node(p))
                                .is_some_and(|item| {
                                    item.parent == Some(list)
                                        && item.props.str(PropId::ListItemKey).is_some()
                                }))
                })
    }

    fn row_presses(&self, list: ViewId, at: ViewId, rows: &mut ListRows) {
        let Some(node) = self.kernel.node(at) else {
            return;
        };
        for child in node.children() {
            let Some(row) = self.kernel.node(child) else {
                continue;
            };
            if self.outside_split_rows(list, &row) {
                continue;
            }
            let press = self.press_writes_router(child);
            let href = row.props.str(PropId::Href).filter(|h| !h.is_empty());
            match (press, href) {
                (Some(writes), _) => rows.pushes |= writes,
                (None, Some(href)) => {
                    if href.starts_with('/') && !href.starts_with("//") && self.route_matches(href)
                    {
                        rows.pushes = true;
                        rows.links.push((child, exact_route::canonical(href)));
                    }
                }
                (None, None) => {
                    self.row_presses(list, child, rows);
                    continue;
                }
            }
            rows.actionable = true;
            if rows.current.is_none()
                && row
                    .props
                    .str(PropId::AccessibilityCurrent)
                    .is_some_and(|c| !matches!(c, "" | "false"))
            {
                rows.current = Some(child);
            }
        }
    }

    /// The outer press owner of a split row hit at `target`. A nested
    /// control takes its own press without selecting the enclosing row.
    pub fn split_row_owner(&self, list: ViewId, mut target: ViewId) -> Option<ViewId> {
        let mut press = None;
        while target != list {
            let node = self.kernel.node(target)?;
            if self.outside_split_rows(list, &node) {
                return None;
            }
            if self.press_writes_router(target).is_some()
                || node.props.str(PropId::Href).is_some_and(|h| !h.is_empty())
            {
                if press.is_some() {
                    return None;
                }
                press = Some(target);
            }
            target = node.parent?;
        }
        press
    }

    /// For a view that hears `press`, whether the action it runs writes the
    /// router; `None` for a view that hears none.
    pub fn press_writes_router(&self, view: ViewId) -> Option<bool> {
        press_writes_router(&self.plan, self.ids.site(view)?)
    }

    /// The routes after `root` among its parent's children, each with its
    /// visit's location.
    fn pane_locations(&self, root: ViewId) -> Vec<(ViewId, String)> {
        let Some((_, router)) = self.carry_router() else {
            return Vec::new();
        };
        let siblings = self
            .kernel
            .node(root)
            .and_then(|n| n.parent)
            .and_then(|p| self.kernel.node(p))
            .map(|p| p.children())
            .unwrap_or_default();
        let location = |route: ViewId| -> Option<String> {
            let id: u64 = self
                .kernel
                .node(route)?
                .props
                .str(PropId::NavigationKey)?
                .parse()
                .ok()?;
            router
                .tabs
                .iter()
                .flat_map(|tab| tab.stack.iter())
                .find(|e| e.id == id)
                .map(|e| e.url.clone())
        };
        siblings
            .into_iter()
            .skip_while(|id| *id != root)
            .skip(1)
            .filter_map(|route| Some((route, location(route)?)))
            .collect()
    }
}

/// Whether a list lays its items out across the page: a flex row, or one
/// the runner windows by item width.
fn sideways(list: &exact_kernel::NodeRef<'_>) -> bool {
    let row = list.style.display == Display::Flex
        && matches!(
            list.style.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        );
    row || matches!(
        list.props.get(PropId::EstimatedItemWidth),
        Some(PropValue::Float(w)) if *w > 0.0
    )
}
