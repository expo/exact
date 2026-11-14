//! The routes, document scroller and hero known before any opening tag streams.

use super::{
    bars::bar_header, escape, props_of, tag_of, NodeFacts, PropId, Route, SortedMap, SortedSet,
    Source, ViewId,
};
use exact_plan::EventKind;

/// The image selected for a JavaScript page's preload and fetch priority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeroImage {
    /// The source URL before HTML escaping.
    pub src: String,
    /// The `<img>`'s `crossorigin` value, before HTML escaping.
    pub crossorigin: Option<String>,
    /// Byte offset in `Document::root` for adding a missing `fetchpriority` attribute.
    pub priority_at: Option<usize>,
}

impl HeroImage {
    /// The `<img>`'s priority attribute, which its preload carries too: the
    /// preload starts the request, at the preload's priority, so a preload
    /// without it fetches the hero at low priority (LLP 1048.007 D5).
    pub const PRIORITY: &'static str = " fetchpriority=\"high\"";

    /// The `<link rel="preload">` that makes the `<img>`'s request: its URL
    /// and `crossorigin`, escaped as the `<img>` escapes them, and its
    /// priority. Chrome leaves a preload of another CORS mode unused and
    /// fetches the image twice. `None` for a value with a NUL character,
    /// which the walk refuses too.
    pub fn preload(&self) -> Option<String> {
        let mut link = String::from("<link rel=\"preload\" as=\"image\" href=\"");
        escape(&mut link, &self.src, true).ok()?;
        link.push('"');
        if let Some(crossorigin) = &self.crossorigin {
            link.push_str(" crossorigin=\"");
            escape(&mut link, crossorigin, true).ok()?;
            link.push('"');
        }
        link.push_str(Self::PRIORITY);
        link.push('>');
        Some(link)
    }
}

#[derive(Default)]
pub(super) struct Before {
    pub routes: SortedMap<ViewId, Route>,
    pub bars: SortedSet<ViewId>,
    pub page: SortedSet<ViewId>,
    pub hero: Option<(ViewId, HeroImage)>,
    hero_seen: bool,
}

impl Before {
    pub fn new<S: Source>(
        src: &S,
        roots: &[ViewId],
        handlers: &SortedMap<ViewId, Vec<EventKind>>,
        lazy: &dyn Fn(ViewId) -> bool,
    ) -> Self {
        let mut before = Self::default();
        let mut live = Vec::new();
        for root in roots {
            before.scan(
                src,
                handlers,
                lazy,
                *root,
                false,
                false,
                &mut Vec::new(),
                &mut live,
            );
        }
        if let [path] = &live[..] {
            before.page = path.iter().copied().collect();
        }
        before
    }

    /// Each path from the roots to an element that is a page scroller on the
    /// page, not in a hidden route, as the walk will find them: found
    /// before any opening tag streams, marking the routes as it goes.
    #[allow(clippy::too_many_arguments)]
    fn scan<S: Source>(
        &mut self,
        src: &S,
        handlers: &SortedMap<ViewId, Vec<EventKind>>,
        lazy: &dyn Fn(ViewId) -> bool,
        id: ViewId,
        hidden: bool,
        covered: bool,
        path: &mut Vec<ViewId>,
        live: &mut Vec<Vec<ViewId>>,
    ) {
        let Some(node) = src.facts(id) else { return };
        if node.node_type.is_metadata() {
            return;
        }
        let hidden = hidden || self.routes.get(&id).is_some_and(|r| r.hidden);
        if !hidden
            && !matches!(
                node.node_type,
                exact_kernel::NodeType::SvgFe | exact_kernel::NodeType::SvgFilter
            )
            && node.props.bool(PropId::ScrollDocument) == Some(true)
        {
            live.push(path.clone());
        }
        let covered = covered
            || self.routes.get(&id).is_some_and(|r| r.inert)
            || node.props.bool(PropId::Inert) == Some(true)
            || node.style.visibility == exact_kernel::Visibility::Hidden;
        if !covered && !self.hero_seen && tag_of(&node, false) == "img" {
            let props = props_of(&node);
            if let Some(source) = props.get("src").filter(|source| {
                !source.is_empty() && !source.starts_with("data:") && !source.starts_with("symbol:")
            }) {
                // A first image in a repeated row is lazy; no later image replaces it.
                self.hero_seen = true;
                if !lazy(id) {
                    let hero = HeroImage {
                        src: source.to_owned(),
                        crossorigin: props.get("crossorigin").cloned(),
                        priority_at: None,
                    };
                    self.hero = Some((id, hero));
                }
            }
        }
        let children = src.children(id);
        self.route_children(src, handlers, &node, &children);
        path.push(id);
        for child in children.iter().copied() {
            self.scan(src, handlers, lazy, child, hidden, covered, path, live);
        }
        path.pop();
    }

    /// Mark the routes under a navigation root as `navigation.project` does;
    /// with tabs, as the `tabs` capability does.
    fn route_children<S: Source>(
        &mut self,
        src: &S,
        handlers: &SortedMap<ViewId, Vec<EventKind>>,
        node: &NodeFacts<'_>,
        children: &[ViewId],
    ) {
        if node.props.str(PropId::NavigationBack).is_none() {
            return;
        }
        let key = node.props.str(PropId::NavigationKey);
        if let Some(tabs) = crate::link::linked().tabs {
            for (id, hidden, inert) in tabs(src, children, key) {
                self.routes.insert(id, Route { hidden, inert });
                if let Some(header) = bar_header(src, handlers, id) {
                    self.bars.insert(header);
                }
            }
            return;
        }
        let routes: Vec<NodeFacts<'_>> = children
            .iter()
            .filter_map(|c| src.facts(*c))
            .filter(|c| c.props.str(PropId::NavigationKey).is_some())
            .collect();
        // A key that names no route leaves the stack as it is.
        let Some(selected) = routes
            .iter()
            .position(|r| r.props.str(PropId::NavigationKey) == key)
        else {
            return;
        };
        let modal = routes[selected].props.str(PropId::NavigationPresentation) == Some("modal");
        for (index, route) in routes.iter().enumerate() {
            if let Some(header) = bar_header(src, handlers, route.id) {
                self.bars.insert(header);
            }
            let active = index == selected;
            let shown = active || (modal && index + 1 == selected);
            self.routes.insert(
                route.id,
                Route {
                    hidden: !shown,
                    inert: !active,
                },
            );
        }
    }
}
