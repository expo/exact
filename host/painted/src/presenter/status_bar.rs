//! Android's status bar request (LLP 1105 D7). Resolve in the painted page's
//! covering scope, using iOS's declaration order and the winner's scheme;
//! `system-bars-hidden` by the same rule, apart from the style.
use super::*;
use std::cell::Cell;

#[derive(Clone, Copy)]
pub(super) struct Choice {
    pub style: &'static str,
    pub source: Option<ViewId>,
    pub light_icons: bool,
    /// Whether the system bars give way, and the declaration that decided it.
    pub hidden: bool,
    pub hidden_source: Option<ViewId>,
}

#[derive(Clone, Copy, PartialEq)]
struct Generation {
    host: u64,
    epoch: u64,
    dark: bool,
    viewport: exact_runner::Viewport,
    split: Option<(ViewId, bool)>,
}

#[derive(Default)]
pub(super) struct State {
    resolved: Cell<Option<(Generation, Choice)>>,
    #[cfg(test)]
    resolutions: Cell<usize>,
}

impl State {
    pub(super) fn invalidate(&self) {
        self.resolved.set(None);
    }
}

impl<D: DataSource> Presenter<D> {
    pub(super) fn status_bar(&self) -> Choice {
        // Tree/style commits advance the kernel epoch; Host::project_navigation
        // synchronizes route visibility from that same tree before chrome is read.
        // Schemes, viewport preferences and split visibility can change without
        // a kernel commit. Scroll offsets cannot change the winning declaration.
        let generation = Generation {
            host: self.hosts,
            epoch: self.host.kernel().epoch(),
            dark: self.dark(),
            viewport: self.host.runner().viewport(),
            split: self
                .host
                .navigation
                .split
                .active
                .as_ref()
                .map(|s| (s.root, s.hidden)),
        };
        let cache = &self.window.status_bar;
        if let Some((at, choice)) = cache.resolved.get() {
            if at == generation {
                return choice;
            }
        }
        let choice = self.resolve_status_bar();
        cache.resolved.set(Some((generation, choice)));
        choice
    }

    fn resolve_status_bar(&self) -> Choice {
        #[cfg(test)]
        self.window
            .status_bar
            .resolutions
            .set(self.window.status_bar.resolutions.get() + 1);
        let kernel = self.host.kernel();
        let mut choice = Choice {
            style: "default",
            source: None,
            light_icons: self.dark(),
            hidden: false,
            hidden_source: None,
        };
        if !kernel.has_prop(PropId::StatusBarStyle) && !kernel.has_prop(PropId::SystemBarsHidden) {
            return choice;
        }
        let ranks: BTreeMap<_, _> = kernel
            .paint_order()
            .into_iter()
            .map(|(id, p)| (id, p.rank))
            .collect();
        // Preorder in paint order: a descendant follows its ancestor; a
        // whole later branch follows an earlier branch, regardless of depth.
        let mut order = Vec::new();
        let mut pending = self.host.runner().roots().to_vec();
        pending.sort_by_key(|id| ranks.get(id).copied().unwrap_or_default());
        pending.reverse();
        while let Some(id) = pending.pop() {
            let Some(node) = kernel.node(id) else {
                continue;
            };
            order.push(id);
            let mut children = node.children();
            children.sort_by_key(|id| ranks.get(id).copied().unwrap_or_default());
            pending.extend(children.into_iter().rev());
        }
        // A full-screen route covers the bar. A modal one does where it is
        // a page; where the stage shows it as a sheet (`stage.rs`) the sheet
        // rests below the bar, which stays the page's.
        let sheets = self.stage_presents();
        let covering: Vec<_> = self
            .host
            .navigation
            .shown_stacks(kernel)
            .into_iter()
            .filter_map(|stack| stack.last().copied())
            .filter(|id| {
                kernel.node(*id).is_some_and(|n| {
                    match n.props.str(PropId::NavigationPresentation) {
                        Some("fullscreen") => true,
                        Some("modal") => !sheets,
                        _ => false,
                    }
                }) && !self.host.navigation.visibility_of(kernel, *id, false).0
                    && self.status_bar_in_page(*id)
            })
            .collect();
        // Unlike UIKit's separate presentation view, Android paints root
        // overlays in this tree. Declarations above the covering page compete;
        // neither its ancestors nor the branches behind it can supply a fallback.
        let start = order
            .iter()
            .rposition(|id| covering.contains(id))
            .unwrap_or(0);
        let scope = &order[start..];
        let winner = |declares: &dyn Fn(ViewId) -> bool| {
            scope
                .iter()
                .rev()
                .copied()
                .find(|&id| declares(id) && self.status_bar_shows(id))
        };
        let style = |id| {
            kernel
                .node(id)
                .and_then(|n| n.props.str(PropId::StatusBarStyle))
        };
        let hidden = |id| {
            kernel
                .node(id)
                .and_then(|n| n.props.bool(PropId::SystemBarsHidden))
        };
        // Invalid bound values are unset, so an earlier valid declaration
        // can still win, as on iOS.
        if let Some(id) =
            winner(&|id| matches!(style(id), Some("light-content" | "dark-content" | "auto")))
        {
            (choice.style, choice.light_icons) = match style(id) {
                Some("light-content") => ("light-content", true),
                Some("dark-content") => ("dark-content", false),
                _ => match self.status_bar_scheme(id) {
                    Some(true) => ("light-content", true),
                    Some(false) => ("dark-content", false),
                    None => ("default", self.dark()),
                },
            };
            choice.source = Some(id);
        }
        // `false` is a declaration too: it shows the bars an ancestor hides.
        if let Some(id) = winner(&|id| hidden(id).is_some()) {
            choice.hidden = hidden(id) == Some(true);
            choice.hidden_source = Some(id);
        }
        choice
    }

    // The painter carries each node's presented scheme through `normal`.
    // Walk logical parents so a route's lifted header keeps that appearance too.
    fn status_bar_scheme(&self, id: ViewId) -> Option<bool> {
        let kernel = self.host.kernel();
        let mut at = Some(id);
        while let Some(id) = at {
            if let Some(dark) = self.host.presented(id).dark {
                return Some(dark);
            }
            at = kernel.node(id)?.parent;
        }
        None
    }

    fn status_bar_shows(&self, id: ViewId) -> bool {
        let kernel = self.host.kernel();
        let Some(node) = kernel.node(id) else {
            return false;
        };
        if node.is_inline_run()
            || node.node_type.is_svg_element()
            || !crate::paint::paints(kernel, id, None)
            || self.host.navigation.visibility_of(kernel, id, false).0
        {
            return false;
        }
        self.status_bar_in_page(id)
    }

    // Scope eligibility must not use inherited visibility: a hidden
    // ancestor can contain a visible descendant, as on iOS.
    fn status_bar_in_page(&self, id: ViewId) -> bool {
        let kernel = self.host.kernel();
        let mut at = kernel.node(id);
        while let Some(node) = at {
            // Lifted route headers still count at their route's place.
            // Dialogs and menus are separate projections, outside this scope.
            if node.style.display == exact_kernel::Display::None
                || node.props.str(PropId::SemanticTag) == Some("dialog")
                || node.props.str(PropId::Popover).is_some()
            {
                return false;
            }
            at = node.parent.and_then(|id| kernel.node(id));
        }
        true
    }

    pub(super) fn status_bar_layout(&self, out: &mut String) {
        let bar = self.status_bar();
        let node = |id: Option<ViewId>| id.map_or_else(|| "null".into(), |id| id.to_string());
        let _ = write!(
            out,
            ",\"statusBar\":{{\"style\":\"{}\",\"source\":{},\"hidden\":{},\"hiddenSource\":{}}}",
            bar.style,
            node(bar.source),
            bar.hidden,
            node(bar.hidden_source)
        );
    }
}

#[cfg(test)]
#[path = "status_bar_tests.rs"]
mod tests;
