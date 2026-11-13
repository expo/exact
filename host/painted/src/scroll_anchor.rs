//! CSS Scroll Anchoring's block-axis selection and suppression, shared by
//! every Linux layout entry. Snapshots are armed at the presenter's current
//! offsets, before a runner commit can replace styles or geometry.
//! https://drafts.csswg.org/css-scroll-anchoring/#description
use crate::paint::{effective_overflow, space, Presented};
use exact_kernel::{
    Display, Kernel, NodeKey, NodeRef, Overflow, PositionType, PropId, StyleId, StyleProps, ViewId,
};
use std::collections::BTreeMap;

const PAGE: ViewId = crate::presenter::PAGE;

impl<D: exact_runner::DataSource> crate::host::Host<D> {
    pub(crate) fn arm_scroll_anchors(
        &mut self,
        offsets: &[(ViewId, (f32, f32))],
        viewport: (f32, f32),
    ) {
        let mut state = std::mem::take(&mut self.scroll_anchors);
        state.arm(self.kernel(), &self.roots(), offsets, viewport);
        self.scroll_anchors = state;
    }
}

#[derive(Default)]
pub(crate) struct State {
    anchors: BTreeMap<ViewId, Anchor>,
    corrections: BTreeMap<ViewId, f32>,
}

struct Anchor {
    key: NodeKey,
    y: f32,
    offset: (f32, f32),
    viewport: (f32, f32),
    chain: Vec<(NodeKey, StyleProps)>,
    positioned: BTreeMap<NodeKey, bool>,
    requested: Option<f64>,
    scroller: Option<(NodeKey, StyleProps)>,
}

impl State {
    pub(crate) fn arm(
        &mut self,
        kernel: &Kernel,
        roots: &[ViewId],
        offsets: &[(ViewId, (f32, f32))],
        viewport: (f32, f32),
    ) {
        self.anchors = offsets
            .iter()
            .filter_map(|&(id, offset)| {
                capture(kernel, roots, id, offset, viewport).map(|anchor| (id, anchor))
            })
            .collect();
    }

    pub(crate) fn laid_out(&mut self, kernel: &Kernel, roots: &[ViewId]) {
        self.anchors = std::mem::take(&mut self.anchors)
            .into_iter()
            .filter_map(|(id, anchor)| {
                let mut offset = anchor.offset;
                if requested(kernel, id) != anchor.requested {
                    self.corrections.remove(&id);
                    return None;
                }
                if let Some(shift) = anchor.shift(kernel, id) {
                    offset.1 += shift;
                    *self.corrections.entry(id).or_default() += shift;
                }
                capture(kernel, roots, id, offset, anchor.viewport).map(|next| (id, next))
            })
            .collect();
    }

    pub(crate) fn renewed(&mut self, view: ViewId, key: NodeKey) {
        self.corrections.remove(&view);
        self.anchors.retain(|&id, anchor| {
            let stays = id != view && !anchor.chain.iter().any(|(k, _)| *k == key);
            if !stays {
                self.corrections.remove(&id);
            }
            stays
        });
    }

    pub(crate) fn take(&mut self) -> BTreeMap<ViewId, f32> {
        std::mem::take(&mut self.corrections)
    }
}

const LAYOUT: &[StyleId] = &[
    StyleId::Top,
    StyleId::Left,
    StyleId::Right,
    StyleId::Bottom,
    StyleId::PositionType,
    StyleId::MarginTop,
    StyleId::MarginLeft,
    StyleId::MarginRight,
    StyleId::MarginBottom,
    StyleId::PaddingTop,
    StyleId::PaddingLeft,
    StyleId::PaddingRight,
    StyleId::PaddingBottom,
    StyleId::Width,
    StyleId::Height,
    StyleId::MinWidth,
    StyleId::MinHeight,
    StyleId::MaxWidth,
    StyleId::MaxHeight,
    StyleId::Transform,
    StyleId::Translate,
    StyleId::TranslatePercent,
    StyleId::TranslateZ,
    StyleId::Rotate,
    StyleId::Scale,
];

fn layout_changed(before: &StyleProps, after: &StyleProps) -> bool {
    // Rotate's numeric value holds the angle without its resolved axis.
    before.rotate_axis != after.rotate_axis
        || LAYOUT.iter().any(|&id| before.get(id) != after.get(id))
}

fn requested(kernel: &Kernel, id: ViewId) -> Option<f64> {
    kernel.node(id)?.props.get(PropId::ScrollTop)?.as_float()
}

fn out_of_flow(node: NodeRef<'_>) -> bool {
    // A popover lays out absolute too (the kernel's top layer), unless sticky.
    node.layout_position() == PositionType::Absolute
}

fn contains_absolute(node: NodeRef<'_>) -> bool {
    // A popover lays out absolute (the kernel's top layer), unless sticky.
    node.layout_position() != PositionType::Static
        || [
            StyleId::Transform,
            StyleId::Translate,
            StyleId::TranslatePercent,
            StyleId::Rotate,
            StyleId::Scale,
            StyleId::Filter,
        ]
        .iter()
        .any(|&id| node.style.mask.has(id))
}

fn excluded(kernel: &Kernel, node: NodeRef<'_>, scroller: ViewId) -> bool {
    // A box in a block of its own (a popover's, the viewport) is in that
    // block's space, and scrolls with no scroller here.
    if node.style.display == Display::None || kernel.host_block(node.id).is_some() {
        return true;
    }
    // A popover lays out absolute (the kernel's top layer), unless sticky.
    match node.layout_position() {
        PositionType::Sticky => true,
        PositionType::Absolute if scroller != PAGE => {
            let mut at = node.parent;
            while let Some(parent) = at.and_then(|id| kernel.node(id)) {
                if contains_absolute(parent) {
                    return false;
                }
                if parent.id == scroller {
                    break;
                }
                at = parent.parent;
            }
            true
        }
        _ => false,
    }
}

fn scrolls(node: NodeRef<'_>) -> bool {
    let (x, y) = effective_overflow(&node);
    !matches!(x, Overflow::Visible | Overflow::Clip)
        || !matches!(y, Overflow::Visible | Overflow::Clip)
}

// Frames share layout coordinates; transforms compose only below the scrollport.
fn bounds(kernel: &Kernel, node: NodeRef<'_>, scroller: ViewId) -> Option<(f32, f32, f32, f32)> {
    let f = node.frame;
    let mut transform = [1., 0., 0., 0., 1., 0., 0., 0., 1.];
    let mut at = Some(node.id);
    while let Some(n) = at
        .filter(|&id| id != scroller)
        .and_then(|id| kernel.node(id))
    {
        let local = space::mapping(kernel, &n, &Presented::from_style(n.style));
        transform = space::mul3(&local, &transform);
        at = n.parent;
    }
    let bounds = crate::placement::clipped_bounds(
        &transform,
        [[0., 0., 1.]; 2],
        (f.x, f.y, f.width, f.height),
    );
    (bounds.2 > 0. && bounds.3 > 0.).then_some(bounds)
}

fn select(kernel: &Kernel, nodes: &[ViewId], scroller: ViewId, port: [f32; 4]) -> Option<NodeKey> {
    let [x, y, w, h] = port;
    for &id in nodes {
        let Some(node) = kernel.node(id) else {
            continue;
        };
        if excluded(kernel, node, scroller) {
            continue;
        }
        let Some((nx, ny, nw, nh)) = bounds(kernel, node, scroller) else {
            continue;
        };
        if nw <= 0. || nh <= 0. || nx >= x + w || ny >= y + h || nx + nw <= x || ny + nh <= y {
            continue;
        }
        let fully = nx >= x && ny >= y && nx + nw <= x + w && ny + nh <= y + h;
        if !fully && !scrolls(node) {
            if let Some(child) = select(kernel, &node.children(), scroller, port) {
                return Some(child);
            }
        }
        return Some(node.key);
    }
    None
}

fn descendants(kernel: &Kernel, nodes: &[ViewId], out: &mut BTreeMap<NodeKey, bool>) {
    for &id in nodes {
        if let Some(node) = kernel.node(id) {
            out.insert(node.key, out_of_flow(node));
            descendants(kernel, &node.children(), out);
        }
    }
}

fn capture(
    kernel: &Kernel,
    roots: &[ViewId],
    id: ViewId,
    offset: (f32, f32),
    viewport: (f32, f32),
) -> Option<Anchor> {
    if offset.1 <= 0. {
        return None;
    }
    let (nodes, port, scroller) = if id == PAGE {
        (
            roots.to_vec(),
            [offset.0, offset.1, viewport.0, viewport.1],
            None,
        )
    } else {
        let node = kernel.node(id)?;
        if node.style.display == Display::None
            || !matches!(
                effective_overflow(&node).1,
                Overflow::Auto | Overflow::Scroll
            )
        {
            return None;
        }
        let f = node.frame;
        let [top, right, bottom, left] = node.style.border_widths();
        (
            node.children(),
            [
                f.x + left + offset.0,
                f.y + top + offset.1,
                (f.width - left - right).max(0.),
                (f.height - top - bottom).max(0.),
            ],
            Some((node.key, node.style.clone())),
        )
    };
    let key = select(kernel, &nodes, id, port)?;
    let node = kernel.node_by_key(key)?;
    let y = bounds(kernel, node, id)?.1 - kernel.node(id).map_or(0., |n| n.frame.y);
    let mut chain = Vec::new();
    let mut at = Some(node.id);
    while let Some(node) = at.filter(|&n| n != id).and_then(|n| kernel.node(n)) {
        chain.push((node.key, node.style.clone()));
        at = node.parent;
    }
    let mut positioned = BTreeMap::new();
    descendants(kernel, &nodes, &mut positioned);
    Some(Anchor {
        key,
        y,
        offset,
        viewport,
        chain,
        positioned,
        requested: requested(kernel, id),
        scroller,
    })
}

impl Anchor {
    fn shift(&self, kernel: &Kernel, scroller: ViewId) -> Option<f32> {
        match (kernel.node(scroller), &self.scroller) {
            (Some(node), Some((key, style))) => {
                if node.key != *key || layout_changed(style, node.style) {
                    return None;
                }
            }
            (None, None) => {}
            _ => return None,
        }
        let node = kernel.node_by_key(self.key)?;
        let mut at = Some(node.id);
        for (key, style) in &self.chain {
            let current = kernel.node(at?)?;
            if current.key != *key
                || excluded(kernel, current, scroller)
                || layout_changed(style, current.style)
            {
                return None;
            }
            at = current.parent;
        }
        if at != (scroller != PAGE).then_some(scroller) {
            return None;
        }
        for (key, was) in &self.positioned {
            if let Some(current) = kernel.node_by_key(*key) {
                if out_of_flow(current) != *was {
                    return None;
                }
            }
        }
        Some(
            bounds(kernel, node, scroller)?.1
                - kernel.node(scroller).map_or(0., |n| n.frame.y)
                - self.y,
        )
    }
}

#[cfg(test)]
#[test]
fn an_anchor_crossing_the_viewer_keeps_its_visible_half() {
    let source = r#"component App
  view
    scroll testId="port" width=400 height=400
      box height=48
      box perspective=100 perspective-origin="50% 0" height=200
        box testId="anchor" width=400 height=200 rotate="y 75deg"
      box height=1000
"#;
    let (p, error) = crate::presenter::Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        std::path::PathBuf::new(),
        crate::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    let kernel = p.host().kernel();
    let key = kernel.find_by_test_id("anchor")[0];
    let node = kernel.node_by_key(key).unwrap();
    let port = kernel
        .node_by_key(kernel.find_by_test_id("port")[0])
        .unwrap()
        .id;
    let f = node.frame;
    let h = space::mapping(kernel, &node, &Presented::from_style(node.style));
    assert!(space::mapped_bounds(&h, (f.x, f.y, f.width, f.height)).is_none());
    let clipped =
        crate::placement::clipped_bounds(&h, [[0., 0., 1.]; 2], (f.x, f.y, f.width, f.height));
    assert!(clipped.2 > 0. && clipped.3 > 0.);
    let anchor = capture(kernel, &p.host().roots(), port, (0., 100.), (400., 400.)).unwrap();
    assert_eq!(anchor.key, key);
    assert_eq!(anchor.y, clipped.1);
}
