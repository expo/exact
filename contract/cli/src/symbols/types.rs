//! Each definition's type as TypeScript, written by the printer of `app.d.ts`'s
//! summary, and the shapes those types name.

use super::{Resolver, Shapes};
use crate::sources::Sources;
use crate::typescript::{inline, parameter, record, Node, Noted, Table};
use contract_syntax::{Component, Param};
use contract_types::Ty;
use serde_json::json;
use std::collections::BTreeMap;

/// The name TypeScript knows a shape by, for each whose own it cannot take.
pub(super) type Aliases = BTreeMap<String, String>;

/// An alias for each shape whose name is no TypeScript type name: one with a
/// hyphen (`A-B`), a reserved word (`class`) or a type of TypeScript's own
/// (`string`). An alias is the name with `_` for what an identifier cannot
/// hold, and `_` after it until no shape and no other alias has it. None
/// outside `symbols --editor`, which writes no type.
pub(super) fn aliases(shapes: &contract_types::Shapes, editor: bool) -> Aliases {
    const TYPES: [&str; 12] = [
        "any",
        "bigint",
        "boolean",
        "never",
        "number",
        "object",
        "string",
        "symbol",
        "undefined",
        "unique",
        "unknown",
        "void",
    ];
    let named = |name: &str| parameter(name).is_some() && !TYPES.contains(&name);
    let mut aliases = Aliases::new();
    if !editor {
        return aliases;
    }
    let mut taken = std::collections::BTreeSet::new();
    for name in shapes.map.keys().filter(|name| !name.is_empty()) {
        if named(name) {
            continue;
        }
        let mut alias: String = (name.chars())
            .map(|c| match c {
                c if c.is_ascii_alphanumeric() || c == '$' => c,
                _ => '_',
            })
            .collect();
        if alias.starts_with(|c: char| c.is_ascii_digit()) {
            alias.insert(0, '_');
        }
        while !named(&alias) || shapes.map.contains_key(&alias) || !taken.insert(alias.clone()) {
            alias.push('_');
        }
        aliases.insert(name.clone(), alias);
    }
    aliases
}

/// The checker's types, as the printer reads a plan's, each shape by the
/// name TypeScript knows it by.
struct Checked<'a> {
    shapes: &'a contract_types::Shapes,
    aliases: &'a Aliases,
}
impl Table for Checked<'_> {
    type Id = Ty;
    fn node<'a>(&'a self, ty: &'a Ty) -> Option<Node<'a, Ty>> {
        Some(match ty {
            Ty::Number => Node::Number,
            Ty::String => Node::String(Vec::new()),
            Ty::Enum(values) => Node::String(values.iter().map(String::as_str).collect()),
            Ty::Bool => Node::Bool,
            Ty::Unit => Node::Unit,
            Ty::Option(t) => Node::Option((**t).clone()),
            Ty::List(t) => Node::List((**t).clone()),
            Ty::Record(name) => {
                let (name, fields) = self.shapes.map.get_key_value(name)?;
                Node::Record(
                    self.aliases.get(name).unwrap_or(name),
                    fields
                        .iter()
                        .map(|(n, t)| (n.as_str(), t.clone()))
                        .collect(),
                )
            }
            Ty::Action(_) | Ty::Unknown => return None,
        })
    }
}

impl<'a> Resolver<'a> {
    /// What `print` writes, with each shape it names noted in the graph's
    /// `shapes`; none when it names a shape the printer cannot write, and
    /// none outside `symbols --editor`.
    fn printed(
        &mut self,
        print: impl FnOnce(&Checked, &mut Noted<Ty>) -> Option<String>,
    ) -> Option<String> {
        if !self.graph.editor {
            return None;
        }
        let table = Checked {
            shapes: &self.types.shapes,
            aliases: self.aliases,
        };
        let mut named = Noted::default();
        let text = print(&table, &mut named)?;
        self.note(&table, named).then_some(text)
    }
    /// `ty` as TypeScript. An action is a function of its parameters only
    /// where it is declared, with `names`: a bare `action` prop or inject is
    /// `Action([])` whatever it is called with, so it has no type to write.
    pub(super) fn typescript(&mut self, ty: &Ty, names: Option<&[Param]>) -> Option<String> {
        self.printed(|table, named| match ty {
            Ty::Action(params) => {
                let names = names?;
                let types = (params.iter())
                    .map(|p| inline(table, p, named))
                    .collect::<Option<Vec<_>>>()?;
                let names = (names.len() == types.len())
                    .then(|| names.iter().map(|p| parameter(&p.name)).collect())
                    .flatten();
                Some(signature(names, &types, "void"))
            }
            _ => inline(table, ty, named),
        })
    }
    /// The body of `shape`, `{ text: string }`.
    pub(super) fn shape_typescript(&mut self, shape: &str) -> Option<String> {
        self.printed(|table, named| record(table, &Ty::Record(shape.into()), named))
    }
    /// A `fn`'s type, its parameters named as declared.
    pub(super) fn fn_typescript(&mut self, name: &str, params: &[Param]) -> Option<String> {
        let (types, result) = self.types.shapes.fns.get(name)?;
        self.printed(|table, named| {
            let types = (types.iter())
                .map(|p| inline(table, p, named))
                .collect::<Option<Vec<_>>>()?;
            let result = inline(table, result, named)?;
            let names = (params.len() == types.len())
                .then(|| params.iter().map(|p| parameter(&p.name)).collect())
                .flatten();
            Some(signature(names, &types, &result))
        })
    }
    /// Each shape in `named`, and each it names in turn, with its body in the
    /// graph's `shapes`; false when one has no body.
    fn note(&mut self, table: &Checked, mut named: Noted<Ty>) -> bool {
        let mut bodies = Vec::new();
        let mut i = 0;
        while i < named.order.len() {
            let ty = named.order[i].clone();
            i += 1;
            let Ty::Record(name) = &ty else { continue };
            let key = (self.graph.alone, name.clone());
            if self.graph.shapes.contains_key(&key) {
                continue;
            }
            match record(table, &ty, &mut named) {
                Some(body) => bodies.push((key, (self.aliases.get(name).cloned(), body))),
                None => return false,
            }
        }
        self.graph.shapes.extend(bodies);
        true
    }
    /// The types of `c`'s members, as its scope holds them.
    /// `root`: whether `c` holds the router's state.
    pub(super) fn member_types(&mut self, c: &'a Component, root: bool) {
        let file = self.file;
        let params =
            |name: &str| (c.actions.iter().find(|a| a.name == name)).map(|a| a.params.as_slice());
        for kind in super::MEMBERS {
            let names: Vec<&str> = match kind {
                "prop" => c.props.iter().map(|p| p.name.as_str()).collect(),
                "inject" => c.injects.iter().map(|p| p.name.as_str()).collect(),
                "state" => (file.routes.iter().filter(|_| root))
                    .map(|r| r.slot.as_str())
                    .chain(c.states.iter().map(|b| b.name.as_str()))
                    .collect(),
                "derive" => c.derives.iter().map(|b| b.name.as_str()).collect(),
                "resource" => c.resources.iter().map(|r| r.name.as_str()).collect(),
                "mutation" => c.mutations.iter().map(|m| m.name.as_str()).collect(),
                _ => c.actions.iter().map(|a| a.name.as_str()).collect(),
            };
            for name in names {
                let Some(to) = self.graph.find(kind, name, Some(&c.name), None) else {
                    continue;
                };
                let Some(ty) = self.scope.lookup(name).map(|(_, ty)| ty.clone()) else {
                    continue;
                };
                let names = if kind == "action" { params(name) } else { None };
                self.graph.definitions[to].ty = self.typescript(&ty, names);
            }
        }
    }
}

/// `(text: string) => void`; with no names, `(...args: [string]) => void`.
fn signature(names: Option<Vec<String>>, types: &[String], result: &str) -> String {
    match names.or_else(|| types.is_empty().then(Vec::new)) {
        Some(names) => format!(
            "({}) => {result}",
            (names.iter().zip(types))
                .map(|(n, t)| format!("{n}: {t}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        None => format!("(...args: [{}]) => {result}", types.join(", ")),
    }
}

/// `shapes`: each shape the definitions' types name, with its body, the name
/// the types write it `as` when that is not its own, and the `file` it was
/// read in when files were read on their own.
pub(super) fn write(out: &mut Vec<u8>, sources: &Sources, shapes: &Shapes) {
    let shapes: Vec<_> = (shapes.iter())
        .map(|((alone, name), (alias, body))| {
            let mut shape = json!({ "name": name, "type": body });
            if let Some(alias) = alias {
                shape["as"] = alias.as_str().into();
            }
            if let Some(path) = alone.and_then(|id| sources.paths().get(id as usize)) {
                shape["file"] = path.to_string_lossy().into();
            }
            shape
        })
        .collect();
    out.extend_from_slice(b",\"shapes\":");
    serde_json::to_writer(out, &shapes).expect("writing JSON into a Vec cannot fail");
}
