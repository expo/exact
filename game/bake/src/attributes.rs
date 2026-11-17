//! KHR_mesh_quantization keeps node/texture transforms; only component decoding changes.
use gltf::accessor::{DataType, Dimensions};

pub(crate) fn read(
    primitive: &gltf::Primitive<'_>,
    buffers: &[gltf::buffer::Data],
    semantic: gltf::Semantic,
) -> Result<Option<Vec<f32>>, String> {
    let Some(accessor) = primitive.get(&semantic) else {
        return Ok(None);
    };
    let dimensions = match semantic {
        gltf::Semantic::Positions | gltf::Semantic::Normals => Dimensions::Vec3,
        gltf::Semantic::TexCoords(_) => Dimensions::Vec2,
        _ => return Err(format!("unsupported float attribute {semantic:?}")),
    };
    let kind = accessor.data_type();
    if accessor.dimensions() != dimensions
        || !matches!(
            kind,
            DataType::I8 | DataType::U8 | DataType::I16 | DataType::U16 | DataType::F32
        )
    {
        return Err(format!("{semantic:?}: invalid accessor type"));
    }
    if semantic == gltf::Semantic::Normals
        && kind != DataType::F32
        && (!matches!(kind, DataType::I8 | DataType::I16) || !accessor.normalized())
    {
        return Err("NORMAL: quantized components must be normalized signed integers".into());
    }
    if kind == DataType::F32 && accessor.normalized() {
        return Err(format!("{semantic:?}: FLOAT cannot be normalized"));
    }
    values(&accessor, buffers)
        .map(Some)
        .map_err(|e| format!("{semantic:?}: {e}"))
}
fn values(
    accessor: &gltf::Accessor<'_>,
    buffers: &[gltf::buffer::Data],
) -> Result<Vec<f32>, String> {
    let view = accessor.view().ok_or("missing accessor bufferView")?;
    let bytes = &buffers[view.buffer().index()].0;
    let kind = accessor.data_type();
    let width = kind.size();
    let components = accessor.dimensions().multiplicity();
    let size = width * components;
    let stride = view.stride().unwrap_or(size);
    let end = accessor
        .count()
        .checked_sub(1)
        .and_then(|n| n.checked_mul(stride))
        .and_then(|n| n.checked_add(accessor.offset()))
        .and_then(|n| n.checked_add(size))
        .ok_or("invalid accessor range")?;
    if stride < size || !stride.is_multiple_of(width) || end > view.length() {
        return Err("accessor exceeds its bufferView or stride".into());
    }
    let start = view
        .offset()
        .checked_add(accessor.offset())
        .ok_or("accessor offset overflow")?;
    let end = view
        .offset()
        .checked_add(end)
        .ok_or("accessor end overflow")?;
    let bytes = bytes.get(start..end).ok_or("accessor exceeds buffer")?;
    let mut result = Vec::with_capacity(accessor.count() * components);
    for i in 0..accessor.count() {
        for v in bytes[i * stride..i * stride + size].chunks_exact(width) {
            let normalized = accessor.normalized();
            let value = match kind {
                DataType::I8 => {
                    let x = f32::from(v[0] as i8);
                    if normalized {
                        (x / 127.).max(-1.)
                    } else {
                        x
                    }
                }
                DataType::U8 => {
                    let x = f32::from(v[0]);
                    if normalized {
                        x / 255.
                    } else {
                        x
                    }
                }
                DataType::I16 => {
                    let x = f32::from(i16::from_le_bytes(v.try_into().unwrap()));
                    if normalized {
                        (x / 32767.).max(-1.)
                    } else {
                        x
                    }
                }
                DataType::U16 => {
                    let x = f32::from(u16::from_le_bytes(v.try_into().unwrap()));
                    if normalized {
                        x / 65535.
                    } else {
                        x
                    }
                }
                DataType::F32 => f32::from_le_bytes(v.try_into().unwrap()),
                _ => return Err("unsupported accessor component type".into()),
            };
            result.push(value);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn integer_attributes_keep_normalization_offsets_and_interleaved_stride() {
        for (kind, bytes, raw, normalized) in [
            (
                5120,
                vec![128, 0, 127],
                vec![-128., 0., 127.],
                vec![-1., 0., 1.],
            ),
            (
                5121,
                vec![0, 127, 255],
                vec![0., 127., 255.],
                vec![0., 127. / 255., 1.],
            ),
            (
                5122,
                vec![0, 128, 0, 0, 255, 127],
                vec![-32768., 0., 32767.],
                vec![-1., 0., 1.],
            ),
            (
                5123,
                vec![0, 0, 0, 128, 255, 255],
                vec![0., 32768., 65535.],
                vec![0., 32768. / 65535., 1.],
            ),
        ] {
            let mut buffer = vec![42; 40];
            buffer[8..8 + bytes.len()].copy_from_slice(&bytes);
            buffer[24..24 + bytes.len()].copy_from_slice(&bytes);
            for normalize in [false, true] {
                let root = json!({"asset":{"version":"2.0"}, "buffers":[{"byteLength":40}], "bufferViews":[{"buffer":0,"byteOffset":4,"byteLength":32,"byteStride":16}], "accessors":[{"bufferView":0,"byteOffset":4,"componentType":kind,"type":"VEC3","count":2,"normalized":normalize}]});
                let doc = gltf::Document::from_json(serde_json::from_value(root).unwrap()).unwrap();
                let actual = values(
                    &doc.accessors().next().unwrap(),
                    &[gltf::buffer::Data(buffer.clone())],
                )
                .unwrap();
                assert_eq!(
                    actual,
                    if normalize {
                        normalized.repeat(2)
                    } else {
                        raw.repeat(2)
                    }
                );
            }
        }
    }
}
