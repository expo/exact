//! The evaluation budget's emission (LLP 1090 §4): what a body counts and
//! checks, where it resets, and that every check names the VM's pc.

use exact_plan::{Opcode, Stdlib};
use exact_runner::vm::instructions;

fn js(src: &str) -> (exact_plan::Plan, String) {
    let plan = contract::compile(src).unwrap_or_else(|e| panic!("{src}\n{e}"));
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    (plan, js)
}

/// The statement `const <name>=…;` up to the next top-level declaration.
fn decl<'a>(js: &'a str, name: &str) -> &'a str {
    let at = js
        .find(&format!("const {name}="))
        .unwrap_or_else(|| panic!("{name} in {js}"));
    let rest = &js[at..];
    // The next declaration of the module's (a body's own are `const $…`).
    let end = rest
        .match_indices(';')
        .find(|(i, _)| {
            let next = &rest[i + 1..];
            next.starts_with("mount(")
                || next.starts_with("usePaint(")
                || next.starts_with("const ") && !next[6..].starts_with('$')
        })
        .map_or(rest.len(), |(i, _)| i + 1);
    &rest[..end]
}

const CARD: &str = "shape Card\n  id: string\n  n: number\n  label: string\n";

/// A body with no list step, construction, concatenation or checked call is
/// emitted as it was; one that builds or concatenates checks there, with no
/// counter.
#[test]
fn only_a_list_step_meters_a_body() {
    let (_, js) = js(&format!("{CARD}component App\n  state n = 1\n  state s = \"a\"\n  derive plain = n + 1\n  derive joined = s + \"b\"\n  derive card = Card(id=s, n=n, label=s)\n  view\n    text `${{plain}} ${{joined}} ${{card.n}}`\n"));
    assert_eq!(decl(&js, "d_0"), "const d_0=memo(()=>(s_0()+1),\"n\");");
    let joined = decl(&js, "d_1");
    assert!(
        joined.contains("cc(s_1(),\"b\",") && !joined.contains("$s"),
        "{joined}"
    );
    let card = decl(&js, "d_2");
    assert!(card.contains("K([") && !card.contains("$s"), "{card}");
    assert!(js.contains("from\"./budget.js\""), "{js}");
}

/// A metered body counts in one local of its own, nested callbacks
/// included; a `join` adds its list's length at its call; a binding that
/// meters is a function called in place, so each evaluation starts at 0.
#[test]
fn a_metered_body_declares_one_counter_and_join_steps_at_its_call() {
    let (_, js) = js(&format!("{CARD}component App\n  resource cards = cards(3) as shape Card[]\n  derive nested = cards.map(a => cards.map(b => b.n))\n  derive labels = cards.map(c => c.label).join(\",\")\n  view\n    column\n      text labels\n      text `${{cards.map(c => c.n).length}}`\n"));
    let nested = decl(&js, "d_0");
    assert_eq!(nested.matches("let $s=0").count(), 1, "{nested}");
    assert_eq!(nested.matches("++$s>65536").count(), 2, "{nested}");
    let labels = decl(&js, "d_1");
    assert!(
        labels.contains(".length)>65536)$T(\"IterationLimit\""),
        "{labels}"
    );
    assert!(labels.contains("x_join("), "{labels}");
    assert!(
        js.contains("tg(e2,2,\"text\",()=>{let $s=0;"),
        "a metered binding is evaluated by its guard: {js}"
    );
}

/// Each resource argument, row-state initializer and handler argument is an
/// evaluation of its own; a handler's arguments are taken inside its
/// action's commit (`.t`).
#[test]
fn each_reset_point_is_its_own_evaluation() {
    let (_, js) = js(&format!("{CARD}component App\n  resource cards = cards(3, 0) as shape Card[]\n  resource two = cards(cards.map(c => c).length, cards.map(c => c).length) as shape Card[]\n  state n = 0\n  action set(a: number, b: number)\n    n = a + b\n  view\n    column\n      button press=set(cards.map(c => c).length, cards.map(c => c).length) testId=\"go\"\n        text \"go\"\n      for c in cards key=c.id\n        Row(c=c, all=cards)\ncomponent Row\n  props\n    c: Card\n    all: Card[]\n  state a = all.map(x => x).length\n  state b = all.map(x => x).length\n  view\n    text `${{a}} ${{b}}`\n"));
    assert_eq!(decl(&js, "r_1").matches("let $s=0").count(), 2, "{js}");
    assert!(js.contains("on(e"), "{js}");
    let press = js.split("\"press\",").nth(1).expect("a press handler");
    assert!(press.starts_with("a_0.t(()=>["), "{press}");
    assert_eq!(
        press
            .split("]),onPress);")
            .next()
            .unwrap()
            .matches("let $s=0")
            .count(),
        2,
        "{press}"
    );
    let owned = js.split("const $r").nth(1).expect("the row's slots");
    let owned = owned.split("};const").next().unwrap();
    assert_eq!(owned.matches("let $s=0").count(), 2, "{owned}");
}

/// An action knows its string parameters' names, and whether it takes the
/// row first, so an argument past MAX_STRING is refused naming it.
#[test]
fn act_names_its_string_parameters() {
    let (_, js) = js("component App\n  state t = \"\"\n  action put(v: string, n: number)\n    t = v\n  resource ids = ids() as shape string[]\n  view\n    column\n      button press=put(\"a\", 1) testId=\"go\"\n        text \"go\"\n      for i in ids key=i\n        Row()\ncomponent Row\n  state got = \"\"\n  action take(w: string)\n    got = w\n  view\n    input value=got input=take testId=\"in\"\n");
    assert!(
        decl(&js, "a_0").ends_with(",[\"s\",\"n\"],0,[\"v\",0]);"),
        "{js}"
    );
    assert!(decl(&js, "a_1").ends_with(",[\"s\"],1,[\"w\"]);"), "{js}");
    assert!(
        js.contains("sig(\"\",\"s\",\"t\")"),
        "a string slot is named: {js}"
    );
}

/// Every check names the pc of the instruction the VM checks at.
#[test]
fn every_check_names_its_instructions_pc() {
    let (plan, js) = js(&format!("{CARD}component App\n  resource cards = cards(3) as shape Card[]\n  state s = \"a\"\n  derive all = cards.map(c => c.label + s).filter(l => l != \"\").join(\",\")\n  derive card = Card(id=encodeURIComponent(s), n=1, label=s)\n  view\n    text `${{all}} ${{card.id}}`\n"));
    for (i, d) in plan.derives.iter().enumerate() {
        let body = decl(&js, &format!("d_{i}"));
        for x in instructions(plan.code(d.body)).flatten() {
            let want = match x.op {
                Opcode::Map | Opcode::Filter => format!("\"IterationLimit\",{})", x.pc),
                Opcode::Concat => format!(",{})", x.pc),
                Opcode::Record | Opcode::List => format!("],{})", x.pc),
                Opcode::Call if x.args[0] == Stdlib::Join as u64 => {
                    format!("x_join(s0,s1,{})", x.pc)
                }
                Opcode::Call if x.args[0] == Stdlib::EncodeURIComponent as u64 => {
                    format!(",{})", x.pc)
                }
                _ => continue,
            };
            assert!(
                body.contains(&want),
                "{:?} at {}: {want} in {body}",
                x.op,
                x.pc
            );
        }
    }
}

/// LLP 1088 §9.1: `concat`, `split`, `sum`, `range`, `flat`, and `slice`, `includes` and `indexOf`
/// over a list, take their steps on the caller's budget, as `join` does:
/// each gets the body's
/// `$s` at its call and its pc, and the body adds the steps it took (`ST`);
/// a body that calls one is metered, text's included.
#[test]
fn the_list_builders_step_on_the_callers_counter_at_their_call() {
    let (plan, js) = js(&format!("{CARD}component App\n  resource cards = cards(3) as shape Card[]\n  state s = \"ab\"\n  derive both = cards.concat(cards).length\n  derive kept = cards.slice(1).length\n  derive found = cards.map(c => c.label).includes(s)\n  derive text = s.includes(\"a\")\n  derive at = cards.map(c => c.label).indexOf(s)\n  derive pieces = s.split(\"\").length\n  derive textAt = s.indexOf(\"b\")\n  derive total = sum(cards.map(c => c.label.length))\n  derive counted = range(s.length).length\n  derive flat = cards.map(c => [c.label]).flat().length\n  view\n    text `${{both}} ${{kept}} ${{found}} ${{text}} ${{at}} ${{pieces}} ${{textAt}} ${{total}} ${{counted}} ${{flat}}`\n"));
    for (i, d) in plan.derives.iter().enumerate() {
        let body = decl(&js, &format!("d_{i}"));
        assert_eq!(body.matches("let $s=0").count(), 1, "{body}");
        let calls: Vec<_> = instructions(plan.code(d.body))
            .flatten()
            .filter(|x| x.op == Opcode::Call)
            .filter(|x| {
                [
                    Stdlib::Concat,
                    Stdlib::Slice,
                    Stdlib::Includes,
                    Stdlib::IndexOf,
                    Stdlib::Split,
                    Stdlib::Sum,
                    Stdlib::Range,
                    Stdlib::Flat,
                ]
                .iter()
                .any(|&f| x.args[0] == f as u64)
            })
            .collect();
        assert_eq!(calls.len(), 1, "{body}");
        let f = Stdlib::from_wire(calls[0].args[0] as u8).unwrap();
        let want = format!("$s,{});$s+=ST;", calls[0].pc);
        assert!(
            body.contains(&format!("x_{}(", f.name())) && body.contains(&want),
            "{want} in {body}"
        );
    }
    assert!(
        js.contains("ST,") || js.contains(",ST}"),
        "ST imported: {js}"
    );
}

#[test]
fn development_derive_checks_name_the_authored_derive() {
    let plan = contract::compile("component App\n  state value = 1\n  derive doubled = value * 2\n  view\n    text `${doubled}`\n").unwrap();
    let dev = crate::emit::emit(&plan, true, false).unwrap().js;
    let production = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(dev.contains(",\"doubled\")"), "{dev}");
    assert!(!production.contains(",\"doubled\")"), "{production}");
}

/// A prop or style binding that can trap is guarded (`tg`): its trap unsets
/// the row with a journal line naming its site, and the commit goes on
/// (LLP 1017.003 D3). One that names nothing of budget.js's cannot trap and
/// is left as it is.
#[test]
fn a_binding_that_can_trap_unsets_its_row() {
    let (_, js) = js("component App\n  state n = 1\n  view\n    column\n      text range(n).map(i => \"x\").join(\",\") opacity=range(n).length / 2\n      text `${n}`\n");
    assert!(js.contains("tg(e1,1,\"text\",()=>"), "{js}");
    assert!(js.contains("tg(e1,1,\"opacity\",()=>"), "{js}");
    assert!(!js.contains("tg(e2,"), "{js}");
}

#[test]
fn labels_are_only_built_on_the_trap_path() {
    let (_, js) = js("component App\n  state n = 1\n  derive visible = range(n)\n  action grow(v: number)\n    n = range(n).length\n  view\n    column\n      button press=grow(range(n).length)\n        text \"grow\"\n");
    for (name, label) in [("d_0", "derive \"visible\""), ("a_0", "action \"grow\"")] {
        let body = decl(&js, name);
        let quoted = serde_json::to_string(label).unwrap();
        assert!(
            body.contains(&format!("catch($e){{$E($e,{quoted},0)")),
            "{body}"
        );
    }
}

#[test]
fn at_read_rows_share_the_readers_list_and_call_budgets() {
    let mut plan = contract::compile("component App\n  state xs = [1, 2]\n  derive picked = xs.map(x => x + 1)\n  view\n    text `${picked.length}`\n").unwrap();
    plan.derives[0].unsettled = true;
    let js = crate::emit::emit(&plan, false, false).unwrap().js;
    assert!(
        js.contains("const d_0=($budget={s:0,n:0,d:0},$pc)=>"),
        "{js}"
    );
    assert!(js.contains("++$budget.s>65536"), "{js}");
    assert!(js.contains("d_0(undefined,"), "{js}");
    assert!(!js.contains("const d_0=memo"), "{js}");
    assert!(js.contains("finally{$budget.d--}"), "{js}");
    assert!(!js.contains("const d_0=memo"), "{js}");
}

#[test]
fn emitted_row_graphs_stop_at_the_shared_work_and_depth_limits() {
    let temp = std::env::temp_dir().join(format!("exact-row-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("budget.js"),
        temp.join("budget.js"),
    )
    .unwrap();
    std::fs::write(temp.join("rt.js"), "export class Refusal extends Error{};export const text=String,segmentOf=()=>null,say=console.log;export const h=()=>({}),mount=f=>f({}),P=(e,k,f)=>f(),x_toString=String;").unwrap();
    for (length, twice) in [(30, true), (65, false)] {
        let mut source = "component App\n  derive d0 = 1\n".to_string();
        for i in 1..length {
            source.push_str(&format!(
                "  derive d{i} = 1 + d{}{}\n",
                i - 1,
                if twice {
                    format!(" + d{}", i - 1)
                } else {
                    String::new()
                }
            ));
        }
        source.push_str(&format!("  view\n    text `${{d{}}}`\n", length - 1));
        let mut plan = contract::compile(&source).unwrap();
        for row in &mut plan.derives {
            row.unsettled = true;
        }
        let pcs = plan
            .derives
            .iter()
            .flat_map(|r| exact_runner::vm::instructions(plan.code(r.body)).map(Result::unwrap))
            .filter(|i| i.op == exact_plan::Opcode::LoadDerive)
            .map(|i| i.pc)
            .collect::<Vec<_>>();
        assert!(pcs.iter().all(|pc| *pc > 0));
        std::fs::write(
            temp.join("app.js"),
            crate::emit::emit(&plan, false, false).unwrap().js,
        )
        .unwrap();
        let out =
            std::process::Command::new(std::env::var_os("BUN").unwrap_or_else(|| "bun".into()))
                .args(["-e", "import app from './app.js';app();"])
                .current_dir(&temp)
                .output()
                .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let output = String::from_utf8(out.stdout).unwrap();
        let (kind, bound) = if twice {
            (
                "CallLimit",
                ": past the 65,536 derive calls one evaluation may take",
            )
        } else {
            ("CallDepth", ": past 64 derive call levels")
        };
        assert!(
            pcs.iter().any(|pc| output.trim()
                == format!("site 0: text trapped Trap({kind} {{ pc: {pc} }}){bound}; unset")),
            "{length}: {output}"
        );
        assert!(
            pcs.iter()
                .any(|pc| output.contains(&format!("pc: {pc} }}"))),
            "{output}"
        );
    }
    std::fs::remove_dir_all(temp).unwrap();
}
