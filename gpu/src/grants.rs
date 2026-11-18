//! Package device requirements, negotiated before any surface prepares resources.
use crate::wgpu;

/// One package's WebGPU requirements from `exact.json`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Requirements {
    /// Package name, used in refusals.
    pub package: &'static str,
    /// Surfaces owned by this package.
    pub surfaces: &'static [&'static str],
    /// Limits the adapter must satisfy.
    pub required_limits: &'static [(&'static str, u64)],
    /// Limits requested up to what the adapter offers.
    pub optional_limits: &'static [(&'static str, u64)],
    /// Features the adapter must offer.
    pub required_features: &'static [&'static str],
    /// Features granted where the adapter offers them.
    pub optional_features: &'static [&'static str],
}

/// A shared device's grant and the packages that cannot use it.
#[derive(Debug)]
pub struct Grant {
    /// Limits to request when creating the device.
    pub limits: wgpu::Limits,
    /// Features to request when creating the device.
    pub features: wgpu::Features,
    /// Refused package names and their diagnostics.
    pub refused: Vec<(&'static str, String)>,
}

/// Union the requirements of packages that this adapter can serve.
pub fn negotiate(
    available: wgpu::Limits,
    features: wgpu::Features,
    packages: &[Requirements],
) -> Grant {
    let mut grant = Grant {
        limits: crate::requested_limits(available.clone()),
        features: features & crate::requested_features(),
        refused: Vec::new(),
    };
    for package in packages {
        if let Err(error) = package.check(&available, features) {
            grant.refused.push((package.package, error));
            continue;
        }
        for &(name, value) in package
            .required_limits
            .iter()
            .chain(package.optional_limits)
        {
            let have = limit(&available, name).expect("build-checked limit name");
            let have = paired_limit(name)
                .map(|paired| have.min(limit(&available, paired).unwrap()))
                .unwrap_or(have);
            let current = limit(&grant.limits, name).expect("build-checked limit name");
            let value = if name.starts_with("min") {
                current.min(value.max(have))
            } else {
                current.max(value).min(have)
            };
            set_limit(&mut grant.limits, name, value);
            if let Some(paired) = paired_limit(name) {
                let current = limit(&grant.limits, paired).unwrap();
                set_limit(&mut grant.limits, paired, current.max(value));
            }
        }
        for name in package
            .required_features
            .iter()
            .chain(package.optional_features)
        {
            grant.features |= feature(name).expect("build-checked feature name") & features;
        }
    }
    grant
}

impl Requirements {
    /// Refuse this package before any of its surfaces allocate on an insufficient device.
    pub fn check(&self, limits: &wgpu::Limits, features: wgpu::Features) -> Result<(), String> {
        for &(name, need) in self.required_limits {
            let Some(have) = limit(limits, name) else {
                return Err(format!("{}: unknown WebGPU limit {name}", self.package));
            };
            if if name.starts_with("min") {
                have > need
            } else {
                have < need
            } {
                let order = if name.starts_with("min") { "<=" } else { ">=" };
                return Err(format!(
                    "{}: GPU requires {name} {order} {need}, available {have}",
                    self.package
                ));
            }
            if let Some(paired) = paired_limit(name) {
                let have = limit(limits, paired).unwrap();
                if have < need {
                    return Err(format!(
                        "{}: GPU requires {name} >= {need}, but {paired} available {have}",
                        self.package
                    ));
                }
            }
        }
        for &(name, _) in self.optional_limits {
            if limit(limits, name).is_none() {
                return Err(format!("{}: unknown WebGPU limit {name}", self.package));
            }
        }
        for name in self.optional_features {
            if feature(name).is_none() {
                return Err(format!("{}: unknown WebGPU feature {name}", self.package));
            }
        }
        for name in self.required_features {
            if feature(name).is_none_or(|f| !features.contains(f)) {
                return Err(format!(
                    "{}: GPU requires feature {name}, not available",
                    self.package
                ));
            }
        }
        Ok(())
    }
}

fn paired_limit(name: &str) -> Option<&'static str> {
    match name {
        "maxStorageBufferBindingSize" | "maxUniformBufferBindingSize" => Some("maxBufferSize"),
        "maxComputeWorkgroupSizeX" | "maxComputeWorkgroupSizeY" | "maxComputeWorkgroupSizeZ" => {
            Some("maxComputeInvocationsPerWorkgroup")
        }
        _ => None,
    }
}

fn limit(limits: &wgpu::Limits, name: &str) -> Option<u64> {
    Some(match name {
        "maxTextureDimension1D" => u64::from(limits.max_texture_dimension_1d),
        "maxTextureDimension2D" => u64::from(limits.max_texture_dimension_2d),
        "maxTextureDimension3D" => u64::from(limits.max_texture_dimension_3d),
        "maxTextureArrayLayers" => u64::from(limits.max_texture_array_layers),
        "maxBindGroups" => u64::from(limits.max_bind_groups),
        "maxBindGroupsPlusVertexBuffers" => u64::from(limits.max_bind_groups_plus_vertex_buffers),
        "maxBindingsPerBindGroup" => u64::from(limits.max_bindings_per_bind_group),
        "maxDynamicUniformBuffersPerPipelineLayout" => {
            u64::from(limits.max_dynamic_uniform_buffers_per_pipeline_layout)
        }
        "maxDynamicStorageBuffersPerPipelineLayout" => {
            u64::from(limits.max_dynamic_storage_buffers_per_pipeline_layout)
        }
        "maxSampledTexturesPerShaderStage" => {
            u64::from(limits.max_sampled_textures_per_shader_stage)
        }
        "maxSamplersPerShaderStage" => u64::from(limits.max_samplers_per_shader_stage),
        "maxStorageBuffersPerShaderStage" => u64::from(limits.max_storage_buffers_per_shader_stage),
        "maxStorageTexturesPerShaderStage" => {
            u64::from(limits.max_storage_textures_per_shader_stage)
        }
        "maxUniformBuffersPerShaderStage" => u64::from(limits.max_uniform_buffers_per_shader_stage),
        "maxUniformBufferBindingSize" => limits.max_uniform_buffer_binding_size,
        "maxStorageBufferBindingSize" => limits.max_storage_buffer_binding_size,
        "minUniformBufferOffsetAlignment" => u64::from(limits.min_uniform_buffer_offset_alignment),
        "minStorageBufferOffsetAlignment" => u64::from(limits.min_storage_buffer_offset_alignment),
        "maxVertexBuffers" => u64::from(limits.max_vertex_buffers),
        "maxBufferSize" => limits.max_buffer_size,
        "maxVertexAttributes" => u64::from(limits.max_vertex_attributes),
        "maxVertexBufferArrayStride" => u64::from(limits.max_vertex_buffer_array_stride),
        "maxInterStageShaderVariables" => u64::from(limits.max_inter_stage_shader_variables),
        "maxColorAttachments" => u64::from(limits.max_color_attachments),
        "maxColorAttachmentBytesPerSample" => {
            u64::from(limits.max_color_attachment_bytes_per_sample)
        }
        "maxComputeWorkgroupStorageSize" => u64::from(limits.max_compute_workgroup_storage_size),
        "maxComputeInvocationsPerWorkgroup" => {
            u64::from(limits.max_compute_invocations_per_workgroup)
        }
        "maxComputeWorkgroupSizeX" => u64::from(limits.max_compute_workgroup_size_x),
        "maxComputeWorkgroupSizeY" => u64::from(limits.max_compute_workgroup_size_y),
        "maxComputeWorkgroupSizeZ" => u64::from(limits.max_compute_workgroup_size_z),
        "maxComputeWorkgroupsPerDimension" => {
            u64::from(limits.max_compute_workgroups_per_dimension)
        }
        "maxImmediateSize" => u64::from(limits.max_immediate_size),
        _ => return None,
    })
}

fn set_limit(limits: &mut wgpu::Limits, name: &str, value: u64) {
    match name {
        "maxTextureDimension1D" => {
            limits.max_texture_dimension_1d = value.try_into().expect("build-checked limit range")
        }
        "maxTextureDimension2D" => {
            limits.max_texture_dimension_2d = value.try_into().expect("build-checked limit range")
        }
        "maxTextureDimension3D" => {
            limits.max_texture_dimension_3d = value.try_into().expect("build-checked limit range")
        }
        "maxTextureArrayLayers" => {
            limits.max_texture_array_layers = value.try_into().expect("build-checked limit range")
        }
        "maxBindGroups" => {
            limits.max_bind_groups = value.try_into().expect("build-checked limit range")
        }
        "maxBindGroupsPlusVertexBuffers" => {
            limits.max_bind_groups_plus_vertex_buffers =
                value.try_into().expect("build-checked limit range")
        }
        "maxBindingsPerBindGroup" => {
            limits.max_bindings_per_bind_group =
                value.try_into().expect("build-checked limit range")
        }
        "maxDynamicUniformBuffersPerPipelineLayout" => {
            limits.max_dynamic_uniform_buffers_per_pipeline_layout =
                value.try_into().expect("build-checked limit range")
        }
        "maxDynamicStorageBuffersPerPipelineLayout" => {
            limits.max_dynamic_storage_buffers_per_pipeline_layout =
                value.try_into().expect("build-checked limit range")
        }
        "maxSampledTexturesPerShaderStage" => {
            limits.max_sampled_textures_per_shader_stage =
                value.try_into().expect("build-checked limit range")
        }
        "maxSamplersPerShaderStage" => {
            limits.max_samplers_per_shader_stage =
                value.try_into().expect("build-checked limit range")
        }
        "maxStorageBuffersPerShaderStage" => {
            limits.max_storage_buffers_per_shader_stage =
                value.try_into().expect("build-checked limit range")
        }
        "maxStorageTexturesPerShaderStage" => {
            limits.max_storage_textures_per_shader_stage =
                value.try_into().expect("build-checked limit range")
        }
        "maxUniformBuffersPerShaderStage" => {
            limits.max_uniform_buffers_per_shader_stage =
                value.try_into().expect("build-checked limit range")
        }
        "maxUniformBufferBindingSize" => limits.max_uniform_buffer_binding_size = value,
        "maxStorageBufferBindingSize" => limits.max_storage_buffer_binding_size = value,
        "minUniformBufferOffsetAlignment" => {
            limits.min_uniform_buffer_offset_alignment =
                value.try_into().expect("build-checked limit range")
        }
        "minStorageBufferOffsetAlignment" => {
            limits.min_storage_buffer_offset_alignment =
                value.try_into().expect("build-checked limit range")
        }
        "maxVertexBuffers" => {
            limits.max_vertex_buffers = value.try_into().expect("build-checked limit range")
        }
        "maxBufferSize" => limits.max_buffer_size = value,
        "maxVertexAttributes" => {
            limits.max_vertex_attributes = value.try_into().expect("build-checked limit range")
        }
        "maxVertexBufferArrayStride" => {
            limits.max_vertex_buffer_array_stride =
                value.try_into().expect("build-checked limit range")
        }
        "maxInterStageShaderVariables" => {
            limits.max_inter_stage_shader_variables =
                value.try_into().expect("build-checked limit range")
        }
        "maxColorAttachments" => {
            limits.max_color_attachments = value.try_into().expect("build-checked limit range")
        }
        "maxColorAttachmentBytesPerSample" => {
            limits.max_color_attachment_bytes_per_sample =
                value.try_into().expect("build-checked limit range")
        }
        "maxComputeWorkgroupStorageSize" => {
            limits.max_compute_workgroup_storage_size =
                value.try_into().expect("build-checked limit range")
        }
        "maxComputeInvocationsPerWorkgroup" => {
            limits.max_compute_invocations_per_workgroup =
                value.try_into().expect("build-checked limit range")
        }
        "maxComputeWorkgroupSizeX" => {
            limits.max_compute_workgroup_size_x =
                value.try_into().expect("build-checked limit range")
        }
        "maxComputeWorkgroupSizeY" => {
            limits.max_compute_workgroup_size_y =
                value.try_into().expect("build-checked limit range")
        }
        "maxComputeWorkgroupSizeZ" => {
            limits.max_compute_workgroup_size_z =
                value.try_into().expect("build-checked limit range")
        }
        "maxComputeWorkgroupsPerDimension" => {
            limits.max_compute_workgroups_per_dimension =
                value.try_into().expect("build-checked limit range")
        }
        "maxImmediateSize" => {
            limits.max_immediate_size = value.try_into().expect("build-checked limit range")
        }
        _ => unreachable!("build-checked limit name"),
    }
}

fn feature(name: &str) -> Option<wgpu::Features> {
    use wgpu::Features as F;
    Some(match name {
        "depth-clip-control" => F::DEPTH_CLIP_CONTROL,
        "depth32float-stencil8" => F::DEPTH32FLOAT_STENCIL8,
        "texture-compression-bc" => F::TEXTURE_COMPRESSION_BC,
        "texture-compression-bc-sliced-3d" => {
            F::TEXTURE_COMPRESSION_BC_SLICED_3D | F::TEXTURE_COMPRESSION_BC
        }
        "texture-compression-etc2" => F::TEXTURE_COMPRESSION_ETC2,
        "texture-compression-astc" => F::TEXTURE_COMPRESSION_ASTC,
        "texture-compression-astc-sliced-3d" => {
            F::TEXTURE_COMPRESSION_ASTC_SLICED_3D | F::TEXTURE_COMPRESSION_ASTC
        }
        "timestamp-query" => F::TIMESTAMP_QUERY,
        "indirect-first-instance" => F::INDIRECT_FIRST_INSTANCE,
        "shader-f16" => F::SHADER_F16,
        "rg11b10ufloat-renderable" => F::RG11B10UFLOAT_RENDERABLE,
        "bgra8unorm-storage" => F::BGRA8UNORM_STORAGE,
        "float32-filterable" => F::FLOAT32_FILTERABLE,
        "float32-blendable" => F::FLOAT32_BLENDABLE,
        "dual-source-blending" => F::DUAL_SOURCE_BLENDING,
        "clip-distances" => F::CLIP_DISTANCES,
        "subgroups" => F::SUBGROUP,
        "primitive-index" => F::PRIMITIVE_INDEX,
        "immediates" => F::IMMEDIATES,
        _ => return None,
    })
}

impl crate::Module {
    /// Set the packages sharing this module's device, before installing a device or creating surfaces.
    pub fn with_requirements(mut self, requirements: &'static [Requirements]) -> Self {
        assert!(self.gpu.is_none() && self.instances.is_empty());
        self.requirements = requirements;
        self
    }

    pub(crate) fn refused_instance(&self, id: u32) -> Option<String> {
        self.instances
            .get(&id)
            .and_then(|i| self.refused.get(i.name.as_str()))
            .cloned()
    }
}

pub(crate) fn refusals(
    requirements: &[Requirements],
    gpu: &crate::Gpu,
) -> std::collections::HashMap<&'static str, String> {
    let mut refused = std::collections::HashMap::new();
    for package in requirements {
        let checked = package
            .check(&gpu.adapter.limits(), gpu.adapter.features())
            .and_then(|()| package.check(&gpu.device.limits(), gpu.device.features()));
        if let Err(error) = checked {
            for &surface in package.surfaces {
                refused.insert(surface, error.clone());
            }
        }
    }
    refused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_declaration_keeps_the_existing_device_request() {
        let available = wgpu::Limits::default();
        let features = wgpu::Features::all();
        let grant = negotiate(available.clone(), features, &[]);
        assert_eq!(grant.limits, crate::requested_limits(available));
        assert_eq!(grant.features, crate::requested_features());
        assert!(grant.refused.is_empty());
    }

    #[test]
    fn packages_union_required_limits_and_clamp_optional_limits() {
        let available = wgpu::Limits {
            max_buffer_size: 1 << 30,
            max_storage_buffer_binding_size: 1 << 30,
            min_storage_buffer_offset_alignment: 64,
            ..wgpu::Limits::default()
        };
        let packages = [
            Requirements {
                package: "grid",
                required_limits: &[("maxBufferSize", 1 << 29)],
                optional_limits: &[("maxStorageBufferBindingSize", 1 << 31)],
                required_features: &["float32-filterable"],
                ..Requirements::default()
            },
            Requirements {
                package: "filter",
                required_limits: &[("maxBufferSize", 1 << 30)],
                optional_limits: &[("minStorageBufferOffsetAlignment", 16)],
                optional_features: &["subgroups", "primitive-index", "shader-f16"],
                ..Requirements::default()
            },
        ];
        let features = wgpu::Features::FLOAT32_FILTERABLE
            | wgpu::Features::SUBGROUP
            | wgpu::Features::PRIMITIVE_INDEX;
        let grant = negotiate(available, features, &packages);
        assert!(grant.refused.is_empty());
        assert_eq!(grant.limits.max_buffer_size, 1 << 30);
        assert_eq!(grant.limits.max_storage_buffer_binding_size, 1 << 30);
        assert_eq!(grant.limits.min_storage_buffer_offset_alignment, 64);
        assert_eq!(grant.features, features);
    }

    #[test]
    fn binding_sizes_raise_the_buffer_limit() {
        let available = wgpu::Limits {
            max_buffer_size: 1 << 30,
            max_storage_buffer_binding_size: 1 << 30,
            max_uniform_buffer_binding_size: 1 << 30,
            ..wgpu::Limits::default()
        };
        for required in [
            &[("maxStorageBufferBindingSize", 1 << 29)][..],
            &[("maxUniformBufferBindingSize", 1 << 29)][..],
        ] {
            let name = required[0].0;
            let grant = negotiate(
                available.clone(),
                wgpu::Features::empty(),
                &[Requirements {
                    required_limits: required,
                    ..Requirements::default()
                }],
            );
            assert!(grant.refused.is_empty());
            assert_eq!(limit(&grant.limits, name), Some(1 << 29));
            assert_eq!(grant.limits.max_buffer_size, 1 << 29);
        }
    }

    #[test]
    fn workgroup_dimensions_raise_the_invocation_limit() {
        let available = wgpu::Limits {
            max_compute_workgroup_size_x: 1024,
            max_compute_workgroup_size_y: 1024,
            max_compute_workgroup_size_z: 1024,
            max_compute_invocations_per_workgroup: 1024,
            ..wgpu::Limits::default()
        };
        for required in [
            &[("maxComputeWorkgroupSizeX", 512)][..],
            &[("maxComputeWorkgroupSizeY", 512)][..],
            &[("maxComputeWorkgroupSizeZ", 512)][..],
        ] {
            let name = required[0].0;
            let grant = negotiate(
                available.clone(),
                wgpu::Features::empty(),
                &[Requirements {
                    required_limits: required,
                    ..Requirements::default()
                }],
            );
            assert!(grant.refused.is_empty());
            assert_eq!(limit(&grant.limits, name), Some(512));
            assert_eq!(grant.limits.max_compute_invocations_per_workgroup, 512);
        }
    }

    #[test]
    fn paired_adapter_limits_refuse_only_the_required_package() {
        let available = wgpu::Limits {
            max_buffer_size: 1 << 28,
            max_storage_buffer_binding_size: 1 << 30,
            max_uniform_buffer_binding_size: 1 << 30,
            max_compute_workgroup_size_x: 1024,
            max_compute_workgroup_size_y: 1024,
            max_compute_workgroup_size_z: 1024,
            max_compute_invocations_per_workgroup: 256,
            ..wgpu::Limits::default()
        };
        for (required, paired, have) in [
            (
                &[("maxStorageBufferBindingSize", 1 << 29)][..],
                "maxBufferSize",
                1 << 28,
            ),
            (
                &[("maxUniformBufferBindingSize", 1 << 29)][..],
                "maxBufferSize",
                1 << 28,
            ),
            (
                &[("maxComputeWorkgroupSizeX", 512)][..],
                "maxComputeInvocationsPerWorkgroup",
                256,
            ),
            (
                &[("maxComputeWorkgroupSizeY", 512)][..],
                "maxComputeInvocationsPerWorkgroup",
                256,
            ),
            (
                &[("maxComputeWorkgroupSizeZ", 512)][..],
                "maxComputeInvocationsPerWorkgroup",
                256,
            ),
        ] {
            let (name, need) = required[0];
            let grant = negotiate(
                available.clone(),
                wgpu::Features::FLOAT32_FILTERABLE,
                &[
                    Requirements {
                        package: "impossible",
                        required_limits: required,
                        optional_features: &["float32-filterable"],
                        ..Requirements::default()
                    },
                    Requirements {
                        package: "works",
                        ..Requirements::default()
                    },
                ],
            );
            assert_eq!(grant.refused.len(), 1);
            assert_eq!(grant.refused[0].0, "impossible");
            for text in [name, &need.to_string(), paired, &have.to_string()] {
                assert!(grant.refused[0].1.contains(text), "{}", grant.refused[0].1);
            }
            assert!(!grant.features.contains(wgpu::Features::FLOAT32_FILTERABLE));
            let optional = negotiate(
                available.clone(),
                wgpu::Features::empty(),
                &[Requirements {
                    optional_limits: required,
                    ..Requirements::default()
                }],
            );
            assert!(optional.refused.is_empty());
            assert_eq!(limit(&optional.limits, name), Some(have));
            assert_eq!(limit(&optional.limits, paired), Some(have));
        }
    }

    #[test]
    fn an_unmeetable_required_limit_refuses_only_its_package() {
        let available = wgpu::Limits::default();
        let packages = [
            Requirements {
                package: "too-large",
                required_limits: &[("maxBufferSize", 1 << 40)],
                optional_features: &["float32-filterable"],
                ..Requirements::default()
            },
            Requirements {
                package: "works",
                required_limits: &[("maxComputeInvocationsPerWorkgroup", 256)],
                ..Requirements::default()
            },
        ];
        let grant = negotiate(
            available.clone(),
            wgpu::Features::FLOAT32_FILTERABLE,
            &packages,
        );
        assert_eq!(grant.refused.len(), 1);
        assert_eq!(grant.refused[0].0, "too-large");
        for word in [
            "maxBufferSize",
            "1099511627776",
            &available.max_buffer_size.to_string(),
        ] {
            assert!(grant.refused[0].1.contains(word), "{}", grant.refused[0].1);
        }
        assert!(!grant.features.contains(wgpu::Features::FLOAT32_FILTERABLE));
        assert_eq!(grant.limits.max_compute_invocations_per_workgroup, 256);
        for requirement in [
            Requirements {
                package: "missing-feature",
                required_features: &["subgroups"],
                ..Requirements::default()
            },
            Requirements {
                package: "missing-primitive-index",
                required_features: &["primitive-index"],
                ..Requirements::default()
            },
            Requirements {
                package: "alignment",
                required_limits: &[("minStorageBufferOffsetAlignment", 16)],
                ..Requirements::default()
            },
        ] {
            let grant = negotiate(available.clone(), wgpu::Features::empty(), &[requirement]);
            assert_eq!(grant.refused.len(), 1);
            assert!(grant.refused[0].1.contains(requirement.package));
        }
    }
}
