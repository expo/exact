use crate::arguments::{resolve, Arguments};
use exact_model_gltf::{framing::Framing, math::*, scene::Scene};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub theta: f32,
    pub phi: f32,
    pub radius: f32,
    pub target: V3,
    pub fov: f32,
}
impl Pose {
    pub fn basis(self) -> (V3, V3, V3) {
        let (s, c) = self.theta.sin_cos();
        let (p, q) = self.phi.sin_cos();
        ([c, 0., -s], [-s * q, p, -c * q], [s * p, q, c * p])
    }
    pub fn eye(self) -> V3 {
        add(self.target, mul(self.basis().2, self.radius))
    }
    pub fn view(self) -> M4 {
        let (right, up, back) = self.basis();
        let eye = self.eye();
        [
            [right[0], up[0], back[0], 0.],
            [right[1], up[1], back[1], 0.],
            [right[2], up[2], back[2], 0.],
            [-dot(right, eye), -dot(up, eye), -dot(back, eye), 1.],
        ]
    }
    pub fn ray(self, x: f32, y: f32, width: f32, height: f32, fov: f32) -> (V3, V3) {
        let (right, up, back) = self.basis();
        let v = (fov * 0.5).tan();
        let direction = add(
            mul(back, -1.),
            add(
                mul(right, (2. * x / width - 1.) * v * width / height),
                mul(up, (1. - 2. * y / height) * v),
            ),
        );
        (self.eye(), norm(direction))
    }
    pub fn strings(self) -> (String, String, String, String) {
        let theta = self.theta.to_degrees().rem_euclid(360.);
        let phi = self.phi.to_degrees();
        let side = ["front", "right", "back", "left"][((theta + 45.) / 90.) as usize % 4];
        let prefix = if phi < 60. {
            "upper-"
        } else if phi > 120. {
            "lower-"
        } else {
            ""
        };
        (
            format!("{}deg {}deg {}m", self.theta.to_degrees(), phi, self.radius),
            format!(
                "{}m {}m {}m",
                self.target[0], self.target[1], self.target[2]
            ),
            format!("{}deg", self.fov.to_degrees()),
            format!("{prefix}{side}"),
        )
    }
}

pub struct Camera {
    pub pose: Pose,
    pub goal: Pose,
    pub authored: Pose,
    pub framing: Framing,
    pub decay: f32,
    pub last_ms: Option<f64>,
}
impl Camera {
    pub fn new(scene: &Scene, a: &Arguments) -> Self {
        let target = resolve(a.target, scene.centre);
        let framing = Framing::new(scene, target, a.fov);
        let radius = if a.orbit.relative {
            a.orbit.radius * framing.distance
        } else {
            a.orbit.radius
        };
        let pose = Pose {
            theta: a.orbit.theta,
            phi: a.orbit.phi,
            radius: radius.clamp(0.15 * framing.radius, 12. * framing.radius),
            target,
            fov: a.fov,
        };
        Self {
            pose,
            goal: pose,
            authored: pose,
            framing,
            decay: a.decay,
            last_ms: None,
        }
    }
    pub fn rebind(&mut self, scene: &Scene, a: &Arguments, changed: bool) {
        self.decay = a.decay;
        if changed {
            let new = Self::new(scene, a);
            self.authored = new.pose;
            self.goal = new.pose;
        }
        if self.decay == 0. {
            self.framing = self.framing_at(scene, self.goal);
            self.pose = self.goal;
        }
    }
    pub fn reset(&mut self, scene: &Scene) {
        self.goal = self.authored;
        if self.decay == 0. {
            self.framing = self.framing_at(scene, self.goal);
            self.pose = self.goal;
        }
    }
    pub fn moving(&self) -> bool {
        (self.pose.theta - self.goal.theta).abs() > 0.01f32.to_radians()
            || (self.pose.phi - self.goal.phi).abs() > 0.01f32.to_radians()
            || (self.pose.radius - self.goal.radius).abs() > self.goal.radius * 0.001
            || dot(
                sub(self.pose.target, self.goal.target),
                sub(self.pose.target, self.goal.target),
            )
            .sqrt()
                > self.goal.radius * 0.001
            || (self.pose.fov - self.goal.fov).abs() > 0.01f32.to_radians()
    }
    pub fn settle_at(&self, now: f64) -> f64 {
        let angle = 0.01f32.to_radians();
        let distance = self.goal.radius * 0.001;
        let target = sub(self.pose.target, self.goal.target);
        let ratio = [
            (self.pose.theta - self.goal.theta).abs() / angle,
            (self.pose.phi - self.goal.phi).abs() / angle,
            (self.pose.radius - self.goal.radius).abs() / distance,
            dot(target, target).sqrt() / distance,
            (self.pose.fov - self.goal.fov).abs() / angle,
        ]
        .into_iter()
        .fold(1., f32::max);
        now + f64::from(self.decay) / 3. * f64::from(ratio).ln() + 1.
    }
    pub fn step(&mut self, scene: &Scene, now: f64) -> bool {
        let before = self.pose;
        let dt = (now - self.last_ms.replace(now).unwrap_or(now)).max(0.) as f32;
        let t = if self.decay == 0. {
            1.
        } else {
            1. - (-3. * dt / self.decay).exp()
        };
        let blend = |a: f32, b: f32| a + (b - a) * t;
        self.pose = Pose {
            theta: blend(self.pose.theta, self.goal.theta),
            phi: blend(self.pose.phi, self.goal.phi),
            radius: blend(self.pose.radius, self.goal.radius),
            target: std::array::from_fn(|k| blend(self.pose.target[k], self.goal.target[k])),
            fov: blend(self.pose.fov, self.goal.fov),
        };
        let moving = self.moving();
        if !moving {
            self.pose = self.goal;
        }
        if self.pose.target != before.target || self.pose.fov != before.fov {
            self.framing = Framing::new(scene, self.pose.target, self.pose.fov);
        }
        moving
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.goal.theta -= dx;
        self.goal.phi = (self.goal.phi - dy).clamp(0., std::f32::consts::PI);
        if self.decay == 0. {
            self.pose = self.goal;
        }
    }
    fn framing_at(&self, scene: &Scene, pose: Pose) -> Framing {
        if pose.target == self.pose.target && pose.fov == self.pose.fov {
            self.framing
        } else {
            Framing::new(scene, pose.target, pose.fov)
        }
    }
    pub fn zoom(&mut self, scene: &Scene, factor: f32) {
        self.radius(scene, self.goal.radius * factor);
    }
    pub fn radius(&mut self, scene: &Scene, radius: f32) {
        let framing = self.framing_at(scene, self.goal);
        self.goal.radius = radius.clamp(0.15 * framing.radius, 12. * framing.radius);
        if self.decay == 0. {
            self.pose = self.goal;
            self.framing = framing;
        }
    }
    pub fn pan(&mut self, scene: &Scene, dx: f32, dy: f32, size: [f32; 2]) {
        let (right, up, _) = self.goal.basis();
        let framing = self.framing_at(scene, self.goal);
        let fov = framing.vertical_field_of_view(size[0] / size[1]);
        let unit = 2. * self.goal.radius * (fov * 0.5).tan() / size[1].max(1.);
        self.goal.target = add(
            self.goal.target,
            add(mul(right, -dx * unit), mul(up, dy * unit)),
        );
        self.zoom(scene, 1.);
    }
    pub fn vertical_fov(&self, aspect: f32) -> f32 {
        self.framing.vertical_field_of_view(aspect)
    }
    pub fn matrix(&self, aspect: f32) -> M4 {
        let [near, far] = self.depth();
        mat_mul(
            perspective(self.vertical_fov(aspect), aspect, near, far),
            self.pose.view(),
        )
    }
    fn depth(&self) -> [f32; 2] {
        [
            (self.pose.radius * 0.001).max(1e-8),
            self.pose.radius + self.framing.radius * 20.,
        ]
    }
    pub fn ray_depth(&self, direction: V3) -> [f32; 2] {
        let forward = -dot(direction, self.pose.basis().2);
        self.depth().map(|depth| depth / forward)
    }
}
