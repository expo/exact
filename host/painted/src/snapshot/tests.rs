use super::*;
use serde_json::Value;
use std::path::PathBuf;

/// Ten minutes before Caltrain's first preview departure.
const NOW: f64 = 1_787_915_100_000.0;

fn app(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps")
        .join(name)
}

/// The app's entry plans as its bake compiles them.
fn entry_plan(name: &str, kind: &str) -> Vec<u8> {
    let dir = app(name);
    let manifest = contract::Manifest::read(&dir).unwrap();
    let entries = contract::snapshot::compile_entries(&dir, &manifest).unwrap();
    entries[kind].plan.encode()
}

fn render_one(
    plan: &[u8],
    kind: &str,
    props: Option<&[u8]>,
    configuration: Option<&[u8]>,
    size: (f32, f32),
) -> Value {
    let entry = Entry {
        plan,
        kind,
        props,
        configuration,
    };
    let json = render(entry, || (), Path::new("/nonexistent"), &[size], NOW, 1.0).unwrap();
    let mut all: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(all["sizes"][0]["width"], size.0 as f64);
    all["sizes"][0]["root"].take()
}

/// Every text line in paint order, with its frame made absolute.
fn lines(node: &Value, origin: (f64, f64), out: &mut Vec<(String, f64, f64, Value)>) {
    let x = origin.0 + node["x"].as_f64().unwrap_or(0.0);
    let y = origin.1 + node["y"].as_f64().unwrap_or(0.0);
    for line in node["lines"].as_array().into_iter().flatten() {
        out.push((
            line["text"].as_str().unwrap().to_owned(),
            x + line["x"].as_f64().unwrap(),
            y + line["baseline"].as_f64().unwrap(),
            line["runs"][0]["color"].clone(),
        ));
    }
    for child in node["children"].as_array().into_iter().flatten() {
        lines(child, (x, y), out);
    }
}

#[test]
fn shelf_entry_renders_its_props_as_kernel_lines_at_the_given_size() {
    let plan = entry_plan("shelf", "library");
    let root = render_one(&plan, "library", None, None, (158.0, 158.0));
    // A block fills the width it is offered and is as tall as its content.
    assert_eq!(root["w"], 158.0);
    assert!(root["h"].as_f64().unwrap() < 158.0, "{root}");
    let mut found = Vec::new();
    lines(&root, (0.0, 0.0), &mut found);
    let texts: Vec<&str> = found.iter().map(|l| l.0.as_str()).collect();
    assert!(texts.contains(&"7"), "{texts:?}");
    assert!(
        texts.iter().any(|t| t.contains("Dispossessed")),
        "{texts:?}"
    );
    assert!(root.to_string().contains(r#""href":"/stats""#), "{root}");
    for (text, x, baseline, _) in &found {
        assert!((0.0..158.0).contains(x), "{text} at x {x}");
        assert!(
            (0.0..=158.0).contains(baseline),
            "{text} at baseline {baseline}"
        );
    }
    // Lines follow one another down the widget.
    let baselines: Vec<f64> = found.iter().map(|l| l.2).collect();
    assert!(baselines.windows(2).all(|w| w[0] < w[1]), "{baselines:?}");

    let props = br#"{"count": 12, "latest": "Kindred"}"#;
    let root = render_one(&plan, "library", Some(props), None, (158.0, 158.0));
    let mut found = Vec::new();
    lines(&root, (0.0, 0.0), &mut found);
    assert!(found.iter().any(|l| l.0 == "12"), "{found:?}");
    assert!(found.iter().any(|l| l.0.contains("Kindred")), "{found:?}");
}

#[test]
fn a_clamped_line_ends_in_an_ellipsis_after_its_kept_text_only() {
    let plan = entry_plan("shelf", "library");
    let title = "The Left Hand of Darkness and other stories of the Hainish cycle collected";
    let props = format!(r#"{{"count": 3, "latest": "{title}"}}"#);
    let root = render_one(
        &plan,
        "library",
        Some(props.as_bytes()),
        None,
        (158.0, 158.0),
    );
    let tree = root.to_string();
    let mut found = Vec::new();
    collect(&root, &mut found);
    let last = found
        .iter()
        .rev()
        .find(|l| l["text"].as_str().is_some_and(|t| t.ends_with('\u{2026}')))
        .expect(&tree);
    let text = last["text"].as_str().unwrap();
    let kept = text.trim_end_matches('\u{2026}');
    assert!(
        title.contains(kept) && kept.len() < title.len() / 2,
        "{text}"
    );
    // The last run takes the ellipsis: its range ends at the text's end, and
    // its extent covers the line's.
    let runs = last["runs"].as_array().unwrap();
    let end = runs.last().unwrap();
    assert_eq!(
        end["end"].as_u64().unwrap() as usize,
        text.encode_utf16().count(),
        "{last}"
    );
    let right = end["x"].as_f64().unwrap() + end["width"].as_f64().unwrap();
    let line_right = last["x"].as_f64().unwrap() + last["width"].as_f64().unwrap();
    assert!(right >= line_right - 0.5, "{last}");
}

fn collect(node: &Value, out: &mut Vec<Value>) {
    out.extend(node["lines"].as_array().into_iter().flatten().cloned());
    for child in node["children"].as_array().into_iter().flatten() {
        collect(child, out);
    }
}

#[test]
fn caltrain_entry_takes_its_configuration_and_names_its_countdown() {
    let plan = entry_plan("caltrain", "next-trains");
    let one = render_one(
        &plan,
        "next-trains",
        None,
        Some(br#"{"rows":"one"}"#),
        (338.0, 158.0),
    );
    let two = render_one(&plan, "next-trains", None, None, (338.0, 158.0));
    let (mut a, mut b) = (Vec::new(), Vec::new());
    lines(&one, (0.0, 0.0), &mut a);
    lines(&two, (0.0, 0.0), &mut b);
    assert!(b.iter().any(|l| l.0.contains("Mountain View")), "{b:?}");
    assert!(b.iter().any(|l| l.0.contains("San Jose")), "{b:?}");
    assert!(!a.iter().any(|l| l.0.contains("San Jose")), "{a:?}");
    let tree = two.to_string();
    assert!(
        tree.contains("\"time\""),
        "a departure's time is a leaf: {tree}"
    );
}

#[test]
fn a_role_colour_is_carried_unresolved_and_a_fixed_one_as_light_and_dark() {
    let widget = contract::Widget {
        requires_secrets: false,
        kind: "card".into(),
        title: None,
        description: None,
        families: vec!["systemSmall".into()],
        watch_families: vec![],
        component: "Card".into(),
        props: "Props".into(),
        refresh: "refresh".into(),
        preview: serde_json::json!({"title": "Hello"}),
        configuration: None,
    };
    let src = "shape Props\n  title: string\ncomponent Card\n  props\n    title: string\n  view\n    column padding=12 background-color=\"#102030\" border-radius=8\n      text title color=\"CanvasText\" font-weight=600\n      text \"fixed\" color=\"light-dark(#ff0000, #00ff00)\"\n";
    let plan = contract::snapshot::compile(src, &widget).unwrap().encode();
    let root = render_one(&plan, "card", None, None, (158.0, 158.0));
    let mut found = Vec::new();
    lines(&root, (0.0, 0.0), &mut found);
    let hello = found.iter().find(|l| l.0 == "Hello").expect("Hello");
    assert!(hello.3.get("m").is_some(), "a role: {}", hello.3);
    let fixed = found.iter().find(|l| l.0 == "fixed").expect("fixed");
    assert_eq!(fixed.3["l"], i64::from(0xffff0000u32 as i32));
    assert_eq!(fixed.3["d"], i64::from(0xff00ff00u32 as i32));
    let tree = root.to_string();
    assert!(tree.contains("\"radius\":8"), "{tree}");
    assert!(tree.contains("\"weight\":600"), "{tree}");
}

#[test]
fn caltrain_refresh_returns_a_timeline_within_its_budget() {
    let plan = entry_plan("caltrain", "next-trains");
    let entry = Entry {
        plan: &plan,
        kind: "next-trains",
        props: None,
        configuration: None,
    };
    let json = timeline(
        entry,
        caltrain_data::Caltrain,
        None,
        NOW,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    let timeline: Value = serde_json::from_str(&json).unwrap();
    let entries = timeline.as_array().expect("an array of entries");
    assert!(!entries.is_empty(), "{json}");
    assert!(entries[0]["date"].is_number(), "{json}");
    assert!(entries[0]["props"]["departures"].is_array(), "{json}");
}

#[test]
fn an_expired_deadline_refuses_before_booting() {
    let plan = entry_plan("caltrain", "next-trains");
    let entry = Entry {
        plan: &plan,
        kind: "next-trains",
        props: None,
        configuration: None,
    };
    let err = timeline(entry, caltrain_data::Caltrain, None, 0.0, Instant::now()).unwrap_err();
    assert!(err.contains("timed out"), "{err}");
}
