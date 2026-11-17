#![cfg(target_vendor = "apple")]
//! A canvas that clips every frame without `save` keeps one region (HTML's
//! `clip()` intersects it), so each frame's scene holds a bounded number of
//! clip layers. Encoding tests need no GPU; pixel tests compare the cached
//! masks with uncompacted clips on Metal and with the browser.
use exact_canvas::{CanvasWindingRule, Context2d};
use exact_canvas_vello::gpu::{self, Canvas, Failed};
use exact_canvas_vello::replay::{Frame, Host, Replayer};
use exact_canvas_vello::surface::{make, IOSurfaceRef};
use vello::kurbo::BezPath;
use vello::peniko::ImageData;
use vello::Scene;

struct NoHost;

impl Host for NoHost {
    fn text(&mut self, _: &[f64], _: &[u32], _: bool) -> Option<BezPath> {
        None
    }
    fn image(&mut self, _: &str) -> Option<ImageData> {
        None
    }
}

/// Replays `frames` draws as the GPU module does (`gpu.rs` `replay`): each
/// frame a new scene over the last frame's pixels. Returns each frame's clip
/// count.
fn clips_per_frame(frames: usize, draw: impl Fn(&Context2d, usize)) -> Vec<u32> {
    let ctx = Context2d::new();
    let mut replayer = Replayer::new(200, 100, 1.0);
    (0..frames)
        .map(|i| {
            draw(&ctx, i);
            let mut frame = Frame {
                scene: Scene::new(),
                keep: true,
                shadows: Vec::new(),
                masks: Vec::new(),
            };
            replayer.begin(&mut frame.scene);
            for list in ctx.take_lists() {
                replayer.apply(&list, &mut frame, &mut NoHost).unwrap();
            }
            replayer.close(&mut frame);
            assert!(replayer.state.clips <= 8);
            for mask in &frame.masks {
                assert!(mask.scene.encoding().n_clips <= 18);
            }
            frame.scene.encoding().n_clips
        })
        .collect()
}

struct Pixels {
    ctx: Context2d,
    canvas: Canvas,
    surface: IOSurfaceRef,
    previous: bool,
}

impl Pixels {
    fn new() -> Self {
        Self::sized(128, 96, 2.0)
    }

    fn sized(width: u32, height: u32, scale: f64) -> Self {
        Self {
            ctx: Context2d::new(),
            canvas: Canvas::new(width, height, scale),
            surface: make::surface(width, height),
            previous: false,
        }
    }

    fn replay(&mut self) {
        let lists = self.ctx.take_lists();
        let lists: Vec<_> = lists.iter().map(Vec::as_slice).collect();
        let previous = if self.previous {
            self.surface
        } else {
            IOSurfaceRef(std::ptr::null_mut())
        };
        // SAFETY: this fixture owns the surface and the lists outlive replay.
        let result = unsafe {
            self.canvas
                .replay(self.surface, previous, &lists, &mut NoHost)
        };
        match result {
            Ok(()) => {}
            Err(Failed::Gpu(e)) => panic!("GPU replay: {e}"),
            Err(Failed::List) => panic!("unreadable list"),
        }
        self.previous = true;
    }

    fn pixels(&self) -> Vec<u8> {
        make::pixels(
            self.surface,
            self.canvas.replayer.width,
            self.canvas.replayer.height,
        )
    }
}

impl Drop for Pixels {
    fn drop(&mut self) {
        make::release(self.surface);
    }
}

fn metal() -> bool {
    match gpu::gpu() {
        Ok(_) => true,
        Err(e) => {
            assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{e}");
            eprintln!("SKIP clip pixels: {e}");
            false
        }
    }
}

/// Run with Metal validation in a fresh process; only that process draws.
fn validated_metal(test: &str, shaders: bool) -> bool {
    if std::env::var_os("EXACT_CLIP_SHADER_VALIDATION").is_some() {
        return metal();
    }
    // Metal reports invalid GPU reads to stderr without failing the
    // command buffer. Enable it before device creation and check its output.
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("EXACT_CLIP_SHADER_VALIDATION", "1")
        .env("MTL_DEBUG_LAYER", "1")
        .env("MTL_SHADER_VALIDATION", if shaders { "1" } else { "0" })
        .env("MTL_SHADER_VALIDATION_REPORT_TO_STDERR", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        !stderr.contains("Invalid Metal usage:") && !stderr.contains("exact canvas gpu:"),
        "{stderr}"
    );
    false
}

fn curved_clip(ctx: &Context2d, i: usize, evenodd: bool) {
    let t = i as f64 / 1000.0;
    ctx.set_transform(1.0, 0.03, 0.07, 1.0, t, -t).unwrap();
    ctx.begin_path();
    ctx.arc(31.0, 23.0, 21.0 + t, 0.0, std::f64::consts::TAU)
        .unwrap();
    ctx.move_to(35.0 + t, 23.0);
    ctx.arc(31.0, 23.0, 4.0 + t, 0.0, std::f64::consts::TAU)
        .unwrap();
    ctx.clip_with_canvas_winding_rule(if evenodd {
        CanvasWindingRule::Evenodd
    } else {
        CanvasWindingRule::Nonzero
    });
    ctx.reset_transform().unwrap();
}

fn compare_pixels(actual: &[u8], expected: &[u8], tolerance: u8, what: &str) {
    compare_pixels_at_width(actual, expected, tolerance, what, 128);
}

fn compare_pixels_at_width(
    actual: &[u8],
    expected: &[u8],
    tolerance: u8,
    what: &str,
    width: usize,
) {
    assert_eq!(actual.len(), expected.len(), "{what}: image sizes");
    let max = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    let at = actual
        .iter()
        .zip(expected)
        .position(|(a, b)| a.abs_diff(*b) == max)
        .unwrap()
        / 4;
    assert!(
        max <= tolerance,
        "{what}: maximum channel error {max} at ({}, {}): {:?} versus {:?}",
        at % width,
        at / width,
        &actual[at * 4..at * 4 + 4],
        &expected[at * 4..at * 4 + 4]
    );
}

#[test]
fn compacted_curves_match_uncompacted_pixels_and_restore_and_reset() {
    if !metal() {
        return;
    }
    let mut compact = Pixels::new();
    let mut reference = Pixels::new();
    for p in [&mut compact, &mut reference] {
        p.ctx.set_fill_style_str("rgba(40,100,180,0.7)");
        p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
        p.replay();
    }
    let scratch_bytes = 128 * 96 * 4;
    for level in 0..2 {
        compact.ctx.save();
        reference.ctx.save();
        for i in 0..160 {
            reference.ctx.save();
            for p in [&mut compact, &mut reference] {
                curved_clip(&p.ctx, level * 160 + i, level == 1);
                p.replay();
            }
            assert!(compact.canvas.bytes() <= scratch_bytes * (level as u64 + 2));
        }
        for p in [&mut compact, &mut reference] {
            p.ctx.set_fill_style_str("white");
            p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
            p.replay();
        }
        compare_pixels(
            &compact.pixels(),
            &reference.pixels(),
            2,
            "curved intersection",
        );
    }
    for p in [&mut compact, &mut reference] {
        let mut image = p.ctx.create_image_data_with_sw_and_sh(128.0, 96.0).unwrap();
        for pixel in image.data.as_chunks_mut::<4>().0 {
            *pixel = [0, 255, 0, 255];
        }
        p.ctx.put_image_data(&image, 0.0, 0.0).unwrap();
    }
    // Uncompacted Vello changes edge coverage when two long clip stacks
    // share a scene. Flush the raw write only in the reference; compaction
    // must produce the same pixels with both operations in one replay.
    reference.replay();
    for p in [&mut compact, &mut reference] {
        p.ctx.set_global_composite_operation("copy").unwrap();
        p.ctx.fill_rect(15.0, 15.0, 8.0, 8.0);
        p.replay();
        assert_eq!(&p.pixels()[(128 + 1) * 4..(128 + 2) * 4], &[0, 255, 0, 255]);
    }
    compare_pixels(
        &compact.pixels(),
        &reference.pixels(),
        3,
        "copy and raw pixels under a mask",
    );
    for level in (0..2).rev() {
        for _ in 0..160 {
            reference.ctx.restore();
        }
        for p in [&mut compact, &mut reference] {
            p.ctx.restore();
            p.ctx.clear_rect(0.0, 0.0, 64.0, 48.0);
            p.ctx.set_fill_style_str("red");
            p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
            p.replay();
        }
        compare_pixels(&compact.pixels(), &reference.pixels(), 3, "restored region");
        assert_eq!(compact.canvas.bytes(), scratch_bytes * (level + 1));
    }
    for i in 0..25 {
        curved_clip(&compact.ctx, i, true);
        reference.ctx.save();
        curved_clip(&reference.ctx, i, true);
    }
    for p in [&mut compact, &mut reference] {
        p.ctx.clear_rect(0.0, 0.0, 64.0, 48.0);
        p.ctx.fill_rect(10.0, 10.0, 10.0, 10.0);
        p.replay();
    }
    compare_pixels(
        &compact.pixels(),
        &reference.pixels(),
        3,
        "many clips in one replay",
    );
    for p in [&mut compact, &mut reference] {
        p.ctx.reset();
        p.ctx.restore();
        p.ctx.set_fill_style_str("lime");
        p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
        p.replay();
        assert!(!p.canvas.replayer.clipped());
        assert_eq!(p.canvas.bytes(), scratch_bytes);
        assert!(p
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == [0, 255, 0, 255]));
    }
}

#[test]
#[ignore = "async lane: Metal clip pixels against Chromium"]
fn compacted_curves_match_the_browser() {
    if !metal() {
        return;
    }
    let mut fixture = Pixels::new();
    let mut snapshots = Vec::new();
    fixture.ctx.save();
    for i in 0..320 {
        if i == 160 {
            fixture.ctx.save();
        }
        curved_clip(&fixture.ctx, i, i >= 160);
        fixture.replay();
    }
    fixture.ctx.set_fill_style_str("white");
    fixture.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
    fixture.replay();
    snapshots.push(fixture.pixels());
    let mut image = fixture
        .ctx
        .create_image_data_with_sw_and_sh(128.0, 96.0)
        .unwrap();
    for pixel in image.data.as_chunks_mut::<4>().0 {
        *pixel = [0, 255, 0, 255];
    }
    fixture.ctx.put_image_data(&image, 0.0, 0.0).unwrap();
    fixture.ctx.set_global_composite_operation("copy").unwrap();
    fixture.ctx.fill_rect(15.0, 15.0, 8.0, 8.0);
    fixture.replay();
    snapshots.push(fixture.pixels());
    for _ in 0..2 {
        fixture.ctx.restore();
        fixture.ctx.clear_rect(0.0, 0.0, 64.0, 48.0);
        fixture.ctx.set_fill_style_str("red");
        fixture.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
        fixture.replay();
        snapshots.push(fixture.pixels());
    }
    fixture.ctx.reset();
    fixture.ctx.set_fill_style_str("lime");
    fixture.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
    fixture.replay();
    snapshots.push(fixture.pixels());
    let script = r#"
        import { chromium } from './scripts/agent-launch.mjs';
        import { chromium as playwright } from 'playwright-core';
        const browser = await playwright.launch({executablePath: chromium().executable, headless: true});
        let pixels;
        try {
            const page = await browser.newPage();
            pixels = await page.evaluate(() => {
                const canvas = document.createElement('canvas');
                canvas.width = 128; canvas.height = 96;
                const ctx = canvas.getContext('2d');
                const shots = [];
                const shot = () => shots.push(...ctx.getImageData(0, 0, 128, 96).data);
                ctx.save();
                for (let i = 0; i < 320; i++) {
                    if (i === 160) ctx.save();
                    const t = i / 1000;
                    ctx.setTransform(2, 0.06, 0.14, 2, 2*t, -2*t);
                    ctx.beginPath();
                    ctx.arc(31, 23, 21+t, 0, 2*Math.PI);
                    ctx.moveTo(35+t, 23);
                    ctx.arc(31, 23, 4+t, 0, 2*Math.PI);
                    ctx.clip(i >= 160 ? 'evenodd' : 'nonzero');
                }
                ctx.setTransform(2, 0, 0, 2, 0, 0);
                ctx.fillStyle = 'white'; ctx.fillRect(0, 0, 64, 48);
                shot();
                const image = ctx.createImageData(128, 96);
                for (let i = 0; i < image.data.length; i += 4) image.data.set([0,255,0,255], i);
                ctx.putImageData(image, 0, 0);
                ctx.globalCompositeOperation = 'copy'; ctx.fillRect(15, 15, 8, 8);
                shot();
                for (let level = 0; level < 2; level++) {
                    ctx.restore();
                    ctx.setTransform(2, 0, 0, 2, 0, 0);
                    ctx.clearRect(0, 0, 64, 48);
                    ctx.fillStyle = 'red'; ctx.fillRect(0, 0, 64, 48);
                    shot();
                }
                ctx.reset(); ctx.setTransform(2, 0, 0, 2, 0, 0);
                ctx.fillStyle = 'lime'; ctx.fillRect(0, 0, 64, 48); shot();
                return shots;
            });
        } finally { await browser.close(); }
        process.stdout.write(Buffer.from(pixels));
    "#;
    let out = std::process::Command::new(std::env::var("BUN").unwrap_or_else(|_| "bun".into()))
        .args(["--eval", script])
        .current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .expect("Bun runs the browser reference");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout.len(), snapshots.len() * 128 * 96 * 4);
    for (phase, (actual, expected)) in snapshots
        .iter()
        .zip(out.stdout.as_chunks::<{ 128 * 96 * 4 }>().0)
        .enumerate()
    {
        let pixel = |x: usize, y: usize| {
            let at = (y * 128 + x) * 4;
            let rgba = &expected[at..at + 4];
            let channel = |c: u8| ((u16::from(c) * u16::from(rgba[3]) + 127) / 255) as u8;
            [
                channel(rgba[2]),
                channel(rgba[1]),
                channel(rgba[0]),
                rgba[3],
            ]
        };
        let mut error = 0u64;
        for y in 1..95 {
            for x in 1..127 {
                let expected = pixel(x, y);
                let at = (y * 128 + x) * 4;
                let got = &actual[at..at + 4];
                error += got
                    .iter()
                    .zip(expected)
                    .map(|(g, e)| u64::from(g.abs_diff(e)))
                    .sum::<u64>();
                // Rasterizers differ at edges; uniform interiors agree.
                if matches!(expected[3], 0 | 255)
                    && (y - 1..=y + 1).all(|ny| (x - 1..=x + 1).all(|nx| pixel(nx, ny) == expected))
                {
                    assert!(
                        got.iter().zip(expected).all(|(g, e)| g.abs_diff(e) <= 1),
                        "phase {phase}, ({x}, {y}): {got:?} versus browser {expected:?}"
                    );
                }
            }
        }
        let mean = error as f64 / (126.0 * 94.0 * 4.0);
        assert!(
            mean < 2.0,
            "phase {phase}: mean channel error against Chromium: {mean}"
        );
    }
}

fn assert_bounded(what: &str, clips: &[u32]) {
    let (second, last) = (clips[1], clips[clips.len() - 1]);
    assert!(
        last <= second,
        "{what}: frame 2 has {second} clips, frame {} has {last}",
        clips.len()
    );
}

#[test]
fn the_same_rectangle_clipped_every_frame_without_save_stays_one_layer() {
    let clips = clips_per_frame(600, |ctx, i| {
        ctx.clear_rect(0.0, 0.0, 200.0, 100.0);
        ctx.begin_path();
        ctx.rect(10.0, 10.0, 100.0, 50.0);
        ctx.clip();
        ctx.fill_rect(20.0 + (i % 60) as f64, 20.0, 30.0, 30.0);
    });
    assert_bounded("same rectangle", &clips);
}

#[test]
fn the_same_circle_clipped_every_frame_without_save_stays_one_layer() {
    let clips = clips_per_frame(600, |ctx, i| {
        ctx.clear_rect(0.0, 0.0, 200.0, 100.0);
        ctx.begin_path();
        ctx.arc(50.0, 50.0, 40.0, 0.0, std::f64::consts::TAU)
            .unwrap();
        ctx.clip();
        ctx.fill_rect((i % 60) as f64, 20.0, 30.0, 30.0);
    });
    assert_bounded("same circle", &clips);
}

#[test]
fn varying_curved_clips_keep_bounded_work_across_frames() {
    let clips = clips_per_frame(600, |ctx, i| {
        ctx.begin_path();
        ctx.arc(
            70.0 + i as f64 / 100.0,
            50.0,
            40.0,
            0.0,
            std::f64::consts::TAU,
        )
        .unwrap();
        ctx.clip();
        ctx.fill_rect(0.0, 0.0, 200.0, 100.0);
    });
    assert!(
        clips.iter().all(|n| *n <= 32),
        "varying curves: final frame has {} clips",
        clips.last().unwrap()
    );
}

#[test]
fn a_moving_rectangle_clipped_every_frame_without_save_folds_into_one_layer() {
    let clips = clips_per_frame(600, |ctx, i| {
        ctx.clear_rect(0.0, 0.0, 200.0, 100.0);
        ctx.begin_path();
        ctx.rect((i % 50) as f64, 0.0, 150.0, 100.0);
        ctx.clip();
        ctx.fill_rect(0.0, 0.0, 200.0, 100.0);
    });
    assert_bounded("moving rectangle", &clips);
}

#[test]
fn a_folded_clip_still_ends_at_its_restore_and_an_outer_one_does_not() {
    let ctx = Context2d::new();
    let mut replayer = Replayer::new(200, 100, 1.0);
    let mut frame = Frame {
        scene: Scene::new(),
        keep: false,
        shadows: Vec::new(),
        masks: Vec::new(),
    };
    let replay = |replayer: &mut Replayer, frame: &mut Frame| {
        for list in ctx.take_lists() {
            replayer.apply(&list, frame, &mut NoHost).unwrap();
        }
    };
    ctx.save();
    for x in [0.0, 10.0] {
        ctx.begin_path();
        ctx.rect(x, 0.0, 100.0, 100.0);
        ctx.clip();
    }
    replay(&mut replayer, &mut frame);
    assert!(replayer.clipped());
    ctx.restore();
    replay(&mut replayer, &mut frame);
    assert!(!replayer.clipped(), "restore drops the folded clip");

    ctx.begin_path();
    ctx.rect(0.0, 0.0, 100.0, 100.0);
    ctx.clip();
    ctx.save();
    ctx.begin_path();
    ctx.rect(10.0, 0.0, 100.0, 100.0);
    ctx.clip();
    ctx.restore();
    replay(&mut replayer, &mut frame);
    assert!(
        replayer.clipped(),
        "a clip inside save does not fold into the one outside it"
    );
}

#[test]
fn a_rectangle_folded_across_frames_paints_only_the_intersection() {
    use exact_canvas_vello::surface::make;
    use exact_canvas_vello::{ecg_canvas_free, ecg_canvas_new, ecg_canvas_replay};
    let (w, h) = (40u32, 10u32);
    let canvas = ecg_canvas_new(w, h, 1.0);
    if canvas.is_null() {
        let why = "missing gpu: the folded clip's pixels need Metal";
        assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{why}");
        eprintln!("SKIP {why}");
        return;
    }
    let surface = make::surface(w, h);
    let ctx = Context2d::new();
    for (x, paint) in [(0.0, false), (10.0, true)] {
        ctx.begin_path();
        ctx.rect(x, 0.0, 20.0, 10.0);
        ctx.clip();
        if paint {
            ctx.fill_rect(0.0, 0.0, 40.0, 10.0);
        }
        let lists = ctx.take_lists();
        let pointers: Vec<_> = lists.iter().map(|l| l.as_ptr()).collect();
        let lengths: Vec<_> = lists.iter().map(|l| l.len()).collect();
        // SAFETY: a live canvas and surface, and lists that outlive the call.
        let result = unsafe {
            ecg_canvas_replay(
                canvas,
                surface.0,
                std::ptr::null_mut(),
                pointers.as_ptr(),
                lengths.as_ptr(),
                lists.len(),
                std::ptr::null(),
            )
        };
        assert_eq!(result, 0);
    }
    let px = make::pixels(surface, w, h);
    let alpha: Vec<u8> = (0..w as usize)
        .map(|x| px[(5 * w as usize + x) * 4 + 3])
        .collect();
    let painted: Vec<usize> = (0..w as usize).filter(|&x| alpha[x] == 255).collect();
    assert_eq!(
        painted,
        (10..20).collect::<Vec<_>>(),
        "alpha by column: {alpha:?}"
    );
    // SAFETY: no replay uses the canvas or surface after this point.
    unsafe { ecg_canvas_free(canvas) };
    make::release(surface);
}

fn circle_clip(ctx: &Context2d, radius: f64) {
    ctx.begin_path();
    ctx.arc(32.0, 24.0, radius, 0.0, std::f64::consts::TAU)
        .unwrap();
    ctx.clip();
}

#[test]
fn compacted_paths_still_skip_duplicate_clips() {
    if !metal() {
        return;
    }
    for repeated in [0, 8] {
        let mut baseline = Pixels::new();
        let mut duplicate = Pixels::new();
        for p in [&mut baseline, &mut duplicate] {
            for i in 0..9 {
                circle_clip(
                    &p.ctx,
                    if i == repeated {
                        20.1
                    } else {
                        24.0 + i as f64 / 10.0
                    },
                );
            }
            p.replay();
        }
        circle_clip(&duplicate.ctx, 20.1);
        for p in [&mut baseline, &mut duplicate] {
            p.ctx.set_fill_style_str("white");
            p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
            p.replay();
        }
        compare_pixels(
            &duplicate.pixels(),
            &baseline.pixels(),
            0,
            "duplicate compacted path",
        );
    }
}

#[test]
fn compacted_masks_do_not_use_the_image_atlas() {
    if !metal() {
        return;
    }
    let mut compact = Pixels::sized(9000, 100, 1.0);
    let mut reference = Pixels::sized(9000, 100, 1.0);
    for i in 0..9 {
        reference.ctx.save();
        for p in [&mut compact, &mut reference] {
            circle_clip(&p.ctx, 20.1 + i as f64 / 10.0);
        }
    }
    for p in [&mut compact, &mut reference] {
        p.ctx.set_fill_style_str("white");
        p.ctx.fill_rect(0.0, 0.0, 9000.0, 100.0);
        p.replay();
    }
    compare_pixels_at_width(
        &compact.pixels(),
        &reference.pixels(),
        1,
        "wide canvas mask",
        9000,
    );
}

#[test]
fn compacted_masks_cover_partial_tiles() {
    if !validated_metal("compacted_masks_cover_partial_tiles", true) {
        return;
    }
    for (width, height) in [(128, 97), (129, 97)] {
        let mut compact = Pixels::sized(width, height, 1.0);
        let mut reference = Pixels::sized(width, height, 1.0);
        for batch in 0..2 {
            for i in 0..9 {
                reference.ctx.save();
                for p in [&mut compact, &mut reference] {
                    p.ctx.begin_path();
                    p.ctx
                        .arc(
                            64.0,
                            48.5,
                            65.0 + (batch * 9 + i) as f64 / 10.0,
                            0.0,
                            std::f64::consts::TAU,
                        )
                        .unwrap();
                    p.ctx.clip();
                }
            }
            for p in [&mut compact, &mut reference] {
                p.ctx
                    .clear_rect(0.0, 0.0, f64::from(width), f64::from(height));
                p.ctx.set_fill_style_str("white");
                p.ctx
                    .fill_rect(0.0, 0.0, f64::from(width), f64::from(height));
                p.replay();
            }
            compare_pixels_at_width(
                &compact.pixels(),
                &reference.pixels(),
                1,
                "partial tile mask",
                width as usize,
            );
        }
    }
}

#[test]
fn compacted_masks_keep_only_clips_that_outlive_batched_resets() {
    if !validated_metal(
        "compacted_masks_keep_only_clips_that_outlive_batched_resets",
        false,
    ) {
        return;
    }
    let mut p = Pixels::sized(2048, 2048, 1.0);
    // A reset drops the masks kept from earlier replays: only the clips in
    // force when the replay ends keep coverage.
    for level in 0..3 {
        for i in 0..9 {
            circle_clip(&p.ctx, 20.1 + level as f64 + i as f64 / 10.0);
        }
        p.ctx.save();
    }
    p.replay();
    assert_eq!(p.canvas.bytes(), 3 * 2048 * 2048 * 4);
    for _ in 0..257 {
        p.ctx.reset();
        for i in 0..9 {
            circle_clip(&p.ctx, 21.0 + i as f64 / 10.0);
        }
    }
    p.ctx.set_fill_style_str("white");
    p.ctx.fill_rect(0.0, 0.0, 2048.0, 2048.0);
    p.replay();
    assert_eq!(p.canvas.bytes(), 2048 * 2048 * 4);
    let pixels = p.pixels();
    assert_eq!(
        &pixels[(24 * 2048 + 32) * 4..(24 * 2048 + 33) * 4],
        &[255; 4]
    );
    assert_eq!(&pixels[..4], &[0; 4]);
    p.ctx.reset();
    p.replay();
    assert_eq!(p.canvas.bytes(), 0);
    assert!(p.pixels().iter().all(|b| *b == 0));
}

#[test]
fn clips_restored_within_one_replay_reserve_no_masks() {
    if !metal() {
        return;
    }
    let mut p = Pixels::sized(2048, 2048, 1.0);
    p.ctx.set_fill_style_str("white");
    for _ in 0..257 {
        p.ctx.save();
        for i in 0..9 {
            circle_clip(&p.ctx, 20.1 + i as f64 / 10.0);
        }
        p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
        p.ctx.restore();
    }
    p.replay();
    assert_eq!(p.canvas.bytes(), 0);
    // Without `save`, the clips outlive the replay: one mask holds them all.
    for i in 0..2100 {
        circle_clip(&p.ctx, 20.1 + i as f64 / 1000.0);
    }
    p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
    p.replay();
    // The scratch copy of the kept pixels, and the mask.
    assert_eq!(p.canvas.bytes(), 2 * 2048 * 2048 * 4);
    p.ctx.set_fill_style_str("lime");
    p.ctx.fill_rect(0.0, 0.0, 64.0, 48.0);
    p.replay();
    let pixels = p.pixels();
    assert_eq!(
        &pixels[(24 * 2048 + 32) * 4..(24 * 2048 + 33) * 4],
        &[0, 255, 0, 255]
    );
    assert_eq!(&pixels[..4], &[0; 4]);
}
