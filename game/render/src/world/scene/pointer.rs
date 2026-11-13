use super::*;
use exact_game::{MouseLook, PointerPan, PointerSample};

#[derive(Default)]
pub(crate) struct CameraPointer {
    camera: Option<Entity>,
    pub look: Option<MouseLook>,
    pan: Option<PointerPan>,
    pan_interpolates: bool,
    kids: Vec<History>,
    pub unshown: glam::Vec2,
    pub sample: Option<PointerSample>,
}
impl CameraPointer {
    pub fn contact(&self) -> Option<u64> {
        self.pan.map(|p| p.pointer)
    }
    pub fn feed(
        &mut self,
        w: &World,
        camera: Option<Entity>,
        next_tick: bool,
        structure: bool,
        chains: Option<&ParentChanges>,
    ) {
        let look = camera.and_then(|e| w.get::<MouseLook>(e).map(|p| *p));
        let pan = camera.and_then(|e| w.get::<PointerPan>(e).map(|p| *p));
        let changed = self.camera != camera;
        if changed || camera.is_some_and(|e| snap_in(w, e, chains)) {
            self.pan_interpolates = false;
        } else if next_tick {
            self.pan_interpolates = true;
        }
        let newly_active = self.look.is_none() && self.pan.is_none();
        self.look = look;
        self.pan = pan;
        self.camera = camera;
        let Some(camera) = camera.filter(|_| look.is_some() || pan.is_some()) else {
            self.kids.clear();
            return;
        };
        if changed || newly_active || structure || chains.is_some() {
            let old = std::mem::take(&mut self.kids);
            for (e, _) in w.query::<&Parent>().iter() {
                let mut at = e;
                for _ in 0..=w.len() {
                    let Some(parent) = w.get::<Parent>(at).map(|p| p.0) else {
                        break;
                    };
                    if parent == camera {
                        if let Some(t) = pose(w, e) {
                            self.kids.push(
                                old.iter()
                                    .copied()
                                    .find(|h| h.entity == e)
                                    .unwrap_or(History::new(e, t)),
                            );
                        }
                        break;
                    }
                    at = parent;
                }
            }
        }
        for history in &mut self.kids {
            history.update(w, next_tick, chains);
        }
    }
    pub fn frame(
        &mut self,
        camera: &mut Transform,
        out: &mut Vec<DisplayedAttachment>,
        alpha: f32,
    ) {
        let motion = std::mem::take(&mut self.unshown);
        let turn = self
            .look
            .filter(|_| motion != glam::Vec2::ZERO)
            .map_or(glam::Quat::IDENTITY, |look| {
                look.turn(camera.rotation, motion)
            });
        let translation = match (self.pan, self.sample.take()) {
            (Some(pan), Some(sample)) if sample.id == pan.pointer && sample.current_contact => {
                let undrawn = if self.pan_interpolates {
                    1. - alpha
                } else {
                    0.
                };
                pan.translation(sample.position - pan.position + pan.delta * undrawn)
            }
            _ => Vec3::ZERO,
        };
        let translation = if translation.is_finite() {
            translation
        } else {
            Vec3::ZERO
        };
        if translation == Vec3::ZERO && turn == glam::Quat::IDENTITY {
            return;
        }
        let eye = camera.position;
        let about = Mat4::from_translation(eye + translation)
            * Mat4::from_quat(turn)
            * Mat4::from_translation(-eye);
        camera.position += translation;
        camera.rotation = (turn * camera.rotation).normalize();
        for h in &self.kids {
            let matrix = about * displayed_matrix(out, h.entity, h.at(alpha));
            let (scale, rotation, position) = matrix.to_scale_rotation_translation();
            let moved = DisplayedAttachment {
                entity: h.entity,
                pose: Transform {
                    scale,
                    rotation,
                    position,
                },
                matrix,
            };
            match out.iter_mut().find(|a| a.entity == h.entity) {
                Some(a) => *a = moved,
                None => out.push(moved),
            }
        }
    }
}
