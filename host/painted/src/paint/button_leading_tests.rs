use super::*;

#[test]
fn neutral_control_text_kernel_and_painter_share_pinned_font_lines() {
    const CHILD: &str = "EXACT_BUTTON_LEADING_TEST";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                std::thread::current().name().unwrap(),
                "--nocapture",
            ])
            .env(CHILD, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    crate::app::show_neutral_look();
    use exact_kernel::{Kernel, NodeType, Offer, Op, PropValue, StyleValue};
    use std::{cell::RefCell, path::Path};
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let latin = root.join("../../scripts/fixtures/fonts/assets/DejaVuSans.ttf");
    let arabic = root.join("tests/fonts/NotoNastaliqUrdu.ttf");
    let mut failures = Vec::new();
    for (family, words, normal) in [
        ("DejaVu Sans", "Go\nGo", 16.0_f32 * 2384.0 / 2048.0),
        ("Noto Nastaliq Urdu", "بيت\nبيت", 40.0),
    ] {
        let engine = Rc::new(RefCell::new(TextEngine::from_font_files(
            &[&latin, &arabic],
            "DejaVu Sans",
        )));
        engine.borrow_mut().install_runtime(8, &[family.into()]);
        let mut k = Kernel::new(Box::new(crate::text::Measurer(engine.clone())));
        let style = |values: &[(StyleId, &str)]| {
            let mut s = StyleProps::default();
            for &(id, value) in values {
                s.set_dynamic(
                    id,
                    &value
                        .parse()
                        .map_or_else(|_| StyleValue::Text(value.into()), StyleValue::Number),
                )
                .unwrap();
            }
            Box::new(s)
        };
        k.apply(
            0,
            0,
            &[
                Op::CreateView {
                    id: 1,
                    node_type: NodeType::View,
                },
                Op::CreateView {
                    id: 2,
                    node_type: NodeType::Control,
                },
                Op::CreateView {
                    id: 3,
                    node_type: NodeType::Text,
                },
                Op::SetProp {
                    id: 2,
                    prop: PropId::Type,
                    value: PropValue::Str("button".into()),
                },
                Op::SetProp {
                    id: 3,
                    prop: PropId::Text,
                    value: PropValue::Str(words.into()),
                },
                Op::SetStyle {
                    id: 1,
                    patch: style(&[
                        (StyleId::Display, "flex"),
                        (StyleId::FlexDirection, "column"),
                        (StyleId::AlignItems, "start"),
                        (StyleId::FontFamily, "8"),
                        (StyleId::LineHeight, "5"),
                    ]),
                },
                Op::SetStyle {
                    id: 2,
                    patch: style(&[
                        (StyleId::BoxSizing, "border-box"),
                        (StyleId::WhiteSpace, "pre-line"),
                    ]),
                },
                Op::SetChildren {
                    id: 2,
                    children: vec![3],
                },
                Op::SetChildren {
                    id: 1,
                    children: vec![2],
                },
                Op::AttachRoot { id: 1 },
            ],
        )
        .unwrap();
        for leading in [
            None,
            Some("1"),
            Some("normal"),
            Some("3"),
            Some("clear"),
            Some("unset"),
            Some("shorthand"),
        ] {
            if matches!(leading, Some("clear" | "unset")) {
                k.apply(
                    0,
                    0,
                    &[Op::ClearStyle {
                        id: 2,
                        mask: exact_kernel::StyleMask::of(StyleId::LineHeight),
                        written: leading == Some("unset"),
                    }],
                )
                .unwrap();
            } else if leading == Some("shorthand") {
                k.apply(
                    0,
                    0,
                    &[
                        Op::ClearStyle {
                            id: 2,
                            mask: exact_kernel::StyleMask::of(StyleId::LineHeight),
                            written: false,
                        },
                        Op::SetStyle {
                            id: 2,
                            patch: style(&[(StyleId::FontSize, "-exact-body")]),
                        },
                        Op::SetProp {
                            id: 2,
                            prop: PropId::FontShorthand,
                            value: PropValue::Bool(true),
                        },
                    ],
                )
                .unwrap();
            } else if let Some(leading) = leading {
                k.apply(
                    0,
                    0,
                    &[Op::SetStyle {
                        id: 2,
                        patch: style(&[(StyleId::LineHeight, leading)]),
                    }],
                )
                .unwrap();
            }
            k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
            let rows = k.button_face_style(2).unwrap();
            if leading == Some("shorthand") && !rows.title.mask.has(StyleId::LineHeight) {
                failures.push("shorthand lost its leading".into());
            }
            let face = k.press_face(2).unwrap();
            let drawn = face_layout(
                &mut engine.borrow_mut(),
                &face,
                &rows,
                AxisOffer::MaxContent,
                false,
                true,
            );
            let line = match leading {
                None | Some("clear") => normal.ceil(),
                Some("1") => 16.0,
                Some("3") => 48.0,
                Some("unset") => 80.0,
                Some("shorthand") => normal.ceil(),
                _ => normal.ceil(),
            };
            let actual = drawn.title.as_ref().unwrap().height;
            let kernel = k.node(2).unwrap().frame.height;
            if (actual - 2.0 * line).abs() >= 0.05
                || (drawn.height - (2.0 * line + 16.0)).abs() >= 0.05
                || (kernel - drawn.height).abs() >= 0.05
            {
                failures.push(format!(
                    "{family} {leading:?}: text {actual}, line {line}, paint {}, kernel {kernel}",
                    drawn.height
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
