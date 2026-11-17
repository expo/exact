use exact_apple::abi::Bridge;
use exact_plan::Value;
use exact_runner::{DataError, DataSource};

#[derive(Default)]
struct StorageModule;
impl DataSource for StorageModule {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}
use exact_kernel::{Kernel, NodeType};
use exact_plan::{EventKind, NodesId};
use exact_runner::{Capability, Event, Runner};

#[derive(Debug)]
enum Check {
    Nested(usize),
    Cover(bool),
    Type(bool, bool),
    Pages(bool),
    Retained(&'static str),
}

fn source(value: &str, view: &str, derived: bool) -> String {
    let declaration = if derived {
        format!("  derive base = {value}\n  derive value = base\n")
    } else {
        String::new()
    };
    let view = view.replace("VALUE", if derived { "value" } else { value });
    let view = if view.starts_with("    for ") {
        format!(
            "    column\n{}",
            view.lines().map(|l| format!("  {l}\n")).collect::<String>()
        )
    } else {
        view
    };
    format!("component App\n  state out = \"\"\n  action save(arg: string)\n    out = arg\n  view\n    Child()\ncomponent Child\n  inject\n    save: action\n{declaration}  view\n{view}")
        .replace("  view\n    Child()", "  provide\n    save\n  view\n    Child()")
}

#[test]
fn static_readers_classify_and_run_child_derives_like_their_bodies() {
    let mut failures = Vec::new();
    let cases = [
        ("nested true", "true", "    list virtualized=VALUE height=80\n      for x in [1, 2] key=x\n        text `${x}`\n", Check::Nested(2)),
        ("nested false", "false", "    list virtualized=VALUE height=80\n      for x in [1, 2] key=x\n        text `${x}`\n", Check::Nested(1)),
        ("cover", "\"cover\"", "    main viewport-fit=VALUE\n", Check::Cover(true)),
        ("auto", "\"auto\"", "    main viewport-fit=VALUE\n", Check::Cover(false)),
        ("date", "\"date\"", "    input type=VALUE\n", Check::Type(false, true)),
        ("time", "\"time\"", "    input type=VALUE\n", Check::Type(false, true)),
        ("datetime", "\"datetime-local\"", "    input type=VALUE\n", Check::Type(false, true)),
        ("text", "\"text\"", "    input type=VALUE\n", Check::Type(false, false)),
        ("file", "\"file\"", "    input type=VALUE\n", Check::Type(true, false)),
        ("singleton", "[1]", "    for x in VALUE key=x\n      image \"hero.png\"\n", Check::Pages(false)),
        ("rows", "[1, 2]", "    for x in VALUE key=x\n      image \"hero.png\"\n", Check::Pages(true)),
        ("stack", "stack(nav)", "    for x in VALUE key=x.id\n      image \"hero.png\"\n", Check::Pages(false)),
        ("retained record", "item", "    for inner in [2] key=inner\n      if true\n        button testId=\"target\" press=save(VALUE.id) swiperight=save(VALUE.id) swipeleft=save(VALUE.id)\n          text \"Save\"\n", Check::Retained("kept")),
        ("retained bound", "item.id", "    for inner in [2] key=inner\n      if true\n        button testId=\"target\" press=save(VALUE) swiperight=save(VALUE) swipeleft=save(VALUE)\n          text \"Save\"\n", Check::Retained("kept")),
        ("retained item", "item.id", "    for inner in [2] key=inner\n      if true\n        button testId=\"target\" press=save(VALUE) swiperight=save(VALUE) swipeleft=save(VALUE)\n          text \"Save\"\n", Check::Retained("kept")),
        ("retained string", "\"kept\"", "    button testId=\"target\" press=save(VALUE) swiperight=save(VALUE) swipeleft=save(VALUE)\n      text \"Save\"\n", Check::Retained("kept")),
    ];
    for (name, value, view, check) in cases {
        let results: Vec<_> = [false, true].into_iter().map(|derived| {
            let mut src = source(value, view, derived);
            if matches!(check, Check::Nested(_)) {
                src = src.replace("    Child()", "    list virtualized=true height=200\n      for outer in [1] key=outer\n        Child()");
            }
            if name == "retained item" || name == "retained record" || name == "retained bound" {
                src = format!("shape Item\n  id: string\n{src}")
                    .replace("    Child()", "    column\n      for item in [Item(id=\"kept\")] key=item.id\n        Child(item=item)")
                    .replace("component Child\n", "component Child\n  props\n    item: Item\n");
            }
            if name == "retained bound" {
                src = src.replace("      for item in [Item(id=\"kept\")] key=item.id\n        Child(item=item)", "      match some(Item(id=\"kept\"))\n        case some(item)\n          Child(item=item)\n        case none\n          text \"none\"");
            }
            if name == "retained record" {
                src = format!("shape Box\n  item: Item\n{src}")
                    .replace("for item in [Item(id=\"kept\")] key=item.id", "for outer in [Box(item=Item(id=\"kept\"))] key=outer.item.id")
                    .replace("Child(item=item)", "Child(item=outer.item)");
            }
            if name == "stack" {
                src = format!("routes nav\n  home \"/\"\n{src}")
                    .replace("    Child()", "    Child(nav=nav)")
                    .replace("component Child\n", "component Child\n  props\n    nav: Router\n");
            }
            let plan = contract::compile(&src).map_err(|e| format!("compile {e:?}"))?;
            if derived { assert!(plan.derives.iter().any(|d| d.unsettled), "{name}"); }
            let uses = exact_runner::uses(&plan);
            let mut bridge = Bridge::<StorageModule>::new();
            let len = bridge.input_write(&plan.encode());
            let cover = bridge.plan_covers_viewport(len, &[]);
            match check {
                Check::Cover(expected) if cover != expected => return Err(format!("cover {cover}, expected {expected}")),
                Check::Type(picker, dates) if uses.has(Capability::Picker) != picker || uses.has(Capability::Dates) != dates => return Err(format!("capabilities {uses:?}")),
                Check::Pages(expected) => {
                    let node = plan.nodes.iter().position(|n| n.node_type == NodeType::Image as u8).unwrap();
                    let actual = exact_runner::rows::repeated(&plan, NodesId(node as u32));
                    if actual != expected { return Err(format!("repeated {actual}, expected {expected}")); }
                }
                _ => {}
            }
            let mut runner = Runner::boot(plan, StorageModule, Kernel::with_monospace(), Default::default(), "/")
                .map_err(|e| format!("boot {e:?}"))?;
            if let Check::Nested(expected) = check {
                if runner.collections().len() != expected { return Err(format!("collections {}, expected {expected}", runner.collections().len())); }
            }
            if let Check::Retained(expected) = check {
                for (kind, event) in [(EventKind::Press, Event::Press), (EventKind::Swiperight, Event::Swiperight), (EventKind::Swipeleft, Event::Swipeleft)] {
                    let key = runner.kernel().find_by_test_id("target")[0];
                    let binding = runner.capture_action_binding(key, kind).map_err(|e| format!("capture {e:?}"))?;
                    runner.dispatch_bound(&binding, event).map_err(|e| format!("dispatch {e:?}"))?;
                    if runner.slot("out").and_then(Value::as_str) != Some(expected) { return Err(format!("out {:?}", runner.slot("out"))); }
                }
            }
            Ok((uses, cover, exact_runner::agent::tree(&runner)))
        }).collect();
        if results[0].is_err() || results[0] != results[1] {
            failures.push(format!(
                "{name}: literal {:?}, derive {:?}",
                results[0], results[1]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
