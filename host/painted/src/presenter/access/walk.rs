use super::*;

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

    /// Whether `node` and what is under it are left out: hidden, inert, not
    /// displayed or `aria-hidden`.
    fn access_excluded(&self, node: &NodeRef<'_>) -> bool {
        let (hidden, inert) = self.host.route_visibility(node.id);
        hidden
            || inert
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
        let kernel = self.host.kernel();
        let toggle = (kernel.grouped_toggle(id).and_then(|t| kernel.node(t)))
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
            let role = match (text, heading) {
                (true, true) => role::HEADING,
                (true, false) => role::TEXT,
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
            let min = number(node.props.str(PropId::AccessibilityValueMin)).unwrap_or(0.0);
            let max = number(node.props.str(PropId::AccessibilityValueMax)).unwrap_or(100.0);
            let role = if aria == Some("slider") {
                role::SLIDER
            } else {
                role::PROGRESS
            };
            let text = node
                .props
                .str(PropId::AccessibilityValueText)
                .unwrap_or_default();
            let words = (
                self.access_label(&node).unwrap_or_default(),
                text.to_string(),
            );
            self.access_emit(b, &scope, &node, role, flag::RANGE, [min, max, now], || {
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
            if let Some(s) = scroller {
                let (flags, range) = Self::access_scroll(&s);
                let words = (label.unwrap_or_default(), String::new());
                let painted = b.boxes.get(&id).copied();
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
            } else if let Some(role) = structure_role(&node).filter(|_| aria.is_none()) {
                let words = (label.unwrap_or_default(), String::new());
                if let Some(own) =
                    self.access_emit(b, &scope, &node, role, 0, [0.0; 3], || Some(words))
                {
                    scope.parent = own;
                }
            } else if aria == Some("tablist")
                || Self::access_live(&node) != 0
                || (label.is_some()
                    && matches!(
                        aria,
                        Some("group" | "radiogroup" | "region" | "dialog" | "alertdialog")
                    ))
            {
                let role = if aria == Some("tablist") {
                    role::TABLIST
                } else {
                    role::GROUP
                };
                let words = (label.unwrap_or_default(), String::new());
                if let Some(own) =
                    self.access_emit(b, &scope, &node, role, 0, [0.0; 3], || Some(words))
                {
                    scope.parent = own;
                }
            }
        }
        let own = match scope.hash {
            Some(hash) => rowed(hash),
            None => (id & 0x3fff_ffff) as i32,
        };
        for (index, child) in node.child_nodes().enumerate() {
            let key = child.props.str(PropId::ListItemKey);
            let mut under = scope;
            under.hash = match key.filter(|key| !key.is_empty()) {
                Some(key) => Some(row_hash(own, key)),
                None => scope
                    .hash
                    .map(|hash| fnv(hash, &(index as u32).to_le_bytes())),
            };
            self.access_walk_node(b, child, under);
        }
    }
}
