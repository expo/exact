use crate::{
    arguments::Arguments,
    camera::{Camera, Pose},
};
use exact_gpu::{InputEvent, PointerKind, PointerPhase};
use exact_model_gltf::{math::*, scene::Scene};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy)]
enum Gesture {
    Orbit,
    Pan,
    Pinch([f32; 2]),
}
struct Contact {
    start: [f32; 2],
    point: [f32; 2],
    began: f64,
    touch: bool,
    dragged: bool,
    gesture: Gesture,
}
#[derive(Default)]
pub struct Controls {
    contacts: BTreeMap<u32, Contact>,
    keys: BTreeSet<String>,
    history: VecDeque<(f64, Pose)>,
}
impl Controls {
    pub fn clear(&mut self) {
        self.contacts.clear();
        self.keys.clear();
        self.history.clear();
    }
    fn pinch(&mut self) {
        let points: Vec<_> = self.contacts.iter().map(|(&id, c)| (id, c.point)).collect();
        for (id, c) in &mut self.contacts {
            if let Some((_, point)) = points.iter().find(|(other, _)| other != id) {
                c.gesture = Gesture::Pinch(*point);
                c.dragged = true;
            }
        }
    }
    fn move_contact(
        &mut self,
        (id, p, at_ms): (u32, [f32; 2], f64),
        scene: &Scene,
        camera: &mut Camera,
        a: &Arguments,
        size: [f32; 2],
    ) {
        let Some(c) = self.contacts.get_mut(&id) else {
            return;
        };
        let old = c.point;
        c.point = p;
        c.dragged |= (p[0] - c.start[0]).hypot(p[1] - c.start[1]) > if c.touch { 10. } else { 8. };
        match c.gesture {
            Gesture::Pinch(other) => {
                let old_mid = [(old[0] + other[0]) * 0.5, (old[1] + other[1]) * 0.5];
                let mid = [(p[0] + other[0]) * 0.5, (p[1] + other[1]) * 0.5];
                let distance = (p[0] - other[0]).hypot(p[1] - other[1]);
                if !a.disable_pan {
                    camera.pan(scene, mid[0] - old_mid[0], mid[1] - old_mid[1], size);
                }
                if !a.disable_zoom && distance > 0. {
                    let old_radius = camera.goal.radius;
                    camera.zoom(
                        scene,
                        (old[0] - other[0]).hypot(old[1] - other[1]) / distance,
                    );
                    if !a.disable_pan {
                        let ratio = camera.goal.radius / old_radius;
                        camera.pan(
                            scene,
                            -(mid[0] - size[0] * 0.5) * (ratio.recip() - 1.),
                            -(mid[1] - size[1] * 0.5) * (ratio.recip() - 1.),
                            size,
                        );
                    }
                }
            }
            Gesture::Pan if !a.disable_pan => {
                camera.pan(scene, p[0] - old[0], p[1] - old[1], size);
            }
            Gesture::Orbit => {
                let radians = std::f32::consts::TAU / size[1].max(1.);
                camera.orbit((p[0] - old[0]) * radians, (p[1] - old[1]) * radians);
            }
            _ => {}
        }
        self.pinch();
        self.sample(at_ms, camera.goal);
    }
    fn sample(&mut self, now: f64, pose: Pose) {
        self.history.push_back((now, pose));
        while self.history.front().is_some_and(|(at, _)| now - at > 100.) {
            self.history.pop_front();
        }
    }
    fn fling(&mut self, scene: &Scene, now: f64, camera: &mut Camera) {
        self.sample(now, camera.goal);
        if let (Some(&(start, a)), Some(&(end, b))) = (self.history.front(), self.history.back()) {
            if end > start && camera.decay > 0. {
                let t = camera.decay / (end - start) as f32;
                camera.goal.theta += (b.theta - a.theta) * t;
                camera.goal.phi =
                    (camera.goal.phi + (b.phi - a.phi) * t).clamp(0., std::f32::consts::PI);
                camera.goal.target = add(camera.goal.target, mul(sub(b.target, a.target), t));
                camera.radius(scene, b.radius + (b.radius - a.radius) * t);
            }
        }
        self.history.clear();
    }
    pub fn event(
        &mut self,
        event: &InputEvent,
        scene: &Scene,
        camera: &mut Camera,
        a: &Arguments,
        size: [f32; 2],
    ) -> Option<[f32; 2]> {
        match event {
            InputEvent::Blur { .. } => self.clear(),
            InputEvent::Key {
                code, key, down, ..
            } => {
                if *down {
                    self.keys.insert(key.clone());
                } else {
                    self.keys.remove(key);
                }
                if !down {
                    return None;
                }
                let delta = std::f32::consts::PI / 8.;
                let (dx, dy) = match code.as_str() {
                    "ArrowLeft" => (-delta, 0.),
                    "ArrowRight" => (delta, 0.),
                    "ArrowUp" => (0., -delta),
                    "ArrowDown" => (0., delta),
                    "PageUp" if !a.disable_zoom => {
                        camera.zoom(scene, 0.9);
                        return None;
                    }
                    "PageDown" if !a.disable_zoom => {
                        camera.zoom(scene, 1. / 0.9);
                        return None;
                    }
                    _ => return None,
                };
                if self.keys.contains("Shift") {
                    if !a.disable_pan {
                        camera.pan(scene, dx * 50., dy * 50., size);
                    }
                } else {
                    camera.orbit(dx, dy);
                }
            }
            InputEvent::Wheel { dy, ctrl_key, .. } if !a.disable_zoom => {
                camera.zoom(
                    scene,
                    (dy * if *ctrl_key { 0.01 } else { 0.002 })
                        .clamp(-5., 5.)
                        .exp(),
                );
            }
            InputEvent::Pointer {
                id,
                phase,
                x,
                y,
                kind,
                buttons,
                shift_key,
                ctrl_key,
                meta_key,
                at_ms,
                ..
            } => {
                let p = [*x, *y];
                let gesture = if *buttons & 2 != 0 || *shift_key || *ctrl_key || *meta_key {
                    Gesture::Pan
                } else {
                    Gesture::Orbit
                };
                match phase {
                    PointerPhase::Down => {
                        if self.contacts.is_empty() {
                            self.history.clear();
                            self.sample(*at_ms, camera.goal);
                        }
                        self.contacts.insert(
                            *id,
                            Contact {
                                start: p,
                                point: p,
                                began: *at_ms,
                                touch: *kind == PointerKind::Touch,
                                dragged: false,
                                gesture,
                            },
                        );
                        self.pinch();
                    }
                    PointerPhase::Move => {
                        if self.contacts.len() == 1 {
                            self.contacts.get_mut(id)?.gesture = gesture;
                        }
                        self.move_contact((*id, p, *at_ms), scene, camera, a, size);
                    }
                    PointerPhase::Up | PointerPhase::Cancel => {
                        // Hosts clear the buttons on release; keep the last drag's gesture.
                        if *phase == PointerPhase::Up
                            && self.contacts.get(id).is_some_and(|c| c.point != p)
                        {
                            self.move_contact((*id, p, *at_ms), scene, camera, a, size);
                        }
                        let Some(c) = self.contacts.remove(id) else {
                            return None;
                        };
                        let tap = *phase == PointerPhase::Up
                            && !c.dragged
                            && (p[0] - c.start[0]).hypot(p[1] - c.start[1])
                                <= if c.touch { 10. } else { 8. }
                            && (!c.touch || *at_ms - c.began <= 500.);
                        if self.contacts.is_empty() {
                            if *phase == PointerPhase::Up && !tap {
                                self.fling(scene, *at_ms, camera);
                            } else {
                                self.history.clear();
                            }
                        }
                        if tap {
                            return Some(p);
                        }
                    }
                }
            }
            _ => {}
        }
        None
    }
}
