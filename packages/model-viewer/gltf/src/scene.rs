use crate::math::{M4, V3};
use std::ops::Range;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub p: V3,
    pub n: V3,
    pub node: u32,
    pub primitive: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlphaMode {
    Opaque,
    Mask,
    Blend,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// Linear glTF baseColorFactor, including alpha even for OPAQUE and MASK.
    pub color: [f32; 4],
    pub alpha_mode: AlphaMode,
    pub alpha_cutoff: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            color: [0.82, 0.82, 0.80, 1.0],
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Primitive {
    pub node: usize,
    pub indices: Range<u32>,
    pub material: Material,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: V3,
    pub max: V3,
}

impl Bounds {
    pub fn point(p: V3) -> Self {
        Self { min: p, max: p }
    }

    pub fn include(&mut self, other: Self) {
        for k in 0..3 {
            self.min[k] = self.min[k].min(other.min[k]);
            self.max[k] = self.max[k].max(other.max[k]);
        }
    }

    pub fn centre(&self) -> V3 {
        std::array::from_fn(|k| self.min[k] * 0.5 + self.max[k] * 0.5)
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    /// File order is preserved, including nodes outside the default scene.
    pub name: String,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub active: bool,
    pub world: M4,
    pub indices: Range<u32>,
    pub primitives: Range<usize>,
    pub bounds: Option<Bounds>,
    pub subtree_bounds: Option<Bounds>,
}

#[derive(Debug)]
pub struct Scene {
    pub nodes: Vec<Node>,
    pub primitives: Vec<Primitive>,
    /// Accessor storage may include unused positions; triangle indices define the geometry.
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub bounds: Option<Bounds>,
    pub centre: V3,
}

impl Scene {
    pub fn find(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }

    pub fn triangles(&self) -> usize {
        self.indices.len() / 3
    }
}
