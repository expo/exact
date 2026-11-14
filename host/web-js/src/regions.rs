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
        body.push_str(&self.unsettled(None, top)?);
        Ok(body)
    }

    pub(super) fn unsettled(
        &mut self,
        owner: Option<u32>,
        scope: &Scope,
    ) -> Result<String, String> {
        let mut out = String::new();
        for (i, row) in self.plan.derives.iter().enumerate() {
            if !row.unsettled || row.owner.map(|a| a.0) != owner {
                continue;
            }
            let mut inner = scope.clone();
            inner.unsettled = true;
            inner.evaluation = None;
            let f = code::function(
                self.plan,
                self.plan.code(row.body),
                &inner,
                0,
                &mut self.uses,
            )?;
            let enter = self.uses.budget("$enter");
            write!(out, "const d_{i}=($budget={{s:0,n:0,d:0}},$pc)=>{{{enter}($budget,$pc);try{{return ({f})($budget)}}finally{{$budget.d--}}}};").unwrap();
        }
        Ok(out)
    }

    pub(super) fn region(&mut self, r: u32, parent: &str, scope: &Scope) -> Result<(), String> {
        let plan = self.plan;
        let row = &plan.regions[r as usize];
        let subject = self
            .f(
                row.subject,
                &code::evaluation(
                    scope,
                    format!(
                        "site {} {} subject",
                        region_site(plan, r),
                        match row.kind {
                            RegionKind::Each => "each",
                            RegionKind::When => "when",
                            RegionKind::Match => "match",
                        }
                    ),
                    true,
                ),
            )
            .map_err(|x| format!("region {r}: {x}"))?;
        let subject = self.status_subject(r, parent, subject);
        let arms: Vec<u32> = row.arms.iter().map(|a| a.0).collect();
        match row.kind {
            RegionKind::When | RegionKind::Match => {
                let bound = (row.kind == RegionKind::Match).then(|| format!("b{r}"));
                let mut inner = scope.clone();
                inner.frames.push(Frame {
                    bound: bound.clone(),
                    ..Frame::default()
                });
                let mut bodies = Vec::new();
                for (k, arm) in arms.iter().enumerate() {
                    let saved = std::mem::take(&mut self.out);
                    // `match`'s none arm holds no binding.
                    let mut sc = if k == 0 {
                        inner.clone()
                    } else {
                        let mut s = scope.clone();
                        s.frames.push(Frame::default());
                        s
                    };
                    // The arm's own slots, started from their initializers
                    // each time it shows, dropped when it hides (LLP 1017 P4c).
                    // A tab panel's one arm (LLP 1075.003 §9.14): the dev
                    // loop carries its state by the panel's `id`, as the runner does.
                    let panel = self.dev_reload
                        && row
                            .parent
                            .is_some_and(|p| self.panels.binary_search(&p).is_ok());
                    let carry = panel.then(|| format!("{parent}.getAttribute(\"id\")"));
                    let decl = self.owned(*arm, &format!("$a{arm}"), &mut sc, carry.as_deref())?;
                    self.out.push_str(&decl);
                    self.children(self.sites.of_arm(*arm), "p", &sc)?;
                    let built = std::mem::replace(&mut self.out, saved);
                    let params = match (&bound, k) {
                        (Some(b), 0) => format!("(p,{b})"),
                        _ => "p".into(),
                    };
                    bodies.push(format!("{params}=>{{{built}}}"));
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

    fn status_subject(&mut self, region: u32, parent: &str, subject: String) -> String {
        let Some(owners) = self.answers.status_region(region) else {
            return subject;
        };
        let owners = super::answers::status_owners(owners);
        let guard = self.uses.rt("statusSubject");
        format!("(globalThis.__exactRender?{guard}({parent},{subject},{owners}):{subject})")
    }

    /// The declaration of arm `arm`'s own slots, as `name` (the enclosing
    /// instances' slots spread in), initialized in declaration order so a
    /// later slot can read an earlier one through `name`; empty when it
    /// owns none. `carry`, the dev loop's for a tab panel's arm: an
    /// expression naming the panel, by which each is carried across a reload.
    fn owned(
        &mut self,
        arm: u32,
        name: &str,
        scope: &mut Scope,
        carry: Option<&str>,
    ) -> Result<String, String> {
        let plan = self.plan;
        if self.answers.slots(arm).is_empty() {
            return self.unsettled(Some(arm), scope);
        }
        let mut decl = match &scope.rows {
            Some(outer) => format!("const {name}={{...{outer}}};"),
            None => format!("const {name}={{}};"),
        };
        scope.rows = Some(name.to_string());
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
                &code::evaluation(
                    scope,
                    format!(
                        "site {} state {:?} initializer",
                        region_site(plan, plan.arms[arm as usize].region.0),
                        plan.str(slot.name)
                    ),
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
            let slot_name = string_name(plan, slot);
            let _ = write!(decl, "{name}[{k}]={sig}({init},{ty}{slot_name});");
        }
        Ok(decl)
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
        let subject = self
            .f(
                row.subject,
                &code::evaluation(
                    scope,
                    format!(
                        "site {} {} subject",
                        region_site(plan, r),
                        match row.kind {
                            RegionKind::Each => "each",
                            RegionKind::When => "when",
                            RegionKind::Match => "match",
                        }
                    ),
                    true,
                ),
            )
            .map_err(|x| format!("region {r}: {x}"))?;
        let subject = self.status_subject(r, parent, subject);
        let arms: Vec<u32> = row.arms.iter().map(|a| a.0).collect();
        let (item, index) = (format!("i{r}"), format!("x{r}"));
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
            &code::evaluation(
                &inner,
                format!("site {} each key", region_site(plan, r)),
                true,
            ),
            before,
            &mut self.uses,
        ));
        // The row's own slots, started from their initializers when
        // the row is created and kept with its key (LLP 1017 P4c).
        let rows_decl = self.owned(arms[0], &format!("$r{r}"), &mut inner, None)?;
        let saved = std::mem::take(&mut self.out);
        self.out.push_str(&rows_decl);
        self.children(self.sites.of_arm(arms[0]), "p", &inner)?;
        let built = std::mem::replace(&mut self.out, saved);
        match list {
            Some(opts) => {
                let inputs = self.row_inputs(r, scope)?;
                let _ = write!(
                    self.out,
                    "$vl({parent},{subject},({item},{index})=>{key},(p,{item},{index})=>{{{built}}},{{...{opts},{inputs}}});"
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
                let _ = write!(
                    self.out,
                    "{each}({parent},{subject},({item},{index})=>{key},(p,{item},{index})=>{{{built}}}{pure});"
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
    let init = if r.persist {
        format!("$stored({quoted},{init},{})", type_json(plan, r.ty))
    } else {
        init
    };
    if dev_reload {
        let _ = write!(
            body,
            "const s_{i}=$devSig({quoted},{init},{ty},{});",
            type_json(plan, r.ty)
        );
    } else {
        let sig = uses.rt("sig");
        let name = string_name(plan, r);
        let _ = write!(body, "const s_{i}={sig}({init},{ty}{name});");
    }
    if r.persist {
        let _ = write!(body, "$keep(s_{i},{quoted},{});", type_json(plan, r.ty));
    }
    Ok(())
}

/// A string slot's name, as `sig`'s third argument: a write past MAX_STRING
/// is refused naming it (the runner's StringTooLong, LLP 1090 D6).
fn string_name(plan: &Plan, slot: &exact_plan::SlotsRow) -> String {
    if type_code(plan, slot.ty) == "s" {
        format!(",{}", serde_json::to_string(plan.str(slot.name)).unwrap())
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
