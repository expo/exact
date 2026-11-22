//! Quarter turns of a completed image; layout and input stay in logical space.
use crate::device::{Device, Result};
use crate::instance::{Bindings, TextureId};
use crate::{Brush, Encoder, Instance, Kind};

/// Clockwise rotation from the logical image to the panel's framebuffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    pub fn parse(value: &str) -> std::result::Result<Self, String> {
        match value {
            "0" => Ok(Self::R0),
            "90" => Ok(Self::R90),
            "180" => Ok(Self::R180),
            "270" => Ok(Self::R270),
            _ => Err(format!(
                "EXACT_ROTATE must be 0, 90, 180 or 270; got {value:?}"
            )),
        }
    }

    /// Rotated dimensions (also the inverse, for layout from a panel mode).
    pub fn size<T: Copy>(self, [w, h]: [T; 2]) -> [T; 2] {
        match self {
            Self::R90 | Self::R270 => [h, w],
            _ => [w, h],
        }
    }

    /// Map normalized physical axes to logical axes, preserving missing axes.
    pub fn input(self, x: Option<f32>, y: Option<f32>) -> (Option<f32>, Option<f32>) {
        match self {
            Self::R0 => (x, y),
            Self::R90 => (y, x.map(|v| 1. - v)),
            Self::R180 => (x.map(|v| 1. - v), y.map(|v| 1. - v)),
            Self::R270 => (y.map(|v| 1. - v), x),
        }
    }

    pub fn degrees(self) -> u16 {
        match self {
            Self::R0 => 0,
            Self::R90 => 90,
            Self::R180 => 180,
            Self::R270 => 270,
        }
    }

    /// Map a logical pixel edge into the rotated framebuffer.
    pub fn affine(self, [w, h]: [u32; 2]) -> [f32; 6] {
        let (w, h) = (w as f32, h as f32);
        match self {
            Self::R0 => [1., 0., 0., 1., 0., 0.],
            Self::R90 => [0., 1., -1., 0., h, 0.],
            Self::R180 => [-1., 0., 0., -1., w, h],
            Self::R270 => [0., -1., 1., 0., 0., w],
        }
    }

    /// Blit through the painter's device before readback, without resampling.
    pub fn blit(
        self,
        device: &mut impl Device,
        source: TextureId,
        size: [u32; 2],
        target: TextureId,
    ) -> Result<()> {
        let [w, h] = self.size(size);
        let mut e = Encoder::new(w, h);
        let mut i = Instance::new(
            Kind::Layer,
            Brush::Image,
            [0., 0., size[0] as f32, size[1] as f32],
        );
        i.xform = self.affine(size);
        e.push(
            i,
            Bindings {
                image: source,
                ..Default::default()
            },
        )
        .expect("unclipped blit");
        device.render(&e.finish(), target, Some([0.; 4]))?;
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use crate::device::TextureDesc;

    #[test]
    fn quarter_turns_blit_pixels_and_invert_the_same_input_transform() {
        let mut d = crate::Wgpu::new().expect("GPU for rotation pixels");
        let source = d.create_texture(TextureDesc::rgba(3, 2)).unwrap();
        let colors: Vec<u8> = (1..=6).flat_map(|v| [v * 20, 0, 0, 255]).collect();
        d.upload_texture(source, [0, 0], [3, 2], &colors).unwrap();
        for (rotation, expected, input, degrees) in [
            (Rotation::R0, vec![1, 2, 3, 4, 5, 6], (0.2, 0.7), 0),
            (Rotation::R90, vec![4, 1, 5, 2, 6, 3], (0.7, 0.8), 90),
            (Rotation::R180, vec![6, 5, 4, 3, 2, 1], (0.8, 0.3), 180),
            (Rotation::R270, vec![3, 6, 2, 5, 1, 4], (0.3, 0.2), 270),
        ] {
            let [w, h] = rotation.size([3, 2]);
            let target = d.create_target(w, h).unwrap();
            rotation.blit(&mut d, source, [3, 2], target).unwrap();
            assert_eq!(
                d.readback(target).unwrap(),
                expected
                    .into_iter()
                    .flat_map(|v| [v * 20, 0, 0, 255])
                    .collect::<Vec<_>>()
            );
            let (x, y) = rotation.input(Some(0.2), Some(0.7));
            assert!((x.unwrap() - input.0).abs() < 1e-6 && (y.unwrap() - input.1).abs() < 1e-6);
            assert_eq!(rotation.degrees(), degrees);
            let [a, b, c, e, tx, ty] = rotation.affine([3, 2]);
            let logical = (x.unwrap() * 3., y.unwrap() * 2.);
            assert!((a * logical.0 + c * logical.1 + tx - 0.2 * w as f32).abs() < 1e-6);
            assert!((b * logical.0 + e * logical.1 + ty - 0.7 * h as f32).abs() < 1e-6);
            let (x_only, y_only) = rotation.input(Some(0.2), None);
            let (other_x, other_y) = rotation.input(None, Some(0.7));
            assert_eq!(x_only.or(other_x), x);
            assert_eq!(y_only.or(other_y), y);
            d.retire_texture(target).unwrap();
        }
        assert!(Rotation::parse("45").is_err());
        for value in ["0", "90", "180", "270"] {
            assert!(Rotation::parse(value).is_ok());
        }
    }
}
