//! A flex item's content-based automatic minimum (CSS Flexbox §4.5) where
//! it decides a frame. Each case is one where measuring the minimum later, or
//! not at all when the line seems to fit, moved a box (LLP 1167). Each frame
//! but the scrollbar case's is headless Chrome 151's for the same CSS; Chrome
//! on macOS overlays its scrollbars, so that one follows Taffy's reserved gutter.

use taffy::compute_leaf_layout;
use taffy::prelude::*;
use taffy::tree::{LayoutInput, LayoutOutput};
use taffy::{Overflow, Point};

/// A leaf whose min-content width is `.0` and max-content width `.1`, 10 points tall.
type Widths = (f32, f32);

fn measure(
    input: LayoutInput,
    _: NodeId,
    context: Option<&mut Widths>,
    style: &Style,
) -> LayoutOutput {
    let (min, max) = context.map_or((0.0, 0.0), |w| *w);
    compute_leaf_layout(
        input,
        style,
        |_, _| 0.0,
        |known, space| Size {
            width: known.width.unwrap_or(match space.width {
                AvailableSpace::Definite(w) => w.clamp(min, max.max(min)),
                AvailableSpace::MinContent => min,
                AvailableSpace::MaxContent => max,
            }),
            height: known.height.unwrap_or(10.0),
        },
    )
}

fn row(width: f32) -> Style {
    Style {
        display: Display::Flex,
        size: Size {
            width: length(width),
            height: Dimension::auto(),
        },
        ..Style::default()
    }
}

fn frame(tree: &TaffyTree<Widths>, node: NodeId) -> (f32, f32, f32, f32) {
    let l = tree.layout(node).unwrap();
    (l.location.x, l.location.y, l.size.width, l.size.height)
}

fn lay_out(tree: &mut TaffyTree<Widths>, root: NodeId) {
    tree.compute_layout_with_measure(root, Size::MAX_CONTENT, measure)
        .unwrap();
}

#[test]
fn padding_floors_an_item_whose_max_width_is_zero() {
    // Centred in 100 points, an empty item with 10 points of padding each side
    // and `max-width: 0` is 20 wide at 40, its padding its minimum.
    let mut tree = TaffyTree::new();
    let item = tree
        .new_leaf_with_context(
            Style {
                box_sizing: BoxSizing::BorderBox,
                size: Size {
                    width: Dimension::auto(),
                    height: length(10.0),
                },
                max_size: Size {
                    width: LengthPercentageAuto::length(0.0),
                    height: LengthPercentageAuto::auto(),
                },
                padding: Rect {
                    left: length(10.0),
                    right: length(10.0),
                    top: zero(),
                    bottom: zero(),
                },
                ..Style::default()
            },
            (0.0, 0.0),
        )
        .unwrap();
    let root = tree
        .new_with_children(
            Style {
                justify_content: Some(JustifyContent::CENTER),
                ..row(100.0)
            },
            &[item],
        )
        .unwrap();
    lay_out(&mut tree, root);
    assert_eq!(frame(&tree, item), (40.0, 0.0, 20.0, 10.0));
}

#[test]
fn a_gap_that_overflows_the_line_floors_its_items() {
    // Two 100-point items that may grow, 120 points apart in 300: the line is
    // 20 short, and each keeps its 100-point minimum.
    let mut tree = TaffyTree::new();
    let item = |tree: &mut TaffyTree<Widths>| {
        tree.new_leaf_with_context(
            Style {
                flex_grow: 0.25,
                ..Style::default()
            },
            (100.0, 100.0),
        )
        .unwrap()
    };
    let (a, b) = (item(&mut tree), item(&mut tree));
    let root = tree
        .new_with_children(
            Style {
                gap: Size {
                    width: length(120.0),
                    height: zero(),
                },
                ..row(300.0)
            },
            &[a, b],
        )
        .unwrap();
    lay_out(&mut tree, root);
    assert_eq!((frame(&tree, a).2, frame(&tree, b).2), (100.0, 100.0));
}

#[test]
fn max_width_caps_a_content_minimum_growing_and_shrinking() {
    // An 80-point item that may grow, capped at 50: its content-based minimum is
    // capped there too (§4.5), so it is 50 in a row with room (300) and in one
    // without (40), where it overflows.
    for width in [300.0, 40.0] {
        let mut tree = TaffyTree::new();
        let item = tree
            .new_leaf_with_context(
                Style {
                    flex_grow: 1.0,
                    max_size: Size {
                        width: LengthPercentageAuto::length(50.0),
                        height: LengthPercentageAuto::auto(),
                    },
                    ..Style::default()
                },
                (80.0, 80.0),
            )
            .unwrap();
        let root = tree.new_with_children(row(width), &[item]).unwrap();
        lay_out(&mut tree, root);
        assert_eq!(frame(&tree, item).2, 50.0, "in a row {width} wide");
    }
}

#[test]
fn a_scrollbar_wider_than_the_row_still_floors_its_item() {
    // A zero-width row that scrolls vertically, its 10-point scrollbar wider than
    // its content box, holding a 20-point item pulled left 20: the item keeps 20.
    let mut tree = TaffyTree::new();
    let item = tree
        .new_leaf_with_context(
            Style {
                margin: Rect {
                    left: LengthPercentageAuto::length(-20.0),
                    ..Rect::zero()
                },
                ..Style::default()
            },
            (20.0, 20.0),
        )
        .unwrap();
    let root = tree
        .new_with_children(
            Style {
                overflow: Point {
                    x: Overflow::Visible,
                    y: Overflow::Scroll,
                },
                scrollbar_width: 10.0,
                ..row(0.0)
            },
            &[item],
        )
        .unwrap();
    lay_out(&mut tree, root);
    assert_eq!(frame(&tree, item).2, 20.0);
}
