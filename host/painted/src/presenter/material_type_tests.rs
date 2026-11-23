//! Unsaid type under the Material look is Material's type scale (LLP 1115.001
//! D4): a text style's size, line height and weight are its role's.
use super::material_tests::{id, material_process};
use super::*;

#[test]
fn materials_text_styles_are_its_type_scale() {
    if !material_process() {
        return;
    }
    let plan = crate::test_plan(
        r##"component App
  view
    column align-items="flex-start"
      text "Body" font="-exact-body" testId="body"
      text "Title" font="-exact-title2" testId="title"
      text "Headline" font="-exact-headline" testId="headline"
"##,
    );
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
    // bodyLarge 16/24, headlineSmall 24/32, titleMedium 16/24 in medium.
    for (name, size, line, weight) in [
        ("body", 16.0, 24.0, 400),
        ("title", 24.0, 32.0, 400),
        ("headline", 16.0, 24.0, 500),
    ] {
        let node = id(&p, name);
        let style = p.host().kernel().node(node).unwrap().text_style();
        assert_eq!(
            (style.font_size, style.font_weight),
            (size, weight),
            "{name}"
        );
        let height = p.rect_of(node).unwrap().3;
        assert!((height - line).abs() < 0.01, "{name}: {height}");
    }
}
