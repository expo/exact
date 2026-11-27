//! Two refusals that bound a value by its type (LLP 1090 D2): an option
//! directly inside an option, and a type nested deeper than a value may.
//!
//! The web's JS target erases `some`, so `some(none)` and `none` are one
//! value there; refusing the type keeps every target counting the same
//! values, and leaves at most one `some` above each value the runner counts.
//! A type's depth bounds its values' (no value has type `?`), so the
//! runner's `MAX_VALUE_DEPTH` becomes a rule every target checks once.

use crate::{err, Shapes, Ty, TypeError};
use contract_syntax::{Span, TypeExpr};
use std::collections::{BTreeMap, BTreeSet};

/// The deepest a value may nest: the runner's `MAX_VALUE_DEPTH`.
pub const MAX_TYPE_DEPTH: u32 = 64;

impl Shapes {
    /// Resolve `t` iteratively so deeply nested wrapper chains never exhaust
    /// native stack space before [`Shapes::bounded`] runs.
    pub(crate) fn resolve_unbounded(&self, mut t: &TypeExpr) -> Result<Ty, TypeError> {
        let mut wrappers = Vec::new();
        let mut ty = loop {
            match t {
                TypeExpr::Option(inner, _) => {
                    wrappers.push(true);
                    t = inner;
                }
                TypeExpr::List(inner, _) => {
                    wrappers.push(false);
                    t = inner;
                }
                TypeExpr::Enum(values, _) => break Ty::Enum(values.clone()),
                TypeExpr::Named(n, span) => {
                    break match n.as_str() {
                        "number" => Ty::Number,
                        "string" => Ty::String,
                        "bool" => Ty::Bool,
                        "unit" => Ty::Unit,
                        other if self.map.contains_key(other) => Ty::Record(other.to_string()),
                        // Bare `action` is an action prop of inferred arity (LLP 1006 §2).
                        // Near-prefix names are ordinary unknown types.
                        "action" => Ty::Action(Vec::new()),
                        other => return Err(self.unknown_type(other, *span)),
                    };
                }
            }
        };
        for is_option in wrappers.into_iter().rev() {
            ty = if is_option {
                Ty::Option(Box::new(ty))
            } else {
                Ty::List(Box::new(ty))
            };
        }
        Ok(ty)
    }

    /// How deep a value of `t` can nest: 0 for a scalar, one more than what
    /// it holds for an option, list or record. `?` and an action are 0. A
    /// shape not yet measured counts 1, the least it can be.
    pub fn depth(&self, mut t: &Ty) -> u32 {
        let mut wrappers = 0;
        while let Ty::Option(inner) | Ty::List(inner) = t {
            wrappers += 1;
            t = inner;
        }
        match t {
            Ty::Record(name) => wrappers + self.depths.get(name).copied().unwrap_or(1),
            _ => wrappers,
        }
    }

    /// Measure every shape once its fields are known; shapes are acyclic
    /// (`type-shape-recursive`), so each is measured from the ones it holds.
    /// The walk keeps its own stack: a chain of thousands of shapes is
    /// measured, and then refused as too deep, without exhausting the native one.
    pub(crate) fn measure_shapes(&mut self) {
        /// The shape `t` holds, with the options and lists around it.
        fn leaf(mut t: &Ty) -> (u32, Option<&str>) {
            let mut wrappers = 0;
            while let Ty::Option(inner) | Ty::List(inner) = t {
                wrappers += 1;
                t = inner;
            }
            match t {
                Ty::Record(name) => (wrappers, Some(name)),
                _ => (wrappers, None),
            }
        }
        let map = &self.map;
        let mut depths: BTreeMap<String, u32> = BTreeMap::new();
        // A shape is pushed unopened, then again opened above what it holds,
        // and measured when the opened entry comes back to the top.
        let mut opened: BTreeSet<&str> = BTreeSet::new();
        let mut stack: Vec<(&str, bool)> = map.keys().rev().map(|n| (n.as_str(), false)).collect();
        while let Some((name, open)) = stack.pop() {
            if depths.contains_key(name) {
                continue;
            }
            let fields = map.get(name).map(Vec::as_slice).unwrap_or_default();
            if !open {
                // A shape already opened would be a cycle, refused before this.
                if opened.insert(name) {
                    stack.push((name, true));
                    stack.extend(
                        fields
                            .iter()
                            .filter_map(|(_, t)| leaf(t).1)
                            .map(|n| (n, false)),
                    );
                }
                continue;
            }
            let inner = fields.iter().map(|(_, t)| match leaf(t) {
                (wrappers, Some(held)) => wrappers + depths.get(held).copied().unwrap_or(1),
                (wrappers, None) => wrappers,
            });
            depths.insert(name.to_owned(), 1 + inner.max().unwrap_or(0));
        }
        self.depths = depths;
    }

    /// Refuse `t` if no target could hold a value of it as the runner does.
    pub fn bounded(&self, t: &Ty, span: Span) -> Result<(), TypeError> {
        if let Some(inner) = option_in_option(t) {
            return err(
                "type-option-option",
                format!("`{t}` holds `option<{inner}>` directly inside an option, and `some(none)` would be `none` on the web: hold the inner option in a record field instead (`shape Maybe` with a field `value: option<{inner}>`)"),
                span,
            );
        }
        let depth = self.depth(t);
        if depth > MAX_TYPE_DEPTH {
            return err(
                "type-too-deep",
                format!(
                    "`{t}` nests {depth} levels deep, and a value nests at most {MAX_TYPE_DEPTH}"
                ),
                span,
            );
        }
        Ok(())
    }
}

/// What the first option directly inside an option in `t` holds.
fn option_in_option(mut t: &Ty) -> Option<&Ty> {
    loop {
        match t {
            Ty::Option(inner) => match &**inner {
                Ty::Option(held) => return Some(held),
                next => t = next,
            },
            Ty::List(inner) => t = inner,
            _ => return None,
        }
    }
}
