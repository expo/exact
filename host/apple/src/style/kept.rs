//! The layout rows a node's style keeps though its frame carries them
//! (`style_json_presented`).
use exact_kernel::props::Presentation;
use exact_kernel::{BoxSizing, NodeRef, NodeType, PropId, StyleId, StyleMask};

/// The layout rows a node's style sends though its frame carries them: a
/// video's size, which its box waits on; a popover route's, its popover's
/// preferred size (`PopoverRoute.size(of:)`); a bar item's drawn face's
/// (BarFaceIOS.swift), the authored size in points of a filled childless
/// box (a status dot) and an authored `column-gap`, since a header the bar
/// replaces is laid out as `display: none` and gives them no frame; and an
/// authored `min-height`, which a list row that UIKit's cell configuration
/// sizes keeps. Read off the node alone, as `faceBoxSize` is, so a box moved
/// into a header needs nothing recomputed. Authored rows only: few nodes
/// have a dot's size or a gap; any authored `min-height` crosses, `0` too.
pub(super) fn kept_geometry(node: &NodeRef<'_>) -> StyleMask {
    let size = StyleMask::of(StyleId::Width).union(StyleMask::of(StyleId::Height));
    if node.node_type == NodeType::Video {
        return size;
    }
    // A popover route's own size, in literal points, is its popover's preferred size (LLP 1123 D7)
    if node.props.presentation() == Some(Presentation::Popover) {
        return node.style.written.intersect(size);
    }
    let mut authored = node
        .style
        .written
        .intersect(StyleMask::of(StyleId::ColumnGap).union(StyleMask::of(StyleId::MinHeight)));
    // A named border box can be a height-drag target. UIKit needs its limit
    // before deciding whether an edge drag belongs to an enclosing scroller.
    if cfg!(target_os = "ios")
        && node.style.box_sizing == BoxSizing::BorderBox
        && node.props.str(PropId::Id).is_some()
    {
        authored = authored.union(
            node.style
                .written
                .intersect(StyleMask::of(StyleId::MaxHeight)),
        );
    }
    // A drawn box: filled, its size literal points, nothing in it. A size
    // that changes with the layout (a percentage, an env length) is no dot's.
    let points = |d: &exact_kernel::Dimension| matches!(d, exact_kernel::Dimension::Points(_));
    if node.node_type == NodeType::View
        && node.style.written.has(StyleId::BackgroundColor)
        && points(&node.style.width)
        && points(&node.style.height)
        && node.children().is_empty()
    {
        return authored.union(node.style.written.intersect(size));
    }
    authored
}
