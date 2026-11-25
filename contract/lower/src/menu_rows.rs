//! Menu row positions and the `lower-menu-row-popover` note.
//! The note describes a row of a menu (`role="menu"`) whose
//! `popovertarget` names a popover that is no menu. exact2's menu rule
//! (LLP 1021; theme2 2026-10-21 19:47, 22:05) opens it as if by the menu's
//! own invoker, so the menu closing as its row is chosen doesn't take it
//! along: the note says where it opens, and blames nothing. Only rows the
//! source settles: literal ids, roles, targets and actions (`commandfor` as
//! well as `popovertarget`); a confirmation is the platform's own.

use crate::LowerError;
use contract_checking::View;
use contract_syntax::{Attr, Expr};
use std::collections::BTreeMap;

fn has(attrs: &[Attr], name: &str) -> bool {
    attrs.iter().any(|a| a.name == name)
}

/// An attribute's value when it is a string literal (the last one written).
fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a str> {
    match attrs
        .iter()
        .rev()
        .find(|a| a.name == name)
        .map(|a| &a.value)
    {
        Some(Expr::Str(s, _)) => Some(s.as_str()),
        _ => None,
    }
}

/// A popover and its immediate sections can each supply native menu rows (LLP 1021 D3).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Parent {
    Popover,
    Section,
}

impl Parent {
    pub(super) fn of_children(parent: Option<Self>, button: bool, attrs: &[Attr]) -> Option<Self> {
        if has(attrs, "popover") {
            Some(Self::Popover)
        } else if parent == Some(Self::Popover)
            && !button
            && literal(attrs, "role") == Some("group")
        {
            Some(Self::Section)
        } else {
            None
        }
    }
}

/// What a popover is, as far as its source says.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A literal `role="menu"`, or a confirmation (`alertdialog`, a `dialog`): the platform's own.
    Menu,
    /// No role, or a literal one of another kind: the rule's.
    Other,
    /// A role the source doesn't settle: no note.
    Unknown,
}

/// Each popover's literal id, and what it is.
fn popovers<'a>(nodes: &View<'a>, out: &mut BTreeMap<&'a str, Kind>) {
    for n in nodes.walk() {
        let Some((tag, _, attrs)) = n.element() else {
            continue;
        };
        if let (true, Some(id)) = (
            has(attrs, "popover") || tag == "dialog",
            literal(attrs, "id"),
        ) {
            let kind = match (tag, has(attrs, "role"), literal(attrs, "role")) {
                ("dialog", ..) | (_, _, Some("menu" | "alertdialog")) => Kind::Menu,
                (_, false, _) | (_, true, Some(_)) => Kind::Other,
                (_, true, None) => Kind::Unknown,
            };
            // Two arms that say different things leave it unsettled.
            let settled = *out.entry(id).or_insert(kind);
            if settled != kind {
                out.insert(id, Kind::Unknown);
            }
        }
    }
}

/// The literal id a button opens, as HTML resolves its invocation:
/// `commandfor` with a showing `command` before `popovertarget` without
/// `hide`; none for a hide, a value the source doesn't settle, or neither.
fn opens(attrs: &[Attr]) -> Option<&str> {
    // An empty `commandfor` names nothing, and HTML falls back to `popovertarget`.
    if has(attrs, "commandfor") && literal(attrs, "commandfor") != Some("") {
        let command = literal(attrs, "command")?.to_ascii_lowercase();
        return matches!(command.as_str(), "show-popover" | "toggle-popover")
            .then(|| literal(attrs, "commandfor"))
            .flatten();
    }
    if has(attrs, "popovertargetaction")
        && literal(attrs, "popovertargetaction")?.eq_ignore_ascii_case("hide")
    {
        return None;
    }
    literal(attrs, "popovertarget")
}

/// The notes for `nodes`, the root's view.
pub(crate) fn notes(view: &View<'_>, out: &mut Vec<LowerError>) {
    let mut ids = BTreeMap::new();
    popovers(view, &mut ids);
    for (node, in_menu) in view.walk_with(false, |node, in_menu| {
        *in_menu
            || node.element().is_some_and(|(_, _, attrs)| {
                has(attrs, "popover") && literal(attrs, "role") == Some("menu")
            })
    }) {
        let Some((tag, _, attrs)) = node.element() else {
            continue;
        };
        if let (true, "button", Some(target)) = (in_menu, tag, opens(attrs)) {
            if ids.get(target) == Some(&Kind::Other) {
                out.push(LowerError {
                    id: "lower-menu-row-popover",
                    message: format!("note: this menu row's `{target}` opens from the menu's own invoker (the button that opened the menu, or the node its context menu opened on), not nested in the menu, so it stays open as the menu closes on choose, unless it is written inside the menu: exact2's menu rule (docs/contract-for-agents.md)"),
                    span: node.span(),
                });
            }
        }
    }
}
