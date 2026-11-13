use super::{route, style};
use crate::browser_cases::{css_rows, props};
use exact_kernel::{BlockAxis, HostBlock, HostCover, Kernel, Offer, Op, StyleId, StyleValue};

const OFFER: Offer = Offer::definite(390.0, 951.0);
const COVER: HostCover = HostCover::Edges([20.0, 30.0, 49.0, 10.0]);

fn fixture(display: &str, css: &str) -> Kernel {
    let mut k = route();
    let container = format!("display:{display};flex-direction:column;padding:7px");
    let child = format!("position:absolute;{css}");
    k.apply(
        1,
        2,
        &[
            Op::SetStyle {
                id: 2,
                patch: Box::new(props(&css_rows(&container))),
            },
            Op::SetStyle {
                id: 4,
                patch: Box::new(props(&css_rows(&child))),
            },
            Op::SetChildren {
                id: 2,
                children: vec![4],
            },
        ],
    )
    .unwrap();
    k
}

fn frame(k: &Kernel, id: u32) -> [f32; 4] {
    let f = k.node(id).unwrap().frame;
    [f.x, f.y, f.width, f.height]
}

// Chrome 156.0.8078.4, 2026-11-13: literal CSS in a positioned inner box
// at (10, 20), 350 by 882, border-box with padding 7. The same nine cases
// give these frames for block, flex-column and grid containers.
#[test]
fn absolute_sizes_constraints_and_insets_match_the_browser_inside_the_cover() {
    let cases = [
        ("left:0;bottom:0;width:100%;height:100%", [10., 20., 350., 882.]),
        (
            "left:0;bottom:0;width:100%;height:2000px;max-height:calc(100% - 500px)",
            [10., 520., 350., 382.],
        ),
        (
            "left:0;top:0;width:1px;height:1px;min-width:50%;min-height:50%",
            [10., 20., 175., 441.],
        ),
        (
            "left:0;bottom:0;width:2000px;height:2000px;max-width:calc(50% - 10px);max-height:clamp(100px,75%,700px)",
            [10., 240.5, 165., 661.5],
        ),
        (
            "left:calc(10% + 3px);top:25%;width:calc(50% - 5px);height:min(80%,700px)",
            [48., 240.5, 170., 700.],
        ),
        (
            "top:0;right:0;bottom:0;left:0;min-width:25%;max-width:75%;min-height:25%;max-height:75%;margin-left:auto;margin-right:auto;margin-top:auto;margin-bottom:auto",
            [53.75, 130.25, 262.5, 661.5],
        ),
        (
            "left:0;top:0;width:100%;height:50%;padding:10%;border-width:2px;border-style:solid",
            [10., 20., 424., 515.],
        ),
        (
            "left:0;top:0;width:100%;height:50%;padding:10%;border-width:2px;border-style:solid;box-sizing:border-box",
            [10., 20., 350., 441.],
        ),
        ("top:0;right:0;bottom:0;left:0", [10., 20., 350., 882.]),
    ];
    let mut failures = Vec::new();
    for display in ["block", "flex", "grid"] {
        for (css, want) in cases {
            let mut k = fixture(display, css);
            k.compute_layout(1, OFFER).unwrap();
            let authored = frame(&k, 4);
            for cover in [Some(COVER), None, Some(COVER), None] {
                k.set_host_cover(2, cover).unwrap();
                k.compute_layout(1, OFFER).unwrap();
                let expected = if cover.is_some() { want } else { authored };
                let got = frame(&k, 4);
                if got != expected {
                    failures.push(format!(
                        "{display}, {css}, {cover:?}: {got:?}, want {expected:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn nested_positioned_boxes_and_host_blocks_take_clearance_once() {
    let mut k = fixture("flex", "left:0;bottom:0;width:100%;height:100%");
    k.apply(
        1,
        3,
        &[
            Op::CreateView {
                id: 5,
                node_type: exact_kernel::NodeType::View,
            },
            Op::SetStyle {
                id: 4,
                patch: Box::new(style(&[
                    (StyleId::PositionType, StyleValue::Text("relative".into())),
                    (StyleId::BoxSizing, StyleValue::Text("border-box".into())),
                ])),
            },
            Op::SetStyle {
                id: 5,
                patch: Box::new(props(&css_rows(
                    "position:absolute;left:0;bottom:0;width:100%;height:100%",
                ))),
            },
            Op::SetChildren {
                id: 4,
                children: vec![5],
            },
        ],
    )
    .unwrap();
    k.set_host_cover(2, Some(COVER)).unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(frame(&k, 4), [17., 27., 336., 868.]);
    assert_eq!(
        frame(&k, 5),
        frame(&k, 4),
        "the positioned parent already cleared it"
    );

    k.set_host_block(
        4,
        Some(HostBlock {
            width: BlockAxis::Is(600.),
            height: BlockAxis::Is(500.),
        }),
    )
    .unwrap();
    for cover in [
        Some(COVER),
        Some(HostCover::Edges([30., 40., 60., 20.])),
        None,
    ] {
        k.set_host_cover(2, cover).unwrap();
        k.compute_layout(1, OFFER).unwrap();
        assert_eq!(
            frame(&k, 5),
            [0., 0., 600., 500.],
            "a host block has its own allocation"
        );
    }
    k.set_host_cover(4, Some(HostCover::Edges([0., 0., 49., 0.])))
        .unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(
        frame(&k, 5),
        [0., 0., 600., 451.],
        "its own cover still applies"
    );
    k.set_host_block(4, None).unwrap();
    k.set_host_cover(4, None).unwrap();
    k.set_host_cover(2, Some(COVER)).unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(frame(&k, 5), [17., 27., 336., 868.]);
}

#[test]
fn resizing_and_opting_out_change_the_percentage_basis() {
    let mut k = fixture("flex", "left:0;bottom:0;width:100%;height:100%");
    k.set_host_cover(2, Some(COVER)).unwrap();
    for (width, height) in [(390., 951.), (700., 1000.), (390., 951.)] {
        k.compute_layout(1, Offer::definite(width, height)).unwrap();
        assert_eq!(frame(&k, 4), [10., 20., width - 40., height - 69.]);
    }
    for (batch, value, expected) in [
        (3, "bottom", [10., 20., 350., 931.]),
        (4, "none", [0., 0., 390., 951.]),
        (5, "auto", [10., 20., 350., 882.]),
    ] {
        k.apply(
            1,
            batch,
            &[Op::SetStyle {
                id: 2,
                patch: Box::new(style(&[(
                    StyleId::SafeArea,
                    StyleValue::Text(value.into()),
                )])),
            }],
        )
        .unwrap();
        k.compute_layout(1, OFFER).unwrap();
        assert_eq!(frame(&k, 4), expected);
    }
}

#[test]
fn grid_track_edges_are_already_inside_the_cover() {
    let mut k = fixture(
        "grid",
        "top:0;left:0;width:100%;height:100%;grid-column:1 / 2;grid-row:1 / 2",
    );
    k.apply(
        1,
        3,
        &[Op::SetStyle {
            id: 2,
            patch: Box::new(props(&css_rows(
                "grid-template-columns:100px 1fr;grid-template-rows:100px 1fr",
            ))),
        }],
    )
    .unwrap();
    k.set_host_cover(2, Some(COVER)).unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(frame(&k, 4), [17., 27., 100., 100.]);
    k.apply(
        1,
        4,
        &[Op::SetStyle {
            id: 4,
            patch: Box::new(props(&css_rows("grid-column:2 / auto;grid-row:2 / auto"))),
        }],
    )
    .unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(frame(&k, 4), [117., 127., 243., 775.]);
}

#[test]
fn grid_insets_credit_only_edges_that_use_the_cleared_padding_box() {
    // Chrome 156.0.8078.4: explicit tracks in the inner-box fixture above
    // keep the full inset (62 top, 70 right, 80 bottom, 35 left).
    let cases = [
        (
            "top:env(safe-area-inset-top);grid-row:1 / 2",
            [10., 89.],
            [10., 89.],
        ),
        (
            "top:auto;bottom:env(safe-area-inset-bottom);grid-row:1 / 2",
            [10., 27.],
            [10., 27.],
        ),
        (
            "top:env(safe-area-inset-top);grid-row:auto / 2",
            [10., 62.],
            [10., 62.],
        ),
        (
            "top:auto;bottom:env(safe-area-inset-bottom);grid-row:1 / auto",
            [10., 851.],
            [10., 851.],
        ),
        (
            "top:calc(25% + env(safe-area-inset-top));grid-row:1 / 2",
            [10., 114.],
            [10., 114.],
        ),
        (
            "top:calc(25% + env(safe-area-inset-top));grid-row:auto / 2",
            [10., 88.75],
            [10., 88.75],
        ),
        (
            "left:env(safe-area-inset-left);grid-column:1 / 2",
            [52., 20.],
            [288., 20.],
        ),
        (
            "left:auto;right:env(safe-area-inset-right);grid-column:1 / 2",
            [27., 20.],
            [263., 20.],
        ),
        (
            "left:env(safe-area-inset-left);grid-column:1 / auto",
            [52., 20.],
            [35., 20.],
        ),
        (
            "left:env(safe-area-inset-left);grid-column:auto / 2",
            [35., 20.],
            [288., 20.],
        ),
        (
            "left:auto;right:env(safe-area-inset-right);grid-column:1 / auto",
            [300., 20.],
            [263., 20.],
        ),
        (
            "left:auto;right:env(safe-area-inset-right);grid-column:auto / 2",
            [27., 20.],
            [300., 20.],
        ),
    ];
    let mut failures = Vec::new();
    for direction in ["ltr", "rtl"] {
        for (css, ltr, rtl) in cases {
            let mut k = fixture(
                "grid",
                &format!("top:0;left:0;width:20px;height:20px;{css}"),
            );
            k.apply(
                1,
                3,
                &[Op::SetStyle {
                    id: 2,
                    patch: Box::new(props(&css_rows(&format!(
                        "direction:{direction};grid-template-columns:100px 1fr;grid-template-rows:100px 1fr"
                    )))),
                }],
            )
            .unwrap();
            k.set_env(exact_kernel::Env::new(62., 70., 80., 35.))
                .unwrap();
            k.compute_layout(1, OFFER).unwrap();
            let authored = frame(&k, 4);
            let [x, y] = if direction == "ltr" { ltr } else { rtl };
            for cover in [Some(COVER), None, Some(COVER)] {
                k.set_host_cover(2, cover).unwrap();
                k.compute_layout(1, OFFER).unwrap();
                let want = if cover.is_some() {
                    [x, y, 20., 20.]
                } else {
                    authored
                };
                let got = frame(&k, 4);
                if got != want {
                    failures.push(format!(
                        "{direction}, {css}, {cover:?}: {got:?}, want {want:?}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_calculated_inset_credits_the_cover_after_resolving_its_percentage() {
    let mut k = fixture(
        "flex",
        "left:calc(10% + env(safe-area-inset-left));top:clamp(0px,25%,env(safe-area-inset-top));width:50%;height:50%",
    );
    k.set_env(exact_kernel::Env::new(62., 0., 0., 15.)).unwrap();
    for cover in [Some(COVER), None, Some(COVER)] {
        k.set_host_cover(2, cover).unwrap();
        k.compute_layout(1, OFFER).unwrap();
        let want = if cover.is_some() {
            [50., 62., 175., 441.]
        } else {
            [54., 62., 195., 475.5]
        };
        assert_eq!(frame(&k, 4), want);
    }
}
