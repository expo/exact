#![cfg(target_vendor = "apple")]
//! What a frame's shadows ask of the GPU: the offscreen texture pixels and
//! the blur shaders' taps, bounded by the canvas rather than the blur
//! radius, and the pixels a wide shadow lands as.
use exact_canvas::Context2d;
use exact_canvas_vello::replay::{Frame, Host, Replayer};
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

/// Each shadow job's texture pixels and blur taps (two passes, each
/// `2 * ceil(3σ) + 1` taps a pixel) when a `width` by `height` canvas at
/// `scale` replays `draw`.
fn jobs(width: u32, height: u32, scale: f64, draw: impl Fn(&Context2d)) -> Vec<(u64, u64)> {
    let ctx = Context2d::new();
    draw(&ctx);
    let mut replayer = Replayer::new(
        (f64::from(width) * scale) as u32,
        (f64::from(height) * scale) as u32,
        scale,
    );
    let mut frame = Frame {
        scene: Scene::new(),
        keep: false,
        shadows: Vec::new(),
    };
    replayer.begin(&mut frame.scene);
    for list in ctx.take_lists() {
        replayer.apply(&list, &mut frame, &mut NoHost).unwrap();
    }
    replayer.end(&mut frame.scene);
    (frame.shadows.iter())
        .map(|job| {
            let pixels = u64::from(job.width) * u64::from(job.height);
            (
                pixels,
                pixels * 2 * (2 * (3.0 * job.sigma).ceil() as u64 + 1),
            )
        })
        .collect()
}

/// A frame's shadow pixels within four canvases' (or a megapixel), and 64
/// by 64 for each shadow past that; at most 97 taps a pass for each.
fn assert_bounded(what: &str, canvas: u64, jobs: &[(u64, u64)]) {
    assert!(!jobs.is_empty(), "{what}: no shadow");
    let pixels: u64 = jobs.iter().map(|j| j.0).sum();
    let taps: u64 = jobs.iter().map(|j| j.1).sum();
    let budget = (4 * canvas).max(1 << 20) + 64 * 64 * jobs.len() as u64;
    assert!(pixels <= budget, "{what}: {pixels} shadow pixels");
    assert!(taps <= budget * 2 * 97, "{what}: {taps} taps");
}

#[test]
fn a_huge_blur_on_a_tiny_canvas_asks_for_work_like_the_canvas() {
    // At shadowBlur 5,000 this was a 15,012-pixel square, 2.7 GB of
    // textures and 6.8e12 taps.
    for (blur, scale) in [(1_000.0, 1.0), (5_000.0, 1.0), (5_000.0, 3.0), (1e9, 1.0)] {
        let jobs = jobs(100, 100, scale, |ctx| {
            ctx.set_shadow_color("black");
            ctx.set_shadow_blur(blur);
            ctx.fill_rect(10.0, 10.0, 10.0, 10.0);
        });
        let canvas = (100.0 * scale * 100.0 * scale) as u64;
        assert_bounded(&format!("blur {blur} at {scale}x"), canvas, &jobs);
    }
}

#[test]
fn many_shadowed_draws_share_one_budget() {
    let jobs = jobs(300, 300, 3.0, |ctx| {
        ctx.set_shadow_color("black");
        ctx.set_shadow_blur(20.0);
        for i in 0..200 {
            ctx.fill_rect(f64::from(i % 50), 0.0, 250.0, 250.0);
        }
    });
    assert_eq!(jobs.len(), 200);
    assert_bounded("200 shadowed draws", 900 * 900, &jobs);
}

#[test]
fn an_ordinary_shadow_is_blurred_at_full_resolution() {
    let jobs = jobs(200, 100, 2.0, |ctx| {
        ctx.set_shadow_color("black");
        ctx.set_shadow_blur(8.0);
        ctx.fill_rect(20.0, 20.0, 50.0, 30.0);
    });
    // σ = 8 device pixels, a 25-pixel margin: (100 + 50) by (60 + 50).
    assert_eq!(jobs, [(150 * 110, 150 * 110 * 2 * 49)]);
}

/// A square's shadow, rendered, against the Gaussian it approximates:
/// blurred at full resolution (shadowBlur 30, σ = 15) and at a lower one
/// drawn back at full size (shadowBlur 120, σ = 60, a quarter of it).
#[test]
fn a_wide_shadow_lands_where_and_as_strong_as_a_full_resolution_one() {
    use exact_canvas_vello::surface::make;
    use exact_canvas_vello::*;

    let (w, h) = (400u32, 300u32);
    let canvas = ecg_canvas_new(w, h, 1.0);
    if canvas.is_null() {
        let why = "missing gpu: the shadow pixel test requires Metal";
        assert_ne!(std::env::var("EXACT_REQUIRED").as_deref(), Ok("1"), "{why}");
        eprintln!("SKIP {why}");
        return;
    }
    // The fraction of a unit Gaussian of `sigma` between `a` and `b`.
    let mass = |a: f64, b: f64, sigma: f64| {
        let steps = 4000;
        let dx = (b - a) / f64::from(steps);
        (0..steps)
            .map(|i| a + (f64::from(i) + 0.5) * dx)
            .map(|x| (-x * x / (2.0 * sigma * sigma)).exp() * dx)
            .sum::<f64>()
            / (sigma * (2.0 * std::f64::consts::PI).sqrt())
    };
    for blur in [30.0, 120.0] {
        let sigma = blur / 2.0;
        let surface = make::surface(w, h);
        let ctx = Context2d::new();
        // The square is off the canvas; its shadow, offset onto it, is
        // (150, 100) to (250, 200).
        ctx.set_shadow_color("black");
        ctx.set_shadow_blur(blur);
        ctx.set_shadow_offset_x(1150.0);
        ctx.set_shadow_offset_y(100.0);
        ctx.fill_rect(-1000.0, 0.0, 100.0, 100.0);
        let lists = ctx.take_lists();
        let pointers: Vec<_> = lists.iter().map(|l| l.as_ptr()).collect();
        let lengths: Vec<_> = lists.iter().map(|l| l.len()).collect();
        let host = EcgHost {
            ctx: std::ptr::null_mut(),
            text_path: None,
            text_runs: None,
            image: None,
        };
        // SAFETY: a live canvas, surface and lists.
        let result = unsafe {
            ecg_canvas_replay(
                canvas,
                surface.0,
                std::ptr::null_mut(),
                pointers.as_ptr(),
                lengths.as_ptr(),
                lists.len(),
                &host,
            )
        };
        assert_eq!(result, 0, "blur {blur}");
        let pixels = make::pixels(surface, w, h);
        for (x, y) in [
            (200, 150),
            (150, 150),
            (120, 150),
            (270, 120),
            (200, 230),
            (330, 260),
        ] {
            let at = |c: u32, lo: f64, hi: f64| {
                mass(lo - (f64::from(c) + 0.5), hi - (f64::from(c) + 0.5), sigma)
            };
            let expected = at(x, 150.0, 250.0) * at(y, 100.0, 200.0);
            let alpha = f64::from(pixels[((y * w + x) * 4 + 3) as usize]) / 255.0;
            assert!(
                (alpha - expected).abs() < 0.03,
                "blur {blur} at ({x}, {y}): alpha {alpha:.3}, a Gaussian's {expected:.3}"
            );
        }
        make::release(surface);
    }
    // SAFETY: made above, freed once.
    unsafe { ecg_canvas_free(canvas) };
}
