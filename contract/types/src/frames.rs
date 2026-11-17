//! The frames a `for`, a `match`, an `if` and a `let` push, and a record's
//! field as a member reads it: one code path for lowering (`contract-lower`'s
//! `Node::Each`, `Node::Match`, `Node::When`, `expr::compile`) and the design
//! server's write check (`theme::scan`, `theme::rows::typed`), so the two
//! type a row alike.

use crate::{Ref, Scope, Shapes, Ty};
use contract_syntax::Node;
use std::sync::Arc;

/// The scopes under a node, shared by parameter inference and checking.
/// An untyped subject skips only its binding arm; the none arm is independent.
/// Kept below checking because action parameter inference needs it before `Checked` exists.
pub fn view_frames(node: &Node, scope: &Arc<Scope>, shapes: &Shapes) -> [Option<Arc<Scope>>; 2] {
    match node {
        Node::When { .. } => {
            let mut inner = (**scope).clone();
            inner.push_none();
            let inner = Arc::new(inner);
            [Some(inner.clone()), Some(inner)]
        }
        Node::Each {
            var, index, list, ..
        } => {
            let inner = match crate::infer(list, scope, shapes) {
                Ok(Ty::List(item)) => {
                    let mut inner = (**scope).clone();
                    inner.push_each(var, index.as_deref(), *item);
                    Some(Arc::new(inner))
                }
                _ => None,
            };
            [inner, None]
        }
        Node::Match { subject, some, .. } => {
            let inner = match crate::infer(subject, scope, shapes) {
                Ok(Ty::Option(item)) => {
                    let mut inner = (**scope).clone();
                    inner.push_region(Some((some.0.clone(), Ref::Bound(0), *item)));
                    Some(Arc::new(inner))
                }
                _ => None,
            };
            let mut none = (**scope).clone();
            none.push_none();
            [inner, Some(Arc::new(none))]
        }
        _ => [Some(scope.clone()), Some(scope.clone())],
    }
}

impl Scope {
    /// A `for`'s frame over a list typed `list`: its item the `t` of `[t]`,
    /// not known for any other type, and its position when named. Lowering
    /// and the design server's write check push it alike.
    pub fn push_each_of(&mut self, item: &str, index: Option<&str>, list: Ty) {
        let ty = match list {
            Ty::List(t) => *t,
            _ => Ty::Unknown,
        };
        self.push_each(item, index, ty);
    }

    /// A `match`'s `some` arm's frame over a subject typed `subject`: its
    /// binding the `t` of `t?`, not known for any other type.
    pub fn push_some_of(&mut self, name: &str, subject: Ty) {
        let ty = match subject {
            Ty::Option(t) => *t,
            _ => Ty::Unknown,
        };
        self.push_region(Some((name.to_string(), Ref::Bound(0), ty)));
    }

    /// A region's frame that binds nothing: an `if`'s arms, a `match`'s
    /// `none` arm.
    pub fn push_none(&mut self) {
        self.push_region(None);
    }

    /// A `let`'s frame: its name, the local `index`, typed `ty`.
    pub fn push_let(&mut self, name: &str, index: u32, ty: Ty) {
        self.push(vec![(name.to_string(), Ref::Local(index), ty)]);
    }
}

impl Shapes {
    /// `obj.name`'s position and type, `obj` a value typed `obj`: a field of
    /// its record, as `expr::compile` reads `Expr::Member` and the design
    /// server's write check types it; `None` for a non-record or a missing
    /// field.
    pub fn member(&self, obj: &Ty, name: &str) -> Option<(usize, &Ty)> {
        match obj {
            Ty::Record(shape) => self.field(shape, name),
            _ => None,
        }
    }
}
