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
fn the_routes_name_in_a_later_component_names_the_first() {
    let app = "routes nav\n  home \"/\"\n\ncomponent Helper\n  view\n    text \"x\"\n\ncomponent App\n  action open(url: string)\n    nav = go(nav, url)\n  view\n    main navigationKey=`${top(nav).id}` navigate=open\n      text \"home\"\n";
    let e = refused(app);
    assert!(
        e.message.ends_with("`routes nav` belongs to the file's first component, `Helper`: if `Helper` is a helper, move it below the component the app starts from"),
        "{}: {}",
        e.id,
        e.message
    );
    // The root first: the same app compiles.
    let (helper, root) = app
        .split_once("component Helper")
        .map(|(head, rest)| {
            let (helper, root) = rest.split_once("component App").unwrap();
            (
                format!("component Helper{helper}"),
                format!("{head}component App{root}"),
            )
        })
        .unwrap();
    contract::compile(&format!("{root}\n{helper}")).unwrap_or_else(|e| panic!("{e}"));
    // Another unknown name in a later component gains nothing.
    let other = refused(
        "routes nav\n  home \"/\"\n\ncomponent Helper\n  view\n    text \"x\"\n\ncomponent App\n  view\n    text `${missing}`\n",
    );
    assert_eq!(other.id, "type-unknown-name", "{}", other.message);
    assert!(
        !other
            .message
            .contains("belongs to the file's first component"),
        "{}",
        other.message
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

/// `for` and `return` in an action name what to write instead, and the
/// forms the `for` refusal suggests compile.
#[test]
fn a_loop_or_return_in_an_action_names_the_form_to_write() {
    let action = |body: &str| {
        format!(
            "shape Item\n  id: number\ncomponent App\n  state items: Item[] = []\n  state total = 0\n  state picked = 0\n  action go(id: number)\n{body}  view\n    button \"go\" press=go(1)\n"
        )
    };
    for body in [
        "    for x in items\n      total = total + x.id\n",
        "    for x in items key=x.id\n      total = total + x.id\n",
        "    if total > 0\n      for x in items\n        total = total + x.id\n",
    ] {
        let e = refused(&action(body));
        assert_eq!(e.id, "syntax-expected-statement", "{e}");
        assert!(
            e.message
                .starts_with("`for` repeats views; an action has no loop"),
            "{e}"
        );
    }
    contract::compile(&action(
        "    items = items.filter(x => x.id != id)\n    total = sum(items.map(x => x.id * 2))\n    match first(items.filter(x => x.id == id))\n      case some(x)\n        picked = x.id\n      case none\n        picked = 0\n",
    ))
    .unwrap();
    for body in ["    total = 1\n    return total\n", "    return\n"] {
        let e = refused(&action(body));
        assert_eq!(e.id, "syntax-expected-statement", "{e}");
        assert!(e.message.starts_with("an action returns no value"), "{e}");
    }
    // `return` is an ordinary name elsewhere: a state so named is assigned.
    contract::compile(
        "component App\n  state return = 0\n  action go\n    return = return + 1\n  view\n    button \"go\" press=go\n",
    )
    .unwrap();
}

#[test]
fn size_keywords_compile_and_their_neighbours_are_refused_by_name() {
    let source = |name: &str, value: &str| {
        format!("component App\n  view\n    column\n      text \"a\" {name}=\"{value}\"\n")
    };
    for name in [
        "width",
        "height",
        "min-width",
        "max-width",
        "min-height",
        "max-height",
        "flex-basis",
    ] {
        for keyword in ["min-content", "max-content", "fit-content", "Fit-Content"] {
            contract::compile(&source(name, keyword)).unwrap();
        }
        let refused = |value: &str, says: &str| {
            let e = contract::compile(&source(name, value)).unwrap_err();
            assert_eq!(e.id, "lower-attr-value", "{e}");
            assert!(e.message.contains(says), "{name}={value}: {e}");
        };
        refused("fit-content(10rem)", "only in a grid track");
        refused("stretch", "`stretch` (CSS Box Sizing 4) is not implemented");
        refused(
            "-webkit-fill-available",
            "`stretch` (CSS Box Sizing 4) is not implemented",
        );
        refused("-webkit-max-content", "write `max-content`");
        refused("calc-size(auto, size)", "`calc-size()`");
    }
    // `fit-content()` names its three-row equivalent where there is one.
    let e = contract::compile(&source("width", "fit-content(10rem)")).unwrap_err();
    assert!(
        e.message
            .contains("`width=\"max-content\" min-width=\"min-content\" max-width=\"<length>\"`"),
        "{e}"
    );
    // `content` is `flex-basis`'s alone.
    contract::compile(&source("flex-basis", "content")).unwrap();
    let e = contract::compile(&source("width", "content")).unwrap_err();
    assert!(
        e.message.contains("`content` is a `flex-basis` value"),
        "{e}"
    );
}

#[test]
fn a_misspelled_enum_member_is_named_with_the_member_it_meant() {
    let app = |state: &str, body: &str| {
        format!(
            "component App\n  state drag: enum<\"idle\", \"move\"> = {state}\n  action set(mode: enum<\"idle\", \"move\">)\n    drag = mode\n  action go\n    {body}\n  view\n    column\n      button \"Go\" press=go\n      Pane(mode=drag)\ncomponent Pane\n  props\n    mode: enum<\"idle\", \"move\">\n  view\n    text mode\n"
        )
    };
    for (src, id) in [
        (app("\"moev\"", "drag = \"idle\""), "type-state-initializer"),
        (app("\"idle\"", "drag = \"moev\""), "type-assign"),
        (app("\"idle\"", "set(\"moev\")"), "type-argument"),
        (
            app("\"idle\"", "drag = \"idle\"").replace("Pane(mode=drag)", "Pane(mode=\"moev\")"),
            "type-prop",
        ),
    ] {
        let e = refused(&src);
        assert_eq!(e.id, id, "{src}\n{e}");
        assert!(
            e.message
                .contains(r#""moev" is not a declared member; did you mean "move"?"#),
            "{src}\n{e}"
        );
    }
    // Two members one edit away: the refusal does not guess
    let e = refused(
        "component App\n  state drag: enum<\"node\", \"move\"> = \"node\"\n  action go\n    drag = \"mode\"\n  view\n    button \"Go\" press=go\n",
    );
    assert!(
        e.message.ends_with(r#""mode" is not a declared member"#),
        "{e}"
    );
}
