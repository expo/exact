//! Infer action interfaces once, then propagate requirements over bindings.
//! @ref LLP 1035.005 D2: invocation, declaration and binding in one diagnostic.

use super::{handler_arity, AnalyzeError, Related, HANDLERS};
use contract_syntax::{Component, Expr, File, Node, Span};
use contract_types::{Ref, Scope, Ty, Types};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

/// A non-action provider still shadows an outer action of the same name.
type Providers = HashMap<String, Option<Binding>>;

#[derive(Clone)]
struct Origin {
    invocation: Span,
    declaration: Span,
}

#[derive(Clone)]
struct Need {
    min: usize,
    max: usize,
    low: Origin,
    high: Origin,
    /// The event's optional record, the slot past `min` (`handler_accepts`).
    record: Option<&'static str>,
}
impl Need {
    fn shifted(&self, bound: usize) -> Self {
        Self {
            min: self.min + bound,
            max: self.max + bound,
            ..self.clone()
        }
    }
    /// The record a count of parameters ends with, when that count is
    /// only this invocation's because of its event's optional record.
    fn record_only(&self, n: usize) -> Option<&'static str> {
        self.record.filter(|_| n == self.max && self.min < self.max)
    }
    fn description(&self) -> String {
        match self.record {
            _ if self.min == self.max => self.min.to_string(),
            Some(record) => format!("{} (or {}, the last its `{record}`)", self.min, self.max),
            None => format!("{} or {}", self.min, self.max),
        }
    }
}
struct Interface {
    name: String,
    declaration: Option<Span>,
    need: Option<Need>,
}
#[derive(Clone)]
enum Target {
    Interface(usize),
    Action { name: String, params: Vec<Ty> },
}
#[derive(Clone)]
struct Binding {
    target: Target,
    bound: usize,
    span: Span,
}
#[derive(Clone)]
struct Edge {
    to: usize,
    bound: usize,
    span: Span,
}
struct Graph<'a> {
    file: &'a File,
    types: &'a Types,
    component_ids: BTreeMap<&'a str, usize>,
    // Explicit props followed by injects, matching Ref::Prop's indices.
    params: Vec<Vec<Option<usize>>>,
    // Ambient provider channels also cross components that do not inject them.
    contexts: Vec<BTreeMap<String, usize>>,
    interfaces: Vec<Interface>,
    edges: Vec<Vec<Edge>>,
    concrete: Vec<(usize, Binding)>,
    queue: VecDeque<usize>,
}

pub(super) fn check(file: &File, types: &Types, expanded: &Component) -> Result<(), AnalyzeError> {
    let mut graph = Graph {
        file,
        types,
        component_ids: file
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.as_str(), i))
            .collect(),
        params: Vec::new(),
        contexts: Vec::new(),
        interfaces: Vec::new(),
        edges: Vec::new(),
        concrete: Vec::new(),
        queue: VecDeque::new(),
    };
    let carried = carried(file, types, &graph.component_ids);
    for (ci, c) in file.components.iter().enumerate() {
        let mut contexts = BTreeMap::new();
        for name in &carried[ci] {
            let declared = c.injects.iter().find(|p| &p.name == name);
            let id = graph.interface(format!("{}.{}", c.name, name), declared.map(|p| p.span));
            contexts.insert(name.clone(), id);
        }
        let mut params = Vec::new();
        for (i, p) in c.props.iter().enumerate() {
            params.push(
                matches!(types.components[ci].props[i], Ty::Action(_))
                    .then(|| graph.interface(format!("{}.{}", c.name, p.name), Some(p.span))),
            );
        }
        for (i, p) in c.injects.iter().enumerate() {
            params.push(
                matches!(types.components[ci].props[c.props.len() + i], Ty::Action(_))
                    .then(|| contexts[&p.name]),
            );
        }
        graph.params.push(params);
        graph.contexts.push(contexts);
    }
    if graph.interfaces.is_empty() {
        return Ok(());
    }
    for (ci, c) in file.components.iter().enumerate() {
        let scoped = if ci == 0 { expanded } else { c };
        let mut scope = types.component_scope(scoped, &types.components[ci]);
        // The component's `provide` section covers its whole view (LLP
        // 1035.005.000 D9); a slot fill is walked in its caller's view.
        let providers = c
            .provides
            .iter()
            .map(|b| (b.name.clone(), graph.binding(ci, &b.expr, &scope, b.span)))
            .collect();
        graph.nodes(ci, &c.view, &mut scope, &providers)?;
    }
    while let Some(id) = graph.queue.pop_front() {
        let need = graph.interfaces[id]
            .need
            .clone()
            .expect("queued requirement");
        for edge in graph.edges[id].clone() {
            graph.require(edge.to, need.shifted(edge.bound), Some(edge.span))?;
        }
    }
    for (id, binding) in &graph.concrete {
        let Some(need) = &graph.interfaces[*id].need else {
            continue;
        };
        let Target::Action { name, params } = &binding.target else {
            unreachable!()
        };
        let arity = params.len();
        let remaining = arity.checked_sub(binding.bound);
        // The record's slot takes the record (`handler_accepts`).
        let record = |n: usize| match need.record {
            Some(record) if n == need.max && need.min < need.max => {
                matches!(params.last(), Some(Ty::Record(r)) if r == record)
                    || matches!(params.last(), Some(Ty::Unknown))
            }
            _ => true,
        };
        if remaining.is_some_and(|n| (need.min..=need.max).contains(&n) && record(n)) {
            continue;
        }
        let origin = if remaining.is_some_and(|n| n < need.min) {
            &need.low
        } else {
            &need.high
        };
        let mut error = refusal(
            format!("`{}` requires {} action parameter(s), but this binding to `{name}` supplies {} of its {arity}",
                graph.interfaces[*id].name, need.description(), binding.bound),
            origin.invocation,
            origin.declaration,
            binding.span,
            remaining.map_or_else(
                || format!("binding to `{name}` supplies more arguments than it accepts"),
                |n| format!("bound to `{name}` here; {n} parameter(s) remain")),
        );
        if let Some(span) = graph.interfaces[*id].declaration {
            if span != origin.declaration {
                error.related.push(Related {
                    span,
                    note: "forwarded action interface declared here".into(),
                });
            }
        }
        return Err(error);
    }
    Ok(())
}

/// Only channels reachable from an action inject cross a component.
fn carried(
    file: &File,
    types: &Types,
    component_ids: &BTreeMap<&str, usize>,
) -> Vec<BTreeSet<String>> {
    fn used(nodes: &[Node], component_ids: &BTreeMap<&str, usize>, out: &mut BTreeSet<usize>) {
        for node in Node::walk(nodes) {
            if let Node::Use { name, .. } = node {
                if let Some(&target) = component_ids.get(name.as_str()) {
                    out.insert(target);
                }
            }
        }
    }
    let uses: Vec<BTreeSet<usize>> = (file.components.iter())
        .map(|c| {
            let mut out = BTreeSet::new();
            used(&c.view, component_ids, &mut out);
            out
        })
        .collect();
    let provided: Vec<BTreeSet<&str>> = (file.components.iter())
        .map(|c| c.provides.iter().map(|b| b.name.as_str()).collect())
        .collect();
    let mut carried: Vec<BTreeSet<String>> = (file.components.iter().enumerate())
        .map(|(ci, c)| {
            (c.injects.iter().enumerate())
                .filter(|(i, _)| {
                    matches!(types.components[ci].props[c.props.len() + i], Ty::Action(_))
                })
                .map(|(_, p)| p.name.clone())
                .collect()
        })
        .collect();
    let mut parents = vec![Vec::new(); carried.len()];
    for (parent, children) in uses.iter().enumerate() {
        for &child in children {
            parents[child].push(parent);
        }
    }
    let mut queue: VecDeque<_> = carried
        .iter()
        .enumerate()
        .flat_map(|(ci, names)| names.iter().map(move |name| (ci, name.clone())))
        .collect();
    while let Some((child, name)) = queue.pop_front() {
        for &parent in &parents[child] {
            if !provided[parent].contains(name.as_str()) && carried[parent].insert(name.clone()) {
                queue.push_back((parent, name.clone()));
            }
        }
    }
    carried
}

fn refusal(
    message: String,
    span: Span,
    declaration: Span,
    other: Span,
    note: String,
) -> AnalyzeError {
    AnalyzeError {
        id: "analyze-action-arity",
        message,
        span,
        related: vec![
            Related {
                span: declaration,
                note: "action interface declared here".into(),
            },
            Related { span: other, note },
        ],
    }
}

impl Graph<'_> {
    fn interface(&mut self, name: String, declaration: Option<Span>) -> usize {
        let id = self.interfaces.len();
        self.interfaces.push(Interface {
            name,
            declaration,
            need: None,
        });
        self.edges.push(Vec::new());
        id
    }

    fn require(
        &mut self,
        id: usize,
        need: Need,
        binding: Option<Span>,
    ) -> Result<(), AnalyzeError> {
        let entry = &mut self.interfaces[id];
        if let Some(old) = &mut entry.need {
            // A count one invocation fills with its event's record and the
            // other with an argument or payload serves neither: narrow both
            // ranges to the counts they agree on (`Need::record_only`).
            let agreed: Vec<usize> = (need.min.max(old.min)..=need.max.min(old.max))
                .filter(|&n| need.record_only(n) == old.record_only(n))
                .collect();
            if need.min > old.max || old.min > need.max || agreed.is_empty() {
                let (incoming, previous) = if need.min > old.max {
                    (&need.low, &old.high)
                } else {
                    (&need.high, &old.low)
                };
                let mut error = refusal(
                    format!(
                        "`{}` has incompatible action requirements: {} and {} parameter(s)",
                        entry.name,
                        old.description(),
                        need.description()
                    ),
                    incoming.invocation,
                    entry.declaration.unwrap_or(incoming.declaration),
                    previous.invocation,
                    "incompatible invocation required here".into(),
                );
                if let Some(span) = binding {
                    error.related.push(Related {
                        span,
                        note: "forwarded through this binding".into(),
                    });
                }
                return Err(error);
            }
            let need = Need {
                min: agreed[0],
                max: agreed[agreed.len() - 1],
                ..need
            };
            if need.min <= old.min && need.max >= old.max {
                return Ok(());
            }
            if need.min > old.min {
                old.min = need.min;
                old.low = need.low;
            }
            if need.max < old.max {
                // Below its record's count, the record no longer ends it.
                if old.record_only(old.max).is_some() {
                    old.record = need.record;
                }
                old.max = need.max;
                old.high = need.high;
            }
        } else {
            entry.need = Some(need);
        }
        self.queue.push_back(id);
        Ok(())
    }

    fn binding(&self, ci: usize, expr: &Expr, scope: &Scope, span: Span) -> Option<Binding> {
        let (name, bound) = match expr {
            Expr::Ident(name, _) => (name, 0),
            Expr::Call(name, args, _) => {
                // Expression calls use the function namespace before actions.
                if self.types.shapes.fns.contains_key(name) {
                    return None;
                }
                (name, args.len())
            }
            _ => return None,
        };
        let (reference, ty) = scope.lookup(name)?;
        let Ty::Action(params) = ty else { return None };
        let target = match reference {
            Ref::Action(_) => Target::Action {
                name: name.clone(),
                params: params.clone(),
            },
            Ref::Prop(pi) => Target::Interface(self.params[ci][pi as usize]?),
            _ => return None,
        };
        Some(Binding {
            target,
            bound,
            span,
        })
    }

    fn connect(&mut self, id: usize, binding: Binding) {
        match binding.target {
            Target::Interface(to) => self.edges[id].push(Edge {
                to,
                bound: binding.bound,
                span: binding.span,
            }),
            Target::Action { .. } => self.concrete.push((id, binding)),
        }
    }

    fn nodes(
        &mut self,
        ci: usize,
        nodes: &[Node],
        scope: &mut Scope,
        providers: &Providers,
    ) -> Result<(), AnalyzeError> {
        for node in nodes {
            match node {
                Node::Element {
                    tag,
                    attrs,
                    children,
                    ..
                } => {
                    let control = contract_syntax::payload_control(tag, attrs);
                    for attr in attrs {
                        if !HANDLERS.contains(&attr.name.as_str()) {
                            continue;
                        }
                        let event = contract_syntax::typed_event(&attr.name, control);
                        let (name, given) = match &attr.value {
                            Expr::Ident(name, _) => (name, 0),
                            Expr::Call(name, args, _) => (name, args.len()),
                            _ => continue,
                        };
                        let Some((Ref::Prop(pi), Ty::Action(_))) = scope.lookup(name) else {
                            continue;
                        };
                        let Some(id) = self.params[ci][pi as usize] else {
                            continue;
                        };
                        let declaration = self.interfaces[id]
                            .declaration
                            .expect("authored action parameter");
                        let Some(range) = handler_arity(event, given) else {
                            return Err(refusal(
                                format!(
                                    "`{}=` does not accept explicitly bound arguments",
                                    attr.name
                                ),
                                attr.span,
                                declaration,
                                attr.value.span(),
                                "remove the explicit arguments here".into(),
                            ));
                        };
                        let origin = Origin {
                            invocation: attr.value.span(),
                            declaration,
                        };
                        self.require(
                            id,
                            Need {
                                min: *range.start(),
                                max: *range.end(),
                                record: contract_types::event_record(event),
                                low: origin.clone(),
                                high: origin,
                            },
                            None,
                        )?;
                    }
                    self.nodes(ci, children, scope, providers)?;
                }
                Node::Use {
                    name,
                    args,
                    children,
                    ..
                } => {
                    // Slot fills are expanded in the caller's lexical/provider scope.
                    self.nodes(ci, children, scope, providers)?;
                    let target = self.component_ids[name.as_str()];
                    for (i, p) in self.file.components[target].props.iter().enumerate() {
                        let Some(id) = self.params[target][i] else {
                            continue;
                        };
                        let Some(arg) = args.iter().find(|arg| arg.name == p.name) else {
                            continue;
                        };
                        if let Some(binding) = self.binding(ci, &arg.value, scope, arg.span) {
                            self.connect(id, binding);
                        }
                    }
                    for (name, id) in self.contexts[target].clone() {
                        match providers.get(&name) {
                            Some(Some(binding)) => self.connect(id, binding.clone()),
                            Some(None) => {} // A non-action provider still shadows an outer one.
                            None => self.edges[id].push(Edge {
                                to: self.contexts[ci][&name],
                                bound: 0,
                                span: node.span(),
                            }),
                        }
                    }
                }
                Node::Children { .. } => {}
                Node::When {
                    then, otherwise, ..
                } => {
                    self.nodes(ci, then, scope, providers)?;
                    self.nodes(ci, otherwise, scope, providers)?;
                }
                Node::Each {
                    var, index, body, ..
                } => {
                    scope.push_each(var, index.as_deref(), Ty::Unknown);
                    self.nodes(ci, body, scope, providers)?;
                    scope.pop();
                }
                Node::Match { some, none, .. } => {
                    scope.push_region(Some((some.0.clone(), Ref::Bound(0), Ty::Unknown)));
                    self.nodes(ci, &some.1, scope, providers)?;
                    scope.pop();
                    self.nodes(ci, none, scope, providers)?;
                }
            }
        }
        Ok(())
    }
}
