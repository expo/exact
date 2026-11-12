//! Refusals that name the input that would succeed (LLP 1156.002 D8, D9):
//! a handler chosen by a condition, and a helper written above the root.

fn refused(src: &str) -> contract::CompileError {
    contract::compile(src).expect_err("refused")
}

#[test]
fn a_conditional_handler_is_told_to_be_one_action_that_chooses() {
    let e = refused(
        "component App\n  state searching = false\n  action close\n    searching = false\n  action closeSearch\n    searching = false\n  view\n    button \"Done\" press=(searching ? closeSearch : close)\n",
    );
    assert_eq!(e.id, "analyze-handler-shape");
    assert!(
        e.message
            .contains("not a condition: write one action that chooses"),
        "{}",
        e.message
    );
    assert!(e.message.contains("`press=pick`"), "{}", e.message);
}

#[test]
fn a_conditional_action_prop_is_refused_at_its_argument() {
    let source = |value: &str| {
        format!(
            "component App\n  state n = 0\n  state flag = true\n  action a()\n    n = 1\n  action b()\n    n = 2\n  view\n    column\n      Child(tap={value})\n\ncomponent Child\n  props\n    tap: action\n  view\n    button press=tap testId=\"go\"\n      text \"go\"\n"
        )
    };
    for handler in ["tap", "tap()"] {
        let source = |value: &str| source(value).replace("press=tap", &format!("press={handler}"));
        for value in ["flag ? a : b", "flag ? a() : b()"] {
            let e = refused(&source(value));
            assert_eq!(e.id, "type-action-prop-shape", "{e}");
            assert_eq!((e.span.line, e.span.col, e.span.end_col), (10, 13, 16));
            assert!(e.message.contains("`tap`"), "{e}");
            assert!(
                e.message.contains("an action name or `action(args)`"),
                "{e}"
            );
            assert!(e.message.contains("write one action that chooses"), "{e}");
        }
        for value in ["a", "a()"] {
            contract::compile(&source(value)).unwrap();
        }
        let fixed = source("pick").replacen(
            "  view\n",
            "  action pick\n    if flag\n      a()\n    else\n      b()\n  view\n",
            1,
        );
        contract::compile(&fixed).unwrap();
    }
}

#[test]
fn a_conditional_action_provider_is_refused_at_its_binding() {
    for handler in ["tap", "tap()"] {
        let source = |value: &str| {
            format!(
                "component App\n  state n = 0\n  state flag = true\n  action a()\n    n = 1\n  action b()\n    n = 2\n  provide\n    tap = {value}\n  view\n    Child()\n\ncomponent Child\n  inject\n    tap: action\n  view\n    button \"go\" press={handler}\n"
            )
        };
        for value in ["flag ? a : b", "flag ? a() : b()"] {
            let e = refused(&source(value));
            assert_eq!(e.id, "type-action-provide-shape", "{e}");
            assert_eq!((e.span.line, e.span.col, e.span.end_col), (9, 5, 8));
            assert!(e.message.contains("`tap`"), "{e}");
            assert!(
                e.message.contains("an action name or `action(args)`"),
                "{e}"
            );
            assert!(e.message.contains("write one action that chooses"), "{e}");
        }
        for value in ["a", "a()"] {
            contract::compile(&source(value)).unwrap();
        }
        let fixed = source("pick").replacen(
            "  provide\n",
            "  action pick\n    if flag\n      a()\n    else\n      b()\n  provide\n",
            1,
        );
        contract::compile(&fixed).unwrap();
    }
}

#[test]
fn conditional_action_providers_follow_shadowing_and_slot_scope() {
    let source = |view: &str, wrapper: &str| {
        format!(
            "component App\n  state n = 0\n  state flag = true\n  action a\n    n = 1\n  action b\n    n = 2\n  provide\n    tap = flag ? a : b\n  view\n    {view}\n\ncomponent Wrap\n  slot\n  action local\n    focus(\"go\")\n  provide\n    tap = local\n  view\n    {wrapper}\n\ncomponent Child\n  inject\n    tap: action\n  view\n    button \"go\" press=tap() testId=\"go\"\n"
        )
    };
    contract::compile(&source("Wrap()", "Child()")).unwrap();
    let e = refused(&source("Wrap()\n      Child()", "children"));
    assert_eq!(e.id, "type-action-provide-shape", "{e}");
    assert_eq!((e.span.line, e.span.col, e.span.end_col), (9, 5, 8));
}

#[test]
fn a_derived_action_choice_is_refused_at_its_binding() {
    for provided in [false, true] {
        for handler in ["tap", "tap()"] {
            let (binding, view, section, id, col) = if provided {
                (
                    "  provide\n    tap = chosen\n",
                    "Child()",
                    "inject",
                    "type-action-provide-shape",
                    5,
                )
            } else {
                (
                    "",
                    "Child(tap=chosen)",
                    "props",
                    "type-action-prop-shape",
                    11,
                )
            };
            let source = format!(
                "component App\n  state n = 0\n  state flag = true\n  action a\n    n = 1\n  action b\n    n = 2\n  derive chosen = flag ? a : b\n{binding}  view\n    {view}\n\ncomponent Child\n  {section}\n    tap: action\n  view\n    button \"go\" press={handler}\n"
            );
            for nested in [false, true] {
                let source = if nested {
                    format!("component Root\n  view\n    App()\n\n{source}")
                } else {
                    source.clone()
                };
                let e = refused(&source);
                assert_eq!(e.id, id, "{e}");
                assert_eq!(
                    (e.span.line, e.span.col),
                    (if nested { 14 } else { 10 }, col),
                    "{e}"
                );
                assert!(e.message.contains("write one action that chooses"), "{e}");
            }
        }
    }
}

#[test]
fn child_action_aliases_resolve_before_binding_validation() {
    for provided in [false, true] {
        let (binding, view, section) = if provided {
            ("  provide\n    tap = chosen\n", "Child()", "inject")
        } else {
            ("", "Child(tap=chosen)", "props")
        };
        for (params, value, body) in [("", "a", "1"), ("(x: number)", "a(7)", "x")] {
            for derives in [
                format!("  derive chosen = {value}\n"),
                format!("  derive chosen = alias\n  derive alias = {value}\n"),
            ] {
                for handler in ["tap", "tap()"] {
                    let source = format!(
                        "component App\n  view\n    Wrap()\n\ncomponent Wrap\n  state n = 0\n  action a{params}\n    n = {body}\n{derives}{binding}  view\n    {view}\n\ncomponent Child\n  {section}\n    tap: action\n  view\n    button \"go\" press={handler}\n"
                    );
                    let plan =
                        contract::compile(&source).unwrap_or_else(|e| panic!("{source}\n{e}"));
                    let direct = source
                        .replace("tap=chosen", &format!("tap={value}"))
                        .replace("tap = chosen", &format!("tap = {value}"));
                    assert_eq!(plan.encode(), contract::compile(&direct).unwrap().encode());
                }
            }
        }
    }
}

#[test]
fn a_helper_above_the_root_is_told_to_move_below_it() {
    let e = refused(
        "component Card\n  props\n    title: string\n  view\n    text \"card\"\n\ncomponent App\n  view\n    Card(title=\"a\")\n",
    );
    assert_eq!(e.id, "analyze-root-props", "{}", e.message);
    assert!(
        e.message
            .ends_with("if `Card` is a helper, move it below the component the app starts from"),
        "{}",
        e.message
    );
    // A file of one component has nothing to move.
    let alone = refused("component App\n  props\n    title: string\n  view\n    text \"card\"\n");
    assert_eq!(
        alone.message,
        "the root component (the first in the file) takes no props"
    );
}

#[test]
fn action_refusals_keep_independent_child_use_errors() {
    let sibling = "component Wrap\n  view\n    Label(count=\"bad\")\n\ncomponent Label\n  props\n    count: number\n  view\n    text `${count}`\n";
    let unknown = format!(
        "component App\n  action go\n    missing()\n  view\n    button \"go\" press=go\n    Wrap()\n\n{sibling}"
    );
    let unknown_errors =
        contract::compile_path_source_all(std::path::Path::new("app.contract"), &unknown, false)
            .err()
            .unwrap();
    assert_eq!(
        unknown_errors
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        ["type-prop", "type-unknown-command"],
        "{unknown_errors:?}"
    );
    let mut cases = Vec::new();
    for provided in [false, true] {
        let (binding, view, section, id) = if provided {
            (
                "  provide\n    tap = flag ? a : b\n",
                "Child()",
                "inject",
                "type-action-provide-shape",
            )
        } else {
            (
                "",
                "Child(tap=flag ? a : b)",
                "props",
                "type-action-prop-shape",
            )
        };
        for nested in [false, true] {
            for handler in ["tap", "tap()"] {
                let caller = format!(
                    "component Caller\n  state flag = true\n  action a\n    focus(\"go\")\n  action b\n    focus(\"go\")\n{binding}  view\n    {view}\n    Wrap()\n\ncomponent Child\n  {section}\n    tap: action\n  view\n    button \"go\" press={handler}\n\n{sibling}"
                );
                let source = if nested {
                    format!("component App\n  view\n    Caller()\n\n{caller}")
                } else {
                    caller
                };
                cases.push((source, id));
            }
        }
    }
    let mut failures = Vec::new();
    for (source, id) in cases {
        let errors =
            contract::compile_path_source_all(std::path::Path::new("app.contract"), &source, false)
                .err()
                .unwrap();
        if errors.iter().map(|e| e.id.as_str()).collect::<Vec<_>>() != [id, "type-prop"] {
            failures.push(format!("{source}\n{errors:?}"));
            continue;
        }
        assert_eq!(
            errors[1].message,
            "`count` expects `number`, given `string`"
        );
        assert_eq!(
            source.lines().nth(errors[1].span.line as usize - 1),
            Some("    Label(count=\"bad\")")
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn an_action_body_call_keeps_priority_without_hiding_child_use_errors() {
    let source = "component App\n  state n = 0\n  view\n    Child(go=n + 1)\n    Wrap()\n\ncomponent Child\n  props\n    go: action\n  action tap\n    go()\n  view\n    button \"go\" press=tap\n\ncomponent Wrap\n  view\n    Label(count=\"bad\")\n\ncomponent Label\n  props\n    count: number\n  view\n    text `${count}`\n";
    let errors =
        contract::compile_path_source_all(std::path::Path::new("app.contract"), source, false)
            .err()
            .unwrap();
    assert_eq!(
        errors
            .iter()
            .map(|e| (e.id.as_str(), e.span.line))
            .collect::<Vec<_>>(),
        [
            ("syntax-call-target", 4),
            ("type-action-prop-shape", 4),
            ("type-prop", 17)
        ],
        "{errors:?}"
    );
}

#[test]
fn an_action_held_outside_a_prop_is_refused_where_it_is_written() {
    let app = |shapes: &str, decls: &str| {
        format!(
            "{shapes}component App\n  state n = 0\n{decls}  action bump\n    n = n + 1\n  view\n    column\n      text `${{n}}`\n"
        )
    };
    let cases = [
        (
            app("shape MenuItem\n  label: string\n  run: action\n\n", ""),
            (3, 8),
            "field `run` of shape `MenuItem` cannot hold an action",
        ),
        (
            app("shape MenuItem\n  run: option<action>\n\n", ""),
            (2, 15),
            "field `run` of shape `MenuItem` cannot hold an action",
        ),
        (
            app("", "  state f: option<action> = none\n"),
            (3, 19),
            "state `f` cannot hold an action",
        ),
        (
            app("", "  mutation m as shape action\n"),
            (3, 23),
            "mutation `m` cannot hold an action",
        ),
        (
            app("", "  resource r = load() as shape action\n"),
            (3, 32),
            "resource `r` cannot hold an action",
        ),
        (
            app("", "  action run(f: action)\n    n = 1\n"),
            (3, 17),
            "parameter `f` of action `run` cannot hold an action",
        ),
        (
            app("fn pick(a: action): number = 1\n\n", ""),
            (1, 12),
            "parameter `a` of `fn pick` cannot hold an action",
        ),
        (
            app("", "  derive fs = [bump]\n"),
            (3, 16),
            "`bump` is an action, which a list cannot hold",
        ),
        (
            app("", "  derive f = some(bump)\n"),
            (3, 19),
            "`bump` is an action, which `some(…)` cannot hold",
        ),
    ];
    for (src, at, lead) in cases {
        let e = refused(&src);
        assert_eq!(e.id, "type-action-held", "{src}\n{e}");
        assert_eq!((e.span.line, e.span.col), at, "{src}\n{e}");
        assert!(e.message.starts_with(&format!("{lead}: ")), "{e}");
        assert!(e.message.contains("`press=open(item.id)`"), "{e}");
    }
    // A prop holds one.
    let prop = app("", "").replace(
        "      text `${n}`\n",
        "      Row(go=bump)\n\ncomponent Row\n  props\n    go: action\n  view\n    button \"Go\" press=go\n",
    );
    contract::compile(&prop).unwrap();
}

#[test]
fn a_typed_states_initializer_is_refused_as_the_states() {
    let remedy = ": start it from a value of the declared type, or declare the type it holds";
    let shapes = "shape Item\n  id: string\n";
    for (decl, at, says) in [
        ("n: number = \"a\"", (6, 21), "state `n` is declared `number`, and its initializer is `string`"),
        (
            "n: option<number> = some(\"a\")",
            (6, 29),
            "state `n` is declared `option<number>`, and its initializer is `option<string>`",
        ),
        ("n: Item[] = [1]", (6, 21), "state `n` is declared `Item[]`, and its initializer is `number[]`"),
        (
            "n: enum<\"a\", \"b\"> = \"c\"",
            (6, 29),
            "state `n` is declared `enum<\"a\", \"b\">`, and its initializer is `string`; \"c\" is not a declared member",
        ),
    ] {
        let src = format!("{shapes}\ncomponent App\n  state k = 1\n  state {decl}\n  view\n    text \"x\"\n");
        let e = refused(&src);
        assert_eq!(e.id, "type-state-initializer", "{src}\n{e}");
        assert_eq!(e.message, format!("{says}{remedy}"), "{src}");
        assert_eq!((e.span.line, e.span.col), at, "{src}\n{e}");
    }
    // A row's state, typed where its component is used, says the same.
    let e = refused(
        "component App\n  view\n    Row(k=1)\ncomponent Row\n  props\n    k: number\n  state n: string = k\n  view\n    text n\n",
    );
    assert_eq!(e.id, "type-state-initializer", "{e}");
    assert_eq!((e.span.line, e.span.col), (7, 21), "{e}");
    contract::compile("component App\n  state n: option<number> = none\n  view\n    text \"x\"\n")
        .unwrap();
}
