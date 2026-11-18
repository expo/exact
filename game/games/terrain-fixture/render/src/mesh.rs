use terrain_fixture_logic::SIDE;

const EDGE: usize = SIDE as usize;
const PADDED: usize = EDGE + 2;
const EDIT_CELL: [usize; 3] = [8; 3];
pub(crate) const WATER_Y: i32 = 64;
pub(crate) type WaterQuad = [i32; 4];

fn edited(cell: [usize; 3], changed: bool, pad: usize) -> bool {
    changed && cell == EDIT_CELL.map(|n| n + pad)
}

pub(crate) fn padded_input(
    [x, y, z]: [i32; 3],
    height: impl Fn(i32, i32) -> i32,
    changed: bool,
) -> Box<[u8]> {
    let mut cells = vec![0; 20 * 20 * 20];
    for iz in 0..20 {
        for ix in 0..20 {
            let top = height(x + ix as i32 - 2, z + iz as i32 - 2);
            for iy in 0..20 {
                let solid = y + iy as i32 - 2 < top;
                cells[(iz * 20 + iy) * 20 + ix] =
                    u8::from(solid ^ edited([ix, iy, iz], changed, 2));
            }
        }
    }
    cells.into_boxed_slice()
}

struct Light {
    side: i32,
    low: i32,
    top: i32,
    cells: Vec<u8>,
}

impl Light {
    fn at(&self, [x, y, z]: [i32; 3]) -> u8 {
        let [x, y, z] = [x - self.low, self.top - y, z - self.low];
        if [x, y, z].iter().all(|n| (0..self.side).contains(n)) {
            self.cells
                .get(((y * self.side + z) * self.side + x) as usize)
                .copied()
                .unwrap_or(0)
        } else {
            0
        }
    }
}

fn propagate(steps: u32, solid: impl Fn([i32; 3]) -> bool) -> Light {
    let count = steps.min(terrain_fixture_logic::MAX_LIGHT_CELLS);
    let side = (1..)
        .find(|&side: &i32| side.pow(3) >= count as i32)
        .unwrap();
    let low = (SIDE - side).div_euclid(2);
    let top = low + side - 1;
    let mut light = Light {
        side,
        low,
        top,
        cells: Vec::with_capacity(count as usize),
    };
    // The sweep starts at the sky and propagates through cells already visited.
    for i in 0..count as i32 {
        let x = low + i % side;
        let z = low + i / side % side;
        let y = top - i / (side * side);
        let level = if solid([x, y, z]) {
            0
        } else if y == top {
            15
        } else {
            [
                light.at([x, y + 1, z]),
                light.at([x - 1, y, z]),
                light.at([x, y, z - 1]),
            ]
            .into_iter()
            .max()
            .unwrap()
            .saturating_sub(1)
        };
        light.cells.push(level);
    }
    light
}

#[derive(Default)]
pub(crate) struct Mesh {
    pub(crate) vertices: Vec<u8>,
    pub(crate) indices: Vec<u32>,
}

pub(crate) fn section_with(
    origin: [i32; 3],
    height: impl Fn(i32, i32) -> i32,
    changed: bool,
    stride: u32,
    steps: u32,
) -> Mesh {
    let [x, y, z] = origin;
    let mut heights = [[0; PADDED]; PADDED];
    let (mut lowest, mut highest) = (i32::MAX, i32::MIN);
    for (row, cells) in heights.iter_mut().enumerate() {
        for (col, cell) in cells.iter_mut().enumerate() {
            *cell = height(x + col as i32 - 1, z + row as i32 - 1);
            lowest = lowest.min(*cell);
            highest = highest.max(*cell);
        }
    }

    let mut mesh = Mesh::default();
    let solid = |cell @ [x, dy, z]: [usize; 3]| {
        (y + dy as i32 - 1 < heights[z][x]) ^ edited(cell, changed, 1)
    };
    let light = (steps > 0).then(|| {
        propagate(steps, |[dx, dy, dz]| {
            (y + dy < height(x + dx, z + dz)) ^ (changed && [dx, dy, dz] == [8; 3])
        })
    });
    if !changed && (highest <= y || lowest > y + SIDE) {
        return mesh;
    }

    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for plane in 0..=EDGE {
            let mut mask = [[0i8; EDGE]; EDGE];
            for (row, cells) in mask.iter_mut().enumerate() {
                for (col, cell) in cells.iter_mut().enumerate() {
                    let mut a = [0; 3];
                    a[axis] = plane;
                    a[u] = col + 1;
                    a[v] = row + 1;
                    let mut b = a;
                    b[axis] += 1;
                    // At a section boundary, only the owner of the solid cell emits the face.
                    *cell = match (solid(a), solid(b)) {
                        (true, false) if plane > 0 => 1,
                        (false, true) if plane < EDGE => -1,
                        _ => 0,
                    };
                }
            }

            for row in 0..EDGE {
                for col in 0..EDGE {
                    let face = mask[row][col];
                    if face == 0 {
                        continue;
                    }
                    let width = mask[row][col..].iter().take_while(|&&n| n == face).count();
                    let height = mask[row..]
                        .iter()
                        .take_while(|cells| cells[col..col + width].iter().all(|&n| n == face))
                        .count();
                    for cells in &mut mask[row..row + height] {
                        cells[col..col + width].fill(0);
                    }

                    let first = (mesh.vertices.len() / stride as usize) as u32;
                    for (du, dv) in [(0, 0), (width, 0), (width, height), (0, height)] {
                        let mut p = origin;
                        p[axis] += plane as i32;
                        p[u] += (col + du) as i32;
                        p[v] += (row + dv) as i32;
                        let mut sample = [0; 3];
                        sample[axis] = plane + usize::from(face > 0);
                        sample[u] = col + du.min(width - 1) + 1;
                        sample[v] = row + dv.min(height - 1) + 1;
                        let level = light
                            .as_ref()
                            .map(|light| light.at(sample.map(|v| v as i32 - 1)));
                        let mut normal = [0.; 3];
                        normal[axis] = f32::from(face);
                        vertex(&mut mesh, p, stride, normal, level, [du as f32, dv as f32]);
                    }
                    let winding = if face > 0 {
                        [0, 1, 2, 0, 2, 3]
                    } else {
                        [0, 2, 1, 0, 3, 2]
                    };
                    mesh.indices.extend(winding.map(|i| first + i));
                }
            }
        }
    }
    mesh
}

fn vertex(
    mesh: &mut Mesh,
    position: [i32; 3],
    stride: u32,
    normal: [f32; 3],
    light: Option<u8>,
    uv: [f32; 2],
) {
    let mut xyz = position.map(|n| n as f32);
    // Section heights are integers. The thin lit shader recovers y and its four light bits.
    if let Some(level) = light.filter(|_| stride == 12) {
        xyz[1] += f32::from(level) / 16.;
    }
    mesh.vertices
        .extend(xyz.into_iter().flat_map(f32::to_le_bytes));
    if stride == 64 {
        let level = light.map_or(1., |n| 0.3 + 0.7 * f32::from(n) / 15.);
        mesh.vertices.extend(
            normal
                .into_iter()
                .chain([level])
                .chain(uv)
                .flat_map(f32::to_le_bytes),
        );
        mesh.vertices.extend([0; 28]);
    }
}

pub(crate) fn water_quads(x: i32, z: i32, height: impl Fn(i32, i32) -> i32) -> Vec<WaterQuad> {
    let mut mask = [[false; EDGE]; EDGE];
    for (row, cells) in mask.iter_mut().enumerate() {
        for (col, cell) in cells.iter_mut().enumerate() {
            *cell = height(x + col as i32, z + row as i32) < WATER_Y;
        }
    }
    let mut quads = Vec::new();
    for row in 0..EDGE {
        for col in 0..EDGE {
            let width = mask[row][col..].iter().take_while(|&&wet| wet).count();
            if width == 0 {
                continue;
            }
            let depth = mask[row..]
                .iter()
                .take_while(|cells| cells[col..col + width].iter().all(|&wet| wet))
                .count();
            for cells in &mut mask[row..row + depth] {
                cells[col..col + width].fill(false);
            }
            quads.push([x + col as i32, z + row as i32, width as i32, depth as i32]);
        }
    }
    quads
}

fn distance([x, z, w, d]: WaterQuad, eye: [f32; 3]) -> f32 {
    let dx = x as f32 + w as f32 / 2. - eye[0];
    let dz = z as f32 + d as f32 / 2. - eye[2];
    dx * dx + dz * dz
}

pub(crate) fn sort_water(quads: &mut [WaterQuad], eye: [f32; 3]) {
    quads.sort_unstable_by(|a, b| {
        distance(*b, eye)
            .total_cmp(&distance(*a, eye))
            .then_with(|| a.cmp(b))
    });
}

pub(crate) fn water(quads: &[WaterQuad], stride: u32) -> Mesh {
    let mut mesh = Mesh::default();
    for &[x, z, width, depth] in quads {
        let first = (mesh.vertices.len() / stride as usize) as u32;
        for (dx, dz) in [(0, 0), (0, depth), (width, depth), (width, 0)] {
            vertex(
                &mut mesh,
                [x + dx, WATER_Y, z + dz],
                stride,
                [0., 1., 0.],
                None,
                [dx as f32, dz as f32],
            );
        }
        mesh.indices.extend([0, 1, 2, 0, 2, 3].map(|i| first + i));
    }
    mesh
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_light_visits_distinct_cells_up_to_the_tile_cap() {
        use std::cell::RefCell;
        use std::collections::BTreeSet;

        for requested in [5, 6000, terrain_fixture_logic::MAX_LIGHT_CELLS + 1] {
            let visited = RefCell::new(BTreeSet::new());
            let light = propagate(requested, |cell| {
                assert!(visited.borrow_mut().insert(cell), "{cell:?} visited twice");
                false
            });
            let count = requested.min(terrain_fixture_logic::MAX_LIGHT_CELLS) as usize;
            assert_eq!(visited.into_inner().len(), count);
            assert_eq!(light.cells.len(), count);
        }
    }

    fn section(origin: [i32; 3], height: impl Fn(i32, i32) -> i32) -> Mesh {
        section_with(origin, height, false, 12, 0)
    }

    #[test]
    fn decision_vertex_width_and_light_preserve_indices() {
        let height = |x, _| if x < 8 { 10 } else { 14 };
        let origin = [0, 0, 0];
        let thin = section(origin, height);
        let wide = section_with(origin, height, false, 64, 0);
        let lit = section_with(origin, height, false, 64, 2);
        assert_eq!(wide.vertices.len() / 64, thin.vertices.len() / 12);
        assert_eq!(wide.indices, thin.indices);
        assert_eq!(lit.indices, wide.indices);
        assert!(lit
            .vertices
            .as_chunks::<64>()
            .0
            .iter()
            .zip(wide.vertices.as_chunks::<64>().0)
            .any(|(lit, wide)| lit[24..28] != wide[24..28]));
        let steps = PADDED.pow(3) as u32;
        let lit_thin = section_with(origin, height, false, 12, steps);
        assert_eq!(lit_thin.indices, thin.indices);
        assert_eq!(lit_thin.vertices.len(), thin.vertices.len());
        assert_ne!(lit_thin.vertices, thin.vertices);
        for (lit, unlit) in lit_thin
            .vertices
            .as_chunks::<12>()
            .0
            .iter()
            .zip(thin.vertices.as_chunks::<12>().0)
        {
            let y = |bytes: &[u8]| f32::from_le_bytes(bytes[4..8].try_into().unwrap());
            assert_eq!(y(lit).floor(), y(unlit));
        }
        assert_ne!(
            lit.vertices,
            section_with(origin, height, false, 64, steps).vertices
        );
    }

    #[test]
    fn decision_compare_reads_the_padded_input() {
        let origin = [0, 0, 0];
        let old = padded_input(origin, |_, _| 10, false);
        assert_eq!(old, padded_input(origin, |_, _| 10, false));
        assert_ne!(
            old,
            padded_input(origin, |x, _| if x == -2 { 11 } else { 10 }, false)
        );
        assert_ne!(old, padded_input(origin, |_, _| 10, true));
    }

    #[test]
    fn section_mesh_merges_faces_and_owns_boundary_voxels() {
        let height = |x, _| if x < 16 { 16 } else { 32 };
        let lower = section([0, 0, 0], height);
        let adjacent = section([16, 0, 0], height);
        let upper = section([16, 16, 0], height);

        assert_eq!(lower.indices.len(), 6, "the flat top is one quad");
        assert!(
            adjacent.indices.is_empty(),
            "the internal seam has no faces"
        );
        assert_eq!(
            upper.indices.len(),
            12,
            "the upper section owns one top and one cliff quad"
        );
    }
}
