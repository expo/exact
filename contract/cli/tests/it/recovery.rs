fn compile_all(source: &str) -> Vec<(String, u32, String)> {
    contract::compile_path_source_all(std::path::Path::new("app.contract"), source, false)
        .err()
        .expect("syntax errors must refuse the app")
        .into_iter()
        .map(|e| (e.id, e.span.line, e.message))
        .collect()
}

#[test]
fn recovery_keeps_errors_that_only_mention_a_refused_name() {
    for (name, declarations, view, id) in [
        (
            "number",
            "state other = 1 - true",
            "text other.toString()",
            "type-operand",
        ),
        (
            "number",
            "resource r = load() as shape Missing",
            "text \"ok\"",
            "type-unknown",
        ),
        ("App", "", "Missing()", "type-unknown-component"),
        (
            "path",
            "",
            "text floor(\"bad\").toString()",
            "type-argument",
        ),
        (
            "style",
            "",
            "text \"ok\" class=Missing",
            "lower-unknown-class",
        ),
        (
            "animation",
            "",
            "text \"ok\" animation=\"missing 1s\"",
            "lower-animation-name",
        ),
    ] {
        let source =
            format!("component App\n  state {name} =\n  {declarations}\n  view\n    {view}\n");
        let found = compile_all(&source);
        let errors: Vec<_> = found.iter().map(|(id, _, _)| id.as_str()).collect();
        assert_eq!(
            errors,
            ["syntax-expected-expression", id],
            "{source}\n{found:#?}"
        );
    }
}

#[test]
fn recovery_suppresses_missing_declaration_consequences() {
    let mut failures = Vec::new();
    for (declaration, usage) in [
        ("  state gone =\n", "  view\n    text gone\n"),
        (
            "  state gone =\n",
            "  action go\n    gone = 1\n  view\n    button \"go\" press=go\n",
        ),
        (
            "  mutation gone as shape\n",
            "  action go\n    send gone = save()\n  view\n    button \"go\" press=go\n",
        ),
        (
            "  resource gone = load() as shape\n",
            "  action go\n    refresh gone\n  view\n    button \"go\" press=go\n",
        ),
        (
            "  resource gone = load() as shape\n",
            "  mutation saved as shape string refreshes gone\n  view\n    text \"ok\"\n",
        ),
        (
            "  resource gone = load() as shape\n",
            "  view\n    text pending(gone).toString()\n",
        ),
        (
            "  resource gone = load() as shape\n",
            "  view\n    text failed(gone).toString()\n",
        ),
        (
            "  resource gone = load() as shape\n",
            "  derive why = failure(gone)\n  view\n    text \"ok\"\n",
        ),
        (
            "  resource gone = load() as shape\n",
            "  view\n    text current(gone).toString()\n",
        ),
        (
            "  action gone\n    n =\n",
            "  view\n    button \"go\" press=gone\n",
        ),
        (
            "  action gone\n    n =\n",
            "  action go\n    gone()\n  view\n    button \"go\" press=go\n",
        ),
        (
            "  state n = 0\n  action gone\n    n =\n",
            "  view\n    Child(go=gone)\ncomponent Child\n  props\n    go: action\n  action tap\n    go()\n  view\n    button \"go\" press=tap\n",
        ),
    ] {
        let source = format!("component App\n{declaration}{usage}");
        let found = compile_all(&source);
        if found.len() != 1 || !found[0].0.starts_with("syntax-") {
            failures.push(format!("{source}\n{found:#?}"));
        }
    }
    for (declaration, usage) in [
        ("fn gone(): number =\n", "text gone().toString()"),
        ("fn empty(): number[] =\n", "text empty().length.toString()"),
        ("fn now(): number =\n", "text now().toString()"),
        ("fn pop(n: number): number =\n", "text pop(1).toString()"),
        ("fn path(id: string): string =\n", "text path(\"home\")"),
        ("style Card\n  color=\n", "text \"ok\" class=Card"),
        (
            "keyframes fade\n  from opacity=\n",
            "text \"ok\" animation=\"fade 1s\"",
        ),
        (
            "fn len(xs: number[]): number =\n",
            "text len([1]).toString()",
        ),
        (
            "shape Gone\n  value number\n",
            "text \"ok\"\n  resource value = load() as shape Gone",
        ),
        (
            "",
            "Child()\ncomponent Child\n  props\n    value number\n  view\n    text \"ok\"",
        ),
    ] {
        let source = format!("{declaration}component App\n  view\n    {usage}\n");
        let found = compile_all(&source);
        if found.len() != 1 || !found[0].0.starts_with("syntax-") {
            failures.push(format!("{source}\n{found:#?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn recovery_keeps_unrelated_missing_styles_keyframes_and_action_arguments() {
    for (source, name, value, ids) in [
        (
            "style Card\n  color=\ncomponent App\n  view\n    text \"ok\" class=Missing\n",
            "Card",
            "\"red\"",
            vec!["lower-unknown-class"],
        ),
        (
            "keyframes fade\n  from opacity=\ncomponent App\n  view\n    text \"ok\" animation=\"Missing 1s\"\n",
            "fade",
            "0",
            vec!["lower-animation-name"],
        ),
        (
            "component App\n  state n = 0\n  action gone\n    n =\n  view\n    Child(go=Missing)\ncomponent Child\n  props\n    go: action\n  action tap\n    go()\n  view\n    button \"go\" press=tap\n",
            "gone",
            "1",
            vec!["syntax-call-target", "type-unknown-name"],
        ),
    ] {
        let found = compile_all(source);
        assert_eq!(
            found.iter().map(|(id, _, _)| id.as_str()).collect::<Vec<_>>(),
            std::iter::once("syntax-expected-expression")
                .chain(ids)
                .collect::<Vec<_>>(),
            "{source}\n{found:#?}"
        );
        let repaired = source
            .replace("Missing", name)
            .replacen("=\n", &format!("= {value}\n"), 1);
        assert!(contract::compile(&repaired).is_ok(), "{repaired}");
    }
}

#[test]
fn recovery_distinguishes_path_function_calls_from_unrelated_route_errors() {
    let routes = "routes nav\n  home \"/\"\n    post \"/post/:id\"\n";
    let mut failures = Vec::new();
    for (declaration, call, id) in [
        ("fn path(id: number): string =", "path(1)", "route-unknown"),
        (
            "fn path(name: string): string =",
            "path(\"missing\")",
            "route-unknown",
        ),
        (
            "fn path(name: string): string =",
            "path(\"post\")",
            "route-unknown",
        ),
        (
            "fn path(name: string, id: bool): string =",
            "path(\"post\", true)",
            "type-argument",
        ),
        (
            "fn path(name: string, id: string): string =",
            "path(\"post\", \"..\")",
            "route-unknown",
        ),
    ] {
        let source = format!("{routes}{declaration}\ncomponent App\n  view\n    text {call}\n");
        let found = compile_all(&source);
        if found.len() != 1 || found[0].0 != "syntax-expected-expression" {
            failures.push(format!("{source}\n{found:#?}"));
        }
        let repaired = source.replace(declaration, &format!("{declaration} \"ok\""));
        assert!(contract::compile(&repaired).is_ok(), "{repaired}");
        let unrelated = source.replace("fn path(", "fn unrelated(");
        let found = compile_all(&unrelated);
        assert_eq!(
            found
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["syntax-expected-expression", id],
            "{unrelated}\n{found:#?}"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn recovery_suppresses_only_the_refused_imported_name() {
    let app = super::diagnostics::App::new("recovery-imports");
    let library = app
        .write("gone.contract", "fn gone(): number =\n")
        .canonicalize()
        .unwrap();
    for (name, expected) in [
        ("gone", vec!["syntax-expected-expression"]),
        (
            "missing",
            vec!["syntax-expected-expression", "contract-use-unknown"],
        ),
    ] {
        let root = app.write(
            "app.contract",
            &format!(
                "use {name} from \"./gone.contract\"\ncomponent App\n  view\n    text \"ok\"\n"
            ),
        );
        let errors = contract::compile_path_all(&root, false)
            .err()
            .expect("the imported syntax error must refuse the app");
        assert_eq!(
            errors.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            expected,
            "{name}: {errors:#?}"
        );
        assert_eq!(errors[0].file.as_deref(), Some(library.as_path()));
    }
}

#[test]
fn recovery_distinguishes_refused_length_idioms() {
    let source = "fn len(xs: number[]): number =\ncomponent App\n  view\n    column\n      text len([1]).toString()\n      text count([1]).toString()\n      text size([1]).toString()\n";
    let found = compile_all(source);
    assert_eq!(
        found
            .iter()
            .map(|(id, line, _)| (id.as_str(), *line))
            .collect::<Vec<_>>(),
        [
            ("syntax-expected-expression", 1),
            ("type-refused-idiom", 6),
            ("type-refused-idiom", 7)
        ],
        "{found:#?}"
    );
}

/// A refused declaration hides only what its kind could have answered: a
/// state is never called, so a call of its name reaches the intrinsic or
/// the roster as it would with the state complete.
#[test]
fn recovery_keeps_call_errors_a_refused_state_did_not_cause() {
    let routes = "routes nav\n  home \"/\"\n    post \"/post/:id\"\n";
    for (source, id) in [
        (
            format!("{routes}component App\n  state path =\n  view\n    text path(\"post\")\n"),
            "route-unknown",
        ),
        (
            format!(
                "{routes}component App\n  state path =\n  view\n    text path(\"post\", true)\n"
            ),
            "type-argument",
        ),
        (
            "component App\n  state count =\n  view\n    text count([1]).toString()\n".into(),
            "type-refused-idiom",
        ),
    ] {
        let found = compile_all(&source);
        assert_eq!(
            found
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["syntax-expected-expression", id],
            "{source}\n{found:#?}"
        );
        let repaired = source
            .replace("path =\n", "path = 0\n")
            .replace("count =\n", "count = 0\n");
        let errors = contract::compile_path_source_all(
            std::path::Path::new("app.contract"),
            &repaired,
            false,
        )
        .err()
        .expect("the completed state leaves the call's error");
        assert_eq!(
            errors.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            [id],
            "{repaired}\n{errors:#?}"
        );
    }
}

/// A refused `fn` or action that shadows a roster function leaves its calls
/// to the roster's: what the roster says about them is the refusal's
/// consequence, not the author's mistake.
#[test]
fn recovery_suppresses_roster_fallbacks_of_a_refused_callable() {
    for (source, repair) in [
        (
            "fn floor(s: string): string =\ncomponent App\n  view\n    text floor(\"hello\")\n",
            "= s\n",
        ),
        (
            "fn floor(s: string, t: string): string =\ncomponent App\n  view\n    text floor(\"a\", \"b\")\n",
            "= s\n",
        ),
        (
            "component App\n  state n = \"\"\n  action floor(s: string)\n    n =\n  view\n    button \"go\" press=floor(\"hello\")\n",
            "= s\n",
        ),
    ] {
        let found = compile_all(source);
        assert_eq!(
            found.iter().map(|(id, _, _)| id.as_str()).collect::<Vec<_>>(),
            ["syntax-expected-expression"],
            "{source}\n{found:#?}"
        );
        let repaired = source.replacen("=\n", repair, 1);
        assert!(contract::compile(&repaired).is_ok(), "{repaired}");
    }
    // Another call of the same roster function, outside the refused
    // action's component, is still checked.
    let source = "component App\n  state n = \"\"\n  action floor(s: string)\n    n =\n  view\n    column\n      button \"go\" press=floor(\"hello\")\n      Child()\ncomponent Child\n  view\n    text floor(\"hello\").toString()\n";
    let found = compile_all(source);
    assert_eq!(
        found
            .iter()
            .map(|(id, _, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["syntax-expected-expression", "type-argument"],
        "{source}\n{found:#?}"
    );
}

/// A refused member's name is its component's: another component's use of
/// the same name is that component's mistake.
#[test]
fn recovery_hides_a_refused_member_only_in_its_component() {
    for (member, usage, id) in [
        ("state gone =", "text gone", "type-unknown-name"),
        (
            "action count\n    n =",
            "text count([1]).toString()",
            "type-refused-idiom",
        ),
    ] {
        let source = format!(
            "component App\n  state n = 0\n  {member}\n  view\n    Child()\ncomponent Child\n  view\n    {usage}\n"
        );
        let found = compile_all(&source);
        assert_eq!(
            found
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["syntax-expected-expression", id],
            "{source}\n{found:#?}"
        );
        let own = format!("component App\n  state n = 0\n  {member}\n  view\n    {usage}\n");
        let found = compile_all(&own);
        assert_eq!(
            found
                .iter()
                .map(|(id, _, _)| id.as_str())
                .collect::<Vec<_>>(),
            ["syntax-expected-expression"],
            "{own}\n{found:#?}"
        );
    }
}
