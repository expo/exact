//! AccessKit's window tree over the painted presenter (LLP 1115.004 D11).
mod node;
#[cfg(test)]
mod tests;
mod text;
use self::text::FieldRuns;
use super::access::{act, role};
use crate::Presenter;
use ::accesskit::{
    Action, ActionData, Affine, Node, NodeId, Rect, Role, ScrollHint, TreeId, TreeInfo, TreeUpdate,
};
use exact_kernel::PropId;
use exact_runner::{DataSource, FieldSelection, SelectionDirection};
use node::set_heading_level;
pub use node::{map_node, scroll_target};
use std::collections::BTreeMap;

const ROOT: NodeId = NodeId(0);

/// The last statement delivered to the window's assistive client.
pub struct WindowTree {
    nodes: BTreeMap<NodeId, Node>,
    runs: BTreeMap<NodeId, FieldRuns>,
    focus: NodeId,
}

impl Default for WindowTree {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            runs: BTreeMap::new(),
            focus: ROOT,
        }
    }
}

impl WindowTree {
    /// The title in the last published tree.
    pub fn title(&self) -> Option<&str> {
        self.nodes.get(&ROOT)?.label()
    }

    /// Publish a window title change that arrives between paints.
    pub fn set_title(&mut self, title: &str) -> Option<TreeUpdate> {
        let root = self.nodes.get_mut(&ROOT)?;
        if root.label() == Some(title) {
            return None;
        }
        root.set_label(title);
        Some(TreeUpdate {
            nodes: vec![(ROOT, root.clone())],
            tree: None,
            tree_id: TreeId::ROOT,
            focus: self.focus,
        })
    }

    /// Return the changes since the previous statement and whether any nodes or focus changed.
    /// A detached popup has its own window tree. Embedded fields have native providers.
    pub fn update<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        title: &str,
        size: (f64, f64),
        scale: f64,
        detached: bool,
        embedded: impl Fn(u32) -> bool,
    ) -> (TreeUpdate, bool) {
        p.reader_access(1., self.nodes.is_empty());
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
        root.set_bounds(Rect::new(0., 0., size.0, size.1));
        root.set_transform(Affine::scale(scale));
        root.set_children(children_by_parent.remove(&0).unwrap_or_default());
        next.insert(ROOT, root);
        let mut runs = BTreeMap::new();
        let mut changed = Vec::new();
        for source in sources {
            let id = NodeId(source.id as u64);
            let props = p.host().kernel().node(source.view).map(|n| n.props);
            let password = props.is_some_and(|props| props.str(PropId::Type) == Some("password"));
            let multiline =
                props.is_some_and(|props| props.str(PropId::SemanticTag) == Some("textarea"));
            let readonly = props.is_some_and(|props| props.bool(PropId::Editable) == Some(false));
            let mut node = map_node(source, origin, password, multiline, readonly);
            if source.role == role::TAB
                && props.and_then(|props| props.bool(PropId::AccessibilitySelected)) == Some(false)
            {
                node.set_selected(false);
            }
            let field = matches!(source.role, role::EDIT | role::FIELD) && !password;
            let selection = if field {
                p.access_editor(source).map(|(value, selection)| {
                    node.set_value(value);
                    selection
                })
            } else {
                None
            };
            if source.role == role::SLIDER {
                if let Some(props) = props.filter(|p| p.str(PropId::Type) == Some("range")) {
                    let (small, large) = exact_kernel::Range::of(props).keyboard_steps();
                    node.set_numeric_value_step(small);
                    node.set_numeric_value_jump(large);
                }
            }
            if source.role == role::HEADING {
                set_heading_level(&mut node, props.and_then(|props| props.heading()));
            }
            let native = embedded(source.view);
            if native {
                node.set_role(Role::Group);
                node.clear_value();
            }
            node.set_children(children_by_parent.remove(&source.id).unwrap_or_default());
            if field && !native {
                let previous = self
                    .nodes
                    .get(&id)
                    .and_then(Node::value)
                    .zip(self.runs.remove(&id));
                let value = node.value().unwrap_or_default();
                let layout = p.access_text_layout(source.view, value);
                let field = FieldRuns::update(id, value, layout, previous, &mut changed);
                if let Some(selection) = selection {
                    let (anchor, focus) = if selection.direction == SelectionDirection::Backward {
                        (selection.end, selection.start)
                    } else {
                        (selection.start, selection.end)
                    };
                    node.set_text_selection(accesskit::TextSelection {
                        anchor: field.position(id, anchor),
                        focus: field.position(id, focus),
                    });
                }
                let children = (0..field.runs.len())
                    .map(|i| FieldRuns::id(id, i))
                    .collect::<Vec<_>>();
                if let Some((mut transform, clip)) = field
                    .layout
                    .as_ref()
                    .and_then(|l| l.transform.map(|t| (t, l.clip)))
                {
                    // A scroll changes the text layer's placement. Every glyph remains
                    // in its original paragraph coordinates.
                    let layer_id = NodeId(id.0 << 32);
                    let mut layer = Node::new(Role::GenericContainer);
                    transform[4] += f64::from(origin.0);
                    transform[5] += f64::from(origin.1);
                    let (x, y, w, h) = clip;
                    layer.set_transform(Affine::new(transform));
                    layer.set_bounds(Rect::new(
                        x.into(),
                        y.into(),
                        (x + w).into(),
                        (y + h).into(),
                    ));
                    layer.set_clips_children();
                    layer.set_children(children);
                    next.insert(layer_id, layer);
                    node.set_clips_children();
                    node.set_children([layer_id]);
                } else {
                    node.set_children(children);
                }
                runs.insert(id, field);
            }
            next.insert(id, node);
        }
        changed.extend(
            next.iter()
                .filter(|(id, node)| self.nodes.get(id) != Some(node))
                .map(|(id, node)| (*id, node.clone())),
        );
        let has_changes = !changed.is_empty() || focus != self.focus;
        let update = TreeUpdate {
            nodes: changed,
            tree: self.nodes.is_empty().then(|| TreeInfo::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus,
        };
        self.nodes = next;
        self.runs = runs;
        self.focus = focus;
        (update, has_changes)
    }

    /// Apply a request against the latest published nodes and text runs.
    pub fn action<D: DataSource>(
        &self,
        p: &mut Presenter<D>,
        request: ::accesskit::ActionRequest,
        line: f32,
    ) {
        if request.target_tree != TreeId::ROOT {
            return;
        }
        if let (Action::ScrollIntoView, Some(ActionData::ScrollHint(hint))) =
            (request.action, &request.data)
        {
            let run = request.target_node;
            let owner = NodeId(run.0 >> 32);
            let Some(line) = self.runs.get(&owner).and_then(|runs| runs.run(owner, run)) else {
                return;
            };
            if matches!(hint, ScrollHint::TopEdge | ScrollHint::BottomEdge) {
                p.access_scroll_text_line(owner.0 as i32, line.start, *hint == ScrollHint::TopEdge);
            }
            return;
        }
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
            (Action::CustomAction, Some(ActionData::CustomAction(target))) => {
                p.access_custom_action(id, target as u32, now);
            }
            (Action::Increment, _) => {
                p.access_act(id, act::FORWARD, 0., now);
            }
            (Action::Decrement, _) => {
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
                let Some(runs) = self.runs.get(&request.target_node) else {
                    return;
                };
                if let (Some(anchor), Some(focus)) = (
                    runs.offset(id, selection.anchor),
                    runs.offset(id, selection.focus),
                ) {
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
            (action, data) => {
                if let Some(to) = scroll_target(node, action, data.as_ref(), line) {
                    p.access_scroll_to(id, to);
                }
            }
        }
    }
}
