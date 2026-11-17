//! `difftest lowering`: translation validation of the real compiler.
//!
//! For each case the Rust compiler's plan supplies the bytecode of every
//! derive, root slot initializer, resource argument and action body. A
//! generated Lean module (`Contract.LowerCheck`) decodes each body, runs it
//! on the VM model (`Contract.Vm`) in the configurations the semantics
//! reaches along the case's script, and compares the result with `eval`
//! (an action: `exec`'s effects); it also compares each body with what
//! `Contract.Lower` compiles from the semantics' syntax, instruction for
//! instruction. A disagreement between the VM run and the semantics is a
//! divergence; a structural difference is reported, not a failure.

use crate::leanrun::{self, LeanCase};
use crate::script::Case;
use crate::{clone_case, compile, prepare, work_dir, Verdict};
use contract::lean::string;
use contract::lean::SharingContext;
use exact_plan::{Opcode, Operand, Plan, Stdlib};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

/// One case ready for the Lean module.
struct Ready {
    lean: LeanCase,
    plan: String,
}

/// A case, ready for Lean or why not.
type Entry = (Case, Option<Ready>, Option<String>);

/// What one case's check reported.
#[derive(Default)]
struct Report {
    lines: Vec<String>,
    sum: [u64; 7],
    boot_refused: bool,
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The plan as a `LowerCheck.PlanInfo` term.
fn plan_info(plan: &Plan, shared: &SharingContext) -> String {
    let region_roster_matches = shared.regions.len() == plan.regions.len()
        && shared
            .regions
            .iter()
            .enumerate()
            .all(|(index, source)| source.matches(plan, index));
    let owner_position = |owner: exact_plan::ArmsId| {
        if !region_roster_matches {
            return None;
        }
        let arm = plan.arms.get(owner.0 as usize)?;
        let region = plan.regions.get(arm.region.0 as usize)?;
        let offset = owner.0.checked_sub(region.arms.start)?;
        (offset < region.arms.len).then_some((arm.region.0, offset))
    };
    let strings: Vec<String> = plan.strings.iter().map(string).collect();
    let types: Vec<String> = plan
        .types
        .iter()
        .map(|t| format!("({}, {})", string(plan.str(t.name)), t.fields.len))
        .collect();
    let slots: Vec<String> = plan
        .slots
        .iter()
        .map(|s| {
            let owner = s
                .owner
                .and_then(owner_position)
                .map_or(".none".into(), |(region, _)| format!(".some {region}"));
            format!("({}, {owner})", string(plan.str(s.name)))
        })
        .collect();
    let derives: Vec<String> = plan
        .derives
        .iter()
        .map(|d| string(plan.str(d.name)))
        .collect();
    let resources: Vec<String> = plan
        .resources
        .iter()
        .map(|r| {
            let owner = r
                .owner
                .and_then(owner_position)
                .map_or(".none".into(), |(region, arm)| {
                    format!(".some ({region}, {arm})")
                });
            format!("({}, {owner})", string(plan.str(r.name)))
        })
        .collect();
    let mutations: Vec<String> = plan
        .mutations
        .iter()
        .map(|m| format!("({}, {})", string(plan.str(m.name)), m.slot.0))
        .collect();
    let mut bodies = Vec::new();
    let body = |kind: &str, name: &str, code: &[u8], writes: &[u32]| {
        let w: Vec<String> = writes.iter().map(u32::to_string).collect();
        format!(
            "{{ kind := {kind}, name := {}, bytes := \"{}\", writes := [{}] }}",
            string(name),
            hex(code),
            w.join(", ")
        )
    };
    let mut executables = Vec::new();
    let plan_owner = |owner: Option<exact_plan::ArmsId>| {
        owner
            .and_then(owner_position)
            .map_or(".none".into(), |(region, arm)| {
                format!(".some ({region}, {arm})")
            })
    };
    for (row, d) in plan.derives.iter().enumerate() {
        let mut declaration = body(".derive", plan.str(d.name), plan.code(d.body), &[]);
        if d.unsettled {
            let row_owner = d
                .owner
                .and_then(owner_position)
                .map_or(".none".into(), |(region, _)| format!(".some {region}"));
            executables.push(format!(
                "some (\"{}\", {row_owner})",
                hex(plan.code(d.body))
            ));
            if let Some(source) = shared
                .expressions
                .iter()
                .find(|source| source.name == plan.str(d.name))
            {
                let source_owner = source
                    .owner
                    .map_or(".none".into(), |(tag, arm)| format!(".some ({tag}, {arm})"));
                declaration.pop();
                declaration.push_str(&format!(
                    ", source := some {}, executable := true, owner := {}, sourceOwner := {source_owner}, row := some {row} }}",
                    source.expr,
                    plan_owner(d.owner)
                ));
            }
        } else {
            executables.push(".none".into());
        }
        bodies.push(declaration);
    }
    for (i, s) in plan.slots.iter().enumerate() {
        let id = exact_plan::SlotsId(i as u32);
        // Owned slots initialize in their arm's frames; a late slot after
        // boot settlement; the router, locale and format slots are outside
        // the semantics.
        if s.owner.is_some()
            || s.late
            || plan.router == Some(id)
            || plan.locale == Some(id)
            || plan.format == Some(id)
            || plan.mutations.iter().any(|m| m.slot == id)
        {
            continue;
        }
        bodies.push(body(".slot", plan.str(s.name), plan.code(s.init), &[]));
    }
    for r in &plan.resources {
        let name = plan.str(r.name);
        // A list's arguments read its element as a parameter, which the
        // semantics binds as a local (`settleEach`): not this check's.
        if name.contains('#') || r.each.len != 0 {
            continue;
        }
        for (k, a) in r.args.iter().enumerate() {
            let code = plan.code(plan.arg(a).expr);
            bodies.push(body(&format!("(.resource {k})"), name, code, &[]));
        }
    }
    for a in &plan.actions {
        let writes: Vec<u32> = a.writes.iter().map(|w| plan.write(w).slot.0).collect();
        bodies.push(body(
            ".action",
            plan.str(a.name),
            plan.code(a.body),
            &writes,
        ));
    }
    let regions: Vec<String> = shared
        .regions
        .iter()
        .enumerate()
        .map(|(plan, source)| format!("({}, {plan})", source.tag))
        .collect();
    format!(
        "{{ strings := #[{}],\n    types := #[{}],\n    slots := [{}],\n    derives := [{}],\n    executables := [{}],\n    regions := [{}],\n    regionRosterMatches := {},\n    resources := [{}],\n    mutations := [{}],\n    bodies := [{}] }}",
        strings.join(", "),
        types.join(", "),
        slots.join(", "),
        derives.join(", "),
        executables.join(", "),
        regions.join(", "),
        region_roster_matches,
        resources.join(", "),
        mutations.join(", "),
        bodies.join(",\n      ")
    )
}

/// The roster and the opcode table, as the plan crate has them.
fn tables() -> String {
    let roster: Vec<String> = Stdlib::ALL
        .iter()
        .map(|f| format!("({}, {})", string(f.name()), f.arity()))
        .collect();
    let mut ops = Vec::new();
    for w in 0..=255u8 {
        let Some(op) = Opcode::from_wire(w) else {
            continue;
        };
        let layout: Vec<&str> = op
            .operands()
            .iter()
            .map(|o| match o {
                Operand::U8 => ".u8",
                Operand::U16 => ".u16",
                Operand::U32 => ".u32",
                Operand::F64 => ".f64",
                Operand::Str => ".str",
                Operand::Enum(_) => ".enum",
                Operand::Idx(_) => ".idx",
            })
            .collect();
        ops.push(format!(
            "({}, [{}])",
            string(&format!("{op:?}")),
            layout.join(", ")
        ));
    }
    format!(
        "def roster : Array (String × Nat) := #[{}]\n\ndef rustOpcodes : List (String × List Contract.Vm.Operand) := [{}]\n\n",
        roster.join(", "),
        ops.join(", ")
    )
}

fn module(cases: &[Ready]) -> String {
    let mut s = String::from(
        "import Contract.LowerCheck\nopen Contract\n\nset_option maxRecDepth 100000\nset_option maxHeartbeats 0\n\n",
    );
    s.push_str(&tables());
    for c in cases {
        let n = &c.lean.name;
        s.push_str(&c.lean.program);
        let _ = write!(
            s,
            "\ndef {n}_oracle : Oracle := OracleText.parse! {}\n\ndef {n}_events : List Observe.ScriptEvent := [{}]\n\ndef {n}_plan : LowerCheck.PlanInfo :=\n  {}\n\n",
            string(&c.lean.oracle),
            c.lean.events.join(", "),
            c.plan
        );
    }
    s.push_str("def main : IO Unit := do\n  for l in LowerCheck.checkOpcodes rustOpcodes do IO.println l\n");
    for (i, c) in cases.iter().enumerate() {
        let _ = writeln!(
            s,
            "  IO.println \"#case {i}\"\n  for l in LowerCheck.check {n} {n}_oracle {n}_events {n}_plan roster do IO.println l",
            n = c.lean.name
        );
    }
    s
}

/// Run one batch in one Lean process: the opcode lines, then each case's.
fn run_batch(cases: &[Ready], dir: &Path, tag: &str) -> Result<(Vec<String>, Vec<Report>), String> {
    let text = module(cases);
    let text = leanrun::run_module(&text, dir, tag)?.join("\n");
    let mut head = Vec::new();
    let mut reports: Vec<Report> = Vec::new();
    for line in text.lines() {
        if line.starts_with("#case ") {
            reports.push(Report::default());
            continue;
        }
        let Some(r) = reports.last_mut() else {
            head.push(line.to_string());
            continue;
        };
        if let Some(rest) = line.strip_prefix("sum ") {
            for (k, n) in rest.split(' ').enumerate().take(7) {
                r.sum[k] = n.parse().unwrap_or(0);
            }
        } else if line == "# boot refused" {
            r.boot_refused = true;
        } else {
            r.lines.push(line.to_string());
        }
    }
    if reports.len() != cases.len() {
        return Err(format!(
            "{}: expected {} cases, got {}",
            dir.join(format!("{tag}-{}.lean", std::process::id()))
                .display(),
            cases.len(),
            reports.len()
        ));
    }
    Ok((head, reports))
}

/// A STRUCT, REFUSED or BOTH line's category: the body kind and the message,
/// names and indices left out.
fn category(line: &str) -> String {
    let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
    let mut words = rest.split(' ');
    let kind = words.next().unwrap_or("");
    let kind = kind.split('-').next().unwrap_or(kind);
    let _name = words.next();
    let msg: Vec<&str> = words.collect();
    let msg = msg.join(" ");
    let msg = match tag {
        // "<what>: vm traps T / semantics fails: E": keep T and E.
        "BOTH" => match line.split_once(": ") {
            Some((_, d)) => d
                .split('`')
                .enumerate()
                .map(|(i, p)| if i % 2 == 1 { "`…`" } else { p })
                .collect(),
            None => msg,
        },
        // "at <i>: rust X, lean Y": keep the two opcodes.
        "STRUCT" => match msg.split_once(": ") {
            Some((_, d)) => d
                .split(", ")
                .map(|part| {
                    let mut w = part.split(' ');
                    let side = w.next().unwrap_or("");
                    let op = w.next().unwrap_or("");
                    format!("{side} {op}")
                })
                .collect::<Vec<_>>()
                .join(", "),
            None => msg,
        },
        _ => msg
            .split('`')
            .enumerate()
            .map(|(i, p)| if i % 2 == 1 { "`…`" } else { p })
            .collect(),
    };
    format!("{tag} {kind}: {msg}")
}

/// Run the cases; print divergences and categories; whether all agreed.
pub fn run(cases: Vec<Case>, batch: usize, tag: &str) -> Result<bool, String> {
    let accounting = crate::accounting::run()?;
    leanrun::build()?;
    let mut prepared: Vec<Entry> = Vec::new();
    for (i, case) in cases.into_iter().enumerate() {
        let index = i % batch.max(1);
        match prepare(&case, index) {
            Ok(p) => {
                let plan = compile(&case).map_err(|e| format!("{}: {e}", case.name))?;
                let shared = contract::lean::sharing_context(&case.source, case.path.as_deref())
                    .map_err(|e| format!("{}: sharing expressions: {e}", case.name))?;
                let ready = Ready {
                    lean: clone_case(&p.lean),
                    plan: plan_info(&plan, &shared),
                };
                prepared.push((case, Some(ready), None));
            }
            Err(Verdict::Refused(e)) => prepared.push((case, None, Some(format!("refused: {e}")))),
            Err(Verdict::Emit(e)) => prepared.push((case, None, Some(format!("emit: {e}")))),
            Err(_) => prepared.push((case, None, Some("not prepared".into()))),
        }
    }
    let chunks: Vec<&[Entry]> = prepared.chunks(batch.max(1)).collect();
    let jobs = std::env::var("DIFFTEST_JOBS")
        .ok()
        .and_then(|j| j.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1))
        });
    type Batch = Result<(Vec<String>, Vec<Report>), String>;
    let results: Vec<std::sync::Mutex<Option<Batch>>> =
        chunks.iter().map(|_| std::sync::Mutex::new(None)).collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(chunks.len()) {
            scope.spawn(|| loop {
                let b = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(chunk) = chunks.get(b) else { break };
                // Each batch's own copies: `Ready` holds no shared state.
                let ready: Vec<Ready> = chunk
                    .iter()
                    .filter_map(|(_, r, _)| r.as_ref())
                    .map(|r| Ready {
                        lean: clone_case(&r.lean),
                        plan: r.plan.clone(),
                    })
                    .collect();
                let r = run_batch(&ready, &work_dir(), &format!("{tag}-{b}"));
                *results[b].lock().expect("result slot") = Some(r);
            });
        }
    });
    let dir = work_dir().join("failures");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut failed = false;
    let mut kept = 0;
    let mut opcodes_reported = false;
    let mut total = [0u64; 7];
    let (mut checked, mut skipped, mut boot_refused) = (0, 0, 0);
    let mut categories: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for (chunk, result) in chunks.iter().zip(results) {
        let (head, reports) = result
            .into_inner()
            .expect("result slot")
            .expect("every batch ran")?;
        if !opcodes_reported {
            for l in &head {
                println!("{l}");
                failed = true;
            }
            opcodes_reported = true;
        }
        let mut reports = reports.into_iter();
        for (case, ready, why) in chunk.iter() {
            if ready.is_none() {
                skipped += 1;
                if let Some(why) = why {
                    if !why.starts_with("refused") {
                        println!("SKIPPED {}: {why}", case.name);
                    }
                }
                continue;
            }
            let r = reports.next().expect("one report per prepared case");
            checked += 1;
            if r.boot_refused {
                boot_refused += 1;
            }
            for (t, n) in total.iter_mut().zip(r.sum) {
                *t += n;
            }
            let mut bad = false;
            for l in &r.lines {
                if l.starts_with("DIVERGE") || l.starts_with("UNDECODED") {
                    println!("{l}  [{}]", case.name);
                    bad = true;
                } else if l.starts_with("STRUCT")
                    || l.starts_with("REFUSED")
                    || l.starts_with("BOTH")
                {
                    let e = categories.entry(category(l)).or_default();
                    e.0 += 1;
                    if e.1.len() < 3 {
                        e.1.push(format!("{} [{}]", l, case.name));
                    }
                }
            }
            if bad {
                failed = true;
                kept += 1;
                let stem = dir.join(format!("{}-lowering-{kept}", std::process::id()));
                let events: Vec<String> = case.events.iter().map(|e| e.label()).collect();
                let _ = std::fs::write(stem.with_extension("contract"), &case.source);
                let _ = std::fs::write(stem.with_extension("events"), events.join("\n"));
                let _ = std::fs::write(stem.with_extension("lean.txt"), r.lines.join("\n"));
                println!("  kept: {}.{{contract,events,lean.txt}}", stem.display());
            }
        }
    }
    for (cat, (n, examples)) in &categories {
        println!("{n:6} {cat}");
        for e in examples {
            println!("         e.g. {e}");
        }
    }
    let [agree, both, diverge, same, differ, refused, undecoded] = total;
    println!(
        "difftest lowering: {checked} cases checked ({skipped} not compiled, {boot_refused} boot refused): runs {agree} agree, {both} both fail, {diverge} diverge; bodies {same} identical to Contract.Lower, {differ} differ, {refused} refused by it, {undecoded} undecoded"
    );
    Ok(accounting && !failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{self, Event};
    use exact_plan::{
        asm::Asm, builder::PlanBuilder, ArmsId, DerivesId, RegionKind, RegionsId, ResourcesId,
        SlotsId, Value,
    };

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn executable_rows_are_checked_against_their_substituted_source() {
        leanrun::build().unwrap();
        let source = "component App\n  state n = 10\n  view\n    Child(n=n)\ncomponent Child\n  props\n    n: number\n  derive expensive = [n - 8].map(n => n + 1).join(\",\")\n  derive value = expensive + \":\" + expensive\n  view\n    text value\ntest \"row\"\n  expect tree has \"3:3\"\n";
        let case = script::scripted("lowering-shared", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let row = plan
            .derives
            .iter()
            .position(|d| d.unsettled && plan.str(d.name).contains("expensive"))
            .unwrap();
        let mut broken = PlanBuilder::from_plan(plan.clone());
        let outer = plan
            .slots
            .iter()
            .position(|s| plan.str(s.name) == "n")
            .map(|i| SlotsId(i as u32))
            .unwrap();
        let mut wrong = Asm::new();
        wrong
            .load_slot(outer)
            .number(8.0)
            .simple(Opcode::Sub)
            .list(1);
        let end = wrong.label();
        wrong.each_item(Opcode::Map, end);
        // Corrupt only the callback read: use the outer `n` (10) where the
        // callback's shadowing `n` is the list item (2).
        wrong.load_slot(outer).number(1.0).simple(Opcode::Add);
        wrong
            .place(end)
            .str(broken.str(","))
            .call(exact_plan::Stdlib::Join);
        let wrong = broken.code(wrong);
        broken.set_derive_body(DerivesId(row as u32), wrong);
        let broken = broken.finish().unwrap();
        let cases = [&plan, &broken]
            .iter()
            .enumerate()
            .map(|(i, p)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(p, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) = run_batch(&cases, &work_dir(), "lowering-shared-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert!(reports[0].sum[0] >= 3, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[5], 0, "{:?}", reports[0].lines);
        assert!(reports[1].sum[2] > 0, "{:?}", reports[1].lines);
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn owned_resource_rows_use_the_instance_answer_and_currentness() {
        leanrun::build().unwrap();
        let source = include_str!("../../corpus/resources/owned-derive.contract");
        let mut case = script::scripted("lowering-owned-answer", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let seed = (0..100)
            .find(|seed| {
                let mut oracle = crate::oracle::Oracle::new(&plan, *seed);
                matches!(
                    exact_runner::DataSource::query(
                        &mut oracle,
                        "ownedAnswer",
                        &[Value::Number(3.0)]
                    ),
                    Ok(Value::Number(n)) if n != 0.0
                )
            })
            .expect("the oracle can give the owned request a non-zero answer");
        case.seed = Some(seed);
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let row = |name: &str| {
            plan.derives
                .iter()
                .position(|d| d.unsettled && plan.str(d.name).contains(name))
                .unwrap_or_else(|| panic!("{name} is an executable row"))
        };
        let mut wrong_value = PlanBuilder::from_plan(plan.clone());
        let placeholder_value = wrong_value.constant(&Value::Number(1.0));
        wrong_value.set_derive_body(DerivesId(row("value") as u32), placeholder_value);
        let wrong_value = wrong_value.finish().unwrap();
        let mut wrong_current = PlanBuilder::from_plan(plan.clone());
        let placeholder_current = wrong_current.constant(&Value::Bool(false));
        wrong_current.set_derive_body(DerivesId(row("fresh") as u32), placeholder_current);
        let wrong_current = wrong_current.finish().unwrap();
        let cases = [&plan, &wrong_value, &wrong_current]
            .iter()
            .enumerate()
            .map(|(i, candidate)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(candidate, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) = run_batch(&cases, &work_dir(), "lowering-owned-answer-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert!(reports[0].sum[0] >= 4, "{:?}", reports[0].lines);
        for broken in &reports[1..] {
            assert!(broken.sum[2] > 0, "{:?}", broken.lines);
        }
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn owned_resource_rows_reject_a_plan_owner_cut_out_of_scope() {
        leanrun::build().unwrap();
        let source = include_str!("../../corpus/resources/owned-derive.contract");
        let case = script::scripted("lowering-owned-owner", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let resource = plan
            .resources
            .iter()
            .position(|r| plan.str(r.name).contains("answer"))
            .expect("the child owns its answer request");
        let owner = plan.resources[resource]
            .owner
            .expect("the answer request has an instance owner");
        let owner_region = plan.arm(owner).region;
        let descendant = (0..plan.regions.len())
            .map(|i| RegionsId(i as u32))
            .find(|region| *region != owner_region && plan.region(*region).kind == RegionKind::Each)
            .expect("the reader has a descendant for arm");
        let mut cut_out = PlanBuilder::from_plan(plan.clone());
        cut_out.set_resource_owner(
            ResourcesId(resource as u32),
            ArmsId(plan.region(descendant).arms.start),
        );
        let cut_out = cut_out.finish().unwrap();
        let owner_arms = plan.region(owner_region).arms;
        assert!(
            owner_arms.len >= 2,
            "the owning conditional has a sibling arm"
        );
        let owner_offset = owner.0 - owner_arms.start;
        let sibling_offset = if owner_offset == 0 { 1 } else { 0 };
        let mut wrong_arm = PlanBuilder::from_plan(plan.clone());
        wrong_arm.set_resource_owner(
            ResourcesId(resource as u32),
            ArmsId(owner_arms.start + sibling_offset),
        );
        let wrong_arm = wrong_arm.finish().unwrap();
        let cases = [&plan, &cut_out, &wrong_arm]
            .iter()
            .enumerate()
            .map(|(i, candidate)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(candidate, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) = run_batch(&cases, &work_dir(), "lowering-owned-owner-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert!(reports[1].sum[2] > 0, "{:?}", reports[1].lines);
        assert!(reports[2].sum[2] > 0, "{:?}", reports[2].lines);
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn reader_probe_does_not_charge_its_synthetic_result_list() {
        leanrun::build().unwrap();
        let mut source = String::from("component App\n  derive s0 = \"x\"\n");
        for depth in 1..=25 {
            let _ = writeln!(
                source,
                "  derive s{depth} = s{} + s{}",
                depth - 1,
                depth - 1
            );
        }
        source.push_str(
            r#"  view
    column
      for outer in ["outer"] key=outer
        for item in [s25] key=0
          Child(value=item)
component Child
  props
    value: string
  derive shown = value + "x"
  view
    text shown testId="shown"
test "boundary value"
  expect tree has "shown"
"#,
        );
        let case = script::scripted("lowering-reader-boundary", &source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(&source, None).unwrap();
        let row = plan
            .derives
            .iter()
            .position(|d| d.unsettled && plan.str(d.name).contains("shown"))
            .expect("the child derive is an executable row");
        let mut wrong_frame = PlanBuilder::from_plan(plan.clone());
        let mut body = Asm::new();
        body.load_item(1);
        let body = wrong_frame.code(body);
        wrong_frame.set_derive_body(DerivesId(row as u32), body);
        let wrong_frame = wrong_frame.finish().unwrap();
        let cases = [&plan, &wrong_frame]
            .iter()
            .enumerate()
            .map(|(i, candidate)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(candidate, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) =
            run_batch(&cases, &work_dir(), "lowering-reader-boundary-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert_eq!(
            reports[0].sum[2], 0,
            "good probe counts: {:?}",
            reports[0].sum
        );
        assert!(reports[1].sum[2] > 0, "{:?}", reports[1].lines);
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn folded_group_labels_keep_later_region_owners() {
        leanrun::build().unwrap();
        let header = r#"component App
  state on = true
  state values = ["a"]
  view
    list appearance="auto"
      section
        header
          if on
            text "On"
          else
            text "Off"
        row
          for value in values key=value
            if true
              Child(value=value)
component Child
  props
    value: string
  derive shown = value + "!"
  view
    text shown
test "row"
  expect state on == true
"#;
        let footer = r#"component App
  state on = true
  state values = ["a"]
  view
    list appearance="auto"
      section
        row
          text "first"
        footer
          if on
            text "On"
          else
            text "Off"
      section
        row
          for value in values key=value
            if true
              Child(value=value)
component Child
  props
    value: string
  derive shown = value + "!"
  view
    text shown
test "row"
  expect state on == true
"#;
        let mut cases = Vec::new();
        for (name, source) in [("header", header), ("footer", footer)] {
            let case = script::scripted(&format!("lowering-folded-{name}"), source, None)
                .unwrap()
                .remove(0)
                .case;
            let plan = compile(&case).unwrap();
            let shared = contract::lean::sharing_context(source, None).unwrap();
            let row = plan
                .derives
                .iter()
                .position(|d| d.unsettled && d.owner.is_some())
                .expect("the child derive is an arm-owned executable row");
            let owner = plan.derives[row].owner.unwrap();
            let owner_region = plan.arm(owner).region;
            let wrong_region = (0..plan.regions.len())
                .map(|i| RegionsId(i as u32))
                .find(|r| *r != owner_region && plan.region(*r).kind == RegionKind::Each)
                .expect("an enclosing each region follows the folded label");
            let mut broken = PlanBuilder::from_plan(plan.clone());
            broken.set_derive_unsettled(
                DerivesId(row as u32),
                Some(ArmsId(plan.region(wrong_region).arms.start)),
            );
            let broken = broken.finish().unwrap();
            let mut extra_region = PlanBuilder::from_plan(plan.clone());
            let unit = extra_region.constant(&Value::Unit);
            extra_region.region(RegionKind::When, None, None, u32::MAX, unit, unit, 2);
            let extra_region = extra_region.finish().unwrap();
            let mut reordered_regions = plan.clone();
            reordered_regions.regions.swap(0, 1);
            for candidate in [&plan, &broken, &extra_region, &reordered_regions] {
                cases.push(Ready {
                    lean: prepare(&case, cases.len()).unwrap().lean,
                    plan: plan_info(candidate, &shared),
                });
            }
        }
        let (head, reports) =
            run_batch(&cases, &work_dir(), "lowering-folded-labels-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        for group in reports.as_chunks::<4>().0 {
            let [good, wrong_region, extra_region, reordered_regions] = group;
            assert_eq!(good.sum[2], 0, "{:?}", good.lines);
            assert_eq!(good.sum[5], 0, "{:?}", good.lines);
            assert!(
                wrong_region
                    .lines
                    .iter()
                    .any(|line| line.contains("plan owner") && line.contains("source owner")),
                "{:?}",
                wrong_region.lines
            );
            assert!(
                extra_region
                    .lines
                    .iter()
                    .any(|line| line == "DIVERGE regions: source and plan region rosters differ"),
                "{:?}",
                extra_region.lines
            );
            assert!(
                reordered_regions
                    .lines
                    .iter()
                    .any(|line| line == "DIVERGE regions: source and plan region rosters differ"),
                "{:?}",
                reordered_regions.lines
            );
        }
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn a_roster_mismatch_with_a_first_region_owner_reports_and_continues() {
        leanrun::build().unwrap();
        let source = r#"component App
  view
    column
      if true
        Child(value="a")
      if true
        text "after"
component Child
  props
    value: string
  derive shown = value + "!"
  view
    text shown
test "row"
  expect tree has "a!"
"#;
        let case = script::scripted("lowering-first-region-owner", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let row = plan
            .derives
            .iter()
            .position(|d| d.unsettled && d.owner.is_some())
            .expect("the child derive is an arm-owned executable row");
        let owner = plan.derives[row].owner.unwrap();
        assert_eq!(plan.arm(owner).region, RegionsId(0));

        let mut reordered = plan.clone();
        reordered.regions.swap(0, 1);
        let mut wrong_body = PlanBuilder::from_plan(plan.clone());
        let wrong = wrong_body.constant(&Value::str("wrong"));
        wrong_body.set_derive_body(DerivesId(row as u32), wrong);
        let wrong_body = wrong_body.finish().unwrap();
        let cases = [&plan, &reordered, &wrong_body]
            .iter()
            .enumerate()
            .map(|(i, candidate)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(candidate, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) =
            run_batch(&cases, &work_dir(), "lowering-first-region-owner-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert!(
            reports[1]
                .lines
                .iter()
                .any(|line| line == "DIVERGE regions: source and plan region rosters differ"),
            "{:?}",
            reports[1].lines
        );
        assert!(reports[2].sum[2] > 0, "{:?}", reports[2].lines);
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn prerendered_panels_inside_virtualized_rows_are_walked() {
        leanrun::build().unwrap();
        let source = r#"component App
  view
    main navigationKey="b" navigationBack="back" height=400
      row role="tablist"
        button "B" role="tab" aria-controls="b" aria-selected=true
      list virtualized=true height=200
        for item in [1] key=item
          column role="tabpanel" id="b"
            Child(value=item)
component Child
  props
    value: number
  derive shown = value + 1
  view
    text `${shown}` testId="shown"
test "waits without a prerender"
  expect tree missing "shown"
"#;
        let mut case = script::scripted("lowering-prerendered-window", source, None)
            .unwrap()
            .remove(0)
            .case;
        case.prerendered = true;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let row = plan
            .derives
            .iter()
            .position(|d| d.unsettled && d.owner.is_some())
            .expect("the panel child derive is an arm-owned executable row");
        let mut wrong_body = PlanBuilder::from_plan(plan.clone());
        let wrong = wrong_body.constant(&Value::Number(7.0));
        wrong_body.set_derive_body(DerivesId(row as u32), wrong);
        let wrong_body = wrong_body.finish().unwrap();
        let cases = [&plan, &wrong_body]
            .iter()
            .enumerate()
            .map(|(i, candidate)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(candidate, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) =
            run_batch(&cases, &work_dir(), "lowering-prerendered-window-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert!(reports[0].sum[0] > 0, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert!(reports[1].sum[2] > 0, "{:?}", reports[1].lines);
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn an_idle_clock_does_not_recompute_a_nonfinite_row_key() {
        leanrun::build().unwrap();
        let source = r#"component App
  view
    column
      for item in [1] key=(1 / (1 - performanceNow()))
        Child(value=item)
component Child
  props
    value: number
  derive shown = value + 1
  view
    text `${shown}` testId="shown"
test "idle clock"
  expect tree has "shown"
  clock +1
  expect tree has "shown"
"#;
        let case = script::scripted("lowering-idle-clock-key", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let ready = Ready {
            lean: prepare(&case, 0).unwrap().lean,
            plan: plan_info(&plan, &shared),
        };
        let (head, reports) =
            run_batch(&[ready], &work_dir(), "lowering-idle-clock-key-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert!(reports[0].sum[0] >= 2, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn a_changing_clock_key_keeps_the_rendered_instance_under_check() {
        leanrun::build().unwrap();
        let source = r#"component App
  view
    column
      for item in [1] key=performanceNow()
        Child(value=item)
component Child
  props
    value: number
  derive shown = value + performanceNow()
  view
    text `${shown}` testId="shown"
test "idle clock"
  expect tree has "shown"
  clock +1
  expect tree has "shown"
"#;
        let case = script::scripted("lowering-changing-clock-key", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let row = plan
            .derives
            .iter()
            .position(|d| d.unsettled && d.owner.is_some())
            .expect("the keyed child derive is an executable row");
        let mut wrong_body = PlanBuilder::from_plan(plan.clone());
        let boot_value = wrong_body.constant(&Value::Number(1.0));
        wrong_body.set_derive_body(DerivesId(row as u32), boot_value);
        let wrong_body = wrong_body.finish().unwrap();
        let cases = [&plan, &wrong_body]
            .iter()
            .enumerate()
            .map(|(i, candidate)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(candidate, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) =
            run_batch(&cases, &work_dir(), "lowering-changing-clock-key-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert!(reports[0].sum[0] >= 2, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert!(
            reports[1]
                .lines
                .iter()
                .any(|line| line.starts_with("DIVERGE derive ") && line.contains(" cfg1 ")),
            "{:?}",
            reports[1].lines
        );
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn closed_panels_do_not_expand_the_instance_walk() {
        leanrun::build().unwrap();
        let keyed = r#"component App
  state selected = "a"
  state z = 0
  state items = [1]
  action open
    z = 1
    selected = "b"
  view
    main navigationKey=selected navigationBack="back" height=400
      row role="tablist"
        button "A" role="tab" aria-controls="a" aria-selected=(selected == "a")
        button "B" role="tab" aria-controls="b" aria-selected=(selected == "b") press=open testId="open"
      column role="tabpanel" id="a"
        column navigationKey="a"
          text "first"
      list role="tabpanel" id="b" virtualized=true height=200
        for item in items key=(1 / z)
          Child(value=item)
component Child
  props
    value: number
  derive shown = value + 1
  view
    text `${shown}` testId="shown"
test "closed then open"
  expect tree missing "shown"
  tap "open"
  expect tree has "shown"
"#;
        let conditional = r#"component App
  state selected = "a"
  state count = -1
  action open
    count = 1
    selected = "b"
  view
    main navigationKey=selected navigationBack="back"
      row role="tablist"
        button "A" role="tab" aria-controls="a" aria-selected=(selected == "a")
        button "B" role="tab" aria-controls="b" aria-selected=(selected == "b") press=open testId="open"
      column role="tabpanel" id="a"
        column navigationKey="a"
          text "first"
      column role="tabpanel" id="b"
        if range(count).isEmpty()
          Child(value="empty")
        else
          Child(value="full")
component Child
  props
    value: string
  derive shown = value + "!"
  view
    text shown testId="shown"
test "closed then open"
  expect tree missing "shown"
  tap "open"
  expect tree has "shown"
"#;
        let mut ready = Vec::new();
        for (name, source) in [("keyed", keyed), ("conditional", conditional)] {
            let case = script::scripted(&format!("lowering-closed-panel-{name}"), source, None)
                .unwrap()
                .remove(0)
                .case;
            let plan = compile(&case).unwrap();
            let shared = contract::lean::sharing_context(source, None).unwrap();
            let row = plan
                .derives
                .iter()
                .rposition(|d| {
                    d.unsettled && d.owner.is_some() && plan.str(d.name).contains("shown")
                })
                .expect("the opened panel child derive is an arm-owned executable row");
            let mut wrong_body = PlanBuilder::from_plan(plan.clone());
            let wrong = match name {
                "keyed" => Value::Number(7.0),
                "conditional" => Value::str("wrong"),
                _ => unreachable!(),
            };
            let wrong = wrong_body.constant(&wrong);
            wrong_body.set_derive_body(DerivesId(row as u32), wrong);
            let wrong_body = wrong_body.finish().unwrap();
            for candidate in [&plan, &wrong_body] {
                ready.push(Ready {
                    lean: prepare(&case, ready.len()).unwrap().lean,
                    plan: plan_info(candidate, &shared),
                });
            }
        }
        let (head, reports) =
            run_batch(&ready, &work_dir(), "lowering-closed-panels-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        for pair in reports.as_chunks::<2>().0 {
            let [good, wrong_body] = pair;
            assert!(good.sum[0] > 0, "{:?}", good.lines);
            assert_eq!(good.sum[2], 0, "{:?}", good.lines);
            assert_eq!(good.sum[5], 0, "{:?}", good.lines);
            assert!(good.lines.is_empty(), "{:?}", good.lines);
            assert!(
                wrong_body.sum[2] > 0,
                "sum {:?}, lines {:?}",
                wrong_body.sum,
                wrong_body.lines
            );
        }
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn arm_rows_reject_owner_binder_and_body_mutations() {
        leanrun::build().unwrap();
        let source = r#"component App
  state values = ["a", "b"]
  action change
    values = ["b", "c"]
  view
    column
      button "change" press=change testId="change"
      for enclosing in ["wrong"] key=enclosing
        for value in values key=value
          if true
            Child(value=value)
component Child
  props
    value: string
  state suffix = "!"
  derive base = value + suffix
  derive hidden = base
  derive shown = hidden
  view
    for value in ["shadow"] key=value
      if true
        text shown
test "row"
  tap "change"
"#;
        let case = script::scripted("lowering-arm-shared", source, None)
            .unwrap()
            .remove(0)
            .case;
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(source, None).unwrap();
        let row = plan
            .derives
            .iter()
            .position(|d| {
                d.unsettled
                    && d.owner.is_some()
                    && plan.code(d.body).first() == Some(&(Opcode::LoadItem as u8))
            })
            .expect("the keyed child derive is an executable row");
        let id = DerivesId(row as u32);

        let mut wrong_owner = PlanBuilder::from_plan(plan.clone());
        let owner = plan.derives[row].owner.unwrap();
        let region = plan.arm(owner).region;
        let arms = plan.region(region).arms;
        assert!(arms.len > 1);
        let offset = owner.0 - arms.start;
        let sibling = ArmsId(arms.start + (offset + 1) % arms.len);
        wrong_owner.set_derive_unsettled(id, Some(sibling));
        let wrong_owner = wrong_owner.finish().unwrap();

        let mut wrong_binder = PlanBuilder::from_plan(plan.clone());
        let mut binder = Asm::new();
        binder.load_item(2);
        let binder = wrong_binder.code(binder);
        wrong_binder.set_derive_body(id, binder);
        let wrong_binder = wrong_binder.finish().unwrap();

        let mut wrong_body = PlanBuilder::from_plan(plan.clone());
        let body = wrong_body.constant(&Value::Number(7.0));
        wrong_body.set_derive_body(id, body);
        let wrong_body = wrong_body.finish().unwrap();

        let plans = [&plan, &wrong_owner, &wrong_binder, &wrong_body];
        let cases = plans
            .iter()
            .enumerate()
            .map(|(i, plan)| Ready {
                lean: prepare(&case, i).unwrap().lean,
                plan: plan_info(plan, &shared),
            })
            .collect::<Vec<_>>();
        let (head, reports) = run_batch(&cases, &work_dir(), "lowering-arm-shared-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        assert!(reports[0].sum[0] > 0, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[2], 0, "{:?}", reports[0].lines);
        assert_eq!(reports[0].sum[5], 0, "{:?}", reports[0].lines);
        for broken in [&reports[1], &reports[3]] {
            assert!(broken.sum[2] > 0, "{:?}", broken.lines);
        }
        assert!(
            reports[2]
                .lines
                .iter()
                .any(|line| { line.contains("vm \"wrong\"") && line.contains("semantics \"a!\"") }),
            "{:?}",
            reports[2].lines
        );
    }

    #[test]
    #[ignore = "async lane: needs the Lean toolchain"]
    fn landings_check_the_configuration_after_queued_continuations() {
        leanrun::build().unwrap();
        let source = include_str!("../../corpus/trace/queue.contract")
            .replace("  state n = 0", "  state n = 0\n  derive observed = n");
        let mut case = script::scripted("lowering-land", &source, None)
            .unwrap()
            .remove(0)
            .case;
        // No later clock or input may land the work on the checker's behalf.
        case.events.truncate(2);
        assert_eq!(case.events, [Event::Tap("save".into()), Event::InputLand]);
        let plan = compile(&case).unwrap();
        let shared = contract::lean::sharing_context(&source, None).unwrap();
        let mut broken = PlanBuilder::from_plan(plan.clone());
        let zero = broken.constant(&Value::Number(0.0));
        broken.set_derive_body(DerivesId(0), zero);
        let broken = broken.finish().unwrap();
        let mut cases = Vec::new();
        for landing in [Event::InputLand, Event::Land] {
            case.events[1] = landing;
            for plan in [&plan, &broken] {
                cases.push(Ready {
                    lean: prepare(&case, cases.len()).unwrap().lean,
                    plan: plan_info(plan, &shared),
                });
            }
        }
        let (head, reports) = run_batch(&cases, &work_dir(), "lowering-land-test").unwrap();
        assert!(head.is_empty(), "{head:?}");
        for [good, broken] in reports.as_chunks::<2>().0 {
            assert!(!good.boot_refused && !broken.boot_refused);
            assert!(good.sum[0] > 0);
            assert!(good.lines.is_empty(), "{:?}", good.lines);
            assert_eq!(good.sum[2], 0);
            assert_eq!(broken.sum[2], 1, "{:?}", broken.lines);
            let vm = format!(
                "DIVERGE derive observed cfg2: vm {} ",
                crate::observe::number(0.0)
            );
            let sem = format!(" / semantics {} ", crate::observe::number(2.0));
            assert!(
                broken
                    .lines
                    .iter()
                    .any(|line| line.starts_with(&vm) && line.contains(&sem)),
                "{:?}",
                broken.lines
            );
        }
    }
}
