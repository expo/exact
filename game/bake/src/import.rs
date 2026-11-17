//! Resolve compressed views before glTF's ordinary image and accessor readers run.
use serde_json::{json, Value};
use std::path::Path;
const MESHOPT: &str = "EXT_meshopt_compression";
const QUANTIZATION: &str = "KHR_mesh_quantization";
type Imported = (
    gltf::Document,
    Vec<gltf::buffer::Data>,
    Vec<gltf::image::Data>,
);

pub(crate) fn read(path: &Path) -> Result<Imported, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let source = gltf::Gltf::from_slice_without_validation(&bytes).map_err(|e| e.to_string())?;
    let mut root = source.document.into_json();
    for extension in root.extensions_used.iter().chain(&root.extensions_required) {
        if !matches!(
            extension.as_str(),
            "KHR_materials_clearcoat"
                | "KHR_materials_emissive_strength"
                | "KHR_materials_sheen"
                | "KHR_materials_unlit"
                | "KHR_texture_transform"
                | MESHOPT
                | QUANTIZATION
        ) {
            return Err(format!("unsupported extension {extension}"));
        }
    }
    // gltf 1.4 does not know these required extensions. We implement them here;
    // all other validation (including other required extensions) stays enabled.
    root.extensions_required
        .retain(|e| !matches!(e.as_str(), MESHOPT | QUANTIZATION));
    let doc = gltf::Document::from_json(root).map_err(|e| e.to_string())?;
    for accessor in doc.accessors() {
        if accessor.sparse().is_some() {
            return Err(format!(
                "sparse accessor {} is unsupported",
                accessor.index()
            ));
        }
    }
    let base = path.parent().unwrap_or(Path::new("."));
    let mut blob = source.blob;
    let mut buffers = Vec::new();
    for buffer in doc.buffers() {
        let placeholder = matches!(buffer.source(), gltf::buffer::Source::Bin) && blob.is_none();
        let data = if placeholder {
            // A URI-less fallback buffer may have no bytes. Only successfully
            // decoded views may refer to it; coverage is checked below.
            gltf::buffer::Data(Vec::new())
        } else {
            let data =
                gltf::buffer::Data::from_source_and_blob(buffer.source(), Some(base), &mut blob)
                    .map_err(|e| e.to_string())?;
            if data.len() < buffer.length() {
                return Err(format!(
                    "buffer {} is shorter than byteLength",
                    buffer.index()
                ));
            }
            data
        };
        buffers.push(data);
    }
    let mut json = serde_json::to_value(doc.as_json()).map_err(|e| e.to_string())?;
    for view in doc.views() {
        let Some(ext) = view.extension_value(MESHOPT) else {
            continue;
        };
        let result = (|| {
            let buffer = number(ext, "buffer")?;
            let offset = optional_number(ext, "byteOffset", 0)?;
            let length = number(ext, "byteLength")?;
            let count = number(ext, "count")?;
            let stride = number(ext, "byteStride")?;
            let mode = ext["mode"].as_str().ok_or("missing meshopt mode")?;
            let filter = match ext.get("filter") {
                None => "NONE",
                Some(v) => v.as_str().ok_or("invalid meshopt filter")?,
            };
            if count.checked_mul(stride) != Some(view.length())
                || view.stride().is_some_and(|s| s != stride)
            {
                return Err("meshopt decoded length/stride disagrees with bufferView".into());
            }
            let source = buffers
                .get(buffer)
                .ok_or("meshopt source buffer is missing")?;
            let declared = doc
                .buffers()
                .nth(buffer)
                .ok_or("meshopt source buffer index is invalid")?
                .length();
            let end = offset
                .checked_add(length)
                .filter(|&e| e <= declared)
                .ok_or("meshopt source range exceeds buffer")?;
            let source = source
                .0
                .get(offset..end)
                .ok_or("meshopt source bytes are missing")?;
            crate::meshopt::decode(count, stride, source, mode, filter)
        })()
        .map_err(|e: String| format!("{MESHOPT}: bufferView {}: {e}", view.index()))?;
        let index = buffers.len();
        json["buffers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"byteLength": result.len()}));
        let v = &mut json["bufferViews"][view.index()];
        v["buffer"] = json!(index);
        v["byteOffset"] = json!(0);
        v["extensions"].as_object_mut().unwrap().remove(MESHOPT);
        buffers.push(gltf::buffer::Data(result));
    }
    let doc = gltf::Document::from_json(serde_json::from_value(json).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for view in doc.views() {
        let end = view.offset().checked_add(view.length());
        if end
            .is_none_or(|e| e > buffers[view.buffer().index()].len() || e > view.buffer().length())
        {
            return Err(format!(
                "{MESHOPT}: bufferView {} has no decoded data or complete fallback",
                view.index()
            ));
        }
    }
    let images = gltf::import_images(&doc, Some(base), &buffers).map_err(|e| e.to_string())?;
    Ok((doc, buffers, images))
}
fn number(value: &Value, key: &str) -> Result<usize, String> {
    value[key]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("invalid meshopt {key}"))
}
fn optional_number(value: &Value, key: &str, default: usize) -> Result<usize, String> {
    if value.get(key).is_none() {
        Ok(default)
    } else {
        number(value, key)
    }
}
