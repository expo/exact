//! Compiler-owned navigation and refusal hints over the authored source.
//! @ref LLP 1035.005 D2 — JSON queries before an editor protocol; LLP 1006 §3 — diagnostics.

use crate::{
    sources::{self, Sources},
    CompileError, RelatedLocation,
};
use contract_syntax::scope::Kind;
use contract_syntax::*;
use contract_types::{Ref, Scope, Ty, Types};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
};

mod partial;

/// A component's members, in the namespace its expressions read.
const MEMBERS: [&str; 7] = [
    "prop", "inject", "state", "derive", "resource", "mutation", "action",
];

struct Definition {
    kind: &'static str,
    name: String,
    span: Span,
    component: Option<String>,
    owner: Option<String>,
    /// An action's inferred effects, in its component's slot order
    /// (LLP 1035.005.000 D1): what `writes` used to restate.
    writes: Option<Vec<String>>,
}
struct Reference {
    span: Span,
    to: usize,
}
type Names = BTreeMap<String, usize>;
type Owners = BTreeMap<String, Names>;
#[derive(Default)]
struct Graph {
    definitions: Vec<Definition>,
    references: Vec<Reference>,
    // Nested namespace keys let queries borrow names instead of allocating
    // temporary component/owner/name Strings for every lookup.
    index: BTreeMap<&'static str, BTreeMap<String, Owners>>,
    ids: BTreeMap<String, Vec<usize>>,
    /// Definitions that share their namespace and name with another, which
    /// only a refused program has: no reference to one is resolved.
    ambiguous: BTreeSet<usize>,
    /// What the syntax refused: a reference spelled as one in its scope may
    /// mean it, so it gets no edge.
    hidden: Vec<partial::Shadow>,
}
impl Graph {
    fn define(
        &mut self,
        kind: &'static str,
        name: &str,
        span: Span,
        component: Option<&str>,
        owner: Option<&str>,
    ) -> usize {
        let index = self.definitions.len();
        // Locals use the lexical stack; IDs use the multi-target index below.
        // testId/provide are declarations only. All stay in navigation output,
        // without unused entries in the named-declaration lookup index.
        if !matches!(kind, "local" | "parameter" | "id" | "testId" | "provide") {
            let names = (self.index.entry(kind).or_default())
                .entry(component.unwrap_or("").into())
                .or_default()
                .entry(owner.unwrap_or("").into())
                .or_default();
            if let Some(first) = names.insert(name.into(), index) {
                self.ambiguous.extend([first, index]);
            }
        }
        self.definitions.push(Definition {
            kind,
            name: name.into(),
            span,
            component: component.map(str::to_owned),
            owner: owner.map(str::to_owned),
            writes: None,
        });
        if kind == "id" {
            self.ids.entry(name.into()).or_default().push(index);
        }
        index
    }
    fn find(
        &self,
        kind: &'static str,
        name: &str,
        component: Option<&str>,
        owner: Option<&str>,
    ) -> Option<usize> {
        self.unique(&[kind], name, component, owner)
    }
    /// The one definition of `name` among `kinds`, or none when there is
    /// none or more than one.
    fn unique(
        &self,
        kinds: &[&'static str],
        name: &str,
        component: Option<&str>,
        owner: Option<&str>,
    ) -> Option<usize> {
        let mut found = kinds.iter().filter_map(|kind| {
            self.index
                .get(kind)?
                .get(component.unwrap_or(""))?
                .get(owner.unwrap_or(""))?
                .get(name)
                .copied()
        });
        let first = found.next()?;
        (found.next().is_none() && !self.ambiguous.contains(&first)).then_some(first)
    }
    fn refer(&mut self, span: Span, to: usize) {
        let target = &self.definitions[to];
        if span != target.span && !self.hidden.iter().any(|h| h.hides(span, target)) {
            self.references.push(Reference { span, to });
        }
    }
    fn source(&mut self, name: &str, span: Span) {
        let to = self
            .find("source", name, None, None)
            .unwrap_or_else(|| self.define("source", name, span, None, None));
        self.refer(span, to);
    }
    fn id(&mut self, name: &str, span: Span) {
        // An authored ID can appear in more than one component. Report all
        // matching declarations rather than inventing a unique runtime target.
        if let Some(ids) = self.ids.get(name).cloned() {
            for to in ids {
                self.refer(span, to);
            }
        }
    }
    fn json(&self, sources: &Sources, name: Option<&str>, errors: &[CompileError]) -> String {
        // Emit the existing graph directly, without cloning every name and
        // filename into a second tree of JSON objects.
        let mut out = Vec::with_capacity(if name.is_some() {
            0
        } else {
            (self.definitions.len() + self.references.len()) * 128
        });
        let mut selected = name.map(|_| vec![None; self.definitions.len()]);
        out.extend_from_slice(b"{\"definitions\":[");
        let mut count = 0;
        for (i, definition) in self.definitions.iter().enumerate() {
            if name.is_some_and(|name| definition.name != name) {
                continue;
            }
            if let Some(selected) = &mut selected {
                selected[i] = Some(count);
            }
            if count != 0 {
                out.push(b',');
            }
            write_symbol(&mut out, sources, definition, definition.span, None);
            count += 1;
        }
        out.extend_from_slice(b"],\"references\":[");
        count = 0;
        for reference in &self.references {
            let to = match &selected {
                Some(selected) => match selected[reference.to] {
                    Some(to) => to,
                    None => continue,
                },
                None => reference.to,
            };
            if count != 0 {
                out.push(b',');
            }
            write_symbol(
                &mut out,
                sources,
                &self.definitions[reference.to],
                reference.span,
                Some(to),
            );
            count += 1;
        }
        out.push(b']');
        close(out, errors)
    }
}

/// The JSON object begun in `out`, with `errors` when there are any.
fn close(mut out: Vec<u8>, errors: &[CompileError]) -> String {
    if !errors.is_empty() {
        out.extend_from_slice(b",\"errors\":[");
        for (i, error) in errors.iter().enumerate() {
            if i != 0 {
                out.push(b',');
            }
            out.extend_from_slice(error.to_json().as_bytes());
        }
        out.push(b']');
    }
    out.push(b'}');
    String::from_utf8(out).expect("JSON serialization is UTF-8")
}

fn write_symbol(
    out: &mut Vec<u8>,
    sources: &Sources,
    definition: &Definition,
    span: Span,
    to: Option<usize>,
) {
    // Keep the previous sorted property order as well as its optional fields.
    write!(out, "{{\"col\":{}", span.col).unwrap();
    if to.is_none() {
        if let Some(component) = &definition.component {
            out.extend_from_slice(b",\"component\":");
            quote(out, component);
        }
    }
    write!(out, ",\"end_col\":{},\"file\":", span.end_col).unwrap();
    quote(out, &sources.path(span).to_string_lossy());
    out.extend_from_slice(b",\"kind\":");
    quote(out, definition.kind);
    write!(out, ",\"line\":{},\"name\":", span.line).unwrap();
    quote(out, &definition.name);
    if let Some(to) = to {
        write!(out, ",\"to\":{to}").unwrap();
    } else if let Some(owner) = &definition.owner {
        out.extend_from_slice(b",\"owner\":");
        quote(out, owner);
    }
    if let (None, Some(writes)) = (to, &definition.writes) {
        out.extend_from_slice(b",\"writes\":[");
        for (i, name) in writes.iter().enumerate() {
            if i != 0 {
                out.push(b',');
            }
            quote(out, name);
        }
        out.push(b']');
    }
    out.push(b'}');
}

fn quote(out: &mut Vec<u8>, value: &str) {
    serde_json::to_writer(out, value).expect("writing JSON into a Vec cannot fail");
}

/// What [`symbols_json`] found in a program it refused.
#[derive(Debug)]
pub struct PartialSymbols {
    /// The JSON, with `errors` after `references`.
    pub json: String,
    /// The refusals of loading, the syntax and the type check, in `build`'s
    /// order.
    pub errors: Vec<CompileError>,
}

/// Definitions and references as JSON, with exact original byte ranges.
/// Uses the build's import policy and type checker. No plan or bake is produced.
/// Local bindings, parameters and shape fields are included; built-in names have
/// no authored definition. Repeated ID declarations produce multiple edges.
/// `name` selects every exact matching declaration and its references, with `to`
/// indices local to that response. Validation still covers the whole source graph.
///
/// A refused program still gives the symbols of what parsed, and no action
/// carries `writes`. A declaration the syntax refused is absent, as is every
/// reference in its scope spelled as its name; so is a reference to a name
/// declared twice in one namespace. When the type check refused, a reference
/// that needs an inferred type (a field read through a value, an action
/// value called) has no edge, nor does one to a shape or function the check
/// refused. When loading stops, each file that parsed is read on its own.
pub fn symbols_json(path: &Path, name: Option<&str>) -> Result<String, PartialSymbols> {
    let nothing = |errors: Vec<CompileError>| PartialSymbols {
        json: close(b"{\"definitions\":[],\"references\":[]".to_vec(), &errors),
        errors,
    };
    let unreadable = |e: std::io::Error| {
        nothing(vec![CompileError {
            pass: "use",
            id: "contract-use-unreadable".into(),
            message: e.to_string(),
            span: Span::default(),
            file: Some(path.into()),
            related: Box::new([]),
        }])
    };
    // The text `with_contract_texts` holds for the file (an editor's unsaved
    // buffer, `symbols --stdin`), else the file's.
    let src = sources::contract_text(path)
        .map_or_else(|| std::fs::read_to_string(path), Ok)
        .map_err(unreadable)?;
    let root = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(unreadable)?;
    // As the build does, the passes after the syntax check what parsed.
    // Refused tables are reported by the check; recovery reads without them.
    let strings = crate::strings::load(&root, path);
    let tables = strings.as_ref().ok().cloned().flatten();
    let mut refused = sources::Refused::default();
    let (file, sources) = match sources::load_partial(path, &src, &root, &mut refused) {
        Ok(loaded) => loaded,
        Err(all) => {
            let errors = refused.join::<()>(Err(all)).unwrap_err();
            let sources = sources::read_all(path, &src, &root);
            let json = partial::file_by_file(&sources, &tables).json(&sources, name, &errors);
            return Err(PartialSymbols { json, errors });
        }
    };
    let checked = strings.and_then(|strings| {
        if file.components.is_empty() {
            contract_types::check_declarations(&file, strings)
                .map(|shapes| (shapes, Vec::new(), None))
                .map_err(|e| vec![sources.resolve(e.into())])
        } else {
            contract_types::check_all(&file, false, contract_lower::tags::style, strings)
                .map(|c| (c.types.shapes, c.types.components, Some(c.expanded)))
                .map_err(|all| {
                    (all.into_iter().take(crate::MAX_DIAGNOSTICS))
                        .map(|e| sources.resolve(authored_action_hint(&file, e.into())))
                        .collect()
                })
        }
    });
    let typed = checked.is_ok();
    let ((shapes, components, expanded), errors) = match checked {
        Ok(checked) => (checked, refused.join(Ok(())).err().unwrap_or_default()),
        // A refused check's inferences may be wrong where it refused; its
        // declarations' types are checked before anything is inferred.
        Err(all) => (
            (partial::declared(&file, &tables), Vec::new(), None),
            refused.join::<()>(Err(all)).unwrap_err(),
        ),
    };
    let types = Types { shapes, components };
    let mut r = Resolver {
        file: &file,
        types: &types,
        typed,
        whole: errors.is_empty(),
        graph: Graph {
            hidden: partial::shadows(&file, &refused.hidden),
            ..Graph::default()
        },
        members: None,
        component: None,
        scope: Scope::default(),
        locals: Vec::new(),
        owner: None,
    };
    r.declarations();
    for import in &sources.imports {
        let kinds: &[&str] = match import.kind {
            Kind::Component => &["component"],
            Kind::Call => &["shape", "fn"],
            Kind::Style => &["style"],
            Kind::Decoration => &["decoration"],
            Kind::Atom => &["atom"],
            Kind::Keyframes => &["keyframes"],
            Kind::Timeline => &[],
        };
        if let Some(to) = r.graph.unique(kinds, &import.name, None, None) {
            r.graph.refer(import.span, to);
        }
    }
    r.file(expanded.as_ref().map(|e| &e.root));
    let json = r.graph.json(&sources, name, &errors);
    match errors.is_empty() {
        true => Ok(json),
        false => Err(PartialSymbols { json, errors }),
    }
}

struct Resolver<'a> {
    file: &'a File,
    types: &'a Types,
    /// Whether the type check accepted the program: `types` holds only its
    /// declarations' otherwise.
    typed: bool,
    /// Whether nothing was refused.
    whole: bool,
    /// The component that is a members file's block, read on its own.
    members: Option<usize>,
    graph: Graph,
    component: Option<&'a Component>,
    scope: Scope,
    locals: Vec<(String, usize)>,
    owner: Option<String>,
}
impl<'a> Resolver<'a> {
    fn declarations(&mut self) {
        let names = &self.file.names;
        // An action writes what the actions it calls write (LLP 1089 D5):
        // its effects are read with its component's calls expanded. A prop
        // or inject call writes another component's slots, which no slot
        // of this one is.
        let (called, _) = contract_syntax::inline::calls::expand_file(self.file);
        for font in &self.file.fonts {
            self.graph
                .define("font", &font.name, names.name(font.span), None, None);
        }
        for shape in &self.file.shapes {
            self.graph
                .define("shape", &shape.name, names.name(shape.span), None, None);
            for field in &shape.fields {
                self.graph
                    .define("field", &field.name, field.span, None, Some(&shape.name));
            }
        }
        for style in &self.file.styles {
            self.graph
                .define("style", &style.name, names.name(style.span), None, None);
        }
        for decoration in &self.file.decorations {
            self.graph.define(
                "decoration",
                &decoration.name,
                names.name(decoration.span),
                None,
                None,
            );
        }
        for atom in &self.file.atoms {
            self.graph
                .define("atom", &atom.name, names.name(atom.span), None, None);
        }
        for keyframes in &self.file.keyframes {
            self.graph.define(
                "keyframes",
                &keyframes.name,
                names.name(keyframes.span),
                None,
                None,
            );
        }
        for f in &self.file.fns {
            self.graph
                .define("fn", &f.name, names.name(f.span), None, None);
        }
        for (ci, c) in self.file.components.iter().enumerate() {
            if self.members != Some(ci) {
                self.graph
                    .define("component", &c.name, names.name(c.span), None, None);
            }
            let cn = Some(c.name.as_str());
            for (kind, params) in [("prop", &c.props), ("inject", &c.injects)] {
                for p in params {
                    self.graph.define(kind, &p.name, p.span, cn, None);
                }
            }
            for (kind, bindings) in [("state", &c.states), ("derive", &c.derives)] {
                for b in bindings {
                    self.graph
                        .define(kind, &b.name, names.name(b.span), cn, None);
                }
            }
            for resource in &c.resources {
                self.graph.define(
                    "resource",
                    &resource.name,
                    names.name(resource.span),
                    cn,
                    None,
                );
            }
            for mutation in &c.mutations {
                self.graph.define(
                    "mutation",
                    &mutation.name,
                    names.name(mutation.span),
                    cn,
                    None,
                );
            }
            let router = self.file.routes.as_ref().filter(|_| ci == 0);
            let slots: Vec<&str> = router
                .map(|r| r.slot.as_str())
                .into_iter()
                .chain(c.states.iter().map(|s| s.name.as_str()))
                .chain(c.mutations.iter().map(|m| m.name.as_str()))
                .collect();
            for (ai, action) in c.actions.iter().enumerate() {
                let at =
                    self.graph
                        .define("action", &action.name, names.name(action.span), cn, None);
                // What a refused line wrote is not among its effects.
                if !self.whole {
                    continue;
                }
                let effects = called.components[ci].actions[ai].effects();
                let writes = slots
                    .iter()
                    .filter(|slot| effects.iter().any(|e| e.target == **slot))
                    .map(|slot| (*slot).to_owned())
                    .collect();
                self.graph.definitions[at].writes = Some(writes);
            }
            for task in &c.tasks {
                self.graph
                    .define("task", &task.name, names.name(task.span), cn, None);
            }
            self.ids(&c.view, &c.name);
        }
        if let Some(routes) = &self.file.routes {
            // A file of routes and no component is refused, and still read.
            if let Some(first) = self.file.components.first() {
                let span = names.name(routes.span);
                (self.graph).define("state", &routes.slot, span, Some(&first.name), None);
            }
            for route in &routes.rows {
                self.graph
                    .define("route", &route.name, names.name(route.span), None, None);
            }
        }
    }
    fn ids(&mut self, nodes: &[Node], component: &str) {
        for node in nodes {
            match node {
                Node::Element {
                    attrs, children, ..
                } => {
                    for a in attrs {
                        if let ("id" | "testId", Expr::Str(id, span)) = (a.name.as_str(), &a.value)
                        {
                            self.graph.define(
                                if a.name == "id" { "id" } else { "testId" },
                                id,
                                *span,
                                Some(component),
                                None,
                            );
                        }
                    }
                    self.ids(children, component);
                }
                node => {
                    for list in node.child_lists() {
                        self.ids(list, component);
                    }
                }
            }
        }
    }
    fn refer(
        &mut self,
        kind: &'static str,
        name: &str,
        span: Span,
        component: Option<&str>,
        owner: Option<&str>,
    ) {
        if let Some(to) = self.graph.find(kind, name, component, owner) {
            self.graph.refer(span, to);
        }
    }
    fn name(&mut self, name: &str, span: Span) {
        if let Some((_, to)) = self.locals.iter().rev().find(|(n, _)| n == name) {
            self.graph.refer(span, *to);
            return;
        }
        self.target(&MEMBERS, name, span);
    }
    /// The member of `kinds` named `name`, when no other member of the
    /// component has the name.
    fn target(&mut self, kinds: &[&'static str], name: &str, span: Span) {
        if let Some(c) = self.component {
            if let Some(to) = self.graph.unique(&MEMBERS, name, Some(&c.name), None) {
                if kinds.contains(&self.graph.definitions[to].kind) {
                    self.graph.refer(span, to);
                }
            }
        }
    }
    /// A call's `kind` of callee, when no other shape or `fn` has the name.
    fn callee(&mut self, kind: &str, name: &str, span: Span) -> bool {
        let to = self.graph.unique(&["shape", "fn"], name, None, None);
        let found = to.filter(|&to| self.graph.definitions[to].kind == kind);
        if let Some(to) = found {
            self.graph.refer(span, to);
        }
        found.is_some()
    }
    fn local(&mut self, kind: &'static str, name: &str, span: Span, ty: Ty) {
        let to = self.graph.define(
            kind,
            name,
            span,
            self.component.map(|c| c.name.as_str()),
            self.owner.as_deref(),
        );
        self.locals.push((name.into(), to));
        self.scope.push(vec![(name.into(), Ref::Local(0), ty)]);
    }
    fn pop_local(&mut self) {
        self.locals.pop();
        self.scope.pop();
    }
    fn ty(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Named(name, span) => {
                if matches!(self.types.shapes.resolve(ty), Ok(Ty::Record(ref shape)) if shape == name)
                {
                    self.refer("shape", name, *span, None, None);
                }
            }
            TypeExpr::Enum(..) => {}
            TypeExpr::List(inner, _) | TypeExpr::Option(inner, _) => self.ty(inner),
        }
    }
    fn infer(&self, expr: &Expr) -> Ty {
        if !self.typed {
            return Ty::Unknown;
        }
        contract_types::infer(expr, &self.scope, &self.types.shapes).unwrap_or(Ty::Unknown)
    }
    fn file(&mut self, expanded_root: Option<&Component>) {
        for style in &self.file.styles {
            for attr in style.attrs.iter().filter(|a| a.name == "font-family") {
                self.attr(attr);
            }
        }
        for shape in &self.file.shapes {
            for field in &shape.fields {
                self.ty(&field.ty);
            }
        }
        for f in &self.file.fns {
            self.owner = Some(f.name.clone());
            for p in &f.params {
                // Unchecked, a parameter may lack a type or name no shape.
                let mut resolved = Ty::Unknown;
                if let Some(ty) = &p.ty {
                    self.ty(ty);
                    resolved = self.types.shapes.resolve(ty).unwrap_or(Ty::Unknown);
                }
                self.local("parameter", &p.name, p.span, resolved);
            }
            self.ty(&f.ret);
            self.expr(&f.body);
            for _ in &f.params {
                self.pop_local();
            }
            self.owner = None;
        }
        for (ci, c) in self.file.components.iter().enumerate() {
            self.component = Some(c);
            self.scope = match self.typed {
                true => self.types.component_scope(
                    if ci == 0 {
                        expanded_root.expect("component present")
                    } else {
                        c
                    },
                    &self.types.components[ci],
                ),
                false => Scope::default(),
            };
            for p in c.props.iter().chain(&c.injects) {
                if let Some(ty) = &p.ty {
                    self.ty(ty);
                }
            }
            for b in c.states.iter().chain(&c.derives) {
                self.expr(&b.expr);
            }
            for r in &c.resources {
                self.graph
                    .source(&r.source, self.file.names.sources[&r.span]);
                for arg in &r.args {
                    self.expr(arg);
                }
                // `empty(…)` is the compiler's constant, not a source
                // (LLP 1054.000.002 D2).
                if let Some(p) = r
                    .placeholder
                    .as_ref()
                    .filter(|p| p.source != contract_types::placeholder::EMPTY)
                {
                    self.graph
                        .source(&p.source, self.file.names.sources[&p.span]);
                    for arg in &p.args {
                        self.expr(arg);
                    }
                }
                self.ty(&r.shape);
            }
            for m in &c.mutations {
                self.ty(&m.shape);
                if let Some((name, span)) = &m.then {
                    self.name(name, self.file.names.name(*span));
                }
            }
            for (ai, a) in c.actions.iter().enumerate() {
                self.owner = Some(a.name.clone());
                for (pi, p) in a.params.iter().enumerate() {
                    if let Some(ty) = &p.ty {
                        self.ty(ty);
                    }
                    let ty = match self.typed {
                        true => self.types.components[ci].actions[ai][pi].clone(),
                        false => Ty::Unknown,
                    };
                    self.local("parameter", &p.name, p.span, ty);
                }
                // Geometry reads type-check only in an action (LLP 1051.000 D2).
                self.scope.enter_action();
                self.stmts(&a.body);
                self.scope.leave_action();
                for _ in &a.params {
                    self.pop_local();
                }
                self.owner = None;
            }
            for t in &c.tasks {
                self.expr(&t.timer.0);
                for e in t.gate.iter().chain(&t.key) {
                    self.expr(e);
                }
                self.name(&t.timer.1, self.file.names.name(t.timer.2));
            }
            // A provided name is a declaration; its value reads the
            // component's scope (a bare name reads the name it spells).
            for b in &c.provides {
                self.graph
                    .define("provide", &b.name, b.span, Some(c.name.as_str()), None);
                self.expr(&b.expr);
            }
            self.nodes(&c.view);
        }
        self.component = None;
    }
    fn stmts(&mut self, stmts: &[Stmt]) {
        let mut lets = 0;
        for stmt in stmts {
            match stmt {
                // A `let` reads as a local through the rest of its block
                // (LLP 1035.005.000 D2).
                Stmt::Let { name, expr, span } => {
                    self.expr(expr);
                    let ty = self.infer(expr);
                    self.local("local", name, *span, ty);
                    lets += 1;
                }
                Stmt::Assign { target, expr, span } => {
                    self.target(&["state", "mutation"], target, *span);
                    self.expr(expr);
                }
                Stmt::Command { name, args, span } => {
                    if let ("focus" | "blur", [Expr::Str(id, span)]) =
                        (name.as_str(), args.as_slice())
                    {
                        self.graph.id(id, *span);
                    }
                    // A call of an action, an action prop or an inject (LLP
                    // 1089 D1) refers to it; a host command keeps its name.
                    if !contract_syntax::HOST_COMMANDS.contains(&name.as_str()) {
                        self.name(name, self.file.names.name(*span));
                    }
                    for arg in args {
                        self.expr(arg);
                    }
                }
                // Expansion's, never an authored tree's: the callee, and the
                // caller's arguments. Its body is the callee's own.
                Stmt::Call {
                    action, args, span, ..
                } => {
                    self.name(action, self.file.names.name(*span));
                    for arg in args {
                        self.expr(arg);
                    }
                }
                Stmt::Send {
                    target,
                    source,
                    args,
                    span,
                } => {
                    self.target(&["mutation"], target, self.file.names.name(*span));
                    self.graph.source(source, self.file.names.sources[span]);
                    for arg in args {
                        self.expr(arg);
                    }
                }
                Stmt::Refresh { target, span } => {
                    self.target(&["resource"], target, self.file.names.name(*span))
                }
                Stmt::If {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    self.expr(cond);
                    self.stmts(then);
                    self.stmts(otherwise);
                }
                Stmt::Match {
                    subject,
                    some,
                    none,
                    span,
                } => {
                    self.expr(subject);
                    let ty = match self.infer(subject) {
                        Ty::Option(t) => *t,
                        _ => Ty::Unknown,
                    };
                    self.local("local", &some.0, self.file.names.name(*span), ty);
                    self.stmts(&some.1);
                    self.pop_local();
                    self.stmts(none);
                }
            }
        }
        for _ in 0..lets {
            self.pop_local();
        }
    }
    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Element {
                    positional,
                    attrs,
                    children,
                    ..
                } => {
                    for p in positional {
                        self.expr(p);
                    }
                    for a in attrs {
                        self.attr(a);
                    }
                    self.nodes(children);
                }
                Node::Use {
                    name,
                    args,
                    children,
                    span,
                } => {
                    self.refer("component", name, *span, None, None);
                    for a in args {
                        self.refer("prop", &a.name, a.span, Some(name), None);
                        self.expr(&a.value);
                    }
                    self.nodes(children);
                }
                Node::Children { .. } => {}
                Node::When {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    self.expr(cond);
                    self.nodes(then);
                    self.nodes(otherwise);
                }
                Node::Each {
                    var,
                    index,
                    list,
                    key,
                    body,
                    span,
                    ..
                } => {
                    self.expr(list);
                    let ty = match self.infer(list) {
                        Ty::List(t) => *t,
                        _ => Ty::Unknown,
                    };
                    self.local("local", var, self.file.names.name(*span), ty);
                    if let Some(index) = index {
                        self.local("local", index, self.file.names.name(*span), Ty::Number);
                    }
                    self.expr(key);
                    self.nodes(body);
                    if index.is_some() {
                        self.pop_local();
                    }
                    self.pop_local();
                }
                Node::Match {
                    subject,
                    some,
                    none,
                    span,
                    ..
                } => {
                    self.expr(subject);
                    let ty = match self.infer(subject) {
                        Ty::Option(t) => *t,
                        _ => Ty::Unknown,
                    };
                    self.local("local", &some.0, self.file.names.name(*span), ty);
                    self.nodes(&some.1);
                    self.pop_local();
                    self.nodes(none);
                }
            }
        }
    }
    fn attr(&mut self, a: &Attr) {
        match (a.name.as_str(), &a.value) {
            ("id" | "testId", Expr::Str(..)) => {}
            ("font-family", Expr::Str(name, span)) => self.refer("font", name, *span, None, None),
            ("class", Expr::Ident(name, span)) => self.refer("style", name, *span, None, None),
            ("class", Expr::Ternary(cond, yes, no, _)) => {
                self.expr(cond);
                for side in [yes, no] {
                    if let Expr::Ident(name, span) = &**side {
                        self.refer("style", name, *span, None, None);
                    }
                }
            }
            ("navigationBack" | "contextTarget" | "popovertarget", Expr::Str(id, span)) => {
                self.graph.id(id, *span)
            }
            (name, Expr::Call(action, args, span))
                if contract_analyze::HANDLERS.contains(&name) =>
            {
                self.name(action, *span);
                for arg in args {
                    self.expr(arg);
                }
            }
            _ => self.expr(&a.value),
        }
    }
    fn expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(name, span) => self.name(name, *span),
            Expr::Member(base, field, span) => {
                self.expr(base);
                if let Ty::Record(shape) = self.infer(base) {
                    self.refer("field", field, *span, None, Some(&shape));
                }
            }
            Expr::Call(name, args, span) => {
                if contract_types::records::is_record_call(name, &self.types.shapes) {
                    // `Shape(field=…)` (LLP 1035.005.000 D3): the shape and
                    // each field it names.
                    if self.callee("shape", name, *span) {
                        for arg in args {
                            if let Expr::NamedArg(field, _, at) = arg {
                                self.refer("field", field, *at, None, Some(name));
                            }
                        }
                    }
                } else if self.types.shapes.fns.contains_key(name) {
                    self.callee("fn", name, *span);
                } else if name == "path" {
                    if let Some(Expr::Str(route, span)) = args.first() {
                        self.refer("route", route, *span, None, None);
                    }
                } else if matches!(self.infer(expr), Ty::Action(_)) {
                    self.name(name, *span);
                }
                for (i, arg) in args.iter().enumerate() {
                    match arg {
                        // A `map`/`filter` callback's item and index (LLP 1017.003).
                        Expr::Arrow { params, body, span } if i == 1 => {
                            let item = match self.infer(&args[0]) {
                                Ty::List(t) => *t,
                                _ => Ty::Unknown,
                            };
                            for (p, t) in params.iter().zip([item, Ty::Number]) {
                                self.local("local", p, *span, t);
                            }
                            self.expr(body);
                            for _ in params.iter().take(2) {
                                self.pop_local();
                            }
                        }
                        _ => self.expr(arg),
                    }
                }
            }
            Expr::Match {
                subject,
                var,
                some,
                none,
                span,
            } => {
                self.expr(subject);
                let ty = match self.infer(subject) {
                    Ty::Option(t) => *t,
                    _ => Ty::Unknown,
                };
                self.local("local", var, self.file.names.name(*span), ty);
                self.expr(some);
                self.pop_local();
                self.expr(none);
            }
            // Only expansion writes a `let`, and an arrow outside
            // `map`/`filter` is the type pass's refusal: neither binds a
            // name in an authored tree.
            _ => {
                for operand in expr.operands() {
                    self.expr(operand);
                }
            }
        }
    }
}

// Expansion lifts child declarations into the root. Only the original tree
// can say which action spellings the author can use at a failing handler.
// Walk it on refusal only; successful compilation does no diagnostic work.
pub(crate) fn authored_action_hint(file: &File, mut error: CompileError) -> CompileError {
    // An ambiguous call (LLP 1089 D1): the declaration it collides with.
    if error.id == contract_syntax::inline::calls::Ambiguous::ID {
        let found = contract_syntax::inline::calls::ambiguous(file);
        if let Some(a) = found.into_iter().find(|a| a.span == error.span) {
            error.related = Box::new([RelatedLocation {
                span: a.declared,
                file: None,
                note: format!("the {} `{}` is declared here", a.what, a.name),
            }]);
        }
        return error;
    }
    if !matches!(
        error.id.as_str(),
        "type-unknown-name" | "type-unknown-function" | "analyze-unknown-action"
    ) {
        return error;
    }
    let refused = error.message.split('`').nth(1).unwrap_or("").to_owned();
    #[derive(Clone, Copy)]
    struct HintContext<'a> {
        refused: &'a str,
        call: bool,
        provider: bool,
    }
    let context = HintContext {
        refused: &refused,
        call: error.id == "type-unknown-function",
        provider: false,
    };
    fn action_type(file: &File, ty: &Option<TypeExpr>) -> bool {
        matches!(ty, Some(TypeExpr::Named(name, _)) if name == "action")
            && !file.shapes.iter().any(|shape| shape.name == "action")
    }
    fn suggestion<'a>(
        file: &File,
        c: &'a Component,
        name: &str,
        shadowed: &[&str],
        timer: bool,
        call: bool,
    ) -> Option<&'a str> {
        if !name.is_ascii() || !(3..=64).contains(&name.len()) {
            return None;
        }
        let names = c.actions.iter().map(|a| a.name.as_str()).chain(
            c.props
                .iter()
                .chain(&c.injects)
                .filter(|p| !timer && action_type(file, &p.ty))
                .map(|p| p.name.as_str()),
        );
        let mut found = None;
        for candidate in names {
            if shadowed.contains(&candidate)
                || (call
                    && (matches!(
                        candidate,
                        "pending" | "failed" | "failure" | "current" | "path"
                    ) || file.fns.iter().any(|f| f.name == candidate)))
                || !contract_syntax::one_spelling_edit(name.as_bytes(), candidate.as_bytes())
            {
                continue;
            }
            if found.is_some_and(|old| old != candidate) {
                return None;
            }
            found = Some(candidate);
        }
        found
    }
    fn view(
        file: &File,
        c: &Component,
        nodes: &[Node],
        span: Span,
        shadowed: &mut Vec<String>,
        context: HintContext<'_>,
    ) -> Option<(String, Option<String>)> {
        for node in nodes {
            let attrs = match node {
                Node::Element { attrs, .. } => Some((attrs, None)),
                Node::Use { name, args, .. } => file
                    .components
                    .iter()
                    .find(|target| target.name == *name)
                    .map(|target| (args, Some(target))),
                _ => None,
            };
            if let Some((attrs, target)) = attrs {
                for attr in attrs {
                    let is_action = match target {
                        Some(target) => target
                            .props
                            .iter()
                            .any(|p| p.name == attr.name && action_type(file, &p.ty)),
                        None => contract_analyze::HANDLERS.contains(&attr.name.as_str()),
                    };
                    if is_action && attr.value.span() == span {
                        if let Expr::Ident(name, _) | Expr::Call(name, _, _) = &attr.value {
                            if name != context.refused
                                || c.props.iter().chain(&c.injects).any(|p| p.name == *name)
                            {
                                continue;
                            }
                            let locals: Vec<_> = shadowed.iter().map(String::as_str).collect();
                            return Some((
                                name.clone(),
                                suggestion(file, c, name, &locals, false, context.call)
                                    .map(str::to_owned),
                            ));
                        }
                    }
                }
            }
            let found = match node {
                Node::Element { children, .. } | Node::Use { children, .. } => {
                    view(file, c, children, span, shadowed, context)
                }
                Node::Each {
                    var, index, body, ..
                } => {
                    let names = 1 + index.is_some() as usize;
                    shadowed.push(var.clone());
                    shadowed.extend(index.clone());
                    let found = view(file, c, body, span, shadowed, context);
                    shadowed.truncate(shadowed.len() - names);
                    found
                }
                Node::Match { some, none, .. } => {
                    shadowed.push(some.0.clone());
                    let found = view(file, c, &some.1, span, shadowed, context);
                    shadowed.pop();
                    found.or_else(|| view(file, c, none, span, shadowed, context))
                }
                Node::When {
                    then, otherwise, ..
                } => view(file, c, then, span, shadowed, context)
                    .or_else(|| view(file, c, otherwise, span, shadowed, context)),
                Node::Children { .. } => None,
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    // A provided value is written in the component's `provide` section,
    // where no view local shadows an action (LLP 1035.005.000 D9).
    fn provided(
        file: &File,
        c: &Component,
        span: Span,
        context: HintContext<'_>,
    ) -> Option<(String, Option<String>)> {
        let b = c.provides.iter().find(|b| b.expr.span() == span)?;
        (context.provider && expression_name(&b.expr) == Some(context.refused)).then(|| {
            (
                context.refused.to_owned(),
                suggestion(file, c, context.refused, &[], false, context.call).map(str::to_owned),
            )
        })
    }
    fn expression_name(expr: &Expr) -> Option<&str> {
        match expr {
            Expr::Ident(name, _) | Expr::Call(name, _, _) => Some(name),
            _ => None,
        }
    }
    fn authored_expr(c: &Component, span: Span) -> Option<&Expr> {
        if let Some(b) = c.provides.iter().find(|b| b.expr.span() == span) {
            return Some(&b.expr);
        }
        let mut found = None;
        Node::walk(&c.view).for_each(|node| {
            if let Node::Element { attrs, .. } | Node::Use { args: attrs, .. } = node {
                if let Some(a) = attrs.iter().find(|a| a.value.span() == span) {
                    found = Some(&a.value);
                }
            }
        });
        found
    }
    /// What the use at `use_span` in `c`'s view is given for `name`: its
    /// argument, or for an inject `c`'s own provided value (a section covers
    /// the whole view, LLP 1035.005.000 D9).
    fn supplied<'a>(
        c: &'a Component,
        use_span: Span,
        name: &str,
        inject: bool,
    ) -> Option<&'a Expr> {
        let mut args = None;
        Node::walk(&c.view).for_each(|node| {
            if let Node::Use {
                args: given, span, ..
            } = node
            {
                if *span == use_span {
                    args = Some(given);
                }
            }
        });
        let args = args?;
        if inject {
            c.provides.iter().find(|b| b.name == name).map(|b| &b.expr)
        } else {
            args.iter().find(|a| a.name == name).map(|a| &a.value)
        }
    }
    fn origin(
        file: &File,
        instances: &[contract_syntax::Instance],
        mut instance: u32,
        mut span: Span,
        refused: &str,
    ) -> Option<(u32, Span)> {
        loop {
            let site = &instances[instance as usize];
            let c = file.components.iter().find(|c| c.name == site.component)?;
            let expr = authored_expr(c, span)?;
            let name = expression_name(expr)?;
            let inject = c.injects.iter().any(|p| p.name == name);
            if name == refused && !inject && !c.props.iter().any(|p| p.name == name) {
                return Some((instance, span));
            }
            if !inject && !c.props.iter().any(|p| p.name == name) {
                return None;
            }
            let mut child = instance;
            loop {
                let site = &instances[child as usize];
                let parent = site.parent?;
                let c = file
                    .components
                    .iter()
                    .find(|c| c.name == instances[parent as usize].component)?;
                if let Some(expr) = supplied(c, site.span, name, inject) {
                    instance = parent;
                    span = expr.span();
                    break;
                }
                if !inject {
                    return None;
                }
                child = parent;
            }
        }
    }
    for c in &file.components {
        let found = if error.id == "analyze-unknown-action" {
            c.tasks
                .iter()
                .find(|task| task.timer.2 == error.span)
                .map(|task| {
                    (
                        task.timer.1.clone(),
                        suggestion(file, c, &task.timer.1, &[], true, false).map(str::to_owned),
                    )
                })
        } else {
            view(file, c, &c.view, error.span, &mut Vec::new(), context)
        };
        if let Some((_name, candidate)) = found {
            // A similarly spelled global function cannot repair an action-valued
            // position. Replace only the old hint, retaining the refusal and span.
            if let Some(at) = error.message.find("; did you mean `") {
                error.message.truncate(at);
            }
            if let Some(candidate) = candidate {
                error
                    .message
                    .push_str(&format!("; did you mean `{candidate}`?"));
            }
            return error;
        }
    }
    if error.id != "analyze-unknown-action" {
        if let Ok(expanded) = contract_syntax::expand_mapped(file) {
            let mut sites = Vec::new();
            Node::walk(&expanded.root.view).for_each(|node| {
                if let Node::Element {
                    attrs, instance, ..
                } = node
                {
                    if attrs.iter().any(|a| {
                        contract_analyze::HANDLERS.contains(&a.name.as_str())
                            && a.value.span() == error.span
                            && expression_name(&a.value) == Some(refused.as_str())
                    }) {
                        sites.push(*instance);
                    }
                }
            });
            if !sites.is_empty() {
                if let Some(at) = error.message.find("; did you mean `") {
                    error.message.truncate(at);
                }
            }
            let mut resolved = Vec::new();
            for instance in sites {
                let Some((owner, span)) =
                    origin(file, &expanded.instances, instance, error.span, &refused)
                else {
                    return error;
                };
                let c = file
                    .components
                    .iter()
                    .find(|c| c.name == expanded.instances[owner as usize].component)
                    .unwrap();
                let context = HintContext {
                    provider: true,
                    ..context
                };
                let Some((_, Some(candidate))) = provided(file, c, span, context)
                    .or_else(|| view(file, c, &c.view, span, &mut Vec::new(), context))
                else {
                    return error;
                };
                resolved.push((span, candidate));
            }
            if let Some((_, candidate)) = resolved.first() {
                if resolved.iter().all(|(_, name)| name == candidate) {
                    if let Some(at) = error.message.find("; did you mean `") {
                        error.message.truncate(at);
                    }
                    error
                        .message
                        .push_str(&format!("; did you mean `{candidate}`?"));
                    let mut related = error.related.into_vec();
                    for (span, _) in resolved {
                        if !related.iter().any(|r| r.span == span) {
                            related.push(RelatedLocation {
                                span,
                                file: None,
                                note: format!("the unknown action `{refused}` is supplied here"),
                            });
                        }
                    }
                    error.related = related.into_boxed_slice();
                }
            }
        }
    }
    error
}
