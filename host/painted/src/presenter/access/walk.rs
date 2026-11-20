use super::*;

/// A table's, list's, or description list's element as its role (HTML-AAM): a
/// node that holds what it heads or holds, named by its author, if at all.
fn structure_role(node: &NodeRef<'_>) -> Option<i32> {
    if node.node_type != exact_kernel::NodeType::View {
        return None;
    }
    Some(match node.props.str(PropId::SemanticTag)? {
        "table" => role::TABLE,
        "caption" => role::CAPTION,
        "tr" => role::ROW,
        "td" => role::CELL,
        "th" if node.props.str(PropId::Scope) == Some("row") => role::ROW_HEADER,
        "th" => role::COLUMN_HEADER,
        "dl" => role::DESCRIPTION_LIST,
        "dt" => role::TERM,
        "dd" => role::DEFINITION,
        "ul" | "ol" => role::LIST,
        "li" => role::LIST_ITEM,
        _ => return None,
    })
}

/// A labelled empty item still belongs to the list. Keep its real bounds so
/// keyboard and screen-reader navigation can find it without adding a hit area.
pub(super) fn empty_item_bounds(
    rect: Rect4,
    clip: Rect4,
    node: &NodeRef<'_>,
    role: i32,
) -> Option<Rect4> {
    if role != role::LIST_ITEM
        || (rect.2 != 0. && rect.3 != 0.)
        || !node
            .props
            .str(PropId::AccessibilityLabel)
            .is_some_and(|s| !s.trim().is_empty())
    {
        return None;
    }
    let axis = |a: f32, length: f32, start: f32, extent: f32| {
        let (lo, hi) = (a.max(start), (a + length).min(start + extent));
        (hi > lo || (length == 0. && a >= start && a < start + extent)).then_some((lo, hi - lo))
    };
    let (x, w) = axis(rect.0, rect.2, clip.0, clip.2)?;
    let (y, h) = axis(rect.1, rect.3, clip.1, clip.3)?;
    Some((x, y, w, h))
}

impl<D: DataSource> Presenter<D> {
    /// Walk `id` and what is under it into the tree.
    pub(super) fn access_walk(&self, b: &mut Build, id: ViewId, scope: Scope) {
        let Some(node) = self.host.kernel().node(id) else {
            return;
        };
        self.access_walk_node(b, node, scope);
    }

    fn access_walk_node(&self, b: &mut Build, node: NodeRef<'_>, mut scope: Scope) {
        let id = node.id;
        // An open popover is drawn in the top layer: it is walked after the
        // page, under no clip or scroll of its page ancestors.
        if b.top == Some(id) {
            return;
        }
        if self.access_excluded(&node) {
            return;
        }
        if let Some(said) = node.props.bool(PropId::AccessibilityDisabled) {
            scope.disabled = said;
        }
        // A split's list route keeps the window's width; what shows of it is
        // its column, the clip paint gave its box, which no scroll inside it moves.
        if self.host.navigation.split.active.as_ref().map(|s| s.root) == Some(id) {
            if let Some((x, y, w, h)) = b.boxes.get(&id).and_then(|p| p.clip) {
                let column = (x + scope.shift.0, y + scope.shift.1, w, h);
                let Some(clip) = intersect(column, scope.clip) else {
                    return;
                };
                scope.clip = clip;
            }
        }
        let position = |prop| node.props.get(prop).and_then(|v| v.as_int());
        let previous = position(PropId::AccessibilityPosInSet).map(|at| {
            let size = position(PropId::AccessibilitySetSize).unwrap_or(0);
            std::mem::replace(&mut b.item, (at as i32, size as i32))
        });
        self.access_walk_contents(b, node, scope);
        // A place no emitted node took must not reach the next subtree.
        if let Some(previous) = previous {
            b.item = previous;
        }
    }

    /// Whether `node` and what is under it are left out: hidden, inert, in a
    /// shared-element flight, not displayed or `aria-hidden`.
    fn access_excluded(&self, node: &NodeRef<'_>) -> bool {
        let (hidden, inert) = self.host.route_visibility(node.id);
        hidden
            || inert
            || self.host.flying(node.id)
            || node.style.display == Display::None
            || node.props.bool(PropId::AccessibilityElementsHidden) == Some(true)
    }

    fn access_walk_contents(&self, b: &mut Build, node: NodeRef<'_>, mut scope: Scope) {
        let id = node.id;
        let aria = node.props.str(PropId::AccessibilityRole);
        let control = ControlKind::of(node.node_type, node.props);
        // A grouped list's toggle row is one node, as Material's list item
        // with a trailing switch is to TalkBack: the switch's kind and
        // state, named by the row, else the switch, else the row's words.
        // A press on it is a press on the switch. A switch left out of the
        // tree leaves the row as it is, and the switch's `aria-disabled` is
        // the node's, as it would be its own.
        // A row with a text or an image that has a press of its own is no such row:
        // each of its parts is walked as it is written.
        let kernel = self.host.kernel();
        let acts = |id| (self.host.runner().handlers_of(id)).contains(&EventKind::Press);
        let toggle = (kernel
            .grouped_toggle(id, &acts)
            .and_then(|t| kernel.node(t)))
        .filter(|t| !self.access_excluded(t));
        if let Some((target, kind)) =
            toggle.and_then(|t| Some((t, ControlKind::of(t.node_type, t.props)?)))
        {
            let (role, mut flags, range, value) = self.access_control(&target, kind);
            if target.props.bool(PropId::Disabled) == Some(true) {
                flags = (flags | flag::DISABLED) & !(flag::CLICK | flag::LONG_CLICK);
            }
            if let Some(said) = target.props.bool(PropId::AccessibilityDisabled) {
                scope.disabled = said;
            }
            let name = (self.access_label(&node))
                .or_else(|| self.access_label(&target))
                .unwrap_or_else(|| self.access_words(&node, false));
            let words = || Some((name, value));
            if self
                .access_emit(b, &scope, &node, role, flags, range, words)
                .is_some()
            {
                if let Some(own) = b.out.last_mut() {
                    own.view = target.id;
                }
            }
            return;
        }
        // What is one node, with nothing under it.
        let leaf = match (node.node_type, control) {
            (NodeType::TextInput, _) => {
                let hosted = self.hosts_field(id);
                let role = if hosted { role::FIELD } else { role::EDIT };
                let name = self.access_label(&node).unwrap_or_default();
                let value = match hosted {
                    true => node
                        .props
                        .str(PropId::Placeholder)
                        .unwrap_or_default()
                        .into(),
                    false => self.field_text(id),
                };
                Some((role, flag::CLICK, [0.0; 3], (name, value)))
            }
            // @ref LLP 1042.001 D11 — one node: what is in a controller
            // is the controller's to report.
            (NodeType::Video, _) => {
                let name = self.access_label(&node).unwrap_or_default();
                Some((role::MEDIA, 0, [0.0; 3], (name, String::new())))
            }
            (_, Some(kind)) if kind != ControlKind::Button => {
                let (role, flags, range, value) = self.access_control(&node, kind);
                let name = self.access_label(&node).unwrap_or_default();
                Some((role, flags, range, (name, value)))
            }
            _ => None,
        };
        if let Some((role, flags, range, words)) = leaf {
            self.access_emit(b, &scope, &node, role, flags, range, || Some(words));
            return;
        }
        if self.access_pressable(&node) {
            let (role, flags) = self.access_pressed(&node);
            let name = || Some((self.access_name(&node), String::new()));
            let Some(own) = self.access_emit(b, &scope, &node, role, flags, [0.0; 3], name) else {
                return;
            };
            scope.parent = own;
            scope.inside = true;
        } else if matches!(
            node.node_type,
            NodeType::Text | NodeType::Image | NodeType::Svg
        ) {
            if scope.inside {
                return;
            }
            let text = node.node_type == NodeType::Text;
            let name = || {
                let label = self.access_label(&node);
                let name = label.unwrap_or_else(|| match text {
                    true => self.access_inline_words(&node),
                    false => String::new(),
                });
                (!name.trim().is_empty()).then_some((name, String::new()))
            };
            // @ref LLP 1115.000 D12.5 — a heading by its role, never by an
            // `aria-level` alone (the kernel's `Props::heading`).
            let heading = node.props.heading().is_some();
            let role = match (text, heading, aria) {
                (true, true, _) => role::HEADING,
                (true, false, Some("listitem")) => role::LIST_ITEM,
                (true, false, _) => role::TEXT,
                _ => role::IMAGE,
            };
            if let Some(own) = self.access_emit(b, &scope, &node, role, 0, [0.0; 3], name) {
                scope.parent = own;
            }
            if text {
                self.access_inline_controls(b, &node, scope);
            }
            return;
        } else if let Some(now) =
            number(node.props.str(PropId::AccessibilityValueNow)).filter(|_| {
                matches!(
                    aria,
                    Some("progressbar" | "slider" | "meter" | "scrollbar" | "spinbutton")
                )
            })
        {
            // A range role is one element, its children presentational.
            let said_min = number(node.props.str(PropId::AccessibilityValueMin));
            let said_max = number(node.props.str(PropId::AccessibilityValueMax));
            let (min, max) = (said_min.unwrap_or(0.0), said_max.unwrap_or(100.0));
            let role = match aria {
                Some("slider") => role::SLIDER,
                Some("spinbutton") => role::SPIN_BUTTON,
                _ => role::PROGRESS,
            };
            // A slider or spin button steps by its author's keys
            // (`access_step_range`) while it is short of a bound: ARIA's 0
            // and 100 for a slider, and only the bounds a spin button says.
            let (low, high) = match aria {
                Some("slider") => (min, max),
                Some("spinbutton") => (
                    said_min.unwrap_or(f32::NEG_INFINITY),
                    said_max.unwrap_or(f32::INFINITY),
                ),
                _ => (f32::INFINITY, f32::NEG_INFINITY),
            };
            let mut flags = flag::RANGE;
            if now < high {
                flags |= flag::FORWARD;
            }
            if now > low {
                flags |= flag::BACKWARD;
            }
            let text = node
                .props
                .str(PropId::AccessibilityValueText)
                .unwrap_or_default();
            let words = (
                self.access_label(&node).unwrap_or_default(),
                text.to_string(),
            );
            self.access_emit(b, &scope, &node, role, flags, [min, max, now], || {
                Some(words)
            });
            return;
        } else {
            // A swipe row is written as a scroller, and is the presenter's
            // own gesture: its actions are not a place to scroll to. What
            // scrolls is what the painter and the wheel scroll, a neutral
            // alert's message box by the alert's defaults too.
            let (ox, oy) = crate::paint::effective_overflow(&node);
            let may = !scope.inside
                && node.props.str(PropId::SwipeContent).is_none()
                && [ox, oy]
                    .iter()
                    .any(|o| matches!(o, Overflow::Scroll | Overflow::Auto));
            let scroller = (may.then(|| self.scroller(id, &b.limits)).flatten())
                .filter(|s| s.axes.0 || s.axes.1);
            let label = self.access_label(&node);
            let dialog = match aria {
                Some("dialog") => Some(role::DIALOG),
                Some("alertdialog") => Some(role::ALERT_DIALOG),
                None if node.props.str(PropId::SemanticTag) == Some("dialog") => Some(role::DIALOG),
                _ => None,
            };
            if let Some(role) = dialog {
                let modal = self.alert_inert() && self.alert_root() == Some(id);
                let flags = if modal { flag::MODAL } else { 0 };
                let name = label
                    .clone()
                    .or_else(|| node.props.str(PropId::Title).map(str::to_owned))
                    .unwrap_or_default();
                if let Some(parent) =
                    self.access_emit(b, &scope, &node, role, flags, [0.; 3], || {
                        Some((name, String::new()))
                    })
                {
                    scope.parent = parent;
                }
            }
            if let Some(s) = scroller {
                let (flags, range) = Self::access_scroll(&s);
                let words = (
                    label.filter(|_| dialog.is_none()).unwrap_or_default(),
                    String::new(),
                );
                let painted = b.boxes.get(&id).cloned();
                let Some(own) =
                    self.access_emit(b, &scope, &node, role::SCROLLER, flags, range, || {
                        Some(words)
                    })
                else {
                    return;
                };
                if let Some(painted) = painted {
                    // What is in it was painted at the offset it had then.
                    let was = painted.scroll.unwrap_or(s.offset);
                    let port = (
                        painted.rect.0 + scope.shift.0,
                        painted.rect.1 + scope.shift.1,
                        painted.rect.2,
                        painted.rect.3,
                    );
                    scope.clip = intersect(port, scope.clip).unwrap_or(scope.clip);
                    scope.shift.0 += was.0 - s.offset.0;
                    scope.shift.1 += was.1 - s.offset.1;
                }
                scope.parent = own;
            } else if let Some(role) = match aria {
                // A virtual row lends its place to the first accessible child.
                // The native list already publishes its own scroll container.
                Some("list") if node.node_type != NodeType::List => Some(role::LIST),
                Some("listitem") if node.props.str(PropId::ListItemKey).is_none() => {
                    Some(role::LIST_ITEM)
                }
                Some("tablist") => Some(role::TABLIST),
                None => structure_role(&node),
                _ => None,
            } {
                let words = (label.unwrap_or_default(), String::new());
                if let Some(own) =
                    self.access_emit(b, &scope, &node, role, 0, [0.0; 3], || Some(words))
                {
                    scope.parent = own;
                }
            } else if (dialog.is_none() && Self::access_live(&node) != 0)
                || (label.is_some() && matches!(aria, Some("group" | "radiogroup" | "region")))
            {
                let words = (label.unwrap_or_default(), String::new());
                if let Some(own) =
                    self.access_emit(b, &scope, &node, role::GROUP, 0, [0.0; 3], || Some(words))
                {
                    scope.parent = own;
                }
            }
        }
        let own = match scope.hash {
            Some(hash) => rowed(hash),
            None => (id & 0x3fff_ffff) as i32,
        };
        let mut visit = |index: usize, child: NodeRef<'_>| {
            let key = child.props.str(PropId::ListItemKey);
            let mut under = scope;
            under.hash = match key.filter(|key| !key.is_empty()) {
                Some(key) => Some(row_hash(own, key)),
                None => scope
                    .hash
                    .map(|hash| fnv(hash, &(index as u32).to_le_bytes())),
            };
            self.access_walk_node(b, child, under);
        };
        // A table's rows are visited in the order it draws and counts them
        // (`Node::grid`), not as written: the first `tfoot` last.
        match kernel.table_order(id) {
            Some(order) => {
                let written: Vec<NodeRef<'_>> = node.child_nodes().collect();
                for index in order {
                    visit(index, written[index]);
                }
            }
            None => node
                .child_nodes()
                .enumerate()
                .for_each(|(i, c)| visit(i, c)),
        }
    }
}
