//! `ink.wgsl`'s coverage and shadow, held per pixel to the shader's own
//! arithmetic run on the CPU without its early returns, at the edges of the
//! regions those returns take: pixels either side of each, radii near half the
//! box and uneven radii, rings, clips, transforms, outer shadows with spread
//! and offset, and local coordinates either side of 2^15.

// The shader's literals, as it writes them.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use super::*;
use crate::instance::{flags, Bindings, Brush, Encoder, Instance, Kind, RoundedClip};

struct Scene {
    name: &'static str,
    size: [u32; 2],
    /// Alpha may differ from the reference by this much (out of 255).
    tolerance: u8,
    instances: Vec<Instance>,
}

const WHITE: [f32; 4] = [1.; 4];

fn rounded(rect: [f32; 4], radii: [f32; 4], xform: [f32; 6]) -> Instance {
    let mut i = Instance::rounded(rect, radii, WHITE);
    i.xform = xform;
    i
}

fn ring(outer: [f32; 4], radii: [f32; 4], inner: [f32; 4], inner_radii: [f32; 4]) -> Instance {
    let mut i = Instance::new(Kind::Ring, Brush::Solid, outer);
    i.radii = radii;
    i.uv = inner;
    i.param = inner_radii;
    i
}

/// An outer `box-shadow` as the Linux adapter emits it: `border` offset by
/// `offset`, grown by `spread`, blurred by `sigma`, excluded inside `border`.
/// `scale` and `translate` place local coordinates on the device.
fn shadow(
    border: [f32; 4],
    radius: f32,
    offset: [f32; 2],
    spread: f32,
    sigma: f32,
    scale: f32,
    translate: [f32; 2],
) -> Instance {
    let [x, y, w, h] = border;
    let shape = [
        x - spread + offset[0],
        y - spread + offset[1],
        w + 2. * spread,
        h + 2. * spread,
    ];
    let reach = 3. * sigma + 1.;
    let mut i = Instance::new(
        Kind::Shadow,
        Brush::Solid,
        [
            shape[0] - reach,
            shape[1] - reach,
            shape[2] + 2. * reach,
            shape[3] + 2. * reach,
        ],
    );
    i.xform = [scale, 0., 0., scale, translate[0], translate[1]];
    i.uv = shape;
    i.radii = [(radius + spread).clamp(0., shape[2].min(shape[3]) / 2.); 4];
    i.param[0] = sigma;
    i.set_clip(
        1,
        RoundedClip {
            rect: [
                x * scale + translate[0],
                y * scale + translate[1],
                w * scale,
                h * scale,
            ],
            radii: [radius * scale; 4],
        },
    )
    .unwrap();
    i
}

const IDENTITY: [f32; 6] = [1., 0., 0., 1., 0., 0.];

fn at(x: f32, y: f32) -> [f32; 6] {
    [1., 0., 0., 1., x, y]
}

fn scenes() -> Vec<Scene> {
    let mut scenes = Vec::new();
    // Each origin moves the early region's edge across pixel centres.
    for (n, o) in [0., 0.3125, 0.5, 0.8125].into_iter().enumerate() {
        scenes.push(Scene {
            name: ["corners-0", "corners-1", "corners-2", "corners-3"][n],
            size: [256, 192],
            tolerance: 1,
            instances: vec![
                rounded([8. + o, 8. + o, 60., 40.], [6.; 4], IDENTITY),
                rounded([80. + o, 8. + o, 40., 40.], [19.97; 4], IDENTITY),
                rounded([130. + o, 8. + o, 40., 40.], [20.; 4], IDENTITY),
                rounded(
                    [180. + o, 8. + o, 60., 50.],
                    [0., 12., 3.5, 24.98],
                    IDENTITY,
                ),
                rounded([8. + o, 70. + o, 7., 7.], [1.; 4], IDENTITY),
                rounded([30. + o, 70. + o, 100., 9.3], [0.; 4], IDENTITY),
                rounded([140. + o, 70. + o, 100., 100.], [0.; 4], IDENTITY),
                rounded([8. + o, 90. + o, 120., 90.], [44.95, 0., 30., 8.], IDENTITY),
            ],
        });
    }
    scenes.push(Scene {
        name: "transforms",
        size: [256, 256],
        tolerance: 1,
        instances: vec![
            rounded([0., 0., 50., 40.], [8.; 4], [2., 0., 0., 2., 10.3, 10.7]),
            rounded(
                [0., 0., 60., 50.],
                [4., 16., 0., 24.9],
                [0.9, 0.2, -0.15, 0.85, 150., 10.],
            ),
            rounded([0., 0., 120., 80.], [30.; 4], [0.5, 0., 0., 0.5, 10., 110.]),
            rounded(
                [0., 0., 40., 30.],
                [6.; 4],
                [1.5, 0., 0., 1.5, 90.25, 110.5],
            ),
            rounded(
                [0., 0., 400., 200.],
                [99.9; 4],
                [0.25, 0., 0., 0.25, 160., 120.],
            ),
            rounded([0., 0., 30., 40.], [3.; 4], [-1., 0., 0., 1., 60., 180.]),
        ],
    });
    // 2^15 is where the coverage early return stops being taken.
    scenes.push(Scene {
        name: "large",
        size: [256, 256],
        tolerance: 1,
        instances: vec![
            rounded([30000.3, 29999.7, 80., 60.], [10.; 4], at(-29990., -29990.)),
            rounded([32700.2, 32700.2, 60., 60.], [12.; 4], at(-32600., -32690.)),
            rounded([32740., 32740., 60., 60.], [12.; 4], at(-32570., -32730.)),
            rounded([100000., 100000., 60., 50.], [8.; 4], at(-99990., -99900.)),
            rounded([-31000., 20000., 70., 50.], [6.; 4], at(31100., -19900.)),
            rounded(
                [30000., 30000., 280., 200.],
                [40.; 4],
                [0.25, 0., 0., 0.25, -7320., -7320.],
            ),
        ],
    });
    scenes.push(Scene {
        name: "rings",
        size: [256, 192],
        tolerance: 1,
        instances: vec![
            ring(
                [10., 10., 80., 60.],
                [12.; 4],
                [13., 13., 74., 54.],
                [9.; 4],
            ),
            ring(
                [100., 10., 80., 80.],
                [40.; 4],
                [120., 30., 40., 40.],
                [20.; 4],
            ),
            ring(
                [190., 10., 60., 40.],
                [0., 10., 4., 16.],
                [191., 11., 58., 38.],
                [0., 9., 3., 15.],
            ),
            ring(
                [10., 100., 100., 60.],
                [8.; 4],
                [11.5, 101.5, 97., 57.],
                [6.5; 4],
            ),
            ring(
                [130., 100., 110., 80.],
                [39.9; 4],
                [150., 120., 70., 40.],
                [19.9; 4],
            ),
        ],
    });
    let mut clipped = Vec::new();
    for (rect, clips) in [
        (
            [0., 0., 128., 192.],
            [
                ([10.5, 10.25, 100., 80.], [12.; 4]),
                ([20., 50., 90., 120.], [0., 30., 8., 44.9375]),
            ],
        ),
        (
            [128., 0., 128., 192.],
            [
                ([-1000., -1000., 3000., 3000.], [16.; 4]),
                ([140., 100., 100., 80.], [39.9375; 4]),
            ],
        ),
        (
            [256., 0., 128., 192.],
            [
                ([-40000., -40000., 80000., 80000.], [8.; 4]),
                ([270., 20., 100., 150.], [0., 0., 49.9375, 3.]),
            ],
        ),
    ] {
        let mut i = rounded(rect, [0.; 4], IDENTITY);
        for (slot, (rect, radii)) in clips.into_iter().enumerate() {
            i.set_clip(slot, RoundedClip { rect, radii }).unwrap();
        }
        clipped.push(i);
    }
    scenes.push(Scene {
        name: "clips",
        size: [384, 192],
        tolerance: 1,
        instances: clipped,
    });
    // A dialog: its shadow, then the box over it.
    scenes.push(Scene {
        name: "dialog",
        size: [384, 320],
        tolerance: 2,
        instances: vec![
            shadow([60., 60., 260., 180.], 12., [0., 8.], 0., 12., 1., [0.; 2]),
            rounded([60., 60., 260., 180.], [12.; 4], IDENTITY),
        ],
    });
    // The same shadows alone, so the excluded border box shows.
    scenes.push(Scene {
        name: "shadows",
        size: [384, 384],
        tolerance: 2,
        instances: vec![
            shadow([60., 60., 260., 180.], 12., [0., 8.], 0., 12., 1., [0.; 2]),
            shadow([20., 270., 80., 60.], 20., [4., 6.], 6., 4., 1., [0.; 2]),
            shadow([130., 270., 90., 70.], 0., [0., 0.], -10., 10., 1., [0.; 2]),
            shadow([250., 270., 60., 60.], 29.9, [2., -3.], 3., 6., 1., [0.; 2]),
        ],
    });
    scenes.push(Scene {
        name: "shadows-hard-and-wide",
        size: [384, 256],
        tolerance: 2,
        instances: vec![
            shadow([20., 20., 120., 90.], 10., [5., 5.], 0., 0., 1., [0.; 2]),
            shadow([230., 60., 50., 40.], 8., [0., 0.], 0., 40., 1., [0.; 2]),
            shadow([20., 150., 80., 40.], 16.5, [0., 4.], 2., 8., 1., [0.; 2]),
        ],
    });
    scenes.push(Scene {
        name: "shadows-scaled-and-far",
        size: [384, 256],
        tolerance: 2,
        instances: vec![
            shadow([10., 10., 80., 60.], 12., [0., 4.], 0., 6., 2., [10.5, 20.]),
            shadow(
                [30000., 30000., 120., 90.],
                14.,
                [0., 6.],
                2.,
                8.,
                1.,
                [-29790., -29960.],
            ),
        ],
    });
    scenes
}

fn rounded_distance(p: [f32; 2], rect: [f32; 4], r: [f32; 4]) -> f32 {
    let lo = [rect[0], rect[1]];
    let hi = [lo[0] + rect[2], lo[1] + rect[3]];
    let corner = |c: [f32; 2], r: f32| ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt() - r;
    if p[0] < lo[0] + r[0] && p[1] < lo[1] + r[0] {
        return corner([lo[0] + r[0], lo[1] + r[0]], r[0]);
    }
    if p[0] > hi[0] - r[1] && p[1] < lo[1] + r[1] {
        return corner([hi[0] - r[1], lo[1] + r[1]], r[1]);
    }
    if p[0] > hi[0] - r[2] && p[1] > hi[1] - r[2] {
        return corner([hi[0] - r[2], hi[1] - r[2]], r[2]);
    }
    if p[0] < lo[0] + r[3] && p[1] > hi[1] - r[3] {
        return corner([lo[0] + r[3], hi[1] - r[3]], r[3]);
    }
    let q = [0, 1].map(|a| (p[a] - (lo[a] + hi[a]) * 0.5).abs() - rect[2 + a] * 0.5);
    (q[0].max(0.).powi(2) + q[1].max(0.).powi(2)).sqrt() + q[0].max(q[1]).min(0.)
}

/// The shader's `shape_coverage` without its early return.
fn coverage(p: [f32; 2], rect: [f32; 4], radii: [f32; 4], pixel: [f32; 2]) -> f32 {
    let distance = rounded_distance(p, rect, radii);
    let normal = [
        rounded_distance([p[0] + 0.01, p[1]], rect, radii) - distance,
        rounded_distance([p[0], p[1] + 0.01], rect, radii) - distance,
    ];
    let width =
        (normal[0].abs() * 100. * pixel[0] + normal[1].abs() * 100. * pixel[1]).max(0.00001);
    if rect[2] > 0. && rect[3] > 0. {
        (0.5 - distance / width).clamp(0., 1.)
    } else {
        0.
    }
}

fn normal_cdf(x: f32) -> f32 {
    let a = x.abs() * 0.707106781;
    let t = 1. / (1. + 0.3275911 * a);
    let poly =
        t * (0.2548296 + t * (-0.2844967 + t * (1.4214137 + t * (-1.4531521 + t * 1.0614054))));
    let tail = 0.5 * poly * (-a * a).exp();
    if x >= 0. {
        1. - tail
    } else {
        tail
    }
}

/// The shader's `shadow`, integral always evaluated.
fn shadow_alpha(p: [f32; 2], rect: [f32; 4], radii: [f32; 4], sigma: f32, pixel: [f32; 2]) -> f32 {
    if sigma <= 0. {
        return coverage(p, rect, radii, pixel);
    }
    let low = p[1] - rect[1] - rect[3];
    let high = p[1] - rect[1];
    let start = (-3. * sigma).clamp(low, high);
    let end = (3. * sigma).clamp(low, high);
    let step = (end - start) / 8.;
    let mut alpha = 0.;
    for i in 0..8 {
        let y = start + (i as f32 + 0.5) * step;
        let line = p[1] - y;
        let top = line - rect[1];
        let bottom = rect[1] + rect[3] - line;
        let left_r = if top < radii[0] { radii[0] } else { radii[3] };
        let right_r = if top < radii[1] { radii[1] } else { radii[2] };
        let dl = (left_r - if top < radii[0] { top } else { bottom }).max(0.);
        let dr = (right_r - if top < radii[1] { top } else { bottom }).max(0.);
        let left = rect[0] + left_r - (left_r * left_r - dl * dl).max(0.).sqrt();
        let right = rect[0] + rect[2] - right_r + (right_r * right_r - dr * dr).max(0.).sqrt();
        let integral = normal_cdf((right - p[0]) / sigma) - normal_cdf((left - p[0]) / sigma);
        alpha += integral * (-0.5 * (y / sigma) * (y / sigma)).exp() * (0.39894228 / sigma) * step;
    }
    alpha.clamp(0., 1.)
}

fn unpack(packed: &[u32]) -> [f32; 4] {
    [
        packed[0] & 65535,
        packed[0] >> 16,
        packed[1] & 65535,
        packed[1] >> 16,
    ]
    .map(|r| r as f32 / 16.)
}

/// One instance's alpha at a device pixel centre, as the shader computes it.
fn reference(i: &Instance, device: [f32; 2]) -> f32 {
    let [a, b, c, d, tx, ty] = i.xform.map(f64::from);
    let det = a * d - b * c;
    let (x, y) = (device[0] as f64 - tx, device[1] as f64 - ty);
    let local = [
        ((d * x - c * y) / det) as f32,
        ((a * y - b * x) / det) as f32,
    ];
    let pixel = [
        ((c.abs() + d.abs()) / det.abs()) as f32,
        ((a.abs() + b.abs()) / det.abs()) as f32,
    ];
    let quad = if i.kind_brush & 65535 <= 1 {
        [
            (c * c + d * d).sqrt() / det.abs(),
            (a * a + b * b).sqrt() / det.abs(),
        ]
    } else {
        [0.; 2]
    };
    let inside = (0..2).all(|n| {
        let p = local[n] as f64;
        p >= i.rect[n] as f64 - quad[n] && p < (i.rect[n] + i.rect[n + 2]) as f64 + quad[n]
    });
    if !inside {
        return 0.;
    }
    let mut alpha = match i.kind_brush & 65535 {
        0 => coverage(local, i.rect, i.radii, pixel),
        1 => (coverage(local, i.rect, i.radii, pixel) - coverage(local, i.uv, i.param, pixel))
            .max(0.),
        2 => shadow_alpha(local, i.uv, i.radii, i.param[0], pixel),
        _ => unreachable!(),
    };
    if i.flags & flags::CLIP0 != 0 {
        alpha *= coverage(device, i.clip0, unpack(&i.clip_radii[..2]), [1.; 2]);
    }
    if i.flags & flags::CLIP1 != 0 {
        let clip = coverage(device, i.clip1, unpack(&i.clip_radii[2..]), [1.; 2]);
        alpha *= if i.kind_brush & 65535 == 2 {
            1. - clip
        } else {
            clip
        };
    }
    alpha.clamp(0., 1.)
}

fn render(d: &mut impl Device, scene: &Scene) -> Vec<u8> {
    let [w, h] = scene.size;
    let target = d.create_target(w, h).unwrap();
    let mut encoder = Encoder::new(w, h);
    for i in &scene.instances {
        encoder.push(*i, Bindings::default()).unwrap();
    }
    d.render(&encoder.finish(), target, Some([0.; 4])).unwrap();
    let pixels = d.readback(target).unwrap();
    d.retire_texture(target).unwrap();
    pixels
}

fn agrees_with_the_shader_arithmetic(d: &mut impl Device) {
    for scene in scenes() {
        let pixels = render(d, &scene);
        let [w, h] = scene.size;
        let mut worst = (0, 0, 0, 0.);
        for y in 0..h {
            for x in 0..w {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let expected = scene.instances.iter().fold(0., |under, i| {
                    let a = reference(i, p);
                    a + (1. - a) * under
                });
                let actual = pixels[((y * w + x) * 4 + 3) as usize];
                let off = actual.abs_diff((expected * 255.).round() as u8);
                if off > worst.0 {
                    worst = (off, x, y, expected);
                }
            }
        }
        let (off, x, y, expected) = worst;
        assert!(
            off <= scene.tolerance,
            "{}: alpha at ({x}, {y}) is {off}/255 from the reference {:.4}",
            scene.name,
            expected * 255.
        );
    }
}

#[test]
fn wgpu_coverage_and_shadows_agree_with_the_shader_arithmetic() {
    match super::wgpu::Wgpu::new() {
        Ok(mut d) => agrees_with_the_shader_arithmetic(&mut d),
        Err(Error::NoAdapter(e)) => eprintln!("no GPU here ({e}); ink's pixels are not checked"),
        Err(e) => panic!("ink device: {e}"),
    }
}

#[cfg(exact_ink_metal)]
#[test]
fn metal_coverage_and_shadows_agree_with_the_shader_arithmetic() {
    agrees_with_the_shader_arithmetic(&mut super::metal::Metal::new().unwrap());
}

#[cfg(windows)]
#[test]
fn d3d11_coverage_and_shadows_agree_with_the_shader_arithmetic() {
    agrees_with_the_shader_arithmetic(&mut super::d3d11::D3d11::new().unwrap());
}

#[test]
fn fixed_programs_match_general_coverage_scenes_pixel_for_pixel() {
    let mut d = wgpu::Wgpu::new().expect("wgpu device for program parity");
    for scene in scenes() {
        d.general_only = true;
        let general = render(&mut d, &scene);
        d.general_only = false;
        let specialized = render(&mut d, &scene);
        assert_eq!(general, specialized, "{}", scene.name);
    }
}
