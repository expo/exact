use exact_game::*;

mod options;
pub use options::{
    camera_chunk, edit, edit_count, Blend, Budget, Inflight, Motion, Options, Window,
    WorkloadSettings,
};

pub const SEED: u32 = 0x93a3_5e71;
pub const SIDE: i32 = 16;
pub const MAX_LIGHT_CELLS: u32 = ((SIDE as u32) + 2 * 15).pow(3);
pub const RADIUS: i32 = 8;
pub const LEVELS: i32 = 8;
pub const SECTION_COUNT: usize = ((2 * RADIUS + 1).pow(2) * LEVELS) as usize;
pub const VIEWPORT: [u32; 2] = [1280, 720];
pub const SECTIONS_DRAWN: usize = 823;
pub const EYE: Vec3 = Vec3::new(23., 95., 120.);
pub const TARGET: Vec3 = Vec3::new(-23., 32., -34.);
pub const CAMERA: Camera = Camera {
    projection: Projection::Perspective,
    fov_y_degrees: 58.,
    near: 1.,
    far: 231.,
    active: true,
    shift: Vec2::ZERO,
};

pub struct TerrainFixture;

impl Game for TerrainFixture {
    const ID: &'static str = "terrain-fixture-workload";
    type Args = Options;

    fn register(world: &mut World, _: &std::collections::BTreeMap<&str, Value>) {
        world.register::<WorkloadSettings>();
    }

    fn setup(world: &mut World, _: &Options) {
        let pose = Transform::at(EYE.x, EYE.y, EYE.z).looking_at(TARGET, Vec3::Y);
        world.spawn_named("camera", (pose, CAMERA));
    }

    fn present(present: &mut Present<'_>, args: &Options) {
        let settings = WorkloadSettings::from(args);
        if settings != WorkloadSettings::default() {
            if let Some(camera) = present.named("camera") {
                present.insert(camera, settings);
            }
        }
    }

    fn tick(world: &mut World, _: &Input, args: &Options) {
        let motion = args.motion();
        if motion != Motion::Stand {
            let Now { tick, hz } = world.tick_end();
            if let Some(mut camera) = world.get_mut::<Transform>("camera") {
                camera.position = motion.eye(tick, hz);
            }
        }
    }

    fn restorable(saved: &World) -> Result<(), String> {
        if saved.get::<Transform>("camera").is_none() {
            return Err("terrain fixture save has no camera transform".into());
        }
        Ok(())
    }

    fn quality(args: &Options) -> Quality {
        args.quality()
    }

    fn max_fps(_: &World, args: &Options) -> u32 {
        args.max_fps()
    }

    fn samples(_: &Options) -> SceneSamples {
        SceneSamples::One
    }
}

pub fn sections() -> impl Iterator<Item = [i32; 3]> {
    Window::SMOKE.sections()
}

pub fn height(x: i32, z: i32) -> i32 {
    fn lattice(x: i32, z: i32) -> i32 {
        let n = SEED ^ (x as u32).wrapping_mul(0x9e37_79b9) ^ (z as u32).wrapping_mul(0x85eb_ca6b);
        let n = (n ^ (n >> 16)).wrapping_mul(0x7feb_352d);
        12 + ((n ^ (n >> 15)) % 94) as i32
    }

    let x0 = x.div_euclid(SIDE);
    let z0 = z.div_euclid(SIDE);
    let u = x.rem_euclid(SIDE);
    let v = z.rem_euclid(SIDE);
    let near = lattice(x0, z0) * (SIDE - u) + lattice(x0 + 1, z0) * u;
    let far = lattice(x0, z0 + 1) * (SIDE - u) + lattice(x0 + 1, z0 + 1) * u;
    (near * (SIDE - v) + far * v) / (SIDE * SIDE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_default_budget_is_deadline() {
        assert_eq!(
            WorkloadSettings::from(&Options::default()).budget,
            Budget::Deadline
        );
    }

    #[test]
    fn decision_moving_tick_tolerates_missing_camera() {
        let args = Options::moving_for_test();
        let mut sim = Sim::<TerrainFixture>::new(args).unwrap();
        let camera = sim.world().named("camera").unwrap();
        sim.world_mut().despawn(camera);
        sim.run(1000.);
    }

    #[test]
    fn decision_bound_restore_uses_canvas_arguments_and_rejects_preargument_saves() {
        struct PreArguments;
        impl Game for PreArguments {
            type Args = ();
            const ID: &'static str = "terrain-fixture";

            fn setup(world: &mut World, _: &()) {
                TerrainFixture::setup(world, &Options::default());
            }
            fn tick(_: &mut World, _: &Input, _: &()) {}
        }

        let pre_arguments = Sim::<PreArguments>::new(()).unwrap().save().unwrap();
        let mut moving = Sim::<TerrainFixture>::new(Options::moving_for_test()).unwrap();
        let error = moving.restore_bound(&pre_arguments).unwrap_err();
        assert!(error
            .to_string()
            .contains("save belongs to `terrain-fixture`"));

        let mut standing = Sim::<TerrainFixture>::new(Options::default()).unwrap();
        standing.run(1000.);
        moving.restore_bound(&standing.save().unwrap()).unwrap();
        moving.run(1000.);
        let eye = moving.world().get::<Transform>("camera").unwrap().position;
        assert_eq!(eye, Motion::Fly.eye(120, TerrainFixture::HZ));
    }

    #[test]
    fn smoke_arguments_keep_the_pinned_tick_hash() {
        let mut sim = Sim::<TerrainFixture>::new(Options::default()).unwrap();
        sim.viewport(VIEWPORT[0] as f32, VIEWPORT[1] as f32);
        sim.run(1000.);
        sim.assert_pin(include_str!("../../pins.json"));
    }

    #[test]
    fn decision_window_sizes_and_centers() {
        for (radius, count) in [(8, 2312), (40, 52488)] {
            let window = Window {
                center: [1, 7],
                radius,
            };
            let origins: Vec<_> = window.sections().collect();
            assert_eq!(origins.len(), count);
            assert_eq!(origins[0], [(1 - radius) * SIDE, 0, (7 - radius) * SIDE]);
            assert_eq!(
                origins.last(),
                Some(&[(1 + radius) * SIDE, 112, (7 + radius) * SIDE])
            );
        }
    }

    #[test]
    fn decision_one_chunk_move_adds_one_column_of_sections() {
        let previous = Window {
            center: [1, 7],
            radius: 40,
        };
        let window = Window {
            center: [2, 7],
            ..previous
        };
        let entering: Vec<_> = window.entering(Some(previous)).collect();
        assert_eq!(entering.len(), 81 * 8);
        assert!(entering
            .iter()
            .all(|&origin| window.contains(origin) && !previous.contains(origin)));
        assert_eq!(
            previous
                .sections()
                .filter(|&origin| !window.contains(origin))
                .count(),
            entering.len()
        );
    }

    #[test]
    fn decision_camera_is_a_function_of_tick() {
        for (motion, distance, chunk) in [
            (Motion::Stand, 0., [1, 7]),
            (Motion::Walk, 8., [1, 7]),
            (Motion::Fly, 40., [3, 7]),
        ] {
            let eye = motion.eye(120, 60);
            assert_eq!(eye, EYE + Vec3::new(distance, 0., 0.));
            assert_eq!(camera_chunk(eye), chunk);
        }
    }

    #[test]
    fn decision_edits_are_seeded_and_rate_limited_by_tick() {
        let window = Window {
            center: [1, 7],
            radius: 8,
        };
        let sequence = |seed| {
            let mut edits = Vec::new();
            for tick in 1..=60 {
                let range = edit_count(tick - 1, 7, 60)..edit_count(tick, 7, 60);
                edits.extend(range.map(|index| edit(seed, tick, index, window)));
            }
            edits
        };
        let a = sequence(SEED);
        assert_eq!(a.len(), 7);
        assert_eq!(a, sequence(SEED));
        assert_ne!(a, sequence(SEED + 1));
        assert!(a.iter().all(|edit| window.contains(edit.origin)));
    }

    fn occupied([x, y, z]: [i32; 3]) -> bool {
        (z..z + SIDE).any(|z| {
            (x..x + SIDE).any(|x| {
                let highest_solid = height(x, z) - 1;
                let lowest_face = [
                    highest_solid,
                    height(x - 1, z),
                    height(x + 1, z),
                    height(x, z - 1),
                    height(x, z + 1),
                ]
                .into_iter()
                .min()
                .unwrap();
                highest_solid >= y && lowest_face < y + SIDE
            })
        })
    }

    #[test]
    fn occupied_sections_in_frustum_match_pin() {
        let aspect = f64::from(VIEWPORT[0]) / f64::from(VIEWPORT[1]);
        let tangent = (f64::from(CAMERA.fov_y_degrees).to_radians() / 2.).tan();
        let forward = (TARGET - EYE).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        let visible = |[x, y, z]: [i32; 3]| {
            let mut excluded = [true; 6];
            for dx in [0, SIDE] {
                for dy in [0, SIDE] {
                    for dz in [0, SIDE] {
                        let offset =
                            Vec3::new((x + dx) as f32, (y + dy) as f32, (z + dz) as f32) - EYE;
                        let depth = f64::from(offset.dot(forward));
                        let across = f64::from(offset.dot(right));
                        let high = f64::from(offset.dot(up));
                        let beyond = [
                            depth < f64::from(CAMERA.near),
                            depth > f64::from(CAMERA.far),
                            across < -depth * tangent * aspect,
                            across > depth * tangent * aspect,
                            high < -depth * tangent,
                            high > depth * tangent,
                        ];
                        for (all, corner) in excluded.iter_mut().zip(beyond) {
                            *all &= corner;
                        }
                    }
                }
            }
            !excluded.into_iter().any(|all| all)
        };
        assert_eq!(sections().count(), 2312);
        let count = sections()
            .filter(|&section| occupied(section) && visible(section))
            .count();
        assert_eq!(count, SECTIONS_DRAWN);
    }
}
