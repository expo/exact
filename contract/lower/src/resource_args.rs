//! A resource's arguments and `with` context, and, over a list (LLP 1017.000
//! P4c, amended 2026-11-04), its list and key: the arguments and the key read
//! the element as their one parameter, which the runner passes each
//! element's request.

use super::{LowerError, Lowerer};
use contract_syntax::{Component, Expr, Owner};
use contract_types::{Ref, Scope, Ty};

impl Lowerer<'_> {
    /// Every resource the root owns: a child's are read in its instance's
    /// frames, once the arms exist ([`Lowerer::owned_resource_args`]).
    pub(crate) fn resource_args(
        &mut self,
        root: &Component,
        owners: &[Owner],
        scope: &Scope,
    ) -> Result<(), LowerError> {
        for (i, r) in root.resources.iter().enumerate() {
            if owners.get(i).is_some_and(|o| *o != Owner::Root) {
                continue;
            }
            self.one_resource_args(i, r, scope)?;
        }
        Ok(())
    }

    /// A child's resources (LLP 1035.005.004 D1): each one's arguments and
    /// gate read in its owning arm's scope, or, for a use outside every
    /// region, the root's, settled once its `late` slots are.
    pub(crate) fn owned_resource_args(
        &mut self,
        root: &Component,
        ex: &contract_syntax::Expanded,
        scope: &Scope,
    ) -> Result<(), LowerError> {
        for (i, r) in root.resources.iter().enumerate() {
            let owner = ex.resource_owners.get(i).copied().unwrap_or(Owner::Root);
            if owner != Owner::Root {
                self.sharing.instance = ex.resource_instances[i];
            }
            match owner {
                Owner::Root => {}
                Owner::Instance => {
                    self.one_resource_args(i, r, scope)?;
                    self.b.set_resource_late(self.resources[i]);
                }
                Owner::Arm { tag, arm } => {
                    let (id, arm_scope) =
                        self.arm_scopes
                            .get(&(tag, arm))
                            .cloned()
                            .ok_or_else(|| LowerError {
                                id: "lower-row-slot",
                                message: format!(
                                    "resource `{}` names a region that was not lowered",
                                    r.name
                                ),
                                span: r.span,
                            })?;
                    self.sharing.arm = Some(id);
                    let lowered = self.one_resource_args(i, r, &arm_scope);
                    self.sharing.arm = None;
                    lowered?;
                    self.b.set_resource_owner(self.resources[i], id);
                }
            }
        }
        Ok(())
    }

    /// A child's mutations (LLP 1035.005.004 D6): an arm-owned one's slot
    /// and row name the arm, and a use outside every region's slot is
    /// `late`; each one's `then` arguments, its lifted action's captured
    /// props, read in the scope its instance was made in.
    pub(crate) fn owned_mutations(
        &mut self,
        root: &Component,
        ex: &contract_syntax::Expanded,
        scope: &Scope,
    ) -> Result<(), LowerError> {
        for (i, m) in root.mutations.iter().enumerate() {
            let owner = ex.mutation_owners.get(i).copied().unwrap_or(Owner::Root);
            let (arm, scope) = match owner {
                Owner::Root => continue,
                Owner::Instance => {
                    self.b.set_slot_late(self.mutation_slots[i]);
                    (None, scope.clone())
                }
                Owner::Arm { tag, arm } => {
                    let (id, arm_scope) =
                        self.arm_scopes
                            .get(&(tag, arm))
                            .cloned()
                            .ok_or_else(|| LowerError {
                                id: "lower-row-slot",
                                message: format!(
                                    "mutation `{}` names a region that was not lowered",
                                    m.name
                                ),
                                span: m.span,
                            })?;
                    self.b.set_slot_owner(self.mutation_slots[i], id);
                    self.b.set_mutation_owner(self.mutations[i], id);
                    (Some(id), arm_scope)
                }
            };
            let args = ex.mutation_then_args.get(i).map_or(&[][..], Vec::as_slice);
            if args.is_empty() {
                continue;
            }
            self.sharing.instance = ex.mutation_instances[i];
            self.sharing.arm = arm;
            let codes: Result<Vec<_>, _> =
                args.iter().map(|a| self.expr_code(a, &scope, 0)).collect();
            self.sharing.arm = None;
            self.b.set_mutation_then_args(self.mutations[i], &codes?);
        }
        Ok(())
    }

    fn one_resource_args(
        &mut self,
        i: usize,
        r: &contract_syntax::ResourceDecl,
        scope: &Scope,
    ) -> Result<(), LowerError> {
        {
            if r.keep {
                if r.source != "exactSurface" || r.transient {
                    return Err(LowerError { id: "lower-resource-keep", message: "`keep` is only for an exactSurface reader and cannot combine with `transient`".into(), span: r.span });
                }
                self.b.set_resource_keep(self.resources[i]);
            }
            let mut inner = scope.clone();
            if let Some(each) = &r.each {
                if exact_plan::runner_owned_source(&r.source) {
                    return Err(LowerError {
                        id: "lower-resource-for-source",
                        message: format!(
                            "`{}` is the runner's, one answer for the app: a resource over a list asks a data source for each element",
                            r.source
                        ),
                        span: r.span,
                    });
                }
                let list = self.expr_code(&each.list, scope, 0)?;
                let item = match contract_types::infer(&each.list, scope, &self.types.shapes) {
                    Ok(Ty::List(t)) => *t,
                    _ => Ty::Unknown,
                };
                inner.push(vec![(each.item.clone(), Ref::Param(0), item)]);
                let element = Expr::Ident(each.item.clone(), each.span);
                let key = self.expr_code(each.key.as_ref().unwrap_or(&element), &inner, 0)?;
                self.b.set_resource_each(self.resources[i], list, key);
            }
            let mut args = Vec::new();
            for a in &r.args {
                args.push(self.expr_code(a, &inner, 0)?);
            }
            let range = self.b.args(&args);
            self.b.set_resource_args(self.resources[i], range);
            // @ref LLP 1161.002 D1 — read in the root's scope, as the arguments are.
            if let Some(gate) = &r.gate {
                let code = self.expr_code(gate, &inner, 0)?;
                self.b.set_resource_gate(self.resources[i], code);
            }
            // @ref LLP 1027.005 D6 — declare first, install the full argument
            // list, then count its context tail. No `with` keeps the default 0.
            if let Some(identity) = r.identity {
                let context = u16::try_from(r.args.len() - identity).map_err(|_| LowerError {
                    id: "lower-resource-context",
                    message: "a resource supports at most 65535 request context values".into(),
                    span: r.span,
                })?;
                self.b.set_resource_context(self.resources[i], context);
            }
        }
        Ok(())
    }
}
