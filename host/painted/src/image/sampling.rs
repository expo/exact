//! CSS Images 3 §5.3's integer nearest-neighbour enlargement, followed by smoothing.
use crate::paint::{Rect4, Shape};
use exact_kernel::ImageRendering;
use std::sync::Arc;
use tiny_skia::{Pixmap, PixmapRef, Point, Transform};

/// Paint bounded pieces of the virtual integer-sized image, with a one-pixel
/// halo for the final bilinear filter. Only pieces intersecting the viewport
/// are materialized; neither a large source nor a large scale allocates it whole.
pub(crate) fn pixelated(
    source: PixmapRef<'_>,
    dst: Rect4,
    device: Transform,
    viewport: [f32; 2],
    rendering: ImageRendering,
    mut paint: impl FnMut(Arc<Pixmap>, Rect4, Shape),
) -> bool {
    if rendering != ImageRendering::Pixelated {
        return false;
    }
    let scale = [
        dst.2 * device.sx.hypot(device.ky) / source.width() as f32,
        dst.3 * device.kx.hypot(device.sy) / source.height() as f32,
    ];
    let n = scale.map(|s| s.round().max(1.));
    if n == [1., 1.] {
        return false;
    }
    let virtual_size = [source.width() as f32 * n[0], source.height() as f32 * n[1]];
    let step = [dst.2 / virtual_size[0], dst.3 / virtual_size[1]];
    let virtual_to_device =
        device.pre_concat(Transform::from_translate(dst.0, dst.1).pre_scale(step[0], step[1]));
    let Some(inverse) = virtual_to_device.invert().filter(Transform::is_finite) else {
        return true;
    };
    let mut corners = [
        Point::from_xy(0., 0.),
        Point::from_xy(viewport[0], 0.),
        Point::from_xy(0., viewport[1]),
        Point::from_xy(viewport[0], viewport[1]),
    ];
    inverse.map_points(&mut corners);
    let bounds = [
        corners
            .iter()
            .map(|p| p.x)
            .fold(f32::INFINITY, f32::min)
            .max(0.),
        corners
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min)
            .max(0.),
        corners
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(virtual_size[0]),
        corners
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max)
            .min(virtual_size[1]),
    ];
    let start = [
        (bounds[0] / 256.).floor() as u64 * 256,
        (bounds[1] / 256.).floor() as u64 * 256,
    ];
    let end = [
        bounds[2].ceil().max(0.) as u64,
        bounds[3].ceil().max(0.) as u64,
    ];
    let full = virtual_size.map(|s| s as u64);
    for y in (start[1]..end[1]).step_by(256) {
        for x in (start[0]..end[0]).step_by(256) {
            let core = [x, y, (x + 256).min(full[0]), (y + 256).min(full[1])];
            let halo = [
                x.saturating_sub(1),
                y.saturating_sub(1),
                (core[2] + 1).min(full[0]),
                (core[3] + 1).min(full[1]),
            ];
            let Some(mut pixels) =
                Pixmap::new((halo[2] - halo[0]) as u32, (halo[3] - halo[1]) as u32)
            else {
                continue;
            };
            let width = pixels.width() as usize;
            for (i, pixel) in pixels.data_mut().chunks_exact_mut(4).enumerate() {
                let sx =
                    (((halo[0] + (i % width) as u64) as f32 / n[0]) as u32).min(source.width() - 1);
                let sy = (((halo[1] + (i / width) as u64) as f32 / n[1]) as u32)
                    .min(source.height() - 1);
                let at = (sy as usize * source.width() as usize + sx as usize) * 4;
                pixel.copy_from_slice(&source.data()[at..at + 4]);
            }
            let rect = |b: [u64; 4]| {
                (
                    dst.0 + b[0] as f32 * step[0],
                    dst.1 + b[1] as f32 * step[1],
                    (b[2] - b[0]) as f32 * step[0],
                    (b[3] - b[1]) as f32 * step[1],
                )
            };
            paint(Arc::new(pixels), rect(halo), Shape::rect(rect(core)));
        }
    }
    true
}

#[cfg(test)]
#[path = "sampling_tests.rs"]
mod tests;
