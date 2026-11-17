use exact_kernel::Kernel;
use exact_plan::{BindingKind, Value};
use exact_runner::{DataError, DataSource, Runner};

struct Empty;
impl DataSource for Empty {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

#[test]
fn child_aliases_keep_static_values_tags_styles_and_image_priority() {
    for (value, view) in [
        (
            "[1]",
            "    for page in VALUE key=page\n      image \"hero.png\"\n",
        ),
        (
            "[1, 2]",
            "    for page in VALUE key=page\n      image \"hero.png\"\n",
        ),
        (
            "stack(nav)",
            "    for page in VALUE key=page.id\n      image \"hero.png\"\n",
        ),
        ("\"date\"", "    input type=VALUE\n"),
        ("\"text\"", "    input type=VALUE\n"),
        ("\"none\"", "    text \"body\" markup=VALUE\n"),
        ("\"markdown\"", "    text \"**body**\" markup=VALUE\n"),
        ("\"**body**\"", "    text VALUE markup=\"markdown\"\n"),
        (
            "false",
            "    list virtualized=VALUE\n      for x in [1] key=x\n        text `${x}`\n",
        ),
        ("32", "    canvas bitmap-width=VALUE bitmap-height=VALUE\n"),
        ("\"red\"", "    main background-color=VALUE\n"),
        ("\"cover\"", "    main viewport-fit=VALUE\n"),
        ("\"tab\"", "    button role=VALUE\n      text \"Tab\"\n"),
    ] {
        let mut observations = Vec::new();
        for derived in [false, true] {
            let declaration = if derived {
                format!("  derive base = {value}\n  derive value = base\n")
            } else {
                String::new()
            };
            let view = if view.starts_with("    for ") {
                format!(
                    "    column\n{}",
                    view.lines().map(|l| format!("  {l}\n")).collect::<String>()
                )
            } else {
                view.to_owned()
            };
            let source = format!("routes nav\n  home \"/\"\ncomponent App\n  view\n    Child()\ncomponent Child\n{declaration}  view\n{}", view.replace("VALUE", if derived { "value" } else { value }));
            let source = if value == "stack(nav)" {
                source.replace("    Child()", "    Child(nav=nav)").replace(
                    "component Child\n",
                    "component Child\n  props\n    nav: Router\n",
                )
            } else {
                source
            };
            let plan = contract::compile(&source).unwrap();
            assert_eq!(
                plan.derives.iter().any(|row| row.unsettled),
                derived,
                "{source}"
            );
            let generated = crate::emit::emit(&plan, false, false).unwrap();
            let prepared = serde_json::to_string(&(
                "**body**",
                exact_web_capabilities::markdown::pieces("**body**", "", ""),
            ))
            .unwrap();
            let prepared = generated.js.contains(&prepared);
            if value == "\"markdown\"" || value == "\"**body**\"" {
                assert!(generated.markdown && prepared, "{source}: {}", generated.js);
            }
            let classifications: Vec<_> = plan
                .bindings
                .iter()
                .filter(|b| b.kind == BindingKind::Prop || b.kind == BindingKind::Style)
                .map(|b| {
                    (
                        b.kind,
                        b.id,
                        crate::style::literal(&plan, plan.code(b.expr)),
                        exact_runner::vm::is_literal(&plan, plan.code(b.expr)),
                    )
                })
                .collect();
            let runner = Runner::boot(
                plan,
                Empty,
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap();
            let doc = exact_web::document::project_for_js(&runner).unwrap();
            if value == "[1]" || value == "stack(nav)" {
                assert!(doc.hero_image.is_some(), "{source}: {}", doc.root);
                assert!(!doc.root.contains("loading=\"lazy\""), "{}", doc.root);
                assert!(
                    !generated.js.contains("\"loading\",\"lazy\""),
                    "{}",
                    generated.js
                );
            }
            observations.push((
                generated.markdown,
                prepared,
                classifications,
                generated.css,
                doc.root,
                doc.hero_image,
                exact_runner::uses(runner.plan()),
            ));
        }
        assert_eq!(observations[0], observations[1], "{value}: {view}");
    }
}

#[test]
fn enum_css_does_not_treat_a_derived_constant_as_a_second_key() {
    let source = "component App\n  state enabled = true\n  action change\n    enabled = !enabled\n  view\n    Child(enabled=enabled)\ncomponent Child\n  props\n    enabled: bool\n  derive color = \"red\"\n  view\n    text \"value\" color=(enabled ? color : \"blue\")\n";
    let plan = contract::compile(source).unwrap();
    let emitted = crate::emit::emit(&plan, false, false).unwrap();
    assert!(
        !emitted.js.contains("S($e,"),
        "an opaque call must keep the live CSS effect"
    );
}
