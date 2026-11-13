use super::{Camera, Projection};
use crate::Data;

impl Camera {
    /// Orthographic vertical extent in world units, looking along negative Z.
    pub fn orthographic(height: f32) -> Self {
        Self {
            projection: Projection::Orthographic {
                height,
                integer_scale: false,
            },
            ..Self::default()
        }
    }
    /// Snap the orthographic ratio down to whole CSS pixels per world unit. Extra world
    /// area becomes visible at noninteger ratios; smaller canvases crop at 1x.
    pub fn integer_scale(mut self) -> Self {
        if let Projection::Orthographic { integer_scale, .. } = &mut self.projection {
            *integer_scale = true;
        }
        self
    }
    /// Validate camera selection in both the renderer and spatial reads.
    pub fn valid(self) -> bool {
        self.active
            && self.shift.is_finite()
            && self.near.is_finite()
            && self.far.is_finite()
            && self.near > 0.
            && self.far > self.near
            && match self.projection {
                Projection::Perspective => self.fov_y_degrees > 0. && self.fov_y_degrees < 180.,
                Projection::Orthographic { height, .. } => height.is_finite() && height > 0.,
            }
    }
    /// Shared WebGPU-depth projection. Size is the CSS-pixel viewport on every
    /// surface, so layout/pick and rendering agree across display scales.
    pub fn matrix(self, size: crate::Vec2) -> glam::Mat4 {
        let mut projection = match self.projection {
            Projection::Perspective => glam::camera::rh::proj::directx::perspective(
                self.fov_y_degrees.to_radians(),
                size.x / size.y,
                self.near,
                self.far,
            ),
            Projection::Orthographic {
                height,
                integer_scale,
            } => {
                let height = if integer_scale {
                    size.y / crate::math::floor(size.y / height).max(1.)
                } else {
                    height
                };
                let width = height * size.x / size.y;
                glam::Mat4::from_cols_array(&[
                    2. / width,
                    0.,
                    0.,
                    0.,
                    0.,
                    2. / height,
                    0.,
                    0.,
                    0.,
                    0.,
                    1. / (self.near - self.far),
                    0.,
                    0.,
                    0.,
                    self.near / (self.near - self.far),
                    1.,
                ])
            }
        };
        if self.shift != crate::Vec2::ZERO {
            for column in [
                &mut projection.x_axis,
                &mut projection.y_axis,
                &mut projection.z_axis,
                &mut projection.w_axis,
            ] {
                column.x += self.shift.x * column.w;
                column.y += self.shift.y * column.w;
            }
        }
        projection
    }
}

impl crate::Component for Camera {
    const NAME: &'static str = "Camera";
}

// Zero shift keeps the camera's existing saved bytes and deterministic hashes.
macro_rules! camera_record {
    ($($field:ident),* $(,)?) => {
        impl Data for Camera {
            const TYPE_ID: &'static str = "table:exact_game::scene::Camera";
            fn write(&self, w: &mut dyn crate::Writer) {
                w.begin_struct();
                $(w.field(stringify!($field)); self.$field.write(w);)*
                if self.shift.to_array().map(f32::to_bits) != [0; 2] {
                    w.field("shift");
                    self.shift.write(w);
                }
                w.end_struct();
            }
            fn read(&mut self, r: &mut dyn crate::Reader) -> Result<(), crate::DataError> {
                let fields = [$(stringify!($field)),*];
                let mut seen = [false; [$(stringify!($field)),*].len()];
                let mut seen_shift = false;
                r.begin_struct()?;
                while let Some(field) = r.field()? {
                    if let Some(index) = fields.iter().position(|f| *f == field) {
                        seen[index] = true;
                    }
                    match field.as_str() {
                        $(stringify!($field) => self.$field.read(r).map_err(|e| e.at(stringify!($field)))?,)*
                        "shift" => {
                            self.shift.read(r).map_err(|e| e.at("shift"))?;
                            seen_shift = true;
                        }
                        _ => r.skip().map_err(|e| e.at(&field))?,
                    }
                }
                if let Some(index) = seen.iter().position(|s| !s).filter(|_| r.complete()) {
                    return Err(crate::DataError::new("missing field").at(fields[index]));
                }
                if !seen_shift && r.complete() {
                    self.shift = crate::Vec2::ZERO;
                }
                Ok(())
            }
            fn set_field(&mut self, field: &str, text: &str) -> Result<(), crate::DataError> {
                match field {
                    $(stringify!($field) => crate::data::edit_field(&mut self.$field, text),)*
                    "shift" => crate::data::edit_field(&mut self.shift, text),
                    _ => Err(crate::DataError::new(format!("unknown field {field}"))),
                }
            }
        }
    };
}
camera_record!(projection, fov_y_degrees, near, far, active);
