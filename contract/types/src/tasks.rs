//! A task's schedule and, for a gated task (LLP 1092 D7, D9), its gate and
//! key: `when` takes a bool, and a key is a string, number or bool, as a
//! `for` key is.

use super::{infer, Scope, Shapes, Sink, Ty, TypeError};
use contract_syntax::{Component, Expr};

pub(crate) mod sequences;

/// A resource over a list stays at the root, where its request count is explicit.
pub(crate) fn child_owns(root: &str, child: &Component) -> String {
    let name = &child.name;
    let r = child.resources.iter().find(|r| r.each.is_some()).unwrap();
    let example = format!("{}{}", lower_first(name), upper_first(&r.name));
    format!(
        "only the root, the root file's first component (`{root}`), may declare a resource over a list: `resource {} = {}(…) for …` asks once for each element, a count of requests its list sets. Declare it in `{root}` (`resource {example} = {}(…) for …`) and pass `{name}` its answers as a prop; or, where each element is a row, give the row a component of its own that renders one answer",
        r.name, r.source, r.source
    )
}

/// A child's state initializer that reads the child's own resource, itself
/// or through a derive (LLP 1035.005.004 D2): an initializer runs once, when
/// the instance is made, before its requests are asked.
pub(crate) fn reads_own_request(child: &Component) -> Option<TypeError> {
    let requests: Vec<&str> = child
        .resources
        .iter()
        .filter(|r| !contract_syntax::inline::reads_device(r))
        .map(|r| r.name.as_str())
        .collect();
    if requests.is_empty() {
        return None;
    }
    // Every name that reads a request: the requests, and the derives that
    // read one, to a fixed point.
    let mut reading: Vec<&str> = requests.clone();
    loop {
        let more: Vec<&str> = child
            .derives
            .iter()
            .filter(|d| !reading.contains(&d.name.as_str()))
            .filter(|d| reading.iter().any(|n| reads_free(&d.expr, n)))
            .map(|d| d.name.as_str())
            .collect();
        if more.is_empty() {
            break;
        }
        reading.extend(more);
    }
    child.states.iter().find_map(|st| {
        let name = reading.iter().find(|n| reads_free(&st.expr, n))?;
        let request = requests
            .iter()
            .find(|r| *r == name)
            .map_or_else(|| format!("`{name}`, which reads a resource"), |r| format!("its resource `{r}`"));
        Some(TypeError {
            id: "type-state-reads-request",
            message: format!(
                "`state {}` in `{}` reads {request}: a state's initializer runs once, when the instance is made, before its resources are asked, so it would see only the placeholder. Read it in a `derive` or in the view, or set the state from an action that runs once the answer is there",
                st.name, child.name
            ),
            span: st.span,
        })
    })
}

/// The refusal of a use of a component that declares a request under a
/// repetition its data sizes (LLP 1035.005.004 D1): each row would ask on its
/// own, with nothing at the use saying so. The root's resource over a list
/// asks for every element in one line its author wrote.
pub(crate) fn repeated_owner(
    file: &contract_syntax::File,
    u: &contract_syntax::OwnedUse,
    repeat: contract_syntax::Repeat,
) -> TypeError {
    let root = file.components.first().map_or("", |c| c.name.as_str());
    let name = &u.component;
    let first = file
        .components
        .iter()
        .find(|c| &c.name == name)
        .and_then(|c| {
            c.resources
                .iter()
                .find(|r| !contract_syntax::inline::reads_device(r))
        });
    let around = match repeat {
        contract_syntax::Repeat::Each(at) => {
            format!("a `for` over data (line {})", at.line)
        }
        contract_syntax::Repeat::List(at) => format!(
            "a virtualized list (line {}), whose rows its window makes and retires as it scrolls",
            at.line
        ),
    };
    if let Some(t) = file
        .components
        .iter()
        .find(|c| &c.name == name)
        .and_then(|c| c.tasks.first())
    {
        return TypeError {
            id: "type-child-resource",
            message: format!("`{name}` declares `task {}`, and this use is under {around}: each row would start its own task. Use the component in a tab, screen or `if` arm; for a data-sized list, keep the task in `{root}` and pass its values and actions to the rows", t.name),
            span: u.span,
        };
    }
    let Some((r, source)) = first.map(|r| (r.name.as_str(), r.source.as_str())) else {
        // It owns only mutations (D6): each row would send and run its
        // `then` on its own.
        let m = (file.components.iter().find(|c| &c.name == name))
            .and_then(|c| c.mutations.first())
            .map_or("sent", |m| m.name.as_str());
        let example = format!("{}{}", lower_first(name), upper_first(m));
        return TypeError {
            id: "type-child-resource",
            message: format!(
                "`{name}` declares `mutation {m}`, and this use is under {around}: each row would send `{m}` and run its `then` on its own. A component that owns requests is used where its instances are tabs, screens or `if` arms. Declare the mutation in `{root}` (`mutation {example} as shape …`) and pass each row an action that sends it (docs/contract-for-agents.md, \"A feature's requests\")"
            ),
            span: u.span,
        };
    };
    let example = format!("{}{}", lower_first(name), upper_first(r));
    TypeError {
        id: "type-child-resource",
        message: format!(
            "`{name}` declares `resource {r}`, and this use is under {around}: each row would ask `{source}` on its own, with nothing here saying how many. A component that owns requests is used where its instances are tabs, screens or `if` arms. For one request per row, declare it in `{root}` over the list, `resource {example} = {source}(x) for x in … key=…`, and pass each row its answer as a prop (docs/contract-for-agents.md, \"A feature's requests\")"
        ),
        span: u.span,
    }
}

/// A package's component that owns a request to a source its own package's
/// module does not declare (LLP 1035.005.004 D7): the app grants what a
/// package's module declares, so a package asks and sends only through its
/// own.
pub(crate) fn foreign_source(file: &contract_syntax::File, child: &Component) -> Option<TypeError> {
    let package = child.params.package.as_deref()?;
    let declared = |source: &str| {
        file.modules.iter().any(|m| {
            m.kind == contract_syntax::ModuleKind::Source
                && m.package == package
                && (m.name == source || m.declared == source)
        })
    };
    let name = &child.name;
    let foreign = |what: String, source: &str, span| {
        TypeError {
        id: "type-package-source",
        message: format!(
            "`{name}` comes from the package `{package}`, and its {what} asks `{source}`, which `{package}`'s module does not declare: a package's component owns requests only to its own package's sources, which the app grants. Declare `{source}` in `{package}`'s `exact.json`, or make the request in the app and pass `{name}` its answer as a prop"
        ),
        span,
    }
    };
    if let Some(r) = child
        .resources
        .iter()
        .find(|r| !contract_syntax::inline::reads_device(r) && !declared(&r.source))
    {
        return Some(foreign(format!("`resource {}`", r.name), &r.source, r.span));
    }
    child.actions.iter().find_map(|a| {
        let (target, source, span) = first_send(&a.body, &|source| !declared(source))?;
        Some(foreign(format!("`send {target}`"), source, span))
    })
}

/// The first `send` in `stmts`, through `if` and `match`, whose source
/// `pick` takes: its mutation, source and span.
fn first_send<'a>(
    stmts: &'a [contract_syntax::Stmt],
    pick: &dyn Fn(&str) -> bool,
) -> Option<(&'a str, &'a str, contract_syntax::Span)> {
    use contract_syntax::Stmt;
    stmts.iter().find_map(|st| match st {
        Stmt::Send {
            target,
            source,
            span,
            ..
        } if pick(source) => Some((target.as_str(), source.as_str(), *span)),
        Stmt::If {
            then, otherwise, ..
        } => first_send(then, pick).or_else(|| first_send(otherwise, pick)),
        Stmt::Match { some, none, .. } => {
            first_send(&some.1, pick).or_else(|| first_send(none, pick))
        }
        Stmt::Call { body, .. } => first_send(body, pick),
        _ => None,
    })
}

/// Whether `e` reads `name` free.
fn reads_free(e: &Expr, name: &str) -> bool {
    matches!(e, Expr::Ident(n, _) if n == name)
        || e.scoped_operands()
            .any(|(child, binds)| !binds.contains(name) && reads_free(child, name))
}

/// `s` with its first letter lowercased: a feature's prefix.
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or(String::new(), |f| f.to_lowercase().chain(c).collect())
}

/// `s` with its first letter uppercased: a name after a prefix.
fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// Check every task of `c` in the component's scope.
pub(crate) fn check_task(
    t: &contract_syntax::Task,
    scope: &Scope,
    shapes: &Shapes,
    sink: &mut Sink,
) {
    match infer(&t.timer.0, scope, shapes) {
        Ok(Ty::Number) => {}
        Ok(_) => sink.push(TypeError {
            id: "type-timer",
            message: "a task needs a number of milliseconds".into(),
            span: t.timer.2,
        }),
        Err(e) => sink.push(e),
    }
    check_gate(t.gate.as_ref(), t.key.as_ref(), "task", scope, shapes, sink);
}

/// A gate takes a bool, and a key is a string, number or bool, as a `for`
/// key is: a task's, and a sequence's (LLP 1125 D2).
pub(crate) fn check_gate(
    gate: Option<&Expr>,
    key: Option<&Expr>,
    what: &str,
    scope: &Scope,
    shapes: &Shapes,
    sink: &mut Sink,
) {
    if let Some(gate) = gate {
        match infer(gate, scope, shapes) {
            Ok(Ty::Bool | Ty::Unknown) => {}
            Ok(ty) => sink.push(TypeError {
                id: "type-task-gate",
                message: gate_message(gate, &ty),
                span: gate.span(),
            }),
            Err(e) => sink.push(e),
        }
    }
    if let Some(key) = key {
        match infer(key, scope, shapes) {
            Ok(Ty::String | Ty::Enum(_) | Ty::Number | Ty::Bool | Ty::Unknown) => {}
            Ok(ty) => sink.push(TypeError {
                id: "type-task-key",
                message: format!(
                    "a {what}'s `key=` is a string, number or bool, as a `for` key is; this one is {ty}"
                ),
                span: key.span(),
            }),
            Err(e) => sink.push(e),
        }
    }
}

/// "`when` takes a bool; `toast` is a string; write `toast != ""`".
fn gate_message(gate: &Expr, ty: &Ty) -> String {
    let (what, fix) = match gate {
        Expr::Ident(name, _) => (
            format!("`{name}` is a {ty}"),
            match ty {
                Ty::String | Ty::Enum(_) => format!("; write `{name} != \"\"`"),
                Ty::Number => format!("; write `{name} != 0`"),
                Ty::List(_) => format!("; write `{name}.length > 0`"),
                _ => String::new(),
            },
        ),
        _ => (format!("this condition is a {ty}"), String::new()),
    };
    format!("`when` takes a bool; {what}{fix}")
}
