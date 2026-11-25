//! Regions (`when`, `match`, `each`) and the slots a component instance
//! owns: an arm's or a row's, declared where it is built, and a late root
//! slot's (LLP 1017 P4c).

use super::{type_code, type_json, Em};
use crate::code::{self, Frame, Scope};
use exact_plan::{Plan, RegionKind};
use std::fmt::Write as _;

impl Em<'_> {
    pub(super) fn declare_derives(
        &mut self,
        top: &Scope,
        site_attrs: bool,
    ) -> Result<String, String> {
        let plan = self.plan;
        let mut body = String::new();
        for (i, r) in plan.derives.iter().enumerate() {
            if r.unsettled {
                continue;
            }
            let f = code::function(
                plan,
                plan.code(r.body),
                &code::evaluation(top, format!("derive {:?}", plan.str(r.name)), false),
                0,
                &mut self.uses,
            )
            .map_err(|e| format!("derive {}: {e}", plan.str(r.name)))?;
            let memo = self.uses.rt("memo");
            let _ = write!(
                body,
                "const d_{i}={memo}({f},{}{});",
                serde_json::to_string(&type_code(plan, r.ty)).unwrap(),
                if site_attrs {
                    format!(",{}", serde_json::to_string(plan.str(r.name)).unwrap())
                } else {
                    String::new()
                }
            );
        }
        body.push_str(&self.unsettled(None, &mut top.clone())?);
        Ok(body)
    }

    fn unsettled(&mut self, owner: Option<u32>, scope: &mut Scope) -> Result<String, String> {
        let rows: Vec<_> = self
            .plan
            .derives
            .iter()
            .enumerate()
            .filter(|(_, row)| row.unsettled && row.owner.map(|a| a.0) == owner)
            .collect();
        if rows.is_empty() {
            return Ok(String::new());
        }
        if owner.is_some() {
            let depth = scope.frames.len();
            let names = std::rc::Rc::make_mut(&mut scope.local_derives);
            for (ordinal, &(i, _)) in rows.iter().enumerate() {
                let name = if scope.capture {
                    format!("d_L{depth}_{ordinal}")
                } else {
                    format!("d_{i}")
                };
                names.insert(i, (name, self.mount_templates.scope_depth + 1));
            }
        }
        let mut inner = scope.clone();
        inner.unsettled = true;
        inner.evaluation = None;
        inner.capture = !self.site_attrs && !self.dev_reload;
        let mut templates = super::templates::Templates::computations(
            self.mount_templates.scope_depth + 1,
            "$budget",
        );
        let read = self.uses.budget("$atRead");
        let mut out = String::new();
        for (i, row) in rows {
            let f = code::function(
                self.plan,
                self.plan.code(row.body),
                &inner,
                0,
                &mut self.uses,
            )?;
            let name = scope
                .local_derives
                .get(&i)
                .map(|(name, _)| name.clone())
                .unwrap_or_else(|| format!("d_{i}"));
            let getter = if inner.capture {
                // Resolve captures when the value is read. Its resource or another
                // getter may be declared after this one in the same scope.
                let body = f.strip_prefix("($budget)=>").expect("at-read function");
                let expr = templates.expression(body.into());
                format!("$budget=>{expr}")
            } else {
                f
            };
            write!(out, "const {name}={read}({getter});").unwrap();
        }
        let (declarations, getters) = templates.render(out, scope.capture, "$read");
        Ok(declarations + &getters)
    }

    pub(super) fn region(&mut self, r: u32, parent: &str, scope: &Scope) -> Result<(), String> {
        let plan = self.plan;
        let row = &plan.regions[r as usize];
        match row.kind {
            RegionKind::When | RegionKind::Match => {
                let subject = self.region_subject(r, parent, scope)?;
                let depth = scope.frames.len();
                let bound = (row.kind == RegionKind::Match).then(|| format!("b{depth}"));
                let mut inner = scope.clone();
                inner.frames.push(Frame {
                    bound: bound.clone(),
                    ..Frame::default()
                });
                let mut bodies = Vec::new();
                for (k, arm) in row.arms.iter().map(|a| a.0).enumerate() {
                    let saved = std::mem::take(&mut self.out);
                    // `match`'s none arm holds no binding.
                    let mut sc = if k == 0 {
                        inner.clone()
                    } else {
                        let mut s = scope.clone();
                        s.frames.push(Frame::default());
                        s
                    };
                    sc.capture = !self.site_attrs && !self.dev_reload;
                    // The arm's own slots, started from their initializers
                    // each time it shows, dropped when it hides (LLP 1017 P4c).
                    // A tab panel's one arm (LLP 1075.003 §9.14): the dev
                    // loop carries its state by the panel's `id`, as the runner does.
                    let panel = self.dev_reload
                        && row
                            .parent
                            .is_some_and(|p| self.panels.binary_search(&p).is_ok());
                    let carry = panel.then(|| format!("{parent}.getAttribute(\"id\")"));
                    let path = self.instance_path(&mut sc, format!("\"/{r}:{k}\""), arm);
                    self.out.push_str(&path);
                    let decl = self.owned(arm, &format!("$a{arm}"), &mut sc, carry.as_deref())?;
                    self.out.push_str(&decl);
                    let outer_names = self.next_local_element;
                    self.next_local_element = (!self.site_attrs && !self.dev_reload).then_some(0);
                    let binds = !path.is_empty() || !decl.is_empty() || (k == 0 && bound.is_some());
                    if binds && sc.capture && path.is_empty() && (k != 0 || bound.is_none()) {
                        // This arm binds only getters and the row state object. A mount shared
                        // outside the arm takes either binding from the instance that calls it.
                        self.mount_templates.scope_depth += 1;
                        let children = self.children(self.sites.of_arm(arm), "p", &sc);
                        self.mount_templates.scope_depth -= 1;
                        children?;
                    } else {
                        self.template_scope(binds, |em| {
                            em.children(em.sites.of_arm(arm), "p", &sc)
                        })?;
                    }
                    self.next_local_element = outer_names;
                    let built = std::mem::replace(&mut self.out, saved);
                    let params = match (&bound, k) {
                        (Some(b), 0) => format!("p,{b}"),
                        _ => "p".into(),
                    };
                    bodies.push(self.mount_templates.mount(
                        &params,
                        built,
                        None,
                        Some(region_site(plan, r)),
                    ));
                }
                while bodies.len() < 2 {
                    bodies.push("0".into());
                }
                let f = self.uses.rt(if row.kind == RegionKind::When {
                    "when"
                } else {
                    "match"
                });
                let _ = write!(
                    self.out,
                    "{f}({parent},{subject},{},{});",
                    bodies[0], bodies[1]
                );
            }
            RegionKind::Each => self.each(r, parent, scope, None)?,
        }
        Ok(())
    }

    fn region_subject(&mut self, r: u32, parent: &str, scope: &Scope) -> Result<String, String> {
        let row = &self.plan.regions[r as usize];
        let kind = match row.kind {
            RegionKind::Each => "each",
            RegionKind::When => "when",
            RegionKind::Match => "match",
        };
        let scope = self.site_evaluation(
            scope,
            region_site(self.plan, r),
            format!("{kind} subject"),
            true,
        );
        let subject = self
            .mount_function(row.subject, &scope)
            .map_err(|x| format!("region {r}: {x}"))?;
        let Some(owners) = self.answers.status_region(r) else {
            return Ok(subject);
        };
        let owners = super::answers::status_owners(owners);
        let guard = self.uses.rt("statusSubject");
        Ok(format!(
            "(globalThis.__exactRender?{guard}({parent},{subject},{owners}):{subject})"
        ))
    }

    /// The declaration of arm `arm`'s own slots, as `name` (the enclosing
    /// instances' slots spread in), initialized in declaration order so a
    /// later slot can read an earlier one through `name`; empty when it
    /// owns none. `carry`, the dev loop's for a tab panel's arm: an
    /// expression naming the panel, by which each is carried across a reload.
    pub(super) fn owned(
        &mut self,
        arm: u32,
        name: &str,
        scope: &mut Scope,
        carry: Option<&str>,
    ) -> Result<String, String> {
        let plan = self.plan;
        let local_name = scope.capture.then(|| format!("$a{}", scope.frames.len()));
        let name = local_name.as_deref().unwrap_or(name);
        // A child's resources this arm's instances own (LLP 1035.005.004 D1).
        let requests: Vec<usize> = (plan.resources.iter().enumerate())
            .filter(|(_, r)| r.owner.is_some_and(|a| a.0 == arm))
            .map(|(i, _)| i)
            .collect();
        // And its mutations (D6).
        let sends: Vec<usize> = (plan.mutations.iter().enumerate())
            .filter(|(_, m)| m.owner.is_some_and(|a| a.0 == arm))
            .map(|(i, _)| i)
            .collect();
        let tasks: Vec<usize> = plan
            .timers
            .iter()
            .enumerate()
            .filter(|(_, t)| t.owner.is_some_and(|a| a.0 == arm))
            .map(|(i, _)| i)
            .collect();
        if self.answers.slots(arm).is_empty()
            && requests.is_empty()
            && sends.is_empty()
            && tasks.is_empty()
        {
            return self.unsettled(Some(arm), scope);
        }
        let mut decl = match &scope.rows {
            Some(outer) => format!("const {name}={{...{outer}}};"),
            None => format!("const {name}={{}};"),
        };
        scope.rows = Some(if scope.capture {
            code::lexical_operand(name, self.mount_templates.scope_depth + 1)
        } else {
            name.to_string()
        });
        scope.owned_arms.insert(arm);
        scope
            .row_slots
            .extend(self.answers.slots(arm).iter().copied());
        decl.push_str(&self.unsettled(Some(arm), scope)?);
        for &k in self.answers.slots(arm) {
            scope.row_slots.insert(k);
            let slot = &plan.slots[k];
            let init = code::expression(
                plan,
                plan.code(slot.init),
                &self.site_evaluation(
                    scope,
                    region_site(plan, plan.arms[arm as usize].region.0),
                    format!("state {:?} initializer", plan.str(slot.name)),
                    true,
                ),
                &mut self.uses,
            )
            .map_err(|x| format!("child state {}: {x}", plan.str(slot.name)))?;
            // Typed as a root slot is: its initializer and every write
            // conform, or the runner refuses (SlotType).
            let ty = serde_json::to_string(&type_code(plan, slot.ty)).unwrap();
            if let Some(panel) = carry {
                let quoted = serde_json::to_string(plan.str(slot.name)).unwrap();
                let declared = type_json(plan, slot.ty);
                let _ = write!(
                    decl,
                    "{name}[{k}]=$devSig({quoted}+\"@\"+{panel},{init},{ty},{declared});"
                );
                continue;
            }
            let sig = self.uses.rt("sig");
            let slot_name = string_name(plan, slot, scope);
            let k = scope.operand(k.to_string());
            let _ = write!(decl, "{name}[{k}]={sig}({init},{ty}{slot_name});");
        }
        // Asked after the instance's state is initialized (D2).
        let path = scope.path.clone().unwrap_or_else(|| "\"\"".into());
        for i in requests {
            decl.push_str(&super::resources::owned(
                plan,
                i,
                name,
                &path,
                scope,
                &mut self.uses,
            )?);
        }
        // Made after its requests, which its sends refresh.
        for i in sends {
            decl.push_str(&super::mutations::owned(
                plan,
                i,
                name,
                scope,
                &mut self.uses,
            )?);
        }
        for i in tasks {
            decl.push_str(&super::timers::task(plan, i, scope, &mut self.uses)?);
        }
        Ok(decl)
    }

    /// Extend the path only where a descendant resource or task reads it.
    fn instance_path(&mut self, scope: &mut Scope, segment: String, arm: u32) -> String {
        if !self.path_arms[arm as usize] {
            return String::new();
        }
        let outer = scope.path.clone().unwrap_or_else(|| "\"\"".into());
        scope.path = Some(format!("$q{arm}"));
        format!("const $q{arm}={outer}+{segment};")
    }

    /// A `for`, or a virtualized list's rows when `list` carries the
    /// list's options (list.js `vl`).
    pub(super) fn each(
        &mut self,
        r: u32,
        parent: &str,
        scope: &Scope,
        list: Option<String>,
    ) -> Result<(), String> {
        let plan = self.plan;
        let row = &plan.regions[r as usize];
        let subject = self.region_subject(r, parent, scope)?;
        let arms: Vec<u32> = row.arms.iter().map(|a| a.0).collect();
        let depth = scope.frames.len();
        let (item, index) = (format!("i{depth}"), format!("x{depth}"));
        let mut inner = scope.clone();
        inner.frames.push(Frame {
            item: Some(item.clone()),
            index: Some(index.clone()),
            bound: None,
        });
        let before = self.uses.metered;
        let key = code::function(
            plan,
            plan.code(row.key),
            &Scope {
                evaluation: None,
                ..inner.clone()
            },
            0,
            &mut self.uses,
        )
        .map_err(|x| format!("region {r} key: {x}"))?;
        let pure = crate::reads::pure_key(&key, &item, &index);
        let key = code::called_expression(code::evaluated(
            key,
            &self.site_evaluation(&inner, region_site(plan, r), "each key".into(), true),
            before,
            &mut self.uses,
        ));
        // The row's own slots, started from their initializers when
        // the row is created and kept with its key (LLP 1017 P4c).
        // The row's identity (`$k`, rows.js `rowKey`) names its instances.
        let path = if self.path_arms[arms[0] as usize] && list.is_none() {
            let row_path = self.uses.rt("rowPath");
            self.instance_path(&mut inner, format!("\"/{r}=\"+{row_path}($k)"), arms[0])
        } else {
            String::new()
        };
        let row_key = if path.is_empty() { "" } else { ",$k" };
        let rows_decl = self.owned(arms[0], &format!("$r{r}"), &mut inner, None)?;
        let saved = std::mem::take(&mut self.out);
        self.out.push_str(&path);
        self.out.push_str(&rows_decl);
        self.template_scope(true, |em| {
            em.children(em.sites.of_arm(arms[0]), "p", &inner)
        })?;
        let built = std::mem::replace(&mut self.out, saved);
        match list {
            Some(opts) => {
                let inputs = self.row_inputs(r, scope)?;
                let text = self.logical_children(self.sites.of_arm(arms[0]), &inner)?;
                let _ = write!(
                    self.out,
                    "$vl({parent},{subject},({item},{index})=>{key},(p,{item},{index})=>{{{}}},{{...{opts},{inputs},text:({item},{index})=>{{{}}}}});",
                    crate::hoist::mark(&built),
                    crate::hoist::mark(&format!("{rows_decl}return {text};"))
                );
            }
            None => {
                let each = self.uses.rt("each");
                let pure = if pure { ",1" } else { "" };
                // A development build names the region, for inspect mode's
                // instance paths (rt.js `Eaches`, LLP 1122 T2b).
                let pure = match (self.dev_reload, pure) {
                    (true, "") => format!(",0,{r}"),
                    (true, p) => format!("{p},{r}"),
                    (false, p) => p.to_string(),
                };
                let params = format!("p,{item},{index}{row_key}");
                let row = if inner.capture && self.next_local_element.is_some() {
                    self.mount_templates
                        .mount(&params, built, None, Some(region_site(plan, r)))
                } else {
                    format!("({params})=>{{{}}}", crate::hoist::mark(&built))
                };
                let _ = write!(
                    self.out,
                    "{each}({parent},{subject},({item},{index})=>{key},{row}{pure});"
                );
            }
        }
        Ok(())
    }
}

/// A root slot's signal, `s_<i>`, from its initializer.
pub(super) fn root_slot(
    plan: &Plan,
    i: usize,
    top: &Scope,
    dev_reload: bool,
    uses: &mut code::Uses,
    body: &mut String,
) -> Result<(), String> {
    let r = &plan.slots[i];
    let init = code::expression(
        plan,
        plan.code(r.init),
        &code::evaluation(
            top,
            format!("state {:?} initializer", plan.str(r.name)),
            false,
        ),
        uses,
    )
    .map_err(|e| format!("slot {}: {e}", plan.str(r.name)))?;
    let ty = serde_json::to_string(&type_code(plan, r.ty)).unwrap();
    // A persisted slot starts from the store's value where it fits, and is
    // kept after each commit (LLP 1116 D5, persist.js).
    let quoted = serde_json::to_string(plan.str(r.name)).unwrap();
    let scope = if r.persist == exact_plan::PersistScope::Window {
        ",true"
    } else {
        ""
    };
    let init = if r.is_persisted() {
        format!("$stored({quoted},{init},{}{scope})", type_json(plan, r.ty))
    } else {
        init
    };
    // A late slot's value is its initializer's as the boot starts (rt.js `late`)
    let init = if r.late { format!("()=>{init}") } else { init };
    if dev_reload {
        let make = if r.late { "$devLate" } else { "$devSig" };
        let declared = type_json(plan, r.ty);
        let _ = write!(body, "const s_{i}={make}({quoted},{init},{ty},{declared});");
    } else {
        let make = uses.rt(if r.late { "late" } else { "sig" });
        let name = string_name(plan, r, top);
        let _ = write!(body, "const s_{i}={make}({init},{ty}{name});");
    }
    if r.is_persisted() {
        let _ = write!(
            body,
            "$keep(s_{i},{quoted},{}{scope});",
            type_json(plan, r.ty)
        );
    }
    Ok(())
}

/// A string slot's name, as `sig`'s third argument: a write past MAX_STRING
/// is refused naming it (the runner's StringTooLong, LLP 1090 D6).
fn string_name(plan: &Plan, slot: &exact_plan::SlotsRow, scope: &Scope) -> String {
    if type_code(plan, slot.ty) == "s" {
        format!(
            ",{}",
            scope.operand(serde_json::to_string(slot.declared_name(plan)).unwrap())
        )
    } else {
        String::new()
    }
}

fn region_site(plan: &Plan, mut region: u32) -> u32 {
    loop {
        let row = &plan.regions[region as usize];
        if let Some(parent) = row.parent {
            return parent.0;
        }
        let Some(arm) = row.arm else {
            return 0;
        };
        region = plan.arms[arm.0 as usize].region.0;
    }
}
