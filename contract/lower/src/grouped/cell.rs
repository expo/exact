//! Whether a row is the platform's standard cell, as the kernel reads one
//! (`GroupedRow`): a leading symbol and a trailing accessory at most around
//! a title, a title and a value, or a `column` of one or two texts.

use super::inputs::column_row;
use super::{accessory, hidden, symbol};
use contract_syntax::{BinOp, Expr, Node, Span};

/// A row's shown parts between a leading symbol and a trailing accessory.
pub(super) fn texts(children: &[Node]) -> Vec<Node> {
    // What the kernel reads at a row's ends, or a condition between it and
    // nothing (a checkmark shown by an `if`).
    fn either(node: &Node, part: &dyn Fn(&Node) -> bool) -> bool {
        match node {
            Node::When {
                then, otherwise, ..
            } => [then, otherwise]
                .iter()
                .all(|arm| matches!(arm.as_slice(), [] | [_]) && arm.iter().all(part)),
            other => part(other),
        }
    }
    let leading = |n: &Node| symbol(n).is_some_and(|s| accessory(s).is_none());
    let trailing = |n: &Node| match n {
        Node::Element { tag, children, .. } => match tag.as_str() {
            "image" => symbol(n).is_some_and(|s| accessory(s).is_some()),
            // A checkbox or switch, as the kernel's toggle is.
            "input" => matches!(n, Node::Element { attrs, .. }
                if attrs.iter().any(|a| a.name == "type" && matches!(&a.value, Expr::Str(v, _) if v.eq_ignore_ascii_case("checkbox")))),
            "button" => {
                matches!(children.as_slice(), [only] if symbol(only).is_some_and(|s| s == "info.circle" || s == "info.circle.fill"))
            }
            _ => false,
        },
        _ => false,
    };
    let mut rest: Vec<Node> = children.iter().filter(|c| !hidden(c)).cloned().collect();
    if rest.first().is_some_and(|first| either(first, &leading)) {
        rest.remove(0);
    }
    if rest.last().is_some_and(|last| either(last, &trailing)) {
        rest.pop();
    }
    rest
}

/// Whether `nodes` show one text or two: a cell's title and its value or
/// subtitle.
pub(super) fn standard_text_lines(nodes: &[Node]) -> bool {
    text_lines(nodes).is_some_and(|(low, high)| low >= 1 && high <= 2)
}

/// Select the minimum from the number of visible text lines in a cell.
pub(super) fn row_minimum(nodes: &[Node], one: &str, two: &str, span: Span) -> Expr {
    match text_lines(nodes) {
        Some((2, 2)) => return super::mv(two, span),
        Some((1, 2)) => {}
        _ => return super::mv(one, span),
    }

    let zero_count = || Expr::Number(0.0, span);
    let is_text_choice = |node: &Node| matches!(node, Node::When { .. } | Node::Match { .. });
    // Accumulate the line count of each arm so the minimum follows mounted text.
    let mut counts = vec![[zero_count(), zero_count()]];
    let events = Node::walk_events_with(nodes, 0, |node, _| {
        if is_text_choice(node) {
            [Some(0), Some(1)]
        } else {
            [None, None]
        }
    });
    for (node, arm, entering) in events {
        if entering {
            if is_text_choice(node) {
                counts.push([zero_count(), zero_count()]);
            }
            continue;
        }
        let count = match node {
            Node::When { cond, .. } => {
                let [yes, no] = counts.pop().unwrap();
                Expr::Ternary(Box::new(cond.clone()), Box::new(yes), Box::new(no), span)
            }
            Node::Match { subject, some, .. } => {
                let [yes, no] = counts.pop().unwrap();
                Expr::Match {
                    subject: Box::new(subject.clone()),
                    var: some.0.clone(),
                    some: Box::new(yes),
                    none: Box::new(no),
                    span,
                }
            }
            _ => Expr::Number(if hidden(node) { 0.0 } else { 1.0 }, span),
        };
        let total = &mut counts.last_mut().unwrap()[arm];
        let previous = std::mem::replace(total, zero_count());
        *total = Expr::Binary(BinOp::Add, Box::new(previous), Box::new(count), span);
    }
    let [total, _] = counts.pop().unwrap();
    let has_subtitle = Expr::Binary(
        BinOp::Eq,
        Box::new(total),
        Box::new(Expr::Number(2.0, span)),
        span,
    );
    Expr::Ternary(
        Box::new(has_subtitle),
        Box::new(super::mv(two, span)),
        Box::new(super::mv(one, span)),
        span,
    )
}

/// How many texts `nodes` show, fewest and most, an `if` or a `match`
/// showing one of them included; `None` when anything else is among them.
pub(super) fn text_lines(nodes: &[Node]) -> Option<(usize, usize)> {
    nodes.iter().try_fold((0, 0), |(low, high), c| match c {
        Node::Element { tag, .. } if tag == "text" && hidden(c) => Some((low, high)),
        Node::Element { tag, .. } if tag == "text" => Some((low + 1, high + 1)),
        Node::When { .. } | Node::Match { .. } => {
            let [first, second] = c.child_lists();
            let (a, b) = (text_lines(first)?, text_lines(second)?);
            Some((low + a.0.min(b.0), high + a.1.max(b.1)))
        }
        _ => None,
    })
}

/// Whether a row's part is a column of one shown text.
pub(super) fn one_line(node: &Node) -> bool {
    matches!(node, Node::Element { tag, attrs, children, .. }
        if !hidden(node) && column_row(tag, attrs) && text_lines(children) == Some((1, 1)))
}

/// Whether a row's part is lines stacked in the row: a shown element that
/// lays out as a column around two parts or more (a column with nothing in
/// it is a box, as a cover's placeholder is). `part` reads an `if` through.
pub(super) fn stacks(node: &Node) -> bool {
    matches!(node, Node::Element { tag, attrs, children, .. }
        if !hidden(node) && column_row(tag, attrs) && children.iter().filter(|c| !hidden(c)).count() >= 2)
}

/// Omit the sheet only where the box value is known to be present.
/// Opaque expressions keep it: they may evaluate to `none`, which clears
/// the kernel's written bit and restores the standard subtitle cell.
fn without_box(
    attrs: &[contract_syntax::Attr],
    class: super::ClassRows<'_>,
    is_box: impl Fn(exact_kernel::StyleMask) -> bool,
    mut fallback: Expr,
) -> Expr {
    fn absent(value: &Expr, fallback: &Expr) -> Expr {
        match value {
            Expr::None(_) => fallback.clone(),
            Expr::Ternary(cond, yes, no, _) if matches!(cond.as_ref(), Expr::Bool(..)) => absent(
                if matches!(cond.as_ref(), Expr::Bool(true, _)) {
                    yes
                } else {
                    no
                },
                fallback,
            ),
            Expr::Ternary(cond, yes, no, span) => {
                let yes = absent(yes, fallback);
                let no = absent(no, fallback);
                if yes == no {
                    yes
                } else {
                    Expr::Ternary(cond.clone(), Box::new(yes), Box::new(no), *span)
                }
            }
            Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::Some(..) => {
                Expr::None(fallback.span())
            }
            _ => fallback.clone(),
        }
    }
    let classed = class(attrs)
        .ok()
        .flatten()
        .map(|(_, rows)| rows)
        .unwrap_or_default();
    // Resolve longhands before testing presence: an inline shorthand can
    // clear a class's corner, and a longhand can replace part of a shorthand.
    let mut seen = exact_kernel::StyleMask::default();
    for a in classed.iter().chain(attrs).rev() {
        let mut written = exact_kernel::StyleMask::default();
        for id in crate::tags::rows(a) {
            if !seen.has(id) && a.written {
                written.set(id);
            }
            seen.set(id);
        }
        if is_box(written) {
            fallback = absent(&a.value, &fallback);
        }
    }
    fallback
}

/// A subtitle-shaped column keeps its text spacing, and takes the cell's
/// block padding unless it or its row is known to write a box.
pub(super) fn column(
    node: &Node,
    row: &[contract_syntax::Attr],
    class: super::ClassRows<'_>,
) -> Node {
    let Node::Element { attrs, span, .. } = node else {
        return node.clone();
    };
    let padding = without_box(
        row,
        class,
        |w| exact_kernel::grouped::authored_row_box(w, false),
        super::mv("lines-padding-block", *span),
    );
    let padding = without_box(
        attrs,
        class,
        exact_kernel::grouped::authored_subtitle_box,
        padding,
    );
    super::lines::subtitle(node, padding)
}
