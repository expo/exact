use crate::load;
use exact_kernel::svg::scene::{Item, Kind, TextRun};
use exact_kernel::FontStyle;

fn runs(body: &str) -> Vec<TextRun> {
    let loaded = load(&format!("<svg viewBox=\"0 0 200 40\">{body}</svg>")).unwrap();
    let scene = loaded.scene(200.0, 40.0);
    fn collect(items: Vec<Item>, out: &mut Vec<TextRun>) {
        for item in items {
            match item.kind {
                Kind::Text(text) => out.extend(text.chunks.into_iter().flat_map(|c| c.runs)),
                Kind::Group(children) | Kind::Viewport { children, .. } => collect(children, out),
                _ => panic!("expected text"),
            }
        }
    }
    let mut out = Vec::new();
    collect(scene.items, &mut out);
    out
}

#[test]
fn system_language_without_user_languages_selects_the_fallback() {
    let runs = runs(
        r#"<switch>
            <text systemLanguage="de" y="20">Hallo</text>
            <text systemLanguage="fr" y="20">Bonjour</text>
            <text y="20">Hello</text>
        </switch>"#,
    );
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        "Hello"
    );
}

#[test]
fn system_language_without_user_languages_hides_elements_outside_switch() {
    let runs = runs(
        r#"<text systemLanguage="fr">Bonjour</text>
        <g systemLanguage="de"><text>Hallo</text></g>
        <text>Hello<tspan systemLanguage="fr"> monde</tspan></text>"#,
    );
    assert_eq!(
        runs.iter().map(|run| run.text.as_str()).collect::<String>(),
        "Hello"
    );
}

#[test]
fn mixed_text_keeps_spans_and_spaces_in_document_order() {
    for (body, expected, weights) in [
        (
            "<text>Price: <tspan font-weight=\"700\">42</tspan> dollars</text>",
            "Price: 42 dollars",
            vec![400, 700, 400],
        ),
        (
            "<text><tspan>Hello <tspan font-weight=\"700\">big</tspan> world</tspan></text>",
            "Hello big world",
            vec![400, 700, 400],
        ),
        (
            "<text><tspan>BTC </tspan><tspan font-weight=\"700\">+4.88%</tspan></text>",
            "BTC +4.88%",
            vec![400, 700],
        ),
        (
            "<text><tspan>A</tspan> <tspan>B</tspan></text>",
            "A B",
            vec![400, 400, 400],
        ),
    ] {
        let runs = runs(body);
        assert_eq!(
            runs.iter().map(|r| r.text.as_str()).collect::<String>(),
            expected
        );
        assert_eq!(
            runs.iter().map(|r| r.style.font_weight).collect::<Vec<_>>(),
            weights
        );
    }
}

#[test]
fn formatting_space_collapses_across_runs_and_trims_only_chunk_edges() {
    let runs = runs("<text> \n<tspan> A  </tspan> \n<tspan>B </tspan> \n</text>");
    assert_eq!(
        runs.iter().map(|r| r.text.as_str()).collect::<String>(),
        "A B"
    );
}

#[test]
fn svg_font_weight_keywords_use_css_weights() {
    let runs = runs("<text font-weight=\"bold\"><tspan>A</tspan><tspan font-weight=\"normal\">B</tspan><tspan font-weight=\"bolder\">C</tspan><tspan font-weight=\"lighter\">D</tspan></text>");
    assert_eq!(
        runs.iter().map(|r| r.style.font_weight).collect::<Vec<_>>(),
        [700, 400, 900, 400]
    );
}

#[test]
fn svg_font_size_accepts_absolute_units() {
    for size in ["9pt", "0.75pc", "0.125in", "0.3175cm", "3.175mm", "12.7Q"] {
        let runs = runs(&format!("<text font-size=\"{size}\">x</text>"));
        assert!(
            (runs[0].style.font_size - 12.0).abs() < 1e-4,
            "{size}: {:?}",
            runs[0].style
        );
    }
}

#[test]
fn svg_oblique_uses_the_hosts_italic_face() {
    assert_eq!(
        runs("<text font-style=\"oblique\">x</text>")[0]
            .style
            .font_style,
        FontStyle::Italic
    );
}

#[test]
fn svg_opacities_accept_percentages() {
    let loaded = load("<svg><text opacity=\"50%\" fill-opacity=\"50%\" stroke=\"red\" stroke-opacity=\"50%\">x</text></svg>").unwrap();
    let scene = loaded.scene(200.0, 40.0);
    assert_eq!(scene.items[0].opacity, 0.5);
    let Kind::Text(text) = &scene.items[0].kind else {
        panic!("expected text")
    };
    let run = &text.chunks[0].runs[0];
    assert_eq!(run.fill.as_ref().unwrap().opacity, 0.5);
    assert_eq!(run.stroke.as_ref().unwrap().opacity, 0.5);
}

#[test]
fn svg_font_families_are_accepted() {
    let loaded = load("<svg><text style=\"font-weight:bold;font-family:Arial,Helvetica,sans-serif;font-size:9pt\">x</text></svg>").unwrap();
    assert!(
        loaded.document.warnings.is_empty(),
        "{:?}",
        loaded.document.warnings
    );
}

#[test]
fn svg_family_stacks_inherit_and_keep_quoted_names() {
    let loaded = load(
        r#"<svg><style>text {font-family: Arial, Helvetica, sans-serif}</style>
      <text><tspan>A</tspan><tspan font-family="'A, B', 'serif', monospace">B</tspan>
      <tspan style="font-family:Arial,Helvetica,sans-serif">C</tspan></text></svg>"#,
    )
    .unwrap();
    assert!(
        loaded.document.warnings.is_empty(),
        "{:?}",
        loaded.document.warnings
    );
    assert_eq!(
        loaded.document.fonts,
        [
            vec![
                (true, "Arial".into()),
                (true, "Helvetica".into()),
                (false, "sans-serif".into())
            ],
            vec![
                (true, "A, B".into()),
                (true, "serif".into()),
                (false, "monospace".into())
            ],
        ]
    );
    let scene = loaded.scene(200.0, 40.0);
    let Kind::Text(text) = &scene.items[0].kind else {
        panic!("expected text")
    };
    let families: Vec<_> = text.chunks[0]
        .runs
        .iter()
        .filter(|r| !r.text.trim().is_empty())
        .map(|r| r.style.font_family)
        .collect();
    assert_eq!(families, [8, 9, 8]);
}

#[test]
fn invalid_family_lists_leave_the_inherited_family() {
    let runs = runs(
        r#"<text font-family="serif"><tspan font-family="Arial;">A</tspan><tspan font-family="serif,,monospace">B</tspan><tspan font-family="9">C</tspan></text>"#,
    );
    assert_eq!(
        runs.iter().map(|r| r.style.font_family).collect::<Vec<_>>(),
        [8, 8, 8]
    );
}

#[test]
fn svg_relative_font_sizes_keep_their_inherited_basis() {
    for (parent, expected) in [(10, 20.0), (24, 48.0)] {
        let runs = runs(&format!(
            r#"<text font-size="{parent}"><tspan font-size="2em">x</tspan></text>"#
        ));
        assert_eq!(runs[0].style.font_size, expected);
    }
}

#[test]
fn svg_relative_weights_resolve_in_each_use_instance() {
    let runs = runs(
        r##"<defs font-weight="100">
      <text id="t" font-weight="bolder">A<tspan>B</tspan><tspan font-weight="lighter">C</tspan></text>
    </defs><use href="#t" font-weight="400"/><use href="#t" font-weight="700"/>"##,
    );
    assert_eq!(
        runs.iter().map(|r| r.style.font_weight).collect::<Vec<_>>(),
        [700, 700, 400, 900, 900, 700]
    );
}

#[test]
fn relative_weight_in_a_pattern_is_computed_once() {
    let loaded = load(r##"<svg><defs><pattern id="p" width="20" height="20" patternUnits="userSpaceOnUse" font-weight="bolder"><text>x</text></pattern></defs><rect width="40" height="40" fill="url(#p)"/></svg>"##).unwrap();
    let scene = loaded.scene(40.0, 40.0);
    let Kind::Shape(shape) = &scene.items[0].kind else {
        panic!("expected a shape")
    };
    let pattern = shape.fill.as_ref().unwrap().pattern.as_ref().unwrap();
    let Kind::Group(children) = &pattern.items[0].kind else {
        panic!("expected a group")
    };
    let Kind::Text(text) = &children[0].kind else {
        panic!("expected text")
    };
    assert_eq!(text.chunks[0].runs[0].style.font_weight, 700);
}

/// Text resolves `url()` as a shape does: a server that names one colour
/// paints it, a missing one paints its fallback (or nothing), and a
/// gradient is attached.
#[test]
fn a_text_paint_server_resolves_as_a_shapes_does() {
    let runs = runs(
        r##"<defs>
            <linearGradient id="one"><stop stop-color="#00f"/></linearGradient>
            <linearGradient id="g" gradientUnits="userSpaceOnUse">
                <stop stop-color="#f00"/><stop offset="1" stop-color="#00f"/>
            </linearGradient>
        </defs>
        <text fill="url(#missing) #f00">A</text>
        <text fill="url(#one)">B</text>
        <text fill="url(#g)">C</text>
        <text fill="url(#missing)">D</text>"##,
    );
    let color = |i: usize| runs[i].fill.as_ref().map(|f| f.color.resolve(false));
    assert_eq!(
        color(0),
        Some(exact_kernel::Color(0xff0000ff)),
        "the fallback"
    );
    assert_eq!(
        color(1),
        Some(exact_kernel::Color(0x0000ffff)),
        "the one stop"
    );
    assert!(
        runs[2].fill.as_ref().is_some_and(|f| f.server.is_some()),
        "the gradient"
    );
    assert!(runs[3].fill.is_none(), "no server and no fallback");
}
