use super::*;
use exact_canvas::{Context2d, Env as CanvasEnv, Images};

struct Tile;
impl Images for Tile {
    fn size(&self, _: u64, src: &str) -> Option<(u32, u32)> {
        (src == "tile").then_some((4, 4))
    }
}

fn replay(w: u32, h: u32, scale: f64, draw: impl Fn(&Context2d)) -> Pixmap {
    let ctx = Context2d::new();
    ctx.set_env(CanvasEnv {
        images: Arc::new(std::sync::Mutex::new(Some(Arc::new(Tile)))),
        ..Default::default()
    });
    draw(&ctx);
    let mut tile = Pixmap::new(4, 4).unwrap();
    tile.fill(Color::from_rgba8(0, 0, 255, 255));
    let mut images = ImageCache::new();
    images.insert("tile".into(), CachedImage::new(tile));
    let env = Env {
        images: &images,
        text: None,
    };
    let mut r = Replayer::new(w, h, scale, 1, 0).unwrap();
    for l in ctx.take_lists() {
        r.apply(&l, &env).unwrap();
    }
    Arc::unwrap_or_clone(r.pixmap)
}

fn at(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = p.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

#[test]
fn multiply_overprint_draws_with_canvas_2d() {
    let p = replay(200, 120, 1.0, |c| {
        c.set_fill_style_str("#f6f2e9");
        c.fill_rect(0., 0., 200., 120.);
        c.set_fill_style_str("#34f1ec");
        c.fill_rect(20., 40., 160., 16.);
        c.set_global_composite_operation("multiply").unwrap();
        c.set_fill_style_str("#fd2f7c");
        c.fill_rect(52., 0., 16., 120.);
    });
    // tiny-skia's integer compositing can round a channel one byte from Chrome.
    for (y, expected) in [(48, [52, 44, 115, 255]), (20, [244, 45, 113, 255])] {
        for (actual, expected) in at(&p, 60, y).into_iter().zip(expected) {
            assert!(actual.abs_diff(expected) <= 1, "{actual} versus {expected}");
        }
    }
}

#[test]
fn zero_dash_lists_stroke_solid_paths_and_rectangles() {
    let ink = |p: &Pixmap| p.pixels().iter().filter(|p| p.alpha() != 0).count();
    for rectangle in [false, true] {
        let draw = |dash: &[f64], offset| {
            replay(200, 100, 1.0, |c| {
                c.set_line_width(4.0);
                c.set_line_dash(dash).unwrap();
                c.set_line_dash_offset(offset);
                if rectangle {
                    c.stroke_rect(10.0, 10.0, 180.0, 80.0);
                } else {
                    c.begin_path();
                    c.move_to(10.0, 50.0);
                    c.line_to(190.0, 50.0);
                    c.stroke();
                }
            })
        };
        let solid = draw(&[], 0.0);
        assert_eq!(ink(&solid), if rectangle { 2080 } else { 720 });
        for dash in [&[0.0][..], &[0.0, 0.0], &[0.0, -0.0, 0.0]] {
            for offset in [0.0, 3.0, -3.0] {
                let actual = draw(dash, offset);
                assert!(
                    actual.data() == solid.data(),
                    "rectangle={rectangle}, dash={dash:?}, offset={offset}: {} ink pixels, expected {}",
                    ink(&actual),
                    ink(&solid)
                );
            }
        }
        let dashed = draw(&[4.0, 4.0], 0.0);
        assert!(ink(&dashed) > 0 && ink(&dashed) < ink(&solid));
    }
}

#[test]
fn a_clockwise_arc_bulges_down_in_canvas_space() {
    // arc(50, 50, 40, 0, π): clockwise from +x through +y (down).
    let p = replay(100, 100, 1.0, |c| {
        c.set_fill_style_str("red");
        c.begin_path();
        c.arc(50.0, 50.0, 40.0, 0.0, std::f64::consts::PI).unwrap();
        c.fill();
    });
    assert_eq!(at(&p, 50, 80), [255, 0, 0, 255], "the lower half is filled");
    assert_eq!(at(&p, 50, 20)[3], 0, "the upper half is not");
}

#[test]
fn device_scale_and_author_matrix_compose() {
    let p = replay(40, 40, 2.0, |c| {
        c.translate(10.0, 0.0).unwrap();
        c.fill_rect(0.0, 0.0, 5.0, 5.0);
    });
    assert_eq!(at(&p, 21, 1)[3], 255);
    assert_eq!(at(&p, 19, 1)[3], 0);
    assert_eq!(at(&p, 29, 9)[3], 255);
    assert_eq!(at(&p, 31, 9)[3], 0);
}

#[test]
fn clear_rect_obeys_the_clip_and_the_path_survives_fill() {
    let p = replay(20, 20, 1.0, |c| {
        c.fill_rect(0.0, 0.0, 20.0, 20.0);
        c.begin_path();
        c.rect(0.0, 0.0, 10.0, 20.0);
        c.clip();
        c.clear_rect(0.0, 0.0, 20.0, 20.0);
    });
    assert_eq!(at(&p, 5, 5)[3], 0);
    assert_eq!(at(&p, 15, 5)[3], 255);
}

#[test]
fn a_clip_extent_operator_clears_outside_the_shape_within_the_clip() {
    let p = replay(20, 20, 1.0, |c| {
        c.fill_rect(0.0, 0.0, 20.0, 20.0);
        c.begin_path();
        c.rect(0.0, 0.0, 10.0, 20.0);
        c.clip();
        c.set_global_composite_operation("copy").unwrap();
        c.set_fill_style_str("red");
        c.fill_rect(0.0, 0.0, 5.0, 5.0);
    });
    assert_eq!(at(&p, 2, 2), [255, 0, 0, 255], "the shape");
    assert_eq!(at(&p, 7, 12)[3], 0, "outside the shape, inside the clip");
    assert_eq!(at(&p, 15, 12)[3], 255, "outside the clip");
}

#[test]
fn a_shadow_is_offset_in_canvas_pixels_whatever_the_matrix() {
    let p = replay(40, 40, 1.0, |c| {
        c.set_shadow_color("black");
        c.set_shadow_offset_x(10.0);
        c.scale(3.0, 3.0).unwrap();
        c.fill_rect(0.0, 0.0, 3.0, 3.0);
    });
    assert_eq!(at(&p, 4, 4)[3], 255, "the shape");
    assert_eq!(at(&p, 16, 4)[3], 255, "its shadow, 10 px right");
    assert_eq!(at(&p, 30, 4)[3], 0, "not 30 px right");
}

#[test]
fn images_patterns_and_pixels_replay() {
    let p = replay(30, 30, 1.0, |c| {
        c.draw_image_with_image_handle_and_dw_and_dh("tile", 0.0, 0.0, 8.0, 8.0)
            .unwrap();
        let pat = c
            .create_pattern_with_image_handle("tile", "repeat-x")
            .unwrap()
            .unwrap();
        c.set_fill_style_canvas_pattern(&pat);
        c.fill_rect(0.0, 0.0, 26.0, 20.0);
        let mut d = c.create_image_data_with_sw_and_sh(2.0, 2.0).unwrap();
        d.data.copy_from_slice(&[255, 0, 0, 255].repeat(4));
        c.put_image_data(&d, 28.0, 0.0).unwrap();
    });
    assert_eq!(at(&p, 6, 6), [0, 0, 255, 255], "drawImage scaled");
    assert_eq!(at(&p, 25, 2), [0, 0, 255, 255], "repeat-x across");
    assert_eq!(at(&p, 25, 10)[3], 0, "but not down");
    assert_eq!(at(&p, 29, 1), [255, 0, 0, 255], "putImageData");
}

#[test]
fn a_conic_gradient_starts_at_its_angle_and_turns_clockwise() {
    let p = replay(40, 40, 1.0, |c| {
        let g = c.create_conic_gradient(0.0, 20.0, 20.0).unwrap();
        g.add_color_stop_f64(0.0, "red").unwrap();
        g.add_color_stop_f64(0.5, "blue").unwrap();
        g.add_color_stop_f64(1.0, "red").unwrap();
        c.set_fill_style_canvas_gradient(&g);
        c.fill_rect(0.0, 0.0, 40.0, 40.0);
    });
    assert!(at(&p, 38, 20)[0] > 200, "red at angle 0");
    assert!(at(&p, 2, 20)[2] > 200, "blue half way round");
}

#[test]
fn capture_refuses_an_unheld_generation_without_overwriting_a_file() {
    use exact_runner::canvas_capture::{captured_image, CanvasCapture};
    let root = std::env::temp_dir().join(format!("exact-canvas-missing-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("shot.png");
    let mut canvases = Canvases::default();
    let mut c = CanvasCapture {
        view: 7,
        lifetime: 1,
        generation: 0,
        sequence: 1,
        job: None,
        path: "app:/tmp/shot.png".into(),
        width: 4,
        height: 4,
        scale: 1.,
        encoding: Default::default(),
    };
    let roots = [root.clone(), root.clone(), root.clone()];
    for held in [false, true] {
        if held {
            canvases
                .replayers
                .insert(7, Replayer::new(4, 4, 1., 1, 0).unwrap());
        }
        canvases.completed.insert(7, 1);
        c.generation = 1;
        std::fs::write(&path, b"keep this file").unwrap();
        let outcome = super::capture::capture(
            &canvases,
            Ok(c.clone()),
            Some(roots.clone()),
            "fs.write app:/tmp",
            None,
        );
        assert!(
            captured_image(&outcome).is_err(),
            "a missing generation must refuse"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"keep this file");
    }
    c.generation = 0;
    // Allocation alone is not a completed draw.
    canvases.completed.clear();
    assert!(captured_image(&super::capture::capture(
        &canvases,
        Ok(c.clone()),
        Some(roots.clone()),
        "fs.write app:/tmp",
        None
    ))
    .is_err());
    canvases.completed.insert(7, 1);
    let outcome = super::capture::capture(
        &canvases,
        Ok(c.clone()),
        Some(roots.clone()),
        "fs.write app:/tmp",
        None,
    );
    assert_eq!(
        captured_image(&outcome).map(|i| i.media_type),
        Ok("image/png".into()),
        "a completed empty bitmap is capturable"
    );
    assert!(Pixmap::load_png(&path)
        .unwrap()
        .pixels()
        .iter()
        .all(|p| p.alpha() == 0));
    // A JPEG, as `toBlob` writes one; a type this host has no encoder for is a PNG.
    for (file, asked, written, magic) in [
        ("shot.jpg", "image/jpeg", "image/jpeg", &b"\xff\xd8\xff"[..]),
        ("shot.avif", "image/avif", "image/png", &b"\x89PNG"[..]),
    ] {
        c.path = format!("app:/tmp/{file}");
        c.encoding = exact_runner::canvas_capture::Encoding::new(asked, Some(0.8));
        let outcome = super::capture::capture(
            &canvases,
            Ok(c.clone()),
            Some(roots.clone()),
            "fs.write app:/tmp",
            None,
        );
        assert_eq!(
            captured_image(&outcome).map(|i| i.media_type),
            Ok(written.into())
        );
        assert!(std::fs::read(root.join(file)).unwrap().starts_with(magic));
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn captures_encode_straight_alpha_in_png_and_flatten_onto_black_for_jpeg() {
    let mut p = Pixmap::new(2, 1).unwrap();
    // Transparent, and red at half alpha (premultiplied).
    p.pixels_mut()[1] = tiny_skia::PremultipliedColorU8::from_rgba(128, 0, 0, 128).unwrap();
    let (encoded, ty) =
        super::capture::encode(&p, &exact_runner::canvas_capture::Encoding::default()).unwrap();
    assert_eq!(ty, "image/png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(encoded))
        .read_info()
        .unwrap();
    assert!(reader.info().srgb.is_some());
    let mut straight = [0; 8];
    reader.next_frame(&mut straight).unwrap();
    assert_eq!(straight, [0, 0, 0, 0, 255, 0, 0, 128]);
    let flat = super::capture::opaque(&p);
    let rgba: Vec<_> = (flat.pixels().iter())
        .map(|c| [c.red(), c.green(), c.blue(), c.alpha()])
        .collect();
    assert_eq!(rgba, [[0, 0, 0, 255], [128, 0, 0, 255]]);
}

#[test]
fn a_redraw_copies_the_bitmap_only_while_a_painter_holds_the_last_one() {
    let draw = |fresh: bool, x: f64| {
        let ctx = Context2d::new();
        ctx.set_fill_style_str("#f00");
        ctx.fill_rect(x, 0., 1., 1.);
        CanvasList {
            view: 7,
            lifetime: 1,
            generation: 0,
            seq: 1,
            fresh,
            pixel_width: 4,
            pixel_height: 4,
            scale: 1.,
            stretch: false,
            lists: ctx.take_lists(),
            animating: false,
            settings: Default::default(),
        }
    };
    let shown = |c: &Canvases| c.snapshots.borrow()[&7].pixels.clone();
    let mut canvases = Canvases::default();
    assert!(canvases.apply(vec![draw(true, 0.)]).is_empty());
    let first = Arc::as_ptr(&shown(&canvases));
    // No painter holds the snapshot: the next list draws into the same bitmap.
    assert!(canvases.apply(vec![draw(false, 1.)]).is_empty());
    let held = shown(&canvases);
    assert_eq!(Arc::as_ptr(&held), first);
    // A painter holds it: the next list draws into a copy, and the held one stays as it was.
    assert!(canvases.apply(vec![draw(false, 2.)]).is_empty());
    let now = shown(&canvases);
    assert_ne!(Arc::as_ptr(&now), Arc::as_ptr(&held));
    assert_eq!(at(&held, 2, 0)[3], 0);
    assert_eq!(at(&now, 2, 0), [255, 0, 0, 255]);
    assert_eq!(at(&now, 1, 0), [255, 0, 0, 255]);
}

#[test]
fn whole_pixel_clear_and_blit_preserve_edge_cases() {
    // Positive clearRect clears whole pixels; negative width/height clear the
    // same 3x3 region without panicking.
    for (x, y, w, h) in [(3., 2., 3., 3.), (6., 2., -3., 3.), (3., 5., 3., -3.)] {
        let p = replay(10, 10, 1.0, |c| {
            c.set_fill_style_str("#ff0000");
            c.fill_rect(0., 0., 10., 10.);
            c.clear_rect(x, y, w, h);
        });
        for py in 0..10 {
            for px in 0..10 {
                let inside = (3..6).contains(&px) && (2..5).contains(&py);
                let want = if inside {
                    [0, 0, 0, 0]
                } else {
                    [255, 0, 0, 255]
                };
                assert_eq!(at(&p, px, py), want, "({x},{y},{w},{h}) at ({px},{py})");
            }
        }
    }
    // Whole-pixel opaque blit copies exact pixels; fractional source crop with
    // integer translation falls back so half-covered edges stay anti-aliased.
    let whole = replay(10, 10, 1.0, |c| {
        c.set_image_smoothing_enabled(false);
        c.draw_image_with_image_handle_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            "tile", 1., 1., 2., 2., 3., 4., 2., 2.,
        )
        .unwrap();
    });
    for py in 0..10 {
        for px in 0..10 {
            let inside = (3..5).contains(&px) && (4..6).contains(&py);
            assert_eq!(
                at(&whole, px, py),
                if inside {
                    [0, 0, 255, 255]
                } else {
                    [0, 0, 0, 0]
                }
            );
        }
    }
    let frac = replay(10, 10, 1.0, |c| {
        c.set_image_smoothing_enabled(false);
        c.draw_image_with_image_handle_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            "tile", 0.5, 0., 2., 2., 2.5, 2., 2., 2.,
        )
        .unwrap();
    });
    // Left half-covered column at x=2 and right half-covered column at x=4 must be
    // partial alpha rather than solid 255.
    assert!(at(&frac, 2, 2)[3] > 0 && at(&frac, 2, 2)[3] < 255);
    assert_eq!(at(&frac, 3, 2), [0, 0, 255, 255]);
    assert!(at(&frac, 4, 2)[3] > 0 && at(&frac, 4, 2)[3] < 255);
}

#[test]
fn full_canvas_clears_accept_frame_dimensions_rounded_to_f32() {
    for scale in [1.25_f32, 1.5, 1.75] {
        let ts = tiny_skia::Transform::from_scale(scale, scale);
        for pixels in 1..=7680 {
            let points = f64::from(pixels as f32 / scale);
            assert!(
                is_full_clear_rect(ts, 0., 0., points, points, pixels, pixels),
                "{pixels} px at {scale}"
            );
        }
    }
    let ts = tiny_skia::Transform::from_scale(1.25, 1.25);
    let fractional = f64::from(4640.8_f32.next_down());
    assert!(!is_full_clear_rect(ts, 0., 0., fractional, 8., 5801, 10));
}

#[test]
fn whole_pixel_clear_fill_and_blit_match_reference_and_preserve_shared_snapshots() {
    struct Sheet;
    impl Images for Sheet {
        fn size(&self, _: u64, src: &str) -> Option<(u32, u32)> {
            match src {
                "opaque" | "translucent" => Some((16, 12)),
                _ => None,
            }
        }
    }

    let mut opaque = Pixmap::new(16, 12).unwrap();
    for y in 0..12u32 {
        for x in 0..16u32 {
            opaque.pixels_mut()[(y * 16 + x) as usize] =
                tiny_skia::PremultipliedColorU8::from_rgba(
                    (x * 15) as u8,
                    (y * 20) as u8,
                    140,
                    255,
                )
                .unwrap();
        }
    }
    let mut translucent = Pixmap::new(16, 12).unwrap();
    for y in 0..12u32 {
        for x in 0..16u32 {
            let a = match (x + y) % 3 {
                0 => 0,
                1 => 128,
                _ => 255,
            };
            let ch = |v: u8| ((u16::from(v) * u16::from(a) + 127) / 255) as u8;
            translucent.pixels_mut()[(y * 16 + x) as usize] =
                tiny_skia::PremultipliedColorU8::from_rgba(
                    ch((x * 15) as u8),
                    ch((y * 20) as u8),
                    ch(200),
                    a,
                )
                .unwrap();
        }
    }
    let mut images = ImageCache::new();
    images.insert("opaque".into(), CachedImage::new(opaque));
    images.insert("translucent".into(), CachedImage::new(translucent));
    assert!(images["opaque"].opaque);
    assert!(!images["translucent"].opaque);

    // Reference replay forces the general tiny-skia path by setting a full-canvas clip mask.
    let run = |w: u32, h: u32, scale: f64, draw: &dyn Fn(&Context2d), force_reference: bool| {
        let ctx = Context2d::new();
        ctx.set_env(CanvasEnv {
            images: Arc::new(std::sync::Mutex::new(Some(Arc::new(Sheet)))),
            ..Default::default()
        });
        draw(&ctx);
        let env = Env {
            images: &images,
            text: None,
        };
        let mut r = Replayer::new(w, h, scale, 1, 0).unwrap();
        if force_reference {
            let mut m = Mask::new(w, h).unwrap();
            m.data_mut().fill(255);
            r.state.clip = Some(m);
        }
        for l in ctx.take_lists() {
            r.apply(&l, &env).unwrap();
        }
        Arc::unwrap_or_clone(r.pixmap)
    };

    let cases: &[(u32, u32, f64, fn(&Context2d))] = &[
        // 1:1 full and subrect opaque blit + partial clear + fillRect.
        (24, 20, 1.0, |c| {
            c.set_fill_style_str("#224466");
            c.fill_rect(0., 0., 24., 20.);
            c.clear_rect(4., 3., 10., 8.);
            c.draw_image_with_image_handle_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                "opaque", 2., 3., 10., 6., -2., 5., 10., 6.,
            )
            .unwrap();
            c.draw_image_with_image_handle("translucent", 6., 4.)
                .unwrap();
        }),
        // Translucent image onto cleared canvas, then full clear with over-extent & negative size.
        (20, 16, 1.0, |c| {
            c.clear_rect(-5.5, -3.2, 40.7, 30.9);
            c.draw_image_with_image_handle("translucent", 2., 1.)
                .unwrap();
            c.clear_rect(12., 10., -4., -3.);
        }),
        // 125% DPI scale with 1:1 physical device pixel image draw (dw = sw / 1.25).
        (20, 15, 1.25, |c| {
            c.clear_rect(0., 0., 16., 12.);
            c.draw_image_with_image_handle_and_dw_and_dh("opaque", 0., 0., 16. / 1.25, 12. / 1.25)
                .unwrap();
        }),
        // Fractional edges and alpha fall back cleanly without visual drift.
        (20, 20, 1.0, |c| {
            c.set_fill_style_str("#cc3311");
            c.fill_rect(0., 0., 20., 20.);
            c.clear_rect(2.5, 2.5, 6., 6.);
            c.set_global_alpha(0.5);
            c.draw_image_with_image_handle("opaque", 1., 1.).unwrap();
        }),
    ];

    for (i, &(w, h, scale, draw)) in cases.iter().enumerate() {
        let actual = run(w, h, scale, &draw, false);
        let expected = run(w, h, scale, &draw, true);
        for (p, (a, e)) in actual.data().iter().zip(expected.data().iter()).enumerate() {
            assert!(
                a.abs_diff(*e) <= 1,
                "case {i} byte {p}: actual {a} vs expected {e}"
            );
        }
    }

    // Overwriting operations never mutate a snapshot held by a painter.
    let env = Env {
        images: &images,
        text: None,
    };
    let mut r = Replayer::new(16, 12, 1.0, 1, 0).unwrap();
    let run_ctx = |r: &mut Replayer, f: fn(&Context2d)| {
        let ctx = Context2d::new();
        ctx.set_env(CanvasEnv {
            images: Arc::new(std::sync::Mutex::new(Some(Arc::new(Sheet)))),
            ..Default::default()
        });
        f(&ctx);
        r.apply_draw(&ctx.take_lists(), &env).unwrap();
    };
    run_ctx(&mut r, |c| {
        c.set_fill_style_str("#112233");
        c.fill_rect(0., 0., 16., 12.);
    });
    let held_fill = r.pixmap.clone();
    run_ctx(&mut r, |c| {
        c.clear_rect(0., 0., 16., 12.);
    });
    assert_eq!(at(&held_fill, 4, 4), [0x11, 0x22, 0x33, 255]);
    assert_eq!(at(&r.pixmap, 4, 4), [0, 0, 0, 0]);
    let held_clear = r.pixmap.clone();
    run_ctx(&mut r, |c| {
        c.draw_image_with_image_handle("opaque", 0., 0.).unwrap();
    });
    assert_eq!(at(&held_clear, 4, 4), [0, 0, 0, 0]);
    assert_eq!(at(&r.pixmap, 4, 4), [60, 80, 140, 255]);
    let held_blit = r.pixmap.clone();
    run_ctx(&mut r, |c| {
        let mut data = c.create_image_data_with_sw_and_sh(16., 12.).unwrap();
        for p in data.data.as_chunks_mut::<4>().0 {
            *p = [200, 100, 50, 128];
        }
        c.put_image_data(&data, 0., 0.).unwrap();
    });
    assert_eq!(at(&held_blit, 4, 4), [60, 80, 140, 255]);
    assert_eq!(at(&r.pixmap, 4, 4), [100, 50, 25, 128]);
}

#[test]
fn prefix_checkpoint_reuses_finished_strokes_and_invalidates_soundly() {
    struct Bg;
    impl Images for Bg {
        fn size(&self, _: u64, src: &str) -> Option<(u32, u32)> {
            (src == "bg").then_some((64, 48))
        }
    }

    let make_bg = |r: u8, g: u8, b: u8| {
        let mut p = Pixmap::new(64, 48).unwrap();
        p.fill(Color::from_rgba8(r, g, b, 255));
        CachedImage::new(p)
    };
    let mut images = ImageCache::new();
    images.insert("bg".into(), make_bg(20, 40, 80));

    let ctx = Context2d::new();
    ctx.set_env(CanvasEnv {
        images: Arc::new(std::sync::Mutex::new(Some(Arc::new(Bg)))),
        ..Default::default()
    });

    let record_frame = |ctx: &Context2d, bg_x: f64, strokes: &[Vec<(f64, f64)>]| {
        ctx.clear_rect(0., 0., 64., 48.);
        ctx.draw_image_with_image_handle("bg", bg_x, 0.).unwrap();
        ctx.set_stroke_style_str("#ff3300");
        ctx.set_line_width(3.);
        for pts in strokes {
            ctx.begin_path();
            for (i, &(x, y)) in pts.iter().enumerate() {
                if i == 0 {
                    ctx.move_to(x, y);
                } else {
                    ctx.line_to(x, y);
                }
            }
            ctx.stroke();
        }
        ctx.take_lists()
    };

    let fresh_reference =
        |lists: &[Vec<u8>], images: &ImageCache, bg_x: f64, strokes: &[Vec<(f64, f64)>]| {
            let _ = lists;
            let fresh_ctx = Context2d::new();
            fresh_ctx.set_env(CanvasEnv {
                images: Arc::new(std::sync::Mutex::new(Some(Arc::new(Bg)))),
                ..Default::default()
            });
            let fresh_lists = record_frame(&fresh_ctx, bg_x, strokes);
            let env = Env { images, text: None };
            let mut r = Replayer::new(64, 48, 1.0, 1, 0).unwrap();
            for l in &fresh_lists {
                r.apply(l, &env).unwrap();
            }
            r.pixmap
        };

    let mut r = Replayer::new(64, 48, 1.0, 1, 0).unwrap();
    let s1 = vec![(4., 4.), (20., 12.), (36., 8.)];
    let s2 = vec![(6., 20.), (30., 28.), (54., 22.)];

    // Draw 1 registers Op::Image("bg"); Draw 2 establishes the clean prefix;
    // Draw 3..5 grow stroke 3 and reuse the checkpoint after stroke 2.
    let mut active = vec![(10., 34.)];
    for step in 0..5 {
        if step > 0 {
            active.push((10. + step as f64 * 10., 34. + (step % 2) as f64 * 6.));
        }
        let strokes = vec![s1.clone(), s2.clone(), active.clone()];
        let lists = record_frame(&ctx, 0., &strokes);
        let env = Env {
            images: &images,
            text: None,
        };
        r.apply_draw(&lists, &env).unwrap();
        let expected = fresh_reference(&lists, &images, 0., &strokes);
        assert_eq!(r.pixmap.data(), expected.data(), "step {step}");
        if step >= 2 {
            assert!(
                r.checkpoint.is_some(),
                "step {step} should hold a prefix checkpoint"
            );
        }
    }
    let cp_after_s2 = r.checkpoint.as_ref().unwrap().recs;

    // Starting a 4th stroke advances the checkpoint past stroke 3.
    let s3 = active;
    let strokes = vec![
        s1.clone(),
        s2.clone(),
        s3.clone(),
        vec![(2., 40.), (18., 44.)],
    ];
    let lists = record_frame(&ctx, 0., &strokes);
    r.apply_draw(
        &lists,
        &Env {
            images: &images,
            text: None,
        },
    )
    .unwrap();
    assert!(r.checkpoint.as_ref().unwrap().recs > cp_after_s2);
    assert_eq!(
        r.pixmap.data(),
        fresh_reference(&lists, &images, 0., &strokes).data()
    );

    // An identical redraw reuses the checkpointed pixmap without copying it.
    let lists_same = record_frame(&ctx, 0., &strokes);
    r.apply_draw(
        &lists_same,
        &Env {
            images: &images,
            text: None,
        },
    )
    .unwrap();
    let ptr_before = Arc::as_ptr(&r.pixmap);
    let lists_same_again = record_frame(&ctx, 0., &strokes);
    r.apply_draw(
        &lists_same_again,
        &Env {
            images: &images,
            text: None,
        },
    )
    .unwrap();
    assert_eq!(Arc::as_ptr(&r.pixmap), ptr_before);

    // Replacing the background image invalidates the checkpoint and repaints it.
    let previous = Arc::downgrade(&images["bg"].pixels);
    images.remove("bg");
    assert!(previous.upgrade().is_none());
    images.insert("bg".into(), make_bg(90, 120, 30));
    let lists_reloaded = record_frame(&ctx, 0., &strokes);
    r.apply_draw(
        &lists_reloaded,
        &Env {
            images: &images,
            text: None,
        },
    )
    .unwrap();
    assert_eq!(
        r.pixmap.data(),
        fresh_reference(&lists_reloaded, &images, 0., &strokes).data()
    );
    assert_eq!(at(&r.pixmap, 60, 44), [90, 120, 30, 255]);

    // Undo (shorter prefix) rebuilds soundly.
    let undone = vec![s1.clone()];
    let lists_undo = record_frame(&ctx, 0., &undone);
    r.apply_draw(
        &lists_undo,
        &Env {
            images: &images,
            text: None,
        },
    )
    .unwrap();
    assert_eq!(
        r.pixmap.data(),
        fresh_reference(&lists_undo, &images, 0., &undone).data()
    );

    // Moving the background (`bg_x` changes every frame) has no shared paint
    // ops after clearRect and keeps no checkpoint memory.
    for dx in [1., 2., 3.] {
        let lists_move = record_frame(&ctx, dx, &[]);
        r.apply_draw(
            &lists_move,
            &Env {
                images: &images,
                text: None,
            },
        )
        .unwrap();
        assert!(
            r.checkpoint.is_none(),
            "moving drawImage must not retain a checkpoint"
        );
        assert_eq!(
            r.pixmap.data(),
            fresh_reference(&lists_move, &images, dx, &[]).data()
        );
    }
}

#[test]
fn moving_a_stroke_restores_only_its_old_bounds_and_keeps_held_frames_intact() {
    let images = ImageCache::new();
    let env = Env {
        images: &images,
        text: None,
    };
    let mut canvas = Replayer::new(5801, 1240, 1.25, 1, 0).unwrap();
    let width = f64::from(5801_f32 / 1.25);
    let frame = |x: f64| {
        let ctx = Context2d::new();
        ctx.clear_rect(0., 0., width, 992.);
        ctx.set_fill_style_str("#345");
        ctx.fill_rect(0., 0., width, 992.);
        ctx.begin_path();
        ctx.set_stroke_style_str("#f00");
        ctx.set_line_width(3.);
        ctx.move_to(x, 40.);
        ctx.line_to(x + 8., 44.);
        ctx.stroke();
        ctx.take_lists()
    };
    canvas.apply_draw(&frame(40.), &env).unwrap();
    canvas.apply_draw(&frame(50.), &env).unwrap();
    assert!(canvas.checkpoint.is_some());
    let storage = canvas.pixmap.data().as_ptr();
    let next = frame(75.);
    canvas.apply_draw(&next, &env).unwrap();
    assert_eq!(canvas.pixmap.data().as_ptr(), storage);
    let area = canvas.update.area.unwrap();
    assert!(area.width() < 128 && area.height() < 64, "{area:?}");
    let mut direct = Replayer::new(5801, 1240, 1.25, 1, 0).unwrap();
    direct.apply(&next[0], &env).unwrap();
    assert!(
        canvas.pixmap.data() == direct.pixmap.data(),
        "replayed and full frames differ"
    );
    let held = canvas.pixmap.clone();
    let next = frame(65.);
    canvas.apply_draw(&next, &env).unwrap();
    assert!(held.data() == direct.pixmap.data(), "held snapshot changed");
    direct.apply(&next[0], &env).unwrap();
    assert!(
        canvas.pixmap.data() == direct.pixmap.data(),
        "replayed and full frames differ"
    );
}
