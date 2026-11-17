//! `lower-row-own-list` (LLP 1017.003 D3): a row whose binding walks the
//! list its own `for` iterates (`tokens.slice(0, i)` in `for t, i in
//! tokens`), directly or through a `fn` that walks its parameter. Each row's
//! work grows with the list, so an update walks rows × items, and the list
//! step bound is per evaluation: such a binding passes every list the app
//! was tried with and is unset on a longer one. `lower-row-lookup` names a
//! row that searches another list. As there, only a list of the file's records
//! is named: a list of strings is most often a short set of choices.

use super::{path, WALKS};
use crate::LowerError;
use contract_checking::{Checking, View};
use contract_syntax::{Expr, FnDecl};
use contract_types::{infer, Shapes, Ty};
use std::collections::BTreeSet;

/// How deep a `fn` calling a `fn` is followed to a walk of its parameter.
const DEPTH: usize = 4;

struct Own<'a> {
    fns: &'a [FnDecl],
    shapes: &'a Shapes,
    /// The file's own shapes: a router's tabs are records too, but each of
    /// their rows is a screen.
    declared: BTreeSet<&'a str>,
    seen: BTreeSet<(u32, u32, u32, u32)>,
    out: Vec<LowerError>,
}

/// The receiver as written when `e` walks one: a roster walk's first
/// argument, or an argument a `fn` walks, as its parameter.
fn walked<'e>(e: &'e Expr, fns: &[FnDecl], depth: usize) -> Vec<(&'e Expr, String)> {
    let Expr::Call(name, args, _) = e else {
        return Vec::new();
    };
    if WALKS.contains(&name.as_str()) {
        let how = if contract_syntax::idioms::is_method(name) {
            format!("`.{name}`")
        } else {
            format!("`{name}(…)`")
        };
        return args.first().map(|a| (a, how)).into_iter().collect();
    }
    let Some(f) = fns.iter().find(|f| &f.name == name).filter(|_| depth > 0) else {
        return Vec::new();
    };
    args.iter()
        .zip(&f.params)
        .filter(|(_, p)| walks(&f.body, &p.name, fns, depth - 1))
        .map(|(a, _)| (a, format!("`{name}(…)`")))
        .collect()
}

/// Whether `e` walks the parameter `name` (never rebound in a `fn` body
/// but by an arrow, `let` or `match`, which are followed as `rows::reads`
/// follows them).
fn walks(e: &Expr, name: &str, fns: &[FnDecl], depth: usize) -> bool {
    if walked(e, fns, depth)
        .iter()
        .any(|(a, _)| matches!(a, Expr::Ident(n, _) if n == name))
    {
        return true;
    }
    e.scoped_operands()
        .any(|(x, binds)| !binds.contains(name) && walks(x, name, fns, depth))
}

impl Own<'_> {
    /// The walks `e` makes of a list in `rows` (each enclosing `for`'s list
    /// as written, and the name it starts from).
    fn expr(&mut self, e: &Expr, rows: &[(String, String)]) {
        for (receiver, how) in walked(e, self.fns, DEPTH) {
            let Some((list, _)) = path(receiver) else {
                continue;
            };
            if rows.iter().any(|(l, _)| *l == list) {
                let span = e.span();
                if self
                    .seen
                    .insert((span.source_id, span.line, span.col, span.end_col))
                {
                    self.out.push(LowerError {
                        id: "lower-row-own-list",
                        message: format!(
                            "each row of `{list}` walks `{list}` ({how}), so a row's work grows with the list: an update walks every row times every item, and past 65,536 list steps the row's binding is unset; count what a row needs once, in the data module, giving each row the field it reads (an offset, a position)"
                        ),
                        span,
                    });
                }
            }
        }
        for (x, binds) in e.scoped_operands() {
            let binds: Vec<&str> = binds.iter().collect();
            self.expr(x, &without(rows, &binds));
        }
    }

    fn nodes(&mut self, view: &View<'_>) {
        let shapes = self.shapes;
        let declared = self.declared.clone();
        for (node, _, rows) in view.scoped_with(Vec::<(String, String)>::new(), |node, scope, rows, binds| {
            let names: Vec<_> = binds.iter().collect();
            let mut inner = without(rows, &names);
            if let Some((_, _, _, list, _)) = node.repetition() {
                let records = matches!(infer(list, scope, shapes), Ok(Ty::List(t)) if matches!(*t, Ty::Record(ref n) if declared.contains(n.as_str())));
                if let Some((list, root)) = path(list).filter(|(_, root)| records && !names.contains(root)) {
                    inner.push((list, root.to_owned()));
                }
            }
            inner
        }) {
            if let Some((_, _, _, list, _)) = node.repetition() {
                self.expr(list, &rows);
            } else {
                for expr in node.expressions() { self.expr(expr, &rows); }
            }
        }
    }
}

/// `rows` less the lists a name in `binds` now shadows.
fn without(rows: &[(String, String)], binds: &[&str]) -> Vec<(String, String)> {
    rows.iter()
        .filter(|(_, root)| !binds.contains(&root.as_str()))
        .cloned()
        .collect()
}

/// `lower-row-own-list`'s sites in the expanded root's view.
pub(super) fn own_list(checking: &Checking<'_>) -> Vec<LowerError> {
    let Some(_) = checking.declarations().next() else {
        return Vec::new();
    };
    let mut own = Own {
        fns: checking.functions(),
        shapes: checking.shapes(),
        declared: checking.declared_shapes().collect(),
        seen: BTreeSet::new(),
        out: Vec::new(),
    };
    own.nodes(&checking.expanded());
    own.out
}
