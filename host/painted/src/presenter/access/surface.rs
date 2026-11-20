use super::*;
use crate::presenter::top::{Row, Shown, SurfaceKind, Target};
use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};

impl Build {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bar_node(
        &mut self,
        view: ViewId,
        parent: i32,
        role: i32,
        flags: i32,
        name: String,
        rect: [f32; 4],
        info: BarNodeInfo,
    ) -> i32 {
        let id = self.unique(info.id);
        self.out.push(Node {
            id,
            parent,
            view,
            role,
            flags: flags | if info.disabled { flag::DISABLED } else { 0 },
            rect,
            range: [0.0; 3],
            set: (0, 0),
            grid: [0; 4],
            name,
            value: String::new(),
            hint: info.hint,
        });
        id
    }
}

/// A detached surface's presented rows and kernel data, retained for a native query.
#[derive(Clone)]
pub struct PopupAccess {
    content: Rc<PopupContent>,
    focus: Option<ViewId>,
}

impl PartialEq for PopupAccess {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.content, &other.content) && self.focus == other.focus
    }
}

struct PopupContent {
    layout: (u32, u32, f32),
    epoch: u64,
    kind: SurfaceKind,
    select: bool,
    node: ViewId,
    plan: Plan,
    size: (f32, f32),
    clip: Option<Rect4>,
    scroll: Option<(f32, f32)>,
    targets: Vec<Target>,
    texts: Vec<(ViewId, Rect4)>,
    ancestors: Vec<Row>,
    scroll_label: String,
    metadata: Rc<RefCell<PopupMetadata>>,
    nodes: OnceCell<Vec<Node>>,
}

#[derive(Default)]
struct PopupMetadata {
    info: BTreeMap<ViewId, BarNodeInfo>,
    menu_flags: BTreeMap<ViewId, i32>,
    menu_targets: BTreeMap<exact_kernel::NodeKey, bool>,
}

impl PopupAccess {
    /// Build the popup's nodes when a service asks. Focus can change without rebuilding the rows.
    pub fn nodes(&self) -> (&[Node], Option<i32>, SurfaceKind) {
        let content = &self.content;
        let nodes = content.nodes.get_or_init(|| {
            let shown = Shown {
                serial: content.layout.0,
                revision: content.layout.1,
                kind: content.kind,
                node: content.node,
                plan: &content.plan,
                size: content.size,
                clip: content.clip,
                scroll: content.scroll,
                targets: &content.targets,
                pressed: None,
                texts: content.texts.clone(),
            };
            let mut build = Build::default();
            let metadata = content.metadata.borrow();
            build_surface(
                &mut build,
                &shown,
                content.select,
                &content.ancestors,
                || content.scroll_label.clone(),
                |view| metadata.info[&view].clone(),
                |row| metadata.menu_flags[&row.node],
            );
            build.out
        });
        let focus = self
            .focus
            .and_then(|view| nodes.iter().find(|node| node.view == view))
            .map(|node| node.id);
        (nodes, focus, content.kind)
    }
}

impl<D: DataSource> Presenter<D> {
    /// Retain the detached surface without publishing or building an accessibility tree.
    pub fn popup_access(&self, previous: Option<&PopupAccess>) -> Option<PopupAccess> {
        let layout = self.popup_layout()?;
        let epoch = self.host.kernel().epoch();
        let focus = self.desktop_surface_access().and_then(|(view, _)| view);
        let previous =
            previous.filter(|p| p.content.layout.0 == layout.0 && p.content.epoch == epoch);
        if let Some(previous) = previous.filter(|p| p.content.layout == layout) {
            return Some(PopupAccess {
                content: previous.content.clone(),
                focus,
            });
        }
        let shown = self.surface()?;
        let select = self.select_surface().is_some();
        let ancestors = previous
            .map(|p| p.content.ancestors.clone())
            .unwrap_or_else(|| self.status_menu_ancestors());
        let has_container = select || shown.scroll.is_some() || shown.kind == SurfaceKind::Alert;
        let metadata = previous
            .map(|p| p.content.metadata.clone())
            .unwrap_or_default();
        {
            let PopupMetadata {
                info,
                menu_flags,
                menu_targets,
            } = &mut *metadata.borrow_mut();
            let mut remember = |view| {
                info.entry(view)
                    .or_insert_with(|| self.access_bar_info(view));
            };
            for view in ancestors
                .iter()
                .map(|row| row.node)
                .chain(has_container.then_some(shown.node))
            {
                remember(view);
            }
            visit_visible_rows(&shown, |item, _| match item {
                VisibleRow::Heading(view, _) => remember(view),
                VisibleRow::Item(_, row) => {
                    remember(row.node);
                    if !select {
                        menu_flags
                            .entry(row.node)
                            .or_insert_with(|| self.surface_menu_flags(row, menu_targets));
                    }
                }
            });
        }
        let content = PopupContent {
            layout,
            epoch,
            kind: shown.kind,
            select,
            node: shown.node,
            plan: shown.plan.clone(),
            size: shown.size,
            clip: shown.clip,
            scroll: shown.scroll,
            targets: shown.targets.to_vec(),
            texts: shown.texts,
            ancestors,
            scroll_label: if has_container {
                self.surface_scroll_label(shown.node)
            } else {
                String::new()
            },
            metadata,
            nodes: OnceCell::new(),
        };
        Some(PopupAccess {
            content: Rc::new(content),
            focus,
        })
    }

    pub(super) fn access_surface(&self, b: &mut Build) -> u32 {
        let Some(shown) = self.surface() else {
            return 0;
        };
        let menu_targets = RefCell::new(BTreeMap::new());
        build_surface(
            b,
            &shown,
            self.select_surface().is_some(),
            &self.status_menu_ancestors(),
            || self.surface_scroll_label(shown.node),
            |view| self.access_bar_info(view),
            |row| self.surface_menu_flags(row, &mut menu_targets.borrow_mut()),
        )
    }

    fn surface_scroll_label(&self, view: ViewId) -> String {
        self.host
            .kernel()
            .node(view)
            .and_then(|node| self.access_label(&node))
            .unwrap_or_default()
    }

    fn surface_menu_flags(
        &self,
        row: &Row,
        menu_targets: &mut BTreeMap<exact_kernel::NodeKey, bool>,
    ) -> i32 {
        let mut flags = 0;
        if self
            .host
            .kernel()
            .node(row.node)
            .is_some_and(|n| n.props.str(PropId::AccessibilityRole) == Some("menuitemcheckbox"))
        {
            flags |= flag::CHECKABLE;
        }
        if let Some(key) = row.opens {
            let opens_menu = *menu_targets.entry(key).or_insert_with(|| {
                self.host.kernel().node_by_key(key).is_some_and(|target| {
                    matches!(self.surface_plan(target.id, false), Some(Plan::Menu(_)))
                })
            });
            if opens_menu {
                flags |= flag::EXPANDABLE;
            }
        }
        flags
    }
}

/// The open surface's nodes, in its own window's points: an alert's
/// headline, its supporting text and its buttons; a menu's items, each
/// section's title a heading before its items. A select's menu is a
/// listbox of its options, each placed in the set, the chosen one
/// selected, as HTML-AAM maps a `select`'s popup and as `access/menu.rs`
/// tells the painted menu.
fn build_surface(
    b: &mut Build,
    shown: &Shown<'_>,
    select: bool,
    ancestors: &[Row],
    scroll_label: impl FnOnce() -> String,
    info: impl Fn(ViewId) -> BarNodeInfo,
    menu_flags: impl Fn(&Row) -> i32,
) -> u32 {
    let whole = [0.0, 0.0, shown.size.0, shown.size.1];
    let scroll = shown.scroll.or(select.then_some((0., 0.)));
    // A detached submenu replaces the painted menu, but its expanded
    // ancestors keep their identities so a service can collapse them.
    let mut ancestor = 0;
    for row in ancestors {
        ancestor = b.bar_node(
            row.node,
            ancestor,
            role::BUTTON,
            flag::CLICK | flag::EXPANDABLE | flag::EXPANDED,
            row.title.clone(),
            [0.; 4],
            info(row.node),
        );
    }
    let alert = shown.kind == SurfaceKind::Alert;
    let mut name = if alert || scroll.is_some() {
        scroll_label()
    } else {
        String::new()
    };
    if alert {
        ancestor = b.bar_node(
            shown.node,
            ancestor,
            role::ALERT_DIALOG,
            flag::MODAL,
            std::mem::take(&mut name),
            whole,
            info(shown.node),
        );
    }
    let parent = if let Some((offset, max)) = scroll {
        let id = b.unique(info(shown.node).id);
        let flags = if offset > 0. { flag::BACKWARD } else { 0 }
            | if offset < max { flag::FORWARD } else { 0 };
        b.out.push(Node {
            id,
            parent: ancestor,
            view: shown.node,
            role: if select {
                role::LIST_BOX
            } else {
                role::SCROLLER
            },
            flags,
            rect: whole,
            range: [offset, max, 0.],
            set: (0, 0),
            grid: [0; 4],
            name,
            value: String::new(),
            hint: String::new(),
        });
        id
    } else {
        ancestor
    };
    if let Plan::Alert { title, message, .. } = shown.plan {
        let words = [
            (HEADLINE, role::HEADING, title),
            (SUPPORTING, role::TEXT, message),
        ];
        let mut texts = shown.texts.iter().filter(|(node, _)| *node == shown.node);
        for (id, role, text) in words {
            let Some(text) = text else {
                continue;
            };
            let Some((_, rect)) = texts.next() else {
                continue;
            };
            let mut rect = *rect;
            if id == SUPPORTING {
                for (_, next) in texts.by_ref().filter(|(_, rect)| rect.3 > 0.) {
                    if rect.3 <= 0. {
                        rect = *next;
                        continue;
                    }
                    let right = (rect.0 + rect.2).max(next.0 + next.2);
                    let bottom = (rect.1 + rect.3).max(next.1 + next.3);
                    rect.0 = rect.0.min(next.0);
                    rect.1 = rect.1.min(next.1);
                    rect.2 = right - rect.0;
                    rect.3 = bottom - rect.1;
                }
            }
            if rect.3 <= 0. {
                continue;
            }
            let id = b.unique(id);
            b.out.push(Node {
                id,
                parent,
                view: 0,
                role,
                flags: 0,
                rect: [rect.0, rect.1, rect.2, rect.3],
                range: [0.0; 3],
                set: (0, 0),
                grid: [0; 4],
                name: text.clone(),
                value: String::new(),
                hint: String::new(),
            });
        }
    }
    visit_visible_rows(shown, |item, rect| {
        let rect = [rect.0, rect.1, rect.2, rect.3];
        let (index, row) = match item {
            VisibleRow::Heading(node, title) => {
                b.bar_node(
                    node,
                    parent,
                    role::HEADING,
                    0,
                    title.to_owned(),
                    rect,
                    info(node),
                );
                return;
            }
            VisibleRow::Item(index, row) => (index, row),
        };
        let mut flags = if row.enabled {
            flag::CLICK
        } else {
            flag::DISABLED
        };
        match (select, row.checked) {
            (true, true) => flags |= flag::SELECTED,
            (false, true) => flags |= flag::CHECKABLE | flag::CHECKED,
            _ => {}
        }
        if !select {
            flags |= menu_flags(row);
        }
        let role = if select { role::OPTION } else { role::BUTTON };
        b.bar_node(
            row.node,
            parent,
            role,
            flags,
            row.title.clone(),
            rect,
            info(row.node),
        );
        if select {
            let count = shown.targets.len() as i32;
            b.out.last_mut().unwrap().set = (index as i32 + 1, count);
        }
    });
    shown.serial
}

enum VisibleRow<'a> {
    Heading(ViewId, &'a str),
    Item(usize, &'a Row),
}

fn visit_visible_rows(shown: &Shown<'_>, mut visit: impl FnMut(VisibleRow<'_>, Rect4)) {
    let clip = |rect| shown.clip.map_or(Some(rect), |clip| intersect(rect, clip));
    // A section title is presented before the item it heads.
    let mut headers: Vec<(usize, &String, ViewId)> = Vec::new();
    if let Plan::Menu(entries) = shown.plan {
        let mut items = 0;
        for entry in entries {
            match entry {
                Entry::Item(_) => items += 1,
                Entry::Header(title, node, _) => headers.push((items, title, *node)),
                Entry::Separator => {}
            }
        }
    }
    let text_rects = if headers.is_empty() {
        BTreeMap::new()
    } else {
        shown.texts.iter().rev().copied().collect()
    };
    let mut headers = headers.into_iter().peekable();
    for (index, (row, target)) in shown.plan.rows().zip(shown.targets).enumerate() {
        while let Some((_, title, node)) = headers.next_if(|h| h.0 == index) {
            if let Some(rect) = text_rects.get(&node).copied().and_then(clip) {
                visit(VisibleRow::Heading(node, title), rect);
            }
        }
        if let Some(rect) = clip(target.rect) {
            visit(VisibleRow::Item(index, row), rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_access_reads_metadata_only_for_exposed_rows() {
        let rows = (0..30)
            .map(|i| format!("        button press=pick({i})\n          text \"Item {i}\"\n"))
            .collect::<String>();
        let source = format!(
            "component App
  state selected = 0
  action pick(item: number)
    selected = item
  view
    column
      button id=\"tray\" -exact-menu=\"status\" popovertarget=\"menu\"
        text \"Ember\"
      column id=\"menu\" popover=\"auto\" role=\"menu\"
{rows}"
        );
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (300., 400.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.serve_overlays();
        p.frame();
        let kernel = p.host.kernel();
        let tray = kernel.node_by_key(kernel.find_by_id("tray")[0]).unwrap().id;
        assert!(p.open_status_menu(tray, (300., 120.), false, false, 0.));
        let shown = p.surface().unwrap();
        let first = shown.targets.first().unwrap().node;
        let last = shown.targets.last().unwrap().node;
        let initial = p.popup_access(None).unwrap();
        let metadata = &initial.content.metadata;
        assert!(initial.content.nodes.get().is_none());
        assert!(metadata.borrow().info.contains_key(&first));
        assert!(!metadata.borrow().info.contains_key(&last));

        p.status_menu_wheel(f32::MAX);
        let scrolled = p.popup_access(Some(&initial)).unwrap();
        assert!(Rc::ptr_eq(metadata, &scrolled.content.metadata));
        assert!(metadata.borrow().info.contains_key(&last));
        let end = scrolled.nodes().0;
        assert!(end.iter().any(|node| node.view == last));
        assert!(!end.iter().any(|node| node.view == first));
        let start = initial.nodes().0;
        assert!(start.iter().any(|node| node.view == first));
        assert!(!start.iter().any(|node| node.view == last));
    }

    #[test]
    fn popup_access_caches_submenu_targets_per_key_and_reuses_ancestors() {
        let triggers = (0..6)
            .map(|i| {
                format!(
                    "        button id=\"open-{i}\" popovertarget=\"sub\"\n          text \"Open {i}\"\n"
                )
            })
            .collect::<String>();
        let sub_rows = (0..30)
            .map(|i| format!("        button press=pick({i})\n          text \"Sub {i}\"\n"))
            .collect::<String>();
        let source = format!(
            "component App
  state selected = 0
  action pick(item: number)
    selected = item
  view
    column
      button id=\"tray\" -exact-menu=\"status\" popovertarget=\"menu\"
        text \"Ember\"
      column id=\"menu\" popover=\"auto\" role=\"menu\"
{triggers}      column id=\"sub\" popover=\"auto\" role=\"menu\"
{sub_rows}"
        );
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (300., 400.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.serve_overlays();
        p.frame();
        let kernel = p.host.kernel();
        let tray = kernel.node_by_key(kernel.find_by_id("tray")[0]).unwrap().id;
        let sub_key = kernel.find_by_id("sub")[0];
        assert!(p.open_status_menu(tray, (300., 180.), false, false, 0.));
        let root = p.popup_access(None).unwrap();
        let meta = root.content.metadata.borrow();
        // Six buttons share one submenu target: classified once per NodeKey.
        assert_eq!(meta.menu_targets.len(), 1);
        assert_eq!(meta.menu_targets.get(&sub_key), Some(&true));
        assert!(meta
            .menu_flags
            .values()
            .all(|f| (*f & flag::EXPANDABLE) != 0));
        drop(meta);
        // Opening the submenu and scrolling retains its ancestor row while serial and epoch match.
        let trigger = p.surface().unwrap().targets[0].node;
        let _ = p.tap(trigger);
        let sub_initial = p.popup_access(Some(&root)).unwrap();
        assert_eq!(sub_initial.content.ancestors.len(), 1);
        assert_eq!(sub_initial.content.ancestors[0].node, trigger);
        p.status_menu_wheel(f32::MAX);
        let sub_scrolled = p.popup_access(Some(&sub_initial)).unwrap();
        assert_eq!(sub_scrolled.content.ancestors.len(), 1);
        assert_eq!(sub_scrolled.content.ancestors[0].node, trigger);
    }
}
