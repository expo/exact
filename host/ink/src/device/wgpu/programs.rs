//! Fixed fragment programs, compiled before the first frame (expo/exact#145).
//! Selectors follow smihica's V3D investigation; clips remain instance data.
use super::*;

pub(super) struct Class {
    kind_brush: u32,
    flags: u32,
}

const CLIPS: u32 = flags::CLIP0 | flags::CLIP1 | flags::MASK;
const IMAGE: u32 = CLIPS | flags::TINT | flags::NEAREST | flags::PIXELATED;

impl Class {
    const fn new(kind: Kind, brush: Brush, flags: u32) -> Self {
        Self {
            kind_brush: kind as u32 | ((brush as u32) << 16),
            flags,
        }
    }

    pub(super) fn constants(&self) -> [(&'static str, f64); 3] {
        [
            ("SPECIALIZED", 1.),
            ("KIND_BRUSH", self.kind_brush as f64),
            ("FLAGS_MASK", self.flags as f64),
        ]
    }
}

// Unlisted kinds, brushes and flags use the general program.
pub(super) const CLASSES: [Class; 8] = [
    Class::new(Kind::RoundedRect, Brush::Solid, CLIPS),
    Class::new(Kind::Ring, Brush::Solid, CLIPS),
    Class::new(Kind::Shadow, Brush::Solid, CLIPS),
    Class::new(Kind::Glyph, Brush::Solid, CLIPS | flags::COLOR_GLYPH),
    Class::new(Kind::Coverage, Brush::Solid, CLIPS),
    Class::new(Kind::Layer, Brush::Image, CLIPS),
    Class::new(Kind::RoundedRect, Brush::Image, IMAGE),
    Class::new(Kind::Coverage, Brush::Image, IMAGE),
];

pub(super) fn class(instance: &Instance) -> Option<usize> {
    CLASSES
        .iter()
        .position(|c| instance.kind_brush == c.kind_brush && instance.flags & !c.flags == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncommon_flags_and_brushes_use_the_general_program() {
        let mut i = Instance::rounded([0.; 4], [0.; 4], [1.; 4]);
        assert_eq!(class(&i), Some(0));
        for bit in 0..32 {
            i.flags = 1 << bit;
            assert_eq!(class(&i), (i.flags & !CLIPS == 0).then_some(0));
        }
        for brush in [Brush::Gradient, Brush::Pattern] {
            i = Instance::new(Kind::RoundedRect, brush, [0.; 4]);
            assert_eq!(class(&i), None);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[test]
fn every_fixed_program_matches_the_general_program_pixel_for_pixel() {
    use crate::instance::{CoverageClip, Encoder, RoundedClip};
    let mut d = Wgpu::new().expect("wgpu device for program parity");
    assert_eq!(d.programs.len(), CLASSES.len());
    eprintln!(
        "Ink: {} fixed + 2 general pipelines, device {:.3} ms, shaders {:.3} ms, cached {}",
        d.programs.len(),
        d.info.device_ms,
        d.info.shaders_ms,
        d.info.cached
    );
    let target = d.create_target(64, 48).unwrap();
    let image = d.create_texture(TextureDesc::rgba(4, 4)).unwrap();
    let alpha = d
        .create_texture(TextureDesc {
            width: 4,
            height: 4,
            format: Format::Alpha,
            mipmapped: false,
        })
        .unwrap();
    d.upload_texture(image, [0, 0], [4, 4], &[32, 16, 8, 128].repeat(16))
        .unwrap();
    d.upload_texture(alpha, [0, 0], [4, 4], &[127; 16]).unwrap();
    let bindings = Bindings {
        image,
        color: image,
        glyph: alpha,
        path: alpha,
        mask: alpha,
        ..Default::default()
    };
    for (index, class) in CLASSES.iter().enumerate() {
        let bits: Vec<_> = (0..32).filter(|b| class.flags & (1 << b) != 0).collect();
        for combination in 0..(1 << bits.len()) {
            let clips = bits.iter().enumerate().fold(0, |value, (n, bit)| {
                value
                    | if combination & (1 << n) != 0 {
                        1 << bit
                    } else {
                        0
                    }
            });
            let mut e = Encoder::new(64, 48);
            let mut i = Instance::rounded(
                [5.25, 4.5, 51., 37.],
                [9., 2., 6., 13.],
                [0.25, 0.125, 0.0625, 0.5],
            );
            i.kind_brush = class.kind_brush;
            i.flags = clips;
            i.uv = [0., 0., 1., 1.];
            i.param = [0.7, 0., 0., 0.];
            if index == 1 {
                i.uv = [8., 8., 43., 27.];
                i.param = [4.; 4];
            }
            if index == 2 {
                i.uv = [13., 12., 32., 23.];
                i.param = [3., 0., 0., 0.];
                i.flags |= flags::CLIP1;
            }
            for slot in 0..2 {
                if i.flags & [flags::CLIP0, flags::CLIP1][slot] != 0 {
                    i.set_clip(
                        slot,
                        RoundedClip {
                            rect: [13.25, 12.5, 33., 24.],
                            radii: [7., 2., 5., 9.],
                        },
                    )
                    .unwrap();
                }
            }
            if clips & flags::MASK != 0 {
                i.set_mask(CoverageClip {
                    texture: alpha,
                    rect: [2., 2., 58., 43.],
                    uv: [0., 0., 1., 1.],
                });
            }
            assert_eq!(super::programs::class(&i), Some(index));
            e.push(i, bindings).unwrap();
            let frame = e.finish();
            d.general_only = true;
            d.render(&frame, target, Some([0.; 4])).unwrap();
            let general = d.readback(target).unwrap();
            d.general_only = false;
            d.render(&frame, target, Some([0.; 4])).unwrap();
            let specialized = d.readback(target).unwrap();
            assert_eq!(general, specialized, "program {index}, clips {clips}");
            assert!(
                specialized.as_chunks::<4>().0.iter().any(|p| p[3] > 0),
                "program {index} drew nothing"
            );
        }
    }
}
