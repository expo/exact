//! @ref LLP 1092 D9 — a gated task's gate and key are read at commits, not
//! as the clock moves: one that reads `performanceNow()`, directly, through a derive
//! or through a `fn` body, is refused (`analyze-task-gate-clock`).

use super::AnalyzeError;
use contract_syntax::{Component, Expr, FnDecl, Span, Stmt, TaskKind, TemplatePart};
use std::collections::BTreeSet;

/// Every gate or key of `c`'s tasks that reads the clock.
pub(super) fn check(c: &Component, fns: &[FnDecl]) -> Vec<AnalyzeError> {
    let mut errors = change_cycles(c);
    for t in &c.tasks {
        for (what, e) in [("gate", &t.gate), ("key", &t.key)] {
            let Some(e) = e else { continue };
            let mut seen = BTreeSet::new();
            if let Some(at) = clock(e, c, fns, &mut seen, &BTreeSet::new()) {
                errors.push(AnalyzeError {
                    id: "analyze-task-gate-clock",
                    message: format!(
                        "`task {}`'s {what} reads `performanceNow()`{}. A gate is read at commits, not as the clock moves, so an `after` gated on `performanceNow()` cannot be dropped before it fires, and an `every` stops up to an interval late. Gate on state (`toast != \"\"`) and let `after(5000, …)` measure the time",
                        t.name,
                        match at {
                            Through::Direct => String::new(),
                            Through::Derive(d) => format!(" through `{d}`"),
                            Through::Fn(f) => format!(" through `fn {f}`"),
                        }
                    ),
                    span: e.span(),
                    related: Vec::new(),
                });
            }
        }
    }
    // @ref LLP 1125 D2 — a sequence's gate and key as a task's; its `play`
    // arguments are re-read after every commit, not as the clock moves, so
    // one that reads the clock would change at commits only.
    for q in &c.sequences {
        let options = q
            .options
            .iter()
            .map(|(name, e, _)| (name.as_str(), Some(e)));
        let gates = [("gate", q.gate.as_ref()), ("key", q.key.as_ref())];
        let play = [("tracks", Some(&q.tracks))].into_iter().chain(options);
        for (what, e) in gates.into_iter().chain(play) {
            let Some(e) = e else { continue };
            let mut seen = BTreeSet::new();
            let Some(at) = clock(e, c, fns, &mut seen, &BTreeSet::new()) else {
                continue;
            };
            let gate = matches!(what, "gate" | "key");
            let through = match at {
                Through::Direct => String::new(),
                Through::Derive(d) => format!(" through `{d}`"),
                Through::Fn(f) => format!(" through `fn {f}`"),
            };
            errors.push(AnalyzeError {
                id: if gate { "analyze-task-gate-clock" } else { "analyze-sequence-clock" },
                message: if gate {
                    format!("`sequence {}`'s {what} reads `performanceNow()`{through}. A gate is read at commits, not as the clock moves; gate on state (`playing`)", q.name)
                } else {
                    format!("`sequence {}`'s `{what}` reads `performanceNow()`{through}. `play`'s arguments are re-read at commits, not as the clock moves; the runner times each step from the beat, so say where in the pattern a step falls (`beat=`), not when", q.name)
                },
                span: e.span(),
                related: Vec::new(),
            });
        }
    }
    errors
}

enum Through {
    Direct,
    Derive(String),
    Fn(String),
}

/// Where `e` reads the clock, if it does: `performanceNow()` itself, or the first
/// derive or `fn` whose value does.
fn clock<'a>(
    e: &'a Expr,
    c: &'a Component,
    fns: &'a [FnDecl],
    seen: &mut BTreeSet<(&'static str, &'a str)>,
    bound: &BTreeSet<&'a str>,
) -> Option<Through> {
    match e {
        Expr::Call(name, args, _)
            if name == "performanceNow"
                && args.is_empty()
                && !fns.iter().any(|f| f.name == "performanceNow") =>
        {
            Some(Through::Direct)
        }
        Expr::Call(name, args, _) => {
            if let Some(f) = fns.iter().find(|f| &f.name == name) {
                if seen.insert(("fn", &f.name)) {
                    let locals = f.params.iter().map(|p| p.name.as_str()).collect();
                    if clock(&f.body, c, fns, seen, &locals).is_some() {
                        return Some(Through::Fn(shown(name)));
                    }
                }
            }
            args.iter().find_map(|a| clock(a, c, fns, seen, bound))
        }
        Expr::Ident(name, _) if !bound.contains(name.as_str()) => {
            if seen.insert(("derive", name)) {
                if let Some(d) = c.derives.iter().find(|d| d.name == *name) {
                    // A derive's inputs belong to its declaration, not the caller's locals.
                    return clock(&d.expr, c, fns, seen, &BTreeSet::new())
                        .map(|_| Through::Derive(shown(name)));
                }
            }
            None
        }
        Expr::Template(parts, _) => parts.iter().find_map(|p| match p {
            TemplatePart::Expr(e) => clock(e, c, fns, seen, bound),
            TemplatePart::Text(_) => None,
        }),
        Expr::Some(a, _)
        | Expr::Member(a, _, _)
        | Expr::NamedArg(_, a, _)
        | Expr::Unary(_, a, _)
        | Expr::Typed(a, _, _) => clock(a, c, fns, seen, bound),
        Expr::Arrow { params, body, .. } => {
            let mut locals = bound.clone();
            locals.extend(params.iter().map(String::as_str));
            clock(body, c, fns, seen, &locals)
        }
        Expr::Binary(_, a, b, _) => {
            clock(a, c, fns, seen, bound).or_else(|| clock(b, c, fns, seen, bound))
        }
        Expr::Let {
            name, value, body, ..
        } => clock(value, c, fns, seen, bound).or_else(|| {
            let mut locals = bound.clone();
            locals.insert(name);
            clock(body, c, fns, seen, &locals)
        }),
        Expr::Ternary(a, b, d, _) => clock(a, c, fns, seen, bound)
            .or_else(|| clock(b, c, fns, seen, bound))
            .or_else(|| clock(d, c, fns, seen, bound)),
        Expr::Match {
            subject,
            var,
            some,
            none,
            ..
        } => clock(subject, c, fns, seen, bound)
            .or_else(|| {
                let mut locals = bound.clone();
                locals.insert(var);
                clock(some, c, fns, seen, &locals)
            })
            .or_else(|| clock(none, c, fns, seen, bound)),
        Expr::List(items, _) => items.iter().find_map(|e| clock(e, c, fns, seen, bound)),
        Expr::Ident(..) | Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) => None,
    }
}

/// A name as written: a lifted child's `name#N` without its suffix.
fn shown(name: &str) -> String {
    name.split('#').next().unwrap_or(name).to_string()
}

/// A change action cannot feed its own trigger, even through another change
/// task, derive, resource argument, called action, or mutation's `then`.
fn change_cycles(c: &Component) -> Vec<AnalyzeError> {
    let tasks: Vec<_> = c
        .tasks
        .iter()
        .filter(|t| t.kind == TaskKind::Change)
        .collect();
    if tasks.is_empty() {
        return Vec::new();
    }
    let names: BTreeSet<&str> = c
        .states
        .iter()
        .map(|s| s.name.as_str())
        .chain(c.derives.iter().map(|d| d.name.as_str()))
        .chain(c.resources.iter().map(|r| r.name.as_str()))
        .chain(c.mutations.iter().map(|m| m.name.as_str()))
        .collect();
    fn inputs<'a>(
        e: &'a Expr,
        names: &BTreeSet<&str>,
        out: &mut BTreeSet<&'a str>,
        bound: &[&'a str],
    ) {
        let mut found = Vec::new();
        trigger_reads(e, names, &mut bound.to_vec(), &mut found);
        out.extend(found.into_iter().map(|(n, _, _)| n));
    }
    let reads: Vec<_> = tasks
        .iter()
        .map(|t| {
            let mut read = BTreeSet::new();
            for e in t.gate.iter().chain(t.key.iter()) {
                inputs(e, &names, &mut read, &[]);
            }
            loop {
                let before = read.len();
                for d in &c.derives {
                    if read.contains(d.name.as_str()) {
                        inputs(&d.expr, &names, &mut read, &[]);
                    }
                }
                for r in &c.resources {
                    if !read.contains(r.name.as_str()) {
                        continue;
                    }
                    let bound: Vec<_> = r.each.iter().map(|e| e.item.as_str()).collect();
                    for e in &r.args {
                        inputs(e, &names, &mut read, &bound);
                    }
                    if let Some(e) = &r.gate {
                        inputs(e, &names, &mut read, &[]);
                    }
                    if let Some(each) = &r.each {
                        inputs(&each.list, &names, &mut read, &[]);
                        if let Some(e) = &each.key {
                            inputs(e, &names, &mut read, &bound);
                        }
                    }
                }
                if read.len() == before {
                    break;
                }
            }
            read
        })
        .collect();
    fn effects<'a>(body: &'a [Stmt], out: &mut BTreeSet<&'a str>) {
        for s in body {
            match s {
                Stmt::Assign { target, .. }
                | Stmt::Send { target, .. }
                | Stmt::Refresh { target, .. } => {
                    out.insert(target);
                }
                Stmt::If {
                    then, otherwise, ..
                } => {
                    effects(then, out);
                    effects(otherwise, out);
                }
                Stmt::Match { some, none, .. } => {
                    effects(&some.1, out);
                    effects(none, out);
                }
                Stmt::Call { body, .. } => effects(body, out),
                _ => {}
            }
        }
    }
    let writes: Vec<_> = tasks
        .iter()
        .map(|t| {
            let mut out = BTreeSet::new();
            if let Some(a) = c.actions.iter().find(|a| a.name == t.timer.1) {
                effects(&a.body, &mut out);
            }
            let mut seen = BTreeSet::new();
            loop {
                let before = seen.len();
                for m in &c.mutations {
                    if !out.contains(m.name.as_str()) || !seen.insert(m.name.as_str()) {
                        continue;
                    }
                    out.extend(m.refreshes.iter().map(|(n, _)| n.as_str()));
                    if let Some((name, _)) = &m.then {
                        if let Some(a) = c.actions.iter().find(|a| &a.name == name) {
                            effects(&a.body, &mut out);
                        }
                    }
                }
                if seen.len() == before {
                    break;
                }
            }
            out
        })
        .collect();
    let edges: Vec<Vec<usize>> = writes
        .iter()
        .map(|written| {
            reads
                .iter()
                .enumerate()
                .filter_map(|(i, read)| (!written.is_disjoint(read)).then_some(i))
                .collect()
        })
        .collect();
    let mut errors = Vec::new();
    for (start, task) in tasks.iter().enumerate() {
        let mut pending = edges[start].clone();
        let mut visited = BTreeSet::new();
        while let Some(i) = pending.pop() {
            if i == start {
                errors.push(AnalyzeError { id: "analyze-change-cycle",
                    message: format!("`task {}` can change its own gate or key through its action or another change task. Use a derive for pure values; a change action must not write or refresh an input of its trigger", task.name),
                    span: task.span, related: Vec::new() });
                break;
            }
            if visited.insert(i) {
                pending.extend(&edges[i]);
            }
        }
    }
    errors
}

fn trigger_reads<'e>(
    e: &'e Expr,
    slots: &BTreeSet<&str>,
    bound: &mut Vec<&'e str>,
    out: &mut Vec<(&'e str, Span, Option<&'e str>)>,
) {
    match e {
        Expr::Ident(n, span) => {
            if slots.contains(n.as_str()) && !bound.contains(&n.as_str()) {
                out.push((n, *span, None));
            }
        }
        Expr::Member(x, field, _) if matches!(&**x, Expr::Ident(..)) => {
            let at = out.len();
            trigger_reads(x, slots, bound, out);
            if let Some(read) = out.get_mut(at) {
                read.2 = Some(field);
            }
        }
        Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) => {}
        Expr::Call(_, args, _) | Expr::List(args, _) => args
            .iter()
            .for_each(|a| trigger_reads(a, slots, bound, out)),
        Expr::Template(parts, _) => {
            for p in parts {
                if let TemplatePart::Expr(x) = p {
                    trigger_reads(x, slots, bound, out);
                }
            }
        }
        Expr::Some(x, _)
        | Expr::Member(x, _, _)
        | Expr::NamedArg(_, x, _)
        | Expr::Typed(x, _, _)
        | Expr::Unary(_, x, _) => trigger_reads(x, slots, bound, out),
        Expr::Binary(_, a, b, _) => {
            trigger_reads(a, slots, bound, out);
            trigger_reads(b, slots, bound, out);
        }
        Expr::Ternary(a, b, c, _) => {
            trigger_reads(a, slots, bound, out);
            trigger_reads(b, slots, bound, out);
            trigger_reads(c, slots, bound, out);
        }
        Expr::Match {
            subject,
            var,
            some,
            none,
            ..
        } => {
            trigger_reads(subject, slots, bound, out);
            trigger_reads(none, slots, bound, out);
            bound.push(var);
            trigger_reads(some, slots, bound, out);
            bound.pop();
        }
        Expr::Let {
            name, value, body, ..
        } => {
            trigger_reads(value, slots, bound, out);
            bound.push(name);
            trigger_reads(body, slots, bound, out);
            bound.pop();
        }
        Expr::Arrow { params, body, .. } => {
            bound.extend(params.iter().map(String::as_str));
            trigger_reads(body, slots, bound, out);
            bound.truncate(bound.len() - params.len());
        }
    }
}
