//! Potential transform owners visible together (LLP 1071, "The height and
//! transform drags"). Handles share an owner when they resolve to the same
//! strict ancestor; a repeated target can produce competing instances.
//! Unknown or ambiguous ancestor IDs stay unknown. Opposite arms outside
//! repeated templates cannot compete; rows may choose different arms only
//! when the condition depends on row values or state owned by a row. Separate
//! blocks over one readable condition or subject exclude each other as an
//! `if` and its `else` do: one render reads one value.

use super::{has, miswiring::names_may_match, width};
use crate::LowerError;
use contract_syntax::{Attr, BinOp, Expr, Node, Owner, Span, StyleDecl, TemplatePart, UnOp};
use contract_types::Checked;
use std::collections::BTreeSet;

struct Ancestor {
    target: usize,
    name: Option<Expr>,
    loops: Vec<u32>,
}

struct Binding {
    target: usize,
    rows: Vec<Option<usize>>,
    span: Span,
    repeated: bool,
    choices: Vec<(Choice, bool)>,
}

/// A condition's `then` arm and a match's `some` arm are arm `false`.
#[derive(Clone, Copy, PartialEq)]
enum Test {
    Cond,
    Subject,
}

/// What chose an arm: the region itself, or a value read without locations.
#[derive(Clone, PartialEq)]
enum Choice {
    Region(u32),
    Read(Test, Expr),
}

struct Local<'a> {
    name: &'a str,
    symbol: String,
    dependencies: BTreeSet<u32>,
}

struct Repetition {
    tag: u32,
    items: Option<Vec<Expr>>,
    eligible: Option<Vec<usize>>,
    var: String,
    index: Option<String>,
}

impl Repetition {
    fn multiple(&self) -> bool {
        self.eligible.as_ref().is_none_or(|rows| rows.len() > 1)
    }
}

#[derive(Default)]
struct Walk<'a> {
    scoped: Vec<(&'a str, u32)>,
    styles: &'a [StyleDecl],
    next: usize,
    ancestors: Vec<Ancestor>,
    choices: Vec<(Choice, bool)>,
    bindings: Vec<Binding>,
    locals: Vec<Local<'a>>,
    loops: Vec<Repetition>,
    regions: Vec<(u32, BTreeSet<u32>)>,
}

pub(super) fn owners(checked: &Checked<'_>, out: &mut Vec<LowerError>) {
    let mut walk = Walk {
        scoped: checked
            .expanded
            .root
            .states
            .iter()
            .zip(&checked.expanded.owners)
            .filter_map(|(s, owner)| match owner {
                Owner::Arm { tag, .. } => Some((s.name.as_str(), *tag)),
                _ => None,
            })
            .collect(),
        styles: &checked.file.styles,
        ..Walk::default()
    };
    walk.nodes(&checked.expanded.root.view);
    let mut seen = BTreeSet::new();
    for binding in &walk.bindings {
        let competing = binding.repeated
            || walk.bindings.iter().any(|other| {
                (binding.target != other.target || binding.rows != other.rows)
                    && !binding.choices.iter().any(|(choice, arm)| {
                        other.choices.iter().any(|(c, a)| c == choice && a != arm)
                    })
            });
        if !competing || !seen.insert(binding.span) {
            continue;
        }
        let why = if binding.repeated {
            "this target is repeated by a `for` template and may compete across rows"
        } else {
            "this binding can coexist with a binding to another target"
        };
        out.push(LowerError {
            id: "lower-transform-drag-owners",
            message: format!(
                "`transformDragFor`: {why}; only the first valid target owns the transform drag, and ownership is never stolen while it has a valid binding (LLP 1071). Other targets' handles stay inactive. Mount handles only for the active target"
            ),
            span: binding.span,
        });
    }
}

impl<'a> Walk<'a> {
    fn nodes(&mut self, nodes: &'a [Node]) {
        for node in nodes {
            match node {
                Node::Element {
                    attrs, children, ..
                } => {
                    let rows =
                        width::rows(attrs, self.styles).unwrap_or_else(|| attrs.iter().collect());
                    if matches!(width::last(&rows, "display"), Some(a) if matches!(&a.value, Expr::Str(s, _) if s == "none"))
                        || ["inert", "disabled"].iter().any(|name| {
                            matches!(
                                attr(attrs, name),
                                Some(Attr {
                                    value: Expr::Bool(true, _),
                                    ..
                                })
                            )
                        })
                    {
                        continue;
                    }
                    if has(attrs, "transformgeometry") && has(attrs, "transformrelease") {
                        if let Some(a) = attr(attrs, "transformDragFor") {
                            if let Some(target) = self.target(&a.value) {
                                self.bindings.push(Binding {
                                    target: target.target,
                                    rows: target
                                        .loops
                                        .iter()
                                        .map(|tag| {
                                            self.loops
                                                .iter()
                                                .find(|l| l.tag == *tag)
                                                .and_then(|l| l.eligible.as_ref()?.first().copied())
                                        })
                                        .collect(),
                                    span: a.span,
                                    repeated: target.loops.iter().any(|tag| self.multiple(*tag)),
                                    choices: self.choices.clone(),
                                });
                            }
                        }
                    }
                    let id = attr(attrs, "id");
                    if let Some(a) = id {
                        self.ancestors.push(Ancestor {
                            target: self.next,
                            name: self.name(&a.value),
                            loops: self.loops.iter().map(|l| l.tag).collect(),
                        });
                        self.next += 1;
                    }
                    self.nodes(children);
                    if id.is_some() {
                        self.ancestors.pop();
                    }
                }
                Node::When {
                    tag,
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    let exclusive = !self.varies(cond);
                    let cond = self.read(cond);
                    let guard = cond.as_ref().map(|cond| (Test::Cond, cond));
                    self.arm(*tag, false, then, exclusive, guard);
                    self.arm(*tag, true, otherwise, exclusive, guard);
                }
                Node::Match {
                    tag,
                    subject,
                    some,
                    none,
                    ..
                } => {
                    let exclusive = !self.varies(subject);
                    let read = self.read(subject);
                    let guard = read.as_ref().map(|subject| (Test::Subject, subject));
                    let count = self.locals.len();
                    let known_arm = match_arm(subject);
                    if known_arm != Some(true) {
                        self.bind(&some.0, *tag, self.dependencies(subject));
                        self.arm(*tag, false, &some.1, exclusive, guard);
                    }
                    self.locals.truncate(count);
                    if known_arm != Some(false) {
                        self.arm(*tag, true, none, exclusive, guard);
                    }
                }
                Node::Each {
                    tag,
                    var,
                    index,
                    list,
                    body,
                    ..
                } => {
                    let items = match list {
                        Expr::List(items, _) => Some(
                            items
                                .iter()
                                .map(|item| self.read(item).unwrap_or_else(|| item.clone()))
                                .collect::<Vec<_>>(),
                        ),
                        _ => None,
                    };
                    if items.as_ref().is_some_and(Vec::is_empty) {
                        continue;
                    }
                    let count = self.locals.len();
                    let mut dependencies = self.dependencies(list);
                    dependencies.insert(*tag);
                    let var = self.bind(var, *tag, dependencies);
                    let index = index
                        .as_deref()
                        .map(|name| self.bind(name, *tag, BTreeSet::from([*tag])));
                    self.loops.push(Repetition {
                        tag: *tag,
                        eligible: items.as_ref().map(|items| (0..items.len()).collect()),
                        items,
                        var,
                        index,
                    });
                    self.arm(*tag, false, body, false, None);
                    self.loops.pop();
                    self.locals.truncate(count);
                }
                Node::Use { .. } | Node::Children { .. } => {}
            }
        }
    }

    fn arm(
        &mut self,
        tag: u32,
        arm: bool,
        nodes: &'a [Node],
        exclusive: bool,
        guard: Option<(Test, &Expr)>,
    ) {
        let saved: Vec<_> = self.loops.iter().map(|l| l.eligible.clone()).collect();
        if let Some((test, guard)) = guard {
            if taken(test, read(guard, &|_| None)) == Some(!arm) {
                return;
            }
            for repetition in &mut self.loops {
                if repetition.eligible.is_none() {
                    repetition.eligible =
                        unique_index(guard, repetition.index.as_deref(), arm).map(|i| vec![i]);
                }
                if let Some(eligible) = &mut repetition.eligible {
                    eligible.retain(|i| {
                        let value = read(guard, &|name| {
                            if name == repetition.var {
                                repetition.items.as_ref().map(|items| items[*i].clone())
                            } else if repetition.index.as_deref() == Some(name) {
                                Some(Expr::Number(*i as f64, Span::default()))
                            } else {
                                None
                            }
                        });
                        taken(test, value) != Some(!arm)
                    });
                }
            }
        }
        let unreachable = self
            .loops
            .iter()
            .any(|l| l.eligible.as_ref().is_some_and(Vec::is_empty));
        if exclusive {
            self.choices.push(match guard {
                Some((Test::Cond, guard)) => {
                    let (guard, arm) = polarity(guard, arm);
                    (Choice::Read(Test::Cond, guard), arm)
                }
                Some((test, guard)) => (Choice::Read(test, guard.clone()), arm),
                None => (Choice::Region(tag), arm),
            });
        }
        self.regions
            .push((tag, self.loops.iter().map(|l| l.tag).collect()));
        if !unreachable {
            self.nodes(nodes);
        }
        self.regions.pop();
        if exclusive {
            self.choices.pop();
        }
        for (repetition, eligible) in self.loops.iter_mut().zip(saved) {
            repetition.eligible = eligible;
        }
    }

    fn multiple(&self, tag: u32) -> bool {
        self.loops.iter().any(|l| l.tag == tag && l.multiple())
    }

    fn bind(&mut self, name: &'a str, tag: u32, dependencies: BTreeSet<u32>) -> String {
        let symbol = format!("{name}@transform:{tag}");
        self.locals.push(Local {
            name,
            symbol: symbol.clone(),
            dependencies,
        });
        symbol
    }

    fn read(&self, value: &Expr) -> Option<Expr> {
        read(value, &|name| {
            self.locals
                .iter()
                .rev()
                .find(|l| l.name == name)
                .map(|l| Expr::Ident(l.symbol.clone(), Span::default()))
        })
    }

    fn name(&self, value: &Expr) -> Option<Expr> {
        let value = self.read(value)?;
        simple(&value).then_some(value)
    }

    /// Shared state and derives choose the same arm in every row. Lifted
    /// child state belongs to its region instance, even with a shared initializer.
    /// Expansion substitutes child derives with their bodies; root derives are shared.
    fn varies(&self, value: &Expr) -> bool {
        self.dependencies(value)
            .iter()
            .any(|tag| self.multiple(*tag))
    }

    fn dependencies(&self, value: &Expr) -> BTreeSet<u32> {
        let mut out = BTreeSet::new();
        self.collect_dependencies(value, &mut out);
        out
    }

    fn collect_dependencies(&self, value: &Expr, out: &mut BTreeSet<u32>) {
        match value {
            Expr::Ident(name, _) => {
                if let Some(local) = self.locals.iter().rev().find(|l| l.name == name) {
                    out.extend(&local.dependencies);
                } else {
                    for (_, tag) in self.scoped.iter().filter(|(n, _)| n == name) {
                        if let Some((_, loops)) =
                            self.regions.iter().find(|(region, _)| region == tag)
                        {
                            out.extend(loops);
                        }
                    }
                }
            }
            Expr::Member(e, _, _)
            | Expr::Unary(_, e, _)
            | Expr::Some(e, _)
            | Expr::NamedArg(_, e, _)
            | Expr::Typed(e, _, _) => self.collect_dependencies(e, out),
            Expr::Binary(_, a, b, _)
            | Expr::Let {
                value: a, body: b, ..
            } => {
                self.collect_dependencies(a, out);
                self.collect_dependencies(b, out);
            }
            Expr::Ternary(a, b, c, _) => {
                for e in [a, b, c] {
                    self.collect_dependencies(e, out);
                }
            }
            Expr::Call(_, items, _) | Expr::List(items, _) => {
                for e in items {
                    self.collect_dependencies(e, out);
                }
            }
            Expr::Template(parts, _) => {
                for part in parts {
                    if let TemplatePart::Expr(e) = part {
                        self.collect_dependencies(e, out);
                    }
                }
            }
            Expr::Match {
                subject,
                some,
                none,
                ..
            } => {
                for e in [subject, some, none] {
                    self.collect_dependencies(e, out);
                }
            }
            Expr::Arrow { body, .. } => self.collect_dependencies(body, out),
            Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) => {}
        }
    }

    fn target(&self, value: &Expr) -> Option<&Ancestor> {
        let value = self.name(value)?;
        if matches!(&value, Expr::Str(s, _) if s.is_empty()) {
            return None;
        }
        let mut target = None;
        for ancestor in &self.ancestors {
            let id = ancestor.name.as_ref()?;
            if id == &value {
                if target.is_some() {
                    return None;
                }
                target = Some(ancestor);
            } else if names_may_match(id, &value) {
                // Different computed names might still name the same ancestor.
                return None;
            }
        }
        target
    }
}

fn attr<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a Attr> {
    attrs.iter().rev().find(|a| a.name == name)
}

fn match_arm(subject: &Expr) -> Option<bool> {
    match subject {
        Expr::Some(..) => Some(false),
        Expr::None(_) => Some(true),
        Expr::Typed(subject, _, _) => match_arm(subject),
        _ => None,
    }
}

/// The arm a value read from a guard takes, when it is known.
fn taken(test: Test, value: Option<Expr>) -> Option<bool> {
    match (test, value?) {
        (Test::Cond, Expr::Bool(value, _)) => Some(!value),
        (Test::Subject, value) => match_arm(&value),
        _ => None,
    }
}

/// `not c` and `a != b` choose the arms of `c` and `a == b` the other way round.
fn polarity(guard: &Expr, arm: bool) -> (Expr, bool) {
    match guard {
        Expr::Unary(UnOp::Not, inner, _) => {
            let (guard, arm) = polarity(inner, arm);
            (guard, !arm)
        }
        Expr::Binary(BinOp::Ne, a, b, span) => {
            (Expr::Binary(BinOp::Eq, a.clone(), b.clone(), *span), !arm)
        }
        _ => (guard.clone(), arm),
    }
}

/// An unknown list still has at most one row at a written index.
fn unique_index(guard: &Expr, index: Option<&str>, arm: bool) -> Option<usize> {
    let Expr::Binary(op, a, b, _) = guard else {
        return None;
    };
    if !matches!((op, arm), (BinOp::Eq, false) | (BinOp::Ne, true)) {
        return None;
    }
    let (name, value) = match (&**a, &**b) {
        (Expr::Ident(name, _), Expr::Number(n, _)) | (Expr::Number(n, _), Expr::Ident(name, _)) => {
            (name, *n)
        }
        _ => return None,
    };
    (index == Some(name.as_str())
        && value >= 0.0
        && value < usize::MAX as f64
        && value.fract() == 0.0)
        .then_some(value as usize)
}

/// Calls and other computed IDs stay with the runtime's ancestor lookup.
fn simple(value: &Expr) -> bool {
    match value {
        Expr::Str(..) | Expr::Ident(..) => true,
        Expr::Member(base, _, _) => simple(base),
        Expr::Template(parts, _) => parts.iter().all(|part| match part {
            TemplatePart::Text(_) => true,
            TemplatePart::Expr(e) => simple(e),
        }),
        _ => false,
    }
}

/// Read names and literal guards without locations. The resolver supplies lexical
/// identities, or one candidate row's values while checking a guard.
fn read(value: &Expr, resolve: &impl Fn(&str) -> Option<Expr>) -> Option<Expr> {
    let span = Span::default();
    Some(match value {
        Expr::Ident(name, _) => resolve(name).unwrap_or_else(|| Expr::Ident(name.clone(), span)),
        Expr::Str(text, _) => Expr::Str(text.clone(), span),
        Expr::Number(n, _) => Expr::Number(*n, span),
        Expr::Bool(value, _) => Expr::Bool(*value, span),
        Expr::None(_) => Expr::None(span),
        Expr::Some(value, _) => Expr::Some(Box::new(read(value, resolve)?), span),
        Expr::Typed(value, _, _) => return read(value, resolve),
        Expr::Member(base, field, _) => {
            Expr::Member(Box::new(read(base, resolve)?), field.clone(), span)
        }
        Expr::Template(parts, _) => {
            let mut result = Vec::new();
            let mut text = String::new();
            for part in parts {
                match part {
                    TemplatePart::Text(t) => text.push_str(t),
                    TemplatePart::Expr(e) => match read(e, resolve)? {
                        Expr::Str(t, _) => text.push_str(&t),
                        Expr::Number(n, _) => exact_num::push_js(n, &mut text),
                        Expr::Bool(b, _) => text.push_str(if b { "true" } else { "false" }),
                        e => {
                            if !text.is_empty() {
                                result.push(TemplatePart::Text(std::mem::take(&mut text)));
                            }
                            result.push(TemplatePart::Expr(e));
                        }
                    },
                }
            }
            if result.is_empty() {
                Expr::Str(text, span)
            } else {
                if !text.is_empty() {
                    result.push(TemplatePart::Text(text));
                }
                Expr::Template(result, span)
            }
        }
        Expr::Ternary(cond, a, b, _) => match read(cond, resolve)? {
            Expr::Bool(cond, _) => return read(if cond { a } else { b }, resolve),
            cond => Expr::Ternary(
                Box::new(cond),
                Box::new(read(a, resolve)?),
                Box::new(read(b, resolve)?),
                span,
            ),
        },
        Expr::Unary(op, value, _) => {
            let value = read(value, resolve)?;
            match (op, &value) {
                (UnOp::Neg, Expr::Number(n, _)) => Expr::Number(-n, span),
                (UnOp::Not, Expr::Bool(b, _)) => Expr::Bool(!b, span),
                _ => Expr::Unary(*op, Box::new(value), span),
            }
        }
        Expr::Binary(op, a, b, _) => {
            let (a, b) = (read(a, resolve)?, read(b, resolve)?);
            let known = match (op, &a, &b) {
                (
                    BinOp::Eq | BinOp::Ne,
                    Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_),
                    Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_),
                ) => Some((a == b) == (*op == BinOp::Eq)),
                (BinOp::Lt, Expr::Number(a, _), Expr::Number(b, _)) => Some(a < b),
                (BinOp::Le, Expr::Number(a, _), Expr::Number(b, _)) => Some(a <= b),
                (BinOp::Gt, Expr::Number(a, _), Expr::Number(b, _)) => Some(a > b),
                (BinOp::Ge, Expr::Number(a, _), Expr::Number(b, _)) => Some(a >= b),
                (BinOp::And, Expr::Bool(false, _), _) | (BinOp::And, _, Expr::Bool(false, _)) => {
                    Some(false)
                }
                (BinOp::Or, Expr::Bool(true, _), _) | (BinOp::Or, _, Expr::Bool(true, _)) => {
                    Some(true)
                }
                (BinOp::And | BinOp::Or, Expr::Bool(a, _), Expr::Bool(b, _)) => {
                    Some(if *op == BinOp::And {
                        *a && *b
                    } else {
                        *a || *b
                    })
                }
                _ => None,
            };
            match known {
                Some(value) => Expr::Bool(value, span),
                None => Expr::Binary(*op, Box::new(a), Box::new(b), span),
            }
        }
        _ => return None,
    })
}
