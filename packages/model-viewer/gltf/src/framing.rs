use crate::math::{dot, sub, V3};
use crate::scene::Scene;

/// Framing about the camera target, in metres and radians (LLP 1133.039 D6).
#[derive(Clone, Copy, Debug)]
pub struct Framing {
    pub radius: f32,
    pub distance: f32,
    pub ideal_aspect: f32,
    pub field_of_view: f32,
}

impl Framing {
    pub fn new(scene: &Scene, target: V3, field_of_view: f32) -> Self {
        let vertices = scene
            .indices
            .iter()
            .map(|&index| scene.vertices[index as usize].p);
        let radius = vertices
            .clone()
            .map(|p| {
                let p = sub(p, target);
                dot(p, p).sqrt()
            })
            .fold(0.0f32, f32::max);
        // Empty and point models still need a finite, nonzero camera distance.
        let radius = radius.max(1e-6);
        let distance = radius / (field_of_view * 0.5).sin();
        let tangent = (field_of_view * 0.5).tan();
        let ideal_aspect = vertices
            .map(|p| {
                let [x, y, z] = sub(p, target);
                x.hypot(z) / (distance - y.abs()) / tangent
            })
            .fold(0.0f32, f32::max);
        Self {
            radius,
            distance,
            ideal_aspect,
            field_of_view,
        }
    }

    pub fn vertical_field_of_view(&self, aspect: f32) -> f32 {
        2.0 * ((self.field_of_view * 0.5).tan() * (self.ideal_aspect / aspect).max(1.0)).atan()
    }
}
