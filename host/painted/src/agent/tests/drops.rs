//! The Linux agent protocol's drops, driven through `handle`.
use super::*;

#[test]
fn a_driven_drop_reaches_the_node_that_takes_it() {
    let plan = contract::compile("component App\n  state text = \"\"\n  state url = \"\"\n  action took(e: DragEvent)\n    text = e.text\n    url = e.url\n  view\n    column width=300 height=200 drop=took testId=\"zone\"\n").unwrap();
    let (mut p, boot_error) = Presenter::boot_with_fixture_fonts(
        &plan.encode(),
        (),
        (300.0, 300.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(boot_error.is_none(), "{boot_error:?}");
    let zone = {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id("zone")[0]).unwrap().id
    };
    let reply = handle(
        &mut p,
        &format!(
            r#"{{"op":"tap","id":{zone},"drop":[],"text":"Mount Fuji","url":"https://example.com/fuji"}}"#
        ),
    );
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(
        (&reply["tapped"], &reply["drop"], &reply["delivery"]),
        (
            &serde_json::json!(zone),
            &serde_json::json!(0),
            &serde_json::json!("presenter")
        ),
        "{reply}"
    );
    let slot = |name: &str| {
        p.host()
            .runner()
            .slot(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    assert_eq!(slot("text").as_deref(), Some("Mount Fuji"));
    assert_eq!(slot("url").as_deref(), Some("https://example.com/fuji"));
}
