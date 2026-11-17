//! Layout-only descendants of quiet UIKit collection rows (LLP 1171 D4).
use super::*;
use exact_kernel::{Display, StyleId};

#[cfg(test)]
#[path = "flat_tests.rs"]
mod tests;

pub(super) use crate::batch::HitRegion;

#[derive(Default)]
pub(super) struct Flat {
    enabled: bool,
    quiet: Vec<bool>,
    layout: Vec<bool>,
    pub(super) nodes: IdSet<ViewId>,
    pub(super) parents: IdSet<ViewId>,
    pub(super) order: Option<Vec<(ViewId, exact_kernel::paint_order::Placed)>>,
    /// Each owner's regions as last emitted, and whether a published batch
    /// carried them: a withheld layout keeps the regions, to withdraw from.
    pub(super) hits: IdMap<ViewId, (Vec<HitRegion>, bool)>,
    // The last logical paths distinguish promotion from an authored move.
    paths: IdMap<ViewId, Vec<ViewId>>,
}

impl Flat {
    pub(super) fn new(enabled: bool, plan: &exact_plan::Plan) -> Self {
        Self {
            enabled,
            layout: if enabled {
                plan.nodes
                    .iter()
                    .map(|node| {
                        node.bindings.iter().all(|b| {
                            let binding = plan.binding(b);
                            match binding.kind {
                                exact_plan::BindingKind::Style => {
                                    StyleId::from_bit(binding.id as u32).is_some_and(layout_row)
                                }
                                exact_plan::BindingKind::Prop => {
                                    PropId::from_wire(binding.id) == Some(PropId::SemanticTag)
                                }
                            }
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            },
            quiet: if enabled {
                quiet_templates(plan)
            } else {
                Vec::new()
            },
            ..Self::default()
        }
    }
}

impl<D: DataSource> Host<D> {
    pub(super) fn flat_agent(&self, id: ViewId, reply: &mut String) {
        use std::fmt::Write;
        if !self.flat.nodes.contains(&id) || !reply.ends_with('}') {
            return;
        }
        let kernel = self.runner.kernel();
        let Some((node, parent)) = kernel
            .node(id)
            .zip(self.presented_parent(id).and_then(|p| kernel.node(p)))
        else {
            return;
        };
        let (x, y, w, h) = relative(node.frame, Some(parent.frame));
        reply.pop();
        let _ = write!(
            reply,
            ",\"layoutOnly\":{{\"parent\":{},\"x\":{x},\"y\":{y},\"w\":{w},\"h\":{h}}}}}",
            parent.id
        );
    }

    pub(super) fn prepare_flat(&mut self, batch: &mut Batch) {
        if !self.flat.enabled || self.native_mode() {
            return;
        }
        let mut eligible = IdSet::default();
        for list in self.runner.collections() {
            let kernel = self.runner.kernel();
            let Some(node) = kernel.node(list.view) else {
                continue;
            };
            if node.style.appearance != exact_kernel::Appearance::None
                || node.props.get(PropId::ListStyle).is_some()
            {
                continue;
            }
            let mut above = Some(list.view);
            let mut excluded = false;
            while let Some(id) = above {
                let Some(node) = kernel.node(id) else {
                    excluded = true;
                    break;
                };
                if super::layout::closed::top_layer(&node)
                    || matches!(
                        node.props.str(PropId::AccessibilityRole),
                        Some("dialog" | "alertdialog")
                    )
                {
                    excluded = true;
                    break;
                }
                above = node.parent;
            }
            if excluded {
                continue;
            }
            for row in list.rows {
                let Some(site) = self.runner.plan_site_of(row.root) else {
                    continue;
                };
                if !self
                    .flat
                    .quiet
                    .get(site.0 as usize)
                    .copied()
                    .unwrap_or(false)
                {
                    continue;
                }
                let mut stack = vec![row.root];
                let mut candidates = Vec::new();
                let mut quiet = true;
                while let Some(id) = stack.pop() {
                    let Some(node) = kernel.node(id) else {
                        quiet = false;
                        break;
                    };
                    if !quiet_node(&node) || self.runner.handlers_of(id).iter().any(drag_event) {
                        quiet = false;
                        break;
                    }
                    // The generated wrapper and the authored root remain UIKit boxes.
                    let declared_layout = self
                        .runner
                        .plan_site_of(id)
                        .and_then(|site| self.flat.layout.get(site.0 as usize))
                        .copied()
                        .unwrap_or(false);
                    if id != row.root
                        && declared_layout
                        && layout_only(&node)
                        && !self.option_part(id)
                        && self.runner.handlers_of(id).is_empty()
                    {
                        candidates.push(id);
                    }
                    stack.extend(node.children());
                }
                if quiet {
                    eligible.extend(candidates)
                }
            }
        }
        let order = self.runner.kernel().paint_order();
        let mut promote = Vec::new();
        for &(id, placed) in &order {
            // Once mirrored, a box keeps its native lifetime.
            if self.mirror.contains_key(&id) {
                continue;
            }
            if eligible.contains(&id) && placed.rank == 0 && !placed.isolated && !placed.policy {
                self.flat.nodes.insert(id);
            } else if self.flat.nodes.contains(&id) {
                promote.push(id);
            }
        }
        let paths = if promote.is_empty() {
            IdMap::default()
        } else {
            self.flat_paths()
        };
        // Attach outer boxes before moving a held descendant into them.
        promote.sort_by_key(|id| {
            let mut depth = 0;
            let mut at = Some(*id);
            while let Some(id) = at {
                depth += 1;
                at = self.runner.kernel().node(id).and_then(|n| n.parent);
            }
            depth
        });
        let children: Vec<_> = promote
            .iter()
            .map(|id| self.flat_children(self.runner.kernel().node(*id).expect("live").children()))
            .collect();
        for id in &promote {
            self.flat.nodes.remove(id);
        }
        self.flat.order = Some(order);
        for (id, children) in promote.into_iter().zip(children) {
            let events = self.runner.handlers_of(id);
            self.create(id, &events, batch);
            if let Some(parent) = self.presented_parent(id) {
                let kernel = self.runner.kernel();
                let frame = relative(
                    kernel.node(id).expect("live").frame,
                    Some(kernel.node(parent).expect("live").frame),
                );
                let keep: Vec<_> = children
                    .iter()
                    .copied()
                    .filter(|child| {
                        paths
                            .get(child)
                            .is_some_and(|p| self.flat.paths.get(child) == Some(p))
                    })
                    .collect();
                let placed: Vec<_> = children
                    .iter()
                    .filter_map(|child| {
                        let child = kernel.node(*child)?;
                        let f = relative(child.frame, Some(kernel.node(id)?.frame));
                        Some((child.id, [f.0, f.1, f.2, f.3]))
                    })
                    .collect();
                batch.layout_promote(
                    id,
                    parent,
                    [frame.0, frame.1, frame.2, frame.3],
                    &placed,
                    &keep,
                );
            }
            self.queue_layout_subtree(id);
            self.flat.parents.insert(id);
            if let Some(parent) = self.presented_parent(id) {
                self.flat.parents.insert(parent);
            }
        }
        self.flat.paths = self.flat_paths();
    }

    fn flat_paths(&self) -> IdMap<ViewId, Vec<ViewId>> {
        let kernel = self.runner.kernel();
        let mut paths = IdMap::default();
        for id in &self.flat.nodes {
            let Some(node) = kernel.node(*id) else {
                continue;
            };
            for child in node.children() {
                if self.flat.nodes.contains(&child) {
                    continue;
                }
                let mut path = Vec::new();
                let mut at = Some(*id);
                while let Some(id) = at {
                    path.push(id);
                    if !self.flat.nodes.contains(&id) {
                        break;
                    }
                    at = kernel.node(id).and_then(|n| n.parent);
                }
                paths.insert(child, path);
            }
        }
        paths
    }

    pub(super) fn presented_parent(&self, id: ViewId) -> Option<ViewId> {
        let kernel = self.runner.kernel();
        let mut parent = kernel.node(id)?.parent;
        while let Some(id) = parent {
            if !self.flat.nodes.contains(&id) {
                return Some(id);
            }
            parent = kernel.node(id)?.parent;
        }
        None
    }

    pub(super) fn flat_children(&self, children: Vec<ViewId>) -> Vec<ViewId> {
        if self.flat.nodes.is_empty() {
            return children;
        }
        let mut out = Vec::new();
        let mut stack: Vec<_> = children.into_iter().rev().collect();
        while let Some(id) = stack.pop() {
            if self.flat.nodes.contains(&id) {
                if let Some(node) = self.runner.kernel().node(id) {
                    stack.extend(node.children().into_iter().rev());
                }
            } else {
                out.push(id);
            }
        }
        out
    }

    pub(super) fn queue_flat_descendants(&mut self) {
        if self.flat.nodes.is_empty() {
            return;
        }
        let flat: Vec<_> = self
            .pending_layout
            .iter()
            .filter_map(|key| {
                let node = self.runner.kernel().node_by_key(*key)?;
                self.flat.nodes.contains(&node.id).then_some(node.id)
            })
            .collect();
        for id in flat {
            let children = self.runner.kernel().node(id).expect("live").children();
            for child in self.flat_children(children) {
                self.queue_layout(child);
            }
        }
    }

    /// Withdraws destroyed omitted boxes' hit regions in the commit that
    /// destroys them, so a contact one holds ends even when layout then fails
    /// and its geometry, `layoutHits` included, is withheld.
    pub(super) fn withdraw_flat_hits(&mut self, gone: &IdSet<ViewId>, batch: &mut Batch) {
        if gone.is_empty() {
            return;
        }
        let kernel = self.runner.kernel();
        // A destroyed owner's view ends its own contacts.
        self.flat
            .hits
            .retain(|owner, _| kernel.node(*owner).is_some());
        for (owner, (hits, _)) in self.flat.hits.iter_mut() {
            let ids: Vec<_> = hits
                .iter()
                .map(|hit| hit.2)
                .filter(|id| gone.contains(id))
                .collect();
            if !ids.is_empty() {
                hits.retain(|hit| !gone.contains(&hit.2));
                batch.layout_hits_withdrawn(*owner, &ids);
            }
        }
    }

    pub(super) fn emit_flat_hits(&mut self, batch: &mut Batch) {
        let mut parents: IdSet<_> = self
            .flat
            .nodes
            .iter()
            .filter_map(|id| self.presented_parent(*id))
            .collect();
        parents.extend(self.flat.hits.keys().copied());
        for id in parents {
            let Some(parent) = self.runner.kernel().node(id) else {
                self.flat.hits.remove(&id);
                continue;
            };
            // `Err(i)` closes box `i` once its descendants are walked.
            let mut stack: Vec<Result<_, usize>> =
                parent.children().into_iter().rev().map(Ok).collect();
            let mut children = Vec::new();
            let mut boxes: Vec<(usize, [f32; 4], ViewId, usize)> = Vec::new();
            while let Some(entry) = stack.pop() {
                let id = match entry {
                    Ok(id) => id,
                    Err(i) => {
                        boxes[i].3 = children.len();
                        continue;
                    }
                };
                let Some(node) = self.runner.kernel().node(id) else {
                    continue;
                };
                if self.flat.nodes.contains(&id) {
                    let (x, y, w, h) = relative(node.frame, Some(parent.frame));
                    stack.push(Err(boxes.len()));
                    boxes.push((children.len(), [x, y, w, h], id, children.len()));
                    stack.extend(node.children().into_iter().rev().map(Ok));
                } else {
                    children.push(id);
                }
            }
            let hits: Vec<_> = boxes
                .into_iter()
                .map(|(i, rect, id, end)| {
                    (
                        children.get(i).copied(),
                        rect,
                        id,
                        children.get(end).copied(),
                    )
                })
                .collect();
            if !matches!(self.flat.hits.get(&id), Some((old, true)) if *old == hits) {
                batch.layout_hits(id, &hits);
                self.flat.hits.insert(id, (hits, true));
            }
        }
    }
}

fn layout_row(row: StyleId) -> bool {
    use StyleId::*;
    matches!(
        row,
        Width
            | Height
            | MinWidth
            | MinHeight
            | MaxWidth
            | MaxHeight
            | PaddingTop
            | PaddingRight
            | PaddingBottom
            | PaddingLeft
            | MarginTop
            | MarginRight
            | MarginBottom
            | MarginLeft
            | FlexDirection
            | FlexWrap
            | JustifyContent
            | AlignItems
            | AlignSelf
            | AlignContent
            | FlexGrow
            | FlexShrink
            | FlexBasis
            | RowGap
            | ColumnGap
            | AspectRatio
            | Display
            | BoxSizing
    )
}

fn layout_only(node: &NodeRef<'_>) -> bool {
    node.node_type == NodeType::View
        && node.looks().is_empty()
        && matches!(node.style.display, Display::Block | Display::Flex)
        && node
            .props
            .iter()
            .all(|(p, v)| p == PropId::SemanticTag && v.as_str() == Some("div"))
        && node
            .style
            .written
            .iter()
            .all(|row| node.style.mask.has(row))
        && node
            .style
            .mask
            .union(node.style.written)
            .iter()
            .all(layout_row)
}

fn moving_row(row: StyleId) -> bool {
    let name = row.name();
    name.contains("animation")
        || name.contains("transition")
        || name.starts_with("press_")
        || name.contains("timeline")
        || name == "symbol_effect"
        || name == "hover_effect"
        || name == "translate_velocity"
}

fn fragmenting_row(row: StyleId) -> bool {
    // Fragmented union frames include column gaps (LLP 1093 D7).
    matches!(row, StyleId::ColumnCount | StyleId::ColumnWidth)
}

fn drag_event(event: &EventKind) -> bool {
    let name = event.name();
    name.contains("drag")
        || name.starts_with("pan")
        || name.starts_with("swipe")
        || name.starts_with("pointer")
        || matches!(
            name,
            "heightrelease" | "transformgeometry" | "transformrelease"
        )
}

fn quiet_type(node_type: NodeType) -> bool {
    matches!(
        node_type,
        NodeType::View | NodeType::Text | NodeType::Image | NodeType::Pressable | NodeType::Control
    )
}

fn quiet_prop(prop: PropId) -> bool {
    // Native presentations can carry their subtree out of the collection row.
    !prop.name().starts_with("navigation")
        && !matches!(
            prop,
            PropId::SharedElement
                | PropId::HeightDragFor
                | PropId::TransformDragFor
                | PropId::Popover
                | PropId::ContextPopover
                | PropId::Hatch
                | PropId::ReorderFor
                | PropId::ReorderGroup
                | PropId::SwipeContent
                | PropId::SwipeLeading
                | PropId::SwipeTrailing
        )
}

fn quiet_node(node: &NodeRef<'_>) -> bool {
    let overflow = style::effective_overflow(node);
    quiet_type(node.node_type)
        && (node.node_type != NodeType::Control || node.props.str(PropId::Type) == Some("progress"))
        && !super::layout::closed::top_layer(node)
        && node.looks().is_empty()
        && !matches!(
            node.props.str(PropId::AccessibilityRole),
            Some("dialog" | "alertdialog" | "tablist" | "radiogroup")
        )
        && !node
            .style
            .mask
            .union(node.style.written)
            .iter()
            .any(|row| moving_row(row) || fragmenting_row(row))
        && ![overflow.0, overflow.1]
            .iter()
            .any(|o| matches!(o, Overflow::Scroll | Overflow::Auto))
        && node.props.iter().all(|(p, _)| quiet_prop(p))
}

// A row's inactive arms and unmounted repetitions are as binding as its live content.
fn quiet_templates(plan: &exact_plan::Plan) -> Vec<bool> {
    use exact_plan::{BindingKind, NodesId};
    let parent = |id: NodesId| {
        let node = plan.node(id);
        let (mut parent, mut arm) = (node.parent, node.arm);
        while parent.is_none() {
            let region = plan.region(plan.arms[arm?.0 as usize].region);
            (parent, arm) = (region.parent, region.arm);
        }
        parent
    };
    let mut quiet = vec![true; plan.nodes.len()];
    for (i, node) in plan.nodes.iter().enumerate() {
        let id = NodesId(i as u32);
        let mut overflow = [Overflow::Visible; 2];
        let safe = NodeType::from_wire(node.node_type).is_some_and(quiet_type)
            && (node.node_type != NodeType::Control as u8
                || node.bindings.iter().any(|b| {
                    let b = plan.binding(b);
                    b.kind == BindingKind::Prop
                        && b.id == PropId::Type as u16
                        && literal_string(plan, b.expr) == Some("progress")
                }))
            && node.surface.is_none()
            && !plan.looks.iter().any(|look| look.node == id)
            && !node
                .handlers
                .iter()
                .any(|h| drag_event(&plan.handler(h).event))
            && node.bindings.iter().all(|b| {
                let b = plan.binding(b);
                match b.kind {
                    BindingKind::Style => {
                        let Some(row) = StyleId::from_bit(b.id as u32) else {
                            return false;
                        };
                        if moving_row(row) || fragmenting_row(row) {
                            return false;
                        }
                        if matches!(row, StyleId::OverflowX | StyleId::OverflowY) {
                            overflow[usize::from(row == StyleId::OverflowY)] =
                                match literal_string(plan, b.expr) {
                                    Some("hidden") => Overflow::Hidden,
                                    Some("clip") => Overflow::Clip,
                                    Some("visible") => Overflow::Visible,
                                    _ => return false,
                                };
                        }
                        true
                    }
                    BindingKind::Prop => {
                        PropId::from_wire(b.id).is_some_and(quiet_prop)
                            && (b.id != PropId::SemanticTag as u16
                                || literal_string(plan, b.expr).is_some_and(|tag| tag != "dialog"))
                            && (b.id != PropId::AccessibilityRole as u16
                                || literal_string(plan, b.expr).is_some_and(|role| {
                                    !matches!(
                                        role,
                                        "dialog" | "alertdialog" | "tablist" | "radiogroup"
                                    )
                                }))
                    }
                }
            })
            && !matches!(
                overflow[0].computed(overflow[1]),
                Overflow::Scroll | Overflow::Auto
            )
            && !matches!(
                overflow[1].computed(overflow[0]),
                Overflow::Scroll | Overflow::Auto
            );
        if !safe {
            let mut at = Some(id);
            while let Some(id) = at {
                if !quiet[id.0 as usize] {
                    break;
                }
                quiet[id.0 as usize] = false;
                at = parent(id);
            }
        }
    }
    quiet
}

fn literal_string(plan: &exact_plan::Plan, code: exact_plan::Code) -> Option<&str> {
    use exact_plan::Opcode;
    let ops: Vec<_> = exact_runner::vm::instructions(plan.read_code(plan.code(code)).0)
        .flatten()
        .collect();
    (ops.len() == 2 && ops[0].op == Opcode::Str && ops[1].op == Opcode::Return)
        .then(|| plan.str(exact_plan::StrId(ops[0].args[0] as u32)))
}
