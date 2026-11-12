//! Scope lookups and slot re-typing: what an index or a narrower refresh
//! must still answer as the scans they replaced did.
use crate::{ComponentTypes, Ref, Scope, Sink, Ty, Types};
use contract_syntax::Component;

/// Run after each body: the scope `retype_written` left is the one
/// re-typing every slot gives.
pub(crate) fn all_slots_retyped(scope: &Scope, c: &Component, ct: &ComponentTypes) {
    let first = c.props.len() + c.injects.len();
    let mut all = scope.clone();
    for (i, ty) in ct.slots.iter().enumerate() {
        all.retype(first + i, ty.clone());
    }
    let types = |s: &Scope| {
        s.frames[0]
            .names
            .iter()
            .map(|n| n.2.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        types(scope),
        types(&all),
        "a slot changed that the body did not assign"
    );
}

/// Run before each caller's calls refine their callees: the declarations
/// kept up to date are the ones a fresh scope would build.
pub(crate) fn same_declarations(kept: &Scope, fresh: &Scope) {
    let names = |s: &Scope| s.frames[0].names.clone();
    assert_eq!(
        names(kept),
        names(fresh),
        "a callee's refined type was not kept"
    );
}

fn names(n: u32) -> Vec<(String, Ref, Ty)> {
    (0..n)
        .map(|i| (format!("n{i}"), Ref::Slot(i), Ty::Number))
        .collect()
}

#[test]
fn an_indexed_frame_finds_a_names_first_entry() {
    let mut frame = names(20);
    frame[17].0 = "n3".into();
    let mut scope = Scope::default();
    scope.push(frame);
    assert_eq!(scope.lookup("n3").map(|(r, _)| r), Some(Ref::Slot(3)));
    assert_eq!(scope.lookup("n19").map(|(r, _)| r), Some(Ref::Slot(19)));
    assert_eq!(scope.lookup("n17").map(|(r, _)| r), None);
}

#[test]
fn an_inner_frame_shadows_an_indexed_outer_one() {
    let mut scope = Scope::default();
    scope.push(names(20));
    let mut inner = names(20);
    for (i, entry) in inner.iter_mut().enumerate() {
        entry.1 = Ref::Param(i as u32);
    }
    scope.push(inner);
    assert_eq!(scope.lookup("n5").map(|(r, _)| r), Some(Ref::Param(5)));
    scope.push_region(Some(("n6".into(), Ref::Item(0), Ty::String)));
    assert_eq!(
        scope.lookup("n6"),
        Some((Ref::Item(0), &Ty::String)),
        "the innermost binding wins"
    );
    assert_eq!(scope.lookup("n7").map(|(r, _)| r), Some(Ref::Param(7)));
    scope.pop();
    scope.pop();
    assert_eq!(scope.lookup("n5").map(|(r, _)| r), Some(Ref::Slot(5)));
}

#[test]
fn a_region_frame_counts_depth_past_an_indexed_frame() {
    let mut scope = Scope::default();
    scope.push_region(Some(("row".into(), Ref::Item(0), Ty::String)));
    let mut frame = names(20);
    frame.push(("row".into(), Ref::Item(0), Ty::Number));
    scope
        .frames
        .push(std::sync::Arc::new(crate::frame::Frame::new(frame, true)));
    scope.push_region(Some(("cell".into(), Ref::Item(0), Ty::String)));
    assert_eq!(scope.lookup("row"), Some((Ref::Item(1), &Ty::Number)));
    assert_eq!(scope.lookup("cell"), Some((Ref::Item(0), &Ty::String)));
}

#[test]
fn a_name_added_after_a_lookup_is_found() {
    let mut scope = Scope::default();
    scope.push(names(20));
    let shared = scope.clone();
    assert_eq!(scope.lookup("late"), None);
    scope.push_name_at(0, ("late".into(), Ref::Slot(20), Ty::String));
    scope.push_name_at(0, ("n2".into(), Ref::Slot(21), Ty::String));
    assert_eq!(scope.lookup("late").map(|(r, _)| r), Some(Ref::Slot(20)));
    assert_eq!(scope.lookup("n2").map(|(r, _)| r), Some(Ref::Slot(2)));
    assert_eq!(
        shared.lookup("late"),
        None,
        "a shared frame is copied, not grown"
    );
}

#[test]
fn refreshing_only_what_changed_matches_building_again() {
    // Under test, `retype_written` asserts that the scope it leaves equals
    // one with every slot re-typed, and `calls::refine` that its kept
    // declarations equal a fresh scope's. Random components drive both
    // through writes that settle, widen and refuse a slot's type, and calls
    // that type a callee's parameter, reading both in later bodies.
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    let (mut checked, mut settled, mut refined) = (0, 0, 0);
    while checked < 300 {
        // Each state is a number, a string, or an option a write settles;
        // the first is a number.
        let kinds: Vec<usize> = (0..1 + next(24))
            .map(|i| if i == 0 { 0 } else { next(3) })
            .collect();
        let of = |kind: usize, next: &mut dyn FnMut(usize) -> usize| {
            let same: Vec<usize> = (0..kinds.len()).filter(|&i| kinds[i] == kind).collect();
            same[next(same.len())]
        };
        let mut source = String::from("component App\n");
        for (i, kind) in kinds.iter().enumerate() {
            let init = ["0", "\"t\"", "none"][*kind];
            source.push_str(&format!("  state s{i} = {init}\n"));
        }
        // Callees with an untyped parameter, which calls type.
        let callees = 1 + next(4);
        for j in 0..callees {
            source.push_str(&format!(
                "  action c{j}(q)\n    s{} = q\n",
                of(0, &mut next)
            ));
        }
        for a in 0..1 + next(24) {
            source.push_str(&format!("  action a{a}(p: number)\n"));
            for _ in 0..1 + next(3) {
                let target = next(kinds.len());
                let value = match (kinds[target], next(2)) {
                    (0, 0) => "p".to_string(),
                    (0, _) => format!("s{}", of(0, &mut next)),
                    (1, _) => "\"u\"".to_string(),
                    (_, 0) => "some(p)".to_string(),
                    (_, _) => format!("s{}", of(2, &mut next)),
                };
                match next(4) {
                    0 => source.push_str(&format!("    c{}(p)\n", next(callees))),
                    _ => source.push_str(&format!("    s{target} = {value}\n")),
                }
            }
        }
        // Every option is settled and every callee called somewhere.
        source.push_str("  action last(p: number)\n");
        for (i, kind) in kinds.iter().enumerate() {
            if *kind == 2 {
                source.push_str(&format!("    s{i} = some(p)\n"));
            }
        }
        for j in 0..callees {
            source.push_str(&format!("    c{j}(p)\n"));
        }
        source.push_str("  view\n    text \"ok\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        // The whole check, as a build runs it: calls between actions are
        // resolved before the component is.
        checked += 1;
        let done = match crate::check_all(&file, false, |_| false, None) {
            Ok(done) => done,
            Err(e) => panic!(
                "{source}\n{:?}",
                e.iter().map(|e| &e.message).collect::<Vec<_>>()
            ),
        };
        let ct = &done.types.components[0];
        settled += ct
            .slots
            .iter()
            .filter(|t| matches!(t, Ty::Option(inner) if **inner != Ty::Unknown))
            .count();
        refined += ct.actions[..callees]
            .iter()
            .filter(|params| params[0] != Ty::Unknown)
            .count();
    }
    // The writes did settle options, and calls did type their callees'
    // parameters.
    assert!(settled > 300, "{settled} settled slots");
    assert!(refined > 300, "{refined} refined parameters");
}

#[test]
fn a_parameter_shadows_a_state_in_a_large_component() {
    // Twenty states make the declarations frame indexed; the parameter's
    // frame is searched first all the same.
    let mut source = String::from("component App\n  state count = 0\n");
    for i in 0..20 {
        source.push_str(&format!("  state pad{i} = \"\"\n"));
    }
    source.push_str("  state n = \"text\"\n  action set(n: number)\n    count = n\n");
    source.push_str("  view\n    text \"ok\"\n");
    let file = contract_syntax::parse(&source).unwrap();
    let mut sink = Sink::default();
    let ct =
        crate::component::check_component(&file.components[0], &Types::default(), None, &mut sink);
    assert!(sink.errors.is_empty(), "{:?}", sink.errors);
    assert_eq!(ct.slots[0], Ty::Number);
}

#[test]
fn untyped_action_parameters_need_a_handler_or_call_argument() {
    let source = |caller: &str, view: &str| {
        format!("component App\n  action keep(value)\n    let unused = value\n{caller}  view\n    {view}\n")
    };
    for (caller, view, expected) in [
        ("", "input change=keep", Ty::String),
        ("", "button \"Keep\" press=keep(7)", Ty::Number),
        (
            "  action go\n    keep(7)\n",
            "button \"Keep\" press=go",
            Ty::Number,
        ),
        (
            "  action go\n    keep([])\n    keep([7])\n",
            "button \"Keep\" press=go",
            Ty::List(Box::new(Ty::Number)),
        ),
    ] {
        let file = contract_syntax::parse(&source(caller, view)).unwrap();
        let checked = crate::check(&file, |_| false).unwrap();
        assert_eq!(checked.types.components[0].actions[0], [expected]);
    }
    let file = contract_syntax::parse(&source("", "text \"ok\"")).unwrap();
    let error = crate::check(&file, |_| false).err().unwrap();
    assert_eq!(error.id, "type-cannot-infer");
    assert!(error.message.contains("parameter `value`"), "{error}");
    assert_eq!((error.span.line, error.span.col), (2, 15));
}

#[test]
fn a_refusal_is_not_said_again_as_the_capture_it_left_untyped() {
    // `wide` is `?` once `rem` is refused, so the capture `note(wide > 1)`
    // curries is too: that is the refusal's consequence, and saying it
    // would ask for a handler `go` already has.
    let source = "component App\n  view\n    Panel()\n\
        component Panel\n  state last = \"\"\n  derive wide = 24 * rem\n\
        \x20 action note(w: bool, label: string)\n    last = `${label} ${w}`\n\
        \x20 view\n    Leaf(label=\"a\", tell=note(wide > 1))\n\
        component Leaf\n  props\n    label: string\n    tell: action\n\
        \x20 action go\n    tell(label)\n  view\n    button \"Go\" press=go\n";
    let file = contract_syntax::parse(source).unwrap();
    let Err(errors) = crate::check_all(&file, false, |_| false, None) else {
        panic!("refused");
    };
    let ids: Vec<_> = errors.iter().map(|e| (e.id, e.message.as_str())).collect();
    assert_eq!(ids, [("type-unknown-name", "unknown name `rem`")]);
}

#[test]
fn an_untyped_none_in_a_derive_is_refused_at_the_literal() {
    for expr in ["none", "true ? none : none", "[none]", "some([])"] {
        let source = format!("component App\n  derive first = {expr}\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
        let e = &errors[0];
        assert_eq!(e.id, "type-cannot-infer", "{source}\n{errors:?}");
        let (literal, context) = if expr.contains("none") {
            ("none", "an `option<T>`")
        } else {
            ("[]", "a `T[]`")
        };
        assert_eq!(
            e.message,
            format!("cannot infer what `{literal}` holds in `first`: pair it with a typed arm (`match`/`?:`) or write it where {context} is declared")
        );
        assert_eq!(
            (e.span.line, e.span.col),
            (2, 18 + expr.find(literal).unwrap() as u32)
        );
    }
}

#[test]
fn an_incomplete_derives_dependents_do_not_repeat_its_refusal() {
    for expr in [
        "none",
        "true ? none : none",
        "[]",
        "true ? [] : []",
        "missing",
    ] {
        for declarations in [
            format!("  derive first = {expr}\n  derive second = first\n  derive third = second\n"),
            format!("  derive third = second\n  derive second = first\n  derive first = {expr}\n"),
        ] {
            let source = format!("component App\n{declarations}  view\n    text \"x\"\n");
            let file = contract_syntax::parse(&source).unwrap();
            let errors = crate::check_all(&file, false, |_| false, None)
                .err()
                .unwrap();
            assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
            let id = if expr == "missing" {
                "type-unknown-name"
            } else {
                "type-cannot-infer"
            };
            assert_eq!(errors[0].id, id, "{source}\n{errors:?}");
        }
    }
}

#[test]
fn derives_reading_an_untyped_state_leave_its_refusal_to_the_state() {
    for expr in ["none", "[]"] {
        let source = format!("component App\n  state picked = {expr}\n  derive later = current\n  derive current = picked\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
        assert_eq!(errors[0].id, "type-cannot-infer");
        assert_eq!(
            errors[0].message,
            "cannot infer the type of `picked`: nothing writes a value into it"
        );
        assert_eq!((errors[0].span.line, errors[0].span.col), (2, 3));
    }
}

#[test]
fn incomplete_derive_diagnostics_keep_cycles_and_independent_causes() {
    for expr in ["second", "true ? second : none", "true ? second : []"] {
        let source = format!("component App\n  derive first = {expr}\n  derive second = first\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 2, "{source}\n{errors:?}");
        assert!(
            errors.iter().all(|e| e.id == "type-derive-cycle"),
            "{errors:?}"
        );
    }
    for expr in [
        "none",
        "on ? none : none",
        "match some(1) { case some(picked) => none, case none => none }",
    ] {
        let source = format!("component App\n  state picked = none\n  derive on = picked == none\n  derive first = {expr}\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 2, "{source}\n{errors:?}");
        assert!(
            errors.iter().all(|e| e.id == "type-cannot-infer"),
            "{errors:?}"
        );
        assert!(errors[0].message.contains("in `first`"), "{errors:?}");
        assert!(errors[1].message.contains("of `picked`"), "{errors:?}");
    }
}

#[test]
fn typed_arms_and_state_writes_still_complete_derives() {
    for expr in [
        "true ? none : some(1)",
        "match some(1) { case some(n) => some(n), case none => none }",
        "true ? [] : [1]",
        "picked",
    ] {
        let source = format!("component App\n  state picked = none\n  derive first = {expr}\n  derive second = first\n  action pick\n    picked = some(1)\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let result = crate::check_all(&file, false, |_| false, None);
        assert!(result.is_ok(), "{source}\n{:?}", result.err());
    }
}

#[test]
fn a_refused_build_types_the_roots_uses_with_one_scope() {
    // A refusal re-checks the root's uses against its declarations alone
    // (`provisional_scope`); a scope built again for each state made that
    // quadratic in a large expanded root.
    let mut source = String::from("component App\n");
    for i in 0..400 {
        source.push_str(&format!("  state s{i} = {i}\n"));
    }
    source.push_str("  derive d = s1 + s399\n  derive e = d + 1\n  view\n    Child(n=e)\n");
    source
        .push_str("component Child\n  props\n    n: number\n  view\n    text `${n} ${missing}`\n");
    let file = contract_syntax::parse(&source).unwrap();
    crate::component::SCOPE_BUILDS.with(|count| count.set(0));
    let Err(errors) = crate::check_all(&file, false, |_| false, None) else {
        panic!("refused");
    };
    assert_eq!(errors[0].id, "type-unknown-name", "{errors:?}");
    let builds = crate::component::SCOPE_BUILDS.with(|count| count.get());
    assert!(builds < 100, "{builds} declaration scopes for 400 states");
}

#[test]
fn incomplete_derives_are_checked_after_handler_writes_type_states() {
    for (expr, refused) in [("(picked == none) ? [] : []", true), ("picked", false)] {
        let source = format!("component App\n  state picked = none\n  derive first = {expr}\n  derive second = first\n  action pick(v)\n    picked = some(v)\n  view\n    button \"Go\" press=pick(1)\n");
        let file = contract_syntax::parse(&source).unwrap();
        let result = crate::check_all(&file, false, |_| false, None);
        if refused {
            let errors = result.err().expect("untyped list refused before lowering");
            assert_eq!(errors.len(), 1, "{errors:?}");
            assert_eq!(errors[0].id, "type-cannot-infer", "{errors:?}");
            assert_eq!((errors[0].span.line, errors[0].span.col), (3, 37));
        } else {
            let checked = result.unwrap();
            assert_eq!(
                checked.types.components[0].derives,
                vec![Ty::Option(Box::new(Ty::Number)); 2]
            );
        }
    }
}

#[test]
fn handler_typed_derives_are_checked_before_assignments() {
    for actions in [
        "  action pick(v)\n    picked = some(v)\n  action copy\n    target = first\n",
        "  action copy\n    target = first\n  action pick(v)\n    picked = some(v)\n",
        "  action pick(v)\n    picked = some(v)\n    target = first\n",
    ] {
        let source = format!("component App\n  state picked = none\n  state target = some(\"text\")\n  derive first = picked\n{actions}  view\n    button \"Pick\" press=pick(1)\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .expect("a number option cannot be assigned to a string option");
        assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
        assert_eq!(errors[0].id, "type-assign", "{source}\n{errors:?}");
        assert!(errors[0].message.contains("option<number>"), "{errors:?}");
        let assignment_line = source
            .lines()
            .position(|line| line.trim() == "target = first")
            .unwrap()
            + 1;
        assert_eq!(errors[0].span.line, assignment_line as u32);
    }
}

#[test]
fn handler_typed_writes_and_derives_settle_before_source_signatures() {
    let source = "component App\n  state first = none\n  state second = none\n  state third = none\n  derive a = first\n  derive b = second\n  derive c = third\n  mutation changed as shape string\n  action publish\n    send changed = out(c)\n  action copyLast\n    third = b\n  action copyFirst\n    second = a\n  action pick(v)\n    first = some(v)\n  view\n    button \"Pick\" press=pick(1)\n";
    let file = contract_syntax::parse(source).unwrap();
    let checked = crate::check_all(&file, false, |_| false, None)
        .unwrap_or_else(|errors| panic!("{errors:?}"));
    let ct = &checked.types.components[0];
    assert_eq!(ct.slots, vec![Ty::Option(Box::new(Ty::Number)); 3]);
    assert_eq!(ct.derives, ct.slots);
    assert_eq!(ct.sources["out"].0, vec![Ty::Option(Box::new(Ty::Number))]);
}

#[test]
fn a_typed_cycle_does_not_hide_an_untyped_literal() {
    let source =
        "component App\n  derive a = a == a\n  derive d = a ? [] : []\n  view\n    text \"x\"\n";
    let file = contract_syntax::parse(source).unwrap();
    let errors = crate::check_all(&file, false, |_| false, None)
        .err()
        .expect("the untyped list is refused before lowering");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].id, "type-cannot-infer", "{errors:?}");
    assert_eq!((errors[0].span.line, errors[0].span.col), (3, 18));
}

#[test]
fn a_derive_cause_precedes_its_readers_in_either_declaration_order() {
    for declarations in [
        "  derive first = []\n  derive second = first.map(x => x.n)\n",
        "  derive second = first.map(x => x.n)\n  derive first = []\n",
    ] {
        let source = format!("component App\n{declarations}  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
        assert_eq!(errors[0].id, "type-cannot-infer", "{errors:?}");
        assert!(errors[0].message.contains("in `first`"), "{errors:?}");
        assert_eq!(errors[0].span.col, 18);
    }
}

#[test]
fn derive_dependencies_exclude_call_heads_and_local_bindings() {
    for expr in [
        "first([])",
        "[].map(first => first)",
        "match some([]) { case some(first) => first, case none => [] }",
    ] {
        let source = format!("component App\n  derive first = {expr}\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 1, "{source}\n{errors:?}");
        assert_eq!(errors[0].id, "type-cannot-infer", "{source}\n{errors:?}");
        assert_eq!(
            (errors[0].span.line, errors[0].span.col),
            (2, 18 + expr.find("[]").unwrap() as u32)
        );
    }
}

#[test]
fn a_local_binding_does_not_propagate_an_unrelated_derive_refusal() {
    let source = "component App\n  derive ok = [].map(bad => bad)\n  derive bad = missing\n  view\n    text \"x\"\n";
    let file = contract_syntax::parse(source).unwrap();
    let errors = crate::check_all(&file, false, |_| false, None)
        .err()
        .unwrap();
    let ids: Vec<_> = errors.iter().map(|e| e.id).collect();
    assert_eq!(
        ids,
        ["type-cannot-infer", "type-unknown-name"],
        "{errors:?}"
    );
}

#[test]
fn incomplete_derive_cycles_are_reported_only_for_their_members() {
    for (declarations, mut expected) in [
        (
            "  state picked = none\n  derive first = (picked == none) ? second : second\n  derive second = first\n",
            vec![("type-derive-cycle", 3), ("type-derive-cycle", 4), ("type-cannot-infer", 2)],
        ),
        (
            "  derive c = a\n  derive a = b\n  derive b = a\n",
            vec![("type-derive-cycle", 3), ("type-derive-cycle", 4)],
        ),
        (
            "  derive a = true ? b : c\n  derive b = a\n  derive c = b\n  derive d = a\n",
            vec![("type-derive-cycle", 2), ("type-derive-cycle", 3), ("type-derive-cycle", 4)],
        ),
        (
            "  derive xs = xs.map(x => x)\n",
            vec![("type-derive-cycle", 2)],
        ),
    ] {
        let source = format!("component App\n{declarations}  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None).err().unwrap();
        let mut actual: Vec<_> = errors.iter().map(|e| (e.id, e.span.line)).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "{source}\n{errors:?}");
    }
}

#[test]
fn an_incomplete_derive_does_not_blame_literals_with_typed_context() {
    for expr in [
        "(true ? none : some(1)) == none ? [] : []",
        "match (true ? none : some(1)) { case some(n) => [], case none => [] }",
        "[1].map(n => (true ? none : some(n)) == none ? [] : [])",
    ] {
        let source = format!("component App\n  derive first = {expr}\n  view\n    text \"x\"\n");
        let file = contract_syntax::parse(&source).unwrap();
        let errors = crate::check_all(&file, false, |_| false, None)
            .err()
            .unwrap();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].id, "type-cannot-infer", "{errors:?}");
        assert!(errors[0].message.contains("`[]`"), "{errors:?}");
        assert_eq!(
            (errors[0].span.line, errors[0].span.col),
            (2, 18 + expr.find("[]").unwrap() as u32)
        );
    }
}

#[test]
fn a_derive_dependency_in_a_let_initializer_is_outside_its_binding() {
    for (value, id) in [("[]", "type-cannot-infer"), ("first", "type-derive-cycle")] {
        let source = format!("component App\n  derive first = {value}\n  view\n    text \"x\"\n");
        let mut file = contract_syntax::parse(&source).unwrap();
        let d = &mut file.components[0].derives[0];
        // Inlining generates expression lets; there is no surface syntax.
        d.expr = contract_syntax::Expr::Let {
            name: "first".into(),
            value: Box::new(d.expr.clone()),
            body: Box::new(contract_syntax::Expr::Ident("first".into(), d.span)),
            span: d.span,
        };
        let mut sink = Sink::default();
        crate::component::check_component(&file.components[0], &Types::default(), None, &mut sink);
        assert_eq!(sink.errors.len(), 1, "{:?}", sink.errors);
        assert_eq!(sink.errors[0].id, id, "{:?}", sink.errors);
    }
}
