use super::*;

#[test]
fn painted_alerts_use_only_the_dialogs_written_family_and_fit_its_lines() {
    let source = r#"style Nastaliq
  font-family="Noto Nastaliq Urdu"
component App
  view
    main font-family="Noto Nastaliq Urdu" direction="rtl"
      dialog id="inherited" role="alertdialog" aria-label="نسیم"
        text "نسیم"
        button commandfor="inherited" command="close"
          text "نسیم"
      dialog id="own" role="alertdialog" aria-label="نسیم" font-family="Noto Nastaliq Urdu"
        text "نسیم"
        button commandfor="own" command="close"
          text "نسیم"
      dialog id="class" role="alertdialog" aria-label="نسیم" class=Nastaliq
        text "نسیم"
        button commandfor="class" command="close"
          text "نسیم"
"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let plan = contract::compile(source).unwrap();
    let mut engine = TextEngine::from_font_files(
        &[
            &root.join("tests/fonts/NotoSansArabic.ttf"),
            &root.join("tests/fonts/NotoNastaliqUrdu.ttf"),
        ],
        "Noto Sans Arabic",
    );
    engine.install_runtime(8, &["Noto Nastaliq Urdu".into()]);
    let runner = exact_runner::Runner::boot(
        plan,
        (),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let kernel = runner.kernel();
    let inputs = Inputs {
        direction: Direction::Rtl,
        dark: false,
        contrast: None,
        font_scale: 1.,
        bold_text: false,
        window: (400., 800.),
        room: 792.,
    };
    for desktop in [false, true] {
        let mut heights = Vec::new();
        for id in ["inherited", "own", "class"] {
            let node = kernel.node_by_key(kernel.find_by_id(id)[0]).unwrap();
            let plan =
                crate::presenter::top::shape::alert(kernel, node.id, &|_| false, true).unwrap();
            let layout = if desktop {
                Layout::desktop(&mut engine, &plan, node.id, &inputs)
            } else {
                Layout::of(&mut engine, &plan, node.id, &inputs)
            };
            let family = node.computed_row(exact_kernel::StyleId::FontFamily, |s| s.font_family);
            assert_eq!(family, 8);
            let face = engine.resolved_face_id(family, 400, false).unwrap();
            let texts: Vec<_> = layout
                .items
                .iter()
                .chain(layout.windows.iter().flat_map(|content| &content.texts))
                .filter_map(|item| match item {
                    Item::Text(p, at, ..) => Some((p, at)),
                    _ => None,
                })
                .collect();
            assert_eq!(texts.len(), 3);
            for (p, at) in &texts {
                let glyph = p.lines().glyphs.first().expect("an alert label has glyphs");
                assert!(
                    p.lines()
                        .faces
                        .iter()
                        .all(|f| (f.id() == face) == (id != "inherited")),
                    "{id}, desktop {desktop}: title, message and buttons must use the same face"
                );
                assert!(at.1 >= 0. && at.1 + p.height <= layout.size.1);
                if id != "inherited" {
                    let style = StyleProps::default().with_rows(|s| {
                        s.font_family = family;
                        s.font_size = glyph.font_size;
                        s.font_weight = p.lines().faces[usize::from(glyph.face)].weight;
                    });
                    let spec = crate::paint::text_spec(&style, "نسیم");
                    assert!(p.height >= engine.normal_line_height(&spec.runs[0]));
                }
            }
            let button = layout.targets[0].rect;
            let (label, at) = texts.iter().find(|(_, at)| at.1 >= button.1).unwrap();
            assert!(at.1 + label.height <= button.1 + button.3);
            heights.push((layout.size.1, button.3));
        }
        assert!(heights[1].0 > heights[0].0 && heights[1].1 > heights[0].1);
        assert_eq!(heights[1], heights[2]);
    }
}
