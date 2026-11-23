//! Unsaid type under the Material look is Material's type scale (LLP 1115.001
//! D4): a text style's size, line height and weight are its role's.
use super::material_tests::{id, material_process};
use super::*;
use std::fmt::Write;

#[test]
fn materials_text_styles_are_its_type_scale() {
    if !material_process() {
        return;
    }
    let styles = [
        ("large-title", 36., 44., 400),
        ("title1", 28., 36., 400),
        ("title2", 24., 32., 400),
        ("title3", 22., 28., 400),
        ("headline", 16., 24., 500),
        ("body", 16., 24., 400),
        ("callout", 16., 24., 400),
        ("subheadline", 14., 20., 400),
        ("footnote", 12., 16., 400),
        ("caption1", 12., 16., 500),
        ("caption2", 11., 16., 500),
    ];
    let mut source = String::from("component App\n  view\n    column align-items=\"flex-start\"\n");
    for (name, ..) in styles {
        writeln!(
            source,
            r#"      text "{name}" font="-exact-{name}" testId="{name}""#
        )
        .unwrap();
    }
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (400., 600.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let mut differences = Vec::new();
    for scale in [1., 1.25] {
        assert_eq!(p.set_text_scale(scale), None);
        let root = p.host().kernel().root_font_size();
        assert_eq!(root, 16. * scale);
        for (name, size, line, weight) in styles {
            let node = id(&p, name);
            let style = p.host().kernel().node(node).unwrap().text_style();
            let height = p.rect_of(node).unwrap().3;
            let expected = (size * scale, line * scale, weight);
            let actual = (style.font_size, height, style.font_weight);
            if actual.0 == expected.0
                && (actual.1 - expected.1).abs() < 0.01
                && actual.2 == expected.2
            {
                continue;
            }
            differences.push(format!(
                "{name} at root {root}: expected {expected:?}, got {actual:?}"
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

#[test]
fn material_type_bindings_and_text_share_the_styles_line_height() {
    if !material_process() {
        return;
    }
    let dir = crate::TestDir::new("exact-painted-material-type");
    std::fs::write(
        dir.join("app.forma"),
        "forma t\n\ntheme\n  type.label = -exact-body system-ui\n  type.normal = -exact-body/normal system-ui\n",
    )
    .unwrap();
    let plan = contract::compile_path_source(
        &dir.join("app.contract"),
        r#"component App
  view
    column
      text "Label" font="type.label" testId="label"
      text "Normal" font="type.normal"
"#,
    )
    .unwrap()
    .encode();
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &plan,
        (),
        (400., 600.),
        2.4375,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let label = id(&p, "label");
    let (_, _, _, height) = p.rect_of(label).unwrap();
    let mut forma = p.host_mut().runner_mut().forma();
    let font = forma.font("type.label", exact_runner::Place::Root).unwrap();
    assert_eq!(font.line_height, Some(f64::from(height)));
    assert_eq!(font.line_height, Some(24.));
    let normal = forma
        .font("type.normal", exact_runner::Place::Root)
        .unwrap();
    assert_eq!(normal.line_height, None);
}
