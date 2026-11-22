use super::*;
use crate::raster::Raster;
use exact_kernel::gradient::Geometry;

fn open() -> Option<Ink> {
    match Ink::new() {
        Ok(ink) => Some(ink),
        Err(e) if e.starts_with("no adapter:") => {
            eprintln!("Ink unavailable: {e}");
            None
        }
        Err(e) => panic!("ink device: {e}"),
    }
}

fn compare(a: &Pixmap, b: &Pixmap) {
    let mut sum = 0u64;
    let mut over = 0;
    for (a, b) in a
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data().as_chunks::<4>().0)
    {
        let differences: Vec<_> = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).collect();
        sum += differences.iter().map(|d| *d as u64).sum::<u64>();
        over += usize::from(differences.iter().any(|d| *d > 32));
    }
    let mean = sum as f64 / a.data().len() as f64;
    let percent = over as f64 * 100. / (a.width() * a.height()) as f64;
    assert!(
        mean < 5. && percent < 6.,
        "mean {mean:.3}/255, {percent:.3}% beyond 32"
    );
}

#[test]
fn geometry_survives_nested_layers_transforms_and_resizes() {
    let Some(mut ink) = open() else {
        return;
    };
    let mut cpu = Raster::new();
    for scale in [1., 1.5, 2.] {
        for transform in [
            Transform::identity(),
            Transform::from_row(0.9, 0.2, -0.15, 0.85, 15., 2.),
        ] {
            for backend in [&mut cpu as &mut dyn Backend, &mut ink] {
                backend.page([19, 35, 62, 181]);
                backend.begin(120., 100., scale);
                let outer = Shape::new((3.25, 2.5, 109., 90.), [17., 8., 23., 11.]);
                backend.fill(&outer, [12, 96, 233, 197], transform);
                for delta in [0., 3., 6.] {
                    backend.push_clip(
                        &Shape::new((6. + delta, 7. + delta, 95. - delta, 80. - delta), [15.; 4]),
                        Transform::identity(),
                    );
                }
                backend.push_opacity(0.63);
                backend.push_clip(
                    &Shape::elliptical((11.5, 10.25, 92., 70.), [(25., 17.); 4]),
                    transform,
                );
                let shape = Shape::elliptical((8., 9., 97., 73.), [(27., 18.); 4]);
                backend.fill_gradient(
                    &shape,
                    &GradientPaint {
                        geometry: Geometry::Linear {
                            start: (10., 10.),
                            end: (100., 80.),
                        },
                        repeat: false,
                        stops: vec![
                            (0., exact_kernel::Color::rgba(255, 0, 0, 200)),
                            (1., exact_kernel::Color::rgba(0, 200, 255, 50)),
                        ],
                    },
                    transform,
                );
                backend.fill(
                    &Shape::new((25., 20., 65., 50.), [12.; 4]),
                    [220, 50, 40, 173],
                    transform,
                );
                backend.pop_clip();
                backend.pop_opacity();
                for _ in 0..3 {
                    backend.pop_clip();
                }
                for part in crate::paint::border::border_fills(
                    &outer,
                    [3., 5., 7., 4.],
                    [[240, 65, 90, 191]; 4],
                ) {
                    backend.fill_border(&part, transform);
                }
                backend.pointer(90., 65.);
                backend.push_clip(&Shape::rect((30., 10., -5., 40.)), Transform::identity());
                backend.fill(
                    &Shape::rect((0., 0., 120., 100.)),
                    [255, 0, 255, 255],
                    Transform::identity(),
                );
                backend.pop_clip();
            }
            compare(&cpu.finish().unwrap(), &ink.finish().unwrap());
        }
    }
}

#[test]
fn image_revisions_and_mips_preserve_alpha_and_group_opacity() {
    let Some(mut ink) = open() else {
        return;
    };
    let mut cpu = Raster::new();
    let photo = Pixmap::decode_png(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/fixtures/picker/photo.png"
    )))
    .unwrap();
    let natural = (photo.width(), photo.height());
    let bytes = photo.data().len() as u64;
    let gate = exact_raster::Gate::new();
    let session = gate.session();
    let reservation = session.reserve_allocation(bytes).unwrap();
    let photo = Arc::new(Bitmap::new(photo, natural, reservation.charge()));
    reservation.commit(bytes).unwrap();
    for (width, height) in [(80., 53.), (43., 29.), (17., 11.)] {
        for backend in [&mut cpu as &mut dyn Backend, &mut ink] {
            backend.begin(width, height, 1.);
            backend.image(
                &photo,
                (0., 0., width, height),
                &[],
                Transform::identity(),
                None,
                exact_kernel::ImageRendering::Auto,
            );
        }
        compare(&cpu.finish().unwrap(), &ink.finish().unwrap());
    }
    for revision in 0..2 {
        let mut pixels = Pixmap::new(256, 128).unwrap();
        for (n, p) in pixels.pixels_mut().iter_mut().enumerate() {
            *p = tiny_skia::Color::from_rgba8(
                (n % 256) as u8,
                (n / 256 * 2) as u8,
                revision * 127,
                173,
            )
            .premultiply()
            .to_color_u8();
        }
        let pixels = Arc::new(pixels);
        for _ in 0..2 {
            for backend in [&mut cpu as &mut dyn Backend, &mut ink] {
                backend.begin(90., 70., 1.);
                backend.push_opacity(0.7);
                backend.canvas(
                    &pixels,
                    (7., 11., 64., 32.),
                    &[Shape::new((7., 11., 64., 32.), [9.; 4])],
                    Transform::identity(),
                );
                backend.pop_opacity();
                backend.island_image(
                    pixels.clone(),
                    (4., 50., 32., 16.),
                    Transform::identity(),
                    0,
                );
            }
            compare(&cpu.finish().unwrap(), &ink.finish().unwrap());
        }
    }
}

#[test]
fn finish_returns_a_group_error_without_hanging() {
    let (send, receive) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut ink = Ink::new().expect("native adapter");
        ink.begin(8., 8., 1.);
        ink.push_opacity(0.5);
        ink.error = Some("allocation refused".into());
        let before = exact_ink::counters::snapshot();
        let error = ink.finish().unwrap_err();
        if let Some(before) = before {
            let after = exact_ink::counters::snapshot().unwrap();
            let attempts = exact_ink::counters::Counter::PaintAttempts as usize;
            assert_eq!(after.values[attempts], before.values[attempts] + 1);
        }
        send.send(error).unwrap();
    });
    assert_eq!(
        receive
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap(),
        "allocation refused"
    );
}

#[test]
fn shadow_exclusion_packs_device_pixel_radii() {
    let mut ink = Ink::new().unwrap();
    ink.begin(80., 80., 2.);
    let shape = Shape::new((10., 10., 40., 40.), [7.34375; 4]);
    assert!(ink.blurred_shadow(&shape, [0, 0, 0, 255], 2., &shape, Transform::identity()));
    let frame = ink.encoder.take_frame();
    let i = frame.instances[0];
    assert_eq!(i.clip1, [20., 20., 80., 80.]);
    assert_eq!(i.clip_radii[2] & 65535, 235);
}

#[test]
fn clipped_backdrops_keep_the_unclipped_filter_samples() {
    use exact_kernel::style::{BackdropFilter, BackdropOp};
    let Some(mut ink) = open() else { return };
    let ts = Transform::identity();
    for scale in [1., 1.5] {
        for transform in [ts, Transform::from_row(0.9, 0.2, -0.15, 1., 8., -7.)] {
            let filters = std::iter::once(BackdropFilter(vec![])).chain(
                [0., 1., 4., 11., 13., 25., 50.]
                    .into_iter()
                    .flat_map(|sigma| {
                        [
                            BackdropFilter(vec![
                                BackdropOp::Blur(sigma),
                                BackdropOp::Saturate(0.4),
                            ]),
                            BackdropFilter(vec![
                                BackdropOp::Saturate(1.8),
                                BackdropOp::Blur(sigma),
                            ]),
                        ]
                    }),
            );
            for filter in filters {
                for clip in [
                    (40., 32., 16., 12.),
                    (0., 0., 12., 10.),
                    (146., 108., 14., 12.),
                ] {
                    let mut frames = Vec::new();
                    for mode in 0..3 {
                        ink.begin(160., 120., scale);
                        ink.push_opacity(0.73);
                        for n in 0..20 {
                            ink.fill(
                                &Shape::rect((n as f32 * 8., 0., 8., 120.)),
                                [(n * 51) as u8, (n * 17) as u8, 190, 210],
                                ts,
                            );
                        }
                        if mode == 2 {
                            ink.push_clip(&Shape::rect(clip), ts);
                        }
                        if mode != 0 {
                            ink.backdrop_filter(
                                &Shape::new((5.25, -3.5, 150.5, 120.), [13.; 4]),
                                &filter,
                                transform,
                            );
                        }
                        if mode == 2 {
                            ink.pop_clip();
                        }
                        ink.pop_opacity();
                        frames.push(ink.finish().unwrap());
                    }
                    let [base, full, clipped] = frames.as_slice() else {
                        unreachable!()
                    };
                    let mut maximum = 0;
                    for y in 0..clipped.height() {
                        for x in 0..clipped.width() {
                            let inside = x >= (clip.0 * scale) as u32
                                && x < ((clip.0 + clip.2) * scale) as u32
                                && y >= (clip.1 * scale) as u32
                                && y < ((clip.1 + clip.3) * scale) as u32;
                            let expected = if inside { full } else { base };
                            let at = ((y * clipped.width() + x) * 4) as usize;
                            for c in 0..4 {
                                maximum = maximum
                                    .max(clipped.data()[at + c].abs_diff(expected.data()[at + c]));
                            }
                        }
                    }
                    assert!(
                        maximum <= 1,
                        "scale {scale}, {transform:?}, {filter:?}, clip {clip:?}: {maximum}"
                    );
                }
            }
        }
    }
}

#[test]
fn overlapping_blends_match_cpu_with_transforms_clips_and_opacity() {
    let Some(mut ink) = open() else { return };
    let mut cpu = Raster::new();
    let mut pixels = Pixmap::new(2, 2).unwrap();
    pixels.fill(tiny_skia::Color::from_rgba8(180, 90, 130, 170));
    let pixels = Arc::new(pixels);
    for scale in [1., 2.] {
        for ts in [
            Transform::identity(),
            Transform::from_row(0.9, 0.3, -0.2, 1.1, 8., -9.),
            Transform::from_row(-1., 0., 0., 1., 95., 0.),
        ] {
            for b in [&mut cpu as &mut dyn Backend, &mut ink] {
                b.begin(100., 80., scale);
                b.push_opacity(0.7);
                b.fill(
                    &Shape::rect((0., 0., 100., 80.)),
                    [70, 140, 230, 160],
                    Transform::identity(),
                );
                b.push_clip(
                    &Shape::new((7., 5., 87., 71.), [9.; 4]),
                    Transform::identity(),
                );
                for (n, mode) in [1, 2, 3, 10].into_iter().enumerate() {
                    b.island_image(
                        pixels.clone(),
                        (-3.5 + n as f32 * 13., 4.25 + n as f32 * 9., 58.5, 43.5),
                        ts,
                        mode,
                    );
                }
                b.pop_clip();
                b.pop_opacity();
            }
            compare(&cpu.finish().unwrap(), &ink.finish().unwrap());
        }
    }
}

#[test]
fn moving_blends_reuse_completed_snapshot_storage() {
    let Some(mut ink) = open() else { return };
    let mut pixels = Pixmap::new(1, 1).unwrap();
    pixels.fill(tiny_skia::Color::from_rgba8(180, 90, 130, 255));
    let pixels = Arc::new(pixels);
    let mut peak_bytes = 0;
    let mut peak_targets = 0;
    for x in 2..=121 {
        ink.begin(1920., 1080., 1.);
        ink.island_image(
            pixels.clone(),
            (0., 0., 1920., 1080.),
            Transform::from_translate(x as f32, 0.),
            1,
        );
        let frame = ink.finish().unwrap();
        assert_eq!(&frame.data()[0..4], &[255; 4]);
        let inside = (540 * 1920 + 960) * 4;
        assert_eq!(&frame.data()[inside..inside + 4], &[180, 90, 130, 255]);
        peak_bytes = peak_bytes.max(ink.pool.bytes());
        peak_targets = peak_targets.max(ink.pool.retained());
    }
    println!("moving blend: {peak_targets} snapshot targets, {peak_bytes} peak bytes");
    assert_eq!(peak_targets, 1);
    assert!(peak_bytes <= 1920 * 1080 * 4);
}

#[test]
fn fractional_clip_intersections_survive_retained_replay_and_shadows() {
    let Some(mut ink) = open() else { return };
    let identity = Transform::identity();
    let masks = [
        Shape::new((-2.75, 3.25, 54., 41.), [11., 7., 10., 12.]),
        Shape::new((2.375, 1.625, 50., 41.5), [9., 6., 12., 8.]),
        Shape::elliptical((0.125, 5.25, 49., 37.), [(13.5, 8.25); 4]),
        Shape::elliptical((3.5, 6.375, 44., 36.), [(11., 9.); 4]),
    ];
    for scale in [1., 1.25, 2.] {
        for nested in [false, true] {
            let mut draw = |order: &[usize], retained: bool| {
                ink.page([0; 4]);
                ink.begin(68., 50., scale);
                if retained {
                    ink.row_begin(1, (0., 0.), None);
                }
                for &index in order {
                    ink.push_clip(&masks[index], identity);
                }
                if nested {
                    ink.push_clip(&masks[3], identity);
                }
                ink.fill(
                    &Shape::rect((0., 0., 68., 50.)),
                    [210, 62, 97, 255],
                    identity,
                );
                let shadow = Shape::new((12.25, 13.5, 19., 13.), [4.; 4]);
                assert!(ink.blurred_shadow(
                    &shadow,
                    [81, 101, 163, 128],
                    3.,
                    &shadow.inset(1.),
                    identity
                ));
                for _ in 0..order.len() + usize::from(nested) {
                    ink.pop_clip();
                }
                if retained {
                    let bounds = (0., 0., 68., 50.);
                    ink.row_end(bounds);
                    ink.begin(68., 50., scale);
                    ink.row_draw(1, (0., 0.), bounds);
                }
                let pixels = ink.finish().unwrap();
                if retained {
                    ink.row_free(1);
                }
                pixels
            };
            let reference = draw(&[2, 0, 1], false);
            let actual = draw(&[0, 1, 2], true);
            assert!(reference
                .data()
                .chunks_exact(4)
                .any(|p| (1..255).contains(&p[3])));
            let maximum = reference
                .data()
                .iter()
                .zip(actual.data())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                maximum <= 2,
                "scale {scale}, nested mask {nested}, maximum difference {maximum}/255"
            );
        }
    }
}

/// A shape clip takes one intermediate draw for each parent clip it cannot
/// share a draw with, and its mask covers the target's last pixel.
#[test]
fn a_shape_clip_draws_only_the_masks_its_parents_need() {
    let Some(mut ink) = open() else { return };
    let identity = Transform::identity();
    let scissor = Shape::rect((0., 0., 64., 64.));
    let round = Shape::new((1., 1., 80., 80.), [8.; 4]);
    let ellipse = Shape::elliptical((1., 1., 80., 80.), [(9., 6.); 4]);
    let clip = Shape::elliptical((2., 2., 80., 80.), [(7., 5.); 4]);
    for (parents, draws) in [
        (&[&scissor][..], 0),
        (&[&round, &round][..], 1),
        (&[&ellipse][..], 2),
    ] {
        ink.page([0; 4]);
        ink.begin(64., 64., 1.);
        for parent in parents {
            ink.push_clip(parent, identity);
        }
        let before = ink.pool.used();
        ink.push_clip(&clip, identity);
        assert_eq!(ink.pool.used() - before, draws);
        let fill = Shape::rect((-8., -8., 96., 96.));
        ink.fill(&fill, [255; 4], identity);
        for _ in 0..=parents.len() {
            ink.pop_clip();
        }
        let pixels = ink.finish().unwrap();
        assert_eq!(pixels.data()[(63 * 64 + 63) * 4 + 3], 255);
    }
}

#[test]
fn opacity_layers_preserve_fractional_edges_after_flush_and_replay() {
    let Some(mut ink) = open() else { return };
    let viewport = (0., 0., 32., 24.);
    let identity = Transform::identity();
    let pixel = |image: &Pixmap, x: usize, y: usize, expected: u8| {
        let index = (y * image.width() as usize + x) * 4;
        let rgba = &image.data()[index..index + 4];
        assert!(
            rgba.iter().all(|v| v.abs_diff(expected) <= 1),
            "pixel ({x},{y}): {rgba:?}, expected [{expected}; 4]"
        );
    };
    let mut draw = |retained: bool| {
        ink.page([0; 4]);
        ink.begin(viewport.2, viewport.3, 1.);
        if retained {
            ink.row_begin(1, (0., 0.), None);
        }
        let clip = Shape::new((2.25, 1.25, 34., 28.), [4.; 4]);
        ink.push_clip(&clip, identity);
        ink.push_opacity(0.5);
        ink.fill(&Shape::rect((0., 6., 14.25, 7.)), [255; 4], identity);
        if !retained {
            ink.flush().unwrap();
        }
        ink.push_opacity(0.5);
        ink.fill(&Shape::rect((27., 18., 8., 8.)), [255; 4], identity);
        ink.pop_opacity();
        ink.pop_opacity();
        ink.pop_clip();

        if retained {
            ink.row_end(viewport);
            ink.begin(viewport.2, viewport.3, 1.);
            ink.row_draw(1, (0., 0.), viewport);
        }
        let image = ink.finish().unwrap();
        if retained {
            ink.row_free(1);
        }
        image
    };
    let fresh = draw(false);
    let replay = draw(true);
    for image in [&fresh, &replay] {
        pixel(image, 1, 8, 0);
        pixel(image, 2, 8, 96);
        pixel(image, 6, 8, 128);
        pixel(image, 14, 8, 32);
        pixel(image, 15, 8, 0);
        pixel(image, 24, 16, 0);
        pixel(image, 31, 23, 64);
    }
    assert!(
        fresh
            .data()
            .iter()
            .zip(replay.data())
            .all(|(a, b)| a.abs_diff(*b) <= 1),
        "retained layer differs from the fresh draw"
    );
}

#[test]
fn canvas_upload_changes_only_its_rectangle_and_preserves_retained_revisions() {
    let Some(mut ink) = open() else { return };
    let mut image = Pixmap::new(17, 9).unwrap();
    let mut images = images::Images::default();
    let mut update = CanvasUpdate {
        previous: 0,
        revision: 1,
        area: None,
    };
    let device = &mut ink.device;
    let texture = images.canvas(device, &image, update, false).unwrap();
    images.end(device).unwrap();
    images.begin(device).unwrap();

    // Mark a pixel outside the update so readback can detect a full upload.
    device
        .upload_texture(texture, [0, 0], [1, 1], &[0, 80, 0, 255])
        .unwrap();
    let rect = tiny_skia::IntRect::from_xywh(4, 2, 3, 5).unwrap();
    for y in rect.top()..rect.bottom() {
        let start = y as usize * 17 + 4;
        image.pixels_mut()[start..start + 3]
            .fill(tiny_skia::PremultipliedColorU8::from_rgba(100, 0, 0, 255).unwrap());
    }
    update = CanvasUpdate {
        previous: 1,
        revision: 2,
        area: Some(rect),
    };
    assert_eq!(
        images.canvas(device, &image, update, false).unwrap(),
        texture
    );
    let mut expected = image.data().to_vec();
    expected[..4].copy_from_slice(&[0, 80, 0, 255]);
    assert_eq!(device.readback(texture).unwrap(), expected);

    images.pin(texture);
    images.end(device).unwrap();
    images.begin(device).unwrap();
    image.fill(tiny_skia::Color::BLACK);
    update = CanvasUpdate {
        previous: 2,
        revision: 3,
        area: tiny_skia::IntRect::from_xywh(0, 0, 17, 9),
    };
    let next = images.canvas(device, &image, update, false).unwrap();
    assert_ne!(next, texture);
    assert_eq!(device.readback(texture).unwrap(), expected);
    assert_eq!(device.readback(next).unwrap(), image.data());

    // A skipped revision has no matching texture and requires a full upload.
    images.end(device).unwrap();
    images.begin(device).unwrap();
    update = CanvasUpdate {
        previous: 4,
        revision: 5,
        area: Some(rect),
    };
    let skipped = images.canvas(device, &image, update, false).unwrap();
    assert_eq!(device.readback(skipped).unwrap(), image.data());
    images.unpin(texture);
    images.trim(device).unwrap();
}
