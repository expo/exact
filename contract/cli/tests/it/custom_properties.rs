//! LLP 1123 D5 as an app writes it: `--name` declared on a node, its own or
//! a class's, read by a length below it through `var()`; and what the
//! compiler refuses or warns of.

use crate::scratch::Scratch;
use exact_kernel::{Kernel, Offer};
use exact_plan::Plan;
use exact_runner::Runner;

const APP: &str = r#"style Spaced
  --gap="8px"

component App
  view
    column class=Spaced --pad="calc(var(--gap) * 2)" height=844 testId="root"
      column padding-top="var(--pad)" testId="padded"
        column height=10 testId="probe"
      column --gap="20px" testId="inner"
        column height="var(--gap)" testId="gap"
        column height="var(--pad)" testId="pad"
        column height="var(--none, 5px)" testId="fallback"
        column height=`calc(var(--gap) + ${2 + 1}px)` testId="computed"
"#;

fn boot(source: &str) -> Runner<()> {
    let plan = contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
    boot_plan(&plan)
}

fn boot_plan(plan: &Plan) -> Runner<()> {
    let plan = Plan::decode(&plan.encode()).unwrap();
    let mut runner =
        Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
    runner
        .kernel_mut()
        .compute_layout(1, Offer::definite(390.0, 844.0))
        .unwrap();
    runner
}

fn frame(runner: &Runner<()>, test_id: &str) -> exact_kernel::Frame {
    let kernel = runner.kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .frame
}

#[test]
fn a_length_reads_the_property_its_ancestors_declare() {
    let runner = boot(APP);
    assert_eq!(
        frame(&runner, "probe").y,
        16.0,
        "the class's --gap, doubled where --pad is declared"
    );
    assert_eq!(frame(&runner, "gap").height, 20.0, "the nearer declaration");
    assert_eq!(
        frame(&runner, "pad").height,
        16.0,
        "substituted where it was declared"
    );
    assert_eq!(
        frame(&runner, "computed").height,
        23.0,
        "a template's value"
    );
}

#[test]
fn a_fallback_of_an_undeclared_name_is_admitted_and_warned() {
    let (plan, warnings) = contract::compile_warned(APP);
    assert!(
        warnings
            .iter()
            .any(|w| w.to_string().contains("lower-var-undeclared")
                && w.to_string().contains("--none")),
        "{warnings:?}"
    );
    assert_eq!(frame(&boot_plan(&plan.unwrap()), "fallback").height, 5.0);
    // A misspelling is named
    let misspelt = APP.replace("var(--none, 5px)", "var(--gpa, 5px)");
    let (_, warnings) = contract::compile_warned(&misspelt);
    assert!(
        warnings
            .iter()
            .any(|w| w.to_string().contains("did you mean `--gap`?")),
        "{warnings:?}"
    );
}

#[test]
fn the_compiler_refuses_what_cannot_resolve() {
    let refused = |source: &str, says: &str| {
        let e = contract::compile(source)
            .err()
            .unwrap_or_else(|| panic!("{source} compiled"));
        assert!(e.to_string().contains(says), "{source}: {e}");
    };
    let view = |attrs: &str| format!("component App\n  view\n    column --gap=\"8px\" {attrs}\n");
    // A name nothing declares, without a fallback, can never resolve
    refused(
        &view(r#"height="var(--nope)""#),
        "no node of the app declares it",
    );
    refused(&view(r#"height="var(--gpa)""#), "did you mean `--gap`?");
    // Nor inside a fallback, which is a value of its own
    refused(&view(r#"height="var(--gap, var(--typo))""#), "`--typo`");
    // One attribute declares one property
    refused(&view(r#"--a="1px; --b: 8px""#), "is one length");
    // An owner's inset override is resolved before any custom property
    refused(
        &view(r#"--exact-safe-area-inset-top="var(--gap)""#),
        "reads none",
    );
    // A colour names a token, not a runtime property
    refused(&view(r#"background-color="var(--gap)""#), "a length");
    // A declaration is a length, since only a length reads one
    refused(
        "component App\n  view\n    column --bad=\"red\"\n",
        "is one length",
    );
}

#[test]
fn a_design_token_and_a_custom_property_never_share_a_name() {
    let dir = Scratch::new(format!("exact2-custom-token-{}", std::process::id()));
    let design = "forma t\n\ntheme\n  gap = 4px\n  space.row = 12px\n";
    std::fs::write(dir.join("app.forma"), design).unwrap();
    let view = "component App\n  view\n    column --gap=\"8px\" --pad=\"2px\" padding=\"var(--space.rows, 4px)\"\n      column height=\"var(--pad)\"\n";
    std::fs::write(dir.join("app.contract"), view).unwrap();
    let Err(all) = contract::compile_path_all(&dir.at("app.contract"), false) else {
        panic!("compiled");
    };
    let says = |what: &str| all.iter().any(|e| e.message.contains(what));
    // `var(--gap)` reads the token, so no node declares `--gap`
    assert!(says("`gap` is a token of this app's design"), "{all:#?}");
    // A name with a `.` is no property name, so a misspelt token keeps its refusal
    assert!(says("`--space.rows` is no token"), "{all:#?}");
    assert_eq!(
        all.len(),
        2,
        "the property no token names compiles: {all:#?}"
    );
}

#[test]
fn a_class_choice_or_an_absent_option_drops_one_declaration_and_keeps_the_rest() {
    // `on` chooses a class that declares --gap or one that does not; `pad` is
    // an option a button clears
    let app = "style Spaced\n  --gap=\"8px\"\n\nstyle Plain\n  color=\"#000000\"\n\ncomponent App\n  state on = true\n  state pad = some(\"6px\")\n  action flip\n    on = !on\n  action clear\n    pad = none\n  view\n    column class=(on ? Spaced : Plain) --keep-2=\"2px\" --pad=pad height=844\n      button \"Flip\" press=flip testId=\"flip\"\n      button \"Clear\" press=clear testId=\"clear\"\n      column height=\"var(--gap, 5px)\" testId=\"gap\"\n      column height=\"var(--keep-2)\" testId=\"keep\"\n      column height=\"var(--pad, 3px)\" testId=\"pad\"\n";
    let mut runner = boot(app);
    let press = |runner: &mut Runner<()>, test_id: &str| {
        let kernel = runner.kernel();
        let view = kernel
            .node_by_key(kernel.find_by_test_id(test_id)[0])
            .unwrap()
            .id;
        runner.dispatch(view, exact_runner::Event::Press).unwrap();
        assert!(!runner.is_poisoned(), "{test_id}");
        runner
            .kernel_mut()
            .compute_layout(1, Offer::definite(390.0, 844.0))
            .unwrap();
    };
    let heights = |runner: &Runner<()>| ["gap", "keep", "pad"].map(|t| frame(runner, t).height);
    assert_eq!(heights(&runner), [8.0, 2.0, 6.0]);
    press(&mut runner, "flip");
    assert_eq!(heights(&runner), [5.0, 2.0, 6.0]);
    press(&mut runner, "flip");
    assert_eq!(heights(&runner), [8.0, 2.0, 6.0]);
    press(&mut runner, "clear");
    assert_eq!(heights(&runner), [8.0, 2.0, 3.0]);
}

#[test]
fn a_declaration_holds_a_design_tokens_length() {
    let dir = Scratch::new(format!("exact2-custom-design-{}", std::process::id()));
    // `min` and `safe-area-inset-top` name tokens too: a function's name and an
    // `env()` variable are never one
    let design = "forma t\n\naxes\n  density = regular | compact\n\ntheme\n  space.md = 4px\n  under = 4px\n  px = 4px\n  e2px = 4px\n  ink = #111111\n  exact-safe-area-inset-top = 9px\n  min = 9px\n  safe-area-inset-top = 5px\n  space.row = 12px\n    when density=compact: 8px\n";
    std::fs::write(dir.join("app.forma"), design).unwrap();
    let app = |rows: &str| {
        std::fs::write(dir.join("app.contract"), format!("component App\n  state extra = 1\n  state prefix = \"\"\n  view\n    column --pad=\"calc(var(--space.md) * 2)\" --least=\"min(10px, 20px)\" --top=\"env(/**/safe-area-inset-top)\" --gap-2=\"2px\" --_under=\"8px\" --copy=\"var(--_under, 1px)\" --held=`calc(var(--space.md) + ${{extra}}px)` --unit=`${{extra}}px` --after-text=`${{prefix}}var(--space.md)` --unit-token=`${{prefix}}var(--px)` --scaled=`calc(${{extra}}e2px)` --remark=\"calc(4px /* var(--ink) */)\" --escaped=\"var(--space.md, '\\\\é')\" --hole-unit=`1${{\"px\"}}` height=844\n{rows}")).unwrap();
        contract::compile_path(&dir.join("app.contract"))
    };
    let plan = app("      column height=\"var(--pad)\" testId=\"pad\"\n      column height=\"VAR(--missing, var(--space.md))\" testId=\"fallback\"\n      column padding=\"var(--missing, var(--space.md)) 0px\" testId=\"sides\"\n      column height=\"var(--least)\" testId=\"least\"\n      column height=\"var(--top, 1px)\" testId=\"top\"\n      column padding=\"var(--gap-2) var(--missing, var(--space.md))\" testId=\"later\"\n        column height=1 testId=\"later-child\"\n      column height=\"var(--copy)\" testId=\"copy\"\n      column height=\"var(--held)\" testId=\"held\"\n      column height=\"var(--unit)\" testId=\"unit\"\n      column height=\"var(--after-text, 7px)\" testId=\"after-text\"\n      column height=\"var(--unit-token, 7px)\" testId=\"unit-token\"\n      column height=\"var(--scaled)\" testId=\"scaled\"\n      column height=\"var(--remark)\" testId=\"remark\"\n      column height=\"var(--escaped)\" testId=\"escaped\"\n      column height=\"var(--exact-safe-area-inset-top)\" testId=\"host\"\n      column height=\"var(--hole-unit)\" testId=\"hole-unit\"\n      column height=`calc(var(--missing, var(--space.md)) + ${2 + 1}px)` testId=\"template-fallback\"\n")
        .unwrap_or_else(|e| panic!("{e}"));
    let runner = boot_plan(&plan);
    let heights = [
        "pad",
        "fallback",
        "sides",
        "least",
        "top",
        "later",
        "copy",
        "held",
        "template-fallback",
        "unit",
        "after-text",
        "unit-token",
        "scaled",
        "remark",
        "escaped",
        "host",
        "hole-unit",
    ]
    .map(|t| frame(&runner, t).height);
    // A call with no fallback leaves the next one's for the scan; `--_under`
    // is a name, not a token; in a template with a hole a token is one only
    // through `var()`, so a unit after the hole stays a unit
    assert_eq!(
        heights,
        [8.0, 4.0, 8.0, 10.0, 0.0, 5.0, 8.0, 5.0, 7.0, 1.0, 4.0, 4.0, 100.0, 4.0, 4.0, 0.0, 1.0]
    );
    let (later, child) = (frame(&runner, "later"), frame(&runner, "later-child"));
    assert_eq!(
        child.x - later.x,
        4.0,
        "the left padding is the fallback's token"
    );
    // A comment names no token
    app("      column height=\"var(--missing, /* space.row */ 4px)\" width=\"var(--missing, 4px /* space.row */)\"\n")
        .unwrap_or_else(|e| panic!("{e}"));
    // A component's argument that is one token's `var()` reaches a declaration as the token
    std::fs::write(dir.join("app.contract"), "component App\n  view\n    column height=844\n      Card(pad=\"var(--space.md)\")\n\ncomponent Card\n  props\n    pad: string\n  view\n    column --gap=pad --whole=`${pad}` --twice=`calc(${pad} * 2)` --nested=`calc(${`${pad}`} * 2)`\n      column height=\"var(--gap)\" testId=\"card\"\n      column height=\"var(--whole, 7px)\" testId=\"card-whole\"\n      column height=\"var(--twice)\" testId=\"card-twice\"\n      column height=\"var(--nested)\" testId=\"card-nested\"\n").unwrap();
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap_or_else(|e| panic!("{e}"));
    // A template whose holes are literal once the component is expanded is that literal
    let runner = boot_plan(&plan);
    assert_eq!(
        ["card", "card-whole", "card-twice", "card-nested"].map(|t| frame(&runner, t).height),
        [4.0, 4.0, 8.0, 8.0]
    );
    // A declaration reads a token through `var()`, a length with one value in every mode
    for (rows, says) in [
        (
            "      column --pad=\"var(--space.row)\"\n",
            "differs between the app's modes",
        ),
        (
            "      column --pad=\"var(--ink)\"\n",
            "of another kind than a length",
        ),
    ] {
        let e = app(rows).unwrap_err();
        assert!(e.to_string().contains(says), "{rows}: {e}");
    }
    // A fallback is read at run time, where nothing binds a token that varies by mode
    let e = app("      column height=\"var(--missing, var(--space.row))\"\n").unwrap_err();
    assert!(
        e.to_string().contains("differs between the app's modes"),
        "{e}"
    );
}

#[test]
fn an_option_is_declared_where_it_is_present() {
    // A `match` arm's binding shadows the option it unwraps; a component's computed argument read
    // more than once is bound once, an option still
    let app = "component App\n  state pad = some(\"6px\")\n  state pads: string[] = []\n  view\n    column --alt=(match pad { case some(pad) => pad, case none => \"2px\" }) height=844\n      column height=\"var(--alt)\" testId=\"alt\"\n      Card(pad=pads.at(0))\n\ncomponent Card\n  props\n    pad: option<string>\n  view\n    column --g=(match pad { case some(v) => pad, case none => pad }) --k=\"2px\"\n      column height=\"var(--g, 5px)\" testId=\"g\"\n      column height=\"var(--k)\" testId=\"k\"";
    let runner = boot(app);
    assert!(!runner.is_poisoned());
    assert_eq!(
        ["alt", "g", "k"].map(|t| frame(&runner, t).height),
        [6.0, 5.0, 2.0]
    );
}
