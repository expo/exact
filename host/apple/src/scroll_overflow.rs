use super::*;

/// Rebuild overflow below a scaled child before applying its transform. Layout's
/// aggregate content includes unscaled descendants, which would keep a squashed
/// 400px descendant at 400px instead of Chrome's 225px.
pub(super) fn scaled_content(
    node: &NodeRef<'_>,
    kernel: &Kernel,
    padding: bool,
) -> Option<(f32, f32)> {
    if !kernel.has_scale() || !kernel.has_scaled_descendant(node.key) {
        return None;
    }
    use exact_kernel::svg::transform as tf;
    fn bounds(node: &NodeRef<'_>, kernel: &Kernel, padding: bool) -> ([f32; 4], bool) {
        let children = node.children();
        let mut rect = [0.0, 0.0, node.frame.width, node.frame.height];
        let mut scaled = false;
        let (_, _, pad_right, pad_bottom) = if padding {
            kernel.layout_padding(node.key).unwrap_or_default()
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };
        for id in children.iter().copied() {
            let Some(child) = kernel.node(id) else {
                continue;
            };
            if kernel.host_block(id).is_some()
                || child.node_type.is_metadata()
                || child.props.str(PropId::SemanticTag) == Some("dialog")
                || child.props.str(PropId::Popover).is_some()
            {
                continue;
            }
            let s = child.style;
            let (right, bottom) = if child.layout_position() == exact_kernel::PositionType::Absolute
            {
                (0.0, 0.0)
            } else {
                (pad_right, pad_bottom)
            };
            let visible =
                style::effective_overflow(&child) == (Overflow::Visible, Overflow::Visible);
            let (inside, below) = if visible {
                bounds(&child, kernel, true)
            } else {
                ([0.0, 0.0, child.frame.width, child.frame.height], false)
            };
            scaled |= below || s.scale.x != 1.0 || s.scale.y != 1.0;
            let x = child.frame.x - node.frame.x;
            let y = child.frame.y - node.frame.y;
            let (ox, oy) = s
                .transform_origin
                .resolve(child.frame.width, child.frame.height);
            let t = tf::mul(
                tf::translate(
                    x + ox + s.translate.x + s.translate_percent.x * child.frame.width / 100.0,
                    y + oy + s.translate.y + s.translate_percent.y * child.frame.height / 100.0,
                ),
                tf::mul(
                    tf::rotate(if s.rotate_axis.is_3d() { 0.0 } else { s.rotate }),
                    tf::mul(tf::scale(s.scale.x, s.scale.y), tf::translate(-ox, -oy)),
                ),
            );
            // CSS keeps the child's original border box as well as its
            // transformed overflow, rather than shrinking the scroll area.
            rect[0] = rect[0].min(x);
            rect[1] = rect[1].min(y);
            rect[2] = rect[2].max(x + child.frame.width + right);
            rect[3] = rect[3].max(y + child.frame.height + bottom);
            if s.scale.x == 0.0 || s.scale.y == 0.0 {
                continue;
            }
            for point in [
                (inside[0], inside[1]),
                (inside[2], inside[1]),
                (inside[2], inside[3]),
                (inside[0], inside[3]),
            ] {
                let (px, py) = tf::apply(t, point);
                rect[0] = rect[0].min(px);
                rect[1] = rect[1].min(py);
                rect[2] = rect[2].max(px + right);
                rect[3] = rect[3].max(py + bottom);
            }
        }
        if !scaled {
            rect[2] = rect[2].max(node.content.0);
            rect[3] = rect[3].max(node.content.1);
        }
        (rect, scaled)
    }
    let (rect, scaled) = bounds(node, kernel, padding);
    scaled.then_some((rect[2], rect[3]))
}
