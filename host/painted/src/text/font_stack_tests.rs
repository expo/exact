use super::*;
use exact_kernel::StyleProps;

const FIRST: &[u8] = include_bytes!("../../tests/fonts/RobotoFlex-subset.ttf");
const SECOND: &[u8] = include_bytes!("../../tests/fonts/Inter-Regular.ttf");
const PLATFORM: &[u8] = include_bytes!("../../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf");
const ARABIC: &[u8] = include_bytes!("../../tests/fonts/NotoSansArabic.ttf");

fn fixture() -> catalog::Catalog {
    catalog::Catalog::from_bytes(&[FIRST, SECOND, PLATFORM, ARABIC], "DejaVu Sans")
}

fn spec(family: u16, text: &str) -> Spec {
    crate::paint::text_spec(
        &StyleProps {
            font_family: family,
            font_size: 32.,
            line_height: exact_kernel::LineHeight::Length(48.),
            ..Default::default()
        },
        text,
    )
}

fn assert_face(p: &Paragraph, start: u32, bytes: &[u8]) {
    let glyph = p
        .layout_runs()
        .flat_map(|r| r.glyphs)
        .find(|g| g.start == start)
        .unwrap();
    assert_ne!(glyph.glyph_id, 0);
    assert!(
        p.lines().faces[glyph.face as usize].font.data.data() == bytes,
        "wrong face at byte {start}"
    );
}

fn pixels(engine: &mut TextEngine, p: &Paragraph) -> Vec<u8> {
    let mut pixmap = Pixmap::new(160, 72).unwrap();
    engine.paint(
        &mut pixmap,
        p,
        &[RunPaint {
            color: [0, 0, 0, 255],
            source: 0,
        }],
        (4., 50. - p.first_baseline),
        1.,
        Transform::identity(),
        None,
    );
    assert!(pixmap.pixels().iter().any(|p| p.alpha() != 0));
    pixmap.data().to_vec()
}

fn assert_fallback(
    engine: &mut TextEngine,
    family: u16,
    reference: u16,
    bytes: &[u8],
    weight: u16,
    italic: bool,
) {
    let mut actual = spec(family, "é");
    actual.strut.weight = weight;
    actual.strut.italic = italic;
    actual.runs[0].weight = weight;
    actual.runs[0].italic = italic;
    let mut expected = actual.clone();
    expected.strut.family = reference;
    expected.runs[0].family = reference;
    let p = engine.paragraph(&actual, None);
    let q = engine.paragraph(&expected, None);
    assert_face(&p, 0, bytes);
    assert_eq!(p.width, q.width, "fallback advance");
    let glyphs = |p: &Paragraph| {
        p.layout_runs()
            .flat_map(|r| r.glyphs)
            .map(|g| {
                let face = &p.lines().faces[g.face as usize];
                (g.glyph_id, g.w.to_bits(), face.coords.clone(), face.skew)
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(glyphs(&p), glyphs(&q));
    assert_eq!(pixels(engine, &p), pixels(engine, &q), "fallback raster");
}

#[test]
fn runtime_stack_uses_second_authored_face_before_platform_fallback() {
    for (bytes, covered) in [(FIRST, false), (SECOND, true), (PLATFORM, true)] {
        assert_eq!(
            swash::FontRef::from_index(bytes, 0)
                .unwrap()
                .charmap()
                .map('é')
                != 0,
            covered
        );
    }
    let mut catalog = fixture();
    catalog.install_runtime(
        8,
        &[
            "Missing, 'monospace', Roboto Flex, Inter".into(),
            "Inter".into(),
            "Roboto Flex, DejaVu Sans".into(),
        ],
    );
    let recipe = catalog_recipe::CatalogSnapshot::capture(&catalog)
        .unwrap()
        .prepare()
        .unwrap();
    for catalog in [catalog.with_locale("ar"), recipe.catalog(), catalog] {
        let mut engine = TextEngine::with_catalog(catalog);
        let p = engine.paragraph(&spec(8, "Aéش"), None);
        for (start, bytes) in [(0, FIRST), (1, SECOND), (3, ARABIC)] {
            assert_face(&p, start, bytes);
        }
        assert_fallback(&mut engine, 8, 9, SECOND, 400, false);
        let mut inline = spec(8, "é");
        let mut tail = inline.runs[0].clone();
        tail.family = 10;
        inline.runs.push(tail);
        let p = engine.paragraph(&inline, None);
        assert_face(&p, 0, SECOND);
        assert_face(&p, 2, PLATFORM);
    }
}

#[test]
fn picture_stack_keeps_families_after_a_generic() {
    let mut catalog = fixture();
    let first = catalog.fonts.collection.family_id("Roboto Flex").unwrap();
    catalog
        .fonts
        .collection
        .set_generic_families(fontique::GenericFamily::Monospace, [first].into_iter());
    catalog.install_picture(&[
        vec![(false, "monospace".into()), (true, "Inter".into())],
        vec![(true, "Inter".into())],
    ]);
    let mut engine = TextEngine::with_catalog(catalog);
    let p = engine.paragraph(&spec(8, "Aéش"), None);
    for (start, bytes) in [(0, FIRST), (1, SECOND), (3, ARABIC)] {
        assert_face(&p, start, bytes);
    }
    assert_fallback(&mut engine, 8, 9, SECOND, 400, false);
}

#[test]
fn compiled_installed_stack_keeps_order_and_platform_fallback() {
    let plan = contract::compile(
        "component App\n  view\n    text \"Aéش\" font-family=\"Missing, Roboto Flex, Inter\"\n",
    )
    .unwrap();
    let family = plan.stacks.len() as u16 - 1;
    let assets = Assets::embedded(Path::new("").into());
    let mut engine = TextEngine::with_catalog(fixture().with_assets(&plan, &assets));
    let p = engine.paragraph(&spec(family, "Aéش"), None);
    for (start, bytes) in [(0, FIRST), (1, SECOND), (3, ARABIC)] {
        assert_face(&p, start, bytes);
    }
}

const BOLD: &[u8] = include_bytes!("../../../../scripts/fixtures/fonts/assets/DejaVuSans-Bold.ttf");
const FIRST_PATH: &str = "host/painted/tests/fonts/RobotoFlex-subset.ttf";
const SECOND_PATH: &str = "host/painted/tests/fonts/Inter-Regular.ttf";
const BOLD_PATH: &str = "scripts/fixtures/fonts/assets/DejaVuSans-Bold.ttf";

#[test]
fn bundled_stacks_keep_each_familys_descriptors_and_raster() {
    use exact_plan::builder::{Face, PlanBuilder};
    let plan = contract::compile("component App\n  view\n    text \"fixture\"\n").unwrap();
    let mut builder = PlanBuilder::from_plan(plan);
    let first = builder.font_family(
        "Primary",
        &[Face {
            source: FIRST_PATH,
            weight: [100, 1000],
            stretch: [250, 1510],
            italic: false,
        }],
    );
    let second = builder.font_family(
        "Fallback",
        &[
            Face {
                source: SECOND_PATH,
                weight: [400, 400],
                stretch: [1000, 1000],
                italic: false,
            },
            Face {
                source: BOLD_PATH,
                weight: [700, 700],
                stretch: [1000, 1000],
                italic: false,
            },
            Face {
                source: SECOND_PATH,
                weight: [400, 400],
                stretch: [1000, 1000],
                italic: true,
            },
        ],
    );
    let family = builder
        .font_stack(&[
            (StackMemberKind::Family, Some(first)),
            (StackMemberKind::Family, Some(second)),
        ])
        .0 as u16;
    let reference = builder
        .font_stack(&[(StackMemberKind::Family, Some(second))])
        .0 as u16;
    let plan = builder.finish().unwrap();
    let memory = Assets::selected(
        Path::new("").into(),
        Arc::new(|name| {
            Ok(match name {
                FIRST_PATH => Some(Arc::from(FIRST)),
                SECOND_PATH => Some(Arc::from(SECOND)),
                BOLD_PATH => Some(Arc::from(BOLD)),
                _ => None,
            })
        }),
    );
    let mapped = Assets::embedded(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    for assets in [memory, mapped] {
        let mut engine = TextEngine::with_catalog(fixture().with_assets(&plan, &assets));
        let p = engine.paragraph(&spec(family, "Aé"), None);
        assert_face(&p, 0, FIRST);
        assert_face(&p, 1, SECOND);
        for (weight, italic, bytes) in [
            (400, false, SECOND),
            (700, false, BOLD),
            (700, true, SECOND),
        ] {
            assert_fallback(&mut engine, family, reference, bytes, weight, italic);
        }
        let canvas = crate::canvas2d::text::CanvasText::new(plan.clone(), assets);
        let mut style = crate::canvas2d::text::RunStyle {
            size: 32.,
            weight: 400,
            style: 0,
            stretch: 100.,
            kerning: 0,
            letter_spacing: 0.,
            word_spacing: 0.,
            families: vec!["Primary".into(), "Fallback".into()],
        };
        let actual = canvas.shape(&style, "é", false);
        style.families = vec!["Fallback".into()];
        let expected = canvas.shape(&style, "é", false);
        assert_eq!(actual.width, expected.width);
        assert!(actual.path.is_some());
        assert_eq!(actual.path, expected.path);
    }
}
