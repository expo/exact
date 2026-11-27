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
        &StyleProps::default().with_rows(|rows| {
            rows.font_family = family;
            rows.font_size = 32.;
            rows.line_height = exact_kernel::LineHeight::Length(48.);
        }),
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
fn emoji_presentation_selects_a_face_for_the_whole_grapheme() {
    const EMOJI: &[u8] = include_bytes!("../../tests/fonts/NotoColorEmoji-subset.ttf");
    let catalog = catalog::Catalog::from_bytes(&[FIRST, PLATFORM, EMOJI], "Roboto Flex");
    let mut engine = TextEngine::with_catalog(catalog);
    for (text, face) in [
        ("↗", PLATFORM),
        ("↗\u{fe0e}", PLATFORM),
        ("↗\u{fe0f}", EMOJI),
        ("↔", PLATFORM),
        ("✌", EMOJI), // The fixture has no text face for the victory hand.
        ("✌\u{fe0f}", EMOJI),
        ("✌🏽", EMOJI),
        ("1", PLATFORM),
        ("1\u{fe0e}", PLATFORM),
        ("1\u{fe0f}", EMOJI),
        ("1\u{20e3}", EMOJI),
        ("1\u{fe0f}\u{20e3}", EMOJI),
        ("☕", EMOJI),
        ("☕\u{fe0e}", PLATFORM),
        ("🇺🇸", EMOJI),
        ("👍🏽", EMOJI),
    ] {
        let p = engine.paragraph(&spec(0, text), None);
        let p_face = &p.lines().faces[p.lines().glyphs[0].face as usize];
        assert!(p_face.font.data.data() == face, "{text:?}");
        assert_ne!(p.lines().glyphs[0].glyph_id, 0, "{text:?}");
        // An unsupported selector can leave an invisible, zero-advance glyph.
        assert!(
            p.lines().glyphs.iter().skip(1).all(|g| g.w == 0.),
            "{text:?}"
        );
    }
}

#[test]
fn a_mixed_face_keeps_its_text_arrows_and_shapes_its_explicit_variant() {
    const MIXED: &[u8] = include_bytes!("../../tests/fonts/MixedPresentation.ttf");
    const EMOJI: &[u8] = include_bytes!("../../tests/fonts/NotoColorEmoji-subset.ttf");
    let mut catalog = catalog::Catalog::from_bytes(&[MIXED, EMOJI, PLATFORM], "Mixed Presentation");
    catalog.install_runtime(8, &["Mixed Presentation".into()]);
    catalog.install_runtime(9, &["Noto Color Emoji".into()]);
    let mut engine = TextEngine::with_catalog(catalog);
    let authored = engine.paragraph(&spec(9, "↗"), None);
    assert_face(&authored, 0, EMOJI);
    for (text, face) in [("↗", MIXED), ("↗\u{fe0f}", EMOJI), ("↔\u{fe0e}", MIXED)] {
        let p = engine.paragraph(&spec(8, text), None);
        assert_face(&p, 0, face);
        if text.ends_with('\u{fe0e}') {
            assert_eq!(p.lines().glyphs[0].glyph_id, 3);
        }
    }
    for text in ["1\u{fe0e}\u{20e3}", "*\u{fe0e}\u{20e3}"] {
        let p = engine.paragraph(&spec(0, text), None);
        assert_face(&p, 0, PLATFORM);
        assert!(p.lines().glyphs.iter().any(|g| g.start == 4));
    }
    let joined = "🌈\u{200d}↗\u{fe0e}";
    let p = engine.paragraph(&spec(8, joined), None);
    assert_face(&p, 0, EMOJI);
    assert_face(&p, "🌈\u{200d}".len() as u32, MIXED);
}

#[test]
fn fallback_reaches_each_subset_of_the_matching_style() {
    let mut collection = fonts::empty_collection();
    for bytes in [SECOND, PLATFORM] {
        collection.register_fonts(
            fontique::Blob::new(Arc::new(bytes.to_vec())),
            Some(fontique::FontInfoOverride {
                family_name: Some("Symbols"),
                ..Default::default()
            }),
        );
    }
    let catalog = catalog::Catalog::with_collection(collection, Arc::default(), "en");
    let mut engine = TextEngine::with_catalog(catalog);
    let p = engine.paragraph(&spec(0, "⁴↝"), None);
    assert_face(&p, 0, SECOND);
    assert_face(&p, "⁴".len() as u32, PLATFORM);
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
fn a_pair_without_a_composed_character_checks_both_glyphs_for_fallback() {
    let mut catalog = fixture();
    catalog.install_runtime(8, &["Noto Sans Arabic, Inter".into()]);
    let mut engine = TextEngine::with_catalog(catalog);

    // Noto Sans Arabic covers the digit, but neither the Latin letter nor the acute.
    for text in ["1\u{0301}", "b\u{0301}"] {
        let p = engine.paragraph(&spec(8, text), None);
        assert_face(&p, 0, SECOND);
        assert!(p.lines().glyphs.iter().all(|g| g.glyph_id != 0));
    }
}

#[test]
fn long_clusters_keep_their_fallback_face_glyphs_and_byte_ranges() {
    let mut catalog = fixture();
    catalog.install_runtime(8, &["Noto Sans Arabic, Inter".into(), "Inter".into()]);
    let mut engine = TextEngine::with_catalog(catalog);

    // Both faces cover the digit and the combining grapheme joiner. Only Inter covers the acute.
    let prefix = format!("1{}", "\u{034f}".repeat(40));
    assert_face(&engine.paragraph(&spec(8, &prefix), None), 0, ARABIC);
    assert_face(
        &engine.paragraph(&spec(8, &(prefix + "\u{0301}")), None),
        0,
        SECOND,
    );

    for marks in [253, 254, 255, 256] {
        let cluster = format!("b{}", "\u{0301}".repeat(marks));
        let p = engine.paragraph(&spec(9, &format!("{cluster}!")), None);
        let glyphs = &p.lines().glyphs;
        assert_eq!(glyphs.len(), marks + 2);
        assert!(glyphs.iter().all(|g| g.glyph_id != 0));
        assert_eq!(glyphs.last().unwrap().start as usize, cluster.len());
    }

    let arrows = "→".repeat(21_846);
    let p = engine.paragraph(&spec(9, &format!("{arrows}!")), None);
    let end = p.lines().glyphs.last().unwrap();
    assert_eq!(
        (end.start as usize, end.end as usize),
        (arrows.len(), arrows.len() + 1)
    );
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

#[test]
fn normal_metrics_follow_font_stretch_in_either_query_order() {
    use exact_plan::builder::{Face, PlanBuilder};
    use exact_plan::StackMemberKind;
    let mut builder = PlanBuilder::from_plan(
        contract::compile("component App\n  view\n    text \"fixture\"\n").unwrap(),
    );
    let family = builder.font_family(
        "WidthFaces",
        &[
            Face {
                source: "narrow",
                weight: [400, 400],
                stretch: [750, 750],
                italic: false,
            },
            Face {
                source: "wide",
                weight: [400, 400],
                stretch: [1000, 1000],
                italic: false,
            },
        ],
    );
    let family = builder
        .font_stack(&[(StackMemberKind::Family, Some(family))])
        .0 as u16;
    let plan = builder.finish().unwrap();
    let assets = Assets::selected(
        Path::new("").into(),
        Arc::new(|name| {
            Ok(match name {
                "narrow" => Some(Arc::from(ARABIC)),
                "wide" => Some(Arc::from(PLATFORM)),
                _ => None,
            })
        }),
    );
    for widths in [[75., 100.], [100., 75.]] {
        let mut engine = TextEngine::with_catalog(fixture().with_assets(&plan, &assets));
        for width in widths {
            let mut s = spec(family, "بيت");
            s.strut.stretch = width;
            let expected = if width == 75. {
                32. * 2.112
            } else {
                32. * 2384. / 2048.
            };
            assert!(
                (engine.normal_line_height(&s.strut) - expected).abs() < 0.001,
                "width {width}"
            );
        }
    }
}

#[test]
fn svg_family_ids_resolve_in_the_pictures_own_catalog() {
    let loaded = exact_svg_file::load(r#"<svg><text font-family="MissingExactFont, monospace">Mono</text><text font-family="serif">Serif</text></svg>"#).unwrap();
    let scene = loaded.scene(200.0, 40.0);
    let mut catalog = fixture();
    fonts::set_generic(
        &mut catalog.fonts.collection,
        fontique::GenericFamily::Monospace,
        "Roboto Flex",
    );
    fonts::set_generic(
        &mut catalog.fonts.collection,
        fontique::GenericFamily::Serif,
        "Inter",
    );
    let mut engine = TextEngine::with_catalog(catalog);
    engine.install_runtime(9, &["Inter".into()]);
    let text = engine.for_svg(&loaded.document.fonts);
    let mut text = text.borrow_mut();
    for (item, generic) in scene.items.iter().zip([6, 4]) {
        let exact_kernel::svg::scene::Kind::Text(item) = &item.kind else {
            panic!("text")
        };
        let run = &item.chunks[0].runs[0];
        let face = text
            .resolved_face_id(run.style.font_family, 400, false)
            .expect("fixture generic face");
        assert_eq!(Some(face), text.resolved_face_id(generic, 400, false));
    }
}

#[test]
fn android_resolves_text_default_symbols_by_file_and_explicit_presentation() {
    const EARLY: &[u8] = include_bytes!("../../tests/fonts/MixedPresentation.ttf");
    const LATE: &[u8] = include_bytes!("../../tests/fonts/MixedPresentation-hand.ttf");
    const EMOJI: &[u8] = include_bytes!("../../tests/fonts/NotoColorEmoji-subset.ttf");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts");
    let paths = [
        "RobotoFlex-subset.ttf",
        "MixedPresentation.ttf",
        "NotoColorEmoji-subset.ttf",
        "MixedPresentation-hand.ttf",
    ]
    .map(|name| dir.join(name));
    let xml = r#"<familyset>
      <family><font>RobotoFlex-subset.ttf</font><font>MixedPresentation.ttf</font></family>
      <family lang="und-Zsye"><font>NotoColorEmoji-subset.ttf</font><font fallbackFor="serif">MixedPresentation-hand.ttf</font></family>
      <family lang="und-Zsym"><font>MixedPresentation-hand.ttf</font></family>
    </familyset>"#;
    for (locale, arrow, hand) in [
        ("", EARLY, LATE),
        ("en", EARLY, EMOJI),
        ("en-u-em-text", EARLY, LATE),
        ("en-u-em-emoji", EMOJI, EMOJI),
    ] {
        let mut c =
            catalog::Catalog::from_files(&paths.each_ref().map(|p| p.as_path()), "Roboto Flex");
        fonts::set_generic(
            &mut c.fonts.collection,
            fontique::GenericFamily::Serif,
            "Roboto Flex",
        );
        if !locale.is_empty() {
            let order =
                android_symbols::Families::read(&mut c.fonts.collection, &c.files, &dir, xml)
                    .unwrap();
            Arc::new(order).configure(&mut c.fonts.collection, locale);
        }
        let mut engine = TextEngine::with_catalog(c);
        if locale == "en" {
            assert_face(&engine.paragraph(&spec(4, "✌"), None), 0, LATE);
        }
        for (text, face) in [
            ("↗", arrow),
            ("↔", arrow),
            ("✌", hand),
            ("✌︎", LATE),
            ("✌️", EMOJI),
        ] {
            assert_face(&engine.paragraph(&spec(0, text), None), 0, face);
        }
    }
}
