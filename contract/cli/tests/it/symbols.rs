//! Navigation resolves actual compiler namespaces and original identifier ranges.

use serde_json::Value;
use std::{path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("exact-symbols-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        path
    }
    fn query(&self, source: &str) -> Value {
        serde_json::from_str(
            &contract::symbols_json(&self.write("app.contract", source), None).unwrap(),
        )
        .unwrap()
    }
    /// [`Fixture::query`] as `symbols --editor` answers it.
    fn editor_query(&self, source: &str) -> Value {
        serde_json::from_str(
            &contract::editor_symbols_json(&self.write("app.contract", source), None).unwrap(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn defs(graph: &Value) -> &[Value] {
    graph["definitions"].as_array().unwrap()
}
fn refs(graph: &Value) -> &[Value] {
    graph["references"].as_array().unwrap()
}
fn definition<'a>(graph: &'a Value, kind: &str, name: &str, owner: Option<&str>) -> &'a Value {
    defs(graph)
        .iter()
        .find(|d| d["kind"] == kind && d["name"] == name && d["owner"].as_str() == owner)
        .unwrap_or_else(|| panic!("missing {kind} {name}: {graph}"))
}
fn target<'a>(graph: &'a Value, reference: &Value) -> &'a Value {
    &defs(graph)[reference["to"].as_u64().unwrap() as usize]
}
fn spelling(symbol: &Value) -> String {
    let source = std::fs::read_to_string(symbol["file"].as_str().unwrap()).unwrap();
    let line = source
        .lines()
        .nth(symbol["line"].as_u64().unwrap() as usize - 1)
        .unwrap();
    line[symbol["col"].as_u64().unwrap() as usize - 1
        ..symbol["end_col"].as_u64().unwrap() as usize - 1]
        .into()
}
fn at_line<'a>(graph: &'a Value, name: &str, line: usize) -> Vec<&'a Value> {
    refs(graph)
        .iter()
        .filter(|r| r["name"] == name && r["line"] == line as u64)
        .collect()
}
fn line(source: &str, text: &str) -> usize {
    source.lines().position(|l| l.contains(text)).unwrap() + 1
}

#[test]
fn resource_context_references_resolve_at_their_original_source_ranges() {
    let source = r#"component App
  state car = "train"
  state minute = 0
  derive sample = minute + 1
  resource status = status(car) with max(sample, minute) as shape string
  action tick
    minute = minute + 1
  action choose(value: string)
    car = value
  view
    text status
"#;
    let f = Fixture::new("resource-context");
    let graph = f.query(source);
    for (name, kind) in [("car", "state"), ("sample", "derive"), ("minute", "state")] {
        let refs = at_line(&graph, name, line(source, "resource status"));
        assert_eq!(refs.len(), 1, "{name}: {graph}");
        assert_eq!(target(&graph, refs[0])["kind"], kind);
        assert_eq!(spelling(refs[0]), name);
    }
}

const SOURCE: &str = r##"shape Item
  id: string
  label: string
shape Other
  label: string
fn label(value: Item): string = value.label
fn otherLabel(value: Other): string = value.label
component App
  state textValue = "root"
  state choice = some("chosen")
  resource items = loadItems() as shape Item[]
  mutation saved as shape Item
  action edit(textValue: string)
    textValue = textValue
  action save(item: Item)
    send saved = saveItem(item)
    refresh items
    focus("entry")
  action tick
    textValue = "tick"
  task ticker mount
    every(1000, tick)
  provide
    accent = "#fff"
  view
    column navigationBack="entry"
      input id="entry" testId="entry-test" value=textValue change=edit
      for item in items key=item.id
        Row(item=item, onPick=save)
      match choice
        case some(textValue)
          text textValue
        case none
          text textValue
      text `é ${match choice { case some(textValue) => textValue, case none => textValue }}`
component Row
  props
    item: Item
    onPick: action
  inject
    accent: string
  view
    button press=onPick(item) background-color=accent
      text label(item)
"##;

#[test]
fn exact_ranges_cover_declarations_sources_locals_fields_and_ids() {
    let f = Fixture::new("ranges");
    let graph = f.query(SOURCE);
    for symbol in defs(&graph).iter().chain(refs(&graph)) {
        let actual = spelling(symbol);
        let name = symbol["name"].as_str().unwrap();
        assert!(
            actual == name || actual == format!("\"{name}\""),
            "{actual:?}: {symbol}"
        );
    }
    for (kind, name) in [
        ("component", "App"),
        ("state", "textValue"),
        ("resource", "items"),
        ("mutation", "saved"),
        ("source", "loadItems"),
        ("source", "saveItem"),
        ("task", "ticker"),
        ("provide", "accent"),
        ("inject", "accent"),
        ("id", "entry"),
        ("testId", "entry-test"),
    ] {
        definition(&graph, kind, name, None);
    }
    assert_eq!(
        refs(&graph)
            .iter()
            .filter(|r| r["kind"] == "id" && r["name"] == "entry")
            .count(),
        2
    );
    let action = at_line(&graph, "tick", line(SOURCE, "every(1000"));
    assert_eq!(action.len(), 1);
    assert_eq!(target(&graph, action[0])["kind"], "action");
    // An action's effects are inferred and shown on its definition
    // (LLP 1035.005.000 D1); nothing else carries `writes`.
    for (name, writes) in [
        ("edit", vec!["textValue"]),
        ("save", vec!["saved"]),
        ("tick", vec!["textValue"]),
    ] {
        let action = definition(&graph, "action", name, None);
        assert_eq!(action["writes"], serde_json::json!(writes));
    }
    assert!(refs(&graph).iter().all(|r| r.get("writes").is_none()));
    assert!(defs(&graph)
        .iter()
        .all(|d| (d["kind"] == "action") == d.get("writes").is_some()));
}

#[test]
fn parameters_and_branch_bindings_shadow_reads_but_never_assignment_targets() {
    let f = Fixture::new("shadow");
    let graph = f.query(SOURCE);
    let both = at_line(&graph, "textValue", line(SOURCE, "textValue = textValue"));
    assert_eq!(both.len(), 2);
    assert_eq!(target(&graph, both[0])["kind"], "state");
    assert_eq!(target(&graph, both[1])["kind"], "parameter");
    let local_line = line(SOURCE, "          text textValue");
    let some = at_line(&graph, "textValue", local_line);
    let none = at_line(&graph, "textValue", local_line + 2);
    assert_eq!(target(&graph, some[0])["kind"], "local");
    assert_eq!(target(&graph, none[0])["kind"], "state");
    let template = at_line(&graph, "textValue", line(SOURCE, "text `é"));
    assert_eq!(template.len(), 2);
    assert_eq!(target(&graph, template[0])["kind"], "local");
    assert_eq!(target(&graph, template[1])["kind"], "state");
}

#[test]
fn field_references_use_the_inferred_shape_and_component_arguments_name_props() {
    let f = Fixture::new("fields");
    let graph = f.query(SOURCE);
    for (function, shape) in [("fn label", "Item"), ("fn otherLabel", "Other")] {
        let reference = at_line(&graph, "label", line(SOURCE, function));
        let field = reference.iter().find(|r| r["kind"] == "field").unwrap();
        assert_eq!(target(&graph, field)["owner"], shape);
    }
    let item_refs = at_line(&graph, "item", line(SOURCE, "Row(item=item"));
    assert_eq!(item_refs.len(), 2);
    assert_eq!(target(&graph, item_refs[0])["kind"], "prop");
    assert_eq!(target(&graph, item_refs[0])["component"], "Row");
    assert_eq!(target(&graph, item_refs[1])["kind"], "local");
    let field = at_line(&graph, "id", line(SOURCE, "for item"));
    assert_eq!(target(&graph, field[0])["owner"], "Item");
}

#[test]
fn imported_definitions_are_canonical_and_imports_point_at_names() {
    let f = Fixture::new("imports");
    let row = f
        .write(
            "lib/row.contract",
            "component Row\n  props\n    label: string\n  view\n    text label\n",
        )
        .canonicalize()
        .unwrap();
    let graph = f.query(
        "use Row from \"./lib/row.contract\"\ncomponent App\n  view\n    Row(label=\"hello\")\n",
    );
    let d = definition(&graph, "component", "Row", None);
    assert_eq!(d["file"], row.to_string_lossy().as_ref());
    assert_eq!(
        (d["col"].as_u64(), d["end_col"].as_u64()),
        (Some(11), Some(14))
    );
    let references: Vec<_> = refs(&graph)
        .iter()
        .filter(|r| r["kind"] == "component" && r["name"] == "Row")
        .collect();
    assert_eq!(references.len(), 2);
    assert_eq!(references[0]["col"], 5);
    for r in references {
        assert_eq!(target(&graph, r), d);
    }
}

#[test]
fn shape_and_function_only_modules_are_queryable_without_a_fake_app() {
    let f = Fixture::new("module");
    let graph=f.query("shape Item\n  name: string\nfn title(item: Item): string = item.name\nstyle Card\n  padding=8\n");
    definition(&graph, "shape", "Item", None);
    definition(&graph, "style", "Card", None);
    assert!(refs(&graph)
        .iter()
        .any(|r| r["kind"] == "field" && r["name"] == "name"));
    assert!(defs(&graph).iter().all(|d| d["kind"] != "component"));
}

#[test]
fn duplicate_ids_report_all_authored_targets() {
    let f = Fixture::new("ids");
    let graph=f.query("component App\n  action focusEntry\n    focus(\"entry\")\n  action dismiss\n    blur(\"entry\")\n  view\n    column\n      input id=\"entry\"\n      input id=\"entry\"\n");
    let refs: Vec<_> = refs(&graph).iter().filter(|r| r["kind"] == "id").collect();
    assert_eq!(refs.len(), 4, "focus and blur each name both targets");
    assert_ne!(refs[0]["to"], refs[1]["to"]);
    assert_ne!(refs[2]["to"], refs[3]["to"]);
}

#[test]
fn font_families_resolve_literal_element_and_style_uses_with_exact_ranges() {
    let f = Fixture::new("fonts");
    let graph = f.query(
        r#"font "Élan \"Display\"" = "assets/display.otf"
font "Body Face"
  400 = "assets/body.otf"
style Heading
  font-family="Élan \"Display\""
component App
  view
    column
      text "Élan \"Display\"" class=Heading
      text "heading" font-family="Élan \"Display\""
      text "body" font-family="Body Face"
      text "generic" font-family="system-ui"
"#,
    );
    let display = definition(&graph, "font", "Élan \"Display\"", None);
    let body = definition(&graph, "font", "Body Face", None);
    assert_eq!(spelling(display), r#""Élan \"Display\"""#);
    assert_eq!(spelling(body), "\"Body Face\"");
    let font_refs: Vec<_> = refs(&graph)
        .iter()
        .filter(|r| r["kind"] == "font")
        .collect();
    assert_eq!(font_refs.len(), 3);
    assert_eq!(
        font_refs
            .iter()
            .map(|r| r["line"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [5, 10, 11]
    );
    for reference in font_refs {
        let declaration = target(&graph, reference);
        assert_eq!(spelling(reference), spelling(declaration));
        assert!(declaration == display || declaration == body);
    }
    // Navigation reads source only; missing font bytes are a build-time refusal.
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
}

#[test]
fn imported_fonts_keep_their_declaration_and_use_locations() {
    let f = Fixture::new("font-imports");
    let font = f
        .write(
            "font.contract",
            "font \"Brand\" = \"assets/brand.otf\"\nstyle Body\n  font-family=\"Brand\"\n",
        )
        .canonicalize()
        .unwrap();
    let style = f
        .write(
            "heading.contract",
            "use Body from \"./font.contract\"\nstyle Heading\n  font-family=\"Brand\"\n",
        )
        .canonicalize()
        .unwrap();
    let graph = f.query("use Heading from \"./heading.contract\"\nuse Body from \"./font.contract\"\ncomponent App\n  view\n    text \"Brand\" font-family=\"Brand\"\n");
    let declaration = definition(&graph, "font", "Brand", None);
    assert_eq!(declaration["file"], font.to_string_lossy().as_ref());
    assert_eq!(
        (declaration["col"].as_u64(), declaration["end_col"].as_u64()),
        (Some(6), Some(13))
    );
    let font_refs: Vec<_> = refs(&graph)
        .iter()
        .filter(|r| r["kind"] == "font")
        .collect();
    assert_eq!(font_refs.len(), 3); // two styles, one element, no font import form
    for reference in &font_refs {
        assert_eq!(target(&graph, reference), declaration);
        assert_eq!(spelling(reference), "\"Brand\"");
    }
    assert_eq!(
        font_refs
            .iter()
            .filter(|r| r["file"] == style.to_string_lossy().as_ref())
            .count(),
        1
    );
    let only_font = f.write(
        "only-font.contract",
        "font \"Brand\" = \"assets/brand.otf\"\n",
    );
    let module: Value =
        serde_json::from_str(&contract::symbols_json(&only_font, None).unwrap()).unwrap();
    assert_eq!(defs(&module).len(), 1);
    assert!(refs(&module).is_empty());
    assert_eq!(spelling(&defs(&module)[0]), "\"Brand\"");
}

#[test]
fn navigation_uses_build_import_refusals_including_cached_name_checks_and_cycles() {
    let f = Fixture::new("refusals");
    f.write("row.contract", "component Row\n  view\n    text \"row\"\n");
    for source in [
        "use Row from \"./row.contract\"\nuse Missing from \"./row.contract\"\ncomponent App\n  view\n    Row()\n",
        "use Row from \"../row.contract\"\ncomponent App\n  view\n    Row()\n",
        "use App from \"./app.contract\"\ncomponent App\n  view\n    text \"app\"\n",
    ] {
        let root=f.write("app.contract",source);
        let build=contract::compile_path(&root).unwrap_err();
        let query=contract::symbols_json(&root, None).unwrap_err();
        assert_eq!(build,query.errors[0]);
        assert_eq!(query.errors, contract::symbols_json(&root, Some("absent")).unwrap_err().errors);
    }
}

#[test]
fn exact_name_queries_preserve_ambiguous_definitions_and_reference_targets() {
    let f = Fixture::new("named");
    let root = f.write("app.contract", SOURCE);
    let full: Value = serde_json::from_str(&contract::symbols_json(&root, None).unwrap()).unwrap();
    for name in [
        "textValue",
        "label",
        "item",
        "App",
        "entry",
        "absent",
        "TextValue",
        "",
    ] {
        let named: Value =
            serde_json::from_str(&contract::symbols_json(&root, Some(name)).unwrap()).unwrap();
        let expected: Vec<_> = defs(&full).iter().filter(|d| d["name"] == name).collect();
        assert_eq!(defs(&named).iter().collect::<Vec<_>>(), expected);
        let expected: Vec<_> = refs(&full).iter().filter(|r| r["name"] == name).collect();
        assert_eq!(refs(&named).len(), expected.len());
        for (actual, original) in refs(&named).iter().zip(expected) {
            assert_eq!(target(&named, actual), target(&full, original));
            for key in ["kind", "name", "file", "line", "col", "end_col"] {
                assert_eq!(actual[key], original[key]);
            }
        }
    }
    // A matching declaration does not hide an unrelated type error.
    let bad = f.write(
        "bad.contract",
        "component App\n  derive broken = missing\n  view\n    text \"ok\"\n",
    );
    assert_eq!(
        contract::symbols_json(&bad, None).unwrap_err().errors,
        contract::symbols_json(&bad, Some("App"))
            .unwrap_err()
            .errors
    );
}

#[test]
fn cli_named_queries_accept_literal_names_and_keep_json_read_only() {
    let f = Fixture::new("named-cli");
    let source = "font \"Élan Display\" = \"assets/font.otf\"\ncomponent App\n  view\n    text \"font\" font-family=\"Élan Display\"\n    input id=\"--entry\"\n";
    let root = f.write("app.contract", source);
    for (name, definitions, references) in
        [("Élan Display", 1, 1), ("--entry", 1, 0), ("missing", 0, 0)]
    {
        let output = Command::new(env!("CARGO_BIN_EXE_contract"))
            .args(["symbols", root.to_str().unwrap(), "--name", name])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let graph: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(defs(&graph).len(), definitions);
        assert_eq!(refs(&graph).len(), references);
        for reference in refs(&graph) {
            assert_eq!(reference["to"], 0);
        }
    }
    assert_eq!(std::fs::read_to_string(&root).unwrap(), source);
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
    for args in [
        vec!["symbols", root.to_str().unwrap(), "--name"],
        vec!["symbols", root.to_str().unwrap(), "--wrong", "App"],
        vec!["symbols", root.to_str().unwrap(), "--name", "App", "extra"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_contract"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("--name"));
    }
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_contract"))
            .args(["symbols", flag])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains("--name <exact-name>"));
    }
}

#[test]
fn cli_is_read_only_json_and_refuses_extra_arguments() {
    let f = Fixture::new("cli");
    let root = f.write("app.contract", SOURCE);
    let out = Command::new(env!("CARGO_BIN_EXE_contract"))
        .arg("symbols")
        .arg(&root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let graph: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(defs(&graph).len() > 20);
    assert_eq!(std::fs::read_to_string(&root).unwrap(), SOURCE);
    assert_eq!(std::fs::read_dir(&f.0).unwrap().count(), 1);
    for args in [
        vec!["symbols"],
        vec!["symbols", "--unknown"],
        vec!["symbols", root.to_str().unwrap(), "extra"],
    ] {
        assert_eq!(
            Command::new(env!("CARGO_BIN_EXE_contract"))
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
}

#[test]
fn built_in_type_names_do_not_resolve_to_unrelated_authored_shapes() {
    let f = Fixture::new("primitive-types");
    for ty in ["string", "number", "bool", "unit"] {
        let field = if ty == "string" { "number" } else { "string" };
        let source =
            format!("shape {ty}\n  field: {field}\nfn identity(value: {ty}): {ty} = value\n");
        let graph = f.query(&source);
        definition(&graph, "shape", ty, None);
        assert!(refs(&graph).iter().all(|r| r["kind"] != "shape"), "{graph}");
    }
}

#[test]
fn route_paths_and_router_slot_offsets_follow_the_type_checker() {
    let f = Fixture::new("routes");
    let graph = f.query("routes nav\n  home \"/\"\n  item \"/item/:id\"\nshape Item\n  label: string\ncomponent App\n  state selected = \"one\"\n  resource item = load() as shape Item\n  derive url = path(\"item\", selected)\n  view\n    text `${nav.tab} ${item.label} ${url}`\n");
    let route = refs(&graph)
        .iter()
        .find(|r| r["kind"] == "route" && r["name"] == "item")
        .unwrap();
    assert_eq!(target(&graph, route)["line"], 3);
    assert!(refs(&graph)
        .iter()
        .any(|r| r["kind"] == "state" && r["name"] == "nav"));
    let field = refs(&graph)
        .iter()
        .find(|r| r["kind"] == "field" && r["name"] == "label")
        .unwrap();
    assert_eq!(target(&graph, field)["owner"], "Item");
    let graph = f.query("fn path(value: string): string = value\ncomponent App\n  view\n    text path(\"ordinary string\")\n");
    assert!(refs(&graph)
        .iter()
        .any(|r| r["kind"] == "fn" && r["name"] == "path"));
    assert!(refs(&graph).iter().all(|r| r["kind"] != "route"));
}

#[test]
fn each_definition_has_its_type_as_typescript_and_the_shapes_its_types_name() {
    let f = Fixture::new("types");
    let graph = f.editor_query(
        r#"routes nav
  home "/"
shape Tag
  name: string
shape Item
  id: string
  tags: string[]
  parent: option<Tag>
fn label(value: Item): string = value.id
component App
  state count = 0
  derive doubled = count * 2
  derive ids = items.map((i) => i.id)
  resource items = loadItems() as shape Item[]
  mutation saved as shape Item
  action save(item: Item, note: string)
    let next = count + 1
    count = next
    send saved = saveItem(item, note)
  view
    column
      for item, at in items key=item.id
        Row(item=item, pick=save)
component Row
  props
    item: Item
    pick: action
  view
    button press=pick(item, "") testId="row"
      text label(item)
"#,
    );
    let ty = |kind: &str, name: &str, owner: Option<&str>| {
        definition(&graph, kind, name, owner)["type"]
            .as_str()
            .map(str::to_owned)
    };
    let local = |name: &str, line: u64| {
        let d = defs(&graph)
            .iter()
            .find(|d| d["kind"] == "local" && d["name"] == name && d["line"] == line)
            .unwrap_or_else(|| panic!("missing local {name}: {graph}"));
        d["type"].as_str().map(str::to_owned)
    };
    let item = "{ id: string; tags: string[]; parent: Tag | null }";
    for (got, expected) in [
        (ty("shape", "Item", None), Some(item)),
        (ty("field", "tags", Some("Item")), Some("string[]")),
        (ty("field", "parent", Some("Item")), Some("Tag | null")),
        (ty("fn", "label", None), Some("(value: Item) => string")),
        (ty("parameter", "value", Some("label")), Some("Item")),
        (ty("state", "count", None), Some("number")),
        (ty("state", "nav", None), Some("Router")),
        (ty("derive", "doubled", None), Some("number")),
        (ty("derive", "ids", None), Some("string[]")),
        (ty("resource", "items", None), Some("Item[]")),
        (ty("mutation", "saved", None), Some("Item | null")),
        (
            ty("action", "save", None),
            Some("(item: Item, note: string) => void"),
        ),
        (ty("parameter", "note", Some("save")), Some("string")),
        // A bare `action` prop is called with two arguments here: its arity is not declared.
        (ty("prop", "pick", None), None),
        (local("next", 17), Some("number")),
        (local("i", 13), Some("Item")),
        (local("item", 22), Some("Item")),
        (local("at", 22), Some("number")),
        // A route, a component and a source have no type in the checker.
        (ty("route", "home", None), None),
        (ty("component", "App", None), None),
        (ty("source", "loadItems", None), None),
    ] {
        assert_eq!(got.as_deref(), expected, "{graph}");
    }
    let shape = |name: &str| {
        let shapes = graph["shapes"].as_array().unwrap();
        let found: Vec<_> = shapes.iter().filter(|s| s["name"] == name).collect();
        assert_eq!(found.len(), 1, "{name}: {graph}");
        // A program that loaded is one scope: no shape names a file.
        assert!(found[0].get("file").is_none(), "{graph}");
        found[0]["type"].as_str().unwrap().to_owned()
    };
    assert_eq!(shape("Item"), item);
    assert_eq!(shape("Tag"), "{ name: string }");
    // The router's shapes, which no file declares, are named by the router state's type.
    assert_eq!(
        shape("Router"),
        "{ tab: string; tabs: Tab[]; next: number }"
    );
    for name in ["Tab", "Entry", "Params"] {
        shape(name);
    }
}

/// When loading stops, each file is read on its own, and two of them may
/// declare different shapes of one name: each keeps its own.
#[test]
fn files_read_on_their_own_keep_their_own_shapes_of_one_name() {
    let f = Fixture::new("alone-shapes");
    let other = f.write(
        "other.contract",
        "shape Item\n  title: string\nfn name(value: Item): string = value.title\n",
    );
    let app = f.write(
        "app.contract",
        r#"use Gone from "./gone.contract"
use name from "./other.contract"
shape Item
  id: number
fn key(value: Item): number = value.id
component App
  view
    text "x"
"#,
    );
    let partial = contract::editor_symbols_json(&app, None).unwrap_err();
    let graph: Value = serde_json::from_str(&partial.json).unwrap();
    let of = |file: &std::path::Path| {
        let file = file.canonicalize().unwrap();
        let found: Vec<_> = (graph["shapes"].as_array().unwrap().iter())
            .filter(|s| {
                s["name"] == "Item"
                    && s["file"]
                        .as_str()
                        .is_some_and(|at| std::path::Path::new(at).canonicalize().unwrap() == file)
            })
            .map(|s| s["type"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(found.len(), 1, "{graph}");
        found[0].clone()
    };
    assert_eq!(of(&app), "{ id: number }");
    assert_eq!(of(&other), "{ title: string }");
}

#[test]
fn a_refused_check_keeps_the_declared_types_of_shapes_and_fields_only() {
    let f = Fixture::new("types-refused");
    let path = f.write(
        "app.contract",
        "shape Item\n  id: string\ncomponent App\n  state count = 0\n  view\n    text count.foo\n",
    );
    let graph: Value =
        serde_json::from_str(&contract::editor_symbols_json(&path, None).unwrap_err().json)
            .unwrap();
    assert_eq!(
        definition(&graph, "shape", "Item", None)["type"],
        "{ id: string }"
    );
    assert_eq!(
        definition(&graph, "field", "id", Some("Item"))["type"],
        "string"
    );
    assert!(definition(&graph, "state", "count", None)
        .get("type")
        .is_none());
}

#[test]
fn a_let_is_a_local_for_its_block_and_a_record_names_its_shape_and_fields() {
    // LLP 1035.005.000 D2 and D3.
    let fixture = Fixture::new("let-records");
    let graph = fixture.query(
        "shape F\n  title: string\n  done: bool\ncomponent App\n  state f = F(title=\"a\", done=false)\n  action go\n    let next = F(f, done=true)\n    f = next\n  view\n    text f.title\n",
    );
    let local = definition(&graph, "local", "next", Some("go"));
    assert_eq!(
        (local["line"].as_u64(), local["col"].as_u64()),
        (Some(7), Some(9))
    );
    let reads = at_line(&graph, "next", 8);
    assert_eq!(reads.len(), 1);
    assert_eq!(target(&graph, reads[0]), local);
    let shape = definition(&graph, "shape", "F", None);
    for line in [5, 7] {
        let uses = at_line(&graph, "F", line);
        assert_eq!(uses.len(), 1, "line {line}");
        assert_eq!(target(&graph, uses[0]), shape);
        assert_eq!(spelling(uses[0]), "F");
    }
    let done = definition(&graph, "field", "done", Some("F"));
    for line in [5, 7] {
        let uses = at_line(&graph, "done", line);
        assert_eq!(uses.len(), 1, "line {line}");
        assert_eq!(target(&graph, uses[0]), done);
        assert_eq!(spelling(uses[0]), "done");
    }
}

#[test]
fn a_call_refers_to_its_callee_and_the_caller_writes_what_it_writes() {
    // LLP 1089 D5: `symbols --name enter` answers what Enter touches.
    let source = r#"component App
  state sel = 0
  state seen = 0
  state log = ""
  action follow(v: number)
    seen = v
  action enter(next: number)
    log = "enter"
    follow(next)
  view
    column
      button press=enter(1) testId="enter"
        text "enter"
      Row(go=follow)

component Row
  props
    go: action
  state taps = 0
  action tap
    go(2)
    taps = taps + 1
  view
    button press=tap testId="tap"
      text "tap"
"#;
    let f = Fixture::new("calls");
    let graph = f.query(source);
    let enter = definition(&graph, "action", "enter", None);
    assert_eq!(enter["writes"], serde_json::json!(["seen", "log"]));
    let call = at_line(&graph, "follow", line(source, "    follow(next)"));
    assert_eq!(call.len(), 1);
    assert_eq!(target(&graph, call[0])["kind"], "action");
    let prop = at_line(&graph, "go", line(source, "    go(2)"));
    assert_eq!(prop.len(), 1);
    assert_eq!(target(&graph, prop[0])["kind"], "prop");
    // A prop call writes the owner's slots, none of this component's.
    let tap = definition(&graph, "action", "tap", None);
    assert_eq!(tap["writes"], serde_json::json!(["taps"]));
}

#[test]
fn an_ambiguous_call_in_an_imported_component_is_refused_alike_with_its_declaration() {
    // LLP 1089 D1: build and navigation report the same refusal, its
    // related location in the imported file where the prop is declared.
    let f = Fixture::new("ambiguous");
    let viewer = f.write(
        "viewer.contract",
        "component Viewer\n  props\n    close: action\n  action swiped\n    close()\n  view\n    button press=swiped testId=\"swipe\"\n      text \"x\"\n",
    );
    let root = f.write(
        "app.contract",
        "use Viewer from \"./viewer.contract\"\ncomponent App\n  state open = true\n  action dismiss\n    open = false\n  view\n    Viewer(close=dismiss)\n",
    );
    let build = contract::compile_path(&root).unwrap_err();
    let query = contract::symbols_json(&root, None).unwrap_err();
    assert_eq!(build, query.errors[0]);
    assert_eq!(build.id, "syntax-call-ambiguous", "{build}");
    let viewer = viewer.canonicalize().unwrap();
    assert_eq!(build.file.as_deref(), Some(viewer.as_path()));
    assert_eq!(build.span.line, 5);
    let related = build.related.first().expect("the declaration");
    assert_eq!(
        (related.file.as_deref(), related.span.line),
        (Some(viewer.as_path()), 3)
    );
}

/// `contract symbols` run as an editor runs it: its exit code, its stdout as
/// JSON, and its stderr.
fn cli(root: &std::path::Path) -> (Option<i32>, Value, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_contract"))
        .arg("symbols")
        .arg(root)
        .output()
        .unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    let graph = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}); stderr: {stderr}"));
    (out.status.code(), graph, stderr)
}

/// A refused program's output: exit 1, the build's first refusal first in
/// `errors` and on stderr, and none of what was refused named.
fn refused(root: &std::path::Path, hidden: &[&str]) -> Value {
    let (code, graph, stderr) = cli(root);
    assert_eq!(code, Some(1), "{graph}");
    let build = contract::compile_path(root).unwrap_err();
    let first: Value = serde_json::from_str(&build.to_json()).unwrap();
    assert_eq!(graph["errors"][0], first, "{graph}");
    assert!(stderr.contains(&build.to_string()), "{stderr}");
    for name in hidden {
        assert!(
            defs(&graph)
                .iter()
                .chain(refs(&graph))
                .all(|s| s["name"] != *name),
            "{name}: {graph}"
        );
    }
    // A program some of which was refused writes no inferred effects.
    assert!(defs(&graph).iter().all(|d| d.get("writes").is_none()));
    graph
}

#[test]
fn a_syntax_refusal_in_one_file_keeps_the_symbols_of_what_parsed() {
    let f = Fixture::new("partial-syntax");
    let app = "use Row from \"./row.contract\"\ncomponent App\n  state count = 0\n  view\n    column\n      text `${count}`\n      Row(label=\"x\")\n";
    let row = "component Row\n  props\n    label: string\n  state taps = 0\n  action tap\n    taps = = 1\n  action reset\n    taps = 0\n  view\n    button press=reset\n      text label\n";
    f.write("row.contract", row);
    let root = f.write("app.contract", app);
    let graph = refused(&root, &["tap"]);
    assert!(graph["errors"][0]["file"]
        .as_str()
        .unwrap()
        .ends_with("row.contract"));
    for (kind, name) in [
        ("component", "App"),
        ("state", "count"),
        ("component", "Row"),
        ("prop", "label"),
        ("state", "taps"),
        ("action", "reset"),
    ] {
        definition(&graph, kind, name, None);
    }
    let edges = |name: &str, line: usize| -> Vec<&str> {
        at_line(&graph, name, line)
            .iter()
            .map(|r| target(&graph, r)["kind"].as_str().unwrap())
            .collect()
    };
    assert_eq!(edges("count", 6), ["state"]);
    assert_eq!(edges("Row", 7), ["component"]);
    assert_eq!(edges("label", 7), ["prop"]);
    assert_eq!(edges("taps", 8), ["state"]);
    assert_eq!(edges("reset", 10), ["action"]);
    assert_eq!(edges("label", 11), ["prop"]);
}

/// The kinds of the definitions each reference to `name` on `line` has an
/// edge to.
fn edges<'a>(graph: &'a Value, name: &str, line: usize) -> Vec<&'a str> {
    at_line(graph, name, line)
        .iter()
        .map(|r| target(graph, r)["kind"].as_str().unwrap())
        .collect()
}

#[test]
fn a_program_that_does_not_load_keeps_each_file_that_parsed() {
    let f = Fixture::new("partial-load");
    // A `use` that names no file: the root and its members file still read.
    let more = f.write(
        "more.contract",
        "members App\n  derive doubled = 2\n  derive twice = doubled * 2\n",
    );
    let root = f.write(
        "app.contract",
        "use Row from \"./missing.contract\"\ncomponent App\n  state count = 0\n  members \"./more.contract\"\n  view\n    text `${count} ${doubled}`\n    Row()\n",
    );
    let graph = refused(&root, &[]);
    assert_eq!(
        graph["errors"][0]["id"], "contract-use-unreadable",
        "{graph}"
    );
    definition(&graph, "component", "App", None);
    assert_eq!(edges(&graph, "count", 6), ["state"]);
    assert!(at_line(&graph, "Row", 7).is_empty());
    // No edge leaves a file when the program does not load.
    assert!(at_line(&graph, "doubled", 6).is_empty());
    let more = more.canonicalize().unwrap();
    let more = more.to_string_lossy();
    let doubled = definition(&graph, "derive", "doubled", None);
    assert_eq!(
        (doubled["file"].as_str(), doubled["component"].as_str()),
        (Some(more.as_ref()), Some("App"))
    );
    assert_eq!(edges(&graph, "doubled", 3), ["derive"]);
    let components = defs(&graph).iter().filter(|d| d["kind"] == "component");
    assert_eq!(components.count(), 1, "{graph}");

    // An imported file whose first component's view is refused: it has no
    // symbols, and the root and the files after it keep theirs.
    let broken = f.write(
        "row.contract",
        "component Row\n  props\n    label: string\n  view\n    text = =\n",
    );
    let label = f.write("label.contract", "fn label(n: number): string = `${n}`\n");
    let root = f.write(
        "app.contract",
        "use Row from \"./row.contract\"\nuse label from \"./label.contract\"\ncomponent App\n  state count = 0\n  view\n    text label(count)\n    Row(label=\"x\")\n",
    );
    let graph = refused(&root, &[]);
    let broken = broken.canonicalize().unwrap();
    assert!(defs(&graph)
        .iter()
        .chain(refs(&graph))
        .all(|s| s["file"] != broken.to_string_lossy().as_ref()));
    assert_eq!(edges(&graph, "count", 6), ["state"]);
    assert!(at_line(&graph, "label", 6).is_empty());
    assert!(at_line(&graph, "Row", 7).is_empty());
    let function = definition(&graph, "fn", "label", None);
    assert_eq!(
        function["file"],
        label.canonicalize().unwrap().to_string_lossy().as_ref()
    );
}

#[test]
fn a_name_declared_twice_in_one_scope_has_no_edge() {
    let f = Fixture::new("partial-twice");
    let source = "component App\n  state count = 0\n  state count = 1\n  state other = 0\n  derive shown = 1\n  action shown\n    other = 2\n  view\n    text `${count} ${shown} ${other}`\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &[]);
    let twice = defs(&graph).iter().filter(|d| d["name"] == "count").count();
    assert_eq!(twice, 2, "{graph}");
    definition(&graph, "derive", "shown", None);
    definition(&graph, "action", "shown", None);
    assert!(at_line(&graph, "count", 9).is_empty(), "{graph}");
    assert!(at_line(&graph, "shown", 9).is_empty(), "{graph}");
    assert_eq!(edges(&graph, "other", 9), ["state"]);
    assert_eq!(edges(&graph, "other", 7), ["state"]);
}

#[test]
fn a_refused_member_hides_its_name_in_its_own_component_only() {
    let f = Fixture::new("partial-scope");
    let source = "component App\n  state count = 0\n  view\n    text `${count}`\n    Row()\n\ncomponent Row\n  state count = 0\n  state taps = 0\n  action count\n    taps = = 1\n  action go\n    let count = 5\n    taps = count\n  view\n    button press=go\n      text `${count}`\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &[]);
    // Another component's member and a local keep their edges.
    assert_eq!(edges(&graph, "count", 4), ["state"]);
    assert_eq!(edges(&graph, "count", 14), ["local"]);
    assert_eq!(edges(&graph, "go", 16), ["action"]);
    // In Row, `count` may be the refused action.
    assert!(at_line(&graph, "count", 17).is_empty(), "{graph}");
}

#[test]
fn a_refused_shape_or_function_leaves_the_others_their_edges() {
    let f = Fixture::new("partial-declarations");
    let source = "shape Good\n  x: number\nshape Bad\n  y: Missing\nfn add(a: number, b: number): number = a + b\nfn bad(a: number): string = a\ncomponent App\n  derive sum = add(1, 2)\n  derive good = Good(x=1)\n  derive odd = bad(1)\n  view\n    text `${sum}`\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &[]);
    assert_eq!(edges(&graph, "add", 8), ["fn"]);
    assert_eq!(edges(&graph, "Good", 9), ["shape"]);
    assert_eq!(edges(&graph, "x", 9), ["field"]);
    assert!(at_line(&graph, "bad", 10).is_empty(), "{graph}");
    definition(&graph, "fn", "bad", None);
    definition(&graph, "shape", "Bad", None);
}

#[test]
fn a_fn_that_calls_t_keeps_its_edges_when_something_else_is_refused() {
    let f = Fixture::new("partial-strings");
    f.write("strings/en.json", r#"{ "hi": "Hello" }"#);
    let source = "fn greeting(): string = t(\"hi\")\ncomponent App\n  view\n    text greeting()\n    text nope\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &[]);
    assert_eq!(edges(&graph, "greeting", 4), ["fn"], "{graph}");
}

#[test]
fn a_type_refusal_keeps_what_names_alone_resolve() {
    let f = Fixture::new("partial-types");
    let source = "shape Item\n  label: string\ncomponent App\n  state count = 0\n  state item = Item(label=\"a\")\n  action bump\n    count = \"many\"\n  view\n    button press=bump\n      text item.label\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &[]);
    assert!(graph["errors"][0]["id"]
        .as_str()
        .unwrap()
        .starts_with("type-"));
    let edges = |name: &str, line: usize| -> Vec<&str> {
        at_line(&graph, name, line)
            .iter()
            .map(|r| target(&graph, r)["kind"].as_str().unwrap())
            .collect()
    };
    definition(&graph, "action", "bump", None);
    definition(&graph, "field", "label", Some("Item"));
    assert_eq!(edges("Item", 5), ["shape"]);
    assert_eq!(edges("label", 5), ["field"]);
    assert_eq!(edges("count", 7), ["state"]);
    assert_eq!(edges("bump", 9), ["action"]);
    assert_eq!(edges("item", 10), ["state"]);
    // A field read is resolved through the type the check inferred, which
    // a refused check may have inferred wrongly: it has no edge.
    assert_eq!(edges("label", 10), Vec::<&str>::new());
}

#[test]
fn an_unresolved_name_has_no_reference_and_the_rest_resolve() {
    let f = Fixture::new("partial-unknown");
    let source = "component App\n  state label = \"a\"\n  derive broken = missing\n  view\n    text `${label} ${broken}`\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &["missing"]);
    assert!(graph["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("missing"));
    assert_eq!(at_line(&graph, "label", 5).len(), 1);
    assert_eq!(
        target(&graph, at_line(&graph, "broken", 5)[0])["kind"],
        "derive"
    );
}

#[test]
fn a_refusal_in_a_members_file_keeps_its_other_members() {
    let f = Fixture::new("partial-members");
    let more = f.write(
        "more.contract",
        "members App\n  derive doubled = count * 2\n  action broken\n    count = = 2\n",
    );
    let root = f.write(
        "app.contract",
        "component App\n  state count = 1\n  members \"./more.contract\"\n  view\n    text `${count} ${doubled}`\n",
    );
    let graph = refused(&root, &["broken"]);
    let more = more.canonicalize().unwrap();
    let more = more.to_string_lossy();
    assert_eq!(graph["errors"][0]["file"], more.as_ref());
    let doubled = definition(&graph, "derive", "doubled", None);
    assert_eq!(doubled["file"], more.as_ref());
    let read = refs(&graph)
        .iter()
        .find(|r| r["name"] == "count" && r["file"] == more.as_ref())
        .unwrap_or_else(|| panic!("{graph}"));
    assert_eq!(target(&graph, read)["line"], 2);
    assert_eq!(target(&graph, at_line(&graph, "doubled", 5)[0]), doubled);
}

#[test]
fn a_program_with_no_refusal_prints_exactly_its_definitions_and_references() {
    let f = Fixture::new("whole");
    let root = f.write("app.contract", SOURCE);
    let (code, graph, stderr) = cli(&root);
    assert_eq!((code, stderr.as_str()), (Some(0), ""));
    let keys: Vec<_> = graph.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["definitions", "references"]);
    assert!(defs(&graph).iter().all(|d| d.get("type").is_none()));
    assert_eq!(
        serde_json::to_string(&graph).unwrap(),
        contract::symbols_json(&root, None).unwrap()
    );
}

#[test]
fn editor_adds_types_shapes_and_elements_and_changes_nothing_else() {
    let f = Fixture::new("editor-cli");
    let root = f.write("app.contract", SOURCE);
    let out = Command::new(env!("CARGO_BIN_EXE_contract"))
        .args(["symbols", "--editor", root.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut editor: Value = serde_json::from_slice(&out.stdout).unwrap();
    let keys: Vec<_> = editor.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["definitions", "elements", "references", "shapes"]);
    assert!(defs(&editor).iter().any(|d| d.get("type").is_some()));
    // Without its additions, the editor's graph is the plain one.
    let editor = editor.as_object_mut().unwrap();
    editor.remove("elements");
    editor.remove("shapes");
    for d in editor["definitions"].as_array_mut().unwrap() {
        d.as_object_mut().unwrap().remove("type");
    }
    let (_, plain, _) = cli(&root);
    assert_eq!(Value::Object(editor.clone()), plain);
}

#[test]
fn a_file_of_routes_alone_is_read_without_a_component() {
    let f = Fixture::new("partial-routes");
    let root = f.write("app.contract", "routes nav\n  home \"/\"\n");
    let graph = refused(&root, &[]);
    definition(&graph, "route", "home", None);
    f.write(
        "nav.contract",
        "routes nav\n  home \"/\"\nshape Item\n  label: string\n",
    );
    let root = f.write(
        "app.contract",
        "use Item from \"./nav.contract\"\ncomponent App\n  state count = 0\n  view\n    text `${count}`\n",
    );
    let graph = refused(&root, &[]);
    definition(&graph, "route", "home", None);
    definition(&graph, "shape", "Item", None);
    assert_eq!(edges(&graph, "count", 5), ["state"]);
}

#[test]
fn a_refused_route_table_leaves_shapes_and_functions_their_edges() {
    let f = Fixture::new("partial-route-table");
    let table = "routes nav\n  home \"/posts\"\n";
    let declarations =
        "shape Item\n  label: string\nfn add(a: number, b: number): number = a + b\n";
    let app = "component App\n  derive item = Item(label=\"a\")\n  derive sum = add(1, 2)\n  view\n    text `${sum}`\n";
    for source in [
        format!("{table}{declarations}{app}"),
        format!("{declarations}{table}{app}"),
    ] {
        let root = f.write("app.contract", &source);
        let graph = refused(&root, &[]);
        let at = |text| line(&source, text);
        assert_eq!(
            edges(&graph, "Item", at("derive item")),
            ["shape"],
            "{source}"
        );
        assert_eq!(
            edges(&graph, "label", at("derive item")),
            ["field"],
            "{source}"
        );
        assert_eq!(edges(&graph, "add", at("derive sum")), ["fn"], "{source}");
    }
}

#[test]
fn a_name_declared_twice_has_no_edge_whatever_reads_it() {
    let f = Fixture::new("partial-twice-target");
    let source = "component App\n  state count = 0\n  derive count = 1\n  action reset\n    count = 0\n  view\n    button press=reset\n      text \"x\"\n";
    let graph = refused(&f.write("app.contract", source), &[]);
    assert!(at_line(&graph, "count", 5).is_empty(), "{graph}");
    assert_eq!(edges(&graph, "reset", 7), ["action"]);
    let source = "shape Pair\n  a: number\nfn Pair(a: number): number = a\ncomponent App\n  derive p = Pair(a=1)\n  view\n    text \"x\"\n";
    let graph = refused(&f.write("app.contract", source), &[]);
    assert!(at_line(&graph, "Pair", 5).is_empty(), "{graph}");
    assert!(at_line(&graph, "a", 5).is_empty(), "{graph}");
}

#[test]
fn a_refused_name_is_hidden_in_its_own_file_and_component() {
    let f = Fixture::new("partial-identity");
    let good = f.write("good.contract", "fn label(n: number): string = `${n}`\n");
    f.write(
        "row.contract",
        "fn label(n: number): string = = 1\ncomponent Row\n  state count = 0\n  action count\n    count = = 1\n  view\n    text `${count}`\n",
    );
    f.write(
        "card.contract",
        "use Row from \"./row.contract\"\ncomponent Card\n  view\n    Row()\n",
    );
    let source = "use label from \"./good.contract\"\nuse Card from \"./card.contract\"\ncomponent App\n  state count = 0\n  view\n    text label(count)\n    Card()\n    Row()\n\ncomponent Row\n  state count = 0\n  view\n    text `${count}`\n";
    let root = f.write("app.contract", source);
    let graph = refused(&root, &[]);
    // Another file's `label` keeps its edge.
    let call = at_line(&graph, "label", 6);
    assert_eq!(call.len(), 1, "{graph}");
    let good = good.canonicalize().unwrap();
    assert_eq!(
        target(&graph, call[0])["file"],
        good.to_string_lossy().as_ref()
    );
    // The root's own `Row` keeps its edge; row.contract's, renamed, has none.
    assert_eq!(edges(&graph, "count", 13), ["state"]);
    let row = f.0.join("row.contract").canonicalize().unwrap();
    let in_row = refs(&graph)
        .iter()
        .filter(|r| r["name"] == "count" && r["file"] == row.to_string_lossy().as_ref());
    assert_eq!(in_row.count(), 0, "{graph}");
}

#[test]
fn elements_report_their_tag_attribute_names_string_values_and_open_line_ends() {
    let f = Fixture::new("elements");
    let source = "component App\n  state liked = false\n  action like\n    liked = not liked\n  view\n    main\n      button press=like aria-label=(liked ? \"Unlike\" : \"Like\") testId=\"like\"  \n        text (liked ? \"on\" : \"off\")\n      h2 \"Title\"\n      button \"Save\" press=like\n      box padding=8\n        -exact-fit=\"shrink\" // why\n";
    let graph = f.editor_query(source);
    let elements = graph["elements"].as_array().unwrap();
    let tags: Vec<_> = elements
        .iter()
        .map(|e| (spelling(e), e["tag"].as_str().unwrap()))
        .collect();
    // A heading is a `text` written as its level; a button's one argument is
    // its `text` child, at the button's tag, and no element of its own.
    assert_eq!(
        tags,
        [
            ("main", "main"),
            ("button", "button"),
            ("text", "text"),
            ("h2", "text"),
            ("button", "button"),
            ("box", "box")
        ]
        .map(|(s, t)| (s.to_owned(), t))
    );
    assert!(elements.iter().all(|e| e["component"] == "App"));
    let button = &elements[1];
    let attrs = button["attrs"].as_array().unwrap();
    assert_eq!(
        attrs
            .iter()
            .map(|a| spelling(&json_at(button, a)))
            .collect::<Vec<_>>(),
        ["press", "aria-label", "testId"]
    );
    assert_eq!(
        attrs
            .iter()
            .map(|a| a["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["press", "aria-label", "testId"]
    );
    assert!(attrs[..2].iter().all(|a| a.get("string").is_none()));
    let id = &attrs[2]["string"];
    assert_eq!(id["value"], "like");
    assert_eq!(spelling(&json_at(button, id)), "\"like\"");
    assert!(
        elements[3]["attrs"].as_array().unwrap().is_empty(),
        "{graph}"
    );
    let box_ = &elements[5];
    assert_eq!(box_["attrs"][1]["name"], "-exact-fit");
    assert_eq!(spelling(&json_at(box_, &box_["attrs"][1])), "-exact-fit");
    // A name query has no elements.
    let named: Value = serde_json::from_str(
        &contract::editor_symbols_json(&f.0.join("app.contract"), Some("like")).unwrap(),
    )
    .unwrap();
    assert!(named.get("elements").is_none());
}

/// A place inside `element`, with the element's file.
fn json_at(element: &Value, place: &Value) -> Value {
    let mut at = place.clone();
    at["file"] = element["file"].clone();
    at
}
