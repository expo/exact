use crate::Shape;
use exact_game::{Affine3A, Entity, Parent, Transform, Vec3, World};
use rapier3d::{
    math::{Pose, Rotation, Vector},
    parry::{
        mass_properties::MassProperties,
        shape::{HeightFieldFlags, SharedShape, TriMeshFlags},
        utils::Array2,
    },
};

// All engine ↔ executor math crosses this module. Both currently use glam 0.33.
pub(crate) fn vector(v: Vec3) -> Vector {
    Vector::from_array(v.to_array())
}
pub(crate) fn vec3(v: Vector) -> Vec3 {
    Vec3::from_array(v.to_array())
}
pub(crate) fn pose(t: Transform) -> Pose {
    try_pose(t).unwrap_or_else(|e| panic!("{e}"))
}
// Each fallible conversion names what it refuses; the panicking forms are for
// game-supplied values, the fallible ones for values read from a save.
pub(crate) fn try_pose(t: Transform) -> Result<Pose, &'static str> {
    check(
        t.position.is_finite()
            && t.rotation.is_finite()
            && (t.rotation.length_squared() - 1.0).abs() < 1e-3,
        "physics: invalid pose",
    )?;
    Ok(Pose::from_parts(
        vector(t.position),
        Rotation::from_array(t.rotation.to_array()),
    ))
}
pub(crate) fn check(ok: bool, refusal: &'static str) -> Result<(), &'static str> {
    if ok {
        Ok(())
    } else {
        Err(refusal)
    }
}
pub(crate) fn transform(p: &Pose, scale: Vec3) -> Transform {
    Transform {
        position: vec3(p.translation),
        rotation: exact_game::Quat::from_array(p.rotation.to_array()),
        scale,
    }
}
pub(crate) fn shape(s: &Shape, scale: Vec3) -> SharedShape {
    try_shape(s, scale).unwrap_or_else(|e| panic!("{e}"))
}
pub(crate) fn try_shape(s: &Shape, scale: Vec3) -> Result<SharedShape, &'static str> {
    check(
        scale.is_finite() && scale.min_element() > 0.0,
        "physics: invalid scale",
    )?;
    let curved = |radius: f32, height: f32| {
        check(
            radius.is_finite()
                && radius > 0.0
                && height.is_finite()
                && height > 0.0
                && scale.x == scale.y
                && scale.y == scale.z,
            "physics: curved shapes require positive dimensions and uniform scale",
        )
    };
    Ok(match s {
        Shape::Sphere { radius } => {
            curved(*radius, 1.0)?;
            SharedShape::ball(radius * scale.x)
        }
        Shape::Capsule { radius, height } => {
            curved(*radius, *height)?;
            check(
                *height >= 2.0 * radius,
                "physics: capsule height must be at least twice its radius",
            )?;
            SharedShape::capsule_y((height * 0.5 - radius) * scale.y, radius * scale.x)
        }
        Shape::Cylinder { radius, height } => {
            curved(*radius, *height)?;
            SharedShape::cylinder(height * 0.5 * scale.y, radius * scale.x)
        }
        Shape::Box { half } => {
            check(
                half.is_finite() && half.min_element() > 0.0,
                "physics: invalid box",
            )?;
            let h = *half * scale;
            SharedShape::cuboid(h.x, h.y, h.z)
        }
        Shape::Heightfield {
            rows,
            cols,
            heights,
            scale: extents,
        } => {
            check(
                *rows >= 2
                    && *cols >= 2
                    && u64::from(*rows) * u64::from(*cols) == heights.len() as u64
                    && heights.iter().all(|x| x.is_finite())
                    && extents.is_finite()
                    && extents.min_element() > 0.0,
                "physics: invalid heightfield",
            )?;
            let samples = Array2::from_fn(*rows as usize, *cols as usize, |r, c| {
                heights[r * *cols as usize + c]
            });
            SharedShape::heightfield_with_flags(
                samples,
                vector(*extents * scale),
                HeightFieldFlags::FIX_INTERNAL_EDGES,
            )
        }
        Shape::Mesh { vertices, indices } => {
            check(
                vertices.iter().all(|v| v.is_finite())
                    && indices
                        .iter()
                        .flatten()
                        .all(|i| (*i as usize) < vertices.len()),
                "physics: invalid mesh",
            )?;
            SharedShape::trimesh_with_flags(
                vertices.iter().map(|v| vector(*v * scale)).collect(),
                indices.clone(),
                TriMeshFlags::FIX_INTERNAL_EDGES,
            )
            .map_err(|_| "physics: invalid mesh")?
        }
        Shape::ConvexHull { points, round } => {
            check(
                (4..=MAX_HULL_POINTS).contains(&points.len())
                    && points.iter().all(|p| p.is_finite())
                    && round.is_finite()
                    && *round >= 0.0,
                "physics: invalid convex hull",
            )?;
            check(
                *round == 0.0 || (scale.x == scale.y && scale.y == scale.z),
                "physics: a rounded convex hull requires uniform scale",
            )?;
            let points: Vec<Vector> = points.iter().map(|p| vector(*p * scale)).collect();
            let flat = "physics: convex hull points lie in one plane";
            check(solid(&points), flat)?;
            let hull = if *round > 0.0 {
                SharedShape::round_convex_hull(&points, round * scale.x)
            } else {
                SharedShape::convex_hull(&points)
            }
            .ok_or("physics: degenerate convex hull")?;
            // Parry flattens a hull thinner than its own tolerance into one without
            // volume, which a dynamic body would carry as infinite mass.
            let mass = hull.mass_properties(1.0).mass();
            check(mass.is_finite() && mass > 0.0, flat)?;
            hull
        }
    })
    .and_then(|shape| {
        let aabb = shape.compute_local_aabb();
        check(
            aabb.mins.is_finite() && aabb.maxs.is_finite(),
            "physics: shape extent overflows",
        )?;
        Ok(shape)
    })
}
// The mass properties of a rounded hull's solid, the hull swept by a ball, at a
// density; None for any other shape. Parry gives a rounded shape those of its
// inner shape. The solid is approximated from inside by the hull of the inner
// hull's points, each face pushed out along its normal, and the inner hull's
// support points in each of 386 fixed directions pushed out along that
// direction: only rounded edges and corners fall short, by a few percent of the
// radius. Every support point of a tied direction is pushed, so a symmetric
// hull keeps its symmetry.
pub(crate) fn rounded_hull_mass(shape: &SharedShape, density: f32) -> Option<MassProperties> {
    let round = shape.as_round_convex_polyhedron()?;
    let (hull, r) = (&round.inner_shape, round.border_radius);
    let vertices = hull.points();
    let mut points = vertices.to_vec();
    let adjacent = hull.vertices_adj_to_face();
    for face in hull.faces() {
        let first = face.first_vertex_or_edge as usize;
        let ids = &adjacent[first..first + face.num_vertices_or_edges as usize];
        points.extend(ids.iter().map(|&i| vertices[i as usize] + face.normal * r));
    }
    // The integer points on the surface of the cube [-4, 4]³, as directions.
    for x in -4i8..=4 {
        for y in -4i8..=4 {
            for z in -4i8..=4 {
                if x.abs().max(y.abs()).max(z.abs()) < 4 {
                    continue;
                }
                let d = Vector::new(x.into(), y.into(), z.into()).normalize();
                let top = vertices.iter().map(|v| v.dot(d)).fold(f32::MIN, f32::max);
                let support = vertices.iter().filter(|v| v.dot(d) == top);
                points.extend(support.map(|v| *v + d * r));
            }
        }
    }
    Some(SharedShape::convex_hull(&points)?.mass_properties(density))
}
// PhysX's hard limit on a convex mesh's vertices (`PxConvexMeshDesc::vertexLimit`),
// applied to the input points. Contact queries walk every hull vertex.
const MAX_HULL_POINTS: usize = 255;
// Whether the points span a solid: some point stands off the plane through three
// spread-out points by more than 1e-4 of their spread. Parry's hull builder accepts
// flat input and returns a hull without volume, hence mass; this refuses most such
// input before building, and the mass check after building refuses the rest.
fn solid(points: &[Vector]) -> bool {
    let a = points[0];
    let farthest = |key: &dyn Fn(Vector) -> f32| {
        points.iter().copied().fold((a, 0.0f32), |best, p| {
            let k = key(p);
            if k > best.1 {
                (p, k)
            } else {
                best
            }
        })
    };
    let (b, ab) = farthest(&|p| (p - a).length());
    let (c, _) = farthest(&|p| (p - a).cross(b - a).length());
    let n = (b - a).cross(c - a);
    let (_, height) = farthest(&|p| (p - a).dot(n).abs());
    let length = n.length();
    ab > 0.0 && length > 0.0 && height / length > 1e-4 * ab
}
// Read current component writes, even before the engine propagates hierarchy.
pub(crate) fn world_pose(world: &World, entity: Entity) -> Transform {
    let t = world
        .get::<Transform>(entity)
        .map(|p| *p)
        .unwrap_or_default();
    if !world.has::<Parent>(entity) {
        return t;
    }
    let mut affine = Affine3A::from_scale_rotation_translation(t.scale, t.rotation, t.position);
    let mut e = entity;
    for _ in 0..world.len() {
        let Some(parent) = world.get::<Parent>(e) else {
            break;
        };
        if !world.contains(parent.0) {
            break;
        }
        e = parent.0;
        assert_ne!(e, entity, "physics: transform cycle");
        let p = world.get::<Transform>(e).map(|p| *p).unwrap_or_default();
        affine =
            Affine3A::from_scale_rotation_translation(p.scale, p.rotation, p.position) * affine;
    }
    let (scale, rotation, position) = affine.to_scale_rotation_translation();
    let rebuilt = Affine3A::from_scale_rotation_translation(scale, rotation, position);
    assert!(
        (affine.matrix3.x_axis - rebuilt.matrix3.x_axis).length() < 1e-4
            && (affine.matrix3.y_axis - rebuilt.matrix3.y_axis).length() < 1e-4
            && (affine.matrix3.z_axis - rebuilt.matrix3.z_axis).length() < 1e-4,
        "physics: sheared collider"
    );
    Transform {
        position,
        rotation,
        scale,
    }
}

pub(crate) fn heightfield(
    rows: u32,
    cols: u32,
    heights: Vec<f32>,
    scale: Vec3,
) -> Result<(exact_game::asset::MeshData, Shape), String> {
    if rows < 2
        || cols < 2
        || u64::from(rows) * u64::from(cols) != heights.len() as u64
        || heights.iter().any(|h| !h.is_finite())
        || !scale.is_finite()
        || scale.min_element() <= 0.
    {
        return Err("heightfield: invalid rows, cols, heights or scale".into());
    }
    let mut mesh = exact_game::asset::MeshData::default();
    let dx = 1. / (cols - 1) as f32;
    let dz = 1. / (rows - 1) as f32;
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for r in 0..rows {
        for c in 0..cols {
            let p = Vec3::new(
                -0.5 + dx * c as f32,
                heights[(r * cols + c) as usize],
                -0.5 + dz * r as f32,
            ) * scale;
            if !p.is_finite() {
                return Err("heightfield: scaled vertex is not finite".into());
            }
            mesh.positions.extend(p.to_array());
            mesh.uvs.extend([dx * c as f32, dz * r as f32]);
            lo = lo.min(p);
            hi = hi.max(p);
            if r + 1 < rows && c + 1 < cols {
                let a = r * cols + c;
                mesh.indices
                    .extend([a, a + cols, a + 1, a + cols, a + cols + 1, a + 1]);
            }
        }
    }
    let mut normals = vec![Vec3::ZERO; heights.len()];
    for t in mesh.indices.as_chunks::<3>().0 {
        let p = |i: u32| Vec3::from_slice(&mesh.positions[i as usize * 3..][..3]);
        let n = (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0]));
        for &i in t {
            normals[i as usize] += n;
        }
    }
    mesh.normals = normals
        .into_iter()
        .flat_map(|n| n.try_normalize().unwrap_or(Vec3::Y).to_array())
        .collect();
    mesh.bounds = [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z];
    Ok((
        mesh,
        Shape::Heightfield {
            rows,
            cols,
            heights,
            scale,
        },
    ))
}
