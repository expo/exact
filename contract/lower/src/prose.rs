//! HTML's prose elements keep their element name in the plan. Paragraphs
//! and emphasis use the same text runs as an authored `text`.

use crate::tags::Tag;
use crate::{err, LowerError};
use contract_syntax::Span;
use exact_kernel::{NodeType, PropId, StyleId};

pub(crate) fn tag(name: &str) -> Tag {
    let props: &[(PropId, &str)] = match name {
        "p" => &[(PropId::SemanticTag, "p")],
        "strong" => &[(PropId::SemanticTag, "strong")],
        "em" => &[(PropId::SemanticTag, "em")],
        "blockquote" => &[(PropId::SemanticTag, "blockquote")],
        "ul" => &[(PropId::SemanticTag, "ul")],
        "ol" => &[(PropId::SemanticTag, "ol")],
        "li" => &[(PropId::SemanticTag, "li")],
        _ => unreachable!("prose tag"),
    };
    let text = matches!(name, "p" | "strong" | "em");
    Tag {
        node_type: if text { NodeType::Text } else { NodeType::View },
        fixed_styles: if name == "em" {
            &[(StyleId::FontStyle, "italic")]
        } else {
            &[]
        },
        fixed_props: props,
        positional: text.then_some(PropId::Text),
    }
}

pub(crate) fn phrasing(tag: &str) -> bool {
    matches!(tag, "text" | "time" | "strong" | "em")
}

pub(crate) fn text_container(tag: &str) -> bool {
    tag == "p" || phrasing(tag)
}

pub(crate) fn check(tag: &str, parent: Option<&str>, span: Span) -> Result<(), LowerError> {
    if let Some(list @ ("ul" | "ol")) = parent {
        if tag != "li" {
            return err(
                "lower-list-content",
                format!("a `{list}` holds `li` elements; put `{tag}` inside an `li`"),
                span,
            );
        }
    } else if tag == "li" {
        return err(
            "lower-list-content",
            "an `li` belongs in a `ul` or `ol`",
            span,
        );
    }
    Ok(())
}
