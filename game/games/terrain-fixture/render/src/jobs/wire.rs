use super::{mesh, Candidate, Input, Job, JobError, Output, SectionWork, WorkloadSettings};
use exact_game_render::WireJob;

impl WireJob for Input {
    const NAME: &'static str = "terrain.mesh";

    fn wire(&self) -> bool {
        self.work[0].kind == SectionWork::Mesh
    }

    fn encode(&self, out: &mut Vec<u8>) {
        let job = self.work[0];
        // The page keeps the revision and crossing for installation.
        for axis in job.origin {
            out.extend(axis.to_le_bytes());
        }
        out.extend(self.settings.vertex.to_le_bytes());
        out.extend(self.settings.light.to_le_bytes());
        out.push(u8::from(job.changed) | (u8::from(self.settings.compare) << 1));
    }

    fn decode(bytes: &[u8]) -> Result<Self, JobError> {
        let (words, &[flags]) = bytes.as_chunks::<4>() else {
            return Err(invalid());
        };
        let &[x, y, z, vertex, light] = words else {
            return Err(invalid());
        };
        let vertex = u32::from_le_bytes(vertex);
        let light = u32::from_le_bytes(light);
        if flags > 3 || !matches!(vertex, 12 | 64) || light > terrain_fixture_logic::MAX_LIGHT_CELLS
        {
            return Err(invalid());
        }
        let origin = [x, y, z].map(i32::from_le_bytes);
        Ok(Self {
            work: [Job::mesh_input(origin, flags & 1 != 0)].into(),
            previous: Vec::new(),
            settings: WorkloadSettings {
                vertex,
                light,
                compare: flags & 2 != 0,
                ..WorkloadSettings::default()
            },
        })
    }

    fn encode_output(output: &Output, out: &mut Vec<u8>) {
        let Output::Mesh(Candidate { mesh, input }) = output else {
            panic!("compare jobs stay on the page");
        };
        let cells = input.as_deref().unwrap_or_default();
        // Request the original capacities for Candidate::cpu_bytes.
        for length in [
            mesh.vertices.len(),
            mesh.vertices.capacity(),
            mesh.indices.len(),
            mesh.indices.capacity(),
        ] {
            out.extend(u32::try_from(length).unwrap().to_le_bytes());
        }
        out.extend(&mesh.vertices);
        for index in &mesh.indices {
            out.extend(index.to_le_bytes());
        }
        out.extend(cells);
    }

    fn decode_output(bytes: &[u8]) -> Result<Output, JobError> {
        let (header, body) = bytes.split_at_checked(16).ok_or_else(invalid)?;
        let [vertices, vertex_capacity, indices, index_capacity] =
            <[[u8; 4]; 4]>::try_from(header.as_chunks::<4>().0)
                .unwrap()
                .map(|word| u32::from_le_bytes(word) as usize);
        let (vertex_bytes, rest) = body.split_at_checked(vertices).ok_or_else(invalid)?;
        let index_size = indices.checked_mul(4).ok_or_else(invalid)?;
        let (index_bytes, cells) = rest.split_at_checked(index_size).ok_or_else(invalid)?;
        let input = match cells.len() {
            0 => None,
            len if len == (super::SIDE as usize + 4).pow(3) => Some(cells.into()),
            _ => return Err(invalid()),
        };
        let mut vertices = Vec::with_capacity(check_capacity(vertices, vertex_capacity)?);
        let mut indices = Vec::with_capacity(check_capacity(indices, index_capacity)?);
        vertices.extend_from_slice(vertex_bytes);
        indices.extend(
            index_bytes
                .as_chunks::<4>()
                .0
                .iter()
                .copied()
                .map(u32::from_le_bytes),
        );
        Ok(Output::Mesh(Candidate {
            mesh: mesh::Mesh { vertices, indices },
            input,
        }))
    }
}

fn check_capacity(length: usize, capacity: usize) -> Result<usize, JobError> {
    if capacity < length || capacity > length.saturating_mul(2).max(8) {
        return Err(invalid());
    }
    Ok(capacity)
}

fn invalid() -> JobError {
    JobError("invalid terrain mesh wire data".into())
}
