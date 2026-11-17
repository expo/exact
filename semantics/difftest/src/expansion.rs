//! `difftest expansion`: the component expander (contract/syntax/src/inline.rs)
//! checked against an independent meaning of the unexpanded file.
//!
//! For each program, `contract lean --components` embeds the file before
//! expansion (`Contract.Components`) and `contract lean` embeds the
//! expansion Rust made (`Contract.Syntax`). In one Lean process per batch,
//! `Contract.ExpandCheck.report`:
//!
//!  (a) runs the Lean expander (`Contract.Expand`, a transcription of
//!      inline.rs) on the component-level embedding and compares its output
//!      with Rust's, declaration by declaration and node by node (types
//!      apart: a child's declarations carry the types the checker gave the
//!      child standalone, which may be less precise than the expansion's);
//!  (b) runs the component-level semantics (`Contract.CompSem`) on the
//!      unexpanded file and the flat semantics on Rust's expansion over the
//!      case's events, the oracle the runner's transcript, and compares the
//!      observations, omitting commit traces (`ran …`), which the component
//!      semantics does not emit, and the slots and resources the expansion
//!      lifted out of children (`slot name#n …`, `resource name#n …`), which
//!      it keeps per instance, not as the root's.
//!
//! A structural difference is a disagreement between the two expanders; an
//! observational one is a disagreement between the expansion and the
//! component-level meaning — a bug in inline.rs or a modelling error, kept
//! under `target/difftest/expansion/` with its script shrunk.

use crate::script::Case;
use crate::{compile, leanrun, observe, oracle, work_dir};
use std::path::Path;

/// How one case ended.
#[derive(Debug, Clone)]
pub enum Verdict {
    /// Both expansions agree and so do both meanings.
    Agree,
    /// The compiler refused the program.
    Refused(String),
    /// A Lean backend refused a program the compiler accepted.
    Emit(String),
    /// The Lean expander refused the component-level embedding.
    ExpandError(String),
    /// The two expansions differ (where, both sides).
    Structure(String),
    /// The observations differ at line `at`.
    Diverge {
        /// First differing line (comparable lines only).
        at: usize,
        /// The flat semantics' observation, projected.
        flat: Vec<String>,
        /// The component-level semantics' observation.
        comp: Vec<String>,
    },
}

/// A case, its verdict, and whether the lifted declarations' types differ.
pub struct Outcome {
    /// The case.
    pub case: Case,
    /// What happened.
    pub verdict: Verdict,
    /// A difference in a lifted state's or parameter's type, if any.
    pub types: Option<String>,
}

struct Prepared {
    flat: String,
    comp: String,
    oracle: String,
    events: Vec<String>,
}

fn prepare(case: &Case, i: usize) -> Result<Prepared, Verdict> {
    let plan = compile(case).map_err(Verdict::Refused)?;
    let (flat, comp) = match &case.path {
        Some(p) => (
            contract::lean::lean_path_plain(p, &format!("p{i}")),
            contract::lean::lean_components_path(p, &format!("c{i}")),
        ),
        None => (
            contract::lean::lean_plain(&case.source, &format!("p{i}")),
            contract::lean::lean_components(&case.source, &format!("c{i}")),
        ),
    };
    let flat = flat.map_err(|e| Verdict::Emit(e.to_string()))?;
    let comp = comp.map_err(|e| Verdict::Emit(e.to_string()))?;
    let oracle = oracle::Oracle::for_case(&plan, case);
    let (_, data) = observe::run(plan, oracle, &case.events);
    Ok(Prepared {
        flat,
        comp,
        oracle: data.map_or_else(String::new, |o| o.text()),
        // Component semantics refuses `then` and queue mutations. Its remaining
        // programs have no modeled work for `land` to deliver.
        events: case
            .events
            .iter()
            .filter(|e| {
                !matches!(
                    e,
                    crate::script::Event::Land | crate::script::Event::InputLand
                )
            })
            .map(crate::script::Event::lean)
            .collect(),
    })
}

fn module(cases: &[(usize, Prepared)]) -> String {
    let mut s = String::from(
        "import Contract.ExpandCheck\nimport Contract.OracleText\nopen Contract\n\n\
         set_option maxRecDepth 100000\nset_option maxHeartbeats 0\n\n",
    );
    for (i, c) in cases {
        s.push_str(&c.flat);
        s.push('\n');
        s.push_str(&c.comp);
        s.push_str(&format!(
            "\ndef o{i} : Oracle := OracleText.parse! {}\n\ndef e{i} : List Observe.Event := [{}]\n\n",
            contract::lean::string(&c.oracle),
            c.events.join(", ")
        ));
    }
    s.push_str("def main : IO Unit := do\n");
    for (i, _) in cases {
        s.push_str(&format!(
            "  IO.println \"#case {i}\"\n  for l in ExpandCheck.report c{i} p{i} o{i} e{i} do IO.println l\n"
        ));
    }
    if cases.is_empty() {
        s.push_str("  pure ()\n");
    }
    s
}

/// Compare outcomes and configurations, omitting commentary, commit traces
/// (the component semantics has none), and the flat side's lifted slots and
/// resources.
fn comparable(lines: &[String], flat: bool) -> Vec<String> {
    lines
        .iter()
        .flat_map(|l| l.lines().map(str::to_string).collect::<Vec<_>>())
        .filter(|l| !l.starts_with('#') && !l.starts_with("ran "))
        .filter(|l| {
            let name = |prefix| {
                l.strip_prefix(prefix)
                    .and_then(|r: &str| r.split(' ').next())
            };
            // A lifted resource is `name#n`; the root's placeholder row
            // `name#else` is the root's own.
            let lifted_resource = |n: &str| {
                n.split('#')
                    .skip(1)
                    .any(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            };
            !flat
                || !(name("slot ").is_some_and(|n| n.contains('#'))
                    || name("resource ").is_some_and(lifted_resource))
        })
        .collect()
}

fn verdict(lines: &[String]) -> (Verdict, Option<String>) {
    let mut structure = None;
    let mut types = None;
    let mut expand_error = None;
    let (mut flat, mut comp) = (Vec::new(), Vec::new());
    let mut section = 0;
    for l in lines {
        if let Some(e) = l.strip_prefix("#expand-error ") {
            expand_error = Some(e.to_string());
        } else if let Some(d) = l.strip_prefix("#struct ") {
            if d != "ok" {
                structure = Some(d.to_string());
            }
        } else if let Some(d) = l.strip_prefix("#types ") {
            if d != "ok" {
                types = Some(d.to_string());
            }
        } else if l == "#flat" {
            section = 1;
        } else if l == "#comp" {
            section = 2;
        } else if section == 1 {
            flat.push(l.clone());
        } else if section == 2 {
            comp.push(l.clone());
        }
    }
    let (flat, comp) = (comparable(&flat, true), comparable(&comp, false));
    let at = flat
        .iter()
        .zip(&comp)
        .position(|(a, b)| a != b)
        .or((flat.len() != comp.len()).then(|| flat.len().min(comp.len())));
    let v = if let Some(e) = expand_error {
        Verdict::ExpandError(e)
    } else if let Some(at) = at {
        Verdict::Diverge { at, flat, comp }
    } else if let Some(d) = structure {
        Verdict::Structure(d)
    } else {
        Verdict::Agree
    };
    (v, types)
}

/// Run `cases`, `batch` per Lean process, the processes side by side.
pub fn check(cases: Vec<Case>, batch: usize, tag: &str) -> Result<Vec<Outcome>, String> {
    let chunks: Vec<&[Case]> = cases.chunks(batch.max(1)).collect();
    let prepared: Vec<Vec<Result<Prepared, Verdict>>> = chunks
        .iter()
        .map(|chunk| {
            chunk
                .iter()
                .enumerate()
                .map(|(i, c)| prepare(c, i))
                .collect()
        })
        .collect();
    let modules: Vec<String> = prepared
        .iter()
        .map(|b| {
            let ready: Vec<(usize, Prepared)> = b
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    p.as_ref().ok().map(|p| {
                        (
                            i,
                            Prepared {
                                flat: p.flat.clone(),
                                comp: p.comp.clone(),
                                oracle: p.oracle.clone(),
                                events: p.events.clone(),
                            },
                        )
                    })
                })
                .collect();
            module(&ready)
        })
        .collect();
    let jobs = std::env::var("DIFFTEST_JOBS")
        .ok()
        .and_then(|j| j.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism().map_or(2, |n| (n.get() / 2).max(1))
        });
    let next = std::sync::atomic::AtomicUsize::new(0);
    type Lines = Result<Vec<String>, String>;
    let results: Vec<std::sync::Mutex<Option<Lines>>> = modules
        .iter()
        .map(|_| std::sync::Mutex::new(None))
        .collect();
    let dir = work_dir().join("expansion");
    std::thread::scope(|scope| {
        for _ in 0..jobs.min(modules.len()) {
            scope.spawn(|| loop {
                let b = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let Some(m) = modules.get(b) else { break };
                let r = leanrun::run_module(m, &dir, &format!("{tag}-{b}"));
                *results[b].lock().expect("result slot") = Some(r);
            });
        }
    });
    let mut outcomes = Vec::new();
    for ((chunk, prep), result) in chunks.iter().zip(prepared).zip(results) {
        let text = result
            .into_inner()
            .expect("result slot")
            .expect("every batch ran")?;
        let mut per: Vec<Vec<String>> = vec![Vec::new(); chunk.len()];
        let mut cur: Option<usize> = None;
        for l in text {
            if let Some(i) = l.strip_prefix("#case ") {
                cur = i.parse().ok();
            } else if let Some(i) = cur {
                per[i].push(l);
            }
        }
        for (i, (case, p)) in chunk.iter().zip(prep).enumerate() {
            let (verdict, types) = match p {
                Err(v) => (v, None),
                Ok(_) => verdict(&per[i]),
            };
            outcomes.push(Outcome {
                case: case.clone(),
                verdict,
                types,
            });
        }
    }
    Ok(outcomes)
}

/// Shrink a diverging case's script: drop events while it still diverges.
fn shrink(case: &Case, tag: &str) -> Result<Case, String> {
    let mut best = case.clone();
    loop {
        if best.events.is_empty() {
            return Ok(best);
        }
        let candidates: Vec<Case> = (0..best.events.len())
            .map(|i| {
                let mut c = best.clone();
                c.events.remove(i);
                c
            })
            .collect();
        let found = check(candidates, usize::MAX, tag)?
            .into_iter()
            .find(|o| matches!(o.verdict, Verdict::Diverge { .. }));
        match found {
            Some(o) => best = o.case,
            None => return Ok(best),
        }
    }
}

/// Run and report: one line per disagreement (kept with its reproduction),
/// then a summary. Passes when nothing disagrees and the Lean backends
/// refused nothing the compiler accepted.
pub fn run(cases: Vec<Case>, batch: usize, tag: &str) -> Result<bool, String> {
    leanrun::build_targets(&["Contract.ExpandCheck", "Contract.OracleText"])?;
    let outcomes = check(cases, batch, tag)?;
    let dir = work_dir().join("expansion");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (mut agree, mut refused, mut emit, mut expand, mut structure, mut diverge, mut typed) =
        (0, 0, 0, 0, 0, 0, 0);
    for (i, o) in outcomes.iter().enumerate() {
        if let Some(t) = &o.types {
            typed += 1;
            println!("TYPES {}: {t}", o.case.name);
        }
        match &o.verdict {
            Verdict::Agree => agree += 1,
            Verdict::Refused(_) => refused += 1,
            Verdict::Emit(e) => {
                emit += 1;
                println!("EMIT {}: {e}", o.case.name);
            }
            Verdict::ExpandError(e) => {
                expand += 1;
                println!("EXPAND {}: the Lean expander refused it: {e}", o.case.name);
            }
            Verdict::Structure(d) => {
                structure += 1;
                let stem = keep(&dir, i, &o.case, &[], &[]);
                println!("STRUCTURE {}: {d}\n  kept: {stem}", o.case.name);
            }
            Verdict::Diverge { .. } => {
                diverge += 1;
                let case = shrink(&o.case, &format!("{tag}-shrink-{i}"))?;
                let again = check(vec![case.clone()], 1, &format!("{tag}-again-{i}"))?;
                let (at, flat, comp) = match &again[0].verdict {
                    Verdict::Diverge { at, flat, comp } => (*at, flat.clone(), comp.clone()),
                    _ => match &o.verdict {
                        Verdict::Diverge { at, flat, comp } => (*at, flat.clone(), comp.clone()),
                        _ => unreachable!(),
                    },
                };
                let stem = keep(&dir, i, &case, &flat, &comp);
                println!(
                    "DIVERGE {} at line {at}:\n  flat: {}\n  comp: {}\n  kept: {stem}",
                    o.case.name,
                    flat.get(at).map_or("<end>", String::as_str),
                    comp.get(at).map_or("<end>", String::as_str),
                );
            }
        }
    }
    println!(
        "difftest expansion: {} cases: {agree} agree, {structure} expand differently, {diverge} mean differently, \
         {expand} refused by the Lean expander, {emit} refused by a Lean backend, {refused} refused by the compiler; \
         {typed} with a lifted type that differs",
        outcomes.len()
    );
    Ok(structure == 0 && diverge == 0 && expand == 0 && emit == 0)
}

fn keep(dir: &Path, i: usize, case: &Case, flat: &[String], comp: &[String]) -> String {
    let stem = dir.join(format!("{}-case-{i}", std::process::id()));
    let events: Vec<String> = case.events.iter().map(|e| e.label()).collect();
    let _ = std::fs::write(stem.with_extension("contract"), &case.source);
    let _ = std::fs::write(stem.with_extension("events"), events.join("\n"));
    let _ = std::fs::write(stem.with_extension("flat.txt"), flat.join("\n"));
    let _ = std::fs::write(stem.with_extension("comp.txt"), comp.join("\n"));
    format!("{}.{{contract,events,flat.txt,comp.txt}}", stem.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commits_do_not_make_equal_configurations_diverge() {
        let lines = [
            "#struct ok",
            "#flat",
            "== tap \"go\"",
            "outcome ok",
            "ran task tick ok",
            "ran then finished ok",
            "ran handler go ok",
            "slot count n3ff0000000000000",
            "slot local#0 n4000000000000000",
            "#comp",
            "== tap \"go\"",
            "outcome ok",
            "slot count n3ff0000000000000",
        ]
        .map(str::to_string);
        assert!(matches!(verdict(&lines).0, Verdict::Agree));
    }

    #[test]
    fn lifted_resources_are_omitted_and_placeholder_rows_kept() {
        let agree = [
            "#flat",
            "resource got#1 \"a\"",
            "resource data#else n0000000000000000",
            "#comp",
            "resource data#else n0000000000000000",
        ]
        .map(str::to_string);
        assert!(matches!(verdict(&agree).0, Verdict::Agree));
        let differ = ["#flat", "resource data#else u", "#comp"].map(str::to_string);
        assert!(matches!(verdict(&differ).0, Verdict::Diverge { .. }));
    }

    #[test]
    fn differences_after_a_commit_remain_visible() {
        for (flat, comp) in [
            ("outcome ok", "outcome refused\n# missing handler"),
            (
                "slot count n3ff0000000000000",
                "slot count n4000000000000000",
            ),
            ("command focus \"a\"", "command focus \"b\""),
            ("view \"label\" \"a\"", "view \"label\" \"b\""),
            ("trace disagrees", ""),
        ] {
            let lines = ["#flat", "ran handler go ok", flat, "#comp", comp].map(str::to_string);
            let Verdict::Diverge { at, flat: got, .. } = verdict(&lines).0 else {
                panic!("{flat:?} and {comp:?} must differ")
            };
            assert_eq!(at, 0);
            assert_eq!(got, [flat]);
        }
    }
}
