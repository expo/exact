//! UI Automation over the presenter's accessibility model; dormant until requested.
use accesskit::{
    Action, ActionData, Affine, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate,
};
use accesskit_winit::{Adapter, WindowEvent as AccessEvent};
use exact_kernel::PropId;
use exact_painted::{
    presenter::access::{act, role},
    Presenter,
};
use exact_runner::{DataSource, FieldSelection, SelectionDirection};
use std::collections::BTreeMap;
use unicode_segmentation::UnicodeSegmentation;
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoopProxy},
    window::Window,
};

pub(super) enum Event {
    Wake,
    Close,
    /// The app's bare `focus()`.
    Focus,
    /// The app's `hideWindow()`.
    Hide,
    ModalWake,
    Timer(std::sync::Arc<crate::timers::Fired>),
    Access(accesskit_winit::Event),
    TrayAccess(accesskit::ActionRequest),
}
impl From<accesskit_winit::Event> for Event {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event)
    }
}

mod model;
use model::map_node;

const ROOT: NodeId = NodeId(0);

pub(super) struct Access {
    adapter: Adapter,
    active: bool,
    nodes: BTreeMap<NodeId, Node>,
    runs: BTreeMap<NodeId, (i32, Vec<u32>)>,
    focus: NodeId,
    embedded: BTreeMap<u32, (NodeId, u32)>,
}

impl Access {
    pub fn new(events: &ActiveEventLoop, window: &Window, proxy: EventLoopProxy<Event>) -> Self {
        Self {
            adapter: Adapter::with_event_loop_proxy(events, window, proxy),
            active: false,
            nodes: BTreeMap::new(),
            runs: BTreeMap::new(),
            focus: ROOT,
            embedded: BTreeMap::new(),
        }
    }

    pub fn process_event(&mut self, window: &Window, event: &WindowEvent) {
        self.adapter.process_event(window, event);
    }

    pub fn event<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        window: &Window,
        title: &str,
        event: AccessEvent,
    ) {
        match event {
            AccessEvent::InitialTreeRequested => {
                self.active = true;
                self.nodes.clear();
                p.access_reset();
            }
            AccessEvent::AccessibilityDeactivated => {
                self.active = false;
                self.nodes.clear();
                self.runs.clear();
                p.access_reset();
                return;
            }
            AccessEvent::ActionRequested(request) => {
                if !self.active || request.target_tree != TreeId::ROOT {
                    return;
                }
                self.sync(p, window, title);
                let Some(node) = self.nodes.get(&request.target_node) else {
                    return;
                };
                if node.is_disabled() || !node.supports_action(request.action) {
                    return;
                }
                let Ok(id) = i32::try_from(request.target_node.0) else {
                    return;
                };
                let now = p.host().now();
                match (request.action, request.data) {
                    (Action::Focus, _) => {
                        p.access_focus_node(id, now);
                    }
                    // A select's Expand and Collapse press it: the press
                    // opens its menu, and closes it while open.
                    (Action::Click | Action::Expand | Action::Collapse, _) => {
                        p.access_act(id, act::CLICK, 0., now);
                    }
                    (Action::ShowContextMenu, _) => {
                        p.access_act(id, act::LONG_CLICK, 0., now);
                    }
                    (Action::Increment | Action::ScrollDown | Action::ScrollRight, _) => {
                        p.access_act(id, act::FORWARD, 0., now);
                    }
                    (Action::Decrement | Action::ScrollUp | Action::ScrollLeft, _) => {
                        p.access_act(id, act::BACKWARD, 0., now);
                    }
                    (Action::SetValue, Some(ActionData::NumericValue(value)))
                        if (value as f32).is_finite() =>
                    {
                        p.access_act(id, act::SET, value as f32, now);
                    }
                    (Action::SetValue, Some(ActionData::Value(value))) => {
                        if let Some(view) = p.access_view(id) {
                            if let Err(error) = p.type_text(view, &value) {
                                p.host_mut().log(error);
                            }
                        }
                    }
                    (Action::SetTextSelection, Some(ActionData::SetTextSelection(selection))) => {
                        let position = |at: accesskit::TextPosition| {
                            let (owner, offsets) = self.runs.get(&at.node)?;
                            (*owner == id)
                                .then(|| offsets.get(at.character_index).copied())
                                .flatten()
                        };
                        if let (Some(anchor), Some(focus)) =
                            (position(selection.anchor), position(selection.focus))
                        {
                            p.access_select(
                                id,
                                FieldSelection {
                                    start: anchor.min(focus),
                                    end: anchor.max(focus),
                                    direction: if focus < anchor {
                                        SelectionDirection::Backward
                                    } else {
                                        SelectionDirection::Forward
                                    },
                                },
                                now,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        self.sync(p, window, title);
    }

    pub fn sync<D: DataSource>(&mut self, p: &mut Presenter<D>, window: &Window, title: &str) {
        if !self.active {
            return;
        }
        p.reader_access(1., self.nodes.is_empty());
        let detached = p.status_menu().is_some();
        let sources = if detached { &[][..] } else { p.access_told() };
        let origin = p.access_origin();
        let focus = if detached {
            ROOT
        } else {
            p.access_focus().map_or(ROOT, |id| NodeId(id as u64))
        };
        let mut next = BTreeMap::new();
        let mut children_by_parent: BTreeMap<i32, Vec<NodeId>> = BTreeMap::new();
        for source in sources {
            children_by_parent
                .entry(source.parent)
                .or_default()
                .push(NodeId(source.id as u64));
        }
        let mut root = Node::new(Role::Window);
        root.set_label(title);
        let size = window.inner_size().to_logical::<f64>(window.scale_factor());
        root.set_bounds(Rect::new(0., 0., size.width, size.height));
        root.set_transform(Affine::scale(window.scale_factor()));
        root.set_children(children_by_parent.remove(&0).unwrap_or_default());
        next.insert(ROOT, root);
        let mut runs = BTreeMap::new();
        for source in sources {
            let mut source = source.clone();
            if let Some(value) = p.access_text(source.id) {
                source.value = value;
            }
            let id = NodeId(source.id as u64);
            let props = p.host().kernel().node(source.view).map(|n| n.props);
            let password = props.is_some_and(|props| props.str(PropId::Type) == Some("password"));
            let multiline =
                props.is_some_and(|props| props.str(PropId::SemanticTag) == Some("textarea"));
            let readonly = props.is_some_and(|props| props.bool(PropId::Editable) == Some(false));
            let mut node = map_node(&source, origin, password, multiline, readonly);
            let native = self
                .embedded
                .get(&source.view)
                .is_some_and(|(_, generation)| {
                    exact_ink::device::d3d11::fields::identity(source.view) == Some(*generation)
                });
            if native {
                node.set_role(Role::Group);
                node.clear_value();
            }
            node.set_children(children_by_parent.remove(&source.id).unwrap_or_default());
            if matches!(source.role, role::EDIT | role::FIELD) && !password && !native {
                let mut offset = 0;
                let mut lines: Vec<_> = source.value.split_inclusive('\n').collect();
                if source.value.is_empty() || source.value.ends_with('\n') {
                    lines.push("");
                }
                let mut children = Vec::new();
                for (index, line) in lines.into_iter().enumerate() {
                    let run_id = NodeId((id.0 << 32) | (index as u64 + 1));
                    let mut run = Node::new(Role::TextRun);
                    run.set_value(line);
                    let (lengths, offsets) = text_offsets(line, offset);
                    offset = *offsets.last().unwrap();
                    run.set_character_lengths(lengths);
                    runs.insert(run_id, (source.id, offsets));
                    children.push(run_id);
                    next.insert(run_id, run);
                }
                if let Some(selection) = p.access_selection(source.id) {
                    let position = |offset| {
                        let id = children
                            .iter()
                            .rev()
                            .find(|id| runs[id].1[0] <= offset)
                            .unwrap();
                        let offsets = &runs[id].1;
                        accesskit::TextPosition {
                            node: *id,
                            character_index: offsets
                                .partition_point(|n| *n < offset)
                                .min(offsets.len() - 1),
                        }
                    };
                    let (anchor, focus) = if selection.direction == SelectionDirection::Backward {
                        (selection.end, selection.start)
                    } else {
                        (selection.start, selection.end)
                    };
                    node.set_text_selection(accesskit::TextSelection {
                        anchor: position(anchor),
                        focus: position(focus),
                    });
                }
                node.set_children(children);
            }
            next.insert(id, node);
        }
        let changed = next
            .iter()
            .filter(|(id, node)| self.nodes.get(id) != Some(node))
            .map(|(id, node)| (*id, node.clone()))
            .collect::<Vec<_>>();
        if !changed.is_empty() || focus != self.focus {
            self.adapter.update_if_active(|| TreeUpdate {
                nodes: changed,
                tree: Some(TreeInfo::new(ROOT)),
                tree_id: TreeId::ROOT,
                focus,
            });
        }
        self.nodes = next;
        self.runs = runs;
        self.focus = focus;
        self.embed_fields(p, window);
    }
    fn embed_fields<D: DataSource>(&mut self, p: &Presenter<D>, window: &Window) {
        if !exact_ink::device::d3d11::fields::enabled() {
            return;
        }
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = accesskit_windows::HWND(handle.hwnd.get() as *mut _);
        let candidates: BTreeMap<_, _> = p
            .access_told()
            .iter()
            .filter_map(|node| {
                exact_ink::device::d3d11::fields::identity(node.view)
                    .map(|generation| (node.view, (NodeId(node.id as u64), generation)))
            })
            .collect();
        let old = std::mem::take(&mut self.embedded);
        for (view, identity) in old {
            if candidates.get(&view) == Some(&identity) {
                self.embedded.insert(view, identity);
            } else {
                let _ = accesskit_windows::embed_windowless(hwnd, identity.0, None);
            }
        }
        for (view, (id, generation)) in candidates {
            if self.embedded.contains_key(&view) {
                continue;
            }
            let result = (|| -> Result<(), String> {
                let site =
                    accesskit_windows::windowless_site(hwnd, id).map_err(|e| e.to_string())?;
                let provider = exact_ink::device::d3d11::fields::provider(view, &site)
                    .map_err(|e| e.to_string())?;
                accesskit_windows::embed_windowless(hwnd, id, Some(&provider))
                    .map_err(|e| e.to_string())
            })();
            match result {
                Ok(()) => {
                    self.embedded.insert(view, (id, generation));
                    if std::env::var_os("EXACT_AGENT_WINDOW_READY").is_some() {
                        eprintln!("exact-windows: RichEdit UIA view {view} under {}", id.0);
                    }
                    window.request_redraw();
                }
                Err(error) => eprintln!("exact-windows: RichEdit UIA: {error}"),
            }
        }
    }
}

fn text_offsets(text: &str, mut offset: u32) -> (Vec<u8>, Vec<u32>) {
    let mut lengths = Vec::new();
    let mut offsets = vec![offset];
    for grapheme in text.graphemes(true) {
        // AccessKit's byte count is u8; an unbounded combining sequence must be split.
        let pieces: Vec<&str> = if grapheme.len() <= u8::MAX as usize {
            vec![grapheme]
        } else {
            grapheme
                .char_indices()
                .map(|(at, ch)| &grapheme[at..at + ch.len_utf8()])
                .collect()
        };
        for piece in pieces {
            lengths.push(piece.len() as u8);
            offset += piece.encode_utf16().count() as u32;
            offsets.push(offset);
        }
    }
    (lengths, offsets)
}

/// A raw popup uses the page's node mapping and AccessKit's HWND adapter.
pub(super) struct MenuAccess {
    adapter: accesskit_windows::SubclassingAdapter,
}

struct MenuActivation(EventLoopProxy<Event>);
impl accesskit::ActivationHandler for MenuActivation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let _ = self.0.send_event(Event::Wake);
        None
    }
}
struct MenuActions(EventLoopProxy<Event>);
impl accesskit::ActionHandler for MenuActions {
    fn do_action(&mut self, request: accesskit::ActionRequest) {
        let _ = self.0.send_event(Event::TrayAccess(request));
    }
}

impl MenuAccess {
    pub fn new(window: accesskit_windows::HWND, proxy: EventLoopProxy<Event>) -> Self {
        Self {
            adapter: accesskit_windows::SubclassingAdapter::new(
                window,
                MenuActivation(proxy.clone()),
                MenuActions(proxy),
            ),
        }
    }

    pub fn sync<D: DataSource>(&mut self, p: &mut Presenter<D>, scale: f64) {
        let Some(menu) = p.status_menu() else {
            return;
        };
        if let Some(events) = self.adapter.update_if_active(|| {
            p.reader_access(1., true);
            model::menu_tree(
                p.access_told(),
                &menu.label,
                menu.size,
                scale,
                p.access_focus(),
                true,
            )
        }) {
            events.raise();
        }
    }

    pub fn hide(&mut self) {
        if let Some(events) = self
            .adapter
            .update_if_active(|| model::menu_tree(&[], "", (0., 0.), 1., None, false))
        {
            events.raise();
        }
    }

    pub fn action<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        request: accesskit::ActionRequest,
    ) {
        model::menu_action(p, request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accessibility_text_offsets_preserve_graphemes_utf16_and_crlf() {
        let (lengths, offsets) = text_offsets("a🦀e\u{301}\r\n", 5);
        assert_eq!(lengths, [1, 4, 3, 2]);
        assert_eq!(offsets, [5, 6, 8, 10, 12]);
        let long = format!("a{}", "\u{301}".repeat(200));
        let (lengths, offsets) = text_offsets(&long, 0);
        assert_eq!(
            lengths.iter().map(|n| *n as usize).sum::<usize>(),
            long.len()
        );
        assert_eq!(offsets.last(), Some(&201));
        assert_eq!(text_offsets("", 5), (vec![], vec![5]));
    }
}
