//! The HTML element a node's `semanticTag` names, where it names one: the
//! landmarks, `time`, `dialog`, `hr`, and HTML's table and description list;
//! and what data can give a node, beside what its literal props and styles do:
//! its tag, and the attributes element.rs `props_of` derives.

use exact_kernel::{NodeFacts, NodeType, PropId, StyleId};

/// The element for `tag`, a `semanticTag`; `None` where the node's type
/// decides (a `textarea`, an `option`, an `audio`).
pub(super) fn element(tag: &str) -> Option<&'static str> {
    ELEMENTS.iter().find(|e| **e == tag).copied()
}

/// The layout row whose presentation belongs to the HTML element. An option
/// remains visible in its native menu (LLP 1069.001 D2). A modal dialog is
/// fixed to the viewport unless its author positions it.
pub(super) fn browser_row(node: &NodeFacts<'_>) -> Option<StyleId> {
    if exact_kernel::control::is_option_node(node.node_type, node.props) {
        Some(StyleId::Display)
    } else if node.props.str(PropId::SemanticTag) == Some("dialog")
        && node.style.mask.has(StyleId::PositionType)
        && !node.style.written.has(StyleId::PositionType)
    {
        Some(StyleId::PositionType)
    } else {
        None
    }
}

const ELEMENTS: &[&str] = &[
    // HTML grouping and phrasing content.
    "p",
    "strong",
    "em",
    "blockquote",
    "ol",
    "ul",
    "li",
    // Landmarks, time and dialog.
    "time",
    "main",
    "header",
    "nav",
    "section",
    "footer",
    "article",
    "aside",
    "dialog",
    // @ref LLP 1021 D1 — HTML's separator, void: the kernel gives it no
    // children, and its UA rows are the node's own.
    "hr",
    // @ref LLP 1153 D7 — HTML's own elements, laid out by the browser's
    // table algorithm and its UA sheet (`display: table` and the rest);
    // the plan carries no grid rows for them.
    "table",
    "caption",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "dl",
    "dt",
    "dd",
];

/// The tags element.rs `element` can give a node of type `node_type` as data
/// sets `prop`, where `prop` decides the tag, and none where it does not. The
/// page's sheet keeps the rules for every possible tag (exact-web-js `style::bound_tokens`,
/// LLP 1047.003 D1). A change to `element` changes this list with it.
pub fn tags_by_data(node_type: NodeType, prop: PropId) -> Vec<&'static str> {
    match (node_type, prop) {
        (NodeType::Pressable, PropId::Href) => vec!["a", "button"],
        (NodeType::Text, PropId::Href) => vec!["a", "span"],
        (NodeType::Text, PropId::AccessibilityRole | PropId::AccessibilityHeadingLevel) => {
            vec!["h1", "h2", "h3", "h4", "h5", "h6", "div"]
        }
        (NodeType::Control, PropId::Value | PropId::Start | PropId::End) => vec!["progress", "div"],
        (_, PropId::SemanticTag) => ELEMENTS
            .iter()
            .copied()
            .chain(["textarea", "audio", "div"])
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether HTML's UA sheet makes the element `tag` names something other
/// than a block (`table-cell`, `table-caption`): a box that holds a folded
/// text keeps that display, where a `div` is made a block (`blocks`).
pub(super) fn not_block(tag: &str) -> bool {
    matches!(
        tag,
        "li" | "table" | "caption" | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td"
    )
}

/// The attributes element.rs `props_of` can derive for a node of type
/// `node_type` from its props and styles, beyond the one a prop names for
/// itself (`role`, `aria-checked`). Where data sets a prop or a style, the JS
/// target's compiled module writes the node's attributes from its literal
/// rows alone and the Rust renderer from the data's, so its page's sheet keeps
/// the rules of each (exact-web-js `style::bound_tokens`, LLP 1047.003 D1).
/// A test in `element_tests.rs` fails when `props_of` writes, by name, an
/// attribute this list lacks.
pub fn attributes_by_data(node_type: NodeType) -> Vec<&'static str> {
    let mut names = vec![
        "allow",
        "data-exact-list",
        "data-exact-said",
        "data-exact-text",
        "data-scroll",
        "data-time-shown",
        "data-wrap-flow",
        "datetime",
        "markupPieces",
        "readonly",
        "rel",
        "target",
        "type",
    ];
    names.extend(match node_type {
        NodeType::Control => &[
            "data-button-style",
            "data-exact-control",
            "data-exact-field",
            "data-exact-progress",
            "data-exact-switch",
            "data-native",
            "switch",
        ][..],
        NodeType::TextInput => &["data-exact-textfield", "data-native"],
        NodeType::Image => &[
            "alt",
            "data-symbol-evenodd",
            "data-symbol-file",
            "data-symbol-fill",
            "data-symbol-nonzero",
            "data-symbol-path",
            "data-symbol-source",
            "data-symbol-viewbox",
            "src",
        ],
        _ => &[],
    });
    names
}
