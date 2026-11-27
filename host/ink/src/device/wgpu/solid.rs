//! Draw a large solid interior without running its edge coverage shader.
use crate::instance::{Instance, Scissor};

fn interior(i: &Instance, scissor: Scissor) -> Option<Scissor> {
    let [sx, skew_y, skew_x, sy, tx, ty] = i.xform;
    let [x, y, w, h] = i.rect;
    if skew_x != 0. || skew_y != 0. || !(sx > 0. && sy > 0.) {
        return None;
    }
    let right = x + w;
    let bottom = y + h;
    let edges = [
        x,
        y,
        right,
        bottom,
        tx,
        ty,
        sx * x + tx,
        sy * y + ty,
        sx * right + tx,
        sy * bottom + ty,
    ];
    if !edges.iter().all(|v| v.abs() < 32768.) || !i.radii.iter().all(|r| r.is_finite()) {
        return None;
    }
    // Inside shape_coverage's saturated region, even its two offset samples
    // miss the arcs. Round the scissor inward so no boundary pixel changes.
    let radius = i.radii.into_iter().fold(0f32, f32::max);
    let margin = (radius + 1.).max(4. * (sx.recip() + sy.recip()) + 1.);
    let left = ((x + margin) * sx + tx).ceil().max(0.) as u32;
    let top = ((y + margin) * sy + ty).ceil().max(0.) as u32;
    let right = ((right - margin) * sx + tx).floor().max(0.) as u32;
    let bottom = ((bottom - margin) * sy + ty).floor().max(0.) as u32;
    let inner = scissor.intersect(Scissor {
        x: left,
        y: top,
        width: right.saturating_sub(left),
        height: bottom.saturating_sub(top),
    });
    // Only split the instanced run to draw the interior and rim for sizeable fills.
    (inner.width >= 32 && inner.height >= 32).then_some(inner)
}

pub(super) fn draw(
    pass: &mut ::wgpu::RenderPass<'_>,
    edge: &::wgpu::RenderPipeline,
    solid: &::wgpu::RenderPipeline,
    s: Scissor,
    start: u32,
    run: &[Instance],
) {
    let mut pending = start;
    for (index, instance) in (start..).zip(run) {
        let Some(center) = interior(instance, s) else {
            continue;
        };
        if pending < index {
            pass.draw(0..4, pending..index);
        }
        pass.set_scissor_rect(center.x, center.y, center.width, center.height);
        pass.set_pipeline(solid);
        pass.draw(0..4, index..index + 1);
        pass.set_pipeline(edge);
        let (right, bottom) = (s.x + s.width, s.y + s.height);
        let (inner_right, inner_bottom) = (center.x + center.width, center.y + center.height);
        for (l, t, r, b) in [
            (s.x, s.y, right, center.y),
            (s.x, inner_bottom, right, bottom),
            (s.x, center.y, center.x, inner_bottom),
            (inner_right, center.y, right, inner_bottom),
        ] {
            if l < r && t < b {
                pass.set_scissor_rect(l, t, r - l, b - t);
                pass.draw(0..4, index..index + 1);
            }
        }
        pass.set_scissor_rect(s.x, s.y, s.width, s.height);
        pending = index + 1;
    }
    let end = start + run.len() as u32;
    if pending < end {
        pass.draw(0..4, pending..end);
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn solid_interiors_match_the_edge_shader_on_the_whole_frame() {
    use super::{Device, Wgpu};
    use crate::instance::Bindings;

    let mut device = Wgpu::new().unwrap();
    let target = device.create_target(160, 120).unwrap();
    let mut encoder = crate::Encoder::new(160, 120);
    encoder
        .push_clip(crate::instance::Clip::Scissor(Scissor {
            x: 3,
            y: 2,
            width: 150,
            height: 113,
        }))
        .unwrap();
    for (x, y, width, height, radii, transform, color, divided) in [
        (
            -8.,
            -8.,
            180.,
            145.,
            [0.; 4],
            [1., 0., 0., 1., 0., 0.],
            [0.4, 0.1, 0.2, 0.65],
            true,
        ),
        (
            1.25,
            2.625,
            122.,
            106.,
            [5., 9., 2., 12.],
            [1.1, 0., 0., 0.9, 0.2, -2.3],
            [0.1, 0.2, 0.3, 0.4],
            true,
        ),
        (
            0.,
            0.,
            110.,
            105.,
            [10.; 4],
            [1., 0.2, 0., 1., 1., 2.],
            [0.3, 0., 0., 0.4],
            false,
        ),
        (
            7.,
            10.,
            115.,
            99.,
            [3.; 4],
            [1., 0., 0., 1., 0., 0.],
            [0.2, 0.2, 0.2, 0.5],
            true,
        ),
        (
            4.,
            8.,
            20.,
            20.,
            [7.; 4],
            [1., 0., 0., 1., 0., 0.],
            [0.1, 0., 0.2, 0.3],
            false,
        ),
    ] {
        let mut fill = Instance::rounded([x, y, width, height], radii, color);
        fill.xform = transform;
        assert_eq!(interior(&fill, encoder.scissor()).is_some(), divided);
        encoder.push(fill, Bindings::default()).unwrap();
    }
    let frame = encoder.finish();
    device.general_only = true;
    device
        .render(&frame, target, Some([0.02, 0.1, 0.15, 0.25]))
        .unwrap();
    let expected = device.readback(target).unwrap();
    device.general_only = false;
    device
        .render(&frame, target, Some([0.02, 0.1, 0.15, 0.25]))
        .unwrap();
    assert_eq!(device.readback(target).unwrap(), expected);
}
