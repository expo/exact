//! AccessKit nodes shared by the page and its popup windows.
use accesskit::{
    Action, Affine, Live, Node, NodeId, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate,
};
use exact_painted::{
    presenter::access::{act, flag, role, Node as Source},
    Presenter,
};
use exact_runner::DataSource;

pub(super) fn map_node(
    source: &Source,
    origin: (f32, f32),
    password: bool,
    multiline: bool,
    readonly: bool,
) -> Node {
    let role = match source.role {
        role::TEXT => Role::Label,
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
        _ => Role::GenericContainer,
    };
    let mut node = Node::new(role);
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
        // UI Automation offers SelectionItem only where a selection is said.
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
    }
    if source.set.0 > 0 {
        node.set_position_in_set(source.set.0 as usize);
    }
    if source.set.1 > 0 {
        node.set_size_of_set(source.set.1 as usize);
    }
    if flags & flag::CLICK != 0 {
        node.add_action(Action::Click);
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
    if flags & flag::RANGE != 0 {
        node.set_min_numeric_value(source.range[0].into());
        node.set_max_numeric_value(source.range[1].into());
        node.set_numeric_value(source.range[2].into());
        if flags & flag::SETTABLE != 0 {
            node.add_action(Action::SetValue);
        }
    }
    if matches!(source.role, role::SCROLLER | role::LIST_BOX) {
        if source.role == role::SCROLLER {
            node.add_action(Action::Focus);
        }
        let sideways = flags & flag::SIDEWAYS != 0;
        if sideways {
            node.set_scroll_x(source.range[0].into());
            node.set_scroll_x_min(0.);
            node.set_scroll_x_max(source.range[1].into());
        } else {
            node.set_scroll_y(source.range[0].into());
            node.set_scroll_y_min(0.);
            node.set_scroll_y_max(source.range[1].into());
        }
        if flags & flag::FORWARD != 0 {
            node.add_action(if sideways {
                Action::ScrollRight
            } else {
                Action::ScrollDown
            });
        }
        if flags & flag::BACKWARD != 0 {
            node.add_action(if sideways {
                Action::ScrollLeft
            } else {
                Action::ScrollUp
            });
        }
    } else if flags & flag::RANGE != 0 {
        if flags & flag::FORWARD != 0 {
            node.add_action(Action::Increment);
        }
        if flags & flag::BACKWARD != 0 {
            node.add_action(Action::Decrement);
        }
    }
    node
}

/// A popup's own tree; closing removes the Menu so UIA observes its lifetime.
pub(super) fn menu_tree(
    sources: &[Source],
    title: &str,
    size: (f32, f32),
    scale: f64,
    focus: Option<i32>,
    open: bool,
) -> TreeUpdate {
    let root_id = NodeId(0);
    // This HWND's root Menu survives replacement surfaces for its submenus.
    let menu_id = NodeId(u64::MAX);
    let mut root = Node::new(Role::Window);
    root.set_transform(Affine::scale(scale));
    root.set_bounds(Rect::new(0., 0., size.0.into(), size.1.into()));
    let mut nodes = Vec::new();
    if open {
        root.set_children(vec![menu_id]);
        let mut menu = Node::new(Role::Menu);
        menu.set_label(title);
        menu.set_bounds(Rect::new(0., 0., size.0.into(), size.1.into()));
        menu.set_children(
            sources
                .iter()
                .filter(|n| n.parent == 0)
                .map(|n| NodeId(n.id as u64))
                .collect::<Vec<_>>(),
        );
        nodes.push((menu_id, menu));
        for source in sources {
            let mut node = map_node(source, (0., 0.), false, false, false);
            if source.role == role::BUTTON {
                node.set_role(Role::MenuItem);
            }
            let children = sources
                .iter()
                .filter(|n| n.parent == source.id)
                .map(|n| NodeId(n.id as u64))
                .collect::<Vec<_>>();
            if source.flags & flag::EXPANDED != 0 {
                // UIA's control view nests a Menu under its expanded MenuItem.
                // The ancestor is no longer painted in this replacement flyout.
                node.clear_bounds();
                let submenu_id = NodeId((1 << 32) + source.id as u64);
                let mut submenu = Node::new(Role::Menu);
                submenu.set_label(source.name.clone());
                submenu.set_children(children);
                node.set_children([submenu_id]);
                nodes.push((submenu_id, submenu));
            } else {
                node.set_children(children);
            }
            nodes.push((NodeId(source.id as u64), node));
        }
    }
    nodes.push((root_id, root));
    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(root_id)),
        tree_id: TreeId::ROOT,
        focus: focus
            .filter(|id| open && sources.iter().any(|n| n.id == *id))
            .map_or(if open { menu_id } else { root_id }, |id| NodeId(id as u64)),
    }
}

pub(super) fn menu_action<D: DataSource>(p: &mut Presenter<D>, request: accesskit::ActionRequest) {
    if request.target_tree != TreeId::ROOT || p.status_menu().is_none() {
        return;
    }
    p.reader_access(1., true);
    let Ok(id) = i32::try_from(request.target_node.0) else {
        return;
    };
    let Some(source) = p.access_told().iter().find(|n| n.id == id) else {
        return;
    };
    let node = map_node(source, (0., 0.), false, false, false);
    if node.is_disabled() || !node.supports_action(request.action) {
        return;
    }
    let (view, expanded) = (source.view, source.flags & flag::EXPANDED != 0);
    let now = p.host().now();
    match request.action {
        Action::Collapse => {
            p.collapse_status_menu(view, now);
        }
        Action::Focus | Action::Click if expanded => {
            p.collapse_status_menu(view, now);
        }
        Action::Expand => {
            if p.access_focus_node(id, now) {
                p.status_menu_key("ArrowRight", now);
            }
        }
        Action::Focus => {
            p.access_focus_node(id, now);
        }
        Action::Click => {
            p.access_act(id, act::CLICK, 0., now);
        }
        Action::ScrollDown => {
            p.access_act(id, act::FORWARD, 0., now);
        }
        Action::ScrollUp => {
            p.access_act(id, act::BACKWARD, 0., now);
        }
        _ => {}
    }
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
            range: [0.; 3],
            set: (0, 0),
            grid: [0; 4],
            name: "Message".into(),
            value: "a🦀e\u{301}".into(),
            hint: "Draft".into(),
        }
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
        let spin = map_node(&spin, (0., 0.), false, false, false);
        assert_eq!(spin.role(), Role::SpinButton);
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
        source.range = [240., 240., 0.];
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
        ] {
            let node = map_node(&source(from), (0., 0.), false, false, false);
            assert_eq!(node.role(), to, "role {from}");
        }
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
        source.range = [0., 900., 0.];
        let list = map_node(&source, (0., 0.), false, false, false);
        assert_eq!(list.scroll_y_max(), Some(900.));
        assert!(list.supports_action(Action::ScrollDown));
        assert!(!list.supports_action(Action::Focus));
    }

    #[test]
    fn a_popup_is_a_named_menu_with_invocable_toggle_and_submenu_items() {
        let mut item = source(role::BUTTON);
        item.name = "Start at Login".into();
        item.flags = flag::CLICK | flag::CHECKABLE;
        let tree = menu_tree(&[item.clone()], "Ember", (150., 96.), 1.25, Some(7), true);
        let menu = tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(u64::MAX))
            .unwrap();
        assert_eq!(menu.1.role(), Role::Menu);
        assert_eq!(menu.1.label(), Some("Ember"));
        assert_eq!(menu.1.children(), &[NodeId(7)]);
        let row = tree.nodes.iter().find(|(id, _)| *id == NodeId(7)).unwrap();
        assert_eq!(row.1.role(), Role::MenuItem);
        assert_eq!(row.1.toggled(), Some(Toggled::False));
        assert!(row.1.supports_action(Action::Click));
        assert_eq!(tree.focus, NodeId(7));
        item.flags = flag::CLICK | flag::EXPANDABLE;
        let tree = menu_tree(&[item], "Ember", (150., 96.), 1., None, true);
        let row = &tree
            .nodes
            .iter()
            .find(|(id, _)| *id == NodeId(7))
            .unwrap()
            .1;
        assert_eq!(row.is_expanded(), Some(false));
        assert!(row.supports_action(Action::Expand));
        let closed = menu_tree(&[], "", (0., 0.), 1., None, false);
        assert_eq!(closed.nodes.len(), 1);
        assert!(closed.nodes[0].1.children().is_empty());
        assert_eq!(closed.focus, NodeId(0));
    }

    #[test]
    fn a_status_submenu_keeps_its_provider_and_collapses_with_focus_restored() {
        crate::on_desktop_stack(|| {
            let plan = contract::compile(
                r#"component App
  state hits = 0
  action ping
    hits = hits + 1
  view
    column
      button id="tray" -exact-menu="status" popovertarget="menu"
        text "Ember"
      column id="menu" popover="auto" role="menu" aria-label="Ember"
        button popovertarget="more"
          text "More"
      column id="more" popover="auto" role="menu" aria-label="More commands"
        button popovertarget="tools"
          text "Tools"
      column id="tools" popover="auto" role="menu"
        button press=ping
          text "Ping"
"#,
            )
            .unwrap()
            .encode();
            let (mut p, error) = Presenter::boot_with(
                &plan,
                (),
                (300., 400.),
                1.,
                Default::default(),
                exact_painted::presenter::PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.serve_overlays();
            p.frame();
            let tray = p.host().kernel().find_by_id("tray")[0];
            let tray = p.host().kernel().node_by_key(tray).unwrap().id;
            assert!(p.open_status_menu(tray, (800., 600.), true, true, 0.));
            let snapshot = |p: &mut Presenter<()>| {
                let menu = p
                    .status_menu()
                    .expect("the submenu remains a detached status menu");
                p.reader_access(1., true);
                menu_tree(
                    p.access_told(),
                    &menu.label,
                    menu.size,
                    1.25,
                    p.access_focus(),
                    true,
                )
            };
            let act = |p: &mut Presenter<()>, id, action| {
                menu_action(
                    p,
                    accesskit::ActionRequest {
                        action,
                        target_tree: TreeId::ROOT,
                        target_node: id,
                        data: None,
                    },
                );
            };
            let tree = snapshot(&mut p);
            let root_menu = tree
                .nodes
                .iter()
                .find(|(id, _)| *id == NodeId(0))
                .unwrap()
                .1
                .children()[0];
            assert_eq!(
                tree.nodes
                    .iter()
                    .find(|(id, _)| *id == root_menu)
                    .unwrap()
                    .1
                    .label(),
                Some("Ember")
            );
            let more = tree
                .nodes
                .iter()
                .find(|(_, n)| n.label() == Some("More"))
                .unwrap()
                .0;
            for nested in [false, true] {
                act(&mut p, more, Action::Expand);
                let tree = snapshot(&mut p);
                let current_menu = tree
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == NodeId(0))
                    .unwrap()
                    .1
                    .children()[0];
                let current_name = tree
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == current_menu)
                    .unwrap()
                    .1
                    .label();
                assert_eq!((current_menu, current_name), (root_menu, Some("Ember")));
                let expanded = &tree.nodes.iter().find(|(id, _)| *id == more).unwrap().1;
                assert_eq!(expanded.role(), Role::MenuItem);
                assert_eq!(expanded.is_expanded(), Some(true));
                assert!(expanded.supports_action(Action::Collapse));
                let submenu = &tree
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == expanded.children()[0])
                    .unwrap()
                    .1;
                assert_eq!(submenu.role(), Role::Menu);
                let tools = submenu.children()[0];
                assert_eq!(tree.focus, tools);
                if nested {
                    act(&mut p, tools, Action::Expand);
                    let tree = snapshot(&mut p);
                    assert_eq!(
                        tree.nodes
                            .iter()
                            .find(|(id, _)| *id == root_menu)
                            .unwrap()
                            .1
                            .label(),
                        Some("Ember")
                    );
                    assert_eq!(
                        tree.nodes
                            .iter()
                            .find(|(id, _)| *id == more)
                            .unwrap()
                            .1
                            .is_expanded(),
                        Some(true)
                    );
                    let ping = tree
                        .nodes
                        .iter()
                        .find(|(_, n)| n.label() == Some("Ping"))
                        .unwrap()
                        .0;
                    assert_eq!(tree.focus, ping);
                }
                act(&mut p, more, Action::Collapse);
                let tree = snapshot(&mut p);
                assert_eq!(
                    tree.nodes
                        .iter()
                        .find(|(id, _)| *id == NodeId(0))
                        .unwrap()
                        .1
                        .children(),
                    &[root_menu]
                );
                let collapsed = &tree.nodes.iter().find(|(id, _)| *id == more).unwrap().1;
                assert_eq!(collapsed.is_expanded(), Some(false));
                assert!(collapsed.supports_action(Action::Expand));
                assert!(collapsed.children().is_empty());
                assert_eq!(tree.focus, more);
            }
        });
    }
}
