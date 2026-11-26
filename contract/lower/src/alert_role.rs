//! Reachable `"alertdialog"` role analysis for `dialog` and popover elements.
//!
//! `check_menu_shapes` and `warnings::roles::alert_font` share this evaluator
//! so computed roles (`cond ? "alertdialog" : "dialog"`, props, derives, state
//! assignments, matches, templates, pure `fn` calls) are checked without
//! scanning unrelated elements, unrelated actions, or enclosing view bindings
//! the `role` expression never reads.
//!
//! Actions are indexed once per component into per-statement `Write`, `Let`,
//! and `Guard` nodes with resolved lexical scopes (`O(AST)`). Root `state` and
//! `derive` values are then solved lazily by a monotone worklist fixed-point
//! over only the reachable subgraph, so cycles, deep derive DAGs, and giant
//! multi-state actions run in linear bounded time without quadratic re-walks.

use contract_syntax::{Attr, BinOp, Component, Expr, Param, Stmt, TemplatePart, UnOp};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

/// Whether an element's `role` attribute can evaluate to `"alertdialog"`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlertRole {
    Never,
    Maybe,
    Always,
}

const ALERT_ROLE: &str = "alertdialog";
const MAX_ROLE_VALUES: usize = 32;
const MAX_STR_LEN: usize = 128;
const MAX_OPTION_DEPTH: usize = 4;
const MAX_EXPR_DEPTH: usize = 32;
const MAX_EVAL_STEPS: usize = 16_384;
const MIN_WORKLIST_STEPS: usize = 65_536;

type LookupFn<'a, 'f> = dyn Fn(&str) -> Option<(&'a [Param], &'a Expr)> + 'f;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RoleVal {
    Bool(bool),
    Str(String),
    None,
    /// `some(v)`, where `None` means the wrapped payload is unknown.
    Some(Option<Box<RoleVal>>),
}

/// Every substring of `"alertdialog"` (at most 63 short strings) is retained
/// across value-set saturation and template overflow so widening an input can
/// never drop a reachable `"alertdialog"` composition.
fn is_alert_fragment(s: &str) -> bool {
    s.len() <= ALERT_ROLE.len() && ALERT_ROLE.contains(s)
}

fn option_depth(value: &RoleVal) -> usize {
    match value {
        RoleVal::Some(Some(inner)) => 1 + option_depth(inner),
        RoleVal::Some(None) => 1,
        _ => 0,
    }
}

fn is_preserved_val(value: &RoleVal) -> bool {
    match value {
        RoleVal::Str(s) => is_alert_fragment(s),
        RoleVal::Some(Some(inner)) if option_depth(value) <= MAX_OPTION_DEPTH => {
            is_preserved_val(inner)
        }
        _ => false,
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct ValSet {
    values: BTreeSet<RoleVal>,
    non_preserved: usize,
    unknown: bool,
}

impl ValSet {
    fn unknown() -> Self {
        Self {
            values: BTreeSet::new(),
            non_preserved: 0,
            unknown: true,
        }
    }

    fn top_alert() -> Self {
        let mut out = Self::unknown();
        out.add(RoleVal::Str(ALERT_ROLE.into()));
        out
    }

    fn one(value: RoleVal) -> Self {
        let mut out = Self::default();
        out.add(value);
        out
    }

    fn any_bool() -> Self {
        let mut out = Self::default();
        out.add(RoleVal::Bool(false));
        out.add(RoleVal::Bool(true));
        out
    }

    fn is_empty(&self) -> bool {
        self.values.is_empty() && !self.unknown
    }

    fn add(&mut self, value: RoleVal) {
        if self.values.contains(&value) {
            return;
        }
        if is_preserved_val(&value) {
            self.values.insert(value);
        } else if self.non_preserved < MAX_ROLE_VALUES {
            self.values.insert(value);
            self.non_preserved += 1;
        } else {
            self.unknown = true;
        }
    }

    fn union_with(&mut self, other: Self) -> bool {
        let before_unknown = self.unknown;
        let before_len = self.values.len();
        self.unknown |= other.unknown;
        for value in other.values {
            self.add(value);
        }
        self.unknown != before_unknown || self.values.len() != before_len
    }

    fn mark_non_string_state(&mut self) -> bool {
        if !self.unknown && !self.values.iter().all(|v| matches!(v, RoleVal::Str(_))) {
            self.unknown = true;
            true
        } else {
            false
        }
    }

    fn may_be_true(&self) -> bool {
        self.unknown || self.values.contains(&RoleVal::Bool(true))
    }

    fn may_be_false(&self) -> bool {
        self.unknown || self.values.contains(&RoleVal::Bool(false))
    }

    fn may_be_some(&self) -> bool {
        self.unknown || self.values.iter().any(|v| matches!(v, RoleVal::Some(_)))
    }

    fn may_be_none(&self) -> bool {
        self.unknown || self.values.contains(&RoleVal::None)
    }

    fn wrap_some(self) -> Self {
        let mut out = Self::default();
        for value in self.values {
            if option_depth(&value) >= MAX_OPTION_DEPTH {
                out.add(RoleVal::Some(None));
            } else {
                out.add(RoleVal::Some(Some(Box::new(value))));
            }
        }
        if self.unknown {
            out.add(RoleVal::Some(None));
        }
        out
    }

    fn unwrap_some(&self) -> Self {
        let mut out = Self {
            values: BTreeSet::new(),
            non_preserved: 0,
            unknown: self.unknown,
        };
        for value in &self.values {
            match value {
                RoleVal::Some(Some(inner)) => out.add((**inner).clone()),
                RoleVal::Some(None) => out.unknown = true,
                _ => {}
            }
        }
        out
    }

    fn to_alert_role(&self) -> AlertRole {
        let has_alert = self
            .values
            .iter()
            .any(|v| matches!(v, RoleVal::Str(s) if s == ALERT_ROLE));
        if !has_alert {
            AlertRole::Never
        } else if !self.unknown && self.values.len() == 1 {
            AlertRole::Always
        } else {
            AlertRole::Maybe
        }
    }
}

fn role_val_eq(a: &RoleVal, b: &RoleVal) -> Option<bool> {
    match (a, b) {
        (RoleVal::Bool(x), RoleVal::Bool(y)) => Some(x == y),
        (RoleVal::Str(x), RoleVal::Str(y)) => Some(x == y),
        (RoleVal::None, RoleVal::None) => Some(true),
        (RoleVal::Some(_), RoleVal::None) | (RoleVal::None, RoleVal::Some(_)) => Some(false),
        (RoleVal::Some(Some(x)), RoleVal::Some(Some(y))) => role_val_eq(x, y),
        _ => None,
    }
}

fn template_parts_may_match_alertdialog(parts: &[(Vec<String>, bool)]) -> bool {
    let mut reachable: u16 = 1;
    for (known, is_unknown) in parts {
        let mut next: u16 = 0;
        for pos in 0..=ALERT_ROLE.len() {
            if reachable & (1 << pos) == 0 {
                continue;
            }
            for s in known {
                if ALERT_ROLE[pos..].starts_with(s.as_str()) {
                    next |= 1 << (pos + s.len());
                }
            }
            if *is_unknown {
                for end in pos..=ALERT_ROLE.len() {
                    next |= 1 << end;
                }
            }
        }
        reachable = next;
        if reachable == 0 {
            return false;
        }
    }
    reachable & (1 << ALERT_ROLE.len()) != 0
}

enum LetKind<'a> {
    Param,
    Expr(&'a Expr),
    MatchSome(&'a Expr),
}

struct LetNode<'a> {
    kind: LetKind<'a>,
    locals: Vec<(&'a str, usize)>,
    root_deps: Vec<&'a str>,
}

enum GuardKind<'a> {
    If {
        cond: &'a Expr,
        expect_true: bool,
    },
    Match {
        subject: &'a Expr,
        expect_some: bool,
    },
}

struct GuardNode<'a> {
    kind: GuardKind<'a>,
    locals: Vec<(&'a str, usize)>,
    root_deps: Vec<&'a str>,
    parent: Option<usize>,
}

struct WriteNode<'a> {
    target: &'a str,
    expr: &'a Expr,
    locals: Vec<(&'a str, usize)>,
    root_deps: Vec<&'a str>,
    guard: Option<usize>,
}

struct RootIndex<'a> {
    derives: HashMap<&'a str, usize>,
    states: HashMap<&'a str, usize>,
    state_writes: HashMap<&'a str, Vec<usize>>,
    writes: Vec<WriteNode<'a>>,
    lets: Vec<LetNode<'a>>,
    guards: Vec<GuardNode<'a>>,
}

impl<'a> RootIndex<'a> {
    fn build(root: &'a Component) -> Self {
        let derives: HashMap<&'a str, usize> = root
            .derives
            .iter()
            .enumerate()
            .map(|(i, d)| (d.name.as_str(), i))
            .collect();
        let states: HashMap<&'a str, usize> = root
            .states
            .iter()
            .enumerate()
            .map(|(i, s)| (s.name.as_str(), i))
            .collect();
        let mut index = Self {
            derives,
            states,
            state_writes: HashMap::new(),
            writes: Vec::new(),
            lets: Vec::new(),
            guards: Vec::new(),
        };
        let mut lexical: HashMap<&'a str, usize> = HashMap::new();
        for action in &root.actions {
            lexical.clear();
            for param in &action.params {
                let id = index.lets.len();
                index.lets.push(LetNode {
                    kind: LetKind::Param,
                    locals: Vec::new(),
                    root_deps: Vec::new(),
                });
                lexical.insert(param.name.as_str(), id);
            }
            index.index_stmts(&action.body, &mut lexical, None);
        }
        index
    }

    fn resolve_expr_deps(
        &self,
        expr: &'a Expr,
        lexical: &HashMap<&'a str, usize>,
    ) -> (Vec<(&'a str, usize)>, Vec<&'a str>) {
        let mut idents = Vec::new();
        collect_expr_idents(expr, &mut idents);
        let mut seen = HashSet::new();
        let mut locals = Vec::new();
        let mut roots = Vec::new();
        for name in idents {
            if !seen.insert(name) {
                continue;
            }
            if let Some(&let_id) = lexical.get(name) {
                locals.push((name, let_id));
            } else if let Some((&k, _)) = self
                .derives
                .get_key_value(name)
                .or_else(|| self.states.get_key_value(name))
            {
                roots.push(k);
            }
        }
        (locals, roots)
    }

    fn index_stmts(
        &mut self,
        stmts: &'a [Stmt],
        lexical: &mut HashMap<&'a str, usize>,
        guard: Option<usize>,
    ) {
        let mut saved: Vec<(&'a str, Option<usize>)> = Vec::new();
        for stmt in stmts {
            match stmt {
                Stmt::Let { name, expr, .. } => {
                    let (locals, root_deps) = self.resolve_expr_deps(expr, lexical);
                    let id = self.lets.len();
                    self.lets.push(LetNode {
                        kind: LetKind::Expr(expr),
                        locals,
                        root_deps,
                    });
                    let prev = lexical.insert(name.as_str(), id);
                    saved.push((name.as_str(), prev));
                }
                Stmt::Assign { target, expr, .. } => {
                    let Some((&target_key, _)) = self.states.get_key_value(target.as_str()) else {
                        continue;
                    };
                    let (locals, root_deps) = self.resolve_expr_deps(expr, lexical);
                    let write_id = self.writes.len();
                    self.writes.push(WriteNode {
                        target: target_key,
                        expr,
                        locals,
                        root_deps,
                        guard,
                    });
                    self.state_writes
                        .entry(target_key)
                        .or_default()
                        .push(write_id);
                }
                Stmt::If {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    let (locals, root_deps) = self.resolve_expr_deps(cond, lexical);
                    let then_guard = self.guards.len();
                    self.guards.push(GuardNode {
                        kind: GuardKind::If {
                            cond,
                            expect_true: true,
                        },
                        locals: locals.clone(),
                        root_deps: root_deps.clone(),
                        parent: guard,
                    });
                    self.index_stmts(then, lexical, Some(then_guard));
                    if !otherwise.is_empty() {
                        let else_guard = self.guards.len();
                        self.guards.push(GuardNode {
                            kind: GuardKind::If {
                                cond,
                                expect_true: false,
                            },
                            locals,
                            root_deps,
                            parent: guard,
                        });
                        self.index_stmts(otherwise, lexical, Some(else_guard));
                    }
                }
                Stmt::Match {
                    subject,
                    some,
                    none,
                    ..
                } => {
                    let (locals, root_deps) = self.resolve_expr_deps(subject, lexical);
                    let some_guard = self.guards.len();
                    self.guards.push(GuardNode {
                        kind: GuardKind::Match {
                            subject,
                            expect_some: true,
                        },
                        locals: locals.clone(),
                        root_deps: root_deps.clone(),
                        parent: guard,
                    });
                    let var_id = self.lets.len();
                    self.lets.push(LetNode {
                        kind: LetKind::MatchSome(subject),
                        locals: locals.clone(),
                        root_deps: root_deps.clone(),
                    });
                    let prev = lexical.insert(some.0.as_str(), var_id);
                    self.index_stmts(&some.1, lexical, Some(some_guard));
                    match prev {
                        Some(old) => {
                            lexical.insert(some.0.as_str(), old);
                        }
                        None => {
                            lexical.remove(some.0.as_str());
                        }
                    }
                    if !none.is_empty() {
                        let none_guard = self.guards.len();
                        self.guards.push(GuardNode {
                            kind: GuardKind::Match {
                                subject,
                                expect_some: false,
                            },
                            locals,
                            root_deps,
                            parent: guard,
                        });
                        self.index_stmts(none, lexical, Some(none_guard));
                    }
                }
                Stmt::Call { body, .. } => {
                    self.index_stmts(body, lexical, guard);
                }
                Stmt::Command { .. } | Stmt::Send { .. } | Stmt::Refresh { .. } => {}
            }
        }
        for (name, prev) in saved.into_iter().rev() {
            match prev {
                Some(old) => {
                    lexical.insert(name, old);
                }
                None => {
                    lexical.remove(name);
                }
            }
        }
    }
}

fn collect_expr_idents<'a>(expr: &'a Expr, out: &mut Vec<&'a str>) {
    match expr {
        Expr::Ident(name, _) => out.push(name.as_str()),
        Expr::Some(inner, _)
        | Expr::Typed(inner, _, _)
        | Expr::Unary(_, inner, _)
        | Expr::NamedArg(_, inner, _) => collect_expr_idents(inner, out),
        Expr::Binary(_, l, r, _) => {
            collect_expr_idents(l, out);
            collect_expr_idents(r, out);
        }
        Expr::Ternary(c, y, n, _) => {
            collect_expr_idents(c, out);
            collect_expr_idents(y, out);
            collect_expr_idents(n, out);
        }
        Expr::Match {
            subject,
            some,
            none,
            ..
        } => {
            collect_expr_idents(subject, out);
            collect_expr_idents(some, out);
            collect_expr_idents(none, out);
        }
        Expr::Let { value, body, .. } => {
            collect_expr_idents(value, out);
            collect_expr_idents(body, out);
        }
        Expr::Template(parts, _) => {
            for part in parts {
                if let TemplatePart::Expr(e) = part {
                    collect_expr_idents(e, out);
                }
            }
        }
        Expr::Call(_, args, _) | Expr::List(args, _) => {
            for arg in args {
                collect_expr_idents(arg, out);
            }
        }
        Expr::Member(inner, _, _) => collect_expr_idents(inner, out),
        Expr::Arrow { body, .. } => collect_expr_idents(body, out),
        Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) => {}
    }
}

/// Cached root `state`/`derive` values, solved action locals/guards, and lazy
/// per-statement action index shared across all `dialog` and popover elements
/// in one lowering/warning pass.
#[derive(Default)]
pub(crate) struct RoleCache<'a> {
    index: Option<RootIndex<'a>>,
    values: HashMap<&'a str, ValSet>,
    let_vals: HashMap<usize, ValSet>,
    guard_open: HashMap<usize, bool>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum WorkItem {
    StateInit(usize),
    Derive(usize),
    Write(usize),
    Let(usize),
    Guard(usize),
}

impl<'a> RoleCache<'a> {
    fn ensure_solved(&mut self, root: &'a Component, lookup_fn: &LookupFn<'a, '_>, start: &str) {
        if self.values.contains_key(start) {
            return;
        }
        let index = self.index.get_or_insert_with(|| RootIndex::build(root));
        let Some(start_key) = index
            .derives
            .get_key_value(start)
            .map(|(k, _)| *k)
            .or_else(|| index.states.get_key_value(start).map(|(k, _)| *k))
        else {
            return;
        };

        // Collect only the reachable unsolved subgraph (states, derives,
        // writes, lets, and guards).
        let mut visited_roots: HashSet<&'a str> = HashSet::new();
        let mut sub_states: Vec<usize> = Vec::new();
        let mut sub_derives: Vec<usize> = Vec::new();
        let mut sub_writes: Vec<usize> = Vec::new();
        let mut sub_lets: Vec<usize> = Vec::new();
        let mut sub_guards: Vec<usize> = Vec::new();
        let mut write_seen: HashSet<usize> = HashSet::new();
        let mut let_seen: HashSet<usize> = HashSet::new();
        let mut guard_seen: HashSet<usize> = HashSet::new();

        enum Explore<'a> {
            Root(&'a str),
            Write(usize),
            Let(usize),
            Guard(usize),
        }

        let mut stack = vec![Explore::Root(start_key)];
        let mut idents = Vec::new();
        while let Some(node) = stack.pop() {
            match node {
                Explore::Root(name) => {
                    if self.values.contains_key(name) || !visited_roots.insert(name) {
                        continue;
                    }
                    idents.clear();
                    if let Some(&d_idx) = index.derives.get(name) {
                        sub_derives.push(d_idx);
                        collect_expr_idents(&root.derives[d_idx].expr, &mut idents);
                    } else if let Some(&s_idx) = index.states.get(name) {
                        sub_states.push(s_idx);
                        collect_expr_idents(&root.states[s_idx].expr, &mut idents);
                        if let Some(writes) = index.state_writes.get(name) {
                            for &w_idx in writes {
                                if write_seen.insert(w_idx) {
                                    sub_writes.push(w_idx);
                                    stack.push(Explore::Write(w_idx));
                                }
                            }
                        }
                    }
                    for &dep in &idents {
                        if let Some((&k, _)) = index
                            .derives
                            .get_key_value(dep)
                            .or_else(|| index.states.get_key_value(dep))
                        {
                            if !self.values.contains_key(k) && !visited_roots.contains(k) {
                                stack.push(Explore::Root(k));
                            }
                        }
                    }
                }
                Explore::Write(w_idx) => {
                    let w = &index.writes[w_idx];
                    for &dep in &w.root_deps {
                        if !self.values.contains_key(dep) && !visited_roots.contains(dep) {
                            stack.push(Explore::Root(dep));
                        }
                    }
                    for &(_, l_idx) in &w.locals {
                        if !self.let_vals.contains_key(&l_idx) && let_seen.insert(l_idx) {
                            sub_lets.push(l_idx);
                            stack.push(Explore::Let(l_idx));
                        }
                    }
                    if let Some(g_idx) = w.guard {
                        if !self.guard_open.contains_key(&g_idx) && guard_seen.insert(g_idx) {
                            sub_guards.push(g_idx);
                            stack.push(Explore::Guard(g_idx));
                        }
                    }
                }
                Explore::Let(l_idx) => {
                    let l = &index.lets[l_idx];
                    for &dep in &l.root_deps {
                        if !self.values.contains_key(dep) && !visited_roots.contains(dep) {
                            stack.push(Explore::Root(dep));
                        }
                    }
                    for &(_, dep_l) in &l.locals {
                        if !self.let_vals.contains_key(&dep_l) && let_seen.insert(dep_l) {
                            sub_lets.push(dep_l);
                            stack.push(Explore::Let(dep_l));
                        }
                    }
                }
                Explore::Guard(g_idx) => {
                    let g = &index.guards[g_idx];
                    for &dep in &g.root_deps {
                        if !self.values.contains_key(dep) && !visited_roots.contains(dep) {
                            stack.push(Explore::Root(dep));
                        }
                    }
                    for &(_, dep_l) in &g.locals {
                        if !self.let_vals.contains_key(&dep_l) && let_seen.insert(dep_l) {
                            sub_lets.push(dep_l);
                            stack.push(Explore::Let(dep_l));
                        }
                    }
                    if let Some(parent) = g.parent {
                        if !self.guard_open.contains_key(&parent) && guard_seen.insert(parent) {
                            sub_guards.push(parent);
                            stack.push(Explore::Guard(parent));
                        }
                    }
                }
            }
        }

        // Build reverse dependency lists for incremental worklist updates.
        let mut root_readers: HashMap<&'a str, Vec<WorkItem>> = HashMap::new();
        let mut let_readers: HashMap<usize, Vec<WorkItem>> = HashMap::new();
        let mut guard_readers: HashMap<usize, Vec<WorkItem>> = HashMap::new();

        let mut add_root_edges = |item: WorkItem, deps: &[&'a str]| {
            let mut local_seen = HashSet::new();
            for &name in deps {
                if visited_roots.contains(name) && local_seen.insert(name) {
                    root_readers.entry(name).or_default().push(item);
                }
            }
        };
        for &s_idx in &sub_states {
            idents.clear();
            collect_expr_idents(&root.states[s_idx].expr, &mut idents);
            add_root_edges(WorkItem::StateInit(s_idx), &idents);
        }
        for &d_idx in &sub_derives {
            idents.clear();
            collect_expr_idents(&root.derives[d_idx].expr, &mut idents);
            add_root_edges(WorkItem::Derive(d_idx), &idents);
        }
        for &w_idx in &sub_writes {
            let w = &index.writes[w_idx];
            let item = WorkItem::Write(w_idx);
            add_root_edges(item, &w.root_deps);
            for &(_, l_idx) in &w.locals {
                if let_seen.contains(&l_idx) {
                    let_readers.entry(l_idx).or_default().push(item);
                }
            }
            if let Some(g_idx) = w.guard {
                if guard_seen.contains(&g_idx) {
                    guard_readers.entry(g_idx).or_default().push(item);
                }
            }
        }
        for &l_idx in &sub_lets {
            let l = &index.lets[l_idx];
            let item = WorkItem::Let(l_idx);
            add_root_edges(item, &l.root_deps);
            for &(_, dep_l) in &l.locals {
                if let_seen.contains(&dep_l) {
                    let_readers.entry(dep_l).or_default().push(item);
                }
            }
        }
        for &g_idx in &sub_guards {
            let g = &index.guards[g_idx];
            let item = WorkItem::Guard(g_idx);
            add_root_edges(item, &g.root_deps);
            for &(_, dep_l) in &g.locals {
                if let_seen.contains(&dep_l) {
                    let_readers.entry(dep_l).or_default().push(item);
                }
            }
            if let Some(parent) = g.parent {
                if guard_seen.contains(&parent) {
                    guard_readers.entry(parent).or_default().push(item);
                }
            }
        }
        drop(add_root_edges);
        let edge_count: usize = root_readers.values().map(Vec::len).sum::<usize>()
            + let_readers.values().map(Vec::len).sum::<usize>()
            + guard_readers.values().map(Vec::len).sum::<usize>();

        let mut cur: HashMap<&'a str, ValSet> =
            HashMap::with_capacity(sub_states.len() + sub_derives.len());
        for &s_idx in &sub_states {
            let key = root.states[s_idx].name.as_str();
            let mut init = ValSet::default();
            if root.persistence(key).is_some() {
                init.unknown = true;
            }
            cur.insert(key, init);
        }
        for &d_idx in &sub_derives {
            cur.insert(root.derives[d_idx].name.as_str(), ValSet::default());
        }
        let mut let_vals: HashMap<usize, ValSet> = HashMap::with_capacity(sub_lets.len());
        for &l_idx in &sub_lets {
            let init = match index.lets[l_idx].kind {
                LetKind::Param => ValSet::unknown(),
                LetKind::Expr(_) | LetKind::MatchSome(_) => ValSet::default(),
            };
            let_vals.insert(l_idx, init);
        }
        let mut guard_open: HashMap<usize, bool> = HashMap::with_capacity(sub_guards.len());
        for &g_idx in &sub_guards {
            guard_open.insert(g_idx, false);
        }

        let mut queue: VecDeque<WorkItem> = VecDeque::new();
        let mut queued: HashSet<WorkItem> = HashSet::new();
        let enqueue = |item: WorkItem, q: &mut VecDeque<WorkItem>, qs: &mut HashSet<WorkItem>| {
            if qs.insert(item) {
                q.push_back(item);
            }
        };
        for &s_idx in &sub_states {
            enqueue(WorkItem::StateInit(s_idx), &mut queue, &mut queued);
        }
        for &d_idx in &sub_derives {
            enqueue(WorkItem::Derive(d_idx), &mut queue, &mut queued);
        }
        for &l_idx in &sub_lets {
            enqueue(WorkItem::Let(l_idx), &mut queue, &mut queued);
        }
        for &g_idx in &sub_guards {
            enqueue(WorkItem::Guard(g_idx), &mut queue, &mut queued);
        }
        for &w_idx in &sub_writes {
            enqueue(WorkItem::Write(w_idx), &mut queue, &mut queued);
        }

        // Finite lattice height per node is <= MAX_ROLE_VALUES + 64 + 1, so a
        // budget scaled to nodes + edges always converges on valid graphs; on
        // exhaustion fail closed by marking alertdialog reachable.
        let total_nodes = sub_states.len()
            + sub_derives.len()
            + sub_writes.len()
            + sub_lets.len()
            + sub_guards.len();
        let worklist_budget = total_nodes
            .saturating_add(edge_count)
            .saturating_mul(128)
            .max(MIN_WORKLIST_STEPS);

        let mut steps = 0usize;
        let mut locals_buf: Vec<(&'a str, ValSet)> = Vec::new();
        while let Some(item) = queue.pop_front() {
            queued.remove(&item);
            steps += 1;
            if steps > worklist_budget {
                for val in cur.values_mut() {
                    val.unknown = true;
                    val.add(RoleVal::Str(ALERT_ROLE.into()));
                }
                for val in let_vals.values_mut() {
                    val.unknown = true;
                    val.add(RoleVal::Str(ALERT_ROLE.into()));
                }
                for open in guard_open.values_mut() {
                    *open = true;
                }
                break;
            }
            match item {
                WorkItem::StateInit(s_idx) => {
                    let s = &root.states[s_idx];
                    let key = s.name.as_str();
                    let val = eval_root_expr(&s.expr, &[], lookup_fn, &self.values, &cur);
                    let entry = cur.get_mut(key).expect("state in cur");
                    let mut changed = entry.union_with(val);
                    changed |= entry.mark_non_string_state();
                    if changed {
                        if let Some(deps) = root_readers.get(key) {
                            for &dep in deps {
                                enqueue(dep, &mut queue, &mut queued);
                            }
                        }
                    }
                }
                WorkItem::Derive(d_idx) => {
                    let d = &root.derives[d_idx];
                    let key = d.name.as_str();
                    let val = eval_root_expr(&d.expr, &[], lookup_fn, &self.values, &cur);
                    let entry = cur.get_mut(key).expect("derive in cur");
                    if entry.union_with(val) {
                        if let Some(deps) = root_readers.get(key) {
                            for &dep in deps {
                                enqueue(dep, &mut queue, &mut queued);
                            }
                        }
                    }
                }
                WorkItem::Let(l_idx) => {
                    let l = &index.lets[l_idx];
                    let val = match l.kind {
                        LetKind::Param => ValSet::unknown(),
                        LetKind::Expr(expr) => {
                            fill_locals(&l.locals, &self.let_vals, &let_vals, &mut locals_buf);
                            eval_root_expr(expr, &locals_buf, lookup_fn, &self.values, &cur)
                        }
                        LetKind::MatchSome(subject) => {
                            fill_locals(&l.locals, &self.let_vals, &let_vals, &mut locals_buf);
                            eval_root_expr(subject, &locals_buf, lookup_fn, &self.values, &cur)
                                .unwrap_some()
                        }
                    };
                    let entry = let_vals.get_mut(&l_idx).expect("let in let_vals");
                    if entry.union_with(val) {
                        if let Some(deps) = let_readers.get(&l_idx) {
                            for &dep in deps {
                                enqueue(dep, &mut queue, &mut queued);
                            }
                        }
                    }
                }
                WorkItem::Guard(g_idx) => {
                    if *guard_open.get(&g_idx).expect("guard in guard_open") {
                        continue;
                    }
                    let g = &index.guards[g_idx];
                    if g.parent.is_some_and(|p| {
                        !self
                            .guard_open
                            .get(&p)
                            .or_else(|| guard_open.get(&p))
                            .copied()
                            .unwrap_or(false)
                    }) {
                        continue;
                    }
                    fill_locals(&g.locals, &self.let_vals, &let_vals, &mut locals_buf);
                    let opens = match g.kind {
                        GuardKind::If { cond, expect_true } => {
                            let cv =
                                eval_root_expr(cond, &locals_buf, lookup_fn, &self.values, &cur);
                            if expect_true {
                                cv.may_be_true()
                            } else {
                                cv.may_be_false()
                            }
                        }
                        GuardKind::Match {
                            subject,
                            expect_some,
                        } => {
                            let sv =
                                eval_root_expr(subject, &locals_buf, lookup_fn, &self.values, &cur);
                            if expect_some {
                                sv.may_be_some()
                            } else {
                                sv.may_be_none()
                            }
                        }
                    };
                    if opens {
                        guard_open.insert(g_idx, true);
                        if let Some(deps) = guard_readers.get(&g_idx) {
                            for &dep in deps {
                                enqueue(dep, &mut queue, &mut queued);
                            }
                        }
                    }
                }
                WorkItem::Write(w_idx) => {
                    let w = &index.writes[w_idx];
                    if w.guard.is_some_and(|g| {
                        !self
                            .guard_open
                            .get(&g)
                            .or_else(|| guard_open.get(&g))
                            .copied()
                            .unwrap_or(false)
                    }) {
                        continue;
                    }
                    fill_locals(&w.locals, &self.let_vals, &let_vals, &mut locals_buf);
                    let value = eval_root_expr(w.expr, &locals_buf, lookup_fn, &self.values, &cur);
                    let entry = cur.get_mut(w.target).expect("target in cur");
                    let mut changed = entry.union_with(value);
                    changed |= entry.mark_non_string_state();
                    if changed {
                        if let Some(deps) = root_readers.get(w.target) {
                            for &dep in deps {
                                enqueue(dep, &mut queue, &mut queued);
                            }
                        }
                    }
                }
            }
        }

        // Any value that never received a concrete contribution (e.g. an
        // uninitialized pure self-cycle) falls back to unknown.
        for val in cur.values_mut() {
            if val.is_empty() {
                val.unknown = true;
            }
        }
        for val in let_vals.values_mut() {
            if val.is_empty() {
                val.unknown = true;
            }
        }
        self.values.extend(cur);
        self.let_vals.extend(let_vals);
        self.guard_open.extend(guard_open);
    }
}

fn fill_locals<'a>(
    bound: &[(&'a str, usize)],
    solved_lets: &HashMap<usize, ValSet>,
    let_vals: &HashMap<usize, ValSet>,
    out: &mut Vec<(&'a str, ValSet)>,
) {
    out.clear();
    for &(name, l_idx) in bound {
        out.push((
            name,
            solved_lets
                .get(&l_idx)
                .or_else(|| let_vals.get(&l_idx))
                .cloned()
                .unwrap_or_else(ValSet::unknown),
        ));
    }
}

fn eval_root_expr<'a>(
    expr: &'a Expr,
    locals: &[(&'a str, ValSet)],
    lookup_fn: &LookupFn<'a, '_>,
    solved: &HashMap<&'a str, ValSet>,
    cur: &HashMap<&'a str, ValSet>,
) -> ValSet {
    let mut ctx = ExprEval {
        lookup_fn,
        seen_fns: HashSet::new(),
        steps: 0,
        resolve_root: &mut |name: &str| {
            solved
                .get(name)
                .or_else(|| cur.get(name))
                .cloned()
                .unwrap_or_else(ValSet::unknown)
        },
        view_bindings: &[],
        view_cache: Vec::new(),
    };
    ctx.eval(expr, locals, 0, 0)
}

struct ExprEval<'a, 'b, 'c>
where
    'a: 'b,
{
    lookup_fn: &'c LookupFn<'a, 'c>,
    seen_fns: HashSet<&'b str>,
    steps: usize,
    resolve_root: &'c mut dyn FnMut(&str) -> ValSet,
    view_bindings: &'b [(&'b str, Option<&'b Expr>)],
    view_cache: Vec<Option<ValSet>>,
}

impl<'a, 'b, 'c> ExprEval<'a, 'b, 'c>
where
    'a: 'b,
{
    fn eval(
        &mut self,
        expr: &'b Expr,
        locals: &[(&'b str, ValSet)],
        view_len: usize,
        depth: usize,
    ) -> ValSet {
        self.steps += 1;
        if depth > MAX_EXPR_DEPTH || self.steps > MAX_EVAL_STEPS {
            // Fail closed on expression-budget exhaustion so truncated subtrees
            // never silently accept an invalid alertdialog role.
            return ValSet::top_alert();
        }
        match expr {
            Expr::Bool(b, _) => ValSet::one(RoleVal::Bool(*b)),
            // Keep literal strings exact regardless of length so distinct long
            // constants remain distinguishable in `==` and `!=`.
            Expr::Str(s, _) => ValSet::one(RoleVal::Str(s.clone())),
            Expr::None(_) => ValSet::one(RoleVal::None),
            Expr::Some(inner, _) => self.eval(inner, locals, view_len, depth + 1).wrap_some(),
            Expr::Typed(inner, _, _) => self.eval(inner, locals, view_len, depth + 1),
            Expr::Template(parts, _) => self.eval_template(parts, locals, view_len, depth + 1),
            Expr::Let {
                name, value, body, ..
            } => {
                let val = self.eval(value, locals, view_len, depth + 1);
                let mut next = locals.to_vec();
                next.push((name.as_str(), val));
                self.eval(body, &next, view_len, depth + 1)
            }
            Expr::Unary(UnOp::Not, inner, _) => {
                let vs = self.eval(inner, locals, view_len, depth + 1);
                if vs.is_empty() {
                    return ValSet::default();
                }
                let mut out = ValSet {
                    values: BTreeSet::new(),
                    non_preserved: 0,
                    unknown: vs.unknown,
                };
                for value in vs.values {
                    match value {
                        RoleVal::Bool(b) => out.add(RoleVal::Bool(!b)),
                        _ => out.unknown = true,
                    }
                }
                out
            }
            Expr::Binary(op, l, r, _) => match op {
                BinOp::And => {
                    let lv = self.eval(l, locals, view_len, depth + 1);
                    if lv.is_empty() {
                        return ValSet::default();
                    }
                    let mut out = ValSet::default();
                    if lv.may_be_false() {
                        out.add(RoleVal::Bool(false));
                    }
                    if lv.may_be_true() {
                        let rv = self.eval(r, locals, view_len, depth + 1);
                        if rv.may_be_false() {
                            out.add(RoleVal::Bool(false));
                        }
                        if rv.may_be_true() {
                            out.add(RoleVal::Bool(true));
                        }
                    }
                    out
                }
                BinOp::Or => {
                    let lv = self.eval(l, locals, view_len, depth + 1);
                    if lv.is_empty() {
                        return ValSet::default();
                    }
                    let mut out = ValSet::default();
                    if lv.may_be_true() {
                        out.add(RoleVal::Bool(true));
                    }
                    if lv.may_be_false() {
                        let rv = self.eval(r, locals, view_len, depth + 1);
                        if rv.may_be_true() {
                            out.add(RoleVal::Bool(true));
                        }
                        if rv.may_be_false() {
                            out.add(RoleVal::Bool(false));
                        }
                    }
                    out
                }
                BinOp::Eq | BinOp::Ne => {
                    let lv = self.eval(l, locals, view_len, depth + 1);
                    let rv = self.eval(r, locals, view_len, depth + 1);
                    if lv.is_empty() || rv.is_empty() {
                        return ValSet::default();
                    }
                    if lv.unknown || rv.unknown {
                        return ValSet::any_bool();
                    }
                    let mut out = ValSet::default();
                    for a in &lv.values {
                        for b in &rv.values {
                            match role_val_eq(a, b) {
                                Some(eq) => {
                                    out.add(RoleVal::Bool(if *op == BinOp::Eq { eq } else { !eq }));
                                }
                                None => return ValSet::any_bool(),
                            }
                        }
                    }
                    out
                }
                _ => ValSet::unknown(),
            },
            Expr::Ternary(cond, yes, no, _) => {
                let cv = self.eval(cond, locals, view_len, depth + 1);
                if cv.is_empty() {
                    return ValSet::default();
                }
                let mut out = ValSet::default();
                if cv.may_be_true() {
                    out.union_with(self.eval(yes, locals, view_len, depth + 1));
                }
                if cv.may_be_false() {
                    out.union_with(self.eval(no, locals, view_len, depth + 1));
                }
                out
            }
            Expr::Match {
                subject,
                var,
                some,
                none,
                ..
            } => {
                let sv = self.eval(subject, locals, view_len, depth + 1);
                if sv.is_empty() {
                    return ValSet::default();
                }
                let mut out = ValSet::default();
                if sv.may_be_some() {
                    let mut next = locals.to_vec();
                    next.push((var.as_str(), sv.unwrap_some()));
                    out.union_with(self.eval(some, &next, view_len, depth + 1));
                }
                if sv.may_be_none() {
                    out.union_with(self.eval(none, locals, view_len, depth + 1));
                }
                out
            }
            Expr::Ident(name, _) => {
                if let Some((_, value)) = locals.iter().rev().find(|(n, _)| *n == name.as_str()) {
                    return value.clone();
                }
                if let Some(idx) = self.view_bindings[..view_len]
                    .iter()
                    .rposition(|(n, _)| *n == name.as_str())
                {
                    if let Some(cached) = &self.view_cache[idx] {
                        return cached.clone();
                    }
                    let Some(subject) = self.view_bindings[idx].1 else {
                        return ValSet::unknown();
                    };
                    // `view_len` strictly decreases to `idx`, so separate enclosing
                    // match subjects start at depth 0 without exhausting `MAX_EXPR_DEPTH`.
                    let value = self.eval(subject, &[], idx, 0).unwrap_some();
                    self.view_cache[idx] = Some(value.clone());
                    return value;
                }
                (self.resolve_root)(name.as_str())
            }
            Expr::Call(name, args, _) => {
                let Some((params, body)) = (self.lookup_fn)(name) else {
                    return ValSet::unknown();
                };
                if params.len() != args.len() {
                    return ValSet::unknown();
                }
                let arg_vals: Vec<ValSet> = args
                    .iter()
                    .map(|a| self.eval(a, locals, view_len, depth + 1))
                    .collect();
                if !self.seen_fns.insert(name.as_str()) {
                    return ValSet::unknown();
                }
                let fn_env: Vec<(&'b str, ValSet)> = params
                    .iter()
                    .zip(arg_vals)
                    .map(|(p, v)| (p.name.as_str(), v))
                    .collect();
                let out = self.eval(body, &fn_env, 0, depth + 1);
                self.seen_fns.remove(name.as_str());
                out
            }
            _ => ValSet::unknown(),
        }
    }

    fn eval_template(
        &mut self,
        parts: &'b [TemplatePart],
        locals: &[(&'b str, ValSet)],
        view_len: usize,
        depth: usize,
    ) -> ValSet {
        let mut single_expr: Option<&Expr> = None;
        let mut expr_count = 0usize;
        let mut has_text = false;
        for part in parts {
            match part {
                TemplatePart::Text(t) if !t.is_empty() => has_text = true,
                TemplatePart::Text(_) => {}
                TemplatePart::Expr(e) => {
                    expr_count += 1;
                    single_expr = Some(e);
                }
            }
        }
        if expr_count == 0 {
            // Preserve non-interpolated backtick literals exactly like `Expr::Str`.
            let mut literal = String::new();
            for part in parts {
                if let TemplatePart::Text(t) = part {
                    literal.push_str(t);
                }
            }
            return ValSet::one(RoleVal::Str(literal));
        }
        if expr_count == 1 && !has_text {
            return self.eval(single_expr.unwrap(), locals, view_len, depth);
        }
        let mut evaluated: Vec<(Vec<String>, bool)> = Vec::with_capacity(parts.len());
        let mut any_unknown = false;
        let mut has_constraint = has_text;
        for part in parts {
            match part {
                TemplatePart::Text(t) => {
                    evaluated.push((vec![t.clone()], false));
                }
                TemplatePart::Expr(e) => {
                    let vs = self.eval(e, locals, view_len, depth);
                    if vs.is_empty() {
                        return ValSet::default();
                    }
                    let mut strs = Vec::new();
                    let mut part_unknown = vs.unknown;
                    for value in vs.values {
                        match value {
                            RoleVal::Str(s) => {
                                if !s.is_empty() {
                                    has_constraint = true;
                                }
                                strs.push(s);
                            }
                            RoleVal::Bool(b) => {
                                has_constraint = true;
                                strs.push(if b { "true" } else { "false" }.into());
                            }
                            _ => part_unknown = true,
                        }
                    }
                    any_unknown |= part_unknown;
                    evaluated.push((strs, part_unknown));
                }
            }
        }
        let mut acc: BTreeSet<String> = BTreeSet::new();
        acc.insert(String::new());
        let mut overflow = false;
        for (strs, _) in &evaluated {
            if strs.is_empty() {
                acc.clear();
                break;
            }
            let mut next: BTreeSet<String> = BTreeSet::new();
            let mut non_preserved = 0usize;
            for prefix in &acc {
                for suffix in strs {
                    let total = prefix.len().saturating_add(suffix.len());
                    if total > MAX_STR_LEN {
                        overflow = true;
                        continue;
                    }
                    let mut combined = String::with_capacity(total);
                    combined.push_str(prefix);
                    combined.push_str(suffix);
                    if is_alert_fragment(&combined) {
                        next.insert(combined);
                    } else if non_preserved < MAX_ROLE_VALUES {
                        if next.insert(combined) {
                            non_preserved += 1;
                        }
                    } else {
                        overflow = true;
                    }
                }
            }
            acc = next;
            if acc.is_empty() {
                break;
            }
        }
        let mut out = ValSet {
            values: BTreeSet::new(),
            non_preserved: 0,
            unknown: any_unknown || overflow,
        };
        for s in acc {
            out.add(RoleVal::Str(s));
        }
        if out.unknown
            && has_constraint
            && !out
                .values
                .iter()
                .any(|v| matches!(v, RoleVal::Str(s) if s == ALERT_ROLE))
            && template_parts_may_match_alertdialog(&evaluated)
        {
            out.add(RoleVal::Str(ALERT_ROLE.into()));
        }
        out
    }
}

/// Whether `attrs`'s `role` can evaluate to `"alertdialog"`, resolving literal
/// branches, enclosing view bindings, component prop substitutions, `derive`s,
/// `state` initializers and reachable assignments, and pure `fn` calls.
pub(crate) fn alertdialog_role<'a, 'b>(
    attrs: &'b [Attr],
    root: &'a Component,
    view_bindings: &'b [(&'b str, Option<&'b Expr>)],
    lookup_fn: &LookupFn<'a, '_>,
    cache: &mut RoleCache<'a>,
) -> AlertRole
where
    'a: 'b,
{
    let Some(role) = attrs.iter().rev().find(|a| a.name == "role") else {
        return AlertRole::Never;
    };
    if let Expr::Str(s, _) = &role.value {
        return if s == ALERT_ROLE {
            AlertRole::Always
        } else {
            AlertRole::Never
        };
    }
    let mut resolve_root = |name: &str| {
        cache.ensure_solved(root, lookup_fn, name);
        cache
            .values
            .get(name)
            .cloned()
            .unwrap_or_else(ValSet::unknown)
    };
    let mut eval = ExprEval {
        lookup_fn,
        seen_fns: HashSet::new(),
        steps: 0,
        resolve_root: &mut resolve_root,
        view_bindings,
        view_cache: vec![None; view_bindings.len()],
    };
    eval.eval(&role.value, &[], view_bindings.len(), 0)
        .to_alert_role()
}
