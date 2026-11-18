use exact_game::*;

pub const SEED: u32 = 0x93a3_5e71;
pub const SIDE: i32 = 16;
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
    const ID: &'static str = "terrain-fixture";
    type Args = ();

    fn setup(world: &mut World, _: &()) {
        let pose = Transform::at(EYE.x, EYE.y, EYE.z).looking_at(TARGET, Vec3::Y);
        world.spawn_named("camera", (pose, CAMERA));
    }

    fn tick(_: &mut World, _: &Input, _: &()) {}

    fn quality(_: &()) -> Quality {
        Quality::High
    }

    fn samples(_: &()) -> SceneSamples {
        SceneSamples::One
    }
}

pub fn sections() -> impl Iterator<Item = [i32; 3]> {
    (-RADIUS..=RADIUS).flat_map(|z| {
        (-RADIUS..=RADIUS)
            .flat_map(move |x| (0..LEVELS).map(move |y| [x * SIDE, y * SIDE, z * SIDE]))
    })
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
