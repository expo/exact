//! SVG attributes shared by the live host, documents and JS templates.

use super::{dom_id, Resolve};
use exact_kernel::{Kernel, NodeFacts, NodeRef, NodeType, PropId, SortedMap};

/// Nested viewports, use instances and paint servers do not consistently
/// read geometry CSS across browsers. Project these rows as SVG attributes
/// (LLP 1055.000 D4, D7).
pub fn attribute_rows(t: NodeType) -> &'static [(exact_kernel::StyleId, &'static str)] {
    use exact_kernel::StyleId::*;
    match t {
        NodeType::SvgViewport
        | NodeType::SvgUse
        | NodeType::SvgMask
        | NodeType::SvgPattern
        | NodeType::SvgForeignObject
        | NodeType::SvgFilter
        | NodeType::SvgFe => &[(X, "x"), (Y, "y"), (Width, "width"), (Height, "height")],
        NodeType::SvgRadialGradient => &[(Cx, "cx"), (Cy, "cy"), (R, "r")],
        _ => &[],
    }
}

/// An SVG element's references and attribute rows as the page takes them:
/// its `id` rewritten to its DOM id, `href` to its target's, and the rows
/// of [`attribute_rows`] as attributes.
pub(crate) fn svg_props(kernel: &Kernel, node: &NodeRef<'_>, out: &mut SortedMap<String, String>) {
    svg_props_of(&|from, id| kernel.resolve_id(from, id), &node.facts(), out)
}

/// [`svg_props`], from a node's facts and its tree's references.
pub fn svg_props_of(
    resolve: Resolve<'_>,
    node: &NodeFacts<'_>,
    out: &mut SortedMap<String, String>,
) {
    if !node.node_type.is_svg_element() {
        return;
    }
    if node.props.str(PropId::Id).is_some() {
        out.insert("id".into(), dom_id(node.id));
    }
    if let Some(target) = node
        .props
        .str(PropId::Href)
        .and_then(|h| h.strip_prefix('#'))
        .and_then(|h| resolve(node.id, h))
    {
        out.insert("href".into(), format!("#{}", dom_id(target)));
    }
    for (row, name) in attribute_rows(node.node_type) {
        if !node.style.mask.has(*row) {
            continue;
        }
        // Templates follow live fonts; use instances resolve these lengths
        // against the instance's inherited font.
        if let Some((unit, n)) = node.style.relative.get(*row) {
            let suffix = match unit {
                exact_kernel::style::relative::Unit::Rem => Some("rem"),
                exact_kernel::style::relative::Unit::Em => Some("em"),
                _ => None,
            };
            if let Some(suffix) = suffix {
                out.insert((*name).into(), format!("{}{suffix}", crate::css::num(n)));
                continue;
            }
        }
        let text = match node.style.get(*row) {
            exact_kernel::RowValue::Dimension(exact_kernel::Dimension::Points(v)) => {
                exact_num::Shortest(v as f64).to_string()
            }
            exact_kernel::RowValue::Dimension(exact_kernel::Dimension::Percent(p)) => {
                format!("{}%", exact_num::Shortest(p as f64))
            }
            exact_kernel::RowValue::Dimension(
                d @ exact_kernel::Dimension::Viewport(
                    exact_kernel::ViewportUnit::Vw
                    | exact_kernel::ViewportUnit::Vh
                    | exact_kernel::ViewportUnit::Vmin
                    | exact_kernel::ViewportUnit::Vmax,
                    _,
                ),
            ) => {
                let mut text = String::new();
                crate::css::dimension(&mut text, d);
                text
            }
            _ => continue,
        };
        out.insert((*name).into(), text);
    }
}
