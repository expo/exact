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
use contract_syntax::{Expr, FnDecl, Node};
use contract_types::{infer, Ref, Scope, Shapes, Ty};
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

    fn nodes(&mut self, nodes: &[Node], scope: &Scope, rows: &[(String, String)]) {
        for node in nodes {
            match node {
                Node::Element {
                    positional,
                    attrs,
                    children,
                    ..
                } => {
                    for e in positional.iter().chain(attrs.iter().map(|a| &a.value)) {
                        self.expr(e, rows);
                    }
                    self.nodes(children, scope, rows);
                }
                Node::When {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    self.expr(cond, rows);
                    self.nodes(then, scope, rows);
                    self.nodes(otherwise, scope, rows);
                }
                Node::Each {
                    var,
                    index,
                    list,
                    body,
                    ..
                } => {
                    self.expr(list, rows);
                    let item = match infer(list, scope, self.shapes) {
                        Ok(Ty::List(t)) => *t,
                        _ => Ty::Unknown,
                    };
                    let records =
                        matches!(&item, Ty::Record(n) if self.declared.contains(n.as_str()));
                    let mut binds = vec![var.as_str()];
                    binds.extend(index.as_deref());
                    let mut inner = without(rows, &binds);
                    // A list named by the row's own variable is another list.
                    if let Some((l, root)) =
                        path(list).filter(|(_, r)| records && !binds.contains(r))
                    {
                        inner.push((l, root.to_string()));
                    }
                    let mut within = scope.clone();
                    within.push_each(var, index.as_deref(), item);
                    self.nodes(body, &within, &inner);
                }
                Node::Match {
                    subject,
                    some,
                    none,
                    ..
                } => {
                    self.expr(subject, rows);
                    let bound = match infer(subject, scope, self.shapes) {
                        Ok(Ty::Option(t)) => *t,
                        _ => Ty::Unknown,
                    };
                    let mut within = scope.clone();
                    within.push_region(Some((some.0.clone(), Ref::Bound(0), bound)));
                    self.nodes(&some.1, &within, &without(rows, &[some.0.as_str()]));
                    let mut none_scope = scope.clone();
                    none_scope.push_region(None);
                    self.nodes(none, &none_scope, rows);
                }
                Node::Use { args, children, .. } => {
                    for a in args {
                        self.expr(&a.value, rows);
                    }
                    self.nodes(children, scope, rows);
                }
                Node::Children { .. } => {}
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
pub(super) fn own_list(checked: &contract_types::Checked<'_>) -> Vec<LowerError> {
    let root = &checked.expanded.root;
    let Some(types) = checked.types.components.first() else {
        return Vec::new();
    };
    let mut own = Own {
        fns: &checked.file.fns,
        shapes: &checked.types.shapes,
        declared: checked
            .file
            .shapes
            .iter()
            .map(|s| s.name.as_str())
            .collect(),
        seen: BTreeSet::new(),
        out: Vec::new(),
    };
    let scope = checked.types.component_scope(root, types);
    own.nodes(&root.view, &scope, &[]);
    own.out
}
