use super::*;
use crate::image::Bitmap;
use crate::paint::Backend;
use exact_kernel::ImageRendering;
use tiny_skia::{FilterQuality, PixmapPaint, Transform};

#[test]
fn image_rendering_matches_two_pass_sampling_at_integer_and_fractional_scales() {
    let mut sprite = Pixmap::new(3, 2).unwrap();
    sprite.data_mut().copy_from_slice(&[
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255, 128, 128, 128, 128, 255, 255,
        255, 255,
    ]);
    let session = exact_raster::Gate::new().session();
    let reservation = session.reserve_allocation(24).unwrap();
    let sprite = Arc::new(Bitmap::new(sprite, (3, 2), reservation.charge()));
    let _allocation = reservation.commit(24).unwrap();
    let mut painters = painters();
    for dpr in [1.0_f32, 1.5, 2.] {
        for size in [(6.0_f32, 4.0_f32), (7., 5.), (8., 6.), (301., 177.)] {
            let width = (size.0 * dpr).round() as u32;
            let height = (size.1 * dpr).round() as u32;
            for mode in [
                ImageRendering::Auto,
                ImageRendering::CrispEdges,
                ImageRendering::Pixelated,
            ] {
                let n = if mode == ImageRendering::Pixelated {
                    [
                        (size.0 * dpr / 3.).round().max(1.) as u32,
                        (size.1 * dpr / 2.).round().max(1.) as u32,
                    ]
                } else {
                    [1, 1]
                };
                let mut enlarged = Pixmap::new(3 * n[0], 2 * n[1]).unwrap();
                for y in 0..enlarged.height() {
                    for x in 0..enlarged.width() {
                        let at = ((y / n[1]) * 3 + x / n[0]) as usize * 4;
                        let out = (y * enlarged.width() + x) as usize * 4;
                        enlarged.data_mut()[out..out + 4]
                            .copy_from_slice(&sprite.pixels().data()[at..at + 4]);
                    }
                }
                let mut expected = Pixmap::new(width, height).unwrap();
                expected.fill(tiny_skia::Color::WHITE);
                expected.draw_pixmap(
                    0,
                    0,
                    enlarged.as_ref(),
                    &PixmapPaint {
                        quality: if mode == ImageRendering::CrispEdges {
                            FilterQuality::Nearest
                        } else {
                            FilterQuality::Bilinear
                        },
                        ..Default::default()
                    },
                    Transform::from_scale(
                        size.0 * dpr / enlarged.width() as f32,
                        size.1 * dpr / enlarged.height() as f32,
                    ),
                    None,
                );
                for (name, painter) in &mut painters {
                    painter.begin(size.0, size.1, dpr);
                    painter.image(
                        &sprite,
                        (0., 0., size.0, size.1),
                        &[],
                        Transform::identity(),
                        None,
                        mode,
                    );
                    let actual = painter.finish().unwrap();
                    // Sampling is compared inside the image; outer-edge AA belongs to each backend.
                    let max = actual
                        .data()
                        .iter()
                        .zip(expected.data())
                        .enumerate()
                        .filter(|(i, _)| {
                            let x = (*i / 4) % width as usize;
                            let y = (*i / 4) / width as usize;
                            let source = [
                                (x as f32 + 0.5) * 3. / (size.0 * dpr),
                                (y as f32 + 0.5) * 2. / (size.1 * dpr),
                            ];
                            // Either nearest texel is valid exactly halfway between centres.
                            let tie = mode == ImageRendering::CrispEdges
                                && source.iter().any(|v| (v - v.round()).abs() < 0.00001);
                            !tie && x > 0
                                && x + 1 < width as usize
                                && y > 0
                                && y + 1 < height as usize
                        })
                        .map(|(_, (a, b))| a.abs_diff(*b))
                        .max()
                        .unwrap();
                    assert!(max <= 3, "{name} {mode:?} {size:?} @{dpr}: max difference {max}; first pixels {:?} != {:?}", &actual.data()[..actual.data().len().min(48)], &expected.data()[..expected.data().len().min(48)]);
                }
            }
        }
    }
}

#[test]
fn pixelated_materializes_only_visible_tiles_with_a_filter_halo() {
    let source = Pixmap::new(2, 1).unwrap();
    let mut count = 0;
    assert!(pixelated(
        source.as_ref(),
        (-1_000_000., 0., 2_000_000., 100.),
        Transform::identity(),
        [400., 100.],
        ImageRendering::Pixelated,
        |tile, _, _| {
            count += 1;
            assert!(tile.width() <= 258 && tile.height() <= 258);
        }
    ));
    assert!(
        (1..=3).contains(&count),
        "only the visible strip: {count} tiles"
    );
}

#[test]
fn retained_pixelated_images_sample_the_viewport_at_replay() {
    let mut source = Pixmap::new(2, 3).unwrap();
    source.data_mut().copy_from_slice(&[
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255,
        255,
    ]);
    let session = exact_raster::Gate::new().session();
    let reservation = session.reserve_allocation(24).unwrap();
    let source = Arc::new(Bitmap::new(source, (2, 3), reservation.charge()));
    let _allocation = reservation.commit(24).unwrap();
    for (name, mut painter) in painters() {
        if !painter.rows() {
            continue;
        }
        for (height, container) in [(600., "row"), (601., "slot"), (601., "layer")] {
            if container == "layer" && name != "Vello" {
                continue;
            }
            painter.begin(160., 256., 1.);
            painter.row_begin(1, (17., 43.), None);
            let own = if container == "layer" {
                assert!(painter.layer_begin(
                    9,
                    Transform::identity(),
                    (0., 0.),
                    [0., 37., 1.25, 0., 1., 0., 0.],
                ));
                Transform::from_translate(0., 37.).pre_scale(1.25, 1.25)
            } else {
                if container == "slot" {
                    painter.slot_begin(9);
                }
                Transform::identity()
            };
            let clips = [Shape::rect((17., 43., 120., height))];
            let tint = (container == "slot").then_some([17, 100, 200, 210]);
            painter.image(
                &source,
                (17., 43., 120., height),
                &clips,
                Transform::identity(),
                tint,
                ImageRendering::Pixelated,
            );
            if container == "layer" {
                painter.layer_end();
            } else if container == "slot" {
                painter.slot_end();
            }
            painter.row_end((17., 43., 120., height));
            for y in [43., -257., -344.] {
                painter.begin(160., 256., 1.);
                painter.row_draw(1, (17., y), (17., y, 120., height));
                let retained = painter.finish().unwrap();
                painter.begin(160., 256., 1.);
                painter.image(
                    &source,
                    (17., 43., 120., height),
                    &clips,
                    Transform::from_translate(0., y - 43.).pre_concat(own),
                    tint,
                    ImageRendering::Pixelated,
                );
                let direct = painter.finish().unwrap();
                assert_eq!(
                    retained, direct,
                    "{name}: {container}, height {height}, row y {y}"
                );
            }
            painter.row_free(1);
        }
    }
}

fn painters() -> Vec<(&'static str, Box<dyn Backend>)> {
    let mut painters: Vec<(&str, Box<dyn Backend>)> =
        vec![("CPU", Box::new(crate::raster::Raster::new()))];
    for (name, painter) in [
        (
            "Ink",
            crate::ink::Ink::new().map(|p| Box::new(p) as Box<dyn Backend>),
        ),
        (
            "Vello",
            crate::gpu::Gpu::new().map(|p| Box::new(p) as Box<dyn Backend>),
        ),
    ] {
        match painter {
            Ok(painter) => painters.push((name, painter)),
            Err(why) => {
                assert_ne!(
                    std::env::var("EXACT_REQUIRED").as_deref(),
                    Ok("1"),
                    "{name}: {why}"
                );
                eprintln!("{name} sampling not checked: {why}");
            }
        }
    }
    painters
}

#[test]
fn minification_smooths_pixelated_but_keeps_crisp_edges_sharp() {
    let mut source = Pixmap::new(32, 32).unwrap();
    for (i, pixel) in source.data_mut().chunks_exact_mut(4).enumerate() {
        let value = if (i / 32 + i % 32).is_multiple_of(2) {
            0
        } else {
            255
        };
        pixel.copy_from_slice(&[value, value, value, 255]);
    }
    let session = exact_raster::Gate::new().session();
    let reservation = session.reserve_allocation(4096).unwrap();
    let source = Arc::new(Bitmap::new(source, (32, 32), reservation.charge()));
    let _allocation = reservation.commit(4096).unwrap();
    for (name, mut painter) in painters() {
        for mode in [ImageRendering::Pixelated, ImageRendering::CrispEdges] {
            painter.begin(3., 3., 1.);
            painter.image(
                &source,
                (0., 0., 3., 3.),
                &[],
                Transform::identity(),
                None,
                mode,
            );
            let result = painter.finish().unwrap();
            let value = result.pixel(1, 1).unwrap().red();
            if mode == ImageRendering::CrispEdges {
                assert!(value == 0 || value == 255, "{name}: crisp sample {value}");
            } else {
                assert!(value.abs_diff(128) <= 3, "{name}: smooth sample {value}");
            }
        }
    }
}
