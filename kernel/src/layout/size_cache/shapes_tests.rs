//! Size-cache bounds and policy comparisons on the shapes in LLP 1165 D4.
use super::cost_probe::{children, n, style, t, text, view};
use super::{assert_same, SizeCachePolicy};
use crate::{Kernel, NodeType, Offer, Op, PropId, StyleId};

const WORDS: &str = "A wrapping text with several words at the bottom of the chain.";
const KINDS: [&str; 10] = [
    "stretch",
    "start",
    "baseline",
    "wrap",
    "baseline-rows",
    "ratio",
    "percent",
    "fifty",
    "grid",
    "table",
];

fn shape(kind: &str, depth: u32, padded: bool, definite: bool, three: bool) -> Vec<Op> {
    let mut ops = Vec::new();
    ops.extend(text(depth + 1, WORDS));
    for level in (0..depth).rev() {
        let id = level + 1;
        let row = level % 2 == 0 || kind == "baseline-rows";
        ops.push(view(id));
        let mut styles = vec![
            (StyleId::Display, t("flex")),
            (
                StyleId::FlexDirection,
                t(if row { "row" } else { "column" }),
            ),
        ];
        if padded {
            for side in [
                StyleId::PaddingTop,
                StyleId::PaddingRight,
                StyleId::PaddingBottom,
                StyleId::PaddingLeft,
            ] {
                styles.push((side, n((level % 3 + 1) as f64)));
            }
        }
        if kind == "start" {
            styles.push((StyleId::AlignItems, t("flex-start")));
        }
        if kind.starts_with("baseline") && row {
            styles.push((StyleId::AlignItems, t("baseline")));
        }
        if kind == "wrap" && row {
            styles.push((StyleId::FlexWrap, t("wrap")));
        }
        if kind == "ratio" {
            styles.push((StyleId::AspectRatio, n(1.5)));
        }
        if kind == "percent" {
            styles.extend([
                (StyleId::Width, crate::StyleValue::Percent(95.0)),
                (StyleId::Height, crate::StyleValue::Percent(90.0)),
            ]);
        }
        if level == 0 && definite {
            styles.extend([(StyleId::Width, n(390.0)), (StyleId::Height, n(844.0))]);
        }
        if kind == "grid" && level == depth / 2 {
            styles.push((StyleId::Display, t("grid")));
        }
        let mut kids = vec![id + 1];
        let siblings = if kind == "fifty" && level == depth / 2 {
            styles.extend([
                (StyleId::FlexDirection, t("row")),
                (StyleId::FlexWrap, t("wrap")),
            ]);
            49
        } else if three {
            2
        } else if kind.starts_with("baseline") && row {
            1
        } else {
            0
        };
        for sibling in 0..siblings {
            let leaf = 1_000 + level * 50 + sibling;
            ops.extend(text(leaf, "x"));
            kids.push(leaf);
        }
        if kind == "table" && (depth / 2 - 1..depth / 2 + 2).contains(&level) {
            let at = (level + 1 - depth / 2) as usize;
            styles.push((StyleId::Display, t("block")));
            ops.push(Op::SetProp {
                id,
                prop: PropId::SemanticTag,
                value: ["table", "tr", "td"][at].into(),
            });
            if at < 2 {
                kids.truncate(1);
            }
        }
        ops.extend([style(id, &styles), children(id, kids)]);
    }
    ops.push(Op::AttachRoot { id: 1 });
    ops
}

fn bounded(kind: &str, k: &Kernel) {
    assert_eq!(
        k.size_cache_counts().repeated,
        0,
        "{kind}: {:?}",
        k.size_cache_counts()
    );
    let tree = &k.tree().taffy;
    for (node, answers) in tree.size_cache_peaks() {
        let mut above = 0;
        let mut parent = tree.parent(node);
        while let Some(p) = parent {
            above += 1;
            parent = tree.parent(p);
        }
        let bound = 64 + 8 * above;
        assert!(answers <= bound, "LLP 1165 alarm: {kind} node {node:?}, {above} levels above, {answers} answers, bound {bound}");
    }
}

fn run(depths: &[u32], compare: bool, variants: bool) {
    let mut failures = Vec::new();
    for kind in KINDS {
        for &depth in depths {
            for padded in [false, true].into_iter().take(if variants { 2 } else { 1 }) {
                for definite in [false, true].into_iter().take(if variants { 2 } else { 1 }) {
                    for three in [false, true].into_iter().take(if variants { 2 } else { 1 }) {
                        let result = std::panic::catch_unwind(|| {
                            let ops = shape(kind, depth, padded, definite, three);
                            let mut kernels = [
                                SizeCachePolicy::All,
                                SizeCachePolicy::Nine,
                                SizeCachePolicy::Off,
                            ]
                            .into_iter()
                            .take(if compare { 3 } else { 1 })
                            .map(|policy| {
                                let mut k = Kernel::with_monospace();
                                k.apply(0, 1, &ops).unwrap();
                                k.set_size_cache_policy(policy);
                                k
                            })
                            .collect::<Vec<_>>();
                            for width in
                                [390.0, 430.0, 390.0]
                                    .into_iter()
                                    .take(if compare { 3 } else { 1 })
                            {
                                for k in &mut kernels {
                                    k.compute_layout(1, Offer::definite(width, 844.0)).unwrap();
                                }
                                eprintln!("kernel shape {kind} depth {depth} padded {padded} definite {definite} three {three} width {width}: {:?}", kernels[0].size_cache_counts());
                                bounded(kind, &kernels[0]);
                                if compare {
                                    eprintln!("All versus Off");
                                    assert_same(&kernels[0], &kernels[2]);
                                    eprintln!("All versus Nine");
                                    assert_same(&kernels[0], &kernels[1]);
                                }
                            }
                        });
                        if result.is_err() {
                            failures.push(format!("{kind} depth {depth} padded {padded} definite {definite} three {three}"));
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "failed shapes: {failures:#?}");
}

#[test]
fn size_cache_shape_slice() {
    run(&[4], true, false);
}

#[test]
#[ignore = "async lane: small shapes under All, Nine and Off"]
fn size_cache_small_shapes() {
    run(&[4, 8], true, true);
}

#[test]
#[ignore = "async lane: size-answer bounds through 128 levels"]
fn size_cache_deep_shapes() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| run(&[8, 16, 32, 64, 128], false, true))
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn size_cache_directed_leaves() {
    for kind in ["exclusion", "field", "button", "boundary"] {
        let mut ops = super::cost_probe::chain("RC", 8, WORDS);
        let leaf = 9;
        match kind {
            "field" | "button" => {
                ops.retain(|op| !matches!(op, Op::CreateView { id: 9, .. }));
                ops.insert(
                    0,
                    Op::CreateView {
                        id: leaf,
                        node_type: if kind == "field" {
                            NodeType::TextInput
                        } else {
                            NodeType::Control
                        },
                    },
                );
                if kind == "button" {
                    ops.push(Op::SetProp {
                        id: leaf,
                        prop: PropId::Type,
                        value: "button".into(),
                    });
                }
            }
            _ => {
                ops.retain(|op| {
                    !matches!(op, Op::CreateView { id: 9, .. } | Op::SetProp { id: 9, .. })
                });
                ops.insert(0, view(leaf));
                ops.extend(text(100, WORDS));
                let mut kids = vec![100];
                if kind == "boundary" {
                    ops.push(style(
                        leaf,
                        &[
                            (StyleId::Width, n(180.0)),
                            (StyleId::Height, n(80.0)),
                            (StyleId::OverflowX, t("hidden")),
                            (StyleId::OverflowY, t("hidden")),
                        ],
                    ));
                } else {
                    ops.push(view(101));
                    ops.push(style(leaf, &[(StyleId::PositionType, t("relative"))]));
                    ops.push(style(
                        101,
                        &[
                            (StyleId::PositionType, t("absolute")),
                            (StyleId::WrapFlow, t("both")),
                            (StyleId::ShapeOutside, t("inset(0)")),
                            (StyleId::Left, n(0.0)),
                            (StyleId::Top, n(0.0)),
                            (StyleId::Width, n(40.0)),
                            (StyleId::Height, n(40.0)),
                        ],
                    ));
                    kids.insert(0, 101);
                }
                ops.push(children(leaf, kids));
            }
        }
        let mut kernels = [
            SizeCachePolicy::All,
            SizeCachePolicy::Nine,
            SizeCachePolicy::Off,
        ]
        .map(|policy| {
            let mut k = Kernel::new(Box::new(Chrome));
            k.apply(0, 1, &ops).unwrap();
            k.set_size_cache_policy(policy);
            k
        });
        let mut most = 0;
        for edited in [false, true] {
            if edited {
                for k in &mut kernels {
                    k.apply(
                        0,
                        2,
                        &[Op::SetProp {
                            id: if kind == "boundary" || kind == "exclusion" {
                                100
                            } else {
                                leaf
                            },
                            prop: PropId::Text,
                            value: "A changed text, still wrapping, with another length.".into(),
                        }],
                    )
                    .unwrap();
                }
            }
            for width in [390.0, 430.0, 390.0] {
                for k in &mut kernels {
                    k.compute_layout(1, Offer::definite(width, 844.0)).unwrap();
                }
                most = most.max(kernels[0].size_cache_counts().max_answers);
                assert_same(&kernels[0], &kernels[2]);
                assert_same(&kernels[0], &kernels[1]);
                bounded(kind, &kernels[0]);
            }
        }
        assert!(most > 9, "{kind} did not exceed nine answers: {most}");
    }
}

struct Chrome;
impl crate::TextMeasurer for Chrome {
    fn measure(&mut self, request: &crate::TextMeasureRequest<'_>) -> crate::TextMetrics {
        crate::MonospaceMeasurer::default().measure(request)
    }
    fn field_chrome(&mut self, _: &crate::FieldChromeRequest) -> crate::FieldChrome {
        crate::FieldChrome {
            minimum_height: 32.0,
            provisional: true,
            ..Default::default()
        }
    }
    fn button_measure(
        &mut self,
        request: &crate::ButtonMeasureRequest,
    ) -> Option<crate::ButtonMeasure> {
        Some(crate::ButtonMeasure {
            width: match request.width {
                crate::AxisOffer::Definite(w) => w,
                crate::AxisOffer::MinContent => 40.0,
                crate::AxisOffer::MaxContent => 180.0,
            },
            height: 32.0,
            provisional: true,
        })
    }
}

#[test]
fn size_cache_baseline_resize_equals_off() {
    let ops = shape("baseline", 8, false, false, true);
    let mut kernels = [SizeCachePolicy::All, SizeCachePolicy::Off].map(|policy| {
        let mut k = Kernel::with_monospace();
        k.apply(0, 1, &ops).unwrap();
        k.set_size_cache_policy(policy);
        k
    });
    for width in [390.0, 430.0, 390.0] {
        for k in &mut kernels {
            k.compute_layout(1, Offer::definite(width, 844.0)).unwrap();
        }
        assert_same(&kernels[0], &kernels[1]);
    }
}
