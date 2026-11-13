use super::*;
use objc2_foundation::NSBundle;

pub(super) fn install(
    device: &wgpu::Device,
    data: Option<&[u8]>,
) -> Option<Arc<wgpu::hal::metal::BinaryArchive>> {
    shader_libraries();
    // SAFETY: the guarded device remains owned by wgpu; this installs only its optional cache.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    match hal.install_binary_archive(data) {
        Ok(archive) => Some(archive),
        Err(error) => {
            eprintln!("game Metal pipeline cache unavailable: {error}");
            None
        }
    }
}

/// Point the backend at the shader libraries the build compiled into the
/// bundle (`exact-metal/`, host/apple/shader-libraries.mjs), and in a
/// development build where to record the MSL it compiles at run time
/// (`ExactMetalSources`). `EXACT_METAL_LIBRARIES` and `EXACT_METAL_SOURCES`
/// replace them for a controlled run.
fn shader_libraries() {
    let bundle = NSBundle::mainBundle();
    let libraries = std::env::var_os("EXACT_METAL_LIBRARIES")
        .map(PathBuf::from)
        .or_else(|| {
            bundle
                .resourcePath()
                .map(|p| PathBuf::from(p.to_string()).join("exact-metal"))
        });
    let sources = std::env::var_os("EXACT_METAL_SOURCES")
        .map(PathBuf::from)
        .or_else(|| {
            let key = objc2_foundation::NSString::from_str("ExactMetalSources");
            let value = bundle.objectForInfoDictionaryKey(&key)?;
            let path = value.downcast::<objc2_foundation::NSString>().ok()?;
            Some(PathBuf::from(path.to_string()))
        });
    wgpu::hal::metal::set_shader_libraries(libraries, sources);
}

pub(super) fn uninstall(
    device: &wgpu::Device,
    archive: Option<&Arc<wgpu::hal::metal::BinaryArchive>>,
) {
    if let Some(archive) = archive {
        // SAFETY: detach only our optional cache, leaving the guarded device intact.
        if let Some(hal) = unsafe { device.as_hal::<wgpu::hal::api::Metal>() } {
            hal.clear_binary_archive(archive);
        }
    }
}

pub(super) fn disk(device: &wgpu::Device, artifact: u64) -> Option<Disk> {
    let bundle = NSBundle::mainBundle();
    let app = std::env::var("EXACT_GPU_CACHE_KEY")
        .ok()
        .or_else(|| bundle.bundleIdentifier().map(|s| s.to_string()))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })?;
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Caches")))?
        .join(format!("exact-gpu-{:016x}", hash(app.as_bytes())));
    // SAFETY: read-only inspection of the guarded Metal device.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    let identity = hal.binary_archive_identity();
    let adapter = format!("metal-{:016x}", hash(identity.as_bytes()));
    let mut key = identity_key(app.as_bytes(), artifact, &device.adapter_info())
        .to_le_bytes()
        .to_vec();
    key.extend(identity.as_bytes());
    let path = cache_path(&dir, &adapter, artifact);
    let mut disk = Disk::new(path.clone(), hash(&key));
    let seed = std::env::var_os("EXACT_GPU_CACHE_SEED")
        .map(PathBuf::from)
        .or_else(|| {
            bundle
                .resourcePath()
                .map(|p| PathBuf::from(p.to_string()).join("exact-pipelines"))
        });
    disk.seed = seed.and_then(|p| path.file_name().map(|name| p.join(name)));
    Some(disk)
}

#[cfg(all(test, target_abi = "sim"))]
mod simulator {
    #[test]
    fn the_simulator_installs_no_archive() {
        let Some(gpu) = crate::fixture::device_or_skip(crate::fixture::device()) else {
            return;
        };
        assert!(super::install(&gpu.device, None).is_none());
    }
}

#[cfg(all(test, not(target_abi = "sim")))]
mod tests {
    use super::*;
    fn pipelines(gpu: &crate::Gpu, blue: bool) {
        let source = format!(
            r#"
            @vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {{
                let p=array(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.)); return vec4(p[i],0.,1.);
            }}
            @fragment fn fragment()->@location(0) vec4<f32> {{ return vec4(0.,{}, {},1.); }}
            @compute @workgroup_size(1) fn compute() {{}}
        "#,
            if blue { 0 } else { 1 },
            if blue { 1 } else { 0 }
        );
        let shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: None,
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fragment"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
        let compute = gpu
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &shader,
                entry_point: Some("compute"),
                compilation_options: Default::default(),
                cache: None,
            });
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&compute);
            pass.dispatch_workgroups(1, 1, 1);
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&pipeline);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([encoder.finish()]);
        let pixel = crate::fixture::read(gpu, &texture).unwrap().at(2, 2);
        assert_eq!(
            pixel,
            if blue {
                [0, 0, 255, 255]
            } else {
                [0, 255, 0, 255]
            }
        );
    }

    #[test]
    fn metal_archives_reuse_pipelines_across_processes_and_reject_stale_envelopes() {
        const CHILD: &str = "EXACT_METAL_ARCHIVE_TEST";
        if let Ok(mode) = std::env::var(CHILD) {
            let path = PathBuf::from(std::env::var_os("EXACT_METAL_ARCHIVE_PATH").unwrap());
            let mut disk = Disk::new(path.clone(), if mode == "wrong-key" { 2 } else { 1 });
            if mode == "seed" {
                disk.path = path.with_extension("seeded");
                disk.seed = Some(path.clone());
            }
            let gpu = crate::fixture::device().expect("Metal adapter required");
            assert_eq!(gpu.adapter.get_info().backend, wgpu::Backend::Metal);
            if mode == "reopen" {
                let standalone = PipelineCache::new(&gpu.device);
                pipelines(&gpu, true);
                drop(standalone);
            }
            let mut cache = PipelineCache::shared_disk(&gpu.device, Some(disk));
            let archive = cache
                .shared
                .as_ref()
                .unwrap()
                .metal
                .as_ref()
                .unwrap()
                .clone();
            assert_eq!(
                archive.restored(),
                matches!(mode.as_str(), "warm" | "shader" | "seed" | "reopen")
            );
            pipelines(&gpu, mode == "shader");
            let (hits, misses) = archive.counts();
            if matches!(mode.as_str(), "warm" | "seed" | "reopen") {
                assert_eq!(
                    (hits, misses),
                    (2, 0),
                    "both pipelines came from the archive"
                );
            } else if mode == "shader" {
                assert!(misses >= 1, "changed shader must miss: {hits} {misses}");
            } else {
                assert_eq!(misses, 2, "an empty archive misses both pipelines");
            }
            cache.save(2);
            for disk in &mut cache.shared.as_ref().unwrap().disk.lock().unwrap().disks {
                disk.reap(true);
                assert_eq!(disk.saved, 2);
            }
            if mode == "reopen" {
                cache.invalidate();
                pipelines(&gpu, true);
                assert_eq!(
                    archive.counts(),
                    (hits, misses),
                    "invalidated archive is detached"
                );
                cache.save(4);
                assert!(!path.exists(), "invalidated cache cannot be resurrected");
            }
            println!("Metal archive {mode}: hits={hits} misses={misses}");
            return;
        }
        let directory = super::super::tests::directory();
        let path = directory.join("archive.bin");
        let run = |mode: &str| {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "pipeline_cache::metal::tests::metal_archives_reuse_pipelines_across_processes_and_reject_stale_envelopes", "--nocapture"])
                .env(CHILD, mode).env("EXACT_METAL_ARCHIVE_PATH", &path).output().unwrap();
            assert!(
                result.status.success(),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
            eprint!("{}", String::from_utf8_lossy(&result.stdout));
        };
        run("cold");
        assert!(std::fs::metadata(&path).unwrap().len() > 24);
        run("warm");
        run("seed");
        run("shader");
        run("wrong-key");
        std::fs::write(&path, b"corrupt").unwrap();
        run("corrupt");
        std::fs::write(&path, pack(b"invalid Metal archive", 1)).unwrap();
        run("metal-corrupt");
        run("reopen");
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// A source the build compiled replaces the run-time compile: a run with no
    /// libraries records each MSL source it compiled; `metal` compiles them as
    /// host/apple/shader-libraries.mjs does; a run with those libraries
    /// compiles none and draws the same; a damaged library falls back.
    #[test]
    fn shipped_shader_libraries_replace_run_time_compiles() {
        const CHILD: &str = "EXACT_METAL_LIBRARY_TEST";
        if std::env::var_os(CHILD).is_some() {
            let gpu = crate::fixture::device().expect("Metal adapter required");
            let _cache = PipelineCache::new(&gpu.device);
            pipelines(&gpu, false);
            return;
        }
        let tool = std::process::Command::new("xcrun")
            .args(["-sdk", "macosx", "metal", "--version"])
            .output();
        if !tool.is_ok_and(|o| o.status.success()) {
            eprintln!("skipped: no Metal toolchain");
            return;
        }
        let directory = super::super::tests::directory();
        let run = |libraries: &str, sources: &str| {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "pipeline_cache::metal::tests::shipped_shader_libraries_replace_run_time_compiles", "--nocapture"])
                .env(CHILD, "1")
                .env("EXACT_METAL_LIBRARIES", directory.join(libraries))
                .env("EXACT_METAL_SOURCES", directory.join(sources))
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let dir = directory.join(sources);
            let mut keys: Vec<String> = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter_map(|e| {
                            e.file_name()
                                .to_str()?
                                .strip_suffix(".metal")
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default();
            keys.sort();
            keys
        };
        let recorded = run("none", "recorded");
        assert_eq!(
            recorded.len(),
            3,
            "a vertex, a fragment and a compute source: {recorded:?}"
        );
        let libraries = directory.join("libraries");
        std::fs::create_dir_all(&libraries).unwrap();
        for key in &recorded {
            let options =
                std::fs::read_to_string(directory.join("recorded").join(format!("{key}.json")))
                    .unwrap();
            let msl = options
                .split("\"msl\":\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap();
            let air = directory.join(format!("{key}.air"));
            for (tool, args) in [
                (
                    "metal",
                    vec![
                        format!("-std=metal{msl}"),
                        "-fpreserve-invariance".into(),
                        "-c".into(),
                        directory
                            .join("recorded")
                            .join(format!("{key}.metal"))
                            .display()
                            .to_string(),
                        "-o".into(),
                        air.display().to_string(),
                    ],
                ),
                (
                    "metallib",
                    vec![
                        air.display().to_string(),
                        "-o".into(),
                        libraries
                            .join(format!("{key}.metallib"))
                            .display()
                            .to_string(),
                    ],
                ),
            ] {
                let status = std::process::Command::new("xcrun")
                    .args(["-sdk", "macosx", tool])
                    .args(&args)
                    .status()
                    .unwrap();
                assert!(status.success(), "{tool} {args:?}");
            }
        }
        assert_eq!(
            run("libraries", "shipped"),
            Vec::<String>::new(),
            "every source came from a library"
        );
        for key in &recorded {
            std::fs::write(libraries.join(format!("{key}.metallib")), b"not a library").unwrap();
        }
        assert_eq!(
            run("libraries", "damaged"),
            recorded,
            "a damaged library falls back to the source"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }
}
