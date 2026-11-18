use crate::{EYE, LEVELS, MAX_LIGHT_CELLS, RADIUS, SEED, SIDE};
use exact_game::*;

#[derive(Args, Debug)]
pub struct Options {
    radius: String,
    r#move: String,
    light: String,
    edits: String,
    inflight: String,
    compare: String,
    vertex: String,
    blend: String,
    budget: String,
    draw: String,
    pool: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            radius: "smoke".into(),
            r#move: "stand".into(),
            light: "0".into(),
            edits: "0".into(),
            inflight: "discard".into(),
            compare: "off".into(),
            vertex: "12".into(),
            blend: "off".into(),
            budget: "deadline".into(),
            draw: "on".into(),
            pool: "off".into(),
        }
    }
}

impl Options {
    pub(crate) fn motion(&self) -> Motion {
        match self.r#move.as_str() {
            "walk" => Motion::Walk,
            "fly" => Motion::Fly,
            _ => Motion::Stand,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub enum Motion {
    #[default]
    Stand,
    Walk,
    Fly,
}

impl Motion {
    fn speed(self) -> f32 {
        match self {
            Self::Stand => 0.,
            Self::Walk => 4.,
            Self::Fly => 20.,
        }
    }

    pub fn crossing_interval_ms(self) -> Option<f64> {
        (self != Self::Stand).then(|| 1000. * f64::from(SIDE) / f64::from(self.speed()))
    }

    pub fn eye(self, tick: u64, hz: u32) -> Vec3 {
        EYE + Vec3::new(self.speed() * (tick as f32 / hz as f32), 0., 0.)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub enum Inflight {
    #[default]
    Discard,
    Keep,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub enum Blend {
    #[default]
    Off,
    Background,
    Surface,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Data)]
pub enum Budget {
    Count(u32),
    #[default]
    Deadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Presentation)]
pub struct WorkloadSettings {
    pub radius: i32,
    pub motion: Motion,
    pub light: u32,
    pub edits: u32,
    pub inflight: Inflight,
    pub compare: bool,
    pub vertex: u32,
    pub blend: Blend,
    pub budget: Budget,
    pub draw: bool,
    pub pool: bool,
}

impl Default for WorkloadSettings {
    fn default() -> Self {
        Self {
            radius: 0,
            motion: Motion::Stand,
            light: 0,
            edits: 0,
            inflight: Inflight::Discard,
            compare: false,
            vertex: 12,
            blend: Blend::Off,
            budget: Budget::default(),
            draw: true,
            pool: false,
        }
    }
}

impl From<&Options> for WorkloadSettings {
    fn from(args: &Options) -> Self {
        Self {
            radius: match args.radius.as_str() {
                "8" => 8,
                "40" => 40,
                _ => 0,
            },
            motion: args.motion(),
            light: args.light.parse::<u32>().unwrap_or(0).min(MAX_LIGHT_CELLS),
            edits: args.edits.parse().unwrap_or(0),
            inflight: if args.inflight == "keep" {
                Inflight::Keep
            } else {
                Inflight::Discard
            },
            compare: args.compare == "on",
            vertex: if args.vertex == "64" { 64 } else { 12 },
            blend: match args.blend.as_str() {
                "window" | "background" => Blend::Background,
                "surface" => Blend::Surface,
                _ => Blend::Off,
            },
            budget: args
                .budget
                .strip_prefix("count:")
                .and_then(|count| count.parse::<u32>().ok())
                .filter(|&count| count > 0)
                .map_or(Budget::Deadline, Budget::Count),
            draw: args.draw != "off",
            pool: args.pool == "on",
        }
    }
}

impl WorkloadSettings {
    pub fn window(self, tick: u64, hz: u32) -> Window {
        if self.radius == 0 && self.motion == Motion::Stand {
            return Window::SMOKE;
        }
        Window {
            center: camera_chunk(self.motion.eye(tick, hz)),
            radius: if self.radius == 0 {
                RADIUS
            } else {
                self.radius
            },
        }
    }
}

pub fn camera_chunk(eye: Vec3) -> [i32; 2] {
    [eye.x, eye.z].map(|axis| (axis.floor() as i32).div_euclid(SIDE))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub center: [i32; 2],
    pub radius: i32,
}

impl Window {
    pub const SMOKE: Self = Self {
        center: [0, 0],
        radius: RADIUS,
    };

    pub fn contains(self, [x, _, z]: [i32; 3]) -> bool {
        (x.div_euclid(SIDE) - self.center[0]).abs() <= self.radius
            && (z.div_euclid(SIDE) - self.center[1]).abs() <= self.radius
    }

    pub fn sections(self) -> impl Iterator<Item = [i32; 3]> {
        self.entering(None)
    }

    pub fn entering(self, previous: Option<Self>) -> impl Iterator<Item = [i32; 3]> {
        let [cx, cz] = self.center;
        let r = self.radius;
        (cz - r..=cz + r).flat_map(move |z| {
            (cx - r..=cx + r)
                .filter(move |&x| !previous.is_some_and(|w| w.contains([x * SIDE, 0, z * SIDE])))
                .flat_map(move |x| (0..LEVELS).map(move |y| [x * SIDE, y * SIDE, z * SIDE]))
        })
    }

    fn address(self, index: u64) -> [i32; 3] {
        let width = (2 * self.radius + 1) as u64;
        let [cx, cz] = self.center;
        [
            (cx - self.radius + (index / LEVELS as u64 % width) as i32) * SIDE,
            (index % LEVELS as u64) as i32 * SIDE,
            (cz - self.radius + (index / LEVELS as u64 / width % width) as i32) * SIDE,
        ]
    }
}

pub fn edit_count(tick: u64, rate: u32, hz: u32) -> u64 {
    (u128::from(tick) * u128::from(rate) / u128::from(hz)) as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edit {
    pub origin: [i32; 3],
    pub changes_input: bool,
}

pub fn edit(seed: u32, tick: u64, index: u64, window: Window) -> Edit {
    let n = index ^ tick.rotate_left(32) ^ u64::from(seed);
    let n = (n ^ (n >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    let n = (n ^ (n >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb) ^ u64::from(SEED);
    Edit {
        origin: window.address(n ^ (n >> 31)),
        changes_input: n & 1 == 1,
    }
}

#[cfg(test)]
impl Options {
    pub(crate) fn moving_for_test() -> Self {
        Self {
            r#move: "fly".into(),
            ..Self::default()
        }
    }
}
