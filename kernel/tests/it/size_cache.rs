//! EXACT PATCH 72: stored size answers and the questions they answer.

use taffy::prelude::*;
use taffy::tree::{LayoutInput, LayoutOutput, RunMode, SizeCachePolicy, SizingMode};
use taffy::{compute_leaf_layout, RequestedAxis};

fn input() -> LayoutInput {
    LayoutInput {
        run_mode: RunMode::ComputeSize,
        sizing_mode: SizingMode::ContentSize,
        axis: RequestedAxis::Both,
        known_dimensions: Size::NONE,
        known_dimensions_are_definite: Size {
            width: true,
            height: true,
        },
        parent_size: Size::NONE,
        available_space: Size::MAX_CONTENT,
        vertical_margins_are_collapsible: Line::FALSE,
    }
}

fn layout_bits(tree: &TaffyTree, node: NodeId) -> Vec<u32> {
    let l = tree.unrounded_layout(node);
    let mut bits = vec![l.order];
    bits.extend(
        [
            l.location.x,
            l.location.y,
            l.size.width,
            l.size.height,
            l.scrollable_overflow_rect.left,
            l.scrollable_overflow_rect.right,
            l.scrollable_overflow_rect.top,
            l.scrollable_overflow_rect.bottom,
            l.scrollbar_size.width,
            l.scrollbar_size.height,
            l.border.left,
            l.border.right,
            l.border.top,
            l.border.bottom,
            l.padding.left,
            l.padding.right,
            l.padding.top,
            l.padding.bottom,
            l.margin.left,
            l.margin.right,
            l.margin.top,
            l.margin.bottom,
        ]
        .map(f32::to_bits),
    );
    bits
}

fn measure(input: LayoutInput, _: NodeId, _: Option<&mut ()>, style: &Style) -> LayoutOutput {
    compute_leaf_layout(
        input,
        style,
        |_, _| 0.0,
        |known, space| {
            let width = known.width.unwrap_or(match space.width {
                AvailableSpace::Definite(w) => w.min(180.0),
                AvailableSpace::MinContent => 30.0,
                AvailableSpace::MaxContent => 180.0,
            });
            Size {
                width,
                height: known
                    .height
                    .unwrap_or((180.0 / width.max(1.0)).ceil() * 10.0),
            }
        },
    )
}

#[test]
fn the_nine_entry_policy_counts_repeated_size_questions() {
    let mut tree = TaffyTree::new();
    tree.track_size_computations(true);
    tree.set_size_cache_policy(SizeCachePolicy::Nine);
    let mut node = tree.new_leaf(Style::default()).unwrap();
    for level in 0..8 {
        node = tree
            .new_with_children(
                Style {
                    flex_direction: if level % 2 == 0 {
                        FlexDirection::Row
                    } else {
                        FlexDirection::Column
                    },
                    align_items: Some(AlignItems::FLEX_START),
                    ..Style::default()
                },
                &[node],
            )
            .unwrap();
    }
    tree.compute_layout_with_measure(
        node,
        Size {
            width: AvailableSpace::Definite(100.0),
            height: AvailableSpace::Definite(200.0),
        },
        measure,
    )
    .unwrap();
    let counts = tree.size_cache_counts();
    eprintln!("nine-entry baseline: {counts:?}");
    assert_eq!(counts.passes, 1);
    assert_eq!(counts.max_answers, 9);
    assert!(counts.repeated > 0);
    assert!(counts.computed > counts.repeated);
}

#[test]
fn a_ratio_leaf_answers_only_the_requested_axis() {
    let mut tree = TaffyTree::new();
    let node = tree
        .new_leaf(Style {
            aspect_ratio: Some(2.0),
            max_size: Size {
                width: LengthPercentageAuto::auto(),
                height: LengthPercentageAuto::length(20.0),
            },
            ..Style::default()
        })
        .unwrap();
    tree.compute_boundary_with_measure(node, input(), measure);
    let horizontal = LayoutInput {
        axis: RequestedAxis::Horizontal,
        ..input()
    };
    let cached = tree.compute_boundary_with_measure(node, horizontal, measure);
    tree.set_size_cache_policy(SizeCachePolicy::Off);
    let computed = tree.compute_boundary_with_measure(node, horizontal, measure);
    assert_eq!(cached.size.width, computed.size.width);
}

#[test]
fn an_indefinite_known_width_keeps_the_available_space() {
    for ratio in [None, Some(2.0)] {
        for percent in [false, true] {
            for mode in [RunMode::ComputeSize, RunMode::PerformLayout] {
                let mut tree = TaffyTree::new();
                let leaf = tree
                    .new_leaf(Style {
                        size: Size {
                            width: if percent {
                                Dimension::percent(0.5)
                            } else {
                                Dimension::auto()
                            },
                            height: if percent {
                                Dimension::percent(1.0)
                            } else {
                                Dimension::auto()
                            },
                        },
                        ..Style::default()
                    })
                    .unwrap();
                let child = tree
                    .new_with_children(
                        Style {
                            aspect_ratio: ratio,
                            ..Style::default()
                        },
                        &[leaf],
                    )
                    .unwrap();
                let root = tree
                    .new_with_children(
                        Style {
                            flex_direction: FlexDirection::Column,
                            align_items: Some(AlignItems::FLEX_START),
                            ..Style::default()
                        },
                        &[child],
                    )
                    .unwrap();
                let mut fresh = tree.clone();
                let first = LayoutInput {
                    run_mode: mode,
                    known_dimensions: Size {
                        width: Some(90.0),
                        height: None,
                    },
                    known_dimensions_are_definite: Size {
                        width: false,
                        height: true,
                    },
                    ..input()
                };
                tree.compute_boundary_with_measure(root, first, measure);
                let second = LayoutInput {
                    available_space: Size {
                        width: AvailableSpace::Definite(90.0),
                        height: AvailableSpace::MaxContent,
                    },
                    ..first
                };
                let cached = tree.compute_boundary_with_measure(root, second, measure);
                fresh.set_size_cache_policy(SizeCachePolicy::Off);
                let computed = fresh.compute_boundary_with_measure(root, second, measure);
                assert_eq!(
                    cached.size, computed.size,
                    "ratio {ratio:?}, percent {percent}, mode {mode:?}"
                );
                for node in [root, child, leaf] {
                    assert_eq!(
                        tree.unrounded_layout(node),
                        fresh.unrounded_layout(node),
                        "ratio {ratio:?}, percent {percent}, mode {mode:?}, node {node:?}"
                    );
                }
            }
        }
    }
}

fn shape(
    kind: &str,
    depth: usize,
    padded: bool,
    definite: bool,
    three: bool,
) -> (TaffyTree, NodeId) {
    let mut tree = TaffyTree::new();
    tree.track_size_computations(true);
    let mut node = tree.new_leaf(Style::default()).unwrap();
    for level in (0..depth).rev() {
        let row = level % 2 == 0 || kind == "baseline-rows";
        let mut style = Style {
            flex_direction: if row {
                FlexDirection::Row
            } else {
                FlexDirection::Column
            },
            ..Style::default()
        };
        if padded {
            style.padding = Rect::length((level % 3 + 1) as f32);
        }
        if kind == "start" {
            style.align_items = Some(AlignItems::FLEX_START);
        }
        if kind.starts_with("baseline") && row {
            style.align_items = Some(AlignItems::BASELINE);
        }
        if kind == "wrap" && row {
            style.flex_wrap = FlexWrap::Wrap;
        }
        if kind == "ratio" {
            style.aspect_ratio = Some(1.5);
        }
        if kind == "percent" {
            style.size = Size {
                width: Dimension::percent(0.95),
                height: Dimension::percent(0.9),
            };
        }
        if kind == "grid" && level == depth / 2 {
            style.display = Display::Grid;
        }
        if kind == "table" && level == depth / 2 {
            // Tables enter Taffy as a grid; the kernel test covers table construction.
            style.display = Display::Grid;
            style.rare.grid_template_columns = vec![auto(), auto()];
        }
        let mut children = vec![node];
        let siblings = if kind == "fifty" && level == depth / 2 {
            style.flex_direction = FlexDirection::Row;
            style.flex_wrap = FlexWrap::Wrap;
            49
        } else if three {
            2
        } else if kind.starts_with("baseline") && row {
            1
        } else {
            0
        };
        for _ in 0..siblings {
            children.push(tree.new_leaf(Style::default()).unwrap());
        }
        node = tree.new_with_children(style, &children).unwrap();
    }
    if definite {
        let mut style = tree.style(node).unwrap().clone();
        style.size = Size {
            width: Dimension::length(390.0),
            height: Dimension::length(844.0),
        };
        tree.set_style(node, style).unwrap();
    }
    (tree, node)
}

fn shape_counts(depths: &[usize]) {
    let mut failures = Vec::new();
    for kind in [
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
    ] {
        for padded in [false, true] {
            for definite in [false, true] {
                for three in [false, true] {
                    for &depth in depths {
                        let result = std::panic::catch_unwind(|| {
                            let (mut tree, root) = shape(kind, depth, padded, definite, three);
                            let offer = if definite {
                                Size {
                                    width: AvailableSpace::Definite(390.0),
                                    height: AvailableSpace::Definite(844.0),
                                }
                            } else {
                                Size::MAX_CONTENT
                            };
                            tree.compute_layout_with_measure(root, offer, measure)
                                .unwrap();
                            let counts = tree.size_cache_counts();
                            eprintln!("SHAPE {kind} depth={depth} padded={padded} definite={definite} three={three}: {counts:?}");
                            for (node, answers) in tree.size_cache_peaks() {
                                let mut levels_above = 0;
                                let mut parent = tree.parent(node);
                                while let Some(ancestor) = parent {
                                    levels_above += 1;
                                    parent = tree.parent(ancestor);
                                }
                                let bound = 64 + 8 * levels_above;
                                assert!(answers <= bound, "LLP 1165 alarm: {kind}, depth {depth}, padded {padded}, definite {definite}, three {three}, node {node:?}, levels above {levels_above}: {answers} answers, bound {bound}");
                            }
                            assert_eq!(counts.repeated, 0, "{kind}, depth {depth}, padded {padded}, definite {definite}, three {three}");
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
#[ignore = "async lane: all size-cache shapes and depths"]
fn size_answers_stay_bounded_at_depth() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| shape_counts(&[8, 16, 32, 64, 128]))
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn disabled_counts_do_not_record_layout_work() {
    let (mut tree, root) = shape("start", 8, false, true, false);
    tree.track_size_computations(false);
    tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
        .unwrap();
    assert_eq!(tree.size_cache_counts(), Default::default());
    tree.track_size_computations(true);
    tree.mark_dirty(root).unwrap();
    tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
        .unwrap();
    assert_eq!(tree.size_cache_counts().passes, 1);
}

#[test]
fn size_answers_stay_bounded_in_short_chains() {
    shape_counts(&[8]);
}

#[test]
#[ignore = "async lane: all small shapes under three cache policies"]
fn small_shapes_equal_layout_without_a_size_cache() {
    let mut failures = Vec::new();
    for kind in [
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
    ] {
        for depth in [4, 8] {
            for padded in [false, true] {
                for definite in [false, true] {
                    for three in [false, true] {
                        let result = std::panic::catch_unwind(|| {
                            let (tree, root) = shape(kind, depth, padded, definite, three);
                            let leaf = tree.size_cache_occupancy().next().unwrap().0;
                            let mut trees = [
                                SizeCachePolicy::All,
                                SizeCachePolicy::Nine,
                                SizeCachePolicy::Off,
                            ]
                            .map(|policy| {
                                let mut t = tree.clone();
                                t.disable_rounding();
                                t.set_size_cache_policy(policy);
                                t
                            });
                            for (round, width) in [390.0, 430.0, 390.0].into_iter().enumerate() {
                                for t in &mut trees {
                                    t.compute_layout_with_measure(
                                        root,
                                        Size {
                                            width: AvailableSpace::Definite(width),
                                            height: AvailableSpace::Definite(844.0),
                                        },
                                        measure,
                                    )
                                    .unwrap();
                                }
                                eprintln!("COMPARE {kind} depth {depth} padded {padded} definite {definite} three {three} round {round} width {width}: {:?}", trees.iter().map(|t| t.unrounded_layout(leaf).size).collect::<Vec<_>>());
                                for (node, _) in trees[0].size_cache_occupancy() {
                                    assert_eq!(layout_bits(&trees[0], node), layout_bits(&trees[2], node), "All versus Off: {kind} depth {depth} padded {padded} definite {definite} three {three} width {width}");
                                }
                                for (node, _) in trees[0].size_cache_occupancy() {
                                    assert_eq!(layout_bits(&trees[0], node), layout_bits(&trees[1], node), "All versus Nine: {kind} depth {depth} padded {padded} definite {definite} three {three} width {width}");
                                }
                                assert_eq!(trees[0].size_cache_counts().repeated, 0);
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
    assert!(failures.is_empty(), "failed comparisons: {failures:#?}");
}

fn final_after_baseline_probe(grid: bool) {
    use taffy::CacheTree;
    for memo in [false, true] {
        let mut tree = TaffyTree::new();
        let leaf = tree
            .new_leaf(Style {
                size: Size {
                    width: Dimension::percent(0.5),
                    height: Dimension::auto(),
                },
                ..Style::default()
            })
            .unwrap();
        let growing = tree
            .new_with_children(
                Style {
                    flex_grow: 1.0,
                    ..Style::default()
                },
                &[leaf],
            )
            .unwrap();
        let fixed_leaf = tree.new_leaf(Style::default()).unwrap();
        let fixed = tree
            .new_with_children(
                Style {
                    size: Size {
                        width: Dimension::length(40.0),
                        height: Dimension::auto(),
                    },
                    ..Style::default()
                },
                &[fixed_leaf],
            )
            .unwrap();
        let mut style = Style {
            align_items: Some(AlignItems::BASELINE),
            ..Style::default()
        };
        if grid {
            style.display = Display::Grid;
            style.rare.grid_template_columns = vec![fr(1.0), fr(1.0)];
        }
        let row = tree.new_with_children(style, &[growing, fixed]).unwrap();
        let root = tree
            .new_with_children(
                Style {
                    flex_direction: FlexDirection::Column,
                    ..Style::default()
                },
                &[row],
            )
            .unwrap();
        tree.set_memo_root(row, memo);
        let final_input = LayoutInput {
            run_mode: RunMode::PerformLayout,
            known_dimensions: Size {
                width: Some(300.0),
                height: None,
            },
            available_space: Size {
                width: AvailableSpace::Definite(300.0),
                height: AvailableSpace::MaxContent,
            },
            ..input()
        };
        let probe = LayoutInput {
            known_dimensions: Size {
                width: Some(60.0),
                height: None,
            },
            available_space: Size {
                width: AvailableSpace::Definite(60.0),
                height: AvailableSpace::MaxContent,
            },
            ..input()
        };
        let mut fresh = tree.clone();
        fresh.compute_boundary_with_measure(root, final_input, measure);
        for _ in 0..2 {
            for node in [root, row, growing, fixed, leaf, fixed_leaf] {
                tree.cache_clear(node);
            }
            tree.compute_boundary_with_measure(root, final_input, measure);
            tree.compute_boundary_with_measure(root, probe, measure);
            assert_ne!(
                layout_bits(&tree, leaf),
                layout_bits(&fresh, leaf),
                "probe must change geometry"
            );
            tree.compute_boundary_with_measure(root, final_input, measure);
            for node in [root, row, growing, fixed, leaf, fixed_leaf] {
                assert_eq!(
                    layout_bits(&tree, node),
                    layout_bits(&fresh, node),
                    "grid {grid}, memo {memo}, node {node:?}"
                );
            }
        }
        if memo {
            assert!(tree.memo_counts().0 > 0, "baseline repair must replay");
        }
    }
}

#[test]
fn a_column_drops_its_final_layout_after_a_descendants_baseline_probe() {
    final_after_baseline_probe(false);
}

#[test]
fn a_grid_drops_its_final_layout_after_sizing_baseline_items() {
    final_after_baseline_probe(true);
}

fn question(width: usize) -> LayoutInput {
    LayoutInput {
        known_dimensions: Size {
            width: Some(width as f32),
            height: None,
        },
        ..input()
    }
}

fn store_questions(tree: &mut TaffyTree, node: NodeId, widths: std::ops::Range<usize>) {
    use taffy::CacheTree;
    for width in widths {
        tree.cache_store(
            node,
            &question(width),
            LayoutOutput::from_outer_size(Size {
                width: width as f32,
                height: 10.0,
            }),
        );
    }
}

fn answers(tree: &TaffyTree, node: NodeId) -> usize {
    tree.size_cache_occupancy()
        .find(|&(id, _)| id == node)
        .unwrap()
        .1
}

#[test]
fn size_cache_peaks_survive_clears_until_counts_reset() {
    use taffy::CacheTree;
    let mut tree = TaffyTree::new();
    tree.track_size_computations(true);
    let a = tree.new_leaf(Style::default()).unwrap();
    let b = tree.new_leaf(Style::default()).unwrap();
    let peak = |tree: &TaffyTree, node| {
        tree.size_cache_peaks()
            .find(|&(id, _)| id == node)
            .unwrap()
            .1
    };
    store_questions(&mut tree, a, 0..12);
    store_questions(&mut tree, b, 0..3);
    tree.cache_clear(a);
    assert_eq!(answers(&tree, a), 0);
    assert_eq!(peak(&tree, a), 12);
    assert_eq!(peak(&tree, b), 3);
    assert_eq!(tree.size_cache_counts().max_answers, 12);
    store_questions(&mut tree, a, 0..1);
    assert_eq!(peak(&tree, a), 12);
    tree.reset_size_cache_counts();
    assert_eq!(peak(&tree, a), 1);
    assert_eq!(peak(&tree, b), 3);
    assert_eq!(tree.size_cache_counts().max_answers, 3);
    tree.cache_clear(b);
    assert_eq!(peak(&tree, b), 3, "answers held before a clear still count");
    tree.track_size_computations(false);
    store_questions(&mut tree, a, 1..20);
    assert_eq!(peak(&tree, a), 1, "disabled tracking leaves peaks alone");
}

#[test]
fn boundary_and_root_passes_belong_to_each_tree() {
    use taffy::CacheTree;
    let mut a = TaffyTree::new();
    let held = a.new_leaf(Style::default()).unwrap();
    let root = a.new_leaf(Style::default()).unwrap();
    let mut b = a.clone();
    store_questions(&mut a, held, 0..9);
    store_questions(&mut b, held, 0..9);
    for boundary in [true, false] {
        if boundary {
            a.compute_boundary_with_measure(root, input(), measure);
        } else {
            a.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
                .unwrap();
        }
        let offset = if boundary { 10 } else { 20 };
        let count = answers(&a, held);
        store_questions(&mut a, held, offset..offset + count);
        assert_eq!(answers(&a, held), count, "earlier pass must be replaced");
        store_questions(&mut a, held, offset + count..offset + count + 1);
        assert_eq!(answers(&a, held), count + 1, "current answers must survive");
        for width in offset..offset + count + 1 {
            assert!(a.cache_get(held, &question(width)).is_some());
        }
    }
    store_questions(&mut b, held, 10..19);
    assert_eq!(
        answers(&b, held),
        18,
        "another tree cannot advance this tree's pass"
    );
}

#[test]
fn a_wrapped_pass_does_not_grow_a_nine_question_cache() {
    let mut tree = TaffyTree::new();
    let held = tree.new_leaf(Style::default()).unwrap();
    let root = tree.new_leaf(Style::default()).unwrap();
    tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
        .unwrap();
    store_questions(&mut tree, held, 0..9);
    for _ in 0..u16::MAX {
        tree.compute_boundary_with_measure(root, input(), measure);
    }
    store_questions(&mut tree, held, 10..19);
    assert_eq!(answers(&tree, held), 9);
}

#[test]
fn answers_used_in_a_pass_survive_new_questions() {
    use taffy::CacheTree;
    let mut tree = TaffyTree::new();
    let node = tree.new_leaf(Style::default()).unwrap();
    let root = tree.new_leaf(Style::default()).unwrap();
    store_questions(&mut tree, node, 0..25);
    assert_eq!(answers(&tree, node), 25);
    for width in 0..25 {
        assert!(tree.cache_get(node, &question(width)).is_some());
    }
    tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
        .unwrap();
    for width in 10..25 {
        assert!(tree.cache_get(node, &question(width)).is_some());
    }
    store_questions(&mut tree, node, 25..36);
    assert_eq!(answers(&tree, node), 26);
    for width in 10..36 {
        assert!(tree.cache_get(node, &question(width)).is_some());
    }
    let bytes = tree.size_cache_bytes();
    tree.mark_dirty(node).unwrap();
    assert_eq!(tree.size_cache_bytes(), bytes);
    store_questions(&mut tree, node, 40..41);
    assert_eq!(answers(&tree, node), 1);
}

#[test]
fn the_first_nine_answers_fill_across_passes() {
    use taffy::CacheTree;
    let mut tree = TaffyTree::new();
    let node = tree.new_leaf(Style::default()).unwrap();
    let root = tree.new_leaf(Style::default()).unwrap();
    for pass in 1..=9 {
        tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
            .unwrap();
        store_questions(&mut tree, node, pass..pass + 1);
        assert_eq!(answers(&tree, node), pass);
    }
    tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
        .unwrap();
    for width in 1..=9 {
        assert!(tree.cache_get(node, &question(width)).is_some());
    }
    store_questions(&mut tree, node, 10..11);
    assert_eq!(answers(&tree, node), 10);
}

#[test]
fn bytes_of_the_allocated_cache() {
    for (count, expected) in [(1, 232), (3, 232), (9, 424)] {
        let mut tree = TaffyTree::new();
        let node = tree.new_leaf(Style::default()).unwrap();
        store_questions(&mut tree, node, 0..count);
        assert_eq!(tree.size_cache_bytes(), expected);
    }
}

#[test]
fn dropping_the_final_entry_preserves_dirty_propagation() {
    use taffy::tree::{Cache, ClearState};
    let mut cache = Cache::new();
    let final_input = LayoutInput {
        run_mode: RunMode::PerformLayout,
        ..input()
    };
    cache.store(&final_input, LayoutOutput::HIDDEN, 1, SizeCachePolicy::All);
    cache.invalidate_final_layout();
    assert!(cache.get(&final_input, 1, false).is_none());
    assert!(matches!(cache.clear(), ClearState::Cleared));
    assert!(matches!(cache.clear(), ClearState::AlreadyEmpty));
}
