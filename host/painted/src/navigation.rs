//! @ref LLP 1038 D6 — the browser's route visibility rule, without a view mirror.
use exact_kernel::invocation::Action;
use exact_kernel::{Kernel, NodeRef, PropId, StyleId, ViewId};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const POPOVER_UNSUPPORTED: &str = "Linux does not support popover presentation";
pub(crate) const DIALOG_UNSUPPORTED: &str =
    "unsupported: Linux dialog presentation is not implemented";

/// Popover invocation is a host default action, even without a press handler.
/// Refuse it before running an application's accompanying refresh/action.
pub(crate) fn popover_invoker(kernel: &Kernel, id: ViewId) -> bool {
    let mut at = Some(id);
    while let Some(id) = at {
        let Some(node) = kernel.node(id) else { break };
        if node.props.bool(PropId::Disabled) == Some(true) {
            return false;
        }
        // A native button is a button: a press on it is its own, not an
        // enclosing invoker's (LLP 1069.011.000 D9; it invokes no menu).
        if node.node_type == exact_kernel::NodeType::Pressable
            || exact_kernel::ControlKind::of(node.node_type, node.props)
                == Some(exact_kernel::ControlKind::Button)
        {
            return invocation(kernel, id).is_some_and(|action| !dialog_command(action));
        }
        at = node.parent;
    }
    false
}

/// What the button `id` invokes, as HTML resolves it.
fn invocation(kernel: &Kernel, id: ViewId) -> Option<Action> {
    let node = kernel.node(id)?;
    exact_kernel::Invocation::of(kernel, &node).map(|i| i.action)
}

fn dialog_command(action: Action) -> bool {
    matches!(action, Action::ShowModal | Action::Close)
}

/// LLP 1035.001 D1, when the root's key matches no route: the stack is left
/// alone, less the routes that have left the tree. The deepest route of the
/// stack last shown (its keys through its selected route) that is still
/// declared is the selected one, so a closed sheet or a popped screen never
/// leaves the route beneath it covered; `None` (nothing shown yet) changes nothing.
pub fn survivor(shown: &[String], declared: impl Fn(&str) -> bool) -> Option<String> {
    shown.iter().rev().find(|key| declared(key)).cloned()
}

/// How a route is presented over the routes beneath it (LLP 1035.001 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presented {
    /// Pushed: in the sheet beneath it if there is one, else a page.
    Pushed,
    /// A sheet (`modal`): it shows what it sits over.
    Sheet,
    /// A full-screen route whose author wrote a background, which
    /// composites over the page (Charlie, 2026-11-10).
    Over,
    /// A full-screen route that paints nothing: it hides the page.
    Full,
}

impl Presented {
    pub(crate) fn of(node: &NodeRef<'_>) -> Self {
        match node.props.str(PropId::NavigationPresentation) {
            Some("modal") => Self::Sheet,
            Some("fullscreen") if over_page(node) => Self::Over,
            Some("fullscreen") => Self::Full,
            _ => Self::Pushed,
        }
    }

    fn presents(self) -> bool {
        self != Self::Pushed
    }
}

/// A full-screen route's authored background composites over the page,
/// including while its colour animates toward transparent (Charlie, 2026-11-10).
pub(crate) fn over_page(node: &NodeRef<'_>) -> bool {
    match node.props.str(PropId::NavigationPresentation) {
        Some("modal") => true,
        Some("fullscreen") => {
            node.style.written.has(StyleId::BackgroundColor)
                || node.style.written.has(StyleId::BackgroundImage)
        }
        _ => false,
    }
}

/// The sheet route `index` shows in: itself when it is one, or the sheet
/// it was pushed in, since a push inside a sheet stays in it as a
/// navigation controller's in a presented sheet does (expo/exact#326; the
/// web's `sheetAt`). `None` for a page, a full-screen route, and a route
/// pushed over one.
pub fn sheet_of(kinds: &[Presented], index: usize) -> Option<usize> {
    (0..=index.min(kinds.len().checked_sub(1)?))
        .rev()
        .find(|i| kinds[*i].presents())
        .filter(|i| kinds[*i] == Presented::Sheet)
}

/// Hidden and inert, respectively, for each direct route child, `kinds`
/// saying how each is presented. A route that shows what it sits over (a
/// sheet, a route pushed inside one, a full-screen route with an authored
/// background) keeps the page (the last route beneath it that is neither)
/// and every such route between visible and inert, as navigation.js
/// `covered` does; of one sheet's routes only its top one shows. An
/// unmatched selection leaves the previously projected state alone.
pub fn route_visibility(
    keys: &[&str],
    selected: &str,
    kinds: &[Presented],
) -> Option<Vec<(bool, bool)>> {
    let selected = keys.iter().position(|key| *key == selected)?;
    let kind = |i: usize| kinds.get(i).copied().unwrap_or(Presented::Pushed);
    let sheet = |i: usize| sheet_of(kinds, i);
    let over =
        |i: usize| matches!(kind(i), Presented::Sheet | Presented::Over) || sheet(i).is_some();
    // The route the selected one is presented as: its sheet, or itself.
    let presented = sheet(selected).unwrap_or(selected);
    let page = (0..presented).rev().find(|i| !over(*i)).unwrap_or(0);
    // A sheet shows the last of its routes beneath the selection.
    let top = |i: usize| sheet(i).is_none() || sheet(i + 1) != sheet(i);
    let covered = |i: usize| over(presented) && (page..presented).contains(&i) && top(i);
    Some(
        (0..keys.len())
            .map(|index| {
                let active = index == selected;
                (!active && !covered(index), !active)
            })
            .collect(),
    )
}

/// A navigation root's stacks (LLP 1075.003 §3.7): with tabs (the kernel's
/// rule, which every host with a tab bar reads, LLP 1115.001 D11) each
/// panel is one, its children that carry a `navigationKey`; with none, the
/// root's own such children are the one stack.
pub(crate) fn stacks(
    kernel: &Kernel,
    root: ViewId,
) -> (Option<exact_kernel::RootTabs>, Vec<Vec<ViewId>>) {
    let route_rows = |id: ViewId| -> Vec<ViewId> {
        let children = kernel.node(id).map(|n| n.children()).unwrap_or_default();
        children
            .into_iter()
            .filter(|id| {
                kernel
                    .node(*id)
                    .is_some_and(|n| n.props.str(PropId::NavigationKey).is_some())
            })
            .collect()
    };
    let tabs = kernel.root_tabs(root);
    let stacks = match &tabs {
        Some(tabs) => tabs.panels.iter().map(|p| route_rows(*p)).collect(),
        None => vec![route_rows(root)],
    };
    (tabs, stacks)
}

#[derive(Default)]
pub(crate) struct Navigation {
    pub(crate) split: crate::presenter::split::State,
    routes: BTreeMap<ViewId, (bool, bool)>,
    /// Tabpanels and tablists last projected: one no longer owned by a root is released.
    tab_containers: BTreeSet<ViewId>,
    refused: BTreeMap<ViewId, String>,
    /// Each root's stack as last shown, its keys through its selected route.
    shown: BTreeMap<ViewId, Vec<String>>,
    /// The first root's stack as its key last named it: its routes through
    /// the selected one. What the focus follows (`presenter/arrive.rs`; LLP
    /// 1115.000 D4.2); a key that matches no route leaves it as it was.
    pub(crate) stack: Option<Vec<ViewId>>,
    popovers: bool,
    pub(crate) shown_popover: Option<ViewId>,
    /// A shell presents the top layer (LLP 1115.001 D10: Android's reader,
    /// as windows): a press on an invoker is then the presenter's to act on
    /// (`presenter/top.rs`), not refused here.
    pub(crate) top_layer: bool,
    /// What a shell's bars are made from (LLP 1115.001 D11; `bars.rs`),
    /// and the nodes a bar stands for: a header its top bar replaces, a
    /// tablist its navigation bar replaces. Those are neither painted nor
    /// hit, as iOS hides the header its bar lifts.
    pub(crate) bars: Projection,
    pub(crate) lifted: BTreeSet<ViewId>,
}

/// A route whose `header` is its bar (the kernel's rule,
/// `exact_kernel::bars`).
#[derive(Debug, Clone, PartialEq)]
pub struct RouteBar {
    /// The route.
    pub route: ViewId,
    /// Not the first of its stack, nor presented over it: the bar's Up
    /// arrow goes Back from it, and stands for its Back control.
    pub pushed: bool,
    /// Its header as the bar shows it.
    pub shape: exact_kernel::HeaderShape<ViewId>,
}

/// The structure a shell's bars are made from (LLP 1115.001 D11), read
/// from the first navigation root after each commit (`bars.rs`). Empty on a
/// host whose shell has no bars: desktop Linux, a headless run.
#[derive(Debug, Default)]
pub struct Projection {
    /// The room the shell's bars take of a route: the top bar from its top
    /// edge and the navigation bar from its bottom, and the system's bar
    /// under a sheet's foot, in points. `None`: no
    /// shell has bars, and nothing is projected.
    pub(crate) room: Option<(f32, f32, f32)>,
    /// The navigation root.
    pub root: Option<ViewId>,
    /// Its routes whose header is a bar, in every stack.
    pub routes: Vec<RouteBar>,
    /// Its tabs, when its tablist is the navigation bar.
    pub tabs: Option<exact_kernel::RootTabs>,
    /// Its stacks of routes: one a tab, in the tabs' order; else the one.
    pub stacks: Vec<Vec<ViewId>>,
    /// What the bars cover of the tree, as the kernel was last told.
    pub(crate) covers: BTreeMap<ViewId, exact_kernel::HostCover>,
    /// Each barred or sheeted route's local insets (LLP 1123 D9), top right
    /// bottom left, as the kernel was last told.
    pub(crate) locals: BTreeMap<ViewId, [f32; 4]>,
}

impl Navigation {
    /// Why a press on `view` is refused before its handler runs, on a host
    /// that presents no top layer: a popover's invoker (`dialog` false), or
    /// a button that names a dialog with `commandfor` (`dialog` true).
    pub(crate) fn refusal(&self, kernel: &Kernel, view: ViewId, dialog: bool) -> Option<&str> {
        if self.top_layer {
            return None;
        }
        match dialog {
            false => popover_invoker(kernel, view).then_some(POPOVER_UNSUPPORTED),
            true => invocation(kernel, view)
                .is_some_and(dialog_command)
                .then_some(DIALOG_UNSUPPORTED),
        }
    }

    pub fn sync(&mut self, kernel: &Kernel, order: &[ViewId]) -> Vec<String> {
        self.routes.retain(|id, _| kernel.node(*id).is_some());
        self.refused.retain(|id, _| kernel.node(*id).is_some());
        self.shown.retain(|id, _| kernel.node(*id).is_some());
        let mut logs = Vec::new();
        let before = std::mem::take(&mut self.tab_containers);
        self.popovers = false;
        let mut first = true;
        for id in order {
            let Some(nav) = kernel.node(*id) else {
                continue;
            };
            self.popovers |= nav.props.str(PropId::Popover).is_some();
            if nav.props.str(PropId::NavigationBack).is_none() {
                continue;
            }
            let first = std::mem::take(&mut first);
            // @ref LLP 1075.003 §3.7 — with tabs, each panel is a stack.
            let (tabs, stacks) = stacks(kernel, *id);
            let panels = tabs.as_ref().map_or(&[][..], |tabs| tabs.panels.as_slice());
            self.tab_containers.extend(panels.iter().copied());
            self.tab_containers
                .extend(tabs.as_ref().map(|tabs| tabs.tablist));
            let key = |id: &ViewId| {
                kernel
                    .node(*id)
                    .and_then(|n| n.props.str(PropId::NavigationKey))
                    .unwrap_or("")
            };
            let named = nav.props.str(PropId::NavigationKey).unwrap_or("");
            let declared = |k: &str| {
                stacks
                    .iter()
                    .any(|routes| routes.iter().any(|r| key(r) == k))
            };
            let matched = declared(named);
            let selected = if matched {
                self.refused.remove(id);
                named.to_string()
            } else {
                if self.refused.get(id).map(String::as_str) != Some(named) {
                    self.refused.insert(*id, named.into());
                    logs.push(format!(
                        "navigationKey \"{named}\" matches no route among the root's children or those of the tabpanels its tablist names; the stack is unchanged"
                    ));
                }
                let shown = self.shown.get(id).map(Vec::as_slice).unwrap_or_default();
                let Some(survivor) = survivor(shown, declared) else {
                    continue;
                };
                survivor
            };
            let Some(at) = stacks
                .iter()
                .position(|routes| routes.iter().any(|r| key(r) == selected))
            else {
                continue;
            };
            for (index, panel) in panels.iter().enumerate() {
                self.routes.insert(*panel, (index != at, index != at));
            }
            for (stack, routes) in stacks.iter().enumerate() {
                let keys: Vec<&str> = routes.iter().map(key).collect();
                // The selected stack shows the route the root names; another
                // keeps its top laid out.
                let shown = if stack == at {
                    selected.as_str()
                } else {
                    keys.last().copied().unwrap_or("")
                };
                let kinds: Vec<Presented> = (routes.iter())
                    .map(|r| {
                        kernel
                            .node(*r)
                            .map_or(Presented::Pushed, |n| Presented::of(&n))
                    })
                    .collect();
                if stack == at {
                    let through = keys.iter().position(|k| *k == shown).map_or(0, |i| i + 1);
                    if let Some(tabs) = &tabs {
                        // A sheet covers the root's tab bar too, and so does
                        // a route pushed inside one (LLP 1115.000 D4.5).
                        let top = through.saturating_sub(1);
                        let inert = kinds.get(top).is_some_and(|k| k.presents())
                            || sheet_of(&kinds, top).is_some();
                        self.routes.insert(tabs.tablist, (false, inert));
                    }
                    self.shown
                        .insert(*id, keys[..through].iter().map(|k| k.to_string()).collect());
                    if first && matched {
                        self.stack = Some(routes[..through].to_vec());
                    }
                }
                if let Some(visibility) = route_visibility(&keys, shown, &kinds) {
                    for (route, state) in routes.iter().zip(visibility) {
                        self.routes.insert(*route, state);
                    }
                }
            }
        }
        for container in before.difference(&self.tab_containers) {
            self.routes.remove(container);
        }
        logs
    }

    /// Committed stacks through their selected routes, including an unmatched
    /// selection's surviving stack. Hidden navigation roots contribute nothing.
    pub(crate) fn shown_stacks(&self, kernel: &Kernel) -> Vec<Vec<ViewId>> {
        self.shown
            .iter()
            .filter_map(|(root, keys)| {
                if self.visibility_of(kernel, *root, false).0 {
                    return None;
                }
                let (_, stacks) = stacks(kernel, *root);
                stacks.into_iter().find_map(|stack| {
                    let selected = keys.last()?;
                    let last = stack.iter().position(|id| {
                        kernel.node(*id).is_some_and(|n| {
                            n.props.str(PropId::NavigationKey) == Some(selected.as_str())
                        })
                    })?;
                    Some(stack[..=last].to_vec())
                })
            })
            .collect()
    }

    pub fn visibility(&self, kernel: &Kernel, id: ViewId) -> (bool, bool) {
        self.visibility_of(kernel, id, true)
    }

    /// [`Navigation::visibility`], and whether what a shell's bar stands
    /// for counts as gone from the page (`lifted`).
    pub(crate) fn visibility_of(&self, kernel: &Kernel, id: ViewId, lifted: bool) -> (bool, bool) {
        self.visibility_to(kernel, id, lifted, None)
    }

    /// A separately presented subtree inherits visibility only through its root.
    pub(crate) fn visibility_to(
        &self,
        kernel: &Kernel,
        id: ViewId,
        lifted: bool,
        root: Option<ViewId>,
    ) -> (bool, bool) {
        self.visibility_with(kernel, id, lifted, root, &[])
    }

    /// Whether `id` is hidden, the routes in `shown` counting as shown: the
    /// presenter's stage draws them though the stack hides them (a route a
    /// push covers, while it slides out).
    pub(crate) fn hidden_but(&self, kernel: &Kernel, id: ViewId, shown: &[ViewId]) -> bool {
        self.visibility_with(kernel, id, true, None, shown).0
    }

    fn visibility_with(
        &self,
        kernel: &Kernel,
        id: ViewId,
        lifted: bool,
        root: Option<ViewId>,
        shown: &[ViewId],
    ) -> (bool, bool) {
        // No route, popover or inert node anywhere: nothing to climb for.
        if self.routes.is_empty()
            && (self.lifted.is_empty() || !lifted)
            && !(self.popovers && kernel.has_prop(PropId::Popover))
            && !kernel.has_prop(PropId::Inert)
        {
            return (false, false);
        }
        let mut result = (false, false);
        let mut at = Some(id);
        while let Some(id) = at {
            let Some(node) = kernel.node(id) else { break };
            let (mut hidden, inert) = self
                .split
                .visibility(id)
                .unwrap_or_else(|| self.routes.get(&id).copied().unwrap_or_default());
            hidden &= !shown.contains(&id);
            // Closed popovers retain their tree but neither paint nor take input.
            let closed = self.popovers
                && Some(id) != self.shown_popover
                && node.props.str(PropId::Popover).is_some();
            // What a shell's bar stands for shows in the bar, not the page.
            let gone = closed || lifted && self.lifted.contains(&id);
            result.0 |= hidden || gone;
            result.1 |= inert || gone || node.props.bool(PropId::Inert) == Some(true);
            if Some(id) == root {
                break;
            }
            at = node.parent;
        }
        result
    }
}

impl<D: exact_runner::DataSource> crate::Host<D> {
    pub(crate) fn neutral_dialog_invoker(&self, view: ViewId) -> bool {
        let kernel = self.kernel();
        kernel
            .node(view)
            .and_then(|node| exact_kernel::Invocation::of(kernel, &node))
            .and_then(|invocation| kernel.node_by_key(invocation.target))
            .is_some_and(|n| {
                crate::paint::look::neutral()
                    && (n.props.str(PropId::SemanticTag) == Some("dialog")
                        || n.props.str(PropId::Popover).is_some()
                            && n.props.str(PropId::AccessibilityRole) != Some("menu"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Presented::{Pushed as P, Sheet as S};

    #[test]
    fn ordinary_and_tabbed_roots_project_their_routes_through_push_and_back() {
        let routes = r#"column navigationKey="home" testId="home"
  header
    text "Home" role="heading"
  button press=push testId="push"
    text "Open"
column navigationKey="detail" testId="detail"
  header
    button id="back" press=back testId="back"
      text "Back"
    text "Detail" role="heading"
"#;
        let tablist = "row role=\"tablist\"\n  button role=\"tab\" aria-controls=\"panel\"\n    text \"Home\"\n";
        let indent = |view: &str| {
            view.lines()
                .map(|line| format!("  {line}\n"))
                .collect::<String>()
        };
        let unlinked = tablist.replace(" aria-controls=\"panel\"", "");
        let linked = format!(
            "column role=\"tabpanel\" id=\"panel\"\n{}{tablist}",
            indent(routes)
        );
        for back in ["", " navigationBack=\"back\""] {
            for (children, tabbed) in [
                (routes.to_string(), false),
                (
                    format!("{routes}{unlinked}column role=\"tabpanel\" id=\"panel\"\n"),
                    false,
                ),
                (
                    format!(
                        "column\n  column role=\"tabpanel\" id=\"panel\"\n{}{tablist}",
                        indent(&indent(routes))
                    ),
                    true,
                ),
                (
                    format!(
                        "{tablist}  column role=\"tabpanel\" id=\"panel\"\n{}",
                        indent(&indent(routes))
                    ),
                    true,
                ),
                (
                    linked.replace("role=\"tablist\"", "role=(shown ? \"tablist\" : \"group\")"),
                    true,
                ),
                (
                    linked.replace("role=\"tab\"", "role=(shown ? \"tab\" : \"group\")"),
                    true,
                ),
                (
                    linked.replace(
                        "role=\"tabpanel\"",
                        "role=(shown ? \"tabpanel\" : \"group\")",
                    ),
                    true,
                ),
            ] {
                let source = format!(
                    "component App\n  state shown = true\n  state selected = \"home\"\n  action push\n    selected = \"detail\"\n  action back\n    selected = \"home\"\n  view\n    main navigationKey=selected{back}\n{}",
                    indent(&indent(&indent(&children)))
                );
                let plan = contract::compile(&source).unwrap();
                let (mut host, error) = crate::Host::boot(
                    &plan.encode(),
                    (),
                    Box::new(exact_kernel::MonospaceMeasurer::default()),
                    390.,
                    844.,
                )
                .unwrap();
                assert!(error.is_none(), "{error:?}");
                let id = |name: &str| {
                    let kernel = host.kernel();
                    kernel
                        .node_by_key(kernel.find_by_test_id(name)[0])
                        .unwrap()
                        .id
                };
                let (home, detail, push, back) = (id("home"), id("detail"), id("push"), id("back"));
                let root = host.kernel().roots()[0];
                let (tabs, stacks) = stacks(host.kernel(), root);
                assert_eq!(tabs.is_some(), tabbed, "{source}");
                assert_eq!(stacks, [vec![home, detail]], "{source}");
                assert_eq!(host.shown_stack(), Some([home].as_slice()));
                assert_eq!(host.route_visibility(home), (false, false));
                assert_eq!(host.route_visibility(detail), (true, true));
                assert!(host
                    .dispatch_at(push, exact_runner::Event::Press, 0.)
                    .is_none());
                assert_eq!(host.shown_stack(), Some([home, detail].as_slice()));
                assert_eq!(host.route_visibility(home), (true, true));
                assert_eq!(host.route_visibility(detail), (false, false));
                assert!(host
                    .dispatch_at(back, exact_runner::Event::Press, 1.)
                    .is_none());
                assert_eq!(host.shown_stack(), Some([home].as_slice()));
                assert_eq!(host.route_visibility(home), (false, false));
                assert_eq!(host.route_visibility(detail), (true, true));
                assert!(
                    !exact_runner::agent::logs(host.runner(), 0).contains("matches no route"),
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn selected_route_and_only_its_modal_underlay_are_visible() {
        let keys = ["a", "b", "c", "d"];
        assert_eq!(
            route_visibility(&keys, "c", &[P; 4]),
            Some(vec![
                (true, true),
                (true, true),
                (false, false),
                (true, true)
            ])
        );
        assert_eq!(
            route_visibility(&keys, "c", &[P, P, S, P]),
            Some(vec![
                (true, true),
                (false, true),
                (false, false),
                (true, true)
            ])
        );
        assert_eq!(
            route_visibility(&keys, "a", &[S, P, P, P]),
            Some(vec![
                (false, false),
                (true, true),
                (true, true),
                (true, true)
            ])
        );
        assert_eq!(route_visibility(&keys, "missing", &[]), None);
        assert_eq!(route_visibility(&[], "missing", &[]), None);
    }

    /// A sheet over a sheet shows both the sheet beneath it and the page
    /// (LLP 1133.029 D3), as navigation.js `covered` does; a route pushed
    /// inside the first sheet shows in it in place of the sheet's own.
    #[test]
    fn a_sheet_over_a_sheet_shows_the_page_and_the_sheets_between() {
        let keys = ["a", "b", "c", "d"];
        assert_eq!(
            route_visibility(&keys, "d", &[P, P, S, S]),
            Some(vec![
                (true, true),
                (false, true),
                (false, true),
                (false, false)
            ])
        );
        assert_eq!(
            route_visibility(&keys, "d", &[P, S, P, S]),
            Some(vec![
                (false, true),
                (true, true),
                (false, true),
                (false, false)
            ])
        );
    }

    /// A route pushed inside a sheet shows in it, over the page the sheet
    /// covers; the sheet's own route waits beneath it (expo/exact#326).
    #[test]
    fn a_route_pushed_inside_a_sheet_keeps_the_page_beneath_the_sheet() {
        let keys = ["home", "info", "settings", "sounds"];
        let kinds = [P, S, P, P];
        assert_eq!(sheet_of(&kinds, 0), None);
        assert_eq!(sheet_of(&kinds, 3), Some(1));
        assert_eq!(
            route_visibility(&keys, "settings", &kinds),
            Some(vec![
                (false, true),
                (true, true),
                (false, false),
                (true, true)
            ])
        );
        assert_eq!(
            route_visibility(&keys, "sounds", &kinds),
            Some(vec![
                (false, true),
                (true, true),
                (true, true),
                (false, false)
            ])
        );
        // Over a full-screen route a push is a page of its own.
        let kinds = [P, Presented::Full, P, P];
        assert_eq!(sheet_of(&kinds, 2), None);
        assert_eq!(
            route_visibility(&keys, "settings", &kinds),
            Some(vec![
                (true, true),
                (true, true),
                (false, false),
                (true, true)
            ])
        );
    }

    /// LLP 1035.001 D1: a root whose key names no route (visit "0" over a
    /// route keyed "rooms") keeps what is left of the stack it showed: the
    /// sheet closed, the route beneath it is selected, live again.
    #[test]
    fn a_key_that_names_no_route_keeps_what_is_left_of_the_stack_shown() {
        let shown = ["rooms".to_string(), "1".to_string()];
        assert_eq!(survivor(&shown, |k| k == "rooms"), Some("rooms".into()));
        assert_eq!(
            survivor(&shown, |k| k == "rooms" || k == "1"),
            Some("1".into())
        );
        assert_eq!(survivor(&shown, |_| false), None);
        assert_eq!(survivor(&[], |_| true), None);
    }
}
