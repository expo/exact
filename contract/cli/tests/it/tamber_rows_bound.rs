//! Stage 14 E: the write check judges a computed row under a `for` or
//! `match` in the arm's scope, the names pushed as lowering pushes them
//! (`Scope::push_each_of`, `push_some_of`, `push_none`), and through a `let`
//! with its binding typed (`Scope::push_let`); a record's field is typed as
//! `expr::compile` types it, at the root too. A subject, a `let`'s value or a
//! field whose type isn't known by its form leaves the row to the build.
#![cfg(unix)]

use super::tamber_package_params::{package, reading};
use super::tamber_rows_check::{held, same_words, transaction};
use super::tamber_serve::{digest, Serve};
use serde_json::{json, Value};

const DESIGN: &str = "tamber t\n\ntheme\n  ink = #111111\n  one = 1\n";
const HEAD: &str = "use Card from \"@acme/reading\"\n";

fn zero() -> Value {
    json!({"op": "set-token", "name": "one", "value": "0"})
}

fn zeroed() -> String {
    DESIGN.replace("one = 1", "one = 0")
}

/// The server accepts `op` on `contract`, which builds; and `contract build`
/// of the design the write leaves refuses it (`refused`: the stated limit, the
/// build's catch) or accepts it.
fn accepted(tag: &str, contract: &str, op: Value, written: &str, refused: bool) {
    let dir = reading(tag);
    std::fs::write(dir.join("app.tamber"), DESIGN).unwrap();
    std::fs::write(dir.join("app.contract"), contract).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap();
    let mut s = Serve::start(&dir, &[]);
    s.ask(json!({"method": "propose", "ops": [{"op": "set-token", "name": "ink", "value": "#444444"}]}));
    let w = transaction(&mut s, &dir, op);
    assert!(w.get("ok").is_some(), "{tag}: {w}");
    drop(s);
    std::fs::write(dir.join("app.tamber"), written).unwrap();
    assert_eq!(
        contract::compile_path(&dir.join("app.contract")).is_err(),
        refused,
        "{tag}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A `for`'s item, typed from the list's type (`string[]` gives `string`): a
/// number row reading it may name any number token, so a write making one
/// unfit is refused as the build refuses it.
#[test]
fn a_for_items_computed_row_is_judged() {
    let contract = format!("{HEAD}component App\n  state picks = [\"2\"]\n  view\n    column\n      for p in picks key=p\n        button \"Go\" -exact-press-scale=p\n      Card()\n");
    same_words(
        &reading("rows-bound-each"),
        DESIGN,
        &contract,
        zero(),
        &zeroed(),
    );
}

/// A `for`'s position is a number, as lowering types it: a number row
/// reading it names no token, and the write is accepted as the build accepts
/// it.
#[test]
fn a_for_position_is_a_number() {
    let contract = format!("{HEAD}component App\n  state picks = [\"2\"]\n  view\n    column\n      for p, i in picks key=p\n        button \"Go\" -exact-press-scale=i\n      Card()\n");
    accepted("rows-bound-index", &contract, zero(), &zeroed(), false);
}

/// A `match`'s binding in its `some` arm, typed from the subject's `t?`; and
/// in the `none` arm, which binds nothing, a root name is judged as at the
/// root.
#[test]
fn a_match_arms_computed_rows_are_judged() {
    let some = format!("{HEAD}component App\n  state picked = some(\"2\")\n  view\n    column\n      match picked\n        case some(p)\n          button \"Go\" -exact-press-scale=p\n        case none\n          text \"none\"\n      Card()\n");
    same_words(
        &reading("rows-bound-some"),
        DESIGN,
        &some,
        zero(),
        &zeroed(),
    );
    let none = format!("{HEAD}component App\n  state picked = some(\"2\")\n  state pick = \"2\"\n  view\n    column\n      match picked\n        case some(p)\n          text p\n        case none\n          button \"Go\" -exact-press-scale=pick\n      Card()\n");
    same_words(
        &reading("rows-bound-none"),
        DESIGN,
        &none,
        zero(),
        &zeroed(),
    );
}

/// An inner binding shadows an outer one of the same name, as lowering's
/// scope does: the `for`'s item (a number) inside a `match` binding the
/// name to a string names no token.
#[test]
fn an_inner_binding_shadows_the_outer() {
    let contract = format!("{HEAD}component App\n  state picked = some(\"2\")\n  state counts = [2]\n  view\n    column\n      match picked\n        case some(p)\n          for p in counts key=p\n            button \"Go\" -exact-press-scale=p\n        case none\n          text \"none\"\n      Card()\n");
    accepted("rows-bound-shadow", &contract, zero(), &zeroed(), false);
}

/// A subject whose type isn't known by its form (a call) leaves the region
/// to the build: not refused here, the build refuses it.
#[test]
fn a_region_over_a_call_is_the_builds() {
    let contract = format!("{HEAD}fn picksOf(x: string): string[] = [x]\ncomponent App\n  state pick = \"2\"\n  view\n    column\n      for p in picksOf(pick) key=p\n        button \"Go\" -exact-press-scale=p\n      Card()\n");
    accepted("rows-bound-call", &contract, zero(), &zeroed(), true);
}

/// A row through a `let` (a child's derive read twice is bound once,
/// `inline::derives`) is typed with its binding and judged.
#[test]
fn a_row_through_a_let_is_judged() {
    let contract = format!("{HEAD}component App\n  state pick = \"2\"\n  view\n    column\n      Pad(pick=pick)\n      Card()\ncomponent Pad\n  props\n    pick: string\n  derive a = pick\n  derive b = `${{a}}${{a}}`\n  view\n    button \"Go\" -exact-press-scale=b\n");
    same_words(
        &reading("rows-bound-let"),
        DESIGN,
        &contract,
        zero(),
        &zeroed(),
    );
}

/// A `let` whose value's type isn't known by its form (a call) is the
/// build's.
#[test]
fn a_let_over_a_call_is_the_builds() {
    let contract = format!("{HEAD}fn same(x: string): string = x\ncomponent App\n  state pick = \"2\"\n  view\n    column\n      Pad(pick=pick)\n      Card()\ncomponent Pad\n  props\n    pick: string\n  derive a = same(pick)\n  derive b = `${{a}}${{a}}`\n  view\n    button \"Go\" -exact-press-scale=b\n");
    accepted("rows-bound-let-call", &contract, zero(), &zeroed(), true);
}

/// A record's field is typed as `expr::compile` types it, at the root too
/// (theme2's Q1): judged; a field of a value whose type isn't a record by its
/// form (a call's) is the build's.
#[test]
fn a_records_field_is_typed_at_the_root() {
    let shape = "shape Pick\n  scale: string\n";
    let contract = format!("{HEAD}{shape}component App\n  state card = Pick(scale=\"2\")\n  view\n    column\n      button \"Go\" -exact-press-scale=card.scale\n      Card()\n");
    same_words(
        &reading("rows-bound-member"),
        DESIGN,
        &contract,
        zero(),
        &zeroed(),
    );
    let call = format!("{HEAD}{shape}fn pickOf(s: string): Pick = Pick(scale=s)\ncomponent App\n  state s = \"2\"\n  view\n    column\n      button \"Go\" -exact-press-scale=pickOf(s).scale\n      Card()\n");
    accepted("rows-bound-member-call", &call, zero(), &zeroed(), true);
}

/// A `match` whose subject isn't typed by its form (a call) leaves both arms
/// to the build, the `none` arm too: the build lowers the `some` arm first,
/// and a refusal judged only in the `none` arm would not be its first
/// (Grok's round 1).
#[test]
fn a_match_over_an_untyped_subject_is_the_builds() {
    let contract = format!("{HEAD}component App\n  state picks = [\"2\"]\n  state other = \"2\"\n  view\n    column\n      for s in picks key=s\n        match first(picks)\n          case some(p)\n            button \"A\" -exact-press-scale=p\n          case none\n            button \"B\" -exact-press-scale=other\n      Card()\n");
    accepted("rows-bound-match-call", &contract, zero(), &zeroed(), true);
}

/// A record's field through a `for`'s item, typed by the field's own type
/// (Astra's round 1): its string field in a number row is judged, its number
/// field names no token and the write is accepted as the build accepts it.
#[test]
fn a_bound_records_field_has_its_own_type() {
    let shape = "shape Pick\n  n: number\n  scale: string\n";
    let row = |field: &str| {
        format!("{HEAD}{shape}component App\n  state cards = [Pick(n=2, scale=\"2\")]\n  view\n    column\n      for c in cards key=c.scale\n        button \"Go\" -exact-press-scale=c.{field}\n      Card()\n")
    };
    same_words(
        &reading("rows-bound-field-string"),
        DESIGN,
        &row("scale"),
        zero(),
        &zeroed(),
    );
    accepted(
        "rows-bound-field-number",
        &row("n"),
        zero(),
        &zeroed(),
        false,
    );
}

/// An `if` inside a region keeps the region's names (its arms push a frame
/// binding nothing, as lowering's do), and a region inside an inlined
/// component's instance is typed in that instance: both judged.
#[test]
fn an_if_in_a_region_and_a_region_in_an_instance_are_judged() {
    let when = format!("{HEAD}component App\n  state on = true\n  state picks = [\"2\"]\n  view\n    column\n      for p in picks key=p\n        if on\n          button \"Go\" -exact-press-scale=p\n      Card()\n");
    same_words(
        &reading("rows-bound-when"),
        DESIGN,
        &when,
        zero(),
        &zeroed(),
    );
    let instance = format!("{HEAD}component App\n  state picks = [\"2\"]\n  view\n    column\n      Picks(picks=picks)\n      Card()\ncomponent Picks\n  props\n    picks: string[]\n  view\n    column\n      for p in picks key=p\n        button \"Go\" -exact-press-scale=p\n");
    same_words(
        &reading("rows-bound-instance"),
        DESIGN,
        &instance,
        zero(),
        &zeroed(),
    );
}

/// The design server's answers to a propose, a verifying scrub frame and a
/// transaction of `op`, before the design's whole build is known, and to a
/// propose and a transaction once it passed: each the same refusal, which
/// is returned. Nothing is written.
fn refused_both_ways(dir: &std::path::Path, op: Value) -> Value {
    let design = std::fs::read_to_string(dir.join("app.tamber")).unwrap();
    let (mut s, release) = held(dir);
    let probe = json!({"method": "propose", "ops": [{"op": "set-token", "name": "ink", "value": "#444444"}]});
    s.ask(probe.clone());
    let mut answers = vec![
        s.ask(json!({"method": "propose", "ops": [op.clone()]})).0,
        s.ask(json!({"method": "scrub-fold", "open": true, "verify": true,
            "files": {"app.tamber": digest(&dir.join("app.tamber"))}, "ops": [op.clone()]}))
            .0,
        transaction(&mut s, dir, op.clone()),
    ];
    release(&mut s, probe);
    answers.push(s.ask(json!({"method": "propose", "ops": [op.clone()]})).0);
    answers.push(transaction(&mut s, dir, op));
    drop(s);
    let w = answers.pop().unwrap();
    assert!(w["error"]["diagnostics"][0].is_object(), "refused: {w}");
    for a in &answers {
        assert_eq!(a["error"]["diagnostics"], w["error"]["diagnostics"], "{a}");
    }
    assert_eq!(
        std::fs::read_to_string(dir.join("app.tamber")).unwrap(),
        design
    );
    w
}

/// `w` refuses at `built`: its id, words, place and file.
fn at_build(w: &Value, built: &contract::CompileError, file: &str) {
    let d = &w["error"]["diagnostics"][0];
    assert_eq!(d["id"], built.id, "{w}");
    assert_eq!(d["message"], built.message, "{w}");
    assert_eq!(d["line"], built.span.line, "{w}");
    assert_eq!(d["col"], built.span.col, "{w}");
    assert_eq!(d["file"], file, "{w}");
}

/// A row the fast check cannot type precedes one it judges: the write
/// returns the whole build's first refusal, at the call.
#[test]
fn a_row_left_to_the_build_before_a_judged_one_is_refused_first() {
    let contract = format!("{HEAD}fn same(x: string): string = x\ncomponent App\n  state pick = \"2\"\n  view\n    column\n      button \"A\" -exact-press-scale=same(pick)\n      button \"B\" -exact-press-scale=pick\n      Card()\n");
    let dir = reading("rows-bound-limit");
    std::fs::write(dir.join("app.tamber"), DESIGN).unwrap();
    std::fs::write(dir.join("app.contract"), &contract).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap();
    let w = refused_both_ways(&dir, zero());
    std::fs::write(dir.join("app.tamber"), zeroed()).unwrap();
    let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!(built.span.line, 7, "{built}");
    at_build(&w, &built, "app.contract");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A motion row the check leaves to the build names a token the write
/// changes, before a row it judges: the build's refusal of the motion row.
#[test]
fn a_motion_refusal_precedes_a_judged_row() {
    let dir = reading("rows-first-motion");
    let design = format!("{DESIGN}  duration.fast = 150ms\n");
    let written = design.replace("one = 1", "one = 0").replace("150ms", "8px");
    let source = format!("{HEAD}component App\n  view\n    column\n      text \"Motion\" transition=\"opacity duration.fast\"\n      button \"Go\" -exact-press-scale=\"one\"\n      Card()\n");
    std::fs::write(dir.join("app.contract"), &source).unwrap();
    std::fs::write(dir.join("app.tamber"), &written).unwrap();
    let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!(built.span.line, 5, "{built}");
    std::fs::write(dir.join("app.tamber"), &design).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap();
    // One write of both tokens, as a transaction's ops are one write.
    let both = json!({"op": "set-token", "name": "duration.fast", "value": "8px"});
    let (mut s, release) = held(&dir);
    let probe = json!({"method": "propose", "ops": [{"op": "set-token", "name": "ink", "value": "#444444"}]});
    s.ask(probe.clone());
    let write = |s: &mut Serve| {
        s.ask(json!({"method": "transaction", "client": "one",
            "files": {"app.tamber": digest(&dir.join("app.tamber"))},
            "ops": [zero(), both.clone()]}))
            .0
    };
    let unverified = write(&mut s);
    release(&mut s, probe);
    let w = write(&mut s);
    drop(s);
    assert_eq!(unverified["error"], w["error"]);
    at_build(&w, &built, "app.contract");
    assert_eq!(
        std::fs::read_to_string(dir.join("app.tamber")).unwrap(),
        design
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A choice of styles, whose rows the check leaves to the build, names a
/// token the write breaks before a row it judges: the build's refusal of
/// the style's row.
#[test]
fn a_style_choice_left_to_the_build_precedes_a_judged_row() {
    let design = "tamber t\n\ntheme\n  ink = #111111\n  gap = 8px\n";
    let contract = "style Pad\n  padding=\"gap\"\nstyle Flat\n  padding=0\ncomponent App\n  state on = false\n  view\n    column\n      text \"a\" class=(on ? Pad : Flat)\n      text \"b\" padding=\"gap\"\n";
    let dir = reading("rows-first-style-choice");
    std::fs::write(dir.join("app.tamber"), design).unwrap();
    std::fs::write(dir.join("app.contract"), contract).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap();
    let w = refused_both_ways(
        &dir,
        json!({"op": "set-token", "name": "gap", "value": "#333333"}),
    );
    std::fs::write(dir.join("app.tamber"), design.replace("8px", "#333333")).unwrap();
    let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!(built.span.line, 2, "{built}");
    at_build(&w, &built, "app.contract");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A token a synthetic expression names, made unable to fold there: the
/// design's refusal, which the build says before any element's, precedes
/// a row the write breaks earlier in the view.
#[test]
fn a_design_refusal_precedes_an_earlier_row() {
    let design = "tamber t\n\ntheme\n  ink = #111111\n  gap = 8px\n";
    let contract = "component App\n  view\n    column\n      text \"a\" color=\"ink\"\n      text \"b\" color=\"color-mix(in srgb, ink 50%, white)\"\n";
    let dir = reading("rows-first-design");
    std::fs::write(dir.join("app.tamber"), design).unwrap();
    std::fs::write(dir.join("app.contract"), contract).unwrap();
    contract::compile_path(&dir.join("app.contract")).unwrap();
    let w = refused_both_ways(
        &dir,
        json!({"op": "set-token", "name": "ink", "value": "8px"}),
    );
    std::fs::write(dir.join("app.tamber"), design.replace("#111111", "8px")).unwrap();
    let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!((built.span.line, built.span.col), (5, 22), "{built}");
    at_build(&w, &built, "app.contract");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The build's first refusal in a package's source is named there: a row
/// the check leaves to the build, and one it judges.
#[test]
fn the_first_refusal_keeps_its_package_source() {
    for (tag, row) in [
        ("rows-first-package", "same(pick)"),
        ("rows-first-package-judged", "\"one\""),
    ] {
        let dir = reading(tag);
        let source = format!("fn same(x: string): string = x\ncomponent Card\n  state pick = \"2\"\n  view\n    button \"A\" -exact-press-scale={row}\n");
        package(&dir, "reading", &source, "tamber reading\n");
        let root = format!("{HEAD}component App\n  state pick = \"2\"\n  view\n    column\n      Card()\n      button \"B\" -exact-press-scale=pick\n");
        std::fs::write(dir.join("app.contract"), &root).unwrap();
        std::fs::write(dir.join("app.tamber"), DESIGN).unwrap();
        contract::compile_path(&dir.join("app.contract")).unwrap();
        let w = refused_both_ways(&dir, zero());
        std::fs::write(dir.join("app.tamber"), zeroed()).unwrap();
        let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
        at_build(&w, &built, "node_modules/@acme/reading/index.contract");
        assert_eq!(
            std::fs::read_to_string(dir.join("node_modules/@acme/reading/index.contract")).unwrap(),
            source
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Until the background verdict arrives, a newly broken row still triggers
/// the build, whose first refusal may be one the file had before the write.
#[test]
fn an_existing_build_failure_precedes_a_newly_broken_row() {
    let dir = reading("rows-first-existing");
    let source = format!("{HEAD}component App\n  view\n    column\n      svg \"oops\" width=20 height=20\n      button \"Go\" -exact-press-scale=\"one\"\n      Card()\n");
    std::fs::write(dir.join("app.contract"), &source).unwrap();
    std::fs::write(dir.join("app.tamber"), DESIGN).unwrap();
    let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!(built.id, "lower-positional");
    let (mut s, _release) = held(&dir);
    let w = transaction(&mut s, &dir, zero());
    drop(s);
    at_build(&w, &built, "app.contract");
    assert_eq!(
        std::fs::read_to_string(dir.join("app.tamber")).unwrap(),
        DESIGN
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A bundled SVG image a native build refuses is the build's first refusal,
/// before every row, wherever the image is: a write breaking a row is
/// refused with it while the background verdict is unknown.
#[test]
fn an_asset_refusal_precedes_a_newly_broken_row() {
    let dir = reading("rows-first-asset");
    std::fs::write(
        dir.join("app.json"),
        r#"{"app":{"id":"dev.test.rows","name":"Rows"},"host":{"linux":{}}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("assets/picture.svg"), "<svg>\n<g>\n</svg>").unwrap();
    let source = format!("{HEAD}component App\n  view\n    column\n      button \"Go\" -exact-press-scale=\"one\"\n      image \"assets/picture.svg\"\n      Card()\n");
    std::fs::write(dir.join("app.contract"), &source).unwrap();
    std::fs::write(dir.join("app.tamber"), DESIGN).unwrap();
    let built = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!((built.id.as_str(), built.span.line), ("lower-image-svg", 6));
    let (mut s, _release) = held(&dir);
    let w = transaction(&mut s, &dir, zero());
    drop(s);
    at_build(&w, &built, "app.contract");
    std::fs::write(dir.join("app.tamber"), zeroed()).unwrap();
    let written = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!(written.id, built.id);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Each proposed design reuses the previous check's work only while the
/// current sources still justify it. None of the proposals reaches disk.
#[test]
fn reused_lowering_matches_fresh_lowering_after_design_and_source_edits() {
    let dir = reading("rows-reused-diagnostics");
    let design = format!("{DESIGN}  duration.fast = 150ms\n");
    let source = format!("{HEAD}component App\n  view\n    column\n      text \"Motion\" transition=\"opacity duration.fast\"\n      button \"Go\" -exact-press-scale=\"one\"\n      Card()\n");
    let root = dir.join("app.contract");
    let path = dir.join("app.tamber");
    std::fs::write(&root, &source).unwrap();
    std::fs::write(&path, &design).unwrap();
    let mut kept = contract::retention_build(&root, None).kept;
    let proposals = [
        design.replace("one = 1", "one = 0"),
        design.replace("one = 1", "one = 0").replace("150ms", "8px"),
        design.replace("one = 1", "one = other\n  other = 0"),
        design.replace("  one = 1\n", ""),
        design.clone(),
    ];
    let diagnostic = |result: Result<(), contract::CompileError>| {
        result.map_err(|e| (e.id, e.message, e.span, e.file))
    };
    for package_source in [
        super::tamber_package_params::CARD,
        "component Card\n  view\n    svg \"oops\"\n",
    ] {
        std::fs::write(
            dir.join("node_modules/@acme/reading/index.contract"),
            package_source,
        )
        .unwrap();
        for after in &proposals {
            contract::with_design_text(&path, after.clone(), || {
                let built = diagnostic(contract::compile_path(&root).map(|_| ()));
                let first = contract::check_path_reusing(&root, &mut kept, contract::Stop::Refusal);
                assert_eq!(
                    diagnostic(first.first.map_or(Ok(()), Err)),
                    built,
                    "{after}\n{package_source}"
                );
                // A refusal before the view is the build's first.
                let view = contract::check_path_reusing(&root, &mut kept, contract::Stop::View);
                if let Some(e) = view.first {
                    assert_eq!(diagnostic(Err(e)), built, "{after}\n{package_source}");
                }
            });
            assert_eq!(std::fs::read_to_string(&path).unwrap(), design);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
