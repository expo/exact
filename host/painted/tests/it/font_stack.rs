use exact_painted::{presenter::PainterChoice, Presenter};
use std::path::Path;

const SECOND: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");

fn boot(family: &str) -> Presenter<()> {
    let source = format!(
        r##"component App
  view
    column width="100%" height="100%" background-color="#ffffff" align-items="flex-start"
      text "éé" font-family="{family}" font-size=32 line-height="48px" testId="sample"
"##
    );
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut builder =
        exact_plan::builder::PlanBuilder::from_plan(contract::compile(&source).unwrap());
    let mut declared = Vec::new();
    for (name, file) in [
        ("Primary", "RobotoFlex-subset.ttf"),
        ("Fallback", "Inter-Regular.ttf"),
    ] {
        let source = format!("fonts/{file}");
        let id = builder.font_family(
            &format!("Fixture{name}"),
            &[exact_plan::builder::Face {
                source: &source,
                weight: [400, 400],
                stretch: [1000, 1000],
                italic: false,
            }],
        );
        declared.push((name, id));
    }
    let mut plan = builder.finish().unwrap();
    for (name, id) in declared {
        let faces = plan.familie(id).faces;
        if let Some(i) = plan.families.iter().position(|f| plan.str(f.name) == name) {
            plan.families[i].faces = faces;
        }
    }
    let (p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (160., 72.),
        1.,
        assets,
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn sample(p: &Presenter<()>) -> &exact_painted::text::Paragraph {
    let kernel = p.host().kernel();
    let id = kernel
        .node_by_key(kernel.find_by_test_id("sample")[0])
        .unwrap()
        .id;
    p.paragraph(id).unwrap()
}

#[test]
fn headless_app_draws_the_authored_glyph_fallback() {
    let mut actual = boot("Primary, Fallback");
    let mut expected = boot("Fallback");
    let actual_frame = actual.frame();
    let expected_frame = expected.frame();
    let p = sample(&actual);
    let q = sample(&expected);
    assert_eq!(p.width, q.width, "fallback advances reach layout");
    let glyphs = p.layout_runs().flat_map(|r| r.glyphs).collect::<Vec<_>>();
    assert_eq!(glyphs.len(), 2);
    for glyph in glyphs {
        assert!(p.lines().faces[glyph.face as usize].font.data.data() == SECOND);
        assert_ne!(glyph.glyph_id, 0);
    }
    assert!(actual_frame.pixels().iter().any(|p| p.red() < 128));
    assert!(
        actual_frame.data() == expected_frame.data(),
        "fallback pixels reach the frame"
    );
}
