use super::*;
use crate::presenter::{PainterChoice, Presenter};
use std::rc::Rc;

fn fixture(text: &str) -> Presenter<()> {
    let source = format!("component App\n  view\n    column width=\"100%\" padding=20 box-sizing=\"border-box\"\n{text}");
    let plan = contract::compile(&source).unwrap();
    let (presenter, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (300.0, 300.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    presenter
}

#[test]
fn nested_text_paints_the_same_paragraph_that_layout_measured() {
    let mut plain = fixture("      text \"Alpha beta gamma delta. Another line wraps here.\" testId=\"paragraph\" font-size=16 line-height=1.5 color=\"#234567\"\n");
    let mut nested = fixture("      text testId=\"paragraph\" font-size=16 line-height=1.5 color=\"#234567\"\n        text \"Alpha beta \"\n        text\n          text \"gamma delta. \"\n          text \"Another line wraps here.\"\n");
    for width in [300.0, 160.0, 240.0] {
        assert!(plain.resize(width, 300.0).is_none());
        assert!(nested.resize(width, 300.0).is_none());
        let frame = |p: &Presenter<()>| {
            let kernel = p.host().kernel();
            kernel
                .node_by_key(kernel.find_by_test_id("paragraph")[0])
                .unwrap()
                .frame
        };
        assert_eq!(
            frame(&plain),
            frame(&nested),
            "kernel paragraph geometry agrees at {width}"
        );
        let expected = plain.frame();
        let actual = nested.frame();
        assert!(expected
            .data()
            .chunks_exact(4)
            .any(|p| p != [255, 255, 255, 255]));
        assert!(
            actual.data() == expected.data(),
            "nested run pixels differ despite identical measured paragraph at width {width}"
        );
    }
}

fn scene_frame(p: &Presenter<()>, dark: bool, backend: Box<dyn Backend>) -> Frame {
    let kernel = p.host().kernel();
    let scene = Scene {
        kernel,
        roots: &kernel.roots(),
        hidden: &|_| false,
        presented: &|_| Presented::IDENTITY,
        paths: &|_| None,
        scroll: &BTreeMap::new(),
        page: (0.0, 0.0),
        images: &BTreeMap::new(),
        focus: None,
        selection: None,
        caret_upstream: false,
        pointer: None,
        controls: &BTreeMap::new(),
        chosen: &BTreeMap::new(),
        menu: None,
    };
    let mut painter = Painter::new(p.text().clone(), 1.0, backend);
    painter.dark = dark;
    painter.paint(&scene, (300.0, 300.0)).unwrap()
}

#[test]
fn repainting_an_unchanged_headline_reuses_its_stroke() {
    let p = fixture(concat!(
        r##"      text "Exact paints a headline" font-size=32 font-weight=800 color="#fdd835" -webkit-text-stroke="2px #000000""##,
        "\n"
    ));
    let first = scene_frame(&p, false, Box::new(crate::raster::Raster::new())).pixmap;
    let built = text_stroke::BAND_BUILDS.get();
    for _ in 0..6 {
        let frame = scene_frame(&p, false, Box::new(crate::raster::Raster::new())).pixmap;
        assert_eq!(frame.data(), first.data());
    }
    assert_eq!(text_stroke::BAND_BUILDS.get(), built);
}

const COLORS: &str = "      text testId=\"paragraph\" color=\"#00000000\" font-size=24 line-height=1.5\n        text color=\"light-dark(#ff0000,#008000)\"\n          text \"MMMM \" font-weight=700 testId=\"red\"\n        text color=\"light-dark(#0000ff,#800080)\" href=\"https://example.com/\" testId=\"link\"\n          text \"WWWW\" font-family=\"monospace\" font-style=\"italic\" font-size=18 testId=\"blue\"\n";

fn has_color(frame: &tiny_skia::Pixmap, color: [u8; 4]) -> bool {
    frame.data().chunks_exact(4).filter(|p| *p == color).count() > 10
}

#[test]
fn nested_run_colors_survive_inheritance_and_appearance() {
    let p = fixture(COLORS);
    for (dark, colors) in [
        (false, [[255, 0, 0, 255], [0, 0, 255, 255]]),
        (true, [[0, 128, 0, 255], [128, 0, 128, 255]]),
    ] {
        let frame = scene_frame(&p, dark, Box::new(crate::raster::Raster::new()));
        for color in colors {
            assert!(
                has_color(&frame.pixmap, color),
                "missing {color:?}, dark={dark}"
            );
        }
    }
}

#[test]
fn styled_paragraph_metrics_and_cpu_gpu_glyph_batches_agree() {
    let mut p = fixture(COLORS);
    for width in [300.0, 160.0, 240.0] {
        assert!(p.resize(width, 300.0).is_none());
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("paragraph")[0])
            .unwrap();
        let canonical = node.text_runs();
        let mut spec = text_spec(&node.computed_style(StyleMask::INHERITED), "");
        spec.runs = canonical
            .iter()
            .map(|r| Run::from_style(&r.text, r.style))
            .collect();
        assert_eq!(spec.runs[0].weight, 700);
        assert!(spec.runs[1].italic);
        assert_eq!(spec.runs[1].size, 18.0);
        let mut light = Vec::new();
        let mut dark = Vec::new();
        text_palette(kernel, &node, false, None, &mut light);
        text_palette(kernel, &node, true, None, &mut dark);
        assert_eq!(light.len(), canonical.len());
        assert_eq!(dark.len(), canonical.len());
        let leaf = kernel.node(light[1].source).unwrap();
        assert_eq!(leaf.props.str(PropId::TestId), Some("blue"));
        let link = kernel.node(leaf.parent.unwrap()).unwrap();
        assert_eq!(link.props.str(PropId::Href), Some("https://example.com/"));
        let mut engine = p.text().borrow_mut();
        let paragraph = engine.paragraph(&spec, Some(node.frame.width));
        let metrics = engine.measure(&spec, exact_kernel::AxisOffer::Definite(node.frame.width));
        assert_eq!(paragraph.height, metrics.height);
        assert_eq!(Some(paragraph.first_baseline), metrics.first_baseline);
        assert_eq!(paragraph.height, node.frame.height);
        for palette in [&light, &dark] {
            let cpu: Vec<_> = paragraph
                .paint_glyphs(palette)
                .map(|(g, baseline, ink)| {
                    (
                        g.glyph_id,
                        g.x,
                        baseline + g.y,
                        g.run(),
                        ink,
                        paragraph.lines().faces[g.face as usize].skew,
                    )
                })
                .collect();
            let gpu: Vec<_> = engine
                .glyph_runs(&paragraph, palette)
                .iter()
                .flat_map(|run| {
                    run.glyphs.iter().map(|(id, x, y)| {
                        (*id, *x, *y, run.run_index, run.paint, run.synthetic_italic)
                    })
                })
                .collect();
            assert!(!cpu.is_empty());
            assert_eq!(cpu, gpu, "common colored glyph stream at width {width}");
        }
        assert!(
            Rc::ptr_eq(&paragraph, &engine.paragraph(&spec, Some(node.frame.width))),
            "appearance/source metadata must not split the shaping cache"
        );
    }
}

#[test]
fn own_text_suppresses_inline_descendants_in_measurement_and_paint() {
    let mut plain = fixture("      text \"Owner\" font-size=24 color=\"#ff0000\"\n");
    let mut nested = fixture("      text \"Owner\" font-size=24 color=\"#ff0000\"\n        text \"Must not paint\" color=\"#0000ff\"\n");
    assert_eq!(plain.frame().data(), nested.frame().data());
}

#[test]
fn identical_fonts_keep_distinct_run_colors_and_source_nodes() {
    let style = StyleProps {
        font_size: 24.0,
        ..StyleProps::default()
    };
    let mut spec = text_spec(&style, "MMMM ");
    spec.runs.push(Run::from_style(
        "WWWW",
        exact_kernel::TextStyle::from_style(&style),
    ));
    let palette = [
        RunPaint {
            color: [255, 0, 0, 255],
            source: 1,
        },
        RunPaint {
            color: [0, 0, 255, 255],
            source: 2,
        },
    ];
    let mut engine = TextEngine::new();
    let paragraph = engine.paragraph(&spec, Some(260.0));
    let batches = engine.glyph_runs(&paragraph, &palette);
    assert_eq!(
        batches.len(),
        2,
        "same font must not merge distinct ink/source runs"
    );
    for (index, batch) in batches.iter().enumerate() {
        assert_eq!(batch.run_index, index);
        assert_eq!(batch.paint, palette[index]);
    }
    let mut raster = crate::raster::Raster::new();
    raster.begin(300.0, 80.0, 1.0);
    raster.text(
        &mut engine,
        &paragraph,
        &palette,
        (20.0, 20.0),
        Transform::identity(),
    );
    let pixels = raster.finish().unwrap();
    assert!(palette.iter().all(|ink| has_color(&pixels, ink.color)));
}

#[test]
fn styled_paragraph_pixels_on_real_gpu_when_available() {
    let gpu = match crate::gpu::Gpu::new() {
        Ok(gpu) => gpu,
        Err(error) => {
            // A missing GPU fails under EXACT_REQUIRED=1, which the async lane sets (scripts/coverage.mjs).
            assert_ne!(
                std::env::var("EXACT_REQUIRED").as_deref(),
                Ok("1"),
                "missing gpu: {error}"
            );
            eprintln!("GPU paragraph pixels NOT checked: {error}; common batch test still runs");
            return;
        }
    };
    eprintln!("GPU paragraph pixels: {} / {}", gpu.adapter, gpu.api);
    let p = fixture(COLORS);
    let mut painter = Painter::new(p.text().clone(), 1.0, Box::new(gpu));
    let kernel = p.host().kernel();
    let scene = Scene {
        kernel,
        roots: &kernel.roots(),
        hidden: &|_| false,
        presented: &|_| Presented::IDENTITY,
        paths: &|_| None,
        scroll: &BTreeMap::new(),
        page: (0.0, 0.0),
        images: &BTreeMap::new(),
        focus: None,
        selection: None,
        caret_upstream: false,
        pointer: None,
        controls: &BTreeMap::new(),
        chosen: &BTreeMap::new(),
        menu: None,
    };
    for (dark, colors) in [
        (false, [[255, 0, 0, 255], [0, 0, 255, 255]]),
        (true, [[0, 128, 0, 255], [128, 0, 128, 255]]),
    ] {
        painter.dark = dark;
        let frame = painter.paint(&scene, (300.0, 300.0)).unwrap();
        let cpu = scene_frame(&p, dark, Box::new(crate::raster::Raster::new()));
        for color in colors {
            assert!(
                has_color(&frame.pixmap, color),
                "GPU missing {color:?}, dark={dark}"
            );
            let bounds = |pixmap: &Pixmap| {
                let mut bounds = [u32::MAX, u32::MAX, 0, 0];
                for (i, pixel) in pixmap.data().chunks_exact(4).enumerate() {
                    if pixel == color {
                        let (x, y) = (i as u32 % pixmap.width(), i as u32 / pixmap.width());
                        bounds = [
                            bounds[0].min(x),
                            bounds[1].min(y),
                            bounds[2].max(x),
                            bounds[3].max(y),
                        ];
                    }
                }
                bounds
            };
            for (cpu, gpu) in bounds(&cpu.pixmap).into_iter().zip(bounds(&frame.pixmap)) {
                assert!(
                    cpu.abs_diff(gpu) <= 2,
                    "CPU/GPU colored ink bounds differ: {cpu}, {gpu}"
                );
            }
        }
    }
}

#[test]
fn css_strike_through_changes_pixels_without_changing_layout() {
    let mut plain = fixture("      text \"MMMM WWWW\" testId=\"line\" font-size=24\n");
    let mut strike = fixture(
        "      text \"MMMM WWWW\" testId=\"line\" font-size=24 text-decoration=\"line-through\"\n",
    );
    let frame = |p: &Presenter<()>| {
        p.host()
            .kernel()
            .node_by_key(p.host().kernel().find_by_test_id("line")[0])
            .unwrap()
            .frame
    };
    assert_eq!(frame(&plain), frame(&strike));
    let before = plain.frame();
    let after = strike.frame();
    assert_ne!(
        before.data(),
        after.data(),
        "the decoration must actually paint"
    );
}

#[test]
fn a_box_draws_its_lines_through_nested_text_in_its_own_colour() {
    // CSS Text Decoration 3 §2.1: the paragraph's lines cross its blue child
    // in the paragraph's red; the paragraph has no text of its own.
    let mut p = fixture(
        "      text text-decoration=\"underline line-through\" color=\"#ff0000\" font-size=24\n        text \"MMMM\" color=\"#0000ff\"\n",
    );
    let frame = p.frame();
    let (mut red, mut blue) = (0, 0);
    for px in frame.data().chunks_exact(4) {
        match (px[0], px[1], px[2], px[3]) {
            (200.., ..160, ..160, 200..) => red += 1,
            (..160, ..160, 200.., 200..) => blue += 1,
            _ => {}
        }
    }
    assert!(blue > 0, "the child's glyphs keep their colour");
    // Two lines across four glyphs at 24 px; in the child's colour, none.
    assert!(
        red >= 100,
        "the lines are the paragraph's colour: {red} red pixels, {blue} blue"
    );
}

#[test]
fn the_nearest_box_line_shows_and_hidden_text_draws_none() {
    // As Chrome: the green box's line-through shows over the red
    // paragraph's on the blue text, and the hidden text has no line.
    let mut p = fixture(
        "      text text-decoration=\"line-through\" color=\"#ff0000\" font-size=24\n        text text-decoration=\"line-through\" color=\"#00ff00\"\n          text \"MMMM\" color=\"#0000ff\"\n        text \"WWWW\" visibility=\"hidden\"\n",
    );
    let frame = p.frame();
    let (mut red, mut green) = (0, 0);
    for px in frame.data().chunks_exact(4) {
        match (px[0], px[1], px[2], px[3]) {
            (200.., ..160, ..160, 200..) => red += 1,
            (..160, 200.., ..160, 200..) => green += 1,
            _ => {}
        }
    }
    assert_eq!(red, 0, "no line of the paragraph's shows");
    assert!(
        green >= 40,
        "the nearer box's line shows: {green} green pixels"
    );
}

fn decoration_frame(text: &str) -> std::sync::Arc<Pixmap> {
    fixture(text).frame().clone()
}

#[test]
fn a_strike_paints_over_the_glyphs_it_crosses() {
    let draw = |color| {
        decoration_frame(&format!(
            "      text color=\"#ff0000\" font-size=40 text-decoration-line=\"line-through\"\n        text \"HHHHHH\" color=\"{color}\"\n"
        ))
    };
    let line = draw("transparent");
    let text = draw("#000000");
    let red = [255, 0, 0, 255];
    let mut covered = 0;
    let mut cut = 0;
    for (expected, actual) in line.data().chunks_exact(4).zip(text.data().chunks_exact(4)) {
        if expected == red {
            covered += 1;
            cut += usize::from(actual != red);
        }
    }
    assert!(covered > 100, "the reference strike must paint");
    assert_eq!(cut, 0, "glyphs must not cut the red strike");
}

#[test]
fn an_underline_covers_the_text_shadow_and_casts_its_own() {
    let line = decoration_frame(
        "      text color=\"#000000\" font-size=40 text-decoration-line=\"underline\"\n        text \"HHHHHH\" color=\"transparent\"\n",
    );
    let text = decoration_frame(
        "      text \"HHHHHH\" color=\"#000000\" font-size=40 text-decoration-line=\"underline\" text-shadow=\"0px 6px 0px #ff0000\"\n",
    );
    let black = [0, 0, 0, 255];
    let red = [255, 0, 0, 255];
    let mut covered = 0;
    let mut cut = 0;
    let mut missing = 0;
    for (i, expected) in line.data().chunks_exact(4).enumerate() {
        if expected == black {
            covered += 1;
            cut += usize::from(text.data()[i * 4..i * 4 + 4] != black);
            let shadow = (i + 6 * text.width() as usize) * 4;
            missing += usize::from(text.data()[shadow..shadow + 4] != red);
        }
    }
    assert!(covered > 100, "the reference underline must paint");
    assert_eq!(
        (cut, missing),
        (0, 0),
        "the line is whole and casts a red copy"
    );
}

#[test]
fn decorations_and_glyphs_cast_one_translucent_shadow_beyond_the_line_box() {
    for height in ["", "line-height=0.5"] {
        let draw = |color, shadow| {
            decoration_frame(&format!(
                "      text color=\"#000000\" font-size=40 {height} text-decoration-line=\"underline line-through\" text-shadow=\"{shadow}\"\n        text \"HHHHHH\" color=\"{color}\"\n"
            ))
        };
        let line = draw("transparent", "none");
        let text = draw("#000000", "0px 70px 0px #ff000080");
        let mut covered = 0;
        for (i, expected) in line.data().chunks_exact(4).enumerate() {
            if expected == [0, 0, 0, 255] {
                covered += 1;
                let shadow = (i + 70 * text.width() as usize) * 4;
                assert_eq!(
                    &text.data()[shadow..shadow + 4],
                    [255, 127, 127, 255],
                    "the composited lines and glyphs cast one shadow: {height}, pixel {i}"
                );
            }
        }
        assert!(covered > 100, "the reference lines must paint");
    }
}

#[test]
fn a_runs_shadow_does_not_shadow_an_ancestors_decoration() {
    let line = decoration_frame(
        "      text color=\"#000000\" font-size=40 text-decoration-line=\"underline\"\n        text \"HHHHHH\" color=\"transparent\"\n",
    );
    let text = decoration_frame(concat!(
        "      text color=\"#000000\" font-size=40 text-decoration-line=\"underline\"\n",
        "        text \"HH\" text-shadow=\"0px 60px 0px #ff0000\"\n",
        "        text \"HH\" text-shadow=\"none\"\n",
        "        text \"HH\" text-shadow=\"0px 70px 0px #0000ff\"\n",
    ));
    assert!(
        has_color(&text, [255, 0, 0, 255]),
        "the first run casts a red glyph shadow"
    );
    assert!(
        has_color(&text, [0, 0, 255, 255]),
        "the last run casts a blue glyph shadow"
    );
    let points: Vec<_> = line
        .data()
        .chunks_exact(4)
        .enumerate()
        .filter_map(|(i, p)| (p == [0, 0, 0, 255]).then_some(i))
        .collect();
    assert!(points.len() > 100, "the reference underline must paint");
    let width = text.width() as usize;
    let left = points.iter().map(|i| i % width).min().unwrap();
    let right = points.iter().map(|i| i % width).max().unwrap();
    let third = (right - left + 1) as f32 / 3.0;
    for i in points {
        let x = (i % width - left) as f32;
        // Keep the probes inside each two-letter run, clear of raster edges.
        let run = (x / third) as usize;
        let within = x % third;
        if within < 3.0 || within > third - 3.0 {
            continue;
        }
        let dy = match run {
            0 => 60,
            _ => 70,
        };
        let shadow = (i + dy * width) * 4;
        assert_eq!(
            &text.data()[shadow..shadow + 4],
            [255; 4],
            "run {run}, offset {dy}"
        );
    }
}

#[test]
fn a_decoration_keeps_its_declaring_boxs_shadow_when_a_child_disables_it() {
    let draw = |shadow| {
        decoration_frame(&format!(
            "      text color=\"#000000\" font-size=40 text-decoration-line=\"underline\" text-shadow=\"{shadow}\"\n        text \"HHHHHH\" color=\"transparent\" text-shadow=\"none\"\n"
        ))
    };
    let line = draw("none");
    let text = draw("0px 6px 0px #ff0000");
    let mut covered = 0;
    for (i, expected) in line.data().chunks_exact(4).enumerate() {
        if expected == [0, 0, 0, 255] {
            covered += 1;
            assert_eq!(&text.data()[i * 4..i * 4 + 4], expected);
            let shadow = (i + 6 * text.width() as usize) * 4;
            assert_eq!(&text.data()[shadow..shadow + 4], [255, 0, 0, 255]);
        }
    }
    assert!(covered > 100, "the reference underline must paint");
    let red = text
        .data()
        .chunks_exact(4)
        .filter(|p| *p == [255, 0, 0, 255])
        .count();
    assert_eq!(red, covered, "only the underline casts a shadow");
}

#[test]
fn a_nearer_decoration_does_not_remove_an_ancestors_shadow() {
    for line in ["underline", "line-through"] {
        let reference = decoration_frame(&format!(
            "      text color=\"#000000\" font-size=40 text-decoration-line=\"{line}\"\n        text \"HHHH\" color=\"transparent\"\n"
        ));
        let text = decoration_frame(&format!(
            "      text color=\"#000000\" font-size=40 text-decoration-line=\"{line}\" text-shadow=\"0px 60px 0px #ff0000\"\n        text \"HHHH\" text-decoration-line=\"{line}\" text-shadow=\"none\"\n"
        ));
        let mut covered = 0;
        for (i, expected) in reference.data().chunks_exact(4).enumerate() {
            if expected == [0, 0, 0, 255] {
                covered += 1;
                let shadow = (i + 60 * text.width() as usize) * 4;
                assert_eq!(
                    &text.data()[shadow..shadow + 4],
                    [255, 0, 0, 255],
                    "the ancestor's {line} keeps its shadow beneath the child's line"
                );
            }
        }
        assert!(covered > 100, "the reference {line} must paint");
    }
}

#[test]
fn nested_decoration_shadows_paint_in_declaring_box_order() {
    for (ancestor, child, offset) in [
        ("line-through", "underline", "45.5px"),
        ("underline", "line-through", "74.5px"),
    ] {
        let draw = |ancestor_shadow, child_shadow| {
            decoration_frame(&format!(
                "      text color=\"transparent\" font-size=40 text-decoration-line=\"{ancestor}\" text-shadow=\"{ancestor_shadow}\"\n        text text-decoration-line=\"{child}\" text-shadow=\"{child_shadow}\"\n          text \"HHHH\" text-shadow=\"none\"\n"
            ))
        };
        let red = "0px 60px 0px #ff0000";
        let green = format!("0px {offset} 0px #00ff00");
        let earlier = draw(red, "none");
        let later = draw("none", &green);
        let combined = draw(red, &green);
        let mut overlap = 0;
        for ((earlier, later), actual) in earlier
            .data()
            .chunks_exact(4)
            .zip(later.data().chunks_exact(4))
            .zip(combined.data().chunks_exact(4))
        {
            if earlier == [255, 0, 0, 255] && later == [0, 255, 0, 255] {
                overlap += 1;
                assert_eq!(actual, later, "the child's {child} shadow paints last");
            }
        }
        assert!(overlap > 100, "the {ancestor} and {child} shadows overlap");
    }
}

#[test]
fn a_leaf_restoring_an_ancestors_shadow_keeps_decoration_shadow_order() {
    let line = decoration_frame(
        "      text color=\"#000000\" font-size=40 text-decoration-line=\"underline\"\n        text \"HHHH\" color=\"transparent\"\n",
    );
    let text = decoration_frame(concat!(
        "      text color=\"transparent\" font-size=40 text-decoration-line=\"underline\" text-shadow=\"0px 60px 0px #ff0000\"\n",
        "        text text-decoration-line=\"underline\" text-shadow=\"0px 60px 0px #00ff00\"\n",
        "          text \"HHHH\" text-shadow=\"0px 60px 0px #ff0000\"\n",
    ));
    let mut covered = 0;
    for (i, expected) in line.data().chunks_exact(4).enumerate() {
        if expected == [0, 0, 0, 255] {
            covered += 1;
            let shadow = (i + 60 * text.width() as usize) * 4;
            assert_eq!(
                &text.data()[shadow..shadow + 4],
                [0, 255, 0, 255],
                "the child's underline shadow covers the ancestor's"
            );
        }
    }
    assert!(covered > 100, "the reference underline must paint");
}

#[test]
fn a_partial_shadow_override_does_not_split_a_blurred_decoration() {
    // Spaces keep glyph shadows away from the join, where they could hide a seam.
    let draw = |first_shadow| {
        decoration_frame(&format!(
            "      text color=\"transparent\" font-size=40 white-space=\"pre\" text-decoration-line=\"underline\" text-shadow=\"0px 60px 8px #ff0000\"\n        text \"HHH   \" text-shadow=\"{first_shadow}\"\n        text \"HHH\" text-shadow=\"none\"\n"
        ))
    };
    let line = draw("none");
    let text = draw("0px 60px 8px #ff0000");
    let mut covered = 0;
    for (i, (expected, actual)) in line
        .data()
        .chunks_exact(4)
        .zip(text.data().chunks_exact(4))
        .enumerate()
    {
        if expected[1] < 250 {
            covered += 1;
            assert!(
                actual[1] <= expected[1] + 1,
                "adding glyph coverage must not lighten the continuous shadow at pixel {i}: {expected:?} -> {actual:?}"
            );
        }
    }
    assert!(
        covered > 100,
        "the reference underline casts a blurred shadow"
    );
}

#[test]
fn a_strike_paints_over_a_later_runs_overlapping_shadow() {
    let draw = |color, shadow| {
        decoration_frame(&format!(
            "      text color=\"#ff0000\" font-size=40 text-decoration-line=\"line-through\"\n        text \"HHHH\" color=\"{color}\" text-shadow=\"none\"\n        text \"HHHH\" color=\"{color}\" text-shadow=\"{shadow}\"\n"
        ))
    };
    let line = draw("transparent", "none");
    let text = draw("#000000", "-24px 0px 0px #00ff00");
    assert!(
        has_color(&text, [0, 255, 0, 255]),
        "the later run casts a shadow"
    );
    let mut covered = 0;
    let mut cut = 0;
    for (expected, actual) in line.data().chunks_exact(4).zip(text.data().chunks_exact(4)) {
        if expected == [255, 0, 0, 255] {
            covered += 1;
            cut += usize::from(actual != expected);
        }
    }
    assert!(covered > 100, "the reference strike must paint");
    assert_eq!(
        cut, 0,
        "a later run's shadow must not cut the ancestor's strike"
    );
}

#[test]
fn hidden_markdown_casts_no_decoration_shadow() {
    for text in ["~~HHHH~~", "[HHHH](https://example.com)"] {
        let draw = |visibility| {
            decoration_frame(&format!(
                "      text \"{text}\" markup=\"markdown\" visibility=\"{visibility}\" font-size=40 text-shadow=\"0px 6px 0px #ff0000\"\n"
            ))
        };
        assert!(has_color(&draw("visible"), [255, 0, 0, 255]));
        assert!(
            draw("hidden").data().chunks_exact(4).all(|p| p == [255; 4]),
            "hidden Markdown paints neither glyphs nor decoration shadows: {text}"
        );
    }
}

#[test]
fn an_inherited_currentcolor_shadow_uses_the_childs_color() {
    let draw = |shadow| {
        decoration_frame(&format!(
            "      text color=\"#ff0000\" font-size=40 text-shadow=\"{shadow}\"\n        text \"HHHH\" color=\"#000000\"\n"
        ))
    };
    let inherited = draw("0px 60px 0px");
    let explicit = draw("0px 60px 0px #000000");
    assert_eq!(inherited.data(), explicit.data());
    assert!(has_color(&inherited, [0, 0, 0, 255]));
    assert_ne!(inherited.data(), draw("none").data());
}

#[test]
fn a_later_siblings_shadow_paints_over_an_earlier_siblings_strike() {
    for nested in [false, true] {
        let draw = |line, color, shadow| {
            let first = if nested {
                format!(
                    "        text color=\"#ff0000\" text-decoration-line=\"{line}\"\n          text \"HH\" color=\"transparent\"\n          text \"HH\" color=\"transparent\"\n"
                )
            } else {
                format!("        text \"HHHH\" color=\"#ff0000\" text-decoration-line=\"{line}\"\n")
            };
            decoration_frame(&format!(
                "      text font-size=40\n{first}        text \"HHHH\" color=\"{color}\" text-shadow=\"{shadow}\"\n"
            ))
        };
        let line = draw("line-through", "transparent", "none");
        let shadow = draw("none", "#000000", "-24px 0px 0px #00ff00");
        let combined = draw("line-through", "#000000", "-24px 0px 0px #00ff00");
        let mut overlap = 0;
        for ((line, shadow), actual) in line
            .data()
            .chunks_exact(4)
            .zip(shadow.data().chunks_exact(4))
            .zip(combined.data().chunks_exact(4))
        {
            if line == [255, 0, 0, 255] && shadow == [0, 255, 0, 255] {
                overlap += 1;
                assert_eq!(actual, shadow, "the later sibling covers the strike");
            }
        }
        assert!(overlap > 0, "the shadow must cross the strike: {nested}");
    }
}

#[test]
fn a_later_siblings_decoration_shadow_follows_earlier_content() {
    for first_line in ["none", "line-through"] {
        for (line, offset) in [("line-through", "0px"), ("underline", "-14px")] {
            let draw = |first_color, first_line, shadow| {
                decoration_frame(&format!(
                    "      text font-size=40\n        text \"HHHH\" color=\"{first_color}\" text-decoration-line=\"{first_line}\"\n        text color=\"#000000\" text-decoration-line=\"{line}\" text-shadow=\"{shadow}\"\n          text \"HH\" text-shadow=\"none\"\n          text \"HH\" text-shadow=\"none\"\n"
                ))
            };
            let shadow = format!("-24px {offset} 0px #00ff00");
            let earlier = draw("#ff0000", first_line, "none");
            let later = draw("transparent", "none", &shadow);
            let combined = draw("#ff0000", first_line, &shadow);
            let mut overlap = 0;
            for ((earlier, later), actual) in earlier
                .data()
                .chunks_exact(4)
                .zip(later.data().chunks_exact(4))
                .zip(combined.data().chunks_exact(4))
            {
                if earlier == [255, 0, 0, 255] && later == [0, 255, 0, 255] {
                    overlap += 1;
                    assert_eq!(
                        actual, later,
                        "the later {line} shadow must cover the earlier sibling's {first_line}"
                    );
                }
            }
            assert!(overlap > 0, "the {line} shadow must cross earlier content");
        }
    }
}

/// Where `earlier` is red and `later` green, `combined` must be green: the
/// later paint covers the earlier. Returns how many pixels were compared.
fn later_covers(earlier: &Pixmap, later: &Pixmap, combined: &Pixmap, what: &str) -> usize {
    let mut overlap = 0;
    for ((earlier, later), actual) in earlier
        .data()
        .chunks_exact(4)
        .zip(later.data().chunks_exact(4))
        .zip(combined.data().chunks_exact(4))
    {
        if earlier == [255, 0, 0, 255] && later == [0, 255, 0, 255] {
            overlap += 1;
            assert_eq!(actual, later, "{what}");
        }
    }
    overlap
}

#[test]
fn a_later_runs_shared_shadow_paints_over_an_earlier_siblings_line() {
    for nested in [false, true] {
        for line in ["underline", "line-through"] {
            // The two runs inherit one shadow, so only the line's box ends between them.
            let draw = |line: &str, color: &str, shared: &str, own: &str| {
                let first = if nested {
                    format!(
                        "        text color=\"#ff0000\" text-decoration-line=\"{line}\"\n          text \"HH\" color=\"{color}\"\n          text \"HH\" color=\"{color}\"\n"
                    )
                } else {
                    format!(
                        "        text \"HHHH\" color=\"{color}\" text-decoration-line=\"{line}\"\n"
                    )
                };
                decoration_frame(&format!(
                    "      text font-size=40 text-shadow=\"{shared}\"\n{first}        text \"HHHH\" color=\"#000000\"{own}\n"
                ))
            };
            let green = "-24px 6px 0px #00ff00";
            let earlier = draw(line, "#ff0000", "none", "");
            let later = draw(
                "none",
                "transparent",
                "none",
                &format!(" text-shadow=\"{green}\""),
            );
            let combined = draw(line, "#ff0000", green, "");
            let overlap = later_covers(
                &earlier,
                &later,
                &combined,
                &format!("the later run's shadow covers the earlier {line}: {nested}"),
            );
            assert!(overlap > 0, "the shadow must cross the {line}: {nested}");
        }
    }
}

#[test]
fn a_later_runs_glyph_shadow_is_not_painted_with_an_ancestors_decoration_shadow() {
    let draw = |first_color, shadow| {
        decoration_frame(&format!(
            "      text font-size=40 text-decoration-line=\"underline\" text-shadow=\"{shadow}\"\n        text \"HHHH\" color=\"{first_color}\" text-shadow=\"none\"\n        text \"HHHH\" color=\"#000000\"\n"
        ))
    };
    let green = "-24px 0px 0px #00ff00";
    let earlier = draw("#ff0000", "none");
    let later = draw("transparent", green);
    let combined = draw("#ff0000", green);
    let overlap = later_covers(
        &earlier,
        &later,
        &combined,
        "the later run's glyph shadow covers the earlier run's glyphs",
    );
    assert!(overlap > 0, "the shadow must cross the earlier glyphs");
}

#[test]
fn html_maxlength_is_enforced_by_linux_typing() {
    let plan = contract::compile("component App\n  state draft = \"\"\n  action edit(value: string)\n    draft = value\n  view\n    input value=draft input=edit maxlength=3 testId=\"limited\"\n").unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (300.0, 300.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none());
    let key = p.host().kernel().find_by_test_id("limited")[0];
    let id = p.host().kernel().node_by_key(key).unwrap().id;
    p.type_text(id, "a😀b").unwrap();
    assert_eq!(
        p.host().kernel().node(id).unwrap().props.str(PropId::Value),
        Some("a😀")
    );
}

#[test]
fn paint_ranks_survive_repaints_and_scroll_until_a_kernel_commit() {
    let mut p = fixture("      box height=900 testId=\"row\"\n");
    p.frame();
    let passes = p.brush.rank_passes;
    let epoch = p.host().kernel().epoch();
    p.frame();
    p.wheel_at(40., 40., 0., 80.);
    p.frame();
    assert_eq!(p.host().kernel().epoch(), epoch);
    assert_eq!(
        p.brush.rank_passes, passes,
        "repaint and scroll reuse the ranks"
    );
    let id = p.host().kernel().find_by_test_id("row")[0];
    let id = p.host().kernel().node_by_key(id).unwrap().id;
    let mut style = exact_kernel::StyleProps {
        opacity: 0.5,
        ..Default::default()
    };
    style.mask.set(exact_kernel::StyleId::Opacity);
    p.host
        .runner_mut()
        .kernel_mut()
        .apply(
            0,
            epoch + 1,
            &[exact_kernel::Op::SetStyle {
                id,
                patch: Box::new(style),
            }],
        )
        .unwrap();
    p.frame();
    assert_eq!(
        p.brush.rank_passes,
        passes + 1,
        "a style commit recomputes ranks"
    );
    assert_eq!(p.brush.ranks[&id], 1);
    let plan = contract::compile("component App\n  view\n    box width=100 height=100\n").unwrap();
    p.reload(&plan.encode(), ()).unwrap();
    p.frame();
    assert_eq!(
        p.brush.rank_passes,
        passes + 2,
        "replacement kernels invalidate ranks"
    );
}

#[test]
fn inline_border_radius_cuts_the_background_corners() {
    let draw = |radius| {
        let mut p = fixture(&format!(
            "      text font-size=24 color=\"transparent\"\n        text \"rounded inline\" background-color=\"#ff0000\" border-radius={radius}\n"
        ));
        *p.text().borrow_mut() = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fonts/Inter-Regular.ttf"
            ))],
            "Inter",
        );
        p.frame().clone()
    };
    let square = draw(0);
    let rounded = draw(8);
    let first = square
        .pixels()
        .iter()
        .position(|p| p.red() == 255 && p.green() == 0 && p.alpha() == 255)
        .expect("the inline background paints red");
    assert!(
        rounded.pixels()[first].green() > 200,
        "border-radius must expose the page at the background's corner"
    );
    assert!(has_color(&rounded, [255, 0, 0, 255]));
}

/// A box inside another with a background paints its own over it, each
/// keeping its corners (CSS 2 Appendix E), as Chrome paints them.
#[test]
fn nested_inline_backgrounds_each_keep_their_own_box() {
    let p = fixture(
        "      text testId=\"paragraph\"\n        text background-color=\"#ff0000\" border-radius=6 testId=\"outer\"\n          text \"before\"\n          text\n            text \"inner\" background-color=\"#0000ff\" border-radius=9\n          text \"after\"\n        text background-color=\"#00ff00\" border-radius=3\n          text \"independent\" background-color=\"transparent\"\n",
    );
    let kernel = p.host().kernel();
    let node = kernel
        .node_by_key(kernel.find_by_test_id("paragraph")[0])
        .unwrap();
    let backgrounds = text_backgrounds(kernel, &node, false, None);
    assert_eq!(backgrounds.len(), 4);
    let values: Vec<_> = backgrounds.into_iter().map(Option::unwrap).collect();
    assert_eq!(values[0].id, values[2].id);
    assert_ne!(values[0].id, values[1].id);
    assert_eq!(values[0].color, [255, 0, 0, 255]);
    assert_eq!(values[1].color, [0, 0, 255, 255]);
    assert_eq!(values[0].radii, [const { Dimension::Points(6.0) }; 4]);
    assert_eq!(values[1].radii, [const { Dimension::Points(9.0) }; 4]);
    assert_eq!(
        values[1].outer.iter().map(|o| o.id).collect::<Vec<_>>(),
        [values[0].id],
        "the outer box paints under the inner"
    );
    assert!(values[0].outer.is_empty());
    assert_eq!(
        values[3].radii,
        [const { Dimension::Points(3.0) }; 4],
        "a transparent descendant keeps its ancestor's rounding"
    );
    for inner in [
        "text background-color=\"#0000ff\"",
        "text \"\" background-color=\"#0000ff\"",
    ] {
        let p = fixture(&format!(
            "      text testId=\"paragraph\"\n        text background-color=\"#ff0000\" border-radius=6\n          text \"hello\"\n          {inner}\n"
        ));
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("paragraph")[0])
            .unwrap();
        let backgrounds = text_backgrounds(kernel, &node, false, None);
        assert_eq!(
            backgrounds[0].as_ref().unwrap().radii,
            [const { Dimension::Points(6.0) }; 4],
            "{inner}"
        );
    }
}

/// A run's background reaches over its padding and border, not its margin
/// (CSS 2 §14.2), from the kernel's `inline_background_reach`.
#[test]
fn an_inline_background_reaches_over_its_padding_and_not_its_margin() {
    let p = fixture(
        "      text testId=\"paragraph\"\n        text \"a \"\n        text \"code\" background-color=\"#dddddd\" padding-left=4 padding-right=4 margin-right=3\n        text \" b\"\n",
    );
    let kernel = p.host().kernel();
    let node = kernel
        .node_by_key(kernel.find_by_test_id("paragraph")[0])
        .unwrap();
    let backgrounds = text_backgrounds(kernel, &node, false, None);
    let code = backgrounds[1].as_ref().expect("its background");
    assert_eq!(
        code.reach,
        exact_kernel::InlineEdges {
            start: 4.0,
            end: 4.0
        }
    );
}

#[test]
fn a_clipping_text_keeps_its_own_glyphs_inside_its_padding_box() {
    // CSS Overflow 3 §3: `overflow: hidden` clips the box's own inline
    // content, not only its descendant boxes. The text box is x 20..70.
    let mut p = fixture("      text \"MMMMMMMMMMMMMMMM\" width=50 height=30 white-space=\"nowrap\" overflow=\"hidden\" font-size=20 color=\"#000000\"\n");
    let frame = p.frame();
    let ink = |columns: std::ops::Range<usize>| {
        let pixels = frame.data().chunks_exact(4).enumerate();
        pixels
            .filter(|(i, px)| columns.contains(&(i % 300)) && px[0] < 128)
            .count()
    };
    assert!(ink(20..70) > 0, "the text paints inside its box");
    assert_eq!(ink(70..300), 0, "no glyph past the box's right edge");
}

/// Where a box's room is drawn: its sides are its paragraph's start and end,
/// so in a left-to-right paragraph an `rtl` box's `padding-right` is the gap
/// after its text, as in Chrome, not one before it.
#[test]
fn a_boxs_padding_is_on_its_side_in_its_paragraph() {
    let xs = |box_style: &str| {
        let p = fixture(&format!(
            "      text testId=\"paragraph\" font-size=16\n        text \"12\"{box_style}\n        text \"The\"\n"
        ));
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("paragraph")[0])
            .unwrap();
        let mut spec = text_spec(&node.computed_style(StyleMask::INHERITED), "");
        spec.runs = node
            .text_runs()
            .iter()
            .map(crate::text::Run::from_kernel)
            .collect();
        let paragraph = p.text().borrow_mut().paragraph(&spec, None);
        paragraph
            .layout_runs()
            .flat_map(|r| r.glyphs.iter().map(|g| (g.start, g.x)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let at = |glyphs: &[(u32, f32)], byte: u32| glyphs.iter().find(|g| g.0 == byte).unwrap().1;
    let bare = xs("");
    let padded = xs(" direction=\"rtl\" padding-right=5");
    assert!(
        (at(&padded, 0) - at(&bare, 0)).abs() < 0.05,
        "no gap before it"
    );
    assert!(
        (at(&padded, 2) - at(&bare, 2) - 5.0).abs() < 0.05,
        "{bare:?} {padded:?}"
    );
}
