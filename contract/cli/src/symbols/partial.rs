//! What navigation reads of a refused program: the shapes and functions
//! that check on their own, the scope of each name the syntax refused, and
//! each file that parses when the program does not load.

use super::{Definition, Graph, Resolver};
use crate::sources::Sources;
use contract_syntax::*;
use contract_types::{strings::Strings, Scope, Shapes, Types};
use std::{borrow::Cow, collections::BTreeSet, sync::Arc};

/// A name the syntax refused, in the scope it would have been declared in:
/// a component's members, or a file's declarations.
pub(super) struct Shadow {
    name: String,
    /// The component, by its name in the program.
    component: Option<String>,
    /// For a declaration, the file whose references may mean it.
    site: Option<u32>,
}

impl Shadow {
    /// Whether the reference at `span` to `target` may mean the refused name.
    /// A local or parameter shadows it; a field or ID is in no namespace a
    /// refused declaration or member names.
    pub(super) fn hides(&self, span: Span, target: &Definition) -> bool {
        let kind = matches!(
            target.kind,
            "local" | "parameter" | "field" | "id" | "testId" | "provide"
        );
        let scope = match (&self.component, self.site) {
            (Some(component), _) => target.component.as_ref() == Some(component),
            (None, site) => target.component.is_none() && site.is_none_or(|s| s == span.source_id),
        };
        self.name == target.name && !kind && scope
    }
}

/// Each refused name (with the source it was refused in) in its scope in
/// `file`. A member's component is the one declared under that name in that
/// source, or listing it as a members file; the loader may have renamed it.
pub(super) fn shadows(file: &File, hidden: &[(u32, Hidden)]) -> Vec<Shadow> {
    let written = |c: &Component| {
        let identity = c
            .params
            .identity
            .as_deref()
            .and_then(|i| i.rsplit_once('#'));
        identity.map_or(c.name.clone(), |(_, name)| name.to_owned())
    };
    let holds = |c: &Component, source: u32| {
        let members = (c.states.iter().chain(&c.derives).map(|b| b.span))
            .chain(c.actions.iter().map(|a| a.span))
            .chain(c.resources.iter().map(|r| r.span))
            .chain(c.mutations.iter().map(|m| m.span))
            .chain(c.tasks.iter().map(|t| t.span));
        c.span.source_id == source || members.into_iter().any(|s| s.source_id == source)
    };
    let mut all = Vec::new();
    for (source, h) in hidden {
        let Some(written_as) = &h.component else {
            all.push(Shadow {
                name: h.name.clone(),
                component: None,
                site: Some(*source),
            });
            continue;
        };
        let named: Vec<&Component> = (file.components.iter())
            .filter(|c| written(c) == *written_as)
            .collect();
        // With none of its members left to say which, every component
        // written under the name.
        let held: Vec<&Component> = named
            .iter()
            .copied()
            .filter(|c| holds(c, *source))
            .collect();
        for c in if held.is_empty() { named } else { held } {
            all.push(Shadow {
                name: h.name.clone(),
                component: Some(c.name.clone()),
                site: None,
            });
        }
    }
    all
}

/// The shapes and functions that check: one a refusal is in is left out and
/// the check run again, so a reference to it has no edge and the rest keep
/// theirs. A refusal in none of them is the route table's, and the rest are
/// checked without it. A `t(…)` is checked against `strings`, the app's tables.
pub(super) fn declared(file: &File, strings: &Option<Arc<Strings>>) -> Shapes {
    enum Out {
        Shape(usize, ShapeDecl),
        Fn(usize, FnDecl),
    }
    let mut file = Cow::Borrowed(file);
    loop {
        let error = match contract_types::check_declarations(&file, strings.clone()) {
            Ok(shapes) => return shapes,
            Err(error) => error,
        };
        // The last shape or function declared before the refusal.
        let (at, source) = ((error.span.line, error.span.col), error.span.source_id);
        let before = |span: Span| {
            (span.source_id == source && (span.line, span.col) <= at)
                .then_some((span.line, span.col))
        };
        let shape =
            (file.shapes.iter().enumerate()).filter_map(|(i, s)| Some((before(s.span)?, i)));
        let shape = shape.max();
        let function =
            (file.fns.iter().enumerate()).filter_map(|(i, f)| Some((before(f.span)?, i)));
        let function = function.max();
        let out = match (shape, function) {
            (Some(s), Some(f)) if f.0 > s.0 => Some(Out::Fn(f.1, file.to_mut().fns.remove(f.1))),
            (None, Some(f)) => Some(Out::Fn(f.1, file.to_mut().fns.remove(f.1))),
            (Some(s), _) => Some(Out::Shape(s.1, file.to_mut().shapes.remove(s.1))),
            (None, None) => None,
        };
        if let Some(out) = out {
            let again = contract_types::check_declarations(&file, strings.clone()).err();
            let same = again.is_some_and(|e| {
                (e.id, e.span, &e.message) == (error.id, error.span, &error.message)
            });
            if !same {
                continue;
            }
            // Not where the refusal is: put back.
            match out {
                Out::Shape(i, shape) => file.to_mut().shapes.insert(i, shape),
                Out::Fn(i, function) => file.to_mut().fns.insert(i, function),
            }
        }
        if file.routes.is_none() {
            return Shapes::default();
        }
        file.to_mut().routes = None;
    }
}

/// Each source that parses, resolved on its own and unchecked: what a program
/// that does not load still declares, with no edge from one file to another.
pub(super) fn file_by_file(sources: &Sources, strings: &Option<Arc<Strings>>) -> Graph {
    let mut graph = Graph::default();
    let mut read = BTreeSet::new();
    for (id, (path, text)) in sources.paths().iter().zip(&sources.texts).enumerate() {
        // A file two lines named, each read anew; a package's module
        // declaration is JSON.
        if !read.insert(path) || path.file_name().is_some_and(|f| f == "exact.json") {
            continue;
        }
        let (mut file, hidden) = match parse_source_partial(text, id as u32) {
            Ok(file) => (file, Vec::new()),
            Err(Partial {
                file: Some(file),
                hidden,
                ..
            }) => (*file, hidden),
            Err(_) => continue,
        };
        // A members file's block holds members of a component declared in
        // another file.
        let members = file.members.take().map(|block| {
            file.components.push(block);
            file.components.len() - 1
        });
        let types = Types {
            shapes: declared(&file, strings),
            components: Vec::new(),
        };
        let hidden: Vec<_> = hidden.into_iter().map(|h| (id as u32, h)).collect();
        graph.index.clear();
        graph.alone = Some(id as u32);
        graph.ids.clear();
        graph.ambiguous.clear();
        graph.hidden = shadows(&file, &hidden);
        let mut r = Resolver {
            file: &file,
            types: &types,
            typed: false,
            whole: false,
            members,
            graph,
            component: None,
            scope: Scope::default(),
            locals: Vec::new(),
            owner: None,
        };
        r.declarations();
        r.file(None);
        graph = r.graph;
    }
    graph
}
