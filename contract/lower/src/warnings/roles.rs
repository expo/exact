//! What a button or a role promises a person and VoiceOver, and the
//! element does not keep (LLP 1116.004 D4): a button with nothing to say,
//! a button in a button, and a dialog's role on a box; and a heading's
//! level with no heading (LLP 1115.000 D12.5), and a bar with only one tab.

use super::{computed, has, literal, BOXES};
use crate::LowerError;
use contract_syntax::{Attr, Expr, Node, Span, VisitSpans};
use std::collections::BTreeSet;

fn elements<'a>(nodes: &'a [Node], visit: &mut impl FnMut(&'a [Attr], Span)) {
    for node in nodes {
        match node {
            Node::Element {
                attrs,
                children,
                span,
                ..
            } => {
                visit(attrs, *span);
                elements(children, visit);
            }
            Node::When {
                then, otherwise, ..
            } => {
                elements(then, visit);
                elements(otherwise, visit);
            }
            Node::Each { body, .. } => elements(body, visit),
            Node::Match { some, none, .. } => {
                elements(&some.1, visit);
                elements(none, visit);
            }
            Node::Use { .. } | Node::Children { .. } => {}
        }
    }
}

/// The `aria-controls` the program writes (LLP 1166.000 D5), over the
/// expanded root, so a field in one file names a group in a component of
/// another and a literal a use passes through a prop is read: each id it can
/// evaluate to, with the element that writes it, and each one not a string
/// literal, without its source positions.
pub(super) struct Controlled<'a> {
    ids: BTreeSet<(Span, &'a str)>,
    exprs: Vec<(Span, Expr)>,
}

pub(super) fn controlled<'a>(checked: &'a contract_types::Checked<'_>) -> Controlled<'a> {
    let mut out = Controlled {
        ids: BTreeSet::new(),
        exprs: Vec::new(),
    };
    elements(&checked.expanded.root.view, &mut |attrs, span| {
        let Some(value) = attrs.iter().rev().find(|a| a.name == "aria-controls") else {
            return;
        };
        let mut ids = Vec::new();
        results(&value.value, &mut ids);
        out.ids.extend(ids.into_iter().map(|id| (span, id)));
        if !matches!(value.value, Expr::Str(..)) {
            out.exprs.push((span, unplaced(&value.value)));
        }
    });
    out
}

/// The ids a written `aria-controls` can name: the string literals it can
/// evaluate to, through `?:`, `match` and `let` (as `crate::roles` reads a
/// computed role), each an ARIA id list.
fn results<'a>(value: &'a Expr, out: &mut Vec<&'a str>) {
    match value {
        Expr::Ternary(_, yes, no, _)
        | Expr::Match {
            some: yes,
            none: no,
            ..
        } => {
            results(yes, out);
            results(no, out);
        }
        Expr::Let { body, .. } => results(body, out),
        Expr::Str(text, _) => out.extend(text.split_ascii_whitespace()),
        _ => {}
    }
}

/// An expression without its source positions, to compare with another.
fn unplaced(value: &Expr) -> Expr {
    let mut value = value.clone();
    value.visit_spans(&mut |span| *span = Span::default());
    value
}

/// The role that applies to a written `role` (WAI-ARIA 1.3 §5.4: the first
/// token that names a role, matched ASCII case-insensitively, as the web
/// host's `role_is` reads one), in ARIA's own spelling.
fn applied(written: &str) -> Option<&'static str> {
    written.split_ascii_whitespace().find_map(|token| {
        exact_kernel::ARIA_ROLES
            .iter()
            .find(|role| role.eq_ignore_ascii_case(token))
            .copied()
    })
}

/// Whether `nodes` hold a `key` or `keycapture` handler, through `if`, `for`
/// and `match`. The check runs on the expanded root, where a use and a slot's
/// fill are already inlined; one met unexpanded is given the benefit of the
/// doubt, as what it holds may handle the keys.
fn keyed(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| match node {
        Node::Element {
            attrs, children, ..
        } => has(attrs, "key") || has(attrs, "keycapture") || keyed(children),
        Node::When {
            then, otherwise, ..
        } => keyed(then) || keyed(otherwise),
        Node::Each { body, .. } => keyed(body),
        Node::Match { some, none, .. } => keyed(&some.1) || keyed(none),
        Node::Use { .. } | Node::Children { .. } => true,
    })
}

/// Composite widget roles whose keyboard behavior an author must supply
/// (LLP 1166.000 D5). The generated table removes roles with built-in keys:
/// the kernel matches its rows to the written role exactly, as the web host's
/// `[role="tablist"]` selectors do, so `role="Tablist"` or a fallback list
/// gets no keys and is warned of. A group another element names is a field's
/// popup, whose keys the field handles: its literal `id` is among the ids an
/// `aria-controls` can evaluate to, or its computed `id` is written as some
/// `aria-controls` is.
pub(super) fn keys(
    attrs: &[Attr],
    children: &[Node],
    span: Span,
    controlled: &Controlled<'_>,
) -> Option<LowerError> {
    const COMPOSITES: &[&str] = &[
        "radiogroup",
        "tablist",
        "menu",
        "listbox",
        "tree",
        "treegrid",
        "grid",
        "toolbar",
        "menubar",
    ];
    let written = literal(attrs, "role")?;
    let role = applied(written)?;
    let named = |id: &str| {
        controlled
            .ids
            .iter()
            .any(|(site, target)| *site != span && *target == id)
    };
    let same = |id: &Attr| {
        let id = unplaced(&id.value);
        controlled
            .exprs
            .iter()
            .any(|(site, written)| *site != span && *written == id)
    };
    let supplied = exact_kernel::ROLE_KEYS
        .iter()
        .any(|row| row.role == written);
    if !COMPOSITES.contains(&role)
        || supplied
        || role == "toolbar" && has(attrs, "toolbarPlacement")
        || literal(attrs, "id").is_some_and(named)
        || attrs
            .iter()
            .rev()
            .find(|a| a.name == "id")
            .is_some_and(|id| !matches!(id.value, Expr::Str(..)) && same(id))
        || has(attrs, "key")
        || has(attrs, "keycapture")
        || keyed(children)
    {
        return None;
    }
    let exactly = if exact_kernel::ROLE_KEYS.iter().any(|row| row.role == role) {
        format!(
            "; or write `role=\"{role}\"`, in lower case and alone, for the keys Exact supplies"
        )
    } else {
        String::new()
    };
    Some(LowerError {
        id: "lower-role-keys",
        message: format!(
            "Exact supplies no keys for `role=\"{written}\"`: each member is its own Tab stop and the arrows do nothing. Write the keys in a `key` action on the group or its members{exactly}"
        ),
        span,
    })
}

/// Whether an element names itself: a written `aria-label` or
/// `aria-labelledby` that is not the empty string.
fn named(attrs: &[Attr]) -> bool {
    ["aria-label", "aria-labelledby"].iter().any(|name| {
        attrs
            .iter()
            .rev()
            .find(|a| a.name == *name)
            .is_some_and(|a| !matches!(&a.value, Expr::Str(v, _) if v.trim().is_empty()))
    })
}

/// What a button's content gives it to say.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Says {
    /// Nothing drawn and nothing read.
    Nothing,
    /// An image or `svg` with no name: drawn, and read as nothing.
    Unnamed,
    /// Words, a name, or what the compiler cannot see through.
    Words,
}

/// The most any of `nodes` says. An `if`, `for` or `match` says what its
/// most telling arm does; a use or a slot, and an element this walk does
/// not know, are given the benefit of the doubt.
fn says(nodes: &[Node], in_svg: bool) -> Says {
    nodes
        .iter()
        .map(|node| match node {
            Node::Element {
                tag,
                positional,
                attrs,
                children,
                ..
            } => {
                if named(attrs) {
                    return Says::Words;
                }
                match tag.as_str() {
                    "text" | "tspan" => match positional.first() {
                        Some(Expr::Str(v, _)) if v.trim().is_empty() => says(children, in_svg),
                        Some(_) => Says::Words,
                        None => says(children, in_svg),
                    },
                    "image" => match attrs.iter().rev().find(|a| a.name == "alt") {
                        Some(a) if !matches!(&a.value, Expr::Str(v, _) if v.trim().is_empty()) => {
                            Says::Words
                        }
                        _ => Says::Unnamed,
                    },
                    // An `svg`'s shapes are drawn and read as nothing; its
                    // `text` is words.
                    "svg" => says(children, true).max(Says::Unnamed),
                    t if BOXES.contains(&t) => says(children, in_svg),
                    _ if in_svg => says(children, in_svg),
                    _ => Says::Words,
                }
            }
            Node::Use { .. } | Node::Children { .. } => Says::Words,
            node => node
                .child_lists()
                .into_iter()
                .map(|list| says(list, in_svg))
                .max()
                .unwrap_or(Says::Nothing),
        })
        .max()
        .unwrap_or(Says::Nothing)
}

/// What a `tag` element says of itself as a button: unlabelled, or holding
/// another.
pub(super) fn button(
    tag: &str,
    attrs: &[Attr],
    children: &[Node],
    span: Span,
) -> impl Iterator<Item = LowerError> {
    let button = tag == "button";
    let unlabelled = button.then(|| unlabelled_button(attrs, children, span));
    let nested = button.then(|| nested_button(children));
    unlabelled.flatten().into_iter().chain(nested.flatten())
}

/// `lower-button-unlabelled`: a `button` with no text anywhere in it, no
/// `aria-label` or `aria-labelledby`, and no image with an `alt` (LLP
/// 1116.004 D4). It is blank: water-tracker's `button press=drink(500)`
/// with no child drew an empty button, and VoiceOver reads one as
/// "button". A text of any expression, a symbol image with a name, and a
/// button hidden from assistive technology are not named.
fn unlabelled_button(attrs: &[Attr], children: &[Node], span: Span) -> Option<LowerError> {
    let hidden = attrs
        .iter()
        .rev()
        .find(|a| a.name == "aria-hidden")
        .is_some_and(|a| match &a.value {
            Expr::Bool(b, _) => *b,
            Expr::Str(v, _) => v != "false",
            _ => true,
        });
    if named(attrs) || hidden {
        return None;
    }
    let message = match says(children, false) {
        Says::Words => return None,
        Says::Unnamed => {
            "this `button` shows only an image with no name: a screen reader on the web reads it as just \"button\", and VoiceOver on iOS by the symbol's generic name (\"add\", \"love\"). Name it: `aria-label=\"Add\"` on the button, beside its `image \"symbol:…\"` (docs/recipes/symbols.md)"
        }
        Says::Nothing => {
            "this `button` holds no text and no `aria-label`, so it is blank: an empty button to a person, just \"button\" to VoiceOver. Put its words in it (a `text \"Add 500 ml\"` child), or name a symbol-only button with `aria-label` beside its `image \"symbol:…\"` (docs/recipes/symbols.md)"
        }
    };
    Some(LowerError {
        id: "lower-button-unlabelled",
        message: message.into(),
        span,
    })
}

/// The first `button` among `nodes` or under them, through `if`, `for`
/// and `match`: its span.
fn inner_button(nodes: &[Node]) -> Option<Span> {
    nodes.iter().find_map(|node| match node {
        Node::Element {
            tag,
            span,
            children,
            ..
        } => (tag == "button")
            .then_some(*span)
            .or_else(|| inner_button(children)),
        Node::Use { .. } | Node::Children { .. } => None,
        node => node.child_lists().into_iter().find_map(inner_button),
    })
}

/// `lower-nested-button`: a `button` inside a `button`, at the inner one.
/// HTML allows no interactive content in a button (HTML § 4.10.6). Every
/// host gives a press to the innermost button alone, as a React Native
/// `Pressable` inside another takes the touch (euxy's Note row and its
/// Listen pill, FRICTION 11), but the outer button's accessible name takes
/// in the inner one's words: the web reads the row as "Note Listen".
fn nested_button(children: &[Node]) -> Option<LowerError> {
    Some(LowerError {
        id: "lower-nested-button",
        message: "this `button` is inside another `button`. A press on it is its own (the outer one does not hear it, on every host), but the outer button's accessible name reads this one's words with its own, and HTML allows no button in a button. Put the two side by side: a `row` holding the outer's content as one `button` (`flex=1`) and this one after it, as a table cell holds its accessory button".into(),
        span: inner_button(children)?,
    })
}

/// `lower-dialog-role`: `role="dialog"` or `role="alertdialog"` on an
/// element that is not a `dialog` (LLP 1116.004 D4). The role is a label:
/// the box is drawn as written, with no Escape, no focus kept inside and
/// no iOS sheet (flashcards' editor, a `column role="dialog"
/// aria-modal=true`). A popover (light dismiss and Escape; with
/// `alertdialog`, the native confirmation) and a route the router
/// presents (`navigationPresentation`) are dialogs already; a computed
/// role is the benefit of the doubt.
pub(super) fn dialog_role(tag: &str, attrs: &[Attr], span: Span) -> Option<LowerError> {
    if tag == "dialog"
        || computed(attrs, "role")
        || has(attrs, "popover")
        || has(attrs, "navigationPresentation")
    {
        return None;
    }
    let role = literal(attrs, "role").filter(|r| matches!(*r, "dialog" | "alertdialog"))?;
    Some(LowerError {
        id: "lower-dialog-role",
        message: format!(
            "`role=\"{role}\"` on a `{tag}` names a dialog without making one: the box is drawn as written, with no Escape, no focus kept inside it (Tab reaches the page behind) and no iOS sheet. A modal you lay out (a form, an editor) is a sheet, a route with `navigationPresentation=\"modal\"`: UIKit's sheet on iOS, closed by Escape on the web, the page under it inert. A confirmation of text and buttons is `dialog role=\"alertdialog\"`, opened with `showModal(id)` (docs/recipes/navigation-stack.md)"
        ),
        span,
    })
}

/// `lower-level-without-heading`: an `aria-level` on an element that
/// writes no `role` (LLP 1115.000 D12.5). A level alone is not a heading:
/// no host tells assistive technology it is one (`Props::heading`),
/// lowering gives it no heading style (`heading_style` reads the role), and
/// the iOS navigation bar does not take it for the route's title, so a
/// header written `text "Inbox" aria-level=1` is body text under an
/// untitled bar. `treeitem`, `row` and `listitem` take a level of their
/// own, so any written role is left alone; a computed one is the benefit of
/// the doubt.
pub(super) fn level_without_heading(tag: &str, attrs: &[Attr], span: Span) -> Option<LowerError> {
    if !has(attrs, "aria-level") || has(attrs, "role") {
        return None;
    }
    Some(LowerError {
        id: "lower-level-without-heading",
        message: format!(
            "`aria-level` on a `{tag}` with no `role=\"heading\"` does not make a heading: a screen reader is not told it is one on any platform, it gets no heading style, and the iOS navigation bar does not take it for the screen's title. Write the role beside it: `text \"Results\" role=\"heading\" aria-level=2`"
        ),
        span,
    })
}
