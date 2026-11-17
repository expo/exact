//! Where `-exact-min-zoom` and `-exact-max-zoom` apply (LLP 1057.001.000
//! D4): on a container that scrolls both axes and holds one element, which
//! the host draws scaled. Each refusal names the case.
use crate::error::err;
use crate::LowerError;
use contract_syntax::{Attr, Expr, Node, Span};
use exact_kernel::NodeType;

const ROWS: [&str; 2] = ["-exact-min-zoom", "-exact-max-zoom"];

/// The container's rows that would place its child (D4).
const PLACING: [&str; 6] = [
    "justify-content",
    "align-items",
    "align-content",
    "place-content",
    "place-items",
    "justify-items",
];
/// The child's own.
const PLACED: [&str; 3] = ["align-self", "justify-self", "place-self"];
const MARGINS: [&str; 5] = [
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
];

/// The elements `nodes` can put in their parent, each arm's.
fn elements(nodes: &[Node]) -> Vec<&[Attr]> {
    let mut out = Vec::new();
    for node in nodes {
        match node {
            Node::Element { attrs, .. } => out.push(attrs.as_slice()),
            Node::When {
                then, otherwise, ..
            } => {
                out.extend(elements(then));
                out.extend(elements(otherwise));
            }
            Node::Match { some, none, .. } => {
                out.extend(elements(&some.1));
                out.extend(elements(none));
            }
            _ => {}
        }
    }
    out
}

/// Whether a literal margin row says `auto` anywhere.
fn auto_margin(a: &Attr) -> bool {
    MARGINS.contains(&a.name.as_str())
        && matches!(&a.value, Expr::Str(s, _) if s.split_whitespace().any(|v| v == "auto"))
}

/// The most elements `nodes` can put in their parent at once, or the span of
/// a `for`, which can put any number.
fn most(nodes: &[Node]) -> Result<usize, Span> {
    let mut n = 0;
    for node in nodes {
        n += match node {
            Node::Element { .. } | Node::Use { .. } => 1,
            Node::Children { span } | Node::Each { span, .. } => return Err(*span),
            Node::When {
                then, otherwise, ..
            } => most(then)?.max(most(otherwise)?),
            Node::Match { some, none, .. } => most(&some.1)?.max(most(none)?),
        };
    }
    Ok(n)
}

/// A literal zoom: a number, or a percentage of 1.
fn literal(e: &Expr) -> Option<f64> {
    match e {
        Expr::Number(n, _) => Some(*n),
        Expr::Str(s, _) => {
            let s = s.trim();
            match s.strip_suffix('%') {
                Some(p) => p.trim().parse::<f64>().ok().map(|p| p / 100.0),
                None => s.parse().ok(),
            }
        }
        _ => None,
    }
}

pub(crate) fn check(
    tag: &str,
    node_type: NodeType,
    attrs: &[Attr],
    children: &[Node],
    span: Span,
) -> Result<(), LowerError> {
    let rows: Vec<&Attr> = attrs
        .iter()
        .filter(|a| ROWS.contains(&a.name.as_str()))
        .collect();
    let Some(first) = rows.first() else {
        return Ok(());
    };
    let virtualized = attrs
        .iter()
        .any(|a| a.name == "virtualized" && !matches!(a.value, Expr::Bool(false, _)));
    if node_type == NodeType::List || virtualized {
        return err(
            "lower-zoom-child",
            format!("`{}` on a `{tag}`: a zoomable container holds one element, which it scales; a list's rows are many. Put the list's content in one element inside a `scroll`", first.name),
            first.span,
        );
    }
    let y = if node_type.scrolls_by_default() {
        "auto"
    } else {
        "visible"
    };
    if !crate::overflow::Overflow::of(attrs, y).scrolls_both() {
        return err(
            "lower-zoom-overflow",
            format!("`{}` on a `{tag}` that does not scroll both ways: a zoomed child must reach every edge. Write `overflow=\"auto\"` (a `scroll` scrolls both ways unless an `overflow` row says otherwise)", first.name),
            first.span,
        );
    }
    match most(children) {
        Ok(1) => {}
        Ok(0) => {}
        Ok(n) => {
            return err(
                "lower-zoom-child",
                format!("`{}` on a `{tag}` that can hold {n} elements: a zoomable container scales its one child. Wrap them in one element", first.name),
                first.span,
            )
        }
        Err(at) => {
            return err(
                "lower-zoom-child",
                format!("`{}` on a `{tag}` whose children a `for` or a slot decides: a zoomable container scales its one child. Wrap them in one element", first.name),
                at,
            )
        }
    }
    let placed = |a: &&Attr| PLACING.contains(&a.name.as_str());
    let child = |a: &&Attr| PLACED.contains(&a.name.as_str()) || auto_margin(a);
    if let Some(a) = attrs.iter().find(placed).or_else(|| {
        elements(children)
            .into_iter()
            .flat_map(|c| c.iter())
            .find(child)
    }) {
        return err(
            "lower-zoom-align",
            format!("`{}` places the child of a zoomable container, which places it itself: centred while it is smaller than the port, as a photo viewer centres a photo (LLP 1057.001.000 D3). Leave it out", a.name),
            a.span,
        );
    }
    let value = |name: &str| {
        rows.iter()
            .find(|a| a.name == name)
            .and_then(|a| literal(&a.value))
    };
    if let (Some(min), Some(max)) = (value("-exact-min-zoom"), value("-exact-max-zoom")) {
        if max < min {
            return err(
                "lower-zoom-range",
                format!("`-exact-max-zoom` {max} is below `-exact-min-zoom` {min}: the range would hold the zoom at {min}. Write equal values for a fixed zoom"),
                span,
            );
        }
    }
    Ok(())
}
