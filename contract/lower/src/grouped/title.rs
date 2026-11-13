//! @ref LLP 1116.021 D4 — a row whose tap area is a `button` or `link`
//! holding its title, with a later text beside it as the value (a checkbox,
//! the item's name as the button that toggles it, its quantity): the button
//! is the title, as a hand-built cell's title label is. It grows, its face
//! is leading and centred in the row's height, and the later text is the
//! value at the far edge. Before, the first `text` straight in the row was
//! the title, so the quantity grew mid-row and the name sat centred at the
//! top of its button (grocery-list, round 19).
//!
//! A lone button row stays an action cell (`actions::action_cell`); a
//! button that lays out as a column is a stack of lines (`super::stacks`).
//! Sheet rows, so an authored `text-align`, `align-items` or `flex` wins.
//! Where the platform's form shows the button at its own size (the Mac's
//! push button) it neither grows nor moves its face: the metrics say so.

use super::{column_row, hidden, m, mv, n, s, symbol, with};
use contract_syntax::Node;

/// Where among `children` the row's title button stands, if it has one: the
/// first `button` or `link` with only symbols and inputs shown before it,
/// whose shown face is texts and symbols, one text at least, and with a
/// shown text after it among the row's parts. An `if` counts by its arms, as
/// `leading` has it: each arm passes, and the face has a text if one arm does.
pub(super) fn title_button(children: &[Node]) -> Option<usize> {
    let text = |n: &Node| matches!(n, Node::Element { tag, .. } if tag == "text") && !hidden(n);
    let at = children.iter().position(|c| {
        !hidden(c) && matches!(c, Node::Element { tag, .. } if tag == "button" || tag == "link")
    })?;
    let before = every_shown(&children[..at], &|c| {
        matches!(c, Node::Element { tag, .. } if tag == "input") || symbol(c).is_some()
    });
    let Node::Element {
        tag,
        attrs,
        children: face,
        ..
    } = &children[at]
    else {
        return None;
    };
    let face_ok = any_shown(face, &text)
        && every_shown(face, &|c| text(c) || symbol(c).is_some())
        && !column_row(tag, attrs)
        // A styled button (bordered, filled) beside a value is a control.
        && !attrs.iter().any(|a| a.name == "buttonStyle" || a.name == "appearance");
    let value = any_shown(&children[at + 1..], &text);
    (before && face_ok && value).then_some(at)
}

/// Whether every shown node of `nodes`, through each arm of an `if`, is `ok`.
fn every_shown(nodes: &[Node], ok: &dyn Fn(&Node) -> bool) -> bool {
    nodes.iter().filter(|c| !hidden(c)).all(|c| match c {
        Node::When {
            then, otherwise, ..
        } => every_shown(then, ok) && every_shown(otherwise, ok),
        other => ok(other),
    })
}

/// Whether some shown node of `nodes`, in some arm of an `if`, is `ok`.
fn any_shown(nodes: &[Node], ok: &dyn Fn(&Node) -> bool) -> bool {
    nodes.iter().filter(|c| !hidden(c)).any(|c| match c {
        Node::When {
            then, otherwise, ..
        } => any_shown(then, ok) || any_shown(otherwise, ok),
        other => ok(other),
    })
}

/// The title button's sheet: the list's press face, as a button row is,
/// so its words are the row's title in the label's colour rather than a
/// native button's tint; it grows as a title does (the Mac's push button
/// does not), and its face is at the leading edge and in the middle of the
/// row's height.
pub(super) fn title_face(node: &Node) -> Node {
    let Node::Element { span, .. } = node else {
        return node.clone();
    };
    let at = *span;
    with(
        node,
        vec![
            s("button-context", "grouped-list row title", at),
            super::attr("flex-grow", mv("button-title-grow", at), at),
            n("min-width", 0.0, at),
            s("align-items", "center", at),
            m("text-align", "title-button-align", at),
        ],
    )
}
