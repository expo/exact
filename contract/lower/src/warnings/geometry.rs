//! `lower-geometry-id`: a `frame`, `rangeFrame` or `measure` whose literal id
//! no element in the view has. The read answers `unavailable` on every host,
//! which looks the same as a read before the first layout, so the mistake
//! shows only as a debugging session (#301: `frame("root")` against
//! `testId="root"`).
//!
//! A miswiring the compiler can see is a warning (@ref LLP 1116.010 D5), and
//! only what is certain is named: the reads are the expanded root's actions,
//! a child's among them, and the ids are every `id` the expanded view writes,
//! a child's among them. A computed `id` anywhere, or a use left unexpanded,
//! may name any element, so with one there is no warning; a computed argument
//! (a prop the caller fills included) is not checked.

use super::literal;
use crate::theme::scan::stmts;
use crate::LowerError;
use contract_syntax::{Expr, Node, Span};
use contract_types::Checked;
use std::collections::BTreeSet;

/// The geometry reads that take an element's `id` as their first argument.
const READS: [&str; 3] = ["frame", "rangeFrame", "measure"];

/// Each geometry read whose literal id no element of the view has.
pub(super) fn unnamed(checked: &Checked<'_>, out: &mut Vec<LowerError>) {
    let root = &checked.expanded.root;
    // A `fn` or an action of the same name is the author's, not the read.
    let own = |name: &str| {
        checked.file.fns.iter().any(|f| f.name == name)
            || root.actions.iter().any(|a| a.name == name)
    };
    let mut reads: Vec<(&str, String, Span)> = Vec::new();
    let mut seen = BTreeSet::new();
    for action in &root.actions {
        stmts(&action.body, &mut |e| {
            e.walk().for_each(|e| {
                if let Expr::Call(name, args, _) = e {
                    if let (Some(read), Some(Expr::Str(id, at))) =
                        (READS.iter().find(|r| *r == name), args.first())
                    {
                        // A child used twice is inlined twice: one warning for its source.
                        if !own(read) && seen.insert((at.source_id, at.line, at.col, id.clone())) {
                            reads.push((read, id.clone(), *at));
                        }
                    }
                }
            })
        });
    }
    if reads.is_empty() {
        return;
    }
    let Some(view) = Ids::of(&root.view) else {
        return;
    };
    for (read, id, at) in reads {
        if view.ids.contains(&id) {
            continue;
        }
        let repair = if view.test_ids.contains(&id) {
            format!("`testId=\"{id}\"` names an element for tests and the agent, not for a geometry read: give that element `id=\"{id}\"` as well")
        } else if let Some(near) =
            contract_syntax::suggestion(&id, view.ids.iter().map(String::as_str))
        {
            format!("did you mean `{read}(\"{near}\")`? Otherwise give the element to read `id=\"{id}\"`")
        } else {
            format!("give the element to read `id=\"{id}\"`")
        };
        out.push(LowerError {
            id: "lower-geometry-id",
            message: format!(
                "`{read}(\"{id}\")` names no element: nothing in the view has `id=\"{id}\"`, so the read answers `unavailable` on every host, as it does before the first layout; {repair}"
            ),
            span: at,
        });
    }
}

/// The literal ids and test ids of a view.
struct Ids {
    ids: BTreeSet<String>,
    test_ids: BTreeSet<String>,
}

impl Ids {
    /// `None` when an element's id is one the compiler cannot read.
    fn of(view: &[Node]) -> Option<Ids> {
        let mut found = Ids {
            ids: BTreeSet::new(),
            test_ids: BTreeSet::new(),
        };
        let mut stack: Vec<&Node> = view.iter().collect();
        while let Some(node) = stack.pop() {
            match node {
                Node::Element { attrs, .. } => {
                    if attrs.iter().any(|a| a.name == "id") {
                        found.ids.insert(literal(attrs, "id")?.to_string());
                    }
                    if let Some(test_id) = literal(attrs, "testId") {
                        found.test_ids.insert(test_id.to_string());
                    }
                }
                Node::Use { .. } => return None,
                _ => {}
            }
            stack.extend(node.children());
        }
        Some(found)
    }
}
