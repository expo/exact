//! LLP 1102 §3.10–§3.11: CSS's own spellings reach the DOM as CSS. An unbounded
//! maximum is `none` (`max-width: auto` is no value), whether it was written
//! `none` or `auto`, and a pixel row's `14px` is its number in pixels. And
//! §3.6: `autocomplete` is HTML's attribute on a field.

use exact_runner::{DataError, DataSource, Value};
use exact_web::Host;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, s: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(s.into()))
    }
}

#[test]
fn a_bars_large_title_size_follows_the_effective_root() {
    let plan = contract::compile(
        r#"
component App
  action fixed
    setRootFontSize(20)
  action relative
    setRootFontSize("125%")
  action reset
    setRootFontSize("medium")
  view
    column
      text "Inbox" font="-exact-large-title"
      button "Fixed" testId="fixed" press=fixed
      button "Relative" testId="relative" press=relative
      button "Reset" testId="reset" press=reset
"#,
    )
    .unwrap();
    let (mut host, first) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
    let size = |batch: &str| {
        let batch: serde_json::Value = serde_json::from_str(batch).unwrap();
        assert!(batch["error"].is_null(), "{batch}");
        let sizes: Vec<_> = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|op| op["op"] == "large-title")
            .map(|op| op["size"].as_f64().unwrap())
            .collect();
        assert!(sizes.len() <= 1, "{batch}");
        sizes.first().copied()
    };
    assert_eq!(size(&first), Some(33.0));
    for (root, large) in [
        (14.0, 31.0),
        (16.0, 33.0),
        (20.0, 37.0),
        (53.0, 60.0),
        (18.0, 35.0),
    ] {
        assert_eq!(size(&host.set_root_font_size(root)), Some(large));
        assert_eq!(size(&host.set_root_font_size(root)), None);
    }
    let press = |host: &mut Host<NoData>, name| {
        let kernel = host.runner().kernel();
        let key = kernel.find_by_test_id(name)[0];
        let id = kernel.node_by_key(key).unwrap().id;
        host.dispatch(id, exact_runner::Event::Press)
    };
    assert_eq!(size(&press(&mut host, "fixed")), Some(37.0));
    assert_eq!(size(&host.set_root_font_size(53.0)), None);
    assert_eq!(size(&press(&mut host, "relative")), Some(75.0));
    assert_eq!(size(&host.set_root_font_size(14.0)), Some(34.5));
    assert_eq!(size(&press(&mut host, "reset")), Some(31.0));
    assert_eq!(size(&host.set_root_font_size(f64::from(f32::MAX))), None);
}

#[test]
fn an_unbounded_maximum_is_none_and_a_px_text_is_pixels() {
    let src = "component App\n  view\n    column max-height=\"none\" max-width=\"auto\"\n      text \"a\" font-size=\"14px\" letter-spacing=\"-0.5px\"\n";
    let plan = contract::compile(src).unwrap();
    let (_, batch) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
    for expected in [
        "max-height:none",
        "max-width:none",
        "font-size:14px",
        "letter-spacing:-0.5px",
    ] {
        assert!(batch.contains(expected), "{expected} in {batch}");
    }
    assert!(
        !batch.contains("max-height:auto") && !batch.contains("max-width:auto"),
        "{batch}"
    );
}

#[test]
fn autocomplete_is_the_fields_attribute() {
    let src = "component App\n  view\n    column\n      input type=\"password\" autocomplete=\"section-a new-password\"\n      textarea autocomplete=\"off\"\n";
    let plan = contract::compile(src).unwrap();
    let (_, batch) = Host::boot(&plan.encode(), NoData, Default::default(), "/").unwrap();
    for expected in [
        "\"autocomplete\":\"section-a new-password\"",
        "\"autocomplete\":\"off\"",
    ] {
        assert!(batch.contains(expected), "{expected} in {batch}");
    }
    assert!(!batch.contains("data-autocomplete"), "{batch}");
}
