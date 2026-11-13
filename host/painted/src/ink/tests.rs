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
    for (a, b) in a.data().chunks_exact(4).zip(b.data().chunks_exact(4)) {
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
