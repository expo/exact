use crate::geometry::{Line, Size};
use crate::prelude::*;
use crate::tree::{LayoutPartialTreeExt, SizingMode};
use crate::Overflow;

// Chrome 151.0.7922.34, 2026-10-18: width:max-content wrappers around
// wrapping rows, including an item whose automatic minimum is disabled.
#[test]
fn wrapping_max_content_is_floored_by_min_content() {
    let mut failures = Vec::new();
    for (name, preferred, content, margin, minimum, maximum, wrap, expected) in [
        ("preferred", length(100.0), None, 0.0, auto(), auto(), FlexWrap::Wrap, 100.0),
        ("contents", auto(), Some(100.0), 0.0, length(0.0), auto(), FlexWrap::Wrap, 100.0),
        ("margin", length(100.0), None, -10.0, auto(), auto(), FlexWrap::Wrap, 90.0),
        ("maximum", length(100.0), None, 0.0, auto(), length(80.0), FlexWrap::Wrap, 80.0),
        ("nowrap", length(100.0), None, 0.0, auto(), auto(), FlexWrap::NoWrap, 40.0),
    ] {
        let mut tree = TaffyTree::<()>::new();
        let children: Vec<_> = content.into_iter().map(|width| tree.new_leaf(Style {
            display: Display::Block,
            size: Size { width: length(width), height: length(10.0) },
            ..Default::default()
        }).unwrap()).collect();
        let item = tree.new_with_children(Style {
            display: Display::Block,
            size: Size { width: preferred, height: length(20.0) },
            flex_basis: length(40.0),
            min_size: Size { width: minimum, height: auto() },
            margin: crate::geometry::Rect { right: length(margin), ..zero() },
            ..Default::default()
        }, &children).unwrap();
        let row = tree.new_with_children(Style {
            flex_wrap: wrap,
            max_size: Size { width: maximum, height: auto() },
            ..Default::default()
        }, &[item]).unwrap();
        for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
            tree.mark_dirty(row).unwrap();
            let size = tree.as_layout_tree().measure_child_size_both(
                row, Size::NONE, Size::NONE, Size::MAX_CONTENT, mode, Line::FALSE,
            );
            if size != (Size { width: expected, height: 20.0 }) {
                failures.push(format!("{name}; {mode:?}: {size:?}, Chrome {expected}×20"));
            }
        }
        tree.mark_dirty(row).unwrap();
        tree.compute_layout(row, Size::MAX_CONTENT).unwrap();
        let size = tree.layout(row).unwrap().size;
        if size != (Size { width: expected, height: 20.0 }) {
            failures.push(format!("{name}; layout: {size:?}, Chrome {expected}×20"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: intrinsic row widths use the
// hypothetical main size only when a definite basis cannot flex to the contribution.
#[test]
fn intrinsic_row_contributions_respect_definite_flex_bases() {
    let mut failures = Vec::new();
    for (name, width, basis, grow, shrink, minimum, maximum, content, expected) in [
        ("non-growing", length(100.0), length(40.0), 0.0, 1.0, auto(), auto(), None, 40.0),
        ("non-shrinking", length(100.0), length(40.0), 0.0, 0.0, auto(), auto(), None, 40.0),
        ("grow", length(100.0), length(40.0), 1.0, 1.0, auto(), auto(), None, 100.0),
        ("larger basis", length(30.0), length(140.0), 0.0, 1.0, auto(), auto(), None, 30.0),
        ("automatic minimum", auto(), length(40.0), 0.0, 1.0, auto(), auto(), Some(100.0), 100.0),
        ("minimum", length(100.0), length(40.0), 0.0, 1.0, length(70.0), auto(), None, 70.0),
        ("maximum", length(100.0), length(40.0), 0.0, 1.0, auto(), length(30.0), None, 30.0),
        ("non-shrinking maximum", length(30.0), length(140.0), 0.0, 0.0, auto(), length(70.0), None, 70.0),
        ("percentage", length(100.0), percent(0.5), 0.0, 1.0, auto(), auto(), None, 100.0),
        ("auto", length(100.0), auto(), 0.0, 1.0, auto(), auto(), None, 100.0),
        ("content", length(100.0), Dimension::content(), 0.0, 1.0, auto(), auto(), Some(10.0), 100.0),
        ("content non-shrinking", length(30.0), Dimension::content(), 0.0, 0.0, auto(), auto(), Some(100.0), 30.0),
    ] {
        let mut tree = TaffyTree::<()>::new();
        let children: Vec<_> = content.into_iter().map(|width| tree.new_leaf(Style {
            display: Display::Block,
            size: Size { width: length(width), height: length(10.0) },
            ..Default::default()
        }).unwrap()).collect();
        let item = tree.new_with_children(Style {
            display: Display::Block,
            size: Size { width, height: length(20.0) },
            min_size: Size { width: minimum, height: auto() },
            max_size: Size { width: maximum, height: auto() },
            flex_basis: basis,
            flex_grow: grow,
            flex_shrink: shrink,
            ..Default::default()
        }, &children).unwrap();
        let row = tree.new_with_children(Style::default(), &[item]).unwrap();
        for width in [AvailableSpace::MinContent, AvailableSpace::MaxContent] {
            for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                tree.mark_dirty(row).unwrap();
                let size = tree.as_layout_tree().measure_child_size_both(
                    row, Size::NONE, Size::NONE, Size { width, height: AvailableSpace::MaxContent },
                    mode, Line::FALSE,
                );
                if size != (Size { width: expected, height: 20.0 }) {
                    failures.push(format!("{name}; {width:?}; {mode:?}: {size:?}, Chrome {expected}×20"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: min-content and max-content rows.
// A ratio can resolve a content basis without making its used basis definite.
#[test]
fn ratio_content_bases_do_not_clamp_intrinsic_contributions() {
    let mut failures = Vec::new();
    for (name, width, basis, minimum, shrink, expected) in [
        ("content", 100.0, Dimension::content(), length(0.0), 1.0, 100.0),
        ("content cannot shrink", 10.0, Dimension::content(), length(0.0), 0.0, 10.0),
        ("content automatic minimum", 100.0, Dimension::content(), auto(), 1.0, 100.0),
        ("min-content", 100.0, Dimension::min_content(), length(0.0), 1.0, 100.0),
        ("max-content", 100.0, Dimension::max_content(), length(0.0), 1.0, 100.0),
        ("auto preferred", 100.0, auto(), length(0.0), 1.0, 100.0),
        ("length", 100.0, length(40.0), length(0.0), 1.0, 40.0),
    ] {
        let mut tree = TaffyTree::<()>::new();
        let item = tree.new_leaf(Style {
            display: Display::Block,
            size: Size { width: length(width), height: length(20.0) },
            aspect_ratio: Some(1.0),
            min_size: Size { width: minimum, height: auto() },
            flex_basis: basis,
            flex_shrink: shrink,
            ..Default::default()
        }).unwrap();
        let row = tree.new_with_children(Style::default(), &[item]).unwrap();
        for width in [AvailableSpace::MinContent, AvailableSpace::MaxContent] {
            for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                tree.mark_dirty(row).unwrap();
                let size = tree.as_layout_tree().measure_child_size_both(
                    row, Size::NONE, Size::NONE, Size { width, height: AvailableSpace::MaxContent },
                    mode, Line::FALSE,
                );
                if size != (Size { width: expected, height: 20.0 }) {
                    failures.push(format!("{name}; {width:?}; {mode:?}: {size:?}, Chrome {expected}×20"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: max-content rows and columns.
#[test]
fn zero_inner_flex_bases_preserve_negative_contributions() {
    let mut failures = Vec::new();
    for direction in [FlexDirection::Row, FlexDirection::Column] {
        let size = |main| if direction.is_row() {
            Size { width: length(main), height: length(20.0) }
        } else {
            Size { width: length(20.0), height: length(main) }
        };
        for (grow, margin, expected) in [(0.0, -10.0, 20.0), (1.0, -10.0, 20.0), (0.0, 10.0, 40.0)] {
            let mut tree = TaffyTree::<()>::new();
            let item = tree.new_leaf(Style {
                size: size(0.0),
                flex_basis: length(0.0),
                flex_grow: grow,
                margin: if direction.is_row() {
                    crate::geometry::Rect { right: length(margin), ..zero() }
                } else {
                    crate::geometry::Rect { bottom: length(margin), ..zero() }
                },
                ..Default::default()
            }).unwrap();
            let sibling = tree.new_leaf(Style { size: size(30.0), ..Default::default() }).unwrap();
            let root = tree.new_with_children(Style { flex_direction: direction, ..Default::default() }, &[item, sibling]).unwrap();
            for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                tree.mark_dirty(root).unwrap();
                let measured = tree.as_layout_tree().measure_child_size_both(
                    root, Size::NONE, Size::NONE, Size::MAX_CONTENT, mode, Line::FALSE,
                ).main(direction);
                if measured != expected {
                    failures.push(format!("{direction:?}; grow {grow}; margin {margin}; {mode:?}: {measured}, Chrome {expected}"));
                }
            }
            tree.mark_dirty(root).unwrap();
            tree.compute_layout(root, Size::MAX_CONTENT).unwrap();
            let measured = tree.layout(root).unwrap().size.main(direction);
            if measured != expected {
                failures.push(format!("{direction:?}; grow {grow}; margin {margin}; layout: {measured}, Chrome {expected}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: stretch supplies the percentage basis
// when it changes the ratio item's height; content growth alone does not.
#[test]
fn stretched_ratio_percentage_height_uses_the_final_cross_size() {
    let mut failures = Vec::new();
    for (name, height, minimum, maximum, align, content, item_height, percentage_height) in [
        ("capped", auto(), auto(), length(30.0), AlignSelf::STRETCH, false, 30.0, 30.0),
        ("definite", length(30.0), auto(), auto(), AlignSelf::STRETCH, false, 30.0, 30.0),
        ("start", auto(), auto(), length(30.0), AlignSelf::FLEX_START, false, 50.0, 50.0),
        ("content growth", auto(), auto(), auto(), AlignSelf::STRETCH, true, 130.0, 50.0),
        ("capped content", auto(), auto(), length(100.0), AlignSelf::STRETCH, true, 100.0, 100.0),
        ("unchanged cap", auto(), auto(), length(130.0), AlignSelf::STRETCH, true, 130.0, 50.0),
        ("definite content", length(130.0), auto(), auto(), AlignSelf::STRETCH, true, 130.0, 130.0),
        ("minimum", auto(), length(80.0), auto(), AlignSelf::STRETCH, false, 80.0, 80.0),
    ] {
        let mut tree = TaffyTree::<()>::new();
        let percentage = tree.new_leaf(Style {
            display: Display::Block,
            size: Size { width: length(20.0), height: percent(1.0) },
            ..Default::default()
        }).unwrap();
        let mut children = vec![percentage];
        if content {
            children.push(tree.new_leaf(Style {
                display: Display::Block,
                size: Size { width: length(20.0), height: length(80.0) },
                ..Default::default()
            }).unwrap());
        }
        let item = tree.new_with_children(Style {
            display: Display::Block,
            size: Size { width: length(100.0), height: auto() },
            aspect_ratio: Some(2.0),
            align_self: Some(align),
            ..Default::default()
        }, &children).unwrap();
        let row = tree.new_with_children(Style {
            size: Size { width: length(100.0), height },
            min_size: Size { width: auto(), height: minimum },
            max_size: Size { width: auto(), height: maximum },
            ..Default::default()
        }, &[item]).unwrap();
        let root = tree.new_with_children(Style {
            display: Display::Block,
            size: Size { width: length(800.0), height: length(600.0) },
            ..Default::default()
        }, &[row]).unwrap();
        tree.compute_layout(root, Size {
            width: AvailableSpace::Definite(800.0), height: AvailableSpace::Definite(600.0),
        }).unwrap();
        for (node, width, height, y) in [
            (item, 100.0, item_height, 0.0),
            (percentage, 20.0, percentage_height, 0.0),
        ].into_iter().chain(content.then(|| (children[1], 20.0, 80.0, percentage_height))) {
            let layout = tree.layout(node).unwrap();
            let frame = [layout.location.x, layout.location.y, layout.size.width, layout.size.height];
            let expected = [0.0, y, width, height];
            if frame != expected {
                failures.push(format!("{name}; {node:?}: {frame:?}, Chrome {expected:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn fractional_cross_margins_do_not_make_content_growth_a_stretch() {
    let mut failures = Vec::new();
    for (name, top, bottom, row_min, row_max, item_max, item_height, percentage_height) in [
        ("top", 200.0, 0.0, auto(), auto(), auto(), 110.1, 50.0),
        ("bottom", 0.0, 200.0, auto(), auto(), auto(), 110.1, 50.0),
        ("split", 100.25, 99.75, auto(), auto(), auto(), 110.1, 50.0),
        ("fractional", 200.125, 20.25, auto(), auto(), auto(), 110.1, 50.0),
        ("negative", -100.0, 0.0, auto(), auto(), auto(), 110.1, 50.0),
        ("negative stretch", -200.0, 0.0, auto(), auto(), auto(), 200.0, 200.0),
        ("maximum cancels stretch", 200.0, 0.0, length(400.0), auto(), length(110.1), 110.1, 50.0),
        ("unchanged cap", 200.0, 0.0, auto(), length(310.1), auto(), 110.1, 50.0),
        ("smaller cap", 200.0, 0.0, auto(), length(300.0), auto(), 100.0, 100.0),
        ("larger minimum", 200.0, 0.0, length(320.0), auto(), auto(), 120.0, 120.0),
    ] {
        let mut tree = TaffyTree::<()>::new();
        tree.disable_rounding();
        let percentage = tree.new_leaf(Style {
            display: Display::Block,
            size: Size { width: length(20.0), height: percent(1.0) },
            ..Default::default()
        }).unwrap();
        let following = tree.new_leaf(Style {
            display: Display::Block,
            size: Size { width: length(20.0), height: length(60.1) },
            ..Default::default()
        }).unwrap();
        let item = tree.new_with_children(Style {
            display: Display::Block,
            size: Size { width: length(100.0), height: auto() },
            max_size: Size { width: auto(), height: item_max },
            margin: crate::geometry::Rect { top: length(top), bottom: length(bottom), ..zero() },
            aspect_ratio: Some(2.0),
            ..Default::default()
        }, &[percentage, following]).unwrap();
        let row = tree.new_with_children(Style {
            size: Size { width: length(100.0), height: auto() },
            min_size: Size { width: auto(), height: row_min },
            max_size: Size { width: auto(), height: row_max },
            ..Default::default()
        }, &[item]).unwrap();
        tree.compute_layout(row, Size {
            width: AvailableSpace::Definite(800.0), height: AvailableSpace::Definite(600.0),
        }).unwrap();
        for (node, y, width, height) in [
            (item, top, 100.0, item_height),
            (percentage, 0.0, 20.0, percentage_height),
            (following, percentage_height, 20.0, 60.1),
        ] {
            let layout = tree.layout(node).unwrap();
            let frame = [layout.location.x, layout.location.y, layout.size.width, layout.size.height];
            let expected = [0.0, y, width, height];
            if frame.iter().zip(expected).any(|(actual, expected)| (actual - expected).abs() > 0.01) {
                failures.push(format!("{name}; {node:?}: {frame:?}, expected {expected:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: a 100px-wide ratio item containing
// a percentage child followed by a 60.1px child. Container insets must
// not turn its natural height into a definite percentage basis.
#[test]
fn container_insets_do_not_make_content_growth_a_stretch() {
    let mut failures = Vec::new();
    for position in [Position::Relative, Position::Absolute] {
        for (name, padding, border, minimum, maximum, item_max, following_height, percentage_height) in [
            ("unit boundary", 200.3, 0.0, auto(), length(400.0), auto(), 60.125, 50.0),
            ("padding", 200.0, 0.0, auto(), length(400.0), auto(), 60.1, 50.0),
            ("border", 0.0, 100.0, auto(), length(400.0), auto(), 60.1, 50.0),
            ("padding and border", 100.0, 100.0, auto(), length(400.0), auto(), 60.1, 50.0),
            ("subunit cap", 200.0, 0.0, auto(), length(110.095), auto(), 60.1, 50.0),
            ("fractional cap", 200.0, 0.0, auto(), length(110.09), auto(), 60.1, 110.078125),
            ("same unit minimum 110.102", 200.0, 0.0, length(110.102), length(400.0), auto(), 60.1, 50.0),
            ("same unit minimum 110.108", 200.0, 0.0, length(110.108), length(400.0), auto(), 60.1, 50.0),
            ("same unit minimum 110.109", 200.0, 0.0, length(110.109), length(400.0), auto(), 60.1, 50.0),
            ("unit shrink", 200.0, 0.0, auto(), length(110.078125), auto(), 60.1, 110.078125),
            ("unit grow", 200.0, 0.0, length(110.109375), length(400.0), auto(), 60.1, 110.109375),
            ("maximum cancels stretch", 200.0, 0.0, length(120.0), length(400.0), length(110.1), 60.1, 50.0),
        ] {
            let mut tree = TaffyTree::<()>::new();
            tree.disable_rounding();
            let percentage = tree.new_leaf(Style {
                display: Display::Block,
                size: Size { width: length(20.0), height: percent(1.0) },
                ..Default::default()
            }).unwrap();
            let following = tree.new_leaf(Style {
                display: Display::Block,
                size: Size { width: length(20.0), height: length(following_height) },
                ..Default::default()
            }).unwrap();
            let item = tree.new_with_children(Style {
                display: Display::Block,
                size: Size { width: length(100.0), height: auto() },
                max_size: Size { width: auto(), height: item_max },
                aspect_ratio: Some(2.0),
                ..Default::default()
            }, &[percentage, following]).unwrap();
            let row = tree.new_with_children(Style {
                position,
                box_sizing: BoxSizing::ContentBox,
                size: Size { width: length(100.0), height: auto() },
                min_size: Size { width: auto(), height: minimum },
                max_size: Size { width: auto(), height: maximum },
                padding: crate::geometry::Rect { top: length(padding), ..zero() },
                border: crate::geometry::Rect { top: length(border), bottom: length(border), ..zero() },
                ..Default::default()
            }, &[item]).unwrap();
            let root = tree.new_with_children(Style {
                display: Display::Block,
                size: Size { width: length(800.0), height: length(600.0) },
                ..Default::default()
            }, &[row]).unwrap();
            tree.compute_layout(root, Size::MAX_CONTENT).unwrap();
            let height = tree.layout(percentage).unwrap().size.height;
            let following_y = tree.layout(following).unwrap().location.y;
            if [height, following_y].into_iter().any(|value| {
                crate::util::sys::floor(value * 64.0) != percentage_height * 64.0
            }) {
                failures.push(format!(
                    "{position:?} {name}: percentage {height}, following y {following_y}; Chrome {percentage_height}",
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: rows inside width:min-content wrappers.
#[test]
fn wrapping_min_content_width_uses_contents_independently_of_flex_basis() {
    let mut failures = Vec::new();
    for (name, content, preferred, shrink, minimum, overflow, wrap, width, item_width, reverse_x) in [
        ("zero minimum", 100.0, auto(), 0.0, length(0.0), Overflow::Visible, FlexWrap::Wrap, 100.0, 60.0, 40.0),
        ("hidden overflow", 100.0, auto(), 0.0, auto(), Overflow::Hidden, FlexWrap::Wrap, 100.0, 60.0, 40.0),
        ("shrinkable", 100.0, auto(), 1.0, length(0.0), Overflow::Visible, FlexWrap::Wrap, 100.0, 60.0, 40.0),
        ("automatic minimum", 100.0, auto(), 0.0, auto(), Overflow::Visible, FlexWrap::Wrap, 100.0, 100.0, 0.0),
        ("preferred width", 100.0, length(60.0), 0.0, length(0.0), Overflow::Visible, FlexWrap::Wrap, 60.0, 60.0, 0.0),
        ("narrow contents rigid", 20.0, auto(), 0.0, length(0.0), Overflow::Visible, FlexWrap::Wrap, 20.0, 60.0, -40.0),
        ("narrow contents shrinkable", 20.0, auto(), 1.0, length(0.0), Overflow::Visible, FlexWrap::Wrap, 20.0, 20.0, 0.0),
        ("preferred width narrow contents", 20.0, length(60.0), 0.0, length(0.0), Overflow::Visible, FlexWrap::Wrap, 60.0, 60.0, 0.0),
        ("nowrap", 100.0, auto(), 0.0, length(0.0), Overflow::Visible, FlexWrap::NoWrap, 180.0, 60.0, 120.0),
    ] {
        for direction in [FlexDirection::Row, FlexDirection::RowReverse] {
            let mut tree = TaffyTree::<()>::new();
            let items: Vec<_> = (0..3).map(|_| {
                let child = tree.new_leaf(Style {
                    size: Size { width: length(content), height: length(10.0) },
                    ..Default::default()
                }).unwrap();
                tree.new_with_children(Style {
                    display: Display::Block,
                    size: Size { width: preferred, height: length(100.0) },
                    min_size: Size { width: minimum, height: auto() },
                    flex_basis: if preferred.is_auto() { length(60.0) } else { auto() },
                    flex_shrink: shrink,
                    overflow: crate::geometry::Point { x: overflow, y: overflow },
                    ..Default::default()
                }, &[child]).unwrap()
            }).collect();
            let row = tree.new_with_children(Style {
                flex_direction: direction,
                flex_wrap: wrap,
                align_content: Some(AlignContent::FLEX_START),
                ..Default::default()
            }, &items).unwrap();
            let available = Size { width: AvailableSpace::MinContent, height: AvailableSpace::MaxContent };
            let expected = Size { width, height: if wrap == FlexWrap::NoWrap { 100.0 } else { 300.0 } };
            for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                tree.mark_dirty(row).unwrap();
                let size = tree.as_layout_tree().measure_child_size_both(
                    row, Size::NONE, Size::NONE, available, mode, Line::FALSE,
                );
                if size != expected {
                    failures.push(format!("{name}; {direction:?}; {mode:?}: {size:?}, Chrome {expected:?}"));
                }
            }
            tree.mark_dirty(row).unwrap();
            tree.compute_layout(row, available).unwrap();
            let size = tree.layout(row).unwrap().size;
            if size != expected {
                failures.push(format!("{name}; {direction:?}; layout: {size:?}, Chrome {expected:?}"));
            }
            for (index, item) in items.into_iter().enumerate() {
                let layout = tree.layout(item).unwrap();
                let x = if direction == FlexDirection::RowReverse { reverse_x } else { 0.0 };
                let (x, y) = if wrap == FlexWrap::NoWrap {
                    (x + index as f32 * if direction == FlexDirection::RowReverse { -60.0 } else { 60.0 }, 0.0)
                } else {
                    (x, index as f32 * 100.0)
                };
                let got = [layout.location.x, layout.location.y, layout.size.width, layout.size.height];
                let want = [x, y, item_width, 100.0];
                if got != want {
                    failures.push(format!("{name}; {direction:?}; item {index}: {got:?}, Chrome {want:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: width:min-content wrappers. The
// intrinsic width can fit several bases, or be smaller than a maximum
// that initially put several items on one line.
#[test]
fn min_content_rows_wrap_at_their_resolved_width() {
    let mut failures = Vec::new();
    for (content, basis, maximum, width, height, xs, reverse_xs, ys) in [
        (200.0, 60.0, auto(), 200.0, 100.0, [0.0, 60.0, 120.0], [140.0, 80.0, 20.0], [0.0; 3]),
        (100.0, 0.0, auto(), 100.0, 100.0, [0.0; 3], [100.0; 3], [0.0; 3]),
        (100.0, 60.0, length(400.0), 100.0, 300.0, [0.0; 3], [40.0; 3], [0.0, 100.0, 200.0]),
        (100.0, 60.0, length(90.0), 90.0, 300.0, [0.0; 3], [30.0; 3], [0.0, 100.0, 200.0]),
    ] {
        for direction in [FlexDirection::Row, FlexDirection::RowReverse] {
            let mut tree = TaffyTree::<()>::new();
            let items: Vec<_> = (0..3).map(|_| {
                let child = tree.new_leaf(Style {
                    size: Size { width: length(content), height: length(10.0) },
                    ..Default::default()
                }).unwrap();
                tree.new_with_children(Style {
                    display: Display::Block,
                    size: Size { width: auto(), height: length(100.0) },
                    min_size: Size { width: length(0.0), height: auto() },
                    flex_basis: length(basis),
                    flex_shrink: 0.0,
                    ..Default::default()
                }, &[child]).unwrap()
            }).collect();
            let row = tree.new_with_children(Style {
                flex_direction: direction,
                flex_wrap: FlexWrap::Wrap,
                max_size: Size { width: maximum, height: auto() },
                align_content: Some(AlignContent::FLEX_START),
                ..Default::default()
            }, &items).unwrap();
            let available = Size { width: AvailableSpace::MinContent, height: AvailableSpace::MaxContent };
            let expected = Size { width, height };
            for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                tree.mark_dirty(row).unwrap();
                let size = tree.as_layout_tree().measure_child_size_both(
                    row, Size::NONE, Size::NONE, available, mode, Line::FALSE,
                );
                if size != expected {
                    failures.push(format!("content {content}; basis {basis}; {direction:?}; {mode:?}: {size:?}, Chrome {expected:?}"));
                }
            }
            tree.mark_dirty(row).unwrap();
            tree.compute_layout(row, available).unwrap();
            let size = tree.layout(row).unwrap().size;
            if size != expected {
                failures.push(format!("content {content}; basis {basis}; {direction:?}; layout: {size:?}, Chrome {expected:?}"));
            }
            let xs = if direction == FlexDirection::RowReverse { reverse_xs } else { xs };
            for (index, item) in items.into_iter().enumerate() {
                let layout = tree.layout(item).unwrap();
                let got = [layout.location.x, layout.location.y, layout.size.width, layout.size.height];
                let want = [xs[index], ys[index], basis, 100.0];
                if got != want {
                    failures.push(format!("content {content}; basis {basis}; {direction:?}; item {index}: {got:?}, Chrome {want:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: width:min-content wrappers around a
// 100px item followed by two items whose nested rows contain 40px boxes.
#[test]
fn min_content_rows_remeasure_content_dependent_bases() {
    let mut failures = Vec::new();
    for (name, block, basis, preferred, maximum, item_width, height) in [
        ("nested auto", false, auto(), auto(), auto(), 80.0, 300.0),
        ("block auto", true, auto(), auto(), auto(), 80.0, 300.0),
        ("definite basis", false, length(40.0), auto(), auto(), 40.0, 200.0),
        ("capped auto", false, auto(), auto(), length(60.0), 60.0, 300.0),
        ("percentage width", false, auto(), percent(0.5), auto(), 50.0, 200.0),
    ] {
        for direction in [FlexDirection::Row, FlexDirection::RowReverse] {
            let mut tree = TaffyTree::<()>::new();
            let first = tree.new_leaf(Style {
                size: Size { width: length(100.0), height: length(100.0) },
                ..Default::default()
            }).unwrap();
            let mut items = vec![first];
            for _ in 0..2 {
                let mut children: Vec<_> = (0..2).map(|_| tree.new_leaf(Style {
                    size: Size { width: length(40.0), height: length(10.0) },
                    ..Default::default()
                }).unwrap()).collect();
                if block {
                    children = vec![tree.new_with_children(Style {
                        flex_wrap: FlexWrap::Wrap,
                        align_content: Some(AlignContent::FLEX_START),
                        ..Default::default()
                    }, &children).unwrap()];
                }
                items.push(tree.new_with_children(Style {
                    display: if block { Display::Block } else { Display::Flex },
                    size: Size { width: preferred, height: length(100.0) },
                    min_size: Size { width: length(0.0), height: auto() },
                    max_size: Size { width: maximum, height: auto() },
                    flex_basis: basis,
                    flex_wrap: FlexWrap::Wrap,
                    align_content: Some(AlignContent::FLEX_START),
                    ..Default::default()
                }, &children).unwrap());
            }
            let row = tree.new_with_children(Style {
                flex_direction: direction,
                flex_wrap: FlexWrap::Wrap,
                align_content: Some(AlignContent::FLEX_START),
                ..Default::default()
            }, &items).unwrap();
            let available = Size { width: AvailableSpace::MinContent, height: AvailableSpace::MaxContent };
            let expected = Size { width: 100.0, height };
            for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                tree.mark_dirty(row).unwrap();
                let size = tree.as_layout_tree().measure_child_size_both(
                    row, Size::NONE, Size::NONE, available, mode, Line::FALSE,
                );
                if size != expected {
                    failures.push(format!("{name}; {direction:?}; {mode:?}: {size:?}, Chrome {expected:?}"));
                }
            }
            tree.mark_dirty(row).unwrap();
            tree.compute_layout(row, available).unwrap();
            let size = tree.layout(row).unwrap().size;
            if size != expected {
                failures.push(format!("{name}; {direction:?}; layout: {size:?}, Chrome {expected:?}"));
            }
            for (index, item) in items.into_iter().enumerate() {
                let layout = tree.layout(item).unwrap();
                let (x, y, width) = match index {
                    0 => (0.0, 0.0, 100.0),
                    _ if height == 300.0 => (0.0, index as f32 * 100.0, item_width),
                    _ => ((index - 1) as f32 * item_width, 100.0, item_width),
                };
                let x = if direction == FlexDirection::RowReverse { 100.0 - x - width } else { x };
                let got = [layout.location.x, layout.location.y, layout.size.width, layout.size.height];
                let want = [x, y, width, 100.0];
                if got != want {
                    failures.push(format!("{name}; {direction:?}; item {index}: {got:?}, Chrome {want:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: rows inside width:max-content wrappers.
#[test]
fn capped_max_content_rows_measure_their_wrapped_height() {
    let mut failures = Vec::new();
    for (width, gap, maximum, expected) in [
        (60.0, 0.0, 100.0, Size { width: 100.0, height: 90.0 }),
        (100.0, 0.0, 100.0, Size { width: 100.0, height: 90.0 }),
        (60.1, 0.1, 300.0, Size { width: 180.5, height: 30.0 }),
        (60.1, 0.1, 150.0, Size { width: 150.0, height: 60.0 }),
        (73.33, 0.1, 300.0, Size { width: 220.19, height: 30.0 }),
        (73.33, 0.1, 150.0, Size { width: 150.0, height: 60.0 }),
        (60.2, 0.2, 120.6, Size { width: 120.6, height: 60.0 }),
        (60.4, 0.1, 181.4, Size { width: 181.4, height: 30.0 }),
        (60.4, 0.125, 181.4, Size { width: 181.4, height: 60.0 }),
    ] {
        for direction in [FlexDirection::Row, FlexDirection::RowReverse] {
            for wrap in [FlexWrap::Wrap, FlexWrap::WrapReverse] {
                let mut tree = TaffyTree::<()>::new();
                let items: Vec<_> = (0..3).map(|_| tree.new_leaf(Style {
                    size: Size { width: length(width), height: length(30.0) },
                    flex_shrink: 0.0,
                    ..Default::default()
                }).unwrap()).collect();
                let row = tree.new_with_children(Style {
                    flex_direction: direction,
                    flex_wrap: wrap,
                    align_content: Some(AlignContent::FLEX_START),
                    max_size: Size { width: length(maximum), height: auto() },
                    gap: Size { width: length(gap), height: length(0.0) },
                    ..Default::default()
                }, &items).unwrap();
                for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
                    tree.mark_dirty(row).unwrap();
                    let size = tree.as_layout_tree().measure_child_size_both(
                        row, Size::NONE, Size::NONE, Size::MAX_CONTENT, mode, Line::FALSE,
                    );
                    if (size.width - expected.width).abs() > 0.001 || size.height != expected.height {
                        failures.push(format!("{width}; gap {gap}; max {maximum}; {direction:?}; {wrap:?}; {mode:?}: {size:?}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

// Chrome 151.0.7922.34, 2026-10-18: a max-content grid item with a
// bounded width sizes its percentage-width square before its height.
#[test]
fn bounded_column_width_sizes_content_and_max_content_grid_height() {
    let mut failures = Vec::new();
    for (direction, maximum) in [
        (FlexDirection::Column, 200.0),
        (FlexDirection::Column, 100.0),
        (FlexDirection::ColumnReverse, 200.0),
    ] {
        let mut tree = TaffyTree::<()>::new();
        let square = tree.new_leaf(Style {
            size: Size { width: percent(1.0), height: auto() },
            aspect_ratio: Some(1.0),
            ..Default::default()
        }).unwrap();
        let column = tree.new_with_children(Style {
            display: Display::Flex,
            flex_direction: direction,
            size: Size { width: Dimension::max_content(), height: Dimension::max_content() },
            min_size: Size { width: length(200.0), height: auto() },
            max_size: Size { width: length(maximum), height: auto() },
            justify_self: Some(AlignSelf::START),
            align_self: Some(AlignSelf::START),
            ..Default::default()
        }, &[square]).unwrap();
        for mode in [SizingMode::ContentSize, SizingMode::InherentSize] {
            tree.mark_dirty(column).unwrap();
            let measured = tree.as_layout_tree().measure_child_size_both(
                column,
                Size::NONE,
                Size { width: Some(800.0), height: Some(600.0) },
                Size::MAX_CONTENT,
                mode,
                Line::FALSE,
            );
            if measured != (Size { width: 200.0, height: 200.0 }) {
                failures.push(format!("{direction:?}; max-width:{maximum}; {mode:?}: {measured:?}"));
            }
        }
        tree.mark_dirty(column).unwrap();
        let grid = tree.new_with_children(Style {
            display: Display::Grid,
            size: Size { width: length(800.0), height: auto() },
            rare: crate::style::RareStyle {
                grid_template_columns: vec![length(800.0)],
                grid_template_rows: vec![max_content()],
                ..crate::style::RareStyle::DEFAULT
            }.into(),
            ..Default::default()
        }, &[column]).unwrap();
        tree.compute_layout(grid, Size {
            width: AvailableSpace::Definite(800.0), height: AvailableSpace::Definite(600.0),
        }).unwrap();
        let frames: Vec<_> = [grid, column, square].into_iter().map(|node| {
            let layout = tree.layout(node).unwrap();
            [layout.location.x, layout.location.y, layout.size.width, layout.size.height]
        }).collect();
        let chrome = [[0.0, 0.0, 800.0, 200.0], [0.0, 0.0, 200.0, 200.0], [0.0, 0.0, 200.0, 200.0]];
        if frames != chrome {
            failures.push(format!("{direction:?}; max-width:{maximum}; grid: {frames:?}, Chrome {chrome:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
