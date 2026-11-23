//! What a button or a role promises a person and VoiceOver, and the
//! element does not keep (LLP 1116.004 D4): a button with nothing to say,
//! a button in a button, and a dialog's role on a box; and a heading's
//! level with no heading (LLP 1115.000 D12.5), and a bar with only one tab.

use super::{computed, guards_in, has, literal, BOXES};
use crate::controls::{guarded, without_ascriptions};
use crate::LowerError;
use contract_checking::{Structure, View, Visit};
use contract_syntax::{Attr, Expr, Span, StyleDecl, VisitSpans};
use std::collections::BTreeSet;

/// The `aria-controls` the program writes (LLP 1166.000 D5), over the
/// expanded root, so a field in one file names a group in a component of
/// another and a literal a use passes through a prop is read: each id it can
/// evaluate to, with the element that writes it, and each one not a string
/// literal, without its source positions.
pub(super) struct Controlled<'a> {
    ids: BTreeSet<(Span, &'a str)>,
    exprs: Vec<(Span, Expr)>,
}

pub(super) fn controlled<'a>(view: &View<'a>) -> Controlled<'a> {
    let mut out = Controlled {
        ids: BTreeSet::new(),
        exprs: Vec::new(),
    };
    for (node, (), entering) in view.events((), |node, _| {
        (node.component_use().is_none() && !node.is_slot()).then_some(())
    }) {
        if !entering {
            continue;
        }
        let Some((_, _, attrs)) = node.element() else {
            continue;
        };
        let span = node.span();
        let Some(value) = attrs.iter().rev().find(|a| a.name == "aria-controls") else {
            continue;
        };
        let mut ids = Vec::new();
        results(&value.value, &mut ids);
        out.ids.extend(ids.into_iter().map(|id| (span, id)));
        if !matches!(value.value, Expr::Str(..)) {
            out.exprs.push((span, unplaced(&value.value)));
        }
    }
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
fn keyed(regions: [Structure<'_>; 2]) -> bool {
    regions
        .into_iter()
        .flat_map(|region| region.walk())
        .any(|node| {
            node.element()
                .is_some_and(|(_, _, attrs)| has(attrs, "key") || has(attrs, "keycapture"))
                || node.component_use().is_some()
                || node.is_slot()
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
    node: Visit<'_>,
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
        || keyed(node.regions())
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

/// A computed name or non-blank literal counts, as does an empty literal
/// its enclosing `if` rules out.
fn has_words(value: &Expr, guards: &[&Expr]) -> bool {
    !matches!(without_ascriptions(value), Expr::Str(v, _) if v.trim().is_empty())
        || guarded(value, guards.iter().copied())
}

fn named(attrs: &[Attr], guards: &[&Expr]) -> bool {
    ["aria-label", "aria-labelledby"].iter().any(|name| {
        attrs
            .iter()
            .rev()
            .find(|a| a.name == *name)
            .is_some_and(|a| has_words(&a.value, guards))
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
fn says<'a>(regions: [Structure<'a>; 2], guards: &[&'a Expr]) -> Says {
    let mut answer = Says::Nothing;
    for region in regions {
        for (node, (svg, guards)) in
            region.walk_regions_with((false, guards.to_vec()), |node, (svg, guards)| {
                if let Some((tag, positional, attrs)) = node.element() {
                    if named(attrs, guards) {
                        return [None, None];
                    }
                    let svg = match tag {
                        "text" | "tspan"
                            if positional.first().is_none_or(|e| !has_words(e, guards)) =>
                        {
                            *svg
                        }
                        "svg" => true,
                        t if t != "image" && (BOXES.contains(&t) || *svg) => *svg,
                        _ => return [None, None],
                    };
                    return [Some((svg, guards.clone())), None];
                }
                node.bindings()
                    .map(|bindings| Some((*svg, guards_in(guards, bindings))))
            })
        {
            let contribution = if let Some((tag, positional, attrs)) = node.element() {
                if named(attrs, &guards) {
                    Says::Words
                } else {
                    match tag {
                        "text" | "tspan" => match positional.first() {
                            Some(e) if has_words(e, &guards) => Says::Words,
                            _ => Says::Nothing,
                        },
                        "image" => match attrs.iter().rev().find(|a| a.name == "alt") {
                            Some(a) if has_words(&a.value, &guards) => Says::Words,
                            _ => Says::Unnamed,
                        },
                        "svg" => Says::Unnamed,
                        t if BOXES.contains(&t) || svg => Says::Nothing,
                        _ => Says::Words,
                    }
                }
            } else if node.component_use().is_some() || node.is_slot() {
                Says::Words
            } else {
                Says::Nothing
            };
            answer = answer.max(contribution);
            if answer == Says::Words {
                return answer;
            }
        }
    }
    answer
}

/// What a `tag` element says of itself as a button: unlabelled, or holding
/// another.
pub(super) fn button(
    tag: &str,
    attrs: &[Attr],
    node: Visit<'_>,
    span: Span,
    guards: &[&Expr],
) -> impl Iterator<Item = LowerError> {
    let button = tag == "button";
    let unlabelled = button.then(|| unlabelled_button(attrs, node, span, guards));
    let nested = button.then(|| nested_button(node));
    unlabelled.flatten().into_iter().chain(nested.flatten())
}

/// `lower-button-unlabelled`: a `button` with no text anywhere in it, no
/// `aria-label` or `aria-labelledby`, and no image with an `alt` (LLP
/// 1116.004 D4). It is blank: water-tracker's `button press=drink(500)`
/// with no child drew an empty button, and VoiceOver reads one as
/// "button". A text of any expression, a symbol image with a name, and a
/// button hidden from assistive technology are not named.
fn unlabelled_button(
    attrs: &[Attr],
    node: Visit<'_>,
    span: Span,
    guards: &[&Expr],
) -> Option<LowerError> {
    let hidden = attrs
        .iter()
        .rev()
        .find(|a| a.name == "aria-hidden")
        .is_some_and(|a| match &a.value {
            Expr::Bool(b, _) => *b,
            Expr::Str(v, _) => v != "false",
            _ => true,
        });
    if named(attrs, guards) || hidden {
        return None;
    }
    let message = match says(node.regions(), guards) {
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
fn inner_button(node: Visit<'_>) -> Option<Span> {
    for region in node.regions() {
        for (node, (), entering) in region.events((), |node, _| {
            (node.component_use().is_none() && !node.is_slot()).then_some(())
        }) {
            if entering && node.element().is_some_and(|(tag, _, _)| tag == "button") {
                return Some(node.span());
            }
        }
    }
    None
}

/// `lower-nested-button`: a `button` inside a `button`, at the inner one.
/// HTML allows no interactive content in a button (HTML § 4.10.6). Every
/// host gives a press to the innermost button alone, as a React Native
/// `Pressable` inside another takes the touch (euxy's Note row and its
/// Listen pill, FRICTION 11), but the outer button's accessible name takes
/// in the inner one's words: the web reads the row as "Note Listen".
fn nested_button(node: Visit<'_>) -> Option<LowerError> {
    Some(LowerError {
        id: "lower-nested-button",
        message: "this `button` is inside another `button`. A press on it is its own (the outer one does not hear it, on every host), but the outer button's accessible name reads this one's words with its own, and HTML allows no button in a button. Put the two side by side: a `row` holding the outer's content as one `button` (`flex=1`) and this one after it, as a table cell holds its accessory button".into(),
        span: inner_button(node)?,
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

pub(super) fn alert_font(tag: &str, attrs: &[Attr], styles: &[StyleDecl]) -> Option<LowerError> {
    if literal(attrs, "role") != Some("alertdialog") || !(tag == "dialog" || has(attrs, "popover"))
    {
        return None;
    }
    let class = crate::class::rows(
        attrs,
        &|name| {
            styles
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.attrs.as_slice())
        },
        &[],
    )
    .ok()
    .flatten()
    .map_or_else(Vec::new, |(_, rows)| rows);
    let family = attrs
        .iter()
        .chain(&class)
        .find(|a| a.written && a.name == "font-family")?;
    Some(LowerError {
        id: "lower-alert-font",
        message: "this alert's `font-family` applies on web, Linux, Android and Windows; iOS and macOS keep the system face because their native alerts have no public font API. Omit `font-family` here if the alert need not request a face".into(),
        span: family.span,
    })
}
