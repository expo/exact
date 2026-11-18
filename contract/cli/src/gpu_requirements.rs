//! Build-time checking of the device declaration; no GPU dependency enters the compiler.
use serde_json::Value;

const LIMITS: &[(&str, u64)] = &[
    ("maxTextureDimension1D", u32::MAX as u64),
    ("maxTextureDimension2D", u32::MAX as u64),
    ("maxTextureDimension3D", u32::MAX as u64),
    ("maxTextureArrayLayers", u32::MAX as u64),
    ("maxBindGroups", u32::MAX as u64),
    ("maxBindGroupsPlusVertexBuffers", u32::MAX as u64),
    ("maxBindingsPerBindGroup", u32::MAX as u64),
    ("maxDynamicUniformBuffersPerPipelineLayout", u32::MAX as u64),
    ("maxDynamicStorageBuffersPerPipelineLayout", u32::MAX as u64),
    ("maxSampledTexturesPerShaderStage", u32::MAX as u64),
    ("maxSamplersPerShaderStage", u32::MAX as u64),
    ("maxStorageBuffersPerShaderStage", u32::MAX as u64),
    ("maxStorageTexturesPerShaderStage", u32::MAX as u64),
    ("maxUniformBuffersPerShaderStage", u32::MAX as u64),
    ("maxUniformBufferBindingSize", 9007199254740991),
    ("maxStorageBufferBindingSize", 9007199254740991),
    ("minUniformBufferOffsetAlignment", u32::MAX as u64),
    ("minStorageBufferOffsetAlignment", u32::MAX as u64),
    ("maxVertexBuffers", u32::MAX as u64),
    ("maxBufferSize", 9007199254740991),
    ("maxVertexAttributes", u32::MAX as u64),
    ("maxVertexBufferArrayStride", u32::MAX as u64),
    ("maxInterStageShaderVariables", u32::MAX as u64),
    ("maxColorAttachments", u32::MAX as u64),
    ("maxColorAttachmentBytesPerSample", u32::MAX as u64),
    ("maxComputeWorkgroupStorageSize", u32::MAX as u64),
    ("maxComputeInvocationsPerWorkgroup", u32::MAX as u64),
    ("maxComputeWorkgroupSizeX", u32::MAX as u64),
    ("maxComputeWorkgroupSizeY", u32::MAX as u64),
    ("maxComputeWorkgroupSizeZ", u32::MAX as u64),
    ("maxComputeWorkgroupsPerDimension", u32::MAX as u64),
    ("maxImmediateSize", u32::MAX as u64),
];

const FEATURES: &[&str] = &[
    "depth-clip-control",
    "depth32float-stencil8",
    "texture-compression-bc",
    "texture-compression-bc-sliced-3d",
    "texture-compression-etc2",
    "texture-compression-astc",
    "texture-compression-astc-sliced-3d",
    "timestamp-query",
    "indirect-first-instance",
    "shader-f16",
    "rg11b10ufloat-renderable",
    "bgra8unorm-storage",
    "float32-filterable",
    "float32-blendable",
    "dual-source-blending",
    "clip-distances",
    "subgroups",
    "primitive-index",
    "immediates",
];

pub(crate) fn check(gpu: &Value) -> Result<(), String> {
    for kind in ["limits", "features"] {
        let Some(value) = gpu.get(kind) else { continue };
        let object = value.as_object().ok_or_else(|| {
            format!("{kind} must be an object with required and optional members")
        })?;
        if let Some(key) = object
            .keys()
            .find(|k| !["required", "optional"].contains(&k.as_str()))
        {
            return Err(format!("{kind}.{key}: expected required or optional"));
        }
        let mut names = std::collections::BTreeSet::new();
        for tier in ["required", "optional"] {
            let Some(value) = object.get(tier) else {
                continue;
            };
            if kind == "limits" {
                let limits = value
                    .as_object()
                    .ok_or_else(|| format!("limits.{tier} must be an object"))?;
                for (name, value) in limits {
                    let max = LIMITS
                        .iter()
                        .find_map(|&(key, max)| (key == name).then_some(max))
                        .ok_or_else(|| format!("unknown WebGPU limit {name}"))?;
                    let n = value
                        .as_f64()
                        .filter(|n| *n >= 0.0 && *n <= max as f64 && n.fract() == 0.0)
                        .map(|n| n as u64)
                        .ok_or_else(|| {
                            format!("limits.{tier}.{name} must be an integer from 0 to {max}")
                        })?;
                    if name.starts_with("min") && !n.is_power_of_two() {
                        return Err(format!(
                            "limits.{tier}.{name} must be a positive power of two"
                        ));
                    }
                    if !names.insert(name.as_str()) {
                        return Err(format!("limit {name} occurs in both required and optional"));
                    }
                }
            } else {
                let features = value
                    .as_array()
                    .ok_or_else(|| format!("features.{tier} must be an array"))?;
                for value in features {
                    let name = value.as_str().ok_or_else(|| {
                        format!("features.{tier} must contain WebGPU feature names")
                    })?;
                    if !FEATURES.contains(&name) {
                        return Err(format!("unknown WebGPU feature {name}"));
                    }
                    if !names.insert(name) {
                        return Err(format!("feature {name} is declared more than once"));
                    }
                }
            }
        }
    }
    Ok(())
}
