//! D13's reach and extension and D14's scroll insets, indicators and edge effects (LLP 1123),
//! lowered to their rows: each form's words and lengths as the kernel holds them, and the values
//! each refuses.

use crate::source::{boxed, NoData, Source};
use exact_kernel::{Dimension, Kernel, SafeAreaEdges, SafeAreaExtend};
use exact_runner::Runner;

fn boot(source: &str) -> Runner<Source> {
    let plan = contract::compile(source).unwrap();
    let plan = contract::bake(plan, boxed(NoData)).unwrap();
    Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn style(r: &mut Runner<Source>, test_id: &str) -> exact_kernel::StyleProps {
    let k = r.kernel_mut();
    let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    k.node(id).unwrap().style.clone()
}

fn points(values: [f32; 4]) -> [Dimension; 4] {
    values.map(Dimension::Points)
}

#[test]
fn the_reach_and_scroll_forms_lower_to_their_rows() {
    let source = "component A\n  view\n    column testId=\"reach\" -exact-safe-area-reach=\"top end\" -exact-safe-area-extend=\"start bottom\" -exact-safe-area-extend-bounds=\"1 2 3 4\"\n      column testId=\"scroller\" overflow-y=\"scroll\" -exact-scroll-content-inset=\"10 -20 30 40\" -exact-scroll-inset-adjustment=\"never\" -exact-scroll-vertical-indicator-insets=\"1 2 3\" -exact-scroll-horizontal-indicator-insets=\"5 6 7 8\" -exact-scroll-indicator-adjustment=\"none\" -exact-scroll-edge-effect-style=\"top start hard\" -exact-scroll-edge-effect-hidden=\"bottom end\"\n      column testId=\"soft\" overflow-y=\"scroll\" -exact-scroll-edge-effect-style=\"left soft\"\n      column testId=\"none\" -exact-safe-area-reach=\"none\" -exact-safe-area-extend=\"none\"\n      column testId=\"auto\" -exact-safe-area-reach=\"auto\"\n      column testId=\"plain\"\n";
    let mut r = boot(source);
    let [reach, scroller, soft, none, auto, plain] =
        ["reach", "scroller", "soft", "none", "auto", "plain"].map(|t| style(&mut r, t));
    // D13: reach on its edges, `none` as `edges` naming none, a written `auto` as the initial
    // value; the extension's edges; and its bounds, `auto` where unsaid
    assert_eq!(format!("{:?}", reach.rare.safe_area_reach), "Edges");
    assert_eq!(
        SafeAreaEdges::css(reach.rare.safe_area_reach_edges),
        "top end"
    );
    assert_eq!(format!("{:?}", none.rare.safe_area_reach), "Edges");
    assert_eq!(none.rare.safe_area_reach_edges, 0);
    assert_eq!(format!("{:?}", auto.rare.safe_area_reach), "Auto");
    assert_eq!(format!("{:?}", plain.rare.safe_area_reach), "Auto");
    // Each edge is one bit, top bit 0 to end bit 5: `top end` is bits 0 and 5
    assert_eq!(reach.rare.safe_area_reach_edges, 0b100001);
    assert_eq!(
        SafeAreaExtend::css(reach.rare.safe_area_extend),
        "bottom start"
    );
    assert_eq!(SafeAreaExtend::css(none.rare.safe_area_extend), "none");
    let b = &reach.rare;
    assert_eq!(
        [
            b.safe_area_extend_bounds_top.clone(),
            b.safe_area_extend_bounds_right.clone(),
            b.safe_area_extend_bounds_bottom.clone(),
            b.safe_area_extend_bounds_left.clone()
        ],
        points([1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(plain.rare.safe_area_extend_bounds_top, Dimension::Auto);
    // D14: the content inset, a negative edge held, and both indicators edge by edge; the two
    // policies; the edge effects' soft, hard and hidden edges
    let s = &scroller.rare;
    assert_eq!(
        [
            s.scroll_content_inset_top.clone(),
            s.scroll_content_inset_right.clone(),
            s.scroll_content_inset_bottom.clone(),
            s.scroll_content_inset_left.clone()
        ],
        points([10.0, -20.0, 30.0, 40.0])
    );
    assert_eq!(
        [
            s.scroll_vertical_indicator_insets_top.clone(),
            s.scroll_vertical_indicator_insets_right.clone(),
            s.scroll_vertical_indicator_insets_bottom.clone(),
            s.scroll_vertical_indicator_insets_left.clone()
        ],
        points([1.0, 2.0, 3.0, 2.0])
    );
    assert_eq!(
        [
            s.scroll_horizontal_indicator_insets_top.clone(),
            s.scroll_horizontal_indicator_insets_right.clone(),
            s.scroll_horizontal_indicator_insets_bottom.clone(),
            s.scroll_horizontal_indicator_insets_left.clone()
        ],
        points([5.0, 6.0, 7.0, 8.0])
    );
    assert_eq!(format!("{:?}", s.scroll_inset_adjustment), "Never");
    assert_eq!(format!("{:?}", s.scroll_indicator_adjustment), "None");
    assert_eq!(SafeAreaEdges::css(s.scroll_edge_effect_hard), "top start");
    assert_eq!(s.scroll_edge_effect_soft, 0);
    assert_eq!(
        SafeAreaEdges::css(soft.rare.scroll_edge_effect_soft),
        "left"
    );
    assert_eq!(soft.rare.scroll_edge_effect_hard, 0);
    assert_eq!(
        SafeAreaEdges::css(s.scroll_edge_effect_hidden),
        "bottom end"
    );
    // A box that says nothing has the two policies' initial values
    let p = &plain.rare;
    assert_eq!(format!("{:?}", p.scroll_inset_adjustment), "Automatic");
    assert_eq!(format!("{:?}", p.scroll_indicator_adjustment), "Auto");
}

#[test]
fn an_edge_effect_list_holds_a_style_per_edge_and_a_written_initial_value_is_set() {
    let source = "component A\n  view\n    column\n      column testId=\"list\" overflow-y=\"scroll\" -exact-scroll-edge-effect-style=\"top start soft, bottom hard\" -exact-safe-area-extend-bounds=\"auto 8\"\n      column testId=\"automatic\" overflow-y=\"scroll\" -exact-scroll-edge-effect-style=\"top automatic\"\n      column testId=\"unset\" -exact-safe-area-reach=\"unset\" -exact-safe-area-extend=\"unset\" -exact-scroll-edge-effect-style=\"unset\"\n";
    let mut r = boot(source);
    let [list, automatic, unset] = ["list", "automatic", "unset"].map(|t| style(&mut r, t));
    // Two styles in one declaration, each on its own edges
    assert_eq!(
        SafeAreaEdges::css(list.rare.scroll_edge_effect_soft),
        "top start"
    );
    assert_eq!(
        SafeAreaEdges::css(list.rare.scroll_edge_effect_hard),
        "bottom"
    );
    // `automatic` written sets both rows to no edge, which the old row's transition reads as set
    assert_eq!(
        (
            automatic.rare.scroll_edge_effect_soft,
            automatic.rare.scroll_edge_effect_hard
        ),
        (0, 0)
    );
    assert!(automatic
        .mask
        .has(exact_kernel::StyleId::ScrollEdgeEffectSoft));
    assert!(automatic
        .mask
        .has(exact_kernel::StyleId::ScrollEdgeEffectHard));
    // A written `auto` bound beside a length
    let b = &list.rare;
    assert_eq!(
        [
            b.safe_area_extend_bounds_top.clone(),
            b.safe_area_extend_bounds_right.clone(),
            b.safe_area_extend_bounds_bottom.clone(),
            b.safe_area_extend_bounds_left.clone()
        ],
        [
            Dimension::Auto,
            Dimension::Points(8.0),
            Dimension::Auto,
            Dimension::Points(8.0)
        ]
    );
    // `unset` clears each shorthand's rows to their initial values
    assert_eq!(format!("{:?}", unset.rare.safe_area_reach), "Auto");
    assert_eq!(SafeAreaExtend::css(unset.rare.safe_area_extend), "none");
    // An authored `unset` leaves the style form unset, which the old row's transition reads
    assert!(!unset.mask.has(exact_kernel::StyleId::ScrollEdgeEffectSoft));
    assert!(!unset.mask.has(exact_kernel::StyleId::ScrollEdgeEffectHard));
}

#[test]
fn a_malformed_reach_or_scroll_form_is_refused_saying_what_to_write() {
    for (attr, says) in [
        // The edge effect's style: a list of edges, each named once across it, each item ending in
        // a style word
        (
            "-exact-scroll-edge-effect-style=\"soft\"",
            "a list of edges",
        ),
        (
            "-exact-scroll-edge-effect-style=\"none soft\"",
            "a list of edges",
        ),
        (
            "-exact-scroll-edge-effect-style=\"top top soft\"",
            "each named once",
        ),
        (
            "-exact-scroll-edge-effect-style=\"top soft, top hard\"",
            "each named once",
        ),
        (
            "-exact-scroll-edge-effect-style=\"top soft,\"",
            "each named once",
        ),
        // `automatic` on a logical edge beside a style on a physical one, which the rows cannot hold
        (
            "-exact-scroll-edge-effect-style=\"left hard, start automatic\"",
            "cannot hold `automatic`",
        ),
        (
            "-exact-scroll-edge-effect-style=\"top none\"",
            "`automatic`, `soft` or `hard`",
        ),
        // The hidden form's edges, each once, or `none` alone
        (
            "-exact-scroll-edge-effect-hidden=\"top top\"",
            "each named once",
        ),
        (
            "-exact-scroll-edge-effect-hidden=\"none top\"",
            "each named once",
        ),
        // Reach stands alone as `auto` or `none`, or names its edges
        (
            "-exact-safe-area-reach=\"sideways\"",
            "the edges it reaches on",
        ),
        ("-exact-safe-area-reach=\"auto top\"", "`auto`, `none`"),
        // The host places an extension by hand, so no placement word is taken
        (
            "-exact-safe-area-extend=\"top manual\"",
            "takes no `manual`",
        ),
        (
            "-exact-safe-area-extend=\"none top\"",
            "`none`, or the edges",
        ),
        // A policy word, five lengths, a percentage and a negative bound
        ("-exact-scroll-inset-adjustment=\"sometimes\"", "automatic"),
        ("-exact-scroll-indicator-adjustment=\"always\"", "auto"),
        (
            "-exact-scroll-content-inset=\"1 2 3 4 5\"",
            "one to four values",
        ),
        (
            "-exact-scroll-content-inset=\"10%\"",
            "a length: no percentage",
        ),
        ("-exact-safe-area-extend-bounds=\"10%\"", "no percentage"),
        ("-exact-safe-area-extend-bounds=-4", "nonnegative length ("),
    ] {
        let source = format!("component A\n  view\n    column overflow-y=\"scroll\" {attr}\n");
        let refused = contract::compile(&source)
            .err()
            .unwrap_or_else(|| panic!("{attr} compiled"));
        assert!(
            refused.message.contains(says),
            "{attr}: {}",
            refused.message
        );
    }
}

#[test]
fn vocab_prints_each_forms_grammar_under_its_rows() {
    // The rows show their storage, the edge sets, and not the grammar an author writes
    for (name, says) in [
        ("-exact-safe-area-reach", "`auto` (the initial value"),
        ("-exact-safe-area-extend", "Mirrors the box's picture"),
        ("-exact-scroll-edge-effect-style", "`top soft, bottom hard`"),
        (
            "-exact-scroll-edge-effect-hidden",
            "whose scroll edge effect is hidden",
        ),
    ] {
        let out = std::process::Command::new(env!("CARGO_BIN_EXE_contract"))
            .args(["vocab", name])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && text.contains(says),
            "{name}: {text}"
        );
    }
}

#[test]
fn a_scrollers_node_reply_carries_its_terms() {
    // @ref LLP 1123 D13, D14 — a document under the session's 47-point top band, padded clear of
    // it: its scroller, at the band's inner side, reaches it and observes the container region
    let source = "component A\n  view\n    column\n      column padding-top=47\n        column testId=\"list\" overflow-y=\"scroll\" height=300\n          text \"a\"\n      column testId=\"plain\"\n";
    let mut r = boot(source);
    let root = r.roots()[0];
    let k = r.kernel_mut();
    k.set_session_band([47.0, 0.0, 34.0, 0.0]).unwrap();
    k.compute_layout(root, exact_kernel::Offer::definite(390.0, 844.0))
        .unwrap();
    let id = |r: &Runner<Source>, t: &str| {
        r.kernel()
            .node_by_key(r.kernel().find_by_test_id(t)[0])
            .unwrap()
            .id
    };
    let list = exact_runner::agent::node(&r, id(&r, "list"));
    assert!(
        list.contains("\"scrollTerms\":{\"reach\":[47,0,0,0],\"reserved\":[null,null,null,null],\"keyboard\":[0,0,0,0],\"bandReaches\":[true,true,true,true],\"container\":[true,true,true,true]}"),
        "{list}"
    );
    assert!(!exact_runner::agent::node(&r, id(&r, "plain")).contains("scrollTerms"));
}

#[test]
fn an_extending_boxs_node_reply_carries_its_extension() {
    // @ref LLP 1123 D13 — a box at the inner side of the session's 47-point top band extends on its
    // top: `auto` goes as far as reach would, the band's 47; a box that extends nothing has none
    let source = "component A\n  view\n    column\n      column padding-top=47\n        column testId=\"hero\" height=100 -exact-safe-area-extend=\"top\"\n      column testId=\"plain\"\n";
    let mut r = boot(source);
    let root = r.roots()[0];
    let k = r.kernel_mut();
    k.set_session_band([47.0, 0.0, 34.0, 0.0]).unwrap();
    k.compute_layout(root, exact_kernel::Offer::definite(390.0, 844.0))
        .unwrap();
    let id = |r: &Runner<Source>, t: &str| {
        r.kernel()
            .node_by_key(r.kernel().find_by_test_id(t)[0])
            .unwrap()
            .id
    };
    let hero = exact_runner::agent::node(&r, id(&r, "hero"));
    assert!(hero.contains("\"extension\":[47,0,0,0]"), "{hero}");
    assert!(!exact_runner::agent::node(&r, id(&r, "plain")).contains("\"extension\""));
}
