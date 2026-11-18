//! Binary glTF reader adapted from Ming Kao's occlusion-viewer, b63cff7.
//! The MIT notice is ../../LICENSE-occlusion-viewer. LLP 1133.039 D11–D13.
use crate::accessor::{integer, optional, required, slice, vector, Reader, Result};
use crate::math::{
    cross, dot, mat_mul, norm, normal_matrix, sub, transform_normal, transform_point, trs, M4,
};
use crate::scene::{AlphaMode, Bounds, Material, Node, Primitive, Scene, Vertex};
use serde_json::Value as J;

const MAX_FILE: usize = 64 * 1024 * 1024;

/// Read with WebGPU's default 256 MiB buffer limit.
/// A GPU caller supplies its device's limit to `parse_with_limit`.
pub fn parse(bytes: &[u8]) -> Result<Scene> {
    parse_with_limit(bytes, 256 * 1024 * 1024)
}

/// `max_buffer_size` is the device limit for each resulting vertex/index buffer.
/// Errors name the GLB and, for accessor failures, its file index.
pub fn parse_with_limit(bytes: &[u8], max_buffer_size: u64) -> Result<Scene> {
    let read = || {
        let (json, bin) = chunks(bytes)?;
        if json["asset"]["version"].as_str() != Some("2.0") {
            return Err("asset.version must be 2.0".into());
        }
        if let Some(extensions) = json.get("extensionsRequired") {
            for e in extensions
                .as_array()
                .ok_or("extensionsRequired must be an array")?
            {
                let name = e.as_str().ok_or("required extension must be a name")?;
                if !matches!(name, "KHR_mesh_quantization" | "EXT_meshopt_compression") {
                    return Err(format!("required extension {name} is not read"));
                }
            }
        }
        let reader = Reader::new(&json, bin)?;
        let (mut nodes, order) = hierarchy(&json)?;
        let mut scene = Scene {
            nodes: Vec::new(),
            primitives: Vec::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            bounds: None,
            centre: [0.0; 3],
        };
        for &ni in &order {
            let node = &mut nodes[ni];
            node.indices.start = scene.indices.len() as u32;
            node.primitives.start = scene.primitives.len();
            if let Some(mesh) = json["nodes"][ni].get("mesh") {
                let mi = integer(mesh).map_err(|e| format!("node {ni}: mesh: {e}"))?;
                let mesh = json["meshes"]
                    .get(mi)
                    .ok_or_else(|| format!("node {ni}: no mesh {mi}"))?;
                let primitives = mesh["primitives"]
                    .as_array()
                    .ok_or_else(|| format!("mesh {mi}: no primitives"))?;
                for (pi, p) in primitives.iter().enumerate() {
                    append(&mut scene, node, ni, p, &json, &reader, max_buffer_size)
                        .map_err(|e| format!("node {ni}, mesh {mi}, primitive {pi}: {e}"))?;
                }
            }
            node.indices.end = scene.indices.len() as u32;
            node.primitives.end = scene.primitives.len();
            node.subtree_bounds = node.bounds;
            if let Some(bounds) = node.bounds {
                include(&mut scene.bounds, bounds);
            }
        }
        for &ni in order.iter().rev() {
            if let (Some(parent), Some(bounds)) = (nodes[ni].parent, nodes[ni].subtree_bounds) {
                include(&mut nodes[parent].subtree_bounds, bounds);
            }
        }
        scene.centre = scene.bounds.map(|b| b.centre()).unwrap_or([0.0; 3]);
        scene.nodes = nodes;
        Ok(scene)
    };
    read().map_err(|e| format!("GLB: {e}"))
}

fn chunks(bytes: &[u8]) -> Result<(J, &[u8])> {
    if bytes.len() > MAX_FILE {
        return Err("file exceeds 64 MiB".into());
    }
    if bytes.len() < 20 || bytes.get(..4) != Some(b"glTF") {
        return Err("expected a binary .glb file; .gltf files are not read".into());
    }
    let word = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
    if word(4) != 2 {
        return Err("version must be 2".into());
    }
    if word(8) != bytes.len() {
        return Err("file length does not match its header".into());
    }
    let mut at = 12;
    let (mut json, mut bin) = (None, None);
    while at < bytes.len() {
        slice(bytes, at, 8).map_err(|_| "truncated chunk header")?;
        let (length, kind) = (word(at), word(at + 4));
        if !length.is_multiple_of(4) {
            return Err("chunk length is not aligned to four bytes".into());
        }
        let chunk = slice(bytes, at + 8, length).map_err(|_| "chunk runs past the file")?;
        if at == 12 && kind != 0x4e4f534a {
            return Err("first chunk must be JSON".into());
        }
        match kind {
            0x4e4f534a => {
                if json.is_some() {
                    return Err("duplicate JSON chunk".into());
                }
                json = Some(serde_json::from_slice(chunk).map_err(|e| format!("JSON: {e}"))?);
            }
            0x004e4942 => {
                if bin.is_some() {
                    return Err("duplicate BIN chunk".into());
                }
                bin = Some(chunk);
            }
            _ => (), // glTF permits unknown chunks after JSON.
        }
        at += 8 + length;
    }
    Ok((json.ok_or("missing JSON chunk")?, bin.unwrap_or(&[])))
}

fn include(bounds: &mut Option<Bounds>, other: Bounds) {
    match bounds {
        Some(b) => b.include(other),
        None => *bounds = Some(other),
    }
}

fn local(n: &J) -> Result<M4> {
    if let Some(matrix) = n.get("matrix") {
        if ["translation", "rotation", "scale"]
            .iter()
            .any(|k| n.get(k).is_some())
        {
            return Err("matrix and TRS cannot both be written".into());
        }
        let values = vector(Some(matrix), [0.0; 16])?;
        return Ok(std::array::from_fn(|c| {
            std::array::from_fn(|r| values[c * 4 + r])
        }));
    }
    Ok(trs(
        vector(n.get("translation"), [0.0; 3])?,
        vector(n.get("rotation"), [0.0, 0.0, 0.0, 1.0])?,
        vector(n.get("scale"), [1.0; 3])?,
    ))
}

fn hierarchy(json: &J) -> Result<(Vec<Node>, Vec<usize>)> {
    let raw = json["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let mut nodes = Vec::with_capacity(raw.len());
    for (i, n) in raw.iter().enumerate() {
        let children = n
            .get("children")
            .map(|c| -> Result<Vec<_>> {
                c.as_array()
                    .ok_or("children must be an array")?
                    .iter()
                    .map(integer)
                    .collect()
            })
            .transpose()
            .map_err(|e| format!("node {i}: {e}"))?
            .unwrap_or_default();
        nodes.push(Node {
            name: n
                .get("name")
                .map(|v| v.as_str().map(str::to_owned).ok_or("name must be a string"))
                .transpose()
                .map_err(|e| format!("node {i}: {e}"))?
                .unwrap_or_else(|| format!("#{i}")),
            parent: None,
            children,
            active: false,
            world: local(n).map_err(|e| format!("node {i}: {e}"))?,
            indices: 0..0,
            primitives: 0..0,
            bounds: None,
            subtree_bounds: None,
        });
    }
    for (i, n) in raw.iter().enumerate() {
        if let Some(children) = n["children"].as_array() {
            for c in children {
                let c = integer(c)?;
                let child = nodes
                    .get_mut(c)
                    .ok_or_else(|| format!("node {i}: no child node {c}"))?;
                if child.parent.replace(i).is_some() {
                    return Err(format!("node {c}: has more than one parent"));
                }
            }
        }
    }
    // Validate and transform the entire forest, including inactive nodes, without recursion.
    let mut stack: Vec<_> = (0..nodes.len())
        .rev()
        .filter(|&i| nodes[i].parent.is_none())
        .collect();
    let mut all = Vec::with_capacity(nodes.len());
    while let Some(i) = stack.pop() {
        if let Some(p) = nodes[i].parent {
            nodes[i].world = mat_mul(nodes[p].world, nodes[i].world);
        }
        if nodes[i].world.iter().flatten().any(|n| !n.is_finite()) {
            return Err(format!("node {i}: transform is not finite"));
        }
        stack.extend(nodes[i].children.iter().rev().copied());
        all.push(i);
    }
    if all.len() != nodes.len() {
        return Err("node hierarchy contains a cycle".into());
    }
    let roots = if let Some(scenes) = json.get("scenes") {
        let si = optional(json, "scene", 0)?;
        let scene = scenes.get(si).ok_or_else(|| format!("no scene {si}"))?;
        scene
            .get("nodes")
            .map(|v| -> Result<Vec<_>> {
                v.as_array()
                    .ok_or("scene nodes must be an array")?
                    .iter()
                    .map(integer)
                    .collect()
            })
            .transpose()?
            .unwrap_or_default()
    } else {
        (0..nodes.len())
            .filter(|&i| nodes[i].parent.is_none())
            .collect()
    };
    for r in roots {
        let n = nodes.get(r).ok_or_else(|| format!("scene: no node {r}"))?;
        if n.parent.is_some() {
            return Err(format!("scene root node {r} has a parent"));
        }
        stack.push(r);
    }
    while let Some(i) = stack.pop() {
        if nodes[i].active {
            return Err(format!("scene repeats node {i}"));
        }
        nodes[i].active = true;
        stack.extend(nodes[i].children.iter().copied());
    }
    all.retain(|&i| nodes[i].active);
    Ok((nodes, all))
}

fn material(json: &J, p: &J) -> Result<Material> {
    let Some(index) = p.get("material") else {
        return Ok(Material::default());
    };
    let i = integer(index)?;
    let read = || -> Result<_> {
        let m = json["materials"].get(i).ok_or("missing material")?;
        let color = vector(m["pbrMetallicRoughness"].get("baseColorFactor"), [1.0; 4])?;
        if color.iter().any(|v| !(0.0..=1.0).contains(v)) {
            return Err("baseColorFactor must be in 0..1".into());
        }
        let alpha_mode = match m.get("alphaMode").and_then(J::as_str).unwrap_or("OPAQUE") {
            "OPAQUE" => AlphaMode::Opaque,
            "MASK" => AlphaMode::Mask,
            "BLEND" => AlphaMode::Blend,
            mode => return Err(format!("unknown alphaMode {mode}")),
        };
        let alpha_cutoff = m
            .get("alphaCutoff")
            .map(|v| v.as_f64().ok_or("invalid alphaCutoff"))
            .transpose()?
            .unwrap_or(0.5) as f32;
        if !alpha_cutoff.is_finite() || alpha_cutoff < 0.0 {
            return Err("alphaCutoff must be finite and nonnegative".into());
        }
        Ok(Material {
            color,
            alpha_mode,
            alpha_cutoff,
        })
    };
    read().map_err(|e| format!("material {i}: {e}"))
}

fn buffer_size(count: usize, size: usize, limit: u64) -> Result<()> {
    if count > u32::MAX as usize
        || (count as u64)
            .checked_mul(size as u64)
            .is_none_or(|n| n > limit)
    {
        return Err("geometry buffer exceeds max_buffer_size or 32-bit index range".into());
    }
    Ok(())
}

fn append(
    scene: &mut Scene,
    node: &mut Node,
    ni: usize,
    p: &J,
    json: &J,
    reader: &Reader<'_>,
    limit: u64,
) -> Result<()> {
    let mode = optional(p, "mode", 4)?;
    if mode <= 3 {
        return Ok(());
    } // Lines and points are outside D11's triangle reader.
    if mode > 6 {
        return Err(format!("invalid primitive mode {mode}"));
    }
    let ai = required(&p["attributes"], "POSITION")?;
    let count = reader.count(ai)?;
    let total = scene
        .vertices
        .len()
        .checked_add(count)
        .ok_or("vertex count overflow")?;
    buffer_size(total, std::mem::size_of::<Vertex>(), limit)
        .map_err(|e| format!("accessor {ai}: {e}"))?;
    let nm = normal_matrix(&node.world);
    let normals = if let Some(normal) = p["attributes"].get("NORMAL") {
        let normal = integer(normal)?;
        if reader.count(normal)? != count {
            return Err(format!(
                "accessor {normal}: NORMAL count differs from POSITION accessor {ai}"
            ));
        }
        Some(
            reader
                .vectors(normal, true)?
                .into_iter()
                .map(|n| transform_normal(&nm, n))
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let index_accessor = p.get("indices").map(integer).transpose()?;
    let index_count = index_accessor
        .map(|a| reader.count(a))
        .transpose()?
        .unwrap_or(count);
    let output_count = if mode == 4 {
        if !index_count.is_multiple_of(3) {
            return Err(format!(
                "accessor {}: triangle index count is not a multiple of three",
                index_accessor.unwrap_or(ai)
            ));
        }
        index_count
    } else {
        index_count
            .saturating_sub(2)
            .checked_mul(3)
            .ok_or("index count overflow")?
    };
    let total = scene
        .indices
        .len()
        .checked_add(output_count)
        .ok_or("index count overflow")?;
    buffer_size(total, 4, limit)
        .map_err(|e| format!("accessor {}: {e}", index_accessor.unwrap_or(ai)))?;
    let vertex_count = if normals.is_some() {
        count
    } else {
        output_count
    };
    let total = scene
        .vertices
        .len()
        .checked_add(vertex_count)
        .ok_or("vertex count overflow")?;
    buffer_size(total, std::mem::size_of::<Vertex>(), limit)
        .map_err(|e| format!("accessor {ai}: {e}"))?;
    let positions = reader.vectors(ai, false)?;
    let indices = match index_accessor {
        Some(a) => reader.indices(a, count)?,
        None => (0..count as u32).collect(),
    };
    let triangles = match mode {
        4 => indices,
        5 | 6 => {
            let mut triangles = Vec::with_capacity(output_count);
            for i in 2..indices.len() {
                let triangle = if mode == 6 {
                    [indices[0], indices[i - 1], indices[i]]
                } else if i % 2 == 0 {
                    [indices[i - 2], indices[i - 1], indices[i]]
                } else {
                    [indices[i - 1], indices[i - 2], indices[i]]
                };
                triangles.extend(triangle);
            }
            triangles
        }
        _ => unreachable!(),
    };
    let primitive = u32::try_from(scene.primitives.len()).map_err(|_| "too many primitives")?;
    let base = scene.vertices.len() as u32;
    let start = scene.indices.len() as u32;
    if let Some(normals) = normals {
        scene
            .vertices
            .extend(positions.into_iter().zip(normals).map(|(p, n)| Vertex {
                p,
                n,
                node: ni as u32,
                primitive,
            }));
        scene
            .indices
            .extend(triangles.into_iter().map(|i| i + base));
    } else {
        // glTF 2.0 §3.7.2.1 requires flat normals without NORMAL; faces need separate corners.
        for triangle in triangles.as_chunks::<3>().0 {
            let [a, b, c] = std::array::from_fn(|i| positions[triangle[i] as usize]);
            let normal = norm(cross(norm(sub(b, a)), norm(sub(c, a))));
            let n = transform_normal(&nm, normal);
            scene.vertices.extend([a, b, c].map(|p| Vertex {
                p,
                n,
                node: ni as u32,
                primitive,
            }));
        }
        scene.indices.extend(base..scene.vertices.len() as u32);
    }
    for vertex in &mut scene.vertices[base as usize..] {
        vertex.p = transform_point(&node.world, vertex.p);
        if vertex.p.iter().any(|v| !v.is_finite()) {
            return Err(format!("accessor {ai}: transformed POSITION is not finite"));
        }
    }
    let column = |c: usize| std::array::from_fn(|r| node.world[c][r]);
    // Positions are in world space; preserve glTF fronts after a reflection.
    if dot(column(0), cross(column(1), column(2))) < 0. {
        for triangle in scene.indices[start as usize..].chunks_exact_mut(3) {
            triangle.swap(1, 2);
        }
    }
    for &index in &scene.indices[start as usize..] {
        include(
            &mut node.bounds,
            Bounds::point(scene.vertices[index as usize].p),
        );
    }
    scene.primitives.push(Primitive {
        node: ni,
        indices: start..scene.indices.len() as u32,
        material: material(json, p)?,
    });
    Ok(())
}
