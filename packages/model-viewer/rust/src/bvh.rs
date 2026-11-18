//! Triangle bounds for each glTF primitive, built alongside parsing (LLP 1133.039 D11).
use exact_model_gltf::{
    math::*,
    scene::{Bounds, Scene},
};

#[derive(Default)]
pub struct Bvh {
    meshes: Vec<Mesh>,
}
struct Mesh {
    branches: Vec<Branch>,
    triangles: Vec<u32>,
}
#[derive(Clone, Copy)]
struct Branch {
    bounds: Bounds,
    start: u32,
    count: u32,
}
struct Triangle {
    bounds: Bounds,
    offset: u32,
}

#[derive(Clone, Copy)]
pub struct Ray {
    pub origin: V3,
    pub direction: V3,
    pub depth: [f32; 2],
    pub clip: crate::pick::Clip,
}
impl Ray {
    fn crosses(self, bounds: Bounds) -> bool {
        let [mut near, mut far] = self.depth;
        for k in 0..3 {
            // Edge hits can round outside exact bounds in the slab and triangle tests.
            let error = 4.
                * f32::EPSILON
                * self.origin[k]
                    .abs()
                    .max(bounds.min[k].abs())
                    .max(bounds.max[k].abs());
            let min = bounds.min[k] - error;
            let max = bounds.max[k] + error;
            if self.direction[k] == 0. {
                if self.origin[k] < min || self.origin[k] > max {
                    return false;
                }
            } else {
                let a = (min - self.origin[k]) / self.direction[k];
                let b = (max - self.origin[k]) / self.direction[k];
                near = near.max(a.min(b));
                far = far.min(a.max(b));
                if near > far {
                    return false;
                }
            }
        }
        true
    }
}

impl Bvh {
    pub fn new(scene: &Scene) -> Self {
        let meshes = scene
            .primitives
            .iter()
            .map(|p| {
                let mut triangles: Vec<_> = (p.indices.start..p.indices.end)
                    .step_by(3)
                    .map(|offset| {
                        let mut bounds = Bounds::point(
                            scene.vertices[scene.indices[offset as usize] as usize].p,
                        );
                        for k in 1..3 {
                            bounds.include(Bounds::point(
                                scene.vertices[scene.indices[offset as usize + k] as usize].p,
                            ));
                        }
                        Triangle { bounds, offset }
                    })
                    .collect();
                let mut mesh = Mesh {
                    branches: vec![],
                    triangles: vec![],
                };
                if !triangles.is_empty() {
                    mesh.branches.push(Branch {
                        bounds: triangles[0].bounds,
                        start: 0,
                        count: 0,
                    });
                    mesh.build(&mut triangles, 0, 0);
                }
                mesh.triangles = triangles.iter().map(|t| t.offset).collect();
                // Retain only the finished tree, not the partitioner's spare capacity.
                mesh.branches.shrink_to_fit();
                mesh
            })
            .collect();
        Self { meshes }
    }

    /// Return early when the visitor accepts a hit. Offsets retain file-order tie breaking.
    pub fn hits(
        &self,
        scene: &Scene,
        primitive: usize,
        ray: Ray,
        mut visit: impl FnMut(f32, u32, bool) -> bool,
    ) -> bool {
        let mesh = &self.meshes[primitive];
        !mesh.branches.is_empty() && mesh.hits(scene, 0, ray, &mut visit)
    }

    pub fn bytes(&self) -> usize {
        self.meshes.capacity() * std::mem::size_of::<Mesh>()
            + self
                .meshes
                .iter()
                .map(|m| {
                    m.branches.capacity() * std::mem::size_of::<Branch>()
                        + m.triangles.capacity() * std::mem::size_of::<u32>()
                })
                .sum::<usize>()
    }
}
impl Mesh {
    fn build(&mut self, triangles: &mut [Triangle], branch: usize, offset: u32) {
        let mut bounds = triangles[0].bounds;
        for t in &triangles[1..] {
            bounds.include(t.bounds);
        }
        self.branches[branch] = Branch {
            bounds,
            start: offset,
            count: triangles.len() as u32,
        };
        if triangles.len() <= 8 {
            return;
        }
        let axis = (0..3)
            .max_by(|&a, &b| {
                (bounds.max[a] - bounds.min[a]).total_cmp(&(bounds.max[b] - bounds.min[b]))
            })
            .unwrap();
        let middle = triangles.len() / 2;
        triangles.select_nth_unstable_by(middle, |a, b| {
            (a.bounds.min[axis] * 0.5 + a.bounds.max[axis] * 0.5)
                .total_cmp(&(b.bounds.min[axis] * 0.5 + b.bounds.max[axis] * 0.5))
        });
        let children = self.branches.len();
        self.branches.extend([self.branches[branch]; 2]);
        self.branches[branch].start = children as u32;
        self.branches[branch].count = 0;
        let (left, right) = triangles.split_at_mut(middle);
        self.build(left, children, offset);
        self.build(right, children + 1, offset + middle as u32);
    }

    fn hits(
        &self,
        scene: &Scene,
        index: usize,
        ray: Ray,
        visit: &mut impl FnMut(f32, u32, bool) -> bool,
    ) -> bool {
        let branch = self.branches[index];
        if !ray.crosses(branch.bounds) {
            return false;
        }
        if branch.count == 0 {
            return self.hits(scene, branch.start as usize, ray, visit)
                || self.hits(scene, branch.start as usize + 1, ray, visit);
        }
        for &offset in
            &self.triangles[branch.start as usize..(branch.start + branch.count) as usize]
        {
            let [a, b, c] = std::array::from_fn(|k| {
                scene.vertices[scene.indices[offset as usize + k] as usize].p
            });
            if let Some(t) = crate::pick::triangle(ray.origin, ray.direction, a, b, c) {
                if (ray.depth[0]..=ray.depth[1]).contains(&t)
                    && ray.clip.keeps(add(ray.origin, mul(ray.direction, t)))
                    && visit(
                        t,
                        offset,
                        dot(cross(sub(b, a), sub(c, a)), ray.direction) < 0.,
                    )
                {
                    return true;
                }
            }
        }
        false
    }
}
