use crate::wgpu;

/// Enable dynamic offsets on a buffer layout entry, preserving every other field.
///
/// Pass a reflected binding constant to keep its shader-declared layout. WGSL
/// does not declare dynamic offsets; the surface chooses them when binding.
///
/// # Panics
///
/// Panics if `entry` is not a buffer binding. In a `const` initializer this is
/// a compile error.
pub const fn dynamic_offset(mut entry: wgpu::BindGroupLayoutEntry) -> wgpu::BindGroupLayoutEntry {
    match &mut entry.ty {
        wgpu::BindingType::Buffer {
            has_dynamic_offset, ..
        } => *has_dynamic_offset = true,
        _ => panic!("dynamic offsets require a buffer binding"),
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;

    #[test]
    fn a_uniform_entry_keeps_its_layout_in_a_const() {
        const ENTRY: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT.union(wgpu::ShaderStages::COMPUTE),
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(32),
            },
            count: None,
        };
        const DYNAMIC: wgpu::BindGroupLayoutEntry = dynamic_offset(ENTRY);
        assert_eq!(
            DYNAMIC,
            wgpu::BindGroupLayoutEntry {
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: NonZeroU64::new(32),
                },
                ..ENTRY
            }
        );
        assert_eq!(dynamic_offset(DYNAMIC), DYNAMIC);
    }

    #[test]
    fn storage_entries_keep_their_access_and_size() {
        for read_only in [true, false] {
            for min_binding_size in [None, NonZeroU64::new(64)] {
                let entry = wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only },
                        has_dynamic_offset: false,
                        min_binding_size,
                    },
                    count: None,
                };
                assert_eq!(
                    dynamic_offset(entry),
                    wgpu::BindGroupLayoutEntry {
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only },
                            has_dynamic_offset: true,
                            min_binding_size,
                        },
                        ..entry
                    }
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "dynamic offsets require a buffer binding")]
    fn a_sampler_is_refused() {
        dynamic_offset(wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
    }
}
