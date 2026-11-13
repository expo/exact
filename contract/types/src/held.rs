//! Where an action may be held. A prop or an inject of type `action` is an
//! action reference the use site supplies (LLP 1006 §2); nothing else holds
//! one, and the plan has no type for an action as a value.

use crate::{Shapes, Ty, TypeError};
use contract_syntax::{Expr, Span, TypeExpr};

impl Shapes {
    /// Resolve a written type that holds a value (a shape's field, a state,
    /// a resource's or mutation's shape, a parameter), refusing `action`
    /// anywhere in it. `holder` names the declaration for the refusal.
    pub fn resolve_value(&self, t: &TypeExpr, holder: &str) -> Result<Ty, TypeError> {
        match self.action_in(t) {
            Some(span) => Err(refusal(&format!("{holder} cannot hold an action"), span)),
            None => self.resolve(t),
        }
    }

    fn action_in(&self, t: &TypeExpr) -> Option<Span> {
        match t {
            TypeExpr::Named(n, span) => {
                (n == "action" && !self.map.contains_key(n)).then_some(*span)
            }
            TypeExpr::Option(inner, _) | TypeExpr::List(inner, _) => self.action_in(inner),
            TypeExpr::Enum(..) => None,
        }
    }
}

/// `[…]`'s item or `some(…)`'s value `x`, of type `t`, refused when it is
/// an action.
pub(crate) fn value(t: &Ty, x: &Expr, holder: &str) -> Result<(), TypeError> {
    match (t, x) {
        (Ty::Action(_), Expr::Ident(name, span)) => Err(refusal(
            &format!("`{name}` is an action, which {holder} cannot hold"),
            *span,
        )),
        (Ty::Action(_), _) => Err(refusal(
            &format!("{holder} cannot hold an action"),
            x.span(),
        )),
        _ => Ok(()),
    }
}

fn refusal(lead: &str, span: Span) -> TypeError {
    TypeError {
        id: "type-action-held",
        message: format!(
            "{lead}: an action is passed to a component only as a prop or an inject (`run: action` under `props`) and bound by a handler (`press=run`); to act on a row, write one action that takes the row's id (`press=open(item.id)`)"
        ),
        span,
    }
}
