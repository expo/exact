use super::super::access::{flag, role, Node as Source, NodeRange};
use accesskit::{Action, ActionData, Live, Node, Rect, Role, Toggled};

/// Map a 1-based ARIA heading level (`Props::heading`) onto AccessKit's
/// 0-based `level` property, which `accesskit-windows` exposes as both
/// `UIA_LevelPropertyId` and `UIA_HeadingLevelPropertyId`.
pub(super) fn set_heading_level(node: &mut Node, level: Option<i64>) {
    if let Some(level) = level.and_then(|l| usize::try_from(l - 1).ok()) {
        node.set_level(level);
    }
}

/// Map one painted element at its window-relative origin.
pub fn map_node(
    source: &Source,
    origin: (f32, f32),
    password: bool,
    multiline: bool,
    readonly: bool,
) -> Node {
    let role = match source.role {
        role::TEXT => Role::Label,
        role::GROUP if !source.name.is_empty() || source.flags & flag::FOCUSABLE != 0 => {
            Role::Group
        }
        role::HEADING => Role::Heading,
        role::BUTTON | role::UP | role::TOGGLE => Role::Button,
        role::LINK => Role::Link,
        role::CHECKBOX => Role::CheckBox,
        role::SWITCH => Role::Switch,
        role::RADIO => Role::RadioButton,
        role::SLIDER => Role::Slider,
        // UI Automation's Spinner, with RangeValue below.
        role::SPIN_BUTTON => Role::SpinButton,
        role::IMAGE => Role::Image,
        role::MEDIA => Role::Video,
        role::TAB => Role::Tab,
        role::SCROLLER => Role::ScrollView,
        role::PROGRESS => Role::ProgressIndicator,
        role::EDIT | role::FIELD if password => Role::PasswordInput,
        role::EDIT | role::FIELD if multiline => Role::MultilineTextInput,
        role::EDIT | role::FIELD => Role::TextInput,
        role::TABLIST => Role::TabList,
        role::DROPDOWN => Role::ComboBox,
        role::LIST_BOX => Role::ListBox,
        role::OPTION => Role::ListBoxOption,
        // @ref LLP 1153 D7 — HTML's table and description list (HTML-AAM).
        role::TABLE => Role::Table,
        role::CAPTION => Role::Caption,
        role::ROW => Role::Row,
        role::CELL => Role::Cell,
        role::COLUMN_HEADER => Role::ColumnHeader,
        role::ROW_HEADER => Role::RowHeader,
        role::DESCRIPTION_LIST => Role::DescriptionList,
        role::TERM => Role::Term,
        role::DEFINITION => Role::Definition,
        role::LIST => Role::List,
        role::LIST_ITEM => Role::ListItem,
        role::DIALOG => Role::Dialog,
        role::ALERT_DIALOG => Role::AlertDialog,
        _ => Role::GenericContainer,
    };
    let mut node = Node::new(role);
    if !source.actions.is_empty() {
        node.add_action(Action::CustomAction);
        for (id, name) in &source.actions {
            node.push_custom_action(accesskit::CustomAction {
                id: *id as i32,
                description: name.clone(),
            });
        }
    }
    if source.flags & flag::MODAL != 0 {
        node.set_modal();
    }
    let [x, y, width, height] = source.rect;
    node.set_bounds(Rect::new(
        (x + origin.0).into(),
        (y + origin.1).into(),
        (x + origin.0 + width).into(),
        (y + origin.1 + height).into(),
    ));
    node.set_label(source.name.clone());
    if source.role == role::TEXT {
        node.set_value(source.name.clone());
    } else if !password
        && (matches!(source.role, role::EDIT | role::FIELD) || !source.value.is_empty())
    {
        node.set_value(source.value.clone());
    }
    if !source.hint.is_empty() {
        node.set_description(source.hint.clone());
    }
    let flags = source.flags;
    if flags & flag::DISABLED != 0 {
        node.set_disabled();
    }
    if flags & flag::CHECKABLE != 0 {
        node.set_toggled(if flags & flag::CHECKED != 0 {
            Toggled::True
        } else {
            Toggled::False
        });
    }
    if flags & flag::SELECTED != 0 {
        node.set_selected(true);
    } else if source.role == role::OPTION {
        // Both UIA and AT-SPI expose selectable controls through this property.
        node.set_selected(false);
    }
    if flags & flag::EXPANDABLE != 0 {
        let expanded = flags & flag::EXPANDED != 0;
        node.set_expanded(expanded);
        if flags & flag::CLICK != 0 {
            node.add_action(if expanded {
                Action::Collapse
            } else {
                Action::Expand
            });
        }
    }
    if flags & flag::ASSERTIVE != 0 {
        node.set_live(Live::Assertive);
    } else if flags & flag::POLITE != 0 {
        node.set_live(Live::Polite);
    } else if flags & flag::LIVE_OFF != 0 {
        node.set_live(Live::Off);
    }
    if source.set.0 > 0 {
        node.set_position_in_set(source.set.0 as usize);
    }
    if source.set.1 > 0 {
        node.set_size_of_set(source.set.1 as usize);
    }
    if flags & flag::CLICK != 0 {
        node.add_action(Action::Click);
    }
    if flags & (flag::CLICK | flag::FOCUSABLE) != 0 {
        node.add_action(Action::Focus);
    }
    if flags & flag::LONG_CLICK != 0 {
        node.add_action(Action::ShowContextMenu);
    }
    if matches!(source.role, role::EDIT | role::FIELD) {
        if readonly {
            node.set_read_only();
        } else {
            node.add_action(Action::SetValue);
        }
        if !password {
            node.add_action(Action::SetTextSelection);
        }
    }
    match source.range {
        NodeRange::Value([min, max, value]) if flags & flag::RANGE != 0 => {
            node.set_min_numeric_value(min.into());
            node.set_max_numeric_value(max.into());
            node.set_numeric_value(value.into());
            if flags & flag::SETTABLE != 0 {
                node.add_action(Action::SetValue);
            }
            if flags & flag::FORWARD != 0 {
                node.add_action(Action::Increment);
            }
            if flags & flag::BACKWARD != 0 {
                node.add_action(Action::Decrement);
            }
        }
        NodeRange::Scroll {
            offset: (x, y),
            max: (width, height),
        } => {
            if source.role == role::SCROLLER {
                node.add_action(Action::Focus);
            }
            if width > 0. {
                node.set_scroll_x(x.into());
                node.set_scroll_x_min(0.);
                node.set_scroll_x_max(width.into());
                if x > 0. {
                    node.add_action(Action::ScrollLeft);
                }
                if x < width {
                    node.add_action(Action::ScrollRight);
                }
            }
            if height > 0. {
                node.set_scroll_y(y.into());
                node.set_scroll_y_min(0.);
                node.set_scroll_y_max(height.into());
                if y > 0. {
                    node.add_action(Action::ScrollUp);
                }
                if y < height {
                    node.add_action(Action::ScrollDown);
                }
            }
            if width > 0. || height > 0. {
                node.add_action(Action::SetScrollOffset);
            }
        }
        _ => {}
    }
    node
}

/// Resolve a scroll action in logical pixels, using the host’s line increment.
pub fn scroll_target(
    node: &Node,
    action: Action,
    data: Option<&ActionData>,
    line: f32,
) -> Option<(f32, f32)> {
    if let (Action::SetScrollOffset, Some(ActionData::SetScrollOffset(to))) = (action, data) {
        return Some((to.x as f32, to.y as f32));
    }
    let (horizontal, forward) = match action {
        Action::ScrollLeft => (true, false),
        Action::ScrollRight => (true, true),
        Action::ScrollUp => (false, false),
        Action::ScrollDown => (false, true),
        _ => return None,
    };
    let bounds = node.bounds()?;
    let step = match data {
        Some(ActionData::ScrollUnit(accesskit::ScrollUnit::Item)) => f64::from(line),
        _ if horizontal => bounds.width(),
        _ => bounds.height(),
    };
    let delta = if forward { step } else { -step };
    let (x, y) = (
        node.scroll_x().unwrap_or_default(),
        node.scroll_y().unwrap_or_default(),
    );
    let to = if horizontal {
        (x + delta, y)
    } else {
        (x, y + delta)
    };
    Some((to.0 as f32, to.1 as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(role: i32) -> Source {
        Source {
            id: 7,
            parent: 0,
            view: 1,
            role,
            flags: flag::CLICK,
            rect: [10., 20., 100., 40.],
            range: [0.; 3].into(),
            set: (0, 0),
            grid: [0; 4],
            name: "Message".into(),
            value: "a🦀e\u{301}".into(),
            hint: "Draft".into(),
            action_owner: 1,
            actions: Vec::new(),
        }
    }

    #[test]
    fn a_named_group_remains_in_the_platform_tree() {
        let mut group = source(role::GROUP);
        let mapped = map_node(&group, (0., 0.), false, false, false);
        assert_eq!(
            (mapped.role(), mapped.label()),
            (Role::Group, Some("Message"))
        );
        group.name.clear();
        assert_eq!(
            map_node(&group, (0., 0.), false, false, false).role(),
            Role::GenericContainer
        );
    }

    #[test]
    fn accessibility_a_drawn_range_steps_and_only_a_native_one_takes_a_value() {
        let mut slider = source(role::SLIDER);
        slider.flags = flag::RANGE | flag::FORWARD;
        let drawn = map_node(&slider, (0., 0.), false, false, false);
        assert!(drawn.supports_action(Action::Increment));
        assert!(!drawn.supports_action(Action::Decrement));
        assert!(!drawn.supports_action(Action::SetValue));
        slider.flags = flag::RANGE | flag::SETTABLE | flag::FORWARD | flag::BACKWARD;
        let native = map_node(&slider, (0., 0.), false, false, false);
        assert!(native.supports_action(Action::Decrement));
        assert!(native.supports_action(Action::SetValue));
        let mut spin = source(role::SPIN_BUTTON);
        spin.flags = flag::RANGE | flag::BACKWARD;
        spin.actions.push((u32::MAX - 11, "Reset quantity".into()));
        let spin = map_node(&spin, (0., 0.), false, false, false);
        assert_eq!(spin.role(), Role::SpinButton);
        assert!(spin.supports_action(Action::CustomAction));
        assert_eq!(spin.custom_actions()[0].description, "Reset quantity");
        assert_eq!(spin.custom_actions()[0].id, -12);
        assert!(spin.supports_action(Action::Decrement));
        assert!(!spin.supports_action(Action::SetValue));
    }

    #[test]
    fn accessibility_editors_protect_passwords_and_readonly_values() {
        let mut source = source(role::EDIT);
        let password = map_node(&source, (0., 0.), true, false, false);
        assert_eq!(password.role(), Role::PasswordInput);
        assert_eq!(password.value(), None);
        assert!(!password.supports_action(Action::SetTextSelection));
        let readonly = map_node(&source, (0., 0.), false, true, true);
        assert_eq!(readonly.role(), Role::MultilineTextInput);
        assert_eq!(readonly.value(), Some("a🦀e\u{301}"));
        assert!(readonly.is_read_only());
        assert!(!readonly.supports_action(Action::SetValue));
        assert!(readonly.supports_action(Action::SetTextSelection));
        source.value.clear();
        let empty = map_node(&source, (0., 0.), false, true, false);
        assert_eq!(empty.value(), Some(""));
        assert!(empty.supports_action(Action::SetValue));
    }

    #[test]
    fn accessibility_menu_bounds_and_scroll_extents_match_presentation() {
        let mut source = source(role::BUTTON);
        let menu = map_node(&source, (300., 50.), false, false, false);
        assert_eq!(menu.bounds(), Some(Rect::new(310., 70., 410., 110.)));
        source.role = role::SCROLLER;
        source.flags = flag::BACKWARD | flag::SIDEWAYS;
        source.range = NodeRange::Scroll {
            offset: (240., 0.),
            max: (240., 0.),
        };
        let scroller = map_node(&source, (0., 0.), false, false, false);
        assert_eq!(scroller.scroll_x(), Some(240.));
        assert_eq!(scroller.scroll_x_max(), Some(240.));
        assert!(scroller.supports_action(Action::ScrollLeft));
        assert!(!scroller.supports_action(Action::ScrollRight));
        source.role = role::TEXT;
        let text = map_node(&source, (0., 0.), false, false, false);
        assert_eq!(text.value(), Some("Message"));
    }

    /// LLP 1153 D7: the Linux tree's table and description-list roles are
    /// UI Automation's, through AccessKit's.
    #[test]
    fn accessibility_tables_and_description_lists_are_their_roles() {
        for (from, to) in [
            (role::TABLE, Role::Table),
            (role::CAPTION, Role::Caption),
            (role::ROW, Role::Row),
            (role::CELL, Role::Cell),
            (role::COLUMN_HEADER, Role::ColumnHeader),
            (role::ROW_HEADER, Role::RowHeader),
            (role::DESCRIPTION_LIST, Role::DescriptionList),
            (role::TERM, Role::Term),
            (role::DEFINITION, Role::Definition),
            (role::LIST_BOX, Role::ListBox),
            (role::OPTION, Role::ListBoxOption),
            (role::LIST, Role::List),
            (role::LIST_ITEM, Role::ListItem),
        ] {
            let node = map_node(&source(from), (0., 0.), false, false, false);
            assert_eq!(node.role(), to, "role {from}");
        }
        let mut h1 = map_node(&source(role::HEADING), (0., 0.), false, false, false);
        set_heading_level(&mut h1, Some(1));
        assert_eq!(h1.level(), Some(0));
        let mut h2 = map_node(&source(role::HEADING), (0., 0.), false, false, false);
        set_heading_level(&mut h2, Some(2));
        assert_eq!(h2.level(), Some(1));
    }

    /// A select is a combobox with UI Automation's ExpandCollapse, and its
    /// open menu's options each have a selection for SelectionItem.
    #[test]
    fn accessibility_select_expands_and_its_options_are_selectable() {
        let mut source = source(role::DROPDOWN);
        source.flags = flag::CLICK | flag::EXPANDABLE;
        let closed = map_node(&source, (0., 0.), false, false, false);
        assert_eq!(closed.is_expanded(), Some(false));
        assert!(closed.supports_action(Action::Expand));
        assert!(!closed.supports_action(Action::Collapse));
        source.flags |= flag::EXPANDED;
        let open = map_node(&source, (0., 0.), false, false, false);
        assert!(open.supports_action(Action::Collapse));
        source.role = role::OPTION;
        source.flags = flag::CLICK;
        let option = map_node(&source, (0., 0.), false, false, false);
        assert_eq!(option.is_selected(), Some(false));
        assert!(option.supports_action(Action::Click));
        source.role = role::LIST_BOX;
        source.flags = flag::FORWARD;
        source.range = NodeRange::Scroll {
            offset: (0., 0.),
            max: (0., 900.),
        };
        let list = map_node(&source, (0., 0.), false, false, false);
        assert_eq!(list.scroll_y_max(), Some(900.));
        assert!(list.supports_action(Action::ScrollDown));
        assert!(!list.supports_action(Action::Focus));
    }
}
