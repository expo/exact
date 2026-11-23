//! LLP 1034 §8: `color-scheme` on a subtree.

use crate::source::{boxed, NoData, Source};
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::Runner;

fn boot(src: &str) -> Runner<Source> {
    let plan = contract::bake(contract::compile(src).unwrap(), boxed(NoData)).unwrap();
    Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// Inherited as CSS's is, a nested `light` winning below it; initially
/// `normal`, the surrounding scheme.
#[test]
fn color_scheme_is_inherited_and_initially_normal() {
    let src = "component App\n  view\n    column\n      column testId=\"sheet\" color-scheme=\"dark\"\n        text \"a\" testId=\"inner\"\n        column color-scheme=\"light\"\n          text \"b\" testId=\"card\"\n      text \"c\" testId=\"plain\"\n";
    let r = boot(src);
    let k = r.kernel();
    let row = exact_kernel::StyleId::ColorScheme;
    for (id, want) in [
        ("sheet", "dark"),
        ("inner", "dark"),
        ("card", "light"),
        ("plain", "normal"),
    ] {
        let got = format!(
            "{:?}",
            k.node_by_key(k.find_by_test_id(id)[0])
                .unwrap()
                .computed(row)
        );
        assert!(got.to_lowercase().contains(want), "{id}: {got}");
    }
}

/// Only `light` and `dark` are written: CSS's `normal` means the page's
/// schemes, not the parent's, and `light dark` and `only` ask a browser to
/// choose.
#[test]
fn only_light_and_dark_are_written() {
    for value in ["normal", "light dark", "only dark", "dim"] {
        let src = format!("component App\n  view\n    column color-scheme=\"{value}\"\n");
        let error = contract::compile(&src).unwrap_err().to_string();
        assert!(error.contains("LLP 1034 §8"), "{value}: {error}");
    }
    for value in ["light", "dark"] {
        let src = format!("component App\n  view\n    column color-scheme=\"{value}\"\n");
        contract::compile(&src).unwrap();
    }
    // A Theme row's Auto as a branch (the euxy port, 2026-10-20): the
    // refusal names the app's own scheme, `setScheme`.
    for auto in ["light dark", "normal", "unset", ""] {
        let src = format!("component App\n  state t = \"auto\"\n  view\n    column color-scheme=(t == \"auto\" ? \"{auto}\" : t)\n");
        let error = contract::compile(&src).unwrap_err().to_string();
        assert!(
            error.contains("`setScheme(\"system\" | \"light\" | \"dark\")` in the action"),
            "{auto}: {error}"
        );
    }
}

/// A bound `normal` unsets the row, so the subtree follows the surrounding
/// scheme (CSS's `normal` would mean the page's).
#[test]
fn a_bound_normal_follows_the_surrounding_scheme() {
    let src = "component App\n  state s = \"normal\"\n  view\n    column color-scheme=\"dark\"\n      column testId=\"bound\" color-scheme=s\n        text \"a\" testId=\"inner\"\n";
    let r = boot(src);
    let k = r.kernel();
    for id in ["bound", "inner"] {
        let node = k.node_by_key(k.find_by_test_id(id)[0]).unwrap();
        assert_eq!(node.color_scheme_dark(), Some(true), "{id}");
    }
}

/// An inline run and an SVG element take their scheme from above.
#[test]
fn a_run_and_an_svg_element_refuse_it() {
    let run = "component App\n  view\n    text \"a \"\n      text \"b\" color-scheme=\"dark\"\n";
    assert!(contract::compile(run)
        .unwrap_err()
        .to_string()
        .contains("inline `text` run"));
    let shape = "component App\n  view\n    svg width=10 height=10 viewBox=\"0 0 10 10\"\n      g color-scheme=\"dark\"\n        rect width=5 height=5\n";
    assert!(contract::compile(shape).is_err());
    let root = "component App\n  view\n    svg color-scheme=\"dark\" width=10 height=10 viewBox=\"0 0 10 10\"\n      rect width=5 height=5 fill=\"light-dark(#fff, #000)\"\n";
    contract::compile(root).unwrap();
}

/// What the store keeps for the app's chosen scheme, by its writes.
fn kept_scheme(writes: Vec<exact_runner::StoreWrite>) -> Vec<Option<String>> {
    writes
        .into_iter()
        .filter(|w| w.name == "exact.state.#scheme")
        .map(|w| w.value.map(|v| String::from_utf8(v).unwrap()))
        .collect()
}

#[test]
fn a_chosen_scheme_is_kept_and_asked_in_the_next_launchs_boot() {
    let plan = contract::compile(
        "component App\n  state zero = 0\n  action pick(s: string)\n    setScheme(s)\n  action wrong\n    setScheme(\"dark\")\n    zero = 1 / zero\n  view\n    text \"page\"\n",
    )
    .unwrap();
    let launch = |snapshot: Vec<(String, Vec<u8>)>| {
        Runner::boot_stored(
            contract::bake(plan.clone(), boxed(NoData)).unwrap(),
            boxed(NoData),
            Kernel::with_monospace(),
            snapshot,
            Default::default(),
            "/",
        )
        .unwrap()
    };
    let schemes = |r: &mut Runner<Source>| -> Vec<Value> {
        (r.take_commands().into_iter())
            .filter(|c| c.name == "setScheme")
            .flat_map(|c| c.args)
            .collect()
    };
    let mut r = launch(Vec::new());
    assert!(schemes(&mut r).is_empty());
    r.take_store_writes();
    r.act("pick", vec![Value::str("dark")]).unwrap();
    assert_eq!(schemes(&mut r), [Value::str("dark")]);
    assert_eq!(
        kept_scheme(r.take_store_writes()),
        [Some("\"dark\"".into())]
    );
    // The next launch asks for it in its boot's commands, before the first frame.
    let mut next = launch(r.store().snapshot());
    assert_eq!(schemes(&mut next), [Value::str("dark")]);
    assert_eq!(next.asked().scheme, Some(Value::str("dark")));
    // A refused commit keeps nothing; `system` forgets the choice.
    assert!(next.act("wrong", vec![]).is_err());
    next.act("pick", vec![Value::str("light")]).unwrap();
    next.act("pick", vec![Value::str("system")]).unwrap();
    let writes = kept_scheme(next.take_store_writes());
    assert_eq!(writes, [Some("\"light\"".into()), None], "{writes:?}");
    let mut after = launch(next.store().snapshot());
    assert!(schemes(&mut after).is_empty());
    // A plan that runs no `setScheme` ignores one an earlier version kept,
    // and only a plan that chooses keeps its rendered pages from waiting.
    let plain = contract::compile("component App\n  view\n    text \"page\"\n").unwrap();
    let kept = vec![("exact.state.#scheme".to_string(), b"\"dark\"".to_vec())];
    let mut r = Runner::boot_stored(
        contract::bake(plain.clone(), boxed(NoData)).unwrap(),
        boxed(NoData),
        Kernel::with_monospace(),
        kept,
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(schemes(&mut r).is_empty());
    assert!(exact_runner::activation::runtime_kept(&plan));
    assert!(!exact_runner::activation::runtime_kept(&plain));
}
