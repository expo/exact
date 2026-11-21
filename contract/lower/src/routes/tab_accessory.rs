//! @ref LLP 1075.003 §9.17 — what the compiler can see of the navigation
//! root's literal `navigationTabAccessory`, as the kernel's rule
//! (`RootTabs::accessory`) would refuse it on every tree the root can make:
//! an id that no element outside the routes carries, or one each of whose
//! carriers lies where the rule refuses it whatever the panels and links
//! are. It reads the checked expansion, so a component's elements are in
//! view. What depends on which panels the tablist links and where they fall
//! (an element before them, a tablist that links none) the kernel's rule
//! decides on the live tree, and a development build journals.

use crate::LowerError;
use contract_checking::{Checking, View};
use contract_syntax::{Attr, Expr};
use contract_types::Checked;
use exact_kernel::AccessoryRefusal as Why;

/// Where a node lies, as the walk passes into it.
#[derive(Clone, Copy, Default)]
struct Within {
    root: bool,
    route: bool,
    panel: bool,
    list: bool,
    top: bool,
    /// In a text node (`text`, `time`), as a run of its paragraph
    inline: bool,
    /// In a `list`, whose cells hold its rows
    rows: bool,
    /// In an `svg` or a `select`, which the platform draws as one
    drawn: bool,
}

/// An attribute's literal value, `Some(None)` where it is computed.
fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<Option<&'a str>> {
    attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .map(|a| match &a.value {
            Expr::Str(s, _) => Some(s.as_str()),
            _ => None,
        })
}

fn keyed(attrs: &[Attr]) -> bool {
    attrs.iter().any(|a| a.name == "navigationKey")
}

/// The refusals of the root tablist's literal `navigationTabAccessory`, where
/// the compiler can prove one: each element inside the navigation root and
/// outside its routes that says the attribute is checked, as an `if` may give
/// another arm's the kernel. For one a `for` repeats, only a name no element
/// can carry is refused: which iteration is the root's tablist, and so what
/// lies in it, is the live tree's.
pub(crate) fn check(checked: &Checked<'_>) -> Vec<LowerError> {
    let checking = Checking::new(checked);
    let view = &checking.expanded();
    view.events((false, false), |visit, &(inside, repeated)| {
        match visit.element() {
            Some((_, _, attrs)) if inside && keyed(attrs) => None,
            Some((_, _, attrs)) => Some((inside || keyed(attrs), repeated)),
            None => Some((inside, repeated || !visit.is_choice())),
        }
    })
    .filter_map(|(visit, (inside, repeated), entering)| {
        let (_, _, attrs) = visit.element().filter(|_| entering && inside)?;
        if keyed(attrs) {
            return None;
        }
        let declared = attrs
            .iter()
            .rev()
            .find(|a| a.name == "navigationTabAccessory")?;
        // On anything but a tablist the placement check (`check_accessory_place`) says what is wrong
        if literal(attrs, "role") != Some(Some("tablist")) {
            return None;
        }
        check_list(view, visit, declared, repeated).map(|message| LowerError {
            id: "lower-tab-accessory",
            message,
            span: declared.span,
        })
    })
    .collect()
}

/// Why the tablist `list`, which says `declared`, can show no accessory on any tree; only that the name
/// names nothing where a `for` repeats it (`repeated`).
fn check_list(
    view: &View<'_>,
    list: contract_checking::Visit<'_>,
    declared: &Attr,
    repeated: bool,
) -> Option<String> {
    let Expr::Str(name, _) = &declared.value else {
        return None;
    };
    if name.is_empty() {
        return None;
    }
    let within = |visit: contract_checking::Visit<'_>, outer: &Within| match visit.element() {
        // The root's own facts do not reach its children, where the kernel's walk starts
        Some((_, _, attrs)) if !outer.root => Within {
            root: keyed(attrs),
            ..Within::default()
        },
        Some((tag, _, attrs)) => Within {
            root: true,
            route: outer.route || keyed(attrs),
            panel: outer.panel || literal(attrs, "role") == Some(Some("tabpanel")),
            list: outer.list || visit.site() == list.site(),
            // A computed `popover` may be none, which leaves the box in the page
            top: outer.top || tag == "dialog" || matches!(literal(attrs, "popover"), Some(Some(_))),
            inline: outer.inline
                || crate::tags::tag(tag)
                    .is_some_and(|t| t.node_type == exact_kernel::NodeType::Text),
            rows: outer.rows
                || crate::tags::tag(tag)
                    .is_some_and(|t| t.node_type == exact_kernel::NodeType::List),
            drawn: outer.drawn
                || tag == "select"
                || crate::tags::tag(tag)
                    .is_some_and(|t| t.node_type == exact_kernel::NodeType::Svg),
        },
        None => *outer,
    };
    // Why the rule refuses each element with the id, and whether an element whose id the
    // compiler cannot read could carry it in a place the rule takes
    let mut carriers: Vec<Option<Why>> = Vec::new();
    let mut doubt = false;
    // For each open element, whether it holds the tablist, and whether it holds a panel
    let mut open: Vec<(bool, bool)> = Vec::new();
    for (visit, outer, entering) in
        view.events(Within::default(), |visit, outer| Some(within(visit, outer)))
    {
        let mine = within(visit, &outer);
        let Some((_, _, attrs)) = visit.element() else {
            // What an `if`, `for` or `match` holds is not on every tree, so it proves nothing
            if entering {
                open.push((false, false));
            } else {
                open.pop();
            }
            continue;
        };
        // Only what lies inside the navigation root; leave has enter's facts, so both skip it
        if !outer.root {
            continue;
        }
        if entering {
            open.push((false, false));
            continue;
        }
        let panel = mine.panel && !outer.panel && !outer.route;
        let (held_list, held_panel) = open.pop().unwrap_or_default();
        let (holds_list, holds_panel) = (
            held_list || visit.site() == list.site(),
            held_panel || panel,
        );
        if let Some(parent) = open.last_mut() {
            parent.0 |= holds_list;
            parent.1 |= holds_panel;
        }
        let holds = holds_list || holds_panel;
        // The kernel's walk takes no element in a route: the name then names nothing outside the routes
        if outer.route {
            continue;
        }
        let why = if mine.route {
            Some(Why::Route)
        } else if outer.list {
            Some(Why::InTablist)
        } else if outer.panel {
            Some(Why::InPanel)
        } else if holds {
            Some(Why::HoldsTabs)
        } else if mine.top {
            Some(Why::TopLayer)
        } else if outer.inline {
            Some(Why::Inline)
        } else if outer.rows {
            Some(Why::InList)
        } else if outer.drawn {
            Some(Why::Part)
        } else {
            None
        };
        match literal(attrs, "id") {
            Some(Some(id)) if id == name => carriers.push(why),
            // Where a `for` repeats the tablist, lying in it or holding it depends on the iteration
            Some(None)
                if why.is_none() || repeated && (outer.list || holds_list && !holds_panel) =>
            {
                doubt = true
            }
            _ => {}
        }
    }
    if doubt || carriers.iter().any(Option::is_none) || repeated && !carriers.is_empty() {
        return None;
    }
    Some(match carriers.first() {
        Some(why) => why.map(|why| why.message(name)).unwrap_or_default(),
        None => format!(
            "`navigationTabAccessory=\"{name}\"` names no element: nothing in the navigation root outside its routes has `id=\"{name}\"`, so the tab bar never shows an accessory. Give `id=\"{name}\"` to the element to show there, a row written after every tab panel the tablist links; write `navigationTabAccessory=\"\"` while there is none (docs/recipes/mini-player.md)"
        ),
    })
}
