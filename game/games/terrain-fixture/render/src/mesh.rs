use terrain_fixture_logic::SIDE;

const EDGE: usize = SIDE as usize;
const PADDED: usize = EDGE + 2;

#[derive(Default)]
pub(crate) struct Mesh {
    pub(crate) vertices: Vec<u8>,
    pub(crate) indices: Vec<u32>,
}

pub(crate) fn section(origin: [i32; 3], height: impl Fn(i32, i32) -> i32) -> Mesh {
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
    if highest <= y || lowest > y + SIDE {
        return mesh;
    }
    let solid = |[x, dy, z]: [usize; 3]| y + dy as i32 - 1 < heights[z][x];

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

                    let first = (mesh.vertices.len() / 12) as u32;
                    for (du, dv) in [(0, 0), (width, 0), (width, height), (0, height)] {
                        let mut p = origin;
                        p[axis] += plane as i32;
                        p[u] += (col + du) as i32;
                        p[v] += (row + dv) as i32;
                        mesh.vertices
                            .extend(p.into_iter().flat_map(|n| (n as f32).to_le_bytes()));
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

#[cfg(test)]
mod tests {
    use super::*;

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
