use super::diagnostics::App;

const FILTER: &str = "items.filter(i => i.length > 2 and i.length < 100 and i != \"hidden\")";

fn source(reads: usize, uses: usize, root: bool, alias: bool) -> String {
    let declaration = if root {
        format!("  derive picked = {FILTER}\n")
    } else {
        String::new()
    };
    let props = if root {
        "picked: string[]"
    } else {
        "items: string[]"
    };
    let child = if root {
        String::new()
    } else {
        format!(
            "  derive picked = {}\n",
            if alias { "items" } else { FILTER }
        )
    };
    let value = if root {
        "picked".to_string()
    } else if alias {
        FILTER.to_string()
    } else {
        "items".to_string()
    };
    let argument = if root { "picked" } else { "items" };
    let calls = format!("      Child({argument}={value})\n").repeat(uses);
    let texts = "      text `${picked.length}`\n".repeat(reads);
    format!("component App\n  state items = [\"alpha\", \"b\"]\n{declaration}  view\n    column\n{calls}component Child\n  props\n    {props}\n{child}  view\n    column\n{texts}")
}

#[test]
fn component_code_costs_count_compiled_uses_and_keep_regions_with_their_component() {
    let app = App::new("component-code");
    let src = source(20, 2, false, false).replace(
        "  view\n    column\n      text",
        "  view\n    if picked.length > 0\n      text",
    );
    app.write("app.contract", &src);
    let (plan, map) = contract::compile_path_mapped(&app.0.join("app.contract")).unwrap();
    let costs = map.component_code_costs(&plan);
    assert_eq!(costs.len(), 2, "{costs:?}");
    let child = costs.iter().find(|c| c.0 == "Child").unwrap();
    assert_eq!(child.1, 2);
    assert!(
        child.2 > plan.code.len(),
        "expanded uses include shared bodies: {costs:?}"
    );
    assert_eq!(costs[0].0, "Child");
    let root = costs.iter().find(|c| c.0 == "App").unwrap();
    assert_eq!(root.1, 1);
    let output = app.run(&["app.contract"]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("own plan code by component (shared bodies counted per use)")
            && text.contains("Child:")
            && text.contains("(2 instances)"),
        "{text}"
    );
}
