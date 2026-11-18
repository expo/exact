//! What a plan uses on Apple, which links its Swift families and the
//! runner's seams by use (LLP 1047.001 D10, D11).
//!
//! @ref LLP 1047.001 D10 (a family's use rule covers every plan that reaches its code)
//!
//! [`uses`] is what the web, Android and Linux link and admit, and their
//! compatibility ids name it. The native roster's entries and Apple's own
//! reading of a shared one are added here, so those hosts do not move.
//! What the bake alone can know (an image file's bytes, the app's module
//! artifact) it adds to the set itself (`exact_bake`'s `production_uses`).

use super::{constant_str, runs_command, uses, Capability, Uses};
use exact_kernel::{NodeType, PropId, StyleId};
use exact_plan::{BindingKind, BindingsRow, Plan};

/// What `plan` uses on Apple: [`uses`], with [`Capability::Grid`] the grid
/// algorithm, which Apple links by use, for a plan whose layout can reach it
/// ([`lays_out_grid`]), [`Capability::Effects`] a box's `filter` or what
/// SVG uses, and the native roster's entries. A native host
/// parses the grid rows whatever it links, so a grid row alone does not need
/// the algorithm. The browser lays out a grid itself, and the other native
/// hosts link the algorithm whole, so for them the capability is the rows'
/// grammars.
pub fn apple_uses(plan: &Plan) -> Uses {
    let mut uses = uses(plan).beyond(Uses::NONE.with(Capability::Grid).with(Capability::Effects));
    let mut add = |capability, used: bool| {
        if used {
            uses = uses.with(capability);
        }
    };
    add(Capability::Grid, lays_out_grid(plan));
    // A box's `filter` is the family's; `clip-path`, which the web's entry
    // counts too, is the core's mask on Apple.
    add(
        Capability::Effects,
        plan.bindings.iter().any(|b| {
            b.kind == BindingKind::Style
                && StyleId::from_bit(u32::from(b.id)) == Some(StyleId::Filter)
        }),
    );
    // An event-only or a view-only package reaches the module seam too.
    add(
        Capability::Modules,
        !plan.modules.is_empty() || !plan.views.is_empty() || !plan.module_events.is_empty(),
    );
    add(Capability::Tools, !plan.tools.is_empty());
    let images = image_sources(plan);
    add(Capability::Images, !images.is_empty());
    let svg = images.iter().any(ImageSource::can_be_svg)
        || has_node(plan, |t| t == NodeType::Svg || t.is_svg_element());
    add(Capability::Svg, svg);
    // SVG's filters run on box effects' engine.
    add(Capability::Effects, svg);
    add(
        Capability::NativeViews,
        !plan.views.is_empty()
            || has_node(plan, |t| t == NodeType::NativeView)
            || binds(plan, BindingKind::Prop, PropId::Hatch as u16),
    );
    add(
        Capability::Traces,
        runs_command(plan, &["startTrace_EXPERIMENTAL", "stopTrace_EXPERIMENTAL"]),
    );
    add(Capability::Controls, uses_controls(plan));
    // A swipe row, a reorder group, a reorder grip and a list whose rows
    // are cells (iOS draws them, LLP 1115.000 D16) reach the family without
    // a windowed list.
    add(
        Capability::Collections,
        [
            PropId::SwipeContent,
            PropId::ReorderGroup,
            PropId::ReorderFor,
            PropId::ListRows,
        ]
        .into_iter()
        .any(|id| binds(plan, BindingKind::Prop, id as u16)),
    );
    uses
}

/// Whether laying out `plan` can reach the grid algorithm: a `display` that
/// can be `grid`, or a node that can be a table's part (`table`, `caption`,
/// a row group, `tr`, `th`, `td`), which the kernel lays out as a grid,
/// with an anonymous table around a part outside one (LLP 1153 D7.3). A
/// computed value counts as either.
pub fn lays_out_grid(plan: &Plan) -> bool {
    plan.bindings.iter().any(|binding| match binding.kind {
        BindingKind::Style => {
            StyleId::from_bit(u32::from(binding.id)) == Some(StyleId::Display)
                && can_be(plan, &binding, |v| v.trim().eq_ignore_ascii_case("grid"))
        }
        BindingKind::Prop => {
            PropId::from_wire(binding.id) == Some(PropId::SemanticTag)
                && can_be(plan, &binding, |v| {
                    matches!(
                        v,
                        "table" | "caption" | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td"
                    )
                })
        }
    })
}

/// An image file the host may load, as the plan names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSource<'a> {
    /// A source the plan computes, or a Markdown atom's icon.
    Computed,
    /// A literal source.
    Literal(&'a str),
}

impl ImageSource<'_> {
    /// Whether the file can be an SVG as far as the plan says. The host
    /// tells an SVG by its bytes, not its name, so a computed source can, and
    /// so can a download and an app file (a picked photo), whose bytes come
    /// at run time. A bundled asset's or a `data:` URL's bytes are the
    /// bake's to read ([`ImageSource::bundled`]).
    pub fn can_be_svg(&self) -> bool {
        self.bundled().is_none()
    }

    /// The literal source whose bytes ship with the app: a bundled asset or
    /// a `data:` URL.
    pub fn bundled(&self) -> Option<&str> {
        match *self {
            ImageSource::Literal(s)
                if !s.starts_with("http:")
                    && !s.starts_with("https:")
                    && !s.starts_with("app:/") =>
            {
                Some(s)
            }
            _ => None,
        }
    }
}

/// The image files `plan` can load (Apple's `RasterLoader`): each `image`'s
/// source but a `symbol:` icon, which loads no file (the empty source is
/// loaded, and fails with an `error`), and a computed one for each Markdown atom's icons (`atomFaces`), whose
/// record the plan builds.
pub fn image_sources(plan: &Plan) -> Vec<ImageSource<'_>> {
    let mut sources = Vec::new();
    for node in plan.nodes.iter() {
        if NodeType::from_wire(node.node_type) != Some(NodeType::Image) {
            continue;
        }
        let source = node
            .bindings
            .iter()
            .map(|b| plan.binding(b))
            .find(|b| b.kind == BindingKind::Prop && b.id == PropId::ImageSource as u16);
        let Some(source) = source else { continue };
        match constant_str(plan, plan.code(source.expr)) {
            Some(s) if s.starts_with("symbol:") => {}
            Some(s) => sources.push(ImageSource::Literal(s)),
            None => sources.push(ImageSource::Computed),
        }
    }
    if binds(plan, BindingKind::Prop, PropId::AtomFaces as u16) {
        sources.push(ImageSource::Computed);
    }
    sources
}

/// Whether `plan` can show an optional native control: a `Control` whose
/// `type` can be other than a button or a file input (the picker's), a
/// number field's stepper, or a segmented control (a `tablist` or a
/// `radiogroup`, which Apple's segment host takes).
fn uses_controls(plan: &Plan) -> bool {
    let prop = |node: &exact_plan::NodesRow, id: PropId| {
        node.bindings
            .iter()
            .map(|b| plan.binding(b))
            .find(|b| b.kind == BindingKind::Prop && b.id == id as u16)
    };
    plan.nodes
        .iter()
        .any(|node| match NodeType::from_wire(node.node_type) {
            Some(NodeType::Control) => prop(&node, PropId::Type)
                .is_none_or(|t| can_be(plan, &t, |v| !matches!(v, "button" | "file"))),
            Some(NodeType::TextInput) => {
                prop(&node, PropId::Type).is_some_and(|t| can_be(plan, &t, |v| v == "number"))
            }
            _ => false,
        })
        || plan.bindings.iter().any(|b| {
            b.kind == BindingKind::Prop
                && b.id == PropId::AccessibilityRole as u16
                && can_be(plan, &b, |v| matches!(v, "tablist" | "radiogroup"))
        })
}

/// Whether a binding's value can be one `used` accepts: a constant that it
/// accepts, or a computed one, which can be anything.
fn can_be(plan: &Plan, binding: &BindingsRow, used: impl Fn(&str) -> bool) -> bool {
    constant_str(plan, plan.code(binding.expr)).is_none_or(used)
}

fn has_node(plan: &Plan, of: impl Fn(NodeType) -> bool) -> bool {
    plan.nodes
        .iter()
        .any(|node| NodeType::from_wire(node.node_type).is_some_and(&of))
}

fn binds(plan: &Plan, kind: BindingKind, id: u16) -> bool {
    plan.bindings.iter().any(|b| b.kind == kind && b.id == id)
}

#[cfg(test)]
#[path = "apple_tests.rs"]
mod tests;

/// A `data:` URL's bytes after `data:` (RFC 2397): base64 after `;base64`,
/// else percent-decoded.
pub fn data_url(url: &str) -> Option<Vec<u8>> {
    let (meta, body) = url.split_once(',')?;
    if meta.to_ascii_lowercase().ends_with(";base64") {
        let body: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        return crate::agent::unbase64(&body);
    }
    let (body, mut bytes, mut i) = (body.as_bytes(), Vec::new(), 0);
    while i < body.len() {
        let hex = body
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex
            .filter(|_| body[i] == b'%')
            .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            Some(byte) => (bytes.push(byte), i += 3),
            None => (bytes.push(body[i]), i += 1),
        };
    }
    Some(bytes)
}
