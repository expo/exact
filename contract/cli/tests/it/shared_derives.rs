use exact_kernel::Kernel;
use exact_plan::{Opcode, Value};
use exact_runner::vm::instructions;
use exact_runner::{DataError, DataSource, Event, Runner};
use std::path::Path;

struct Empty;
impl DataSource for Empty {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

fn plan(name: &str) -> exact_plan::Plan {
    contract::compile_path(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../corpus/derive-form")
            .join(name),
    )
    .unwrap()
}

#[test]
fn component_form_stays_within_ten_percent_of_its_root_twin_and_edits_identically() {
    let child = plan("components.contract");
    let root = plan("root.contract");
    assert!(child.derives.iter().any(|d| d.unsettled));
    assert!(
        child.code.len() * 100 <= root.code.len() * 110,
        "child {} / root {}",
        child.code.len(),
        root.code.len()
    );
    assert!(
        child.encode().len() * 100 <= root.encode().len() * 110,
        "encoded child {} / root {}",
        child.encode().len(),
        root.encode().len()
    );
    let boot =
        |p| Runner::boot(p, Empty, Kernel::with_monospace(), Default::default(), "/").unwrap();
    let (mut a, mut b) = (boot(child), boot(root));
    assert_eq!(
        a.kernel().export(None).unwrap(),
        b.kernel().export(None).unwrap()
    );
    let mut seed = 0x63d5_u64;
    for step in 0..100 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let (id, event) = match step % 5 {
            0 => (
                "field".to_string(),
                Event::Input(format!("label {step}").into()),
            ),
            1 => ("field".to_string(), Event::Input("x".into())),
            2 => ("toggle".to_string(), Event::Press),
            3 => ("rekey".to_string(), Event::Press),
            _ => (
                format!("edit{}", seed % 19),
                Event::Input(format!("draft {step}").into()),
            ),
        };
        let view = |r: &Runner<Empty>| {
            r.kernel()
                .node_by_key(r.kernel().find_by_test_id(&id)[0])
                .unwrap()
                .id
        };
        let before = a.kernel().export(None).unwrap();
        let ar = a.dispatch(view(&a), event.clone()).unwrap();
        let br = b.dispatch(view(&b), event).unwrap();
        assert_eq!(format!("{ar:?}"), format!("{br:?}"));
        assert_eq!(
            a.kernel().export(None).unwrap(),
            b.kernel().export(None).unwrap()
        );
        if step == 0 {
            assert_ne!(before, a.kernel().export(None).unwrap());
            assert!(exact_runner::agent::tree(&a).contains("label 0"));
        }
        if step == 1 {
            assert!(!exact_runner::agent::tree(&a).contains("label 0"));
        }
        assert!(a.journal().all(|line| !line.contains("trapped")));
        assert!(b.journal().all(|line| !line.contains("trapped")));
    }
}

#[test]
fn derives_in_stateless_arms_keep_their_owner_through_deeper_frames() {
    let plan = plan("stateless.contract");
    let mut runner = Runner::boot(
        plan,
        Empty,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let view = runner
        .kernel()
        .node_by_key(runner.kernel().find_by_test_id("change")[0])
        .unwrap()
        .id;
    runner.dispatch(view, Event::Press).unwrap();
    assert!(!runner.is_poisoned());
    assert!(runner.journal().all(|line| !line.contains("trapped")));
    let tree = exact_runner::agent::tree(&runner);
    assert!(tree.contains("second!") && tree.contains("c!"), "{tree}");
}

#[test]
fn sharing_dependencies_does_not_depend_on_declaration_order() {
    let source = |derives: &str| {
        format!("component App\n  state items = [\"one\", \"two\"]\n  view\n    Child(items=items)\ncomponent Child\n  props\n    items: string[]\n{derives}  view\n    text `${{a}} ${{b}} ${{expensive.length}}`\n")
    };
    let declarations = [
        "  derive expensive = items.filter(x => x.length > 2)\n",
        "  derive a = expensive.length\n",
        "  derive b = expensive.join(\",\")\n",
    ];
    let forward = contract::compile(&source(&declarations.concat())).unwrap();
    let reverse = contract::compile(&source(
        &[declarations[1], declarations[2], declarations[0]].concat(),
    ))
    .unwrap();
    assert_eq!(forward.code.len(), reverse.code.len());
    assert_eq!(forward.encode().len(), reverse.encode().len());
}

#[test]
fn equivalent_sibling_derives_render_without_spending_call_depth() {
    let plan = plan("siblings.contract");
    assert_eq!(plan.derives.iter().filter(|row| row.unsettled).count(), 1);
    let runner = Runner::boot(
        plan,
        Empty,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for i in 0..65 {
        let id = format!("alias{i}");
        let key = runner.kernel().find_by_test_id(&id)[0];
        assert_eq!(
            runner
                .kernel()
                .node_by_key(key)
                .unwrap()
                .props
                .str(exact_kernel::PropId::Text),
            Some("1"),
            "{id}"
        );
    }
    assert!(runner.journal().all(|line| !line.contains("trapped")));
}

#[test]
fn transitive_derives_share_without_a_direct_view_read() {
    let source = |direct: bool| {
        let derives = (0..40)
            .map(|i| format!("  derive a{i} = expensive.length + {i}\n"))
            .collect::<String>();
        let labels = (0..40)
            .map(|i| format!("      text `${{a{i}}}`\n"))
            .collect::<String>();
        let direct = if direct {
            "      text `${expensive.length}`\n"
        } else {
            ""
        };
        format!("component App\n  state items = [\"one\", \"two\"]\n  view\n    Child(items=items)\ncomponent Child\n  props\n    items: string[]\n  derive expensive = items.filter(x => x.length > 2)\n{derives}  view\n    column\n{labels}{direct}")
    };
    let indirect = contract::compile(&source(false)).unwrap();
    let direct = contract::compile(&source(true)).unwrap();
    let rows = indirect
        .derives
        .iter()
        .enumerate()
        .filter(|(_, row)| row.unsettled)
        .map(|(id, row)| {
            let body = instructions(indirect.code(row.body))
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            (id, body)
        })
        .collect::<Vec<_>>();
    let filters = rows
        .iter()
        .filter(|(_, body)| body.iter().any(|ins| ins.op == Opcode::Filter))
        .collect::<Vec<_>>();
    assert_eq!(filters.len(), 1);
    let filter_id = filters[0].0;
    let readers = rows.iter().filter(|(id, _)| *id != filter_id);
    assert_eq!(readers.clone().count(), 40);
    for (_, body) in readers {
        let loads = body
            .iter()
            .filter(|ins| ins.op == Opcode::LoadDerive)
            .map(|ins| ins.args[0] as usize)
            .collect::<Vec<_>>();
        assert_eq!(loads, [filter_id]);
    }
    assert_eq!(
        instructions(&indirect.code)
            .map(Result::unwrap)
            .filter(|ins| ins.op == Opcode::Filter)
            .count(),
        1
    );
    assert!(
        indirect.encode().len() <= direct.encode().len(),
        "indirect {} / direct {}",
        indirect.encode().len(),
        direct.encode().len()
    );
}

#[test]
fn equivalent_aliases_keep_their_style_on_the_runner() {
    let runner = Runner::boot(
        plan("aliases.contract"),
        Empty,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for i in 0..65 {
        let key = runner.kernel().find_by_test_id(&format!("alias{i}"))[0];
        let node = runner.kernel().node_by_key(key).unwrap();
        assert_eq!(node.style.opacity, 0.25);
    }
    assert!(runner.journal().all(|line| !line.contains("trapped")));
}

#[test]
fn distinct_derive_dependencies_preserve_valid_call_depth() {
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/derive-form/depth.contract"),
    )
    .unwrap();
    for all_reads in [false, true] {
        let source = if all_reads {
            source.replace(
                "    text `${d64}` testId=\"last\"",
                &format!(
                    "    column\n{}",
                    (0..65)
                        .map(|i| format!("      text `${{d{i}}}` testId=\"d{i}\"\n"))
                        .collect::<String>()
                ),
            )
        } else {
            source.clone()
        };
        let plan = contract::compile(&source).unwrap();
        let mut depths = vec![0; plan.derives.len()];
        for (i, row) in plan.derives.iter().enumerate() {
            if row.unsettled {
                depths[i] = 1 + exact_runner::vm::instructions(plan.code(row.body))
                    .map(Result::unwrap)
                    .filter(|ins| ins.op == exact_plan::Opcode::LoadDerive)
                    .map(|ins| depths[ins.args[0] as usize])
                    .max()
                    .unwrap_or(0);
            }
        }
        assert_eq!(depths.into_iter().max(), Some(48));
        let runner = Runner::boot(
            plan,
            Empty,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let id = if all_reads { "d64" } else { "last" };
        let key = runner.kernel().find_by_test_id(id)[0];
        assert_eq!(
            runner
                .kernel()
                .node_by_key(key)
                .unwrap()
                .props
                .str(exact_kernel::PropId::Text),
            Some("65")
        );
        assert!(runner.journal().all(|line| !line.contains("trapped")));
    }
}

#[test]
fn executable_call_limit_names_the_load_instruction() {
    let plan = plan("call-limit.contract");
    let pc = plan
        .derives
        .iter()
        .filter(|r| r.unsettled)
        .flat_map(|r| exact_runner::vm::instructions(plan.code(r.body)).map(Result::unwrap))
        .find(|i| i.op == exact_plan::Opcode::LoadDerive)
        .unwrap()
        .pc;
    assert!(pc > 0);
    let runner = Runner::boot(
        plan,
        Empty,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let journal = runner
        .journal()
        .filter(|line| line.contains(" trapped "))
        .map(|line| line.split_once(' ').unwrap().1.to_owned())
        .collect::<Vec<_>>();
    assert_eq!(journal, [format!("view 1 (site 0): text trapped Trap(CallLimit {{ pc: {pc} }}): past the 65,536 derive calls one evaluation may take; unset")]);
    eprintln!("call-limit LoadDerive pc: {pc}");
}
