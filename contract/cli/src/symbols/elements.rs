//! Where each view element stands: its tag, and its attributes' names and
//! string values.

use super::Resolver;
use crate::sources::Sources;
use contract_syntax::*;
use serde_json::json;

/// An authored view element: its tag and each attribute it was written with.
pub(super) struct Element {
    tag: String,
    span: Span,
    component: Option<String>,
    attrs: Vec<ElementAttr>,
}
struct ElementAttr {
    name: String,
    span: Span,
    /// A string literal value and where it is, quotes included.
    string: Option<(String, Span)>,
}

impl Resolver<'_> {
    pub(super) fn element(&mut self, tag: &str, span: Span, attrs: &[Attr]) {
        if !self.graph.editor {
            return;
        }
        // A button's one positional is the parser's `text` child, at the button's tag.
        if self.graph.elements.last().is_some_and(|e| e.span == span) {
            return;
        }
        // A heading's level is written at its tag (`h2`), not as attributes.
        let attrs = (attrs.iter().filter(|a| a.written && a.span != span))
            .map(|a| ElementAttr {
                name: a.name.clone(),
                span: a.span,
                string: match &a.value {
                    Expr::Str(value, at) => Some((value.clone(), *at)),
                    _ => None,
                },
            })
            .collect();
        self.graph.elements.push(Element {
            tag: tag.into(),
            span,
            component: self.component.map(|c| c.name.clone()),
            attrs,
        });
    }
}

pub(super) fn write(out: &mut Vec<u8>, sources: &Sources, elements: &[Element]) {
    let place = |span: Span| json!({ "col": span.col, "end_col": span.end_col, "line": span.line });
    for (i, element) in elements.iter().enumerate() {
        if i != 0 {
            out.push(b',');
        }
        let attrs: Vec<_> = (element.attrs.iter())
            .map(|a| {
                let mut attr = place(a.span);
                attr["name"] = a.name.as_str().into();
                if let Some((value, span)) = &a.string {
                    attr["string"] = place(*span);
                    attr["string"]["value"] = value.as_str().into();
                }
                attr
            })
            .collect();
        let mut one = place(element.span);
        one["attrs"] = attrs.into();
        one["file"] = sources.path(element.span).to_string_lossy().into();
        one["tag"] = element.tag.as_str().into();
        if let Some(component) = &element.component {
            one["component"] = component.as_str().into();
        }
        serde_json::to_writer(&mut *out, &one).expect("writing JSON into a Vec cannot fail");
    }
}
