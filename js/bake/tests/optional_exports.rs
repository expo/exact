//! A module's `overlay` and `tools` reach the host however app.ts exports
//! them, as the web JS target, which imports the module itself, runs them;
//! one written only in a comment is not exported, and the module builds.
use exact_js::Module;
use exact_js_bake::{bake, Tools};
use exact_plan::Value;
use exact_runner::{Answer, DataSource, Store};

const CONTRACT: &str =
    "component App\n  resource message = message() as shape string\n  view\n    text message\n";

const HEAD: &str = "import type { Answer, Overlay } from './app.contract.d.ts';\nexport const grants = '';\nexport const answer: Answer = () => 'answer';\n";

/// Bake `app.ts` (with `files` beside it), load it, and return what its
/// overlay shows over `"answer"` and what its named tool returns.
fn run(
    n: usize,
    tail: &str,
    files: &[(&str, &str)],
    name: &str,
) -> Result<(Option<Value>, Option<String>), String> {
    let dir = std::env::temp_dir().join(format!(
        "exact-optional-exports-{}-{}",
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir(&dir).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    write(
        "app.json",
        r#"{"app": {"id": "test.exact.optional", "name": "Optional"}}"#,
    );
    write("app.contract", CONTRACT);
    write("app.ts", &format!("{HEAD}{tail}\n"));
    for (name, text) in files {
        write(name, text);
    }
    let baked = bake(&dir, &Tools::default());
    std::fs::remove_dir_all(&dir).unwrap();
    let baked = baked?;
    let mut module = Module::loaded(baked.bytecode, "test.exact.optional", "").unwrap();
    module.set_budget_ms(f64::INFINITY);
    module.bind(&contract::compile(CONTRACT).unwrap());
    let shown = module
        .overlay("message", &[], &Value::from("answer"), &[])
        .unwrap()
        .map(|o| o.value);
    let mut store = Store::new("", vec![]);
    let tool = match module.answer(&mut store, &format!("tool:{name}"), &[Value::from("{}")]) {
        Ok(Answer::Now(value)) => Some(value.as_str().unwrap().to_owned()),
        Ok(Answer::Later(_)) => panic!("the tool answers now"),
        Err(_) => None,
    };
    Ok((shown, tool))
}

const OVERLAY_TS: &str = "import type { Overlay } from './app.contract.d.ts';\nexport const overlay: Overlay = () => 'overlaid';\n";
const TOOLS_TS: &str = "export const tools = { ping: () => 'pong' };\n";

/// An `app.ts` tail, the files beside it, and whether it exports both members.
type Case = (&'static str, &'static [(&'static str, &'static str)], bool);

#[test]
fn every_export_form_reaches_the_host_and_a_comment_none() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let cases: &[Case] = &[
        (
            "export const overlay: Overlay = () => 'overlaid';\nexport const tools = { ping: () => 'pong' };",
            &[],
            true,
        ),
        (
            "export * from './overlay';\nexport * from './tools';",
            &[("overlay.ts", OVERLAY_TS), ("tools.ts", TOOLS_TS)],
            true,
        ),
        (
            "export var overlay: Overlay = () => 'overlaid';\nexport var tools = { ping: () => 'pong' };",
            &[],
            true,
        ),
        (
            "const made: { overlay: Overlay; tools: { ping(): string } } = { overlay: () => 'overlaid', tools: { ping: () => 'pong' } };\nexport const { overlay, tools } = made;",
            &[],
            true,
        ),
        (
            "// export const overlay = (...) => 'x';\n// export const tools = {};",
            &[],
            false,
        ),
        ("/* export function overlay() {}\nexport { tools } */", &[], false),
    ];
    let mut failures = Vec::new();
    for (n, (tail, files, exported)) in cases.iter().enumerate() {
        let expected = match exported {
            true => (Some(Value::from("overlaid")), Some("\"pong\"".to_owned())),
            false => (None, None),
        };
        match run(n, tail, files, "ping") {
            Ok(got) if got == expected => {}
            got => failures.push(format!("{tail}\n  expected {expected:?}\n  got {got:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn an_overlay_is_checked_against_its_type() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let refused = run(
        100,
        "export * from './overlay';",
        &[(
            "overlay.ts",
            "export const overlay = (source: number) => source;\n",
        )],
        "ping",
    )
    .expect_err("an overlay of the wrong type is refused");
    assert!(refused.contains("not assignable"), "{refused}");
}

#[test]
fn tool_handlers_must_be_own_exports() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    for (i, name) in ["constructor", "toString"].iter().enumerate() {
        let (_, tool) = run(200 + i, "export const tools = {};", &[], name).unwrap();
        assert_eq!(tool, None, "an inherited {name} is no tool handler");
        let (_, tool) = run(
            210 + i,
            &format!("export const tools = {{{name}: () => 'own'}};"),
            &[],
            name,
        )
        .unwrap();
        assert_eq!(tool.as_deref(), Some("\"own\""));
    }
}
