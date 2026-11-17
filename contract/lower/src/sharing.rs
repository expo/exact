//! Share instance expressions after type and shape checks have seen substitution.
use crate::{expr, theme, LowerError, Lowerer};
use contract_syntax::{
    inline::{Owner, Thunk},
    Expr, Node, Span,
};
use contract_types::{Scope, Ty};
use exact_plan::{asm::Asm, ArmsId, DerivesId, NodesId, Plan};
use std::collections::{BTreeMap, BTreeSet};

// Keep sixteen of the executors' 64 call frames free. Sharing is optional;
// an unusually deep dependency stays substituted instead of spending them.
const MAX_SHARED_DEPTH: usize = 48;

#[derive(Clone, Copy)]
struct AdmissionAt<'a> {
    plan: &'a Plan,
    parents: &'a [Option<u32>],
    active_depth: usize,
    candidate_active: bool,
    reader_instance: u32,
    candidate_instance: u32,
    reader_arm: Option<ArmsId>,
    owner_arm: Option<ArmsId>,
    reader_scope: &'a Scope,
    owner_scope: &'a Scope,
    reader_expr: &'a Expr,
    candidate_expr: &'a Expr,
    cached_depth: Option<usize>,
}

fn admitted_cut(site: AdmissionAt<'_>) -> Option<u32> {
    if site.active_depth >= MAX_SHARED_DEPTH || site.candidate_active {
        return None;
    }
    let mut chain = vec![site.reader_instance];
    while let Some(Some(parent)) = site.parents.get(*chain.last()? as usize) {
        chain.push(*parent);
    }
    if !chain.contains(&site.candidate_instance) || site.reader_expr != site.candidate_expr {
        return None;
    }
    // A `for` key has its item scope before its row arm exists. Arm distance
    // proves owner reachability; lexical depth independently rebases reads.
    site.plan.frame_cut(site.reader_arm, site.owner_arm)?;
    let cut = site
        .reader_scope
        .region_depth()
        .checked_sub(site.owner_scope.region_depth())?;
    // A callback or expression-local binder can shadow a prop's argument.
    // Only free value reads must resolve as they do in the owner frame.
    if !same_reads(
        site.reader_expr,
        site.reader_scope,
        site.owner_scope,
        cut,
        &mut Vec::new(),
    ) || site
        .cached_depth
        .is_some_and(|depth| site.active_depth + depth > MAX_SHARED_DEPTH)
    {
        return None;
    }
    Some(cut)
}

pub(super) struct Sharing {
    thunks: Vec<Thunk>,
    by_span: BTreeMap<Span, Vec<usize>>,
    rows: Vec<Option<(DerivesId, Ty, usize)>>,
    active: BTreeSet<usize>,
    body_depths: Vec<usize>,
    pub root_scope: Scope,
    pub arm: Option<ArmsId>,
    pub instance: u32,
    pub regions: BTreeMap<u32, u32>,
    parents: Vec<Option<u32>>,
}

impl Sharing {
    pub fn new(ex: &contract_syntax::Expanded) -> Self {
        let thunks = &ex.thunks;
        let mut by_span = BTreeMap::<_, Vec<usize>>::new();
        for (i, thunk) in thunks.iter().enumerate() {
            let candidates = by_span.entry(thunk.expr.span()).or_default();
            // Equivalent declarations in one instance are one body. Registering
            // each alias would let an active body call an equivalent sibling.
            if !candidates.iter().any(|&j| {
                thunks[j].instance == thunk.instance
                    && thunks[j].owner == thunk.owner
                    && thunks[j].expr == thunk.expr
            }) {
                candidates.push(i);
            }
        }
        Self {
            thunks: thunks.to_vec(),
            by_span,
            rows: vec![None; thunks.len()],
            active: BTreeSet::new(),
            body_depths: Vec::new(),
            root_scope: Scope::default(),
            arm: None,
            instance: 0,
            regions: ex.region_instances.clone(),
            parents: ex.instances.iter().map(|i| i.parent).collect(),
        }
    }
}

impl Lowerer<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn nodes(
        &mut self,
        nodes: &[Node],
        parent: Option<NodesId>,
        arm: Option<ArmsId>,
        scope: &Scope,
        locals: u16,
        parent_tag: Option<&str>,
    ) -> Result<(), LowerError> {
        let outer = (self.sharing.arm, self.sharing.instance);
        self.sharing.arm = arm;
        for (order, n) in nodes.iter().enumerate() {
            if theme::retained::stops_at_refusal() && !self.errors.is_empty() {
                break;
            }
            self.sharing.instance = match n {
                Node::Element { instance, .. } => *instance,
                Node::When { tag, .. } | Node::Each { tag, .. } | Node::Match { tag, .. } => {
                    self.sharing.regions.get(tag).copied().unwrap_or(outer.1)
                }
                _ => outer.1,
            };
            if let Err(e) = self.node(n, parent, arm, order as u32, scope, locals, parent_tag) {
                self.errors.push(e);
            }
        }
        (self.sharing.arm, self.sharing.instance) = outer;
        Ok(())
    }

    pub(crate) fn shared_expression(
        &mut self,
        e: &Expr,
        scope: &Scope,
    ) -> Result<Option<(DerivesId, Ty)>, LowerError> {
        let Some(candidates) = self.sharing.by_span.get(&e.span()) else {
            return Ok(None);
        };
        let mut chain = vec![self.sharing.instance];
        while let Some(Some(parent)) = self.sharing.parents.get(*chain.last().unwrap() as usize) {
            chain.push(*parent);
        }
        let mut candidates = candidates.clone();
        candidates.sort_by_key(|i| {
            chain
                .iter()
                .position(|n| *n == self.sharing.thunks[*i].instance)
                .unwrap_or(usize::MAX)
        });
        let found = candidates.into_iter().find_map(|i| {
            let (owner, own_scope) = match self.sharing.thunks[i].owner {
                Owner::Root | Owner::Instance => (None, self.sharing.root_scope.clone()),
                Owner::Arm { tag, arm } => {
                    let (id, scope) = self.arm_scopes.get(&(tag, arm))?;
                    (Some(*id), scope.clone())
                }
            };
            admitted_cut(AdmissionAt {
                plan: self.b.plan(),
                parents: &self.sharing.parents,
                active_depth: self.sharing.active.len(),
                candidate_active: self.sharing.active.contains(&i),
                reader_instance: self.sharing.instance,
                candidate_instance: self.sharing.thunks[i].instance,
                reader_arm: self.sharing.arm,
                owner_arm: owner,
                reader_scope: scope,
                owner_scope: &own_scope,
                reader_expr: e,
                candidate_expr: &self.sharing.thunks[i].expr,
                cached_depth: None,
            })?;
            Some((i, owner, own_scope))
        });
        let Some((i, owner, own_scope)) = found else {
            return Ok(None);
        };
        if let Some((row, ty, depth)) = &self.sharing.rows[i] {
            if admitted_cut(AdmissionAt {
                plan: self.b.plan(),
                parents: &self.sharing.parents,
                active_depth: self.sharing.active.len(),
                candidate_active: false,
                reader_instance: self.sharing.instance,
                candidate_instance: self.sharing.thunks[i].instance,
                reader_arm: self.sharing.arm,
                owner_arm: owner,
                reader_scope: scope,
                owner_scope: &own_scope,
                reader_expr: e,
                candidate_expr: &self.sharing.thunks[i].expr,
                cached_depth: Some(*depth),
            })
            .is_none()
            {
                return Ok(None);
            }
            if let Some(parent) = self.sharing.body_depths.last_mut() {
                *parent = (*parent).max(*depth);
            }
            return Ok(Some((*row, ty.clone())));
        }
        let thunk = self.sharing.thunks[i].clone();
        let (arm, instance) = (self.sharing.arm, self.sharing.instance);
        self.sharing.active.insert(i);
        self.sharing.body_depths.push(0);
        self.sharing.arm = owner;
        self.sharing.instance = thunk.instance;
        let mut asm = Asm::new();
        let result = expr::compile(self, &mut asm, &thunk.expr, &own_scope, &mut 0);
        let depth = self.sharing.body_depths.pop().unwrap() + 1;
        self.sharing.active.remove(&i);
        self.sharing.arm = arm;
        self.sharing.instance = instance;
        let ty = result?;
        let type_id = self.ty_id(&ty)?;
        let body = self.b.code(asm);
        let row = self.b.derive(&thunk.name, type_id, body);
        self.b.set_derive_unsettled(row, owner);
        if let Some(sites) = &mut self.sites {
            sites.derives.push(crate::sites::Declared {
                name: thunk.name,
                span: thunk.span,
                instance: thunk.instance,
                component: None,
            });
        }
        self.sharing.rows[i] = Some((row, ty.clone(), depth));
        if let Some(parent) = self.sharing.body_depths.last_mut() {
            *parent = (*parent).max(depth);
        }
        Ok(Some((row, ty)))
    }
}

fn same_reads(e: &Expr, reader: &Scope, owner: &Scope, cut: u32, bound: &mut Vec<String>) -> bool {
    use contract_types::Ref;
    if let Expr::Ident(name, _) = e {
        if bound.contains(name) {
            return true;
        }
        let rebase = |r| match r {
            Ref::Item(d) => d.checked_sub(cut).map(Ref::Item),
            Ref::Index(d) => d.checked_sub(cut).map(Ref::Index),
            Ref::Bound(d) => d.checked_sub(cut).map(Ref::Bound),
            Ref::Local(_) => None,
            r => Some(r),
        };
        return reader.lookup(name).and_then(|(r, _)| rebase(r))
            == owner.lookup(name).map(|(r, _)| r);
    }
    match e {
        Expr::Let {
            name, value, body, ..
        } => {
            if !same_reads(value, reader, owner, cut, bound) {
                return false;
            }
            bound.push(name.clone());
            let yes = same_reads(body, reader, owner, cut, bound);
            bound.pop();
            yes
        }
        Expr::Arrow { params, body, .. } => {
            let n = bound.len();
            bound.extend(params.iter().cloned());
            let yes = same_reads(body, reader, owner, cut, bound);
            bound.truncate(n);
            yes
        }
        Expr::Match {
            subject,
            var,
            some,
            none,
            ..
        } => {
            if !same_reads(subject, reader, owner, cut, bound)
                || !same_reads(none, reader, owner, cut, bound)
            {
                return false;
            }
            bound.push(var.clone());
            let yes = same_reads(some, reader, owner, cut, bound);
            bound.pop();
            yes
        }
        _ => e
            .operands()
            .all(|e| same_reads(e, reader, owner, cut, bound)),
    }
}

#[cfg(test)]
mod tests {
    use super::{admitted_cut, same_reads, AdmissionAt};
    use contract_syntax::{BinOp, Expr, Span};
    use contract_types::{Ref, Scope, Ty};
    use exact_plan::{builder::PlanBuilder, Opcode, Operand, Plan, RegionKind, Value};

    fn scope(names: Vec<(&str, Ref, Ty)>) -> Scope {
        let mut scope = Scope::default();
        scope.push(
            names
                .into_iter()
                .map(|(name, reference, ty)| (name.into(), reference, ty))
                .collect(),
        );
        scope
    }

    fn agrees(e: &Expr, reader: &Scope, owner: &Scope, cut: u32) -> bool {
        same_reads(e, reader, owner, cut, &mut Vec::new())
    }

    fn lower(source: &str) -> Plan {
        let file = contract_syntax::parse(source).unwrap();
        let checked = contract_types::check_all(&file, false, crate::tags::style, None).unwrap();
        let analysis = contract_analyze::check_all(&checked).unwrap();
        crate::lower(&checked, &analysis, None).unwrap()
    }

    fn opcodes(code: &[u8]) -> Vec<(Opcode, Option<u32>)> {
        let mut at = 0;
        let mut out = Vec::new();
        while at < code.len() {
            let op = Opcode::from_wire(code[at]).unwrap();
            at += 1;
            let mut index = None;
            for operand in op.operands() {
                let width = match operand {
                    Operand::U8 | Operand::Enum(_) => 1,
                    Operand::U16 => 2,
                    Operand::U32 | Operand::Str | Operand::Idx(_) => 4,
                    Operand::F64 => 8,
                };
                if op == Opcode::LoadDerive {
                    index = Some(u32::from_le_bytes(code[at..at + 4].try_into().unwrap()));
                }
                at += width;
            }
            out.push((op, index));
        }
        out
    }

    #[test]
    fn matcher_admits_bound_names_and_rebased_free_reads() {
        let s = Span::default();
        let mut reader = scope(vec![
            ("local", Ref::Local(0), Ty::Number),
            ("root", Ref::Slot(0), Ty::Number),
        ]);
        reader.push_region(Some(("item".into(), Ref::Item(0), Ty::String)));
        reader.push_none();
        let mut owner = scope(vec![
            ("local", Ref::Slot(1), Ty::Number),
            ("root", Ref::Slot(0), Ty::Number),
        ]);
        owner.push_region(Some(("item".into(), Ref::Item(0), Ty::Number)));
        let body = Expr::Let {
            name: "local".into(),
            value: Box::new(Expr::Ident("root".into(), s)),
            body: Box::new(Expr::Binary(
                BinOp::Add,
                Box::new(Expr::Ident("local".into(), s)),
                Box::new(Expr::Ident("item".into(), s)),
                s,
            )),
            span: s,
        };

        assert!(agrees(&body, &reader, &owner, 1));
        assert!(!agrees(&Expr::Ident("local".into(), s), &reader, &owner, 1));
    }

    #[test]
    fn matcher_refuses_unreachable_depths_and_different_owners() {
        let s = Span::default();
        let reader = scope(vec![("item", Ref::Item(0), Ty::String)]);
        let same_owner = scope(vec![("item", Ref::Item(0), Ty::String)]);
        let other_owner = scope(vec![("item", Ref::Slot(0), Ty::String)]);
        let item = Expr::Ident("item".into(), s);

        assert!(!agrees(&item, &reader, &same_owner, 1));
        assert!(!agrees(&item, &reader, &other_owner, 0));
    }

    #[test]
    fn child_derives_stay_shared_in_for_keys() {
        let plan = lower(
            r#"component App
  state names = ["a", "b"]
  view
    Child(names=names)

component Child
  props
    names: string[]
  derive prefix = names.join("-")
  derive keyOnly = names.join(":")
  view
    column
      text prefix
      for first in names key=`${prefix}:${first}`
        text first
      for second in names key=`${prefix}!${second}`
        text second
      for third in names key=keyOnly
        text third
"#,
        );
        let row = |name: &str| {
            plan.derives
                .iter()
                .position(|row| plan.str(row.name).contains(name))
                .unwrap() as u32
        };
        let prefix = row("prefix");
        let key_only = row("keyOnly");
        let keys = plan
            .regions
            .iter()
            .filter(|region| region.kind == RegionKind::Each)
            .map(|region| opcodes(plan.code(region.key)))
            .collect::<Vec<_>>();

        assert_eq!(keys.len(), 3);
        for key in &keys[..2] {
            assert!(key.contains(&(Opcode::LoadDerive, Some(prefix))));
            assert!(!key.iter().any(|(op, _)| *op == Opcode::Call));
        }
        assert_eq!(
            keys[2],
            vec![(Opcode::LoadDerive, Some(key_only)), (Opcode::Return, None)]
        );
        assert!(opcodes(plan.code(plan.derives[prefix as usize].body))
            .iter()
            .any(|(op, _)| *op == Opcode::Call));
        assert!(opcodes(plan.code(plan.derives[key_only as usize].body))
            .iter()
            .any(|(op, _)| *op == Opcode::Call));
    }

    #[test]
    fn shared_expression_admission_checks_the_whole_conjunction() {
        let mut plan = PlanBuilder::new(0, 1);
        let unit = plan.constant(&Value::Unit);
        let (_, outer) = plan.region(RegionKind::When, None, None, 0, unit, unit, 2);
        let (_, nested) = plan.region(RegionKind::Each, None, Some(outer[0]), 0, unit, unit, 1);
        let (_, sibling) = plan.region(RegionKind::Each, None, None, 1, unit, unit, 1);

        let mut owner_scope = scope(vec![("root", Ref::Slot(0), Ty::Number)]);
        owner_scope.push_region(Some(("item".into(), Ref::Item(0), Ty::Number)));
        let mut reader_scope = owner_scope.clone();
        reader_scope.push_none();
        let wrong_owner_scope = {
            let mut scope = scope(vec![("root", Ref::Slot(0), Ty::Number)]);
            scope.push_region(Some(("item".into(), Ref::Slot(1), Ty::Number)));
            scope
        };
        let s = Span::default();
        let expression = Expr::Let {
            name: "shadow".into(),
            value: Box::new(Expr::Ident("item".into(), s)),
            body: Box::new(Expr::Binary(
                BinOp::Add,
                Box::new(Expr::Ident("shadow".into(), s)),
                Box::new(Expr::Ident("root".into(), s)),
                s,
            )),
            span: s,
        };
        let other_expression = Expr::Ident("root".into(), s);
        let slot_expression = Expr::Ident("root".into(), s);
        let slot_owner_scope = scope(vec![("root", Ref::Slot(0), Ty::Number)]);
        let mut slot_reader_scope = slot_owner_scope.clone();
        slot_reader_scope.push_none();
        let mut deeper_slot_owner_scope = slot_reader_scope.clone();
        deeper_slot_owner_scope.push_none();
        let parents = [None, Some(0), None];
        let admitted = AdmissionAt {
            plan: plan.plan(),
            parents: &parents,
            active_depth: 1,
            candidate_active: false,
            reader_instance: 1,
            candidate_instance: 0,
            reader_arm: Some(nested[0]),
            owner_arm: Some(outer[0]),
            reader_scope: &reader_scope,
            owner_scope: &owner_scope,
            reader_expr: &expression,
            candidate_expr: &expression,
            cached_depth: Some(47),
        };

        assert_eq!(admitted_cut(admitted), Some(1));
        assert_eq!(
            admitted_cut(AdmissionAt {
                reader_arm: Some(sibling[0]),
                ..admitted
            }),
            None,
            "a sibling reader cannot reach the candidate's owner frame"
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                candidate_instance: 2,
                ..admitted
            }),
            None
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                candidate_active: true,
                ..admitted
            }),
            None
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                reader_arm: Some(outer[0]),
                owner_arm: Some(outer[0]),
                reader_scope: &slot_reader_scope,
                owner_scope: &slot_owner_scope,
                reader_expr: &slot_expression,
                candidate_expr: &slot_expression,
                ..admitted
            }),
            Some(1),
            "scope rebasing is independent of an equal reader and owner arm"
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                reader_scope: &slot_owner_scope,
                owner_scope: &slot_owner_scope,
                reader_expr: &slot_expression,
                candidate_expr: &slot_expression,
                ..admitted
            }),
            Some(0),
            "arm reachability is independent of an equal reader and owner scope depth"
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                reader_scope: &slot_reader_scope,
                owner_scope: &deeper_slot_owner_scope,
                reader_expr: &slot_expression,
                candidate_expr: &slot_expression,
                ..admitted
            }),
            None,
            "a reader shallower than its owner has no scope rebase cut"
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                owner_scope: &wrong_owner_scope,
                ..admitted
            }),
            None
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                candidate_expr: &other_expression,
                ..admitted
            }),
            None
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                cached_depth: Some(48),
                ..admitted
            }),
            None
        );
        assert_eq!(
            admitted_cut(AdmissionAt {
                active_depth: 48,
                cached_depth: None,
                ..admitted
            }),
            None
        );
    }
}
