//! LLP 1162.001: a `source` declared in `app.contract` is checked, written to
//! the plan's `fetches` table, and left out of the data module's types.

const APP: &str = r#"shape Current
  temperature_2m: number

shape Ack
  id: string

source weather(latitude: number, place: string) as shape Current
  url "https://API.example.test:443/v1?lat={latitude}&q={place}"
  header "X-Key" "k-{place}"
  pointer "/current"

source save(title: string) as shape Ack
  url "https://api.example.test/items"
  method "POST"
  body json

component App
  resource now = weather(1, "a") as shape Current
  resource other = loadOther() as shape Current
  mutation saved as shape Ack
  action add
    send saved = save("Milk")
  view
    text `${now.temperature_2m} ${other.temperature_2m}`
"#;

#[test]
fn a_declaration_is_written_to_the_plan_with_its_origin() {
    let plan = contract::compile(APP).unwrap();
    let row = plan.fetch_named("weather").expect("the declared row");
    assert_eq!(plan.str(row.method), "GET");
    assert_eq!(plan.str(row.origin), "https://api.example.test");
    assert_eq!(plan.str(row.pointer), "/current");
    let params: Vec<_> = plan.fetch_parts(row.params).map(|(n, _)| n).collect();
    assert_eq!(params, ["latitude", "place"]);
    let headers: Vec<_> = plan.fetch_parts(row.headers).collect();
    assert_eq!(headers, [("X-Key", "k-{place}")]);
    let save = plan.fetch_named("save").unwrap();
    assert_eq!(
        (plan.str(save.method), plan.str(save.body)),
        ("POST", "json")
    );
    assert!(plan.fetch_named("loadOther").is_none());
    assert_eq!(
        exact_runner::grants::declared(&plan),
        ["net.fetch https://api.example.test"]
    );
    // The data module answers `loadOther` alone.
    let types = contract::typescript(&plan).unwrap();
    assert!(types.contains("loadOther"), "{types}");
    assert!(
        !types.contains("weather") && !types.contains("\"save\""),
        "{types}"
    );
}

fn refused(src: &str) -> String {
    match contract::compile(src) {
        Ok(_) => panic!("compiled:\n{src}"),
        Err(e) => format!("{} {}", e.id, e.message),
    }
}

#[test]
fn a_declaration_that_names_no_origin_or_disagrees_with_its_call_is_refused() {
    let head = "shape S\n  n: number\n";
    let view = "component App\n  resource r = s(1) as shape S\n  view\n    text `${r.n}`\n";
    for (decl, expected) in [
        ("source s(x: number) as shape S\n  url \"https://{x}/a\"\n", "origin is written out"),
        ("source s(x: number) as shape S\n  url \"/a/{x}\"\n", "absolute"),
        ("source s(x: number) as shape S\n  url \"ftp://a.test/{x}\"\n", "`https` or `http`"),
        ("source s(x: number) as shape S\n  url \"https://a.test/{y}\"\n", "names no parameter"),
        ("source s(x: number) as shape S\n  url \"https://a.test/{secret:t}\"\n", "never the URL"),
        ("source s(x: number) as shape S\n  method \"GET\"\n", "needs a `url`"),
        ("source s(x: number) as shape S\n  url \"https://a.test/\"\n  body json\n", "name its `method`"),
        ("source s(x: number) as shape S\n  url \"https://a.test/\"\n  graphql \"{ a }\"\n  method \"POST\"\n", "takes no `method`"),
        ("source s(x: number) as shape S\n  url \"https://a.test/\"\n  header \"Origin\" \"x\"\n", "browser sets itself"),
        ("source s(x: number) as shape S\n  url \"https://a.test/\"\n  pointer \"data\"\n", "JSON Pointer"),
        ("source s(x: string) as shape S\n  url \"https://a.test/{x}\"\n", "type-source-declared"),
    ] {
        let message = refused(&format!("{head}{decl}{view}"));
        assert!(message.contains(expected), "{decl}→ {message}");
    }
    // A record goes in a body, never a URL.
    let message = refused(&format!(
        "{head}source s(x: S) as shape S\n  url \"https://a.test/{{x}}\"\ncomponent App\n  resource r = s(S(n=1)) as shape S\n  view\n    text `${{r.n}}`\n"
    ));
    assert!(message.contains("body json"), "{message}");
}

#[test]
fn a_declaration_outside_the_root_file_is_refused() {
    let dir = std::env::temp_dir().join(format!("exact-declared-root-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("app.contract"),
        "use Card from \"./card.contract\"\ncomponent App\n  view\n    Card()\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("card.contract"),
        "shape S\n  n: number\nsource s() as shape S\n  url \"https://a.test/\"\ncomponent Card\n  view\n    text \"card\"\n",
    )
    .unwrap();
    let error = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(error.id, "contract-source-root", "{}", error.message);
}

#[test]
fn a_declaration_formats_unchanged() {
    assert_eq!(contract_syntax::fmt::format(APP).unwrap(), APP);
}
