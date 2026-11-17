//! A selected generation carrying a TypeScript module pair boots that module
//! through `exact_js` (LLP 1030.006 D4).
use super::*;
use std::sync::OnceLock;

/// What a TypeScript binary records: it runs Hermes and replaces no Rust.
const TYPESCRIPT_COMPAT: &str = r#"{"id":"fixture00000000","inputs":{"app":"com.exact.fixture","keys":null,"trust":"development","store":{"L":"A"},"executors":["hermes","native"],"rustMode":"off"},"delivery":{"channel":"prod","origin":"https://updates.example"}}"#;

const CONTRACT: &str = "component App\n  state n: number = 0\n  resource message = message(n) as shape string\n  action bump\n    n = n + 1\n  view\n    column\n      text message testId=\"message\"\n      button press=bump testId=\"bump\"\n        text \"bump\"\n";

/// The app baked with `app.ts` answering `<word> <n>`.
fn bake(word: &str) -> exact_js_bake::Baked {
    let dir = std::env::temp_dir().join(format!("exact-linux-ts-{word}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, text: &str| std::fs::write(dir.join(name), text).unwrap();
    write(
        "app.json",
        r#"{"app": {"id": "com.exact.fixture", "name": "Fixture"}}"#,
    );
    write("app.contract", CONTRACT);
    write("app.ts", &format!("import type {{ Sources, Answer }} from './app.contract.d.ts';\nexport const grants = '';\nconst sources: Sources = {{ message: ([n]) => `{word} ${{n}}` }};\nexport const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);\n"));
    let baked = exact_js_bake::bake(&dir, &exact_js_bake::Tools::default()).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    baked
}

static BAKED: OnceLock<exact_js_bake::Baked> = OnceLock::new();

exact_logic::configured!(Baked, exact_js::Module, || {
    let bytecode = BAKED.get().expect("baked first").bytecode.clone();
    exact_logic::Swappable::off(exact_js::Module::new(bytecode, "com.exact.fixture", ""))
});

#[test]
fn a_selected_typescript_pair_runs_in_place_of_the_baked_module() {
    if !exact_js::ENGINE_LINKED {
        return;
    }
    let baked = BAKED.get_or_init(|| bake("baked"));
    let selected = bake("selected");
    let dir = std::env::temp_dir().join(format!("exact-linux-ts-pair-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pair = [
        ("typescript/app.module.json", selected.receipt.into_bytes()),
        ("typescript/app.hbc", selected.bytecode),
    ];
    let client = checked_as(
        TYPESCRIPT_COMPAT,
        &dir,
        &baked.plan,
        &selected.plan,
        &pair,
        false,
    );
    let mut config = selected_config_as(TYPESCRIPT_COMPAT, &dir, &baked.plan, client);
    assert!(config.entry.is_some(), "the generation is selected");
    let (mut presenter, error) = boot_presenter::<Baked>(&mut config, (390.0, 844.0)).unwrap();
    assert!(error.is_none(), "{error:?}");
    let _ = presenter.frame();
    presenter.first_pixel();
    let kernel = presenter.host().kernel();
    let bump = kernel
        .node_by_key(kernel.find_by_test_id("bump")[0])
        .unwrap()
        .id;
    presenter.tap(bump).unwrap();
    let tree = exact_painted::agent::handle(&mut presenter, r#"{"op":"clock","settle":true}"#);
    assert!(!tree.contains("error"), "{tree}");
    let tree = presenter.host().agent("{\"op\":\"tree\"}");
    assert!(tree.contains("selected 1"), "{tree}");
    assert!(!tree.contains("baked"), "{tree}");
    drop(presenter);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The same pair in a binary that runs no Hermes is refused, and entry zero
/// boots.
#[test]
fn a_rust_binary_refuses_a_typescript_pair_and_boots_entry_zero() {
    let baked = contract::compile("component App\n  view\n    text \"baked\"\n")
        .unwrap()
        .encode();
    let candidate = contract::compile("component App\n  view\n    text \"candidate\"\n")
        .unwrap()
        .encode();
    let dir = std::env::temp_dir().join(format!("exact-linux-ts-refused-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let client = stage(
        &dir,
        &baked,
        &candidate,
        &[
            ("typescript/app.module.json", b"{}".to_vec()),
            ("typescript/app.hbc", b"bytecode".to_vec()),
        ],
    );
    let mut config = selected_config(&dir, &baked, client);
    let (presenter, _) = boot_presenter::<Named>(&mut config, (390.0, 844.0)).unwrap();
    let tree = presenter.host().agent("{\"op\":\"tree\"}");
    assert!(
        tree.contains("baked") && !tree.contains("candidate"),
        "{tree}"
    );
    assert!(config.entry.is_none());
    drop(presenter);
    std::fs::remove_dir_all(dir).unwrap();
}

/// A generation needing a capability the APK did not link is refused before
/// its boot is counted, and entry zero boots (LLP 1030.006 D4a).
#[test]
fn a_generation_beyond_the_linked_composition_boots_entry_zero() {
    let compat = r#"{"id":"fixture00000000","inputs":{"app":"com.exact.fixture","keys":null,"trust":"development","store":{"L":"A"},"rustMode":"off","androidLinked":{"uses":[],"extensions":[],"permissions":[]}},"delivery":{"channel":"prod","origin":"https://updates.example"}}"#;
    let baked = contract::compile("component App\n  view\n    text \"baked\"\n")
        .unwrap()
        .encode();
    let candidate = contract::compile(
        "component App\n  state day = \"2026-09-14\"\n  action setDay(value: string)\n    day = value\n  view\n    column gap=8\n      input type=\"date\" value=day change=setDay\n      text \"candidate\"\n",
    )
    .unwrap()
    .encode();
    let dir = std::env::temp_dir().join(format!("exact-linux-unlinked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let client = checked_as(compat, &dir, &baked, &candidate, &[], false);
    let record = client.dir().join("record.json");
    let mut config = selected_config_as(compat, &dir, &baked, client);
    assert!(config.entry.is_some());
    let (presenter, _) = boot_presenter::<Named>(&mut config, (390.0, 844.0)).unwrap();
    let tree = presenter.host().agent("{\"op\":\"tree\"}");
    assert!(
        tree.contains("baked") && !tree.contains("candidate"),
        "{tree}"
    );
    let saved = std::fs::read_to_string(record).unwrap();
    assert!(saved.contains("\"failures\":0"), "not counted: {saved}");
    drop(presenter);
    std::fs::remove_dir_all(dir).unwrap();
}
