//! The entries `vocab` answers for names no table lists: the attributes the
//! compiler consumes itself, and the headings.

use super::*;

#[test]
fn module_tags_distinguish_app_props_from_package_declarations() {
    let one: Value = serde_json::from_str(&stdout(&vocab(&["--json", "exact-map"]))).unwrap();
    let note = one["note"].as_str().unwrap();
    for expected in [
        "app.json's `modules`",
        "direct dependency's exact.json `views`",
        "an app module's other attributes are its props",
        "a package view accepts only its declared props and events",
        "standard box attributes",
        "press, change, hover, focus, blur, key, submit, load and message",
    ] {
        assert!(note.contains(expected), "missing {expected}: {note}");
    }
    let all: Value = serde_json::from_str(&stdout(&vocab(&["--json"]))).unwrap();
    assert_eq!(all["notes"]["modules"], note);
    for args in [&["exact-map"][..], &["--all"]] {
        let text = stdout(&vocab(args));
        assert!(text.contains(note), "{args:?}: {text}");
    }
}

#[test]
fn compiler_attributes_answer_in_text_json_and_listings() {
    for source in [
        "style Card\n  padding=12\ncomponent App\n  view\n    column class=Card -exact-row-look=\"platform\"\n      text \"a\"\n",
        "component App\n  state on = false\n  action flip(v: bool)\n    on = v\n  view\n    SwitchRow(id=\"a\", checked=on, set=flip)\ncomponent SwitchRow\n  props\n    id: string\n    checked: bool\n    set: action\n  parts\n    switch: control\n  view\n    row\n      input part=\"switch\" type=\"checkbox\" switch testId=id checked=checked change=set aria-label=\"x\"\n",
    ] {
        contract::compile(source).unwrap_or_else(|e| panic!("{e}\n{source}"));
    }
    let all: Value = serde_json::from_str(&stdout(&vocab(&["--json"]))).unwrap();
    let listing = stdout(&vocab(&["--all"]));
    for (name, accepts, rule) in [
        ("class", "class=(cond ? A : B)", "node's own attributes win"),
        ("part", "string literal", "parts"),
        ("-exact-row-look", "platform", "grouped list"),
    ] {
        let text = stdout(&vocab(&[name]));
        let one: Value = serde_json::from_str(&stdout(&vocab(&["--json", name]))).unwrap();
        let attr = &one["attribute"];
        assert_eq!(attr["name"], name, "{one}");
        assert_eq!(attr["kind"], "compile-time", "{one}");
        assert!(attr["accepts"].as_str().unwrap().contains(accepts), "{one}");
        assert!(attr["note"].as_str().unwrap().contains(rule), "{one}");
        for key in ["accepts", "only", "note"] {
            assert!(text.contains(attr[key].as_str().unwrap()), "{text}");
        }
        assert!(listing.contains(&format!("  {name} ")), "{name}");
        let listed: Vec<_> = all["attributes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["name"] == name)
            .collect();
        assert_eq!(listed, vec![attr], "{name}");
    }
}

#[test]
fn headings_answer_in_text_json_and_listings() {
    let all: Value = serde_json::from_str(&stdout(&vocab(&["--json"]))).unwrap();
    let listing = stdout(&vocab(&["--all"]));
    for level in 1..=6 {
        let name = format!("h{level}");
        contract::compile(&format!("component App\n  view\n    {name} \"Title\"\n")).unwrap();
        let text = stdout(&vocab(&[&name]));
        let one: Value = serde_json::from_str(&stdout(&vocab(&["--json", &name]))).unwrap();
        let tag = &one["tag"];
        assert_eq!(tag["name"], name);
        assert_eq!(tag["node"], "Text");
        assert_eq!(tag["positional"], "text");
        assert_eq!(
            tag["fixedProps"],
            serde_json::json!([
                { "prop": "accessibilityRole", "value": "heading" },
                { "prop": "accessibilityHeadingLevel", "value": level.to_string() },
            ])
        );
        let note = tag["note"].as_str().unwrap();
        assert_eq!(text.trim(), format!("{name}: {note}"));
        assert!(note.contains(&format!(
            "`text \"Title\" role=\"heading\" aria-level={level}`"
        )));
        assert!(listing.contains(&format!("  {name} ")), "{name}");
        let listed: Vec<_> = all["tags"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|t| t["name"] == name)
            .collect();
        assert_eq!(listed, vec![tag], "{name}");
    }
    let many: Value = serde_json::from_str(&stdout(&vocab(&["--json", "h2", "text"]))).unwrap();
    assert_eq!(many[0]["name"], "h2");
    assert_eq!(many[1]["name"], "text");
    for name in ["h0", "h7", "h01", "h+1"] {
        assert!(
            contract::compile(&format!("component App\n  view\n    {name} \"Title\"\n")).is_err()
        );
        assert_eq!(vocab(&[name]).status.code(), Some(1), "{name}");
        assert_eq!(vocab(&["--json", name]).status.code(), Some(1), "{name}");
    }
}

/// LLP 1043.000 §3 D1: the keywords `vocab` lists for a row are those the
/// compiler admits where the attribute is admitted, and the schema's
/// `refused` ones are named as refused, in text and JSON alike; `buttonStyle`
/// lists its names in JSON too.
#[test]
fn listed_keywords_compile_and_refused_ones_are_named() {
    let at = |attr: &str, v: &str| {
        match attr {
        "wrap-flow" => format!("component A\n  view\n    column position=\"relative\"\n      text \"x\"\n      box position=\"absolute\" wrap-flow=\"{v}\" width=40 height=40\n"),
        "user-select" => format!("component A\n  view\n    text \"x\" user-select=\"{v}\"\n"),
        _ => format!("component A\n  action go\n  view\n    button \"Go\" press=go buttonStyle=\"{v}\"\n"),
    }
    };
    for (attr, refused) in [
        (
            "wrap-flow",
            &["start", "end", "minimum", "maximum", "clear"][..],
        ),
        ("user-select", &["contain"]),
    ] {
        let one: Value = serde_json::from_str(&stdout(&vocab(&["--json", attr]))).unwrap();
        let row = &one["attribute"]["rows"][0];
        assert_eq!(row["refused"], serde_json::json!(refused), "{one}");
        let text = stdout(&vocab(&[attr]));
        assert!(
            text.contains(&format!("; refused: {}", refused.join("|"))),
            "{text}"
        );
        for v in row["values"].as_array().unwrap() {
            let v = v.as_str().unwrap();
            assert!(!refused.contains(&v), "{one}");
            contract::compile(&at(attr, v)).unwrap_or_else(|e| panic!("{attr}={v}: {e}"));
        }
        for v in refused {
            let error = contract::compile(&at(attr, v)).unwrap_err();
            assert!(error.message.contains("Write one of auto, "), "{error}");
        }
    }
    let one: Value = serde_json::from_str(&stdout(&vocab(&["--json", "buttonStyle"]))).unwrap();
    let names = one["attribute"]["values"].as_array().unwrap();
    assert_eq!(names.len(), 12, "{one}");
    for v in names {
        let v = v.as_str().unwrap();
        contract::compile(&at("buttonStyle", v)).unwrap_or_else(|e| panic!("{v}: {e}"));
    }
}
