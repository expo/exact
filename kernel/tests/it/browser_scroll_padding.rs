//! A scroll container's end padding against literal Chrome.
//!
//! Chrome 155, 2026-10-24: each scroller is `width: 200px; height: 100px;
//! overflow: auto; padding: 10px`, and the expected size is its
//! `scrollWidth` × `scrollHeight` (the padding box is 220 × 120). The end
//! padding extends the margin boxes of the scroller's in-flow children, not
//! the overflow of their descendants (CSS Overflow 3 §2.2, "scrollable
//! overflow"): a descendant reaching the padding edge, as a full-bleed
//! hairline in a padded list does, leaves nothing to scroll.
use crate::browser_cases::{css_rows, lay_out_with, props};
use crate::support::reader::text as t;
use exact_kernel::StyleId::{OverflowX, OverflowY};
use exact_kernel::{Kernel, NodeType, Offer, Op, PropId, PropValue};

// `overflow: auto` is added as rows: `css_rows` reads `auto` as a length.
const SCROLLER: &str = "width:200px;height:100px;padding:10px;";

/// A case: its name, the scroller's own declarations after [`SCROLLER`],
/// its descendants as `(id, parent, css)` (the scroller is 2), and Chrome's
/// `scrollWidth` × `scrollHeight`.
type Case = (
    &'static str,
    &'static str,
    &'static [(u32, u32, &'static str)],
    (f32, f32),
);

const CASES: &[Case] = &[
    (
        "block: a child bleeding into the side padding",
        "",
        &[(3, 2, "margin-left:-10px;margin-right:-10px;height:20px")],
        (220.0, 120.0),
    ),
    (
        "block: a grandchild bleeding to the padding edge",
        "",
        &[
            (3, 2, "height:20px"),
            (4, 3, "margin-left:-10px;width:220px;height:20px"),
        ],
        (220.0, 120.0),
    ),
    (
        "block: the last child's bottom margin, then the padding",
        "",
        &[(3, 2, "height:150px;margin-bottom:20px")],
        (220.0, 190.0),
    ),
    (
        "block: a grandchild overflowing its parent, with no padding after it",
        "",
        &[(3, 2, "height:50px"), (4, 3, "height:200px")],
        (220.0, 210.0),
    ),
    (
        "block: a wide child's right margin, then the padding",
        "",
        &[(3, 2, "width:300px;margin-right:15px;height:10px")],
        (335.0, 120.0),
    ),
    (
        "flex column: the last item's bottom margin, then the padding",
        "display:flex;flex-direction:column",
        &[(3, 2, "height:150px;flex-shrink:0;margin-bottom:20px")],
        (220.0, 190.0),
    ),
    (
        "flex column: a grandchild overflowing its item, with no padding after it",
        "display:flex;flex-direction:column",
        &[(3, 2, "height:50px;flex-shrink:0"), (4, 3, "height:200px")],
        (220.0, 210.0),
    ),
    (
        "flex column: a full-bleed hairline in an item",
        "display:flex;flex-direction:column",
        &[
            (3, 2, ""),
            (4, 3, "margin-left:-10px;margin-right:-10px;height:1px"),
        ],
        (220.0, 120.0),
    ),
    (
        "flex column, rtl: a full-bleed hairline in an item",
        "display:flex;flex-direction:column;direction:rtl",
        &[
            (3, 2, ""),
            (4, 3, "margin-left:-10px;margin-right:-10px;height:1px"),
        ],
        (220.0, 120.0),
    ),
    (
        "flex column: a grandchild 30 past the padding edge",
        "display:flex;flex-direction:column",
        &[(3, 2, ""), (4, 3, "margin-right:-40px;height:1px")],
        (250.0, 120.0),
    ),
    (
        "flex row: a wide item's right margin, then the padding",
        "display:flex",
        &[(
            3,
            2,
            "width:300px;flex-shrink:0;margin-right:15px;height:10px",
        )],
        (335.0, 120.0),
    ),
    (
        "grid: a full-bleed hairline in an item",
        "display:grid",
        &[
            (3, 2, ""),
            (4, 3, "margin-left:-10px;margin-right:-10px;height:1px"),
        ],
        (220.0, 120.0),
    ),
    (
        "grid: an item's right margin in a 300px column, then the padding",
        "display:grid;grid-template-columns:300px",
        &[(3, 2, "margin-right:15px;height:10px")],
        (320.0, 120.0),
    ),
    // A relative offset moves the box, not where the padding is added.
    (
        "block: a child moved by a relative offset",
        "",
        &[(3, 2, RELATIVE)],
        (240.0, 120.0),
    ),
    (
        "flex column: an item moved by a relative offset",
        "display:flex;flex-direction:column",
        &[(
            3,
            2,
            "flex-shrink:0;position:relative;left:30px;top:-5px;width:200px;height:100px",
        )],
        (240.0, 120.0),
    ),
    (
        "grid: an item moved by a relative offset",
        "display:grid",
        &[(3, 2, RELATIVE)],
        (240.0, 120.0),
    ),
    (
        "flex column, rtl: an item moved by a relative offset",
        "display:flex;flex-direction:column;direction:rtl",
        &[(
            3,
            2,
            "flex-shrink:0;position:relative;right:30px;top:-5px;width:200px;height:100px",
        )],
        (240.0, 120.0),
    ),
];

const RELATIVE: &str = "position:relative;left:30px;top:-5px;width:200px;height:100px";

#[test]
fn end_padding_extends_children_not_descendants() {
    let mut failures = Vec::new();
    for (name, own, nodes, (width, height)) in CASES {
        let mut scroller = css_rows(&format!("{SCROLLER}{own}"));
        scroller.extend([(OverflowX, t("auto")), (OverflowY, t("auto"))]);
        let mut all = vec![(2, 1, scroller)];
        all.extend(
            nodes
                .iter()
                .map(|(id, parent, css)| (*id, *parent, css_rows(css))),
        );
        let k = lay_out_with(props(&css_rows("width:800px")), all, &[], &[]);
        let (w, h) = k.node(2).unwrap().content;
        // scrollWidth and scrollHeight are at least the padding box.
        let got = (w.max(220.0), h.max(120.0));
        if (got.0 - width).abs() > 0.01 || (got.1 - height).abs() > 0.01 {
            failures.push(format!("{name}: {got:?}, Chrome {width} × {height}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A form control's natural size is not scrollable overflow: a control
/// narrower than it adds its border box to its scroller, as any box does,
/// and nothing past it (CSS Overflow 3 §2.1). Each case is one control, the
/// only child of the scroller above made a flex column, its CSS, and
/// Chrome's `scrollWidth`. Chrome for Testing 159 (macOS headless shell),
/// 2026-11-24, plain HTML: a `progress` (`value=0.5`) is `10em` by `1em`,
/// a fixed `input` twenty characters wide and a `textarea` twenty by two,
/// all wider than the scroller at a 60px font, as the kernel's measures are.
const CONTROLS: &[(&str, NodeType, &str, &str, f32)] = &[
    (
        "a full-width progress",
        NodeType::Control,
        "progress",
        "width:100%;min-width:0",
        220.0,
    ),
    (
        "a progress given a narrow width",
        NodeType::Control,
        "progress",
        "width:100px",
        220.0,
    ),
    (
        "a full-width text field",
        NodeType::TextInput,
        "text",
        "width:100%;min-width:0;box-sizing:border-box",
        220.0,
    ),
    (
        "a full-width textarea",
        NodeType::TextInput,
        "textarea",
        "width:100%;min-width:0;box-sizing:border-box",
        220.0,
    ),
    // `width: 100%` sizes the content box: the border box sticks out 12px.
    (
        "a full-width content-box field with padding and a border",
        NodeType::TextInput,
        "text",
        "width:100%;min-width:0;box-sizing:content-box;padding-left:4px;padding-right:4px;border-width:2px;border-style:solid",
        232.0,
    ),
];

#[test]
fn a_form_controls_natural_width_does_not_widen_its_scroller() {
    let mut failures = Vec::new();
    for (name, kind, ty, css, width) in CONTROLS {
        let mut scroller = css_rows(&format!("{SCROLLER}display:flex;flex-direction:column"));
        scroller.extend([(OverflowX, t("auto")), (OverflowY, t("auto"))]);
        let control = css_rows(&format!("{css};font-size:60px"));
        let mut ops = vec![
            Op::CreateView {
                id: 1,
                node_type: NodeType::View,
            },
            Op::SetStyle {
                id: 1,
                patch: Box::new(props(&css_rows("width:800px"))),
            },
            Op::CreateView {
                id: 2,
                node_type: NodeType::View,
            },
            Op::SetStyle {
                id: 2,
                patch: Box::new(props(&scroller)),
            },
            Op::CreateView {
                id: 3,
                node_type: *kind,
            },
            Op::SetStyle {
                id: 3,
                patch: Box::new(props(&control)),
            },
        ];
        let tag = |prop, value: &str| Op::SetProp {
            id: 3,
            prop,
            value: PropValue::Str(value.into()),
        };
        ops.push(match *ty {
            "textarea" => tag(PropId::SemanticTag, "textarea"),
            _ => tag(PropId::Type, ty),
        });
        if *ty == "progress" {
            ops.push(tag(PropId::Value, "0.5"));
        }
        ops.extend([
            Op::SetChildren {
                id: 2,
                children: vec![3],
            },
            Op::SetChildren {
                id: 1,
                children: vec![2],
            },
            Op::AttachRoot { id: 1 },
        ]);
        let mut k = Kernel::with_monospace();
        k.apply(0, 1, &ops).unwrap();
        k.compute_layout(1, Offer::definite(800.0, 600.0)).unwrap();
        let used = k.node(3).unwrap().frame.width;
        let got = k.node(2).unwrap().content.0.max(220.0);
        if (got - width).abs() > 0.01 {
            failures.push(format!(
                "{name} (a {used} box): scrollWidth {got}, Chrome {width}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
