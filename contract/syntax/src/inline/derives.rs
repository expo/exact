//! A child's derives, resolved through one another before the use's props
//! are substituted (so a parent `a` passed in is never mistaken for the
//! child's derive `a`). Type inference admits either declaration order, so
//! resolution does too.
//!
//! A resolved derive is its body with its dependencies in scope, evaluated
//! exactly when the body evaluated them before: a dependency the body reads
//! once is written where it is read; one it reads more than once is bound
//! once (`Expr::Let`) at the smallest part of the body that evaluates it on
//! every path — inside the branch that needs it, never ahead of a condition
//! or a short-circuit that would have skipped it. Reads are counted through
//! other derives, so a chain of derives each reading the one before twice
//! resolves in linear size, not exponential.

use super::{err, renamed_locals};
use crate::ast::{BinOp, Binding, Component, Expr, Node, Stmt, TemplatePart};
use crate::parser::SyntaxError;
use std::collections::{BTreeMap, BTreeSet};

/// Bounds expanded child-derive expressions to stay within parser-depth and
/// linear-size budgets across downstream recursive passes.
const MAX_DERIVE_DEPTH: usize = 100;
const MAX_DERIVE_NODES: usize = 4_096;

/// The derives reachable from `c`'s view, states, provides, actions,
/// resources, or tasks, each with its resolved expression. Dependencies also
/// remain available to lowering as shared bodies when nothing outside another
/// derive reads them.
pub fn resolved_derives(c: &Component) -> Result<Vec<(&Binding, Expr)>, SyntaxError> {
    let mut indices = BTreeMap::new();
    for (i, derive) in c.derives.iter().enumerate() {
        if indices.insert(derive.name.as_str(), i).is_some() {
            return err(
                "type-duplicate-name",
                format!("`{}` declared twice", derive.name),
                derive.span,
            );
        }
    }
    if c.derives.is_empty() {
        return Ok(Vec::new());
    }
    // Binders renamed apart first, so a `let` placed inside a `match` arm
    // can never capture a name its value reads.
    let mut fresh = 0;
    let bodies: Vec<Expr> = c
        .derives
        .iter()
        .map(|d| freshen(&d.expr, &mut fresh))
        .collect();
    let reads: Vec<BTreeSet<usize>> = bodies
        .iter()
        .map(|body| {
            let mut out = BTreeSet::new();
            dependencies(body, &indices, &mut out);
            out
        })
        .collect();
    let mut order = Vec::with_capacity(c.derives.len());
    let mut state = vec![0u8; c.derives.len()];
    for i in 0..c.derives.len() {
        visit(i, c, &reads, &mut state, &mut order)?;
    }
    let mut rank = vec![0; order.len()];
    for (at, &i) in order.iter().enumerate() {
        rank[i] = at;
    }
    // Each derive's evaluation in summary, dependencies first.
    let n = c.derives.len();
    let mut summaries = vec![Summary::new(n); n];
    for &i in &order {
        summaries[i] = summarize(&bodies[i], &indices, &summaries);
    }
    let cx = Cx {
        component: &c.name,
        indices: &indices,
        names: c.derives.iter().map(|d| d.name.as_str()).collect(),
        bodies: &bodies,
        summaries: &summaries,
        rank: &rank,
        resolve: true,
    };
    let read = read_outside_derives(c);
    let mut reachable: Vec<bool> = c
        .derives
        .iter()
        .map(|d| read.contains(d.name.as_str()))
        .collect();
    for &i in order.iter().rev() {
        if reachable[i] {
            for &dependency in &reads[i] {
                reachable[dependency] = true;
            }
        }
    }
    let mut out = Vec::new();
    for (i, derive) in c.derives.iter().enumerate() {
        if !reachable[i] {
            continue;
        }
        let mut nodes = 0;
        let placed = cx.place(&bodies[i], &mut Vec::new(), true, 0, &mut nodes)?;
        out.push((derive, placed));
    }
    Ok(out)
}

/// Bind repeated prop reads within one evaluation, preserving its lazy parts.
/// Substitution supplies each let's value afterwards, in the caller's scope.
pub(super) fn share_props(e: &Expr, names: &BTreeSet<String>) -> Expr {
    if names.is_empty() {
        return e.clone();
    }
    let indices: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.as_str(), i))
        .collect();
    let n = names.len();
    let summaries = vec![Summary::new(n); n];
    let e = freshen(e, &mut 0);
    let bodies: Vec<_> = names
        .iter()
        .map(|name| Expr::Ident(name.clone(), e.span()))
        .collect();
    let rank: Vec<_> = (0..n).collect();
    let cx = Cx {
        component: "",
        indices: &indices,
        names: names.iter().map(String::as_str).collect(),
        bodies: &bodies,
        summaries: &summaries,
        rank: &rank,
        resolve: false,
    };
    cx.place(&e, &mut Vec::new(), true, 0, &mut 0)
        .unwrap_or_else(|_| e.clone())
}

/// Depth-first dependency order (dependencies first); a cycle is refused
/// at the derive where it closes. Iterative so long chains fit a small stack.
fn visit(
    start: usize,
    c: &Component,
    reads: &[BTreeSet<usize>],
    state: &mut [u8],
    order: &mut Vec<usize>,
) -> Result<(), SyntaxError> {
    if state[start] == 2 {
        return Ok(());
    }
    state[start] = 1;
    let mut stack = vec![(start, reads[start].iter())];
    while let Some((node, iter)) = stack.last_mut() {
        if let Some(&dep) = iter.next() {
            match state[dep] {
                2 => {}
                1 => {
                    return err(
                        "type-derive-cycle",
                        format!(
                            "cannot resolve `{}`: it depends on itself through other derives",
                            c.derives[dep].name
                        ),
                        c.derives[dep].span,
                    )
                }
                _ => {
                    state[dep] = 1;
                    stack.push((dep, reads[dep].iter()));
                }
            }
        } else {
            let node = *node;
            stack.pop();
            state[node] = 2;
            order.push(node);
        }
    }
    Ok(())
}

/// What evaluating an expression does to each derive: how often its value
/// is read, through other derives too (0, 1, or 2 for "more"), and whether
/// it is read on every path through the expression.
#[derive(Clone)]
struct Summary {
    reads: Vec<u8>,
    always: Vec<bool>,
}

impl Summary {
    fn new(n: usize) -> Self {
        Summary {
            reads: vec![0; n],
            always: vec![false; n],
        }
    }

    fn add(&mut self, other: &Summary) {
        self.add_sometimes(other);
        for (mine, theirs) in self.always.iter_mut().zip(&other.always) {
            *mine |= theirs;
        }
    }

    /// Reads from a part evaluated on only some paths: counted, not always.
    fn add_sometimes(&mut self, other: &Summary) {
        for (mine, theirs) in self.reads.iter_mut().zip(&other.reads) {
            *mine = (*mine + theirs).min(2);
        }
    }
}

/// Summarize `e`, whose derive reads are summarized in `summaries`.
fn summarize(e: &Expr, indices: &BTreeMap<&str, usize>, summaries: &[Summary]) -> Summary {
    let mut out = Summary::new(summaries.len());
    summarize_into(e, indices, summaries, &mut out);
    out
}

fn summarize_into(
    e: &Expr,
    indices: &BTreeMap<&str, usize>,
    summaries: &[Summary],
    out: &mut Summary,
) {
    let part = |x: &Expr| summarize(x, indices, summaries);
    match e {
        Expr::Ident(name, _) => {
            if let Some(&d) = indices.get(name.as_str()) {
                out.add(&summaries[d]);
                out.reads[d] = (out.reads[d] + 1).min(2);
                out.always[d] = true;
            }
        }
        Expr::Binary(BinOp::And | BinOp::Or, a, b, _) => {
            summarize_into(a, indices, summaries, out);
            out.add_sometimes(&part(b));
        }
        Expr::Ternary(c, a, b, _) => {
            summarize_into(c, indices, summaries, out);
            branches(out, &part(a), &part(b));
        }
        Expr::Match {
            subject,
            some,
            none,
            ..
        } => {
            summarize_into(subject, indices, summaries, out);
            branches(out, &part(some), &part(none));
        }
        // A callback runs once per item, perhaps never (LLP 1017.003).
        Expr::Arrow { body, .. } => out.add_sometimes(&part(body)),
        _ => e
            .operands()
            .for_each(|child| summarize_into(child, indices, summaries, out)),
    }
}

/// Two exclusive branches: every read counts, and what both always read is
/// always read.
fn branches(out: &mut Summary, a: &Summary, b: &Summary) {
    out.add_sometimes(a);
    out.add_sometimes(b);
    for (i, always) in out.always.iter_mut().enumerate() {
        *always |= a.always[i] && b.always[i];
    }
}

struct Cx<'a> {
    component: &'a str,
    indices: &'a BTreeMap<&'a str, usize>,
    names: Vec<&'a str>,
    bodies: &'a [Expr],
    summaries: &'a [Summary],
    rank: &'a [usize],
    resolve: bool,
}

impl<'a> Cx<'a> {
    fn too_deep(&self, span: crate::Span) -> SyntaxError {
        SyntaxError {
            id: "syntax-derive-depth",
            message: format!(
                "derives in `{}` expand more than {MAX_DERIVE_DEPTH} levels deep: shorten the chain or lift shared derives to the parent",
                self.component
            ),
            span,
        }
    }

    fn too_large(&self, span: crate::Span) -> SyntaxError {
        SyntaxError {
            id: "syntax-derive-size",
            message: format!(
                "derives in `{}` expand past {MAX_DERIVE_NODES} expressions: reduce repeated conditional reads or lift shared derives to the parent",
                self.component
            ),
            span,
        }
    }

    /// `e` with every derive it reads resolved; `bound` are the derives an
    /// enclosing `let` binds. `whole` is true where no enclosing part of the
    /// same body always evaluates `e` (the body itself, a branch, the right
    /// of `and`/`or`): only there can a derive first qualify to be bound.
    fn place(
        &self,
        mut e: &'a Expr,
        bound: &mut Vec<usize>,
        mut whole: bool,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<Expr, SyntaxError> {
        // Chase direct derive aliases iteratively so alias chains take no
        // native stack frames.
        if self.resolve {
            while let Expr::Ident(name, _) = e {
                let Some(&d) = self.indices.get(name.as_str()) else {
                    break;
                };
                if bound.contains(&d) {
                    *nodes += 1;
                    if *nodes > MAX_DERIVE_NODES {
                        return Err(self.too_large(e.span()));
                    }
                    return Ok(e.clone());
                }
                e = &self.bodies[d];
                whole = true;
            }
        }
        if depth > MAX_DERIVE_DEPTH {
            return Err(self.too_deep(e.span()));
        }
        *nodes += 1;
        if *nodes > MAX_DERIVE_NODES {
            return Err(self.too_large(e.span()));
        }
        let mut lets = Vec::new();
        if whole {
            let s = summarize(e, self.indices, self.summaries);
            let mut shared: Vec<usize> = (0..s.reads.len())
                .filter(|&d| s.reads[d] >= 2 && s.always[d] && !bound.contains(&d))
                .collect();
            shared.sort_by_key(|&d| self.rank[d]);
            if depth + shared.len() > MAX_DERIVE_DEPTH {
                return Err(self.too_deep(e.span()));
            }
            *nodes += shared.len();
            if *nodes > MAX_DERIVE_NODES {
                return Err(self.too_large(e.span()));
            }
            for (k, d) in shared.into_iter().enumerate() {
                let value = if self.resolve {
                    self.place(&self.bodies[d], bound, true, depth + k + 1, nodes)?
                } else {
                    self.bodies[d].clone()
                };
                lets.push((d, value));
                bound.push(d);
            }
        }
        let next_depth = depth + lets.len() + 1;
        let body = match e {
            Expr::Ident(..) => Ok(e.clone()),
            Expr::Binary(op @ (BinOp::And | BinOp::Or), a, b, span) => Ok(Expr::Binary(
                *op,
                Box::new(self.place(a, bound, false, next_depth, nodes)?),
                Box::new(self.place(b, bound, true, next_depth, nodes)?),
                *span,
            )),
            Expr::Ternary(c, a, b, span) => Ok(Expr::Ternary(
                Box::new(self.place(c, bound, false, next_depth, nodes)?),
                Box::new(self.place(a, bound, true, next_depth, nodes)?),
                Box::new(self.place(b, bound, true, next_depth, nodes)?),
                *span,
            )),
            Expr::Match {
                subject,
                var,
                some,
                none,
                span,
            } => Ok(Expr::Match {
                subject: Box::new(self.place(subject, bound, false, next_depth, nodes)?),
                var: var.clone(),
                some: Box::new(self.place(some, bound, true, next_depth, nodes)?),
                none: Box::new(self.place(none, bound, true, next_depth, nodes)?),
                span: *span,
            }),
            // Evaluated once per item, perhaps never: a part of its own.
            Expr::Arrow { params, body, span } => Ok(Expr::Arrow {
                params: params.clone(),
                body: Box::new(self.place(body, bound, true, next_depth, nodes)?),
                span: *span,
            }),
            _ => try_map_children(e, &mut |child| {
                self.place(child, bound, false, next_depth, nodes)
            }),
        };
        bound.truncate(bound.len() - lets.len());
        let body = body?;
        let span = e.span();
        Ok(lets
            .into_iter()
            .rev()
            .fold(body, |body, (d, value)| Expr::Let {
                name: self.names[d].to_owned(),
                value: Box::new(value),
                body: Box::new(body),
                span,
            }))
    }
}

/// `e` with every `match` binder and arrow parameter renamed to a spelling
/// no author can write (`x@b1`), each distinct. Call heads keep their names.
fn freshen(e: &Expr, fresh: &mut u32) -> Expr {
    match e {
        Expr::Match {
            subject,
            var,
            some,
            none,
            span,
        } => {
            *fresh += 1;
            let name = format!("{var}@b{fresh}");
            let some = freshen(some, fresh);
            let renamed = BTreeMap::from([(var.clone(), name.clone())]);
            let some = renamed_locals(&some, &renamed);
            Expr::Match {
                subject: Box::new(freshen(subject, fresh)),
                var: name,
                some: Box::new(some),
                none: Box::new(freshen(none, fresh)),
                span: *span,
            }
        }
        Expr::Arrow { params, body, span } => {
            let mut renamed = BTreeMap::new();
            let params = params
                .iter()
                .map(|p| {
                    *fresh += 1;
                    let name = format!("{p}@b{fresh}");
                    renamed.insert(p.clone(), name.clone());
                    name
                })
                .collect();
            let body = freshen(body, fresh);
            Expr::Arrow {
                params,
                body: Box::new(renamed_locals(&body, &renamed)),
                span: *span,
            }
        }
        _ => map_children(e, &mut |child| freshen(child, fresh)),
    }
}

/// The derives `expr` reads as values (binders are fresh, so none shadows).
/// A call head names a function or action, never a derive's value.
fn dependencies(expr: &Expr, indices: &BTreeMap<&str, usize>, out: &mut BTreeSet<usize>) {
    if let Expr::Ident(name, _) = expr {
        if let Some(&i) = indices.get(name.as_str()) {
            out.insert(i);
        }
    }
    expr.operands()
        .for_each(|child| dependencies(child, indices, out));
}

/// Every name written outside `c`'s derives: the derives any of its view,
/// states, provides, actions, resources, or tasks may read.
fn read_outside_derives(c: &Component) -> BTreeSet<&str> {
    fn expr<'a>(e: &'a Expr, out: &mut BTreeSet<&'a str>) {
        if let Expr::Ident(name, _) | Expr::Call(name, _, _) = e {
            out.insert(name);
        }
        e.operands().for_each(|child| expr(child, out));
    }
    fn stmts<'a>(body: &'a [Stmt], out: &mut BTreeSet<&'a str>) {
        for st in body {
            match st {
                Stmt::Assign { expr: e, .. } | Stmt::Let { expr: e, .. } => expr(e, out),
                Stmt::Command { args, .. } | Stmt::Send { args, .. } => {
                    args.iter().for_each(|a| expr(a, out))
                }
                Stmt::Call { args, body, .. } => {
                    args.iter().for_each(|a| expr(a, out));
                    stmts(body, out);
                }
                Stmt::Refresh { .. } => {}
                Stmt::If {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    expr(cond, out);
                    stmts(then, out);
                    stmts(otherwise, out);
                }
                Stmt::Match {
                    subject,
                    some,
                    none,
                    ..
                } => {
                    expr(subject, out);
                    stmts(&some.1, out);
                    stmts(none, out);
                }
            }
        }
    }
    fn nodes<'a>(view: &'a [Node], out: &mut BTreeSet<&'a str>) {
        for e in Node::walk(view).flat_map(Node::exprs) {
            expr(e, out);
        }
    }
    let mut out = BTreeSet::new();
    nodes(&c.view, &mut out);
    for b in c.states.iter().chain(&c.provides) {
        expr(&b.expr, &mut out);
    }
    for a in &c.actions {
        stmts(&a.body, &mut out);
    }
    // A child's own requests and tasks read its derives (LLP 1035.005.004 D1).
    for r in &c.resources {
        r.args.iter().chain(&r.gate).for_each(|e| expr(e, &mut out));
        if let Some(each) = &r.each {
            expr(&each.list, &mut out);
            if let Some(key) = &each.key {
                expr(key, &mut out);
            }
        }
    }
    for t in &c.tasks {
        t.gate.iter().chain(&t.key).for_each(|e| expr(e, &mut out));
    }
    out
}

fn map_children(e: &Expr, f: &mut dyn FnMut(&Expr) -> Expr) -> Expr {
    try_map_children(e, &mut |child| Ok(f(child))).unwrap()
}

fn try_map_children(
    e: &Expr,
    f: &mut dyn FnMut(&Expr) -> Result<Expr, SyntaxError>,
) -> Result<Expr, SyntaxError> {
    Ok(match e {
        Expr::Some(x, s) => Expr::Some(Box::new(f(x)?), *s),
        Expr::Unary(op, x, s) => Expr::Unary(*op, Box::new(f(x)?), *s),
        Expr::Member(x, field, s) => Expr::Member(Box::new(f(x)?), field.clone(), *s),
        Expr::NamedArg(n, x, s) => Expr::NamedArg(n.clone(), Box::new(f(x)?), *s),
        Expr::Typed(x, t, s) => Expr::Typed(Box::new(f(x)?), t.clone(), *s),
        Expr::Binary(op, x, y, s) => {
            let x = Box::new(f(x)?);
            Expr::Binary(*op, x, Box::new(f(y)?), *s)
        }
        Expr::Ternary(x, y, z, s) => {
            let (x, y) = (Box::new(f(x)?), Box::new(f(y)?));
            Expr::Ternary(x, y, Box::new(f(z)?), *s)
        }
        Expr::Match {
            subject,
            var,
            some,
            none,
            span,
        } => {
            let (subject, some) = (Box::new(f(subject)?), Box::new(f(some)?));
            Expr::Match {
                subject,
                var: var.clone(),
                some,
                none: Box::new(f(none)?),
                span: *span,
            }
        }
        Expr::Let {
            name,
            value,
            body,
            span,
        } => {
            let value = Box::new(f(value)?);
            Expr::Let {
                name: name.clone(),
                value,
                body: Box::new(f(body)?),
                span: *span,
            }
        }
        Expr::Arrow { params, body, span } => Expr::Arrow {
            params: params.clone(),
            body: Box::new(f(body)?),
            span: *span,
        },
        Expr::Call(n, args, s) => {
            let args: Result<Vec<_>, SyntaxError> = args.iter().map(&mut *f).collect();
            Expr::Call(n.clone(), args?, *s)
        }
        Expr::List(items, s) => {
            let items: Result<Vec<_>, SyntaxError> = items.iter().map(&mut *f).collect();
            Expr::List(items?, *s)
        }
        Expr::Template(parts, s) => {
            let parts: Result<Vec<_>, SyntaxError> = parts
                .iter()
                .map(|p| match p {
                    TemplatePart::Expr(x) => Ok(TemplatePart::Expr(f(x)?)),
                    TemplatePart::Text(t) => Ok(TemplatePart::Text(t.clone())),
                })
                .collect();
            Expr::Template(parts?, *s)
        }
        leaf => leaf.clone(),
    })
}
