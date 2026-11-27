//! What a realized node's bindings last emitted.
use super::*;

impl NodeInst {
    /// Where binding `i` of `row` keeps its last value in `last`: past the
    /// bindings before it that can change. A plan's bindings are mostly
    /// constant (euxy's first screen: 3,652 of 3,974), and those are kept once
    /// for the plan's life in [`SiteIndex::constants`], not per instance.
    pub(super) fn slot(sites: &SiteIndex, row: &exact_plan::NodesRow, i: usize) -> usize {
        (0..i).filter(|&j| !Self::constant(sites, row, j)).count()
    }

    pub(super) fn constant(sites: &SiteIndex, row: &exact_plan::NodesRow, i: usize) -> bool {
        sites.data.deps.bindings[(row.bindings.start + i as u32) as usize].is_constant()
    }

    /// Record what a nonconstant binding last emitted.
    pub(super) fn set_emitted(&mut self, slot: usize, value: Option<Value>) {
        Rc::make_mut(self.last.as_mut().expect("binding slots"))[slot] = value;
    }

    /// The value binding `i` last emitted.
    pub(super) fn emitted(&self, plan: &Plan, sites: &SiteIndex, i: usize) -> Option<Value> {
        let row = plan.node(self.node);
        if Self::constant(sites, &row, i) {
            let b = row.bindings.start + i as u32;
            return sites.constants.borrow().get(&b).cloned();
        }
        self.last.as_ref().expect("binding slots")[Self::slot(sites, &row, i)].clone()
    }

    /// The value a style binding last emitted.
    pub(super) fn bound_style(
        &self,
        plan: &Plan,
        sites: &SiteIndex,
        style: exact_kernel::StyleId,
    ) -> Option<Value> {
        self.bound(plan, sites, BindingKind::Style, style as u16)
    }

    /// The value a prop binding last emitted.
    pub(super) fn bound_prop(
        &self,
        plan: &Plan,
        sites: &SiteIndex,
        prop: exact_kernel::PropId,
    ) -> Option<Value> {
        self.bound(plan, sites, BindingKind::Prop, prop as u16)
    }

    /// Whether binding `b` is currently trapped on this node.
    pub(super) fn is_trapped(&self, b: exact_plan::BindingsId) -> bool {
        self.rare.as_ref().is_some_and(|r| r.trapped.contains(&b.0))
    }

    fn bound(&self, plan: &Plan, sites: &SiteIndex, kind: BindingKind, id: u16) -> Option<Value> {
        let bindings = plan.node(self.node).bindings;
        let mut fallback = None;
        for i in (0..bindings.len as usize).rev() {
            let b = plan.binding(exact_plan::BindingsId(bindings.start + i as u32));
            if b.kind != kind || b.id != id {
                continue;
            }
            let Some(v) = self.emitted(plan, sites, i) else {
                continue;
            };
            if !matches!(v, Value::Option(None)) {
                return Some(v);
            }
            if fallback.is_none() {
                fallback = Some(v);
            }
        }
        fallback
    }

    /// Style rows that have more than one binding on `row` (an implied
    /// fallback under an authored conditional binding).
    pub(super) fn layered_styles(
        plan: &Plan,
        row: &exact_plan::NodesRow,
    ) -> exact_kernel::StyleMask {
        let mut seen = exact_kernel::StyleMask::default();
        let mut layered = exact_kernel::StyleMask::default();
        for b in row.bindings.iter() {
            let b = plan.binding(b);
            if b.kind == BindingKind::Style {
                if let Some(style) = exact_kernel::StyleId::from_bit(b.id as u32) {
                    if seen.has(style) {
                        layered.set(style);
                    } else {
                        seen.set(style);
                    }
                }
            }
        }
        layered
    }

    /// Emit one evaluated style binding (`i`) onto `patch` or `u.ops`.
    pub(super) fn apply_style_binding(
        &mut self,
        u: &mut Update<'_>,
        i: usize,
        binding: &exact_plan::BindingsRow,
        value: &Value,
        themed_plan: bool,
        patch: &mut Option<Box<StyleProps>>,
    ) -> Result<(), InstanceError> {
        let plan = u.env.plan;
        // A missing class row removes its declaration. An authored CSS clear
        // keeps declaration intent while inheriting or using the initial value.
        if matches!(value, Value::Option(None)) || bridge::unsets(binding.id, value) {
            let style =
                exact_kernel::StyleId::from_bit(binding.id as u32).expect("known to the bridge");
            let mut mask = exact_kernel::StyleMask::default();
            mask.set(style);
            u.ops.push(Op::ClearStyle {
                id: self.view,
                mask,
                written: binding.written && !matches!(value, Value::Option(None)),
            });
            self.unthemed(u, i, style);
            return Ok(());
        }
        let p = patch.get_or_insert_with(Box::default);
        let (family_value, family_id) =
            bridge::family_binding(plan, u.fonts.as_deref(), binding, value);
        // A row a runtime family or a refusal sets reads no token:
        // the index forgets what it read before.
        let row = exact_kernel::StyleId::from_bit(binding.id as u32);
        let styled = if let Some(id) = family_id {
            if let (true, Some(row)) = (themed_plan, row) {
                self.unthemed(u, i, row);
            }
            bridge::set_runtime_font_style(
                p,
                binding,
                id,
                plan.stacks.len() + u.fonts.as_ref().unwrap().borrow().names().len(),
            )
        } else if let Err(e) = bridge::inherited_varying(
            binding,
            family_value,
            |name| u.sites.varying_type(name),
            || u.sites.has_children(self.node),
        ) {
            if let (true, Some(row)) = (themed_plan, row) {
                self.unthemed(u, i, row);
            }
            Err(e)
        } else {
            exact_kernel::style::theme::take_named();
            let styled = bridge::set_binding_style(p, binding, family_value, plan);
            if themed_plan {
                self.index(u, i, binding, family_value, styled.is_ok());
            }
            styled
        };
        match styled {
            // Unlinked, the plan names no rule: it was refused at boot (LLP 1047.001 D5).
            Ok(exact_kernel::StyleId::Animation) => {
                if let Some(resolve) = u.sites.resolve_keyframes {
                    u.notes
                        .extend(resolve(&u.sites.keyframes, &mut p.animation));
                }
            }
            // An exit names keyframes as `animation` does (LLP 1063).
            Ok(exact_kernel::StyleId::ExitAnimation) => {
                if let Some(resolve) = u.sites.resolve_keyframes {
                    u.notes
                        .extend(resolve(&u.sites.keyframes, &mut p.rare.exit_animation));
                }
            }
            // So do a route's entrance and exit (LLP 1013.001 D1).
            Ok(
                row @ (exact_kernel::StyleId::ViewTransitionNew
                | exact_kernel::StyleId::ViewTransitionOld),
            ) => {
                if let Some(resolve) = u.sites.resolve_keyframes {
                    let list = match row {
                        exact_kernel::StyleId::ViewTransitionNew => &mut p.rare.view_transition_new,
                        _ => &mut p.rare.view_transition_old,
                    };
                    u.notes.extend(resolve(&u.sites.keyframes, list));
                }
            }
            Ok(_) => {}
            Err(bridge::BridgeError::Style(_) | bridge::BridgeError::StyleKind { .. }) => {
                let style = exact_kernel::StyleId::from_bit(binding.id as u32)
                    .expect("known to the bridge");
                let mut mask = exact_kernel::StyleMask::default();
                mask.set(style);
                u.ops.push(Op::ClearStyle {
                    id: self.view,
                    mask,
                    written: binding.written,
                });
                let name = style.name().replace('_', "-");
                u.notes.push(super::invalid(self.view, &name, value));
            }
            Err(e) => return Err(InstanceError::Bridge(e)),
        }
        Ok(())
    }

    /// Resolve every dirty layered style row on `row`: the latest non-`none`
    /// (or trapped) binding wins, keeping its own `written` provenance.
    pub(super) fn emit_layered_styles(
        &mut self,
        u: &mut Update<'_>,
        row: &exact_plan::NodesRow,
        mut dirty: exact_kernel::StyleMask,
        changed: &[usize],
        themed_plan: bool,
        patch: &mut Option<Box<StyleProps>>,
    ) -> Result<(), InstanceError> {
        let plan = u.env.plan;
        for b in row.bindings.iter() {
            let b = plan.binding(b);
            if b.kind != BindingKind::Style {
                continue;
            }
            let Some(style) = exact_kernel::StyleId::from_bit(b.id as u32) else {
                continue;
            };
            if !dirty.has(style) {
                continue;
            }
            dirty.clear(style);
            let mut winner: Option<(usize, exact_plan::BindingsRow, Option<Value>)> = None;
            let mut affected = false;
            let mut first_idx = 0usize;
            for i in (0..row.bindings.len as usize).rev() {
                let bid = exact_plan::BindingsId(row.bindings.start + i as u32);
                let binding = plan.binding(bid);
                if binding.kind != BindingKind::Style || binding.id != style as u16 {
                    continue;
                }
                first_idx = i;
                if winner.is_some() {
                    continue;
                }
                if changed.contains(&i) {
                    affected = true;
                }
                if self.is_trapped(bid) {
                    winner = Some((i, binding, None));
                } else if let Some(val) = self.emitted(plan, u.sites, i) {
                    if !matches!(val, Value::Option(None)) {
                        winner = Some((i, binding, Some(val)));
                    }
                }
            }
            if !affected {
                continue;
            }
            let win_idx = winner.as_ref().map(|(i, ..)| *i);
            for (j, bid) in row.bindings.iter().enumerate() {
                let binding = plan.binding(bid);
                if binding.kind == BindingKind::Style
                    && binding.id == style as u16
                    && Some(j) != win_idx
                {
                    self.themed.set(j, false, false);
                }
            }
            match winner {
                Some((i, binding, Some(val))) => {
                    self.apply_style_binding(u, i, &binding, &val, themed_plan, patch)?;
                }
                Some((i, binding, None)) => {
                    self.unset_trapped(u, i, &binding);
                }
                None => {
                    let mut mask = exact_kernel::StyleMask::default();
                    mask.set(style);
                    u.ops.push(Op::ClearStyle {
                        id: self.view,
                        mask,
                        written: false,
                    });
                    self.unthemed(u, first_idx, style);
                }
            }
        }
        Ok(())
    }
}

/// The string node `n`'s `prop` is bound to, when that binding is a literal.
pub(super) fn literal_str(plan: &Plan, n: NodesId, prop: exact_kernel::PropId) -> Option<&str> {
    plan.node(n).bindings.iter().find_map(|b| {
        let b = plan.binding(b);
        if b.kind != BindingKind::Prop || b.id != prop as u16 {
            return None;
        }
        crate::uses::constant_str(plan, plan.code(b.expr))
    })
}
