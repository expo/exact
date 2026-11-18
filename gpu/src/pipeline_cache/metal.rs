use super::*;
use objc2_foundation::NSBundle;

/// The file beside an archive's that is there only while a pipeline is added to
/// the archive restored from it (`BinaryArchive::mark_additions`).
pub(super) fn marker(path: &Path) -> PathBuf {
    path.with_extension("adding")
}

/// Install the device's archive, restored from `data`, with `marker` as its
/// additions' marker (`BinaryArchive::mark_additions`).
pub(super) fn install(
    device: &wgpu::Device,
    data: Option<&[u8]>,
    marker: Option<PathBuf>,
) -> Option<Arc<wgpu::hal::metal::BinaryArchive>> {
    shader_libraries();
    // SAFETY: the guarded device remains owned by wgpu; this installs only its optional cache.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    match hal.install_binary_archive(data) {
        Ok(archive) => {
            if let Some(marker) = marker {
                archive.mark_additions(marker);
            }
            Some(archive)
        }
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
    // A device that keeps no archive reads none and writes none; one an earlier
    // build wrote for it goes.
    if hal.binary_archive_unavailable().is_some() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(marker(&path));
        return None;
    }
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
        assert!(super::install(&gpu.device, None, None).is_none());
    }
}

#[cfg(all(test, not(target_abi = "sim")))]
mod tests {
    use super::*;

    #[link(name = "QuartzCore", kind = "framework")]
    extern "C" {}

    fn command(test: &str, directory: &Path) -> std::process::Command {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test, "--nocapture"])
            .env("XDG_CACHE_HOME", directory)
            .env("EXACT_GPU_CACHE_KEY", "metal-archive-test")
            .env("EXACT_GPU_CACHE_SEED", directory.join("no-seed"))
            .env("EXACT_METAL_LIBRARIES", directory.join("no-libraries"))
            .env("EXACT_METAL_SOURCES", directory.join("sources"));
        command
    }

    /// A Metal device that keeps archives (not a virtual machine's, `binary_archive_unavailable`).
    fn archive_device() -> Option<crate::Gpu> {
        let gpu = crate::fixture::device_or(crate::fixture::device(), false)?;
        // SAFETY: read-only inspection of the guarded Metal device.
        let keeps = unsafe { gpu.device.as_hal::<wgpu::hal::api::Metal>() }
            .is_some_and(|hal| hal.binary_archive_unavailable().is_none());
        keeps.then_some(gpu)
    }

    fn isolated(test: &str) -> bool {
        const CHILD: &str = "EXACT_METAL_MODULE_TEST";
        if std::env::var(CHILD).as_deref() == Ok(test) {
            return false;
        }
        if archive_device().is_none() {
            return true;
        }
        let directory = super::super::tests::directory();
        let result = command(test, &directory).env(CHILD, test).output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        eprint!("{}", String::from_utf8_lossy(&result.stdout));
        true
    }

    fn module(gpu: crate::Gpu, disk: Disk) -> crate::Module {
        static REGISTRY: crate::Registry = crate::Registry {
            surfaces: &[("triangle", 0, || Box::new(Triangle))],
            shaders: &[],
        };
        let cache = PipelineCache::shared_disk(&gpu.device, Some(disk));
        let mut module = crate::Module::new(&REGISTRY);
        module.set_gpu(gpu);
        module.pipeline_cache = cache;
        module
    }

    struct Triangle;
    impl crate::Surface for Triangle {
        fn bind(&mut self, _: &[crate::Value], _: Option<f64>) -> Result<(), crate::SurfaceError> {
            Ok(())
        }

        fn render(
            &mut self,
            _: &crate::Frame,
            device: &wgpu::Device,
            _: &wgpu::Queue,
            encoder: &mut wgpu::CommandEncoder,
            view: &wgpu::TextureView,
            format: wgpu::TextureFormat,
        ) -> bool {
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(
                    r#"
                    @vertex fn vertex(@builtin(vertex_index) i:u32)->@builtin(position) vec4<f32> {
                        let p=array(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.)); return vec4(p[i],0.,1.);
                    }
                    @fragment fn fragment()->@location(0) vec4<f32> { return vec4(0.,1.,0.,1.); }
                    "#.into(),
                ),
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
                    targets: &[Some(format.into())],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            false
        }
    }

    fn frame() -> crate::Frame {
        crate::Frame {
            width: 4.,
            height: 4.,
            scale: 1.,
            now_ms: 0.,
            seekable: true,
            period_ms: 0.,
            children_generation: 0,
            shader_generation: 0,
            headroom: 1.,
        }
    }

    fn written(module: &crate::Module) {
        let shared = module.pipeline_cache.shared.as_ref().unwrap();
        assert_eq!(shared.metal.as_ref().unwrap().counts().1, 1);
        let mut state = shared.disk.lock().unwrap();
        let disk = &mut state.disks[0];
        disk.reap(true);
        assert_eq!(disk.saved, 1, "the module must save after submitting");
        assert_eq!(disk.attempts, 1);
        let bytes = std::fs::read(&disk.path).unwrap();
        assert!(!unpack(&bytes, disk.key).unwrap().is_empty());
    }

    #[test]
    fn flush_writes_the_module_metal_archive() {
        if isolated("pipeline_cache::metal::tests::flush_writes_the_module_metal_archive") {
            return;
        }
        let gpu = crate::fixture::device().expect("Metal adapter required");
        let directory = super::super::tests::directory();
        // SAFETY: the layer is retained until after the module is dropped.
        let layer: objc2::rc::Retained<objc2::runtime::AnyObject> =
            unsafe { objc2::msg_send![objc2::class!(CAMetalLayer), new] };
        let mut module = module(gpu, Disk::new(directory.join("archive.bin"), 1));
        module.set_seekable(true);
        let ptr = objc2::rc::Retained::as_ptr(&layer) as *mut std::ffi::c_void;
        // SAFETY: the layer outlives the module and the surface it owns.
        let target = unsafe {
            module
                .gpu()
                .unwrap()
                .instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(ptr))
        }
        .unwrap();
        let id = module.create("triangle", target, 4, 4).unwrap();
        assert!(module.bind(id, &[], None));
        for _ in 0..2 {
            assert_eq!(module.render(id, &frame()), Some(false));
            assert!(module.open.is_some());
            assert!(module.flush());
            assert!(module.open.is_none());
            written(&module);
        }
        drop(module);
    }

    #[test]
    fn readback_writes_the_module_metal_archive_without_an_open_frame() {
        if isolated("pipeline_cache::metal::tests::readback_writes_the_module_metal_archive_without_an_open_frame") {
            return;
        }
        let gpu = crate::fixture::device().expect("Metal adapter required");
        let directory = super::super::tests::directory();
        let mut module = module(gpu, Disk::new(directory.join("archive.bin"), 1));
        let id = module.create_headless("triangle").unwrap();
        assert!(module.bind(id, &[], None));
        for _ in 0..2 {
            assert!(module.open.is_none());
            let mut output = [0; 4 * 4 * 4];
            assert_eq!(module.readback_into(id, &frame(), &mut output), Some(false));
            assert!(output
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == [0, 255, 0, 255]));
            assert!(module.open.is_none());
            written(&module);
        }
    }

    /// A snapshot's file lives as long as its archive, so a killed process
    /// leaves it; the next process to use an archive removes what the dead
    /// left and nothing of the living.
    #[test]
    fn the_next_process_removes_a_dead_process_snapshot_files() {
        const CHILD: &str = "EXACT_METAL_SWEEP_TEST";
        if std::env::var_os(CHILD).is_some() {
            let Some(gpu) = archive_device() else {
                return;
            };
            let root = std::env::temp_dir().join("exact-metal");
            let left = |name: &str| {
                let directory = root.join(name);
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(directory.join("0.metallib"), b"left").unwrap();
                (
                    std::fs::File::create(directory.join("lock")).unwrap(),
                    directory,
                )
            };
            let (_, dead) = left("dead");
            let (lock, living) = left("living");
            lock.try_lock().unwrap();
            let archive = install(&gpu.device, None, None).unwrap();
            pipelines(&gpu, false);
            assert!(archive.data().is_some());
            assert!(!dead.exists());
            assert!(living.join("0.metallib").exists());
            return;
        }
        if archive_device().is_none() {
            return;
        }
        let directory = super::super::tests::directory();
        let result = command(
            "pipeline_cache::metal::tests::the_next_process_removes_a_dead_process_snapshot_files",
            &directory,
        )
        .env(CHILD, "1")
        .env("TMPDIR", &*directory)
        .output()
        .unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    /// Metal writes an archive that gained nothing by copying the file of its
    /// last write that had additions; a snapshot must not depend on a file an
    /// earlier snapshot removed.
    #[test]
    fn repeated_snapshots_of_an_unchanged_metal_archive_succeed() {
        if isolated(
            "pipeline_cache::metal::tests::repeated_snapshots_of_an_unchanged_metal_archive_succeed",
        ) {
            return;
        }
        let Some(gpu) = archive_device() else {
            return;
        };
        let directory = super::super::tests::directory();
        let module = module(gpu, Disk::new(directory.join("archive.bin"), 1));
        let gpu = module.gpu.as_ref().unwrap();
        let shared = module.pipeline_cache.shared.as_ref().unwrap();
        let archive = shared.metal.as_ref().unwrap();
        for blue in [false, true] {
            pipelines(gpu, blue);
            let first = archive.data().expect("a snapshot after additions");
            for _ in 0..3 {
                assert_eq!(archive.data().as_ref(), Some(&first));
            }
        }
    }

    #[test]
    fn module_and_renderer_save_one_metal_archive_generation() {
        if isolated(
            "pipeline_cache::metal::tests::module_and_renderer_save_one_metal_archive_generation",
        ) {
            return;
        }
        let Some(gpu) = archive_device() else {
            return;
        };
        let directory = super::super::tests::directory();
        let module = module(gpu, Disk::new(directory.join("archive.bin"), 1));
        let gpu = module.gpu.as_ref().unwrap();
        let mut renderer = PipelineCache::new(&gpu.device);
        let shared = module.pipeline_cache.shared.as_ref().unwrap();
        let archive = shared.metal.as_ref().unwrap();
        pipelines(gpu, false);
        module.pipeline_cache.save_metal();
        {
            let mut state = shared.disk.lock().unwrap();
            let disk = &mut state.disks[0];
            disk.reap(true);
            assert_eq!(disk.saved, archive.counts().1);
            assert_eq!(disk.saved, 2);
        }
        renderer.save(1000);
        {
            let state = shared.disk.lock().unwrap();
            assert_eq!(state.disks[0].work, archive.counts().1);
            assert!(state.disks[0].writer.is_none(), "one write per addition");
        }
        pipelines(gpu, true);
        renderer.save(1001);
        {
            let mut state = shared.disk.lock().unwrap();
            state.disks[0].reap(true);
            assert_eq!(state.disks[0].saved, archive.counts().1);
        }
        module.pipeline_cache.save_metal();
        let state = shared.disk.lock().unwrap();
        assert_eq!(state.disks[0].work, archive.counts().1);
        assert!(state.disks[0].writer.is_none(), "renderer may save first");
    }

    #[test]
    fn module_save_without_new_metal_additions_starts_no_writer() {
        if isolated("pipeline_cache::metal::tests::module_save_without_new_metal_additions_starts_no_writer") {
            return;
        }
        let Some(gpu) = archive_device() else {
            return;
        };
        let directory = super::super::tests::directory();
        let path = directory.join("archive.bin");
        let module = module(gpu, Disk::new(path.clone(), 1));
        let gpu = module.gpu.as_ref().unwrap();
        let empty_path = directory.join("late-empty.bin");
        let _empty =
            PipelineCache::shared_disk(&gpu.device, Some(Disk::new(empty_path.clone(), 2)));
        module.pipeline_cache.save_metal();
        let shared = module.pipeline_cache.shared.as_ref().unwrap();
        {
            let state = shared.disk.lock().unwrap();
            for disk in &state.disks {
                assert_eq!((disk.work, disk.saved, disk.attempts), (0, 0, 0));
                assert!(disk.writer.is_none());
                assert!(!disk.path.exists(), "an empty archive needs no snapshot");
            }
        }
        pipelines(gpu, false);
        module.pipeline_cache.save_metal();
        {
            let mut state = shared.disk.lock().unwrap();
            for disk in &mut state.disks {
                disk.reap(true);
                assert_eq!(disk.saved, 2);
                assert_eq!(disk.attempts, 1);
            }
        }
        assert!(!unpack(&std::fs::read(&empty_path).unwrap(), 2)
            .unwrap()
            .is_empty());
        let added_path = directory.join("late-added.bin");
        let _added =
            PipelineCache::shared_disk(&gpu.device, Some(Disk::new(added_path.clone(), 3)));
        module.pipeline_cache.save_metal();
        shared.disk.lock().unwrap().disks[2].reap(true);
        assert!(!unpack(&std::fs::read(&added_path).unwrap(), 3)
            .unwrap()
            .is_empty());
        for _ in 0..65 {
            module.pipeline_cache.save_metal();
            let state = shared.disk.lock().unwrap();
            for disk in &state.disks {
                assert!(disk.writer.is_none());
                assert_eq!(disk.attempts, 1);
                assert_eq!(disk.saved, 2);
            }
        }
        module.pipeline_cache.invalidate();
        module.pipeline_cache.save_metal();
        assert!(
            !path.exists(),
            "module saves cannot resurrect a lost device"
        );
    }

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
            if matches!(mode.as_str(), "seed" | "late-seed") {
                disk.path = path.with_extension("seeded");
                disk.seed = Some(path.clone());
            }
            let gpu = crate::fixture::device().expect("Metal adapter required");
            assert_eq!(gpu.adapter.get_info().backend, wgpu::Backend::Metal);
            if mode == "stopped-idle" {
                // A launch that discards a stopped archive and ends before writing one.
                let module = module(gpu, disk);
                let archive = module.pipeline_cache.shared.as_ref().unwrap().metal.clone();
                assert!(!archive.unwrap().restored());
                assert!(!path.exists() && marker(&path).exists());
                return;
            }
            if mode == "reopen" {
                let standalone = PipelineCache::new(&gpu.device);
                pipelines(&gpu, true);
                drop(standalone);
            }
            let module = module(gpu, disk);
            let cache = &module.pipeline_cache;
            let gpu = module.gpu.as_ref().unwrap();
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
                matches!(
                    mode.as_str(),
                    "warm" | "shader" | "seed" | "reopen" | "late" | "late-seed" | "refused"
                )
            );
            let late_path = path.with_extension(format!("{mode}-late"));
            let late = matches!(mode.as_str(), "late" | "late-seed").then(|| {
                PipelineCache::shared_disk(&gpu.device, Some(Disk::new(late_path.clone(), 2)))
            });
            if mode == "refused" {
                // Metal refusing to serialize the restored archive after an addition:
                // this process's snapshot directory is made unwritable.
                use std::os::unix::fs::PermissionsExt;
                let own = std::env::temp_dir()
                    .join("exact-metal")
                    .join(std::process::id().to_string());
                let set =
                    |mode| std::fs::set_permissions(&own, std::fs::Permissions::from_mode(mode));
                set(0o500).unwrap();
                pipelines(gpu, true);
                cache.save_metal();
                let mut state = cache.shared.as_ref().unwrap().disk.lock().unwrap();
                state.disks[0].reap(true);
                set(0o700).unwrap();
                assert_eq!(state.disks[0].saved, 0);
                assert!(
                    !path.exists(),
                    "an archive that cannot be written again goes"
                );
                return;
            }
            pipelines(gpu, mode == "shader");
            let (hits, misses) = archive.counts();
            assert_eq!(
                archive.additions(),
                misses,
                "every missed pipeline was added"
            );
            if matches!(
                mode.as_str(),
                "warm" | "seed" | "reopen" | "late" | "late-seed"
            ) {
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
            cache.save_metal();
            for disk in &mut cache.shared.as_ref().unwrap().disk.lock().unwrap().disks {
                disk.reap(true);
                let expected = if disk.path == late_path { 1 } else { misses };
                assert_eq!(disk.saved, expected);
                assert_eq!(disk.attempts, u8::from(expected != 0));
            }
            if let Some(late) = late {
                let bytes = std::fs::read(&late_path).unwrap();
                assert!(!unpack(&bytes, 2).unwrap().is_empty());
                for _ in 0..65 {
                    late.save_metal();
                    let state = late.shared.as_ref().unwrap().disk.lock().unwrap();
                    assert!(state.disks.iter().all(|disk| disk.writer.is_none()));
                    assert_eq!(state.disks[1].attempts, 1);
                }
            }
            if mode == "reopen" {
                cache.invalidate();
                pipelines(gpu, true);
                assert_eq!(
                    archive.counts(),
                    (hits, misses),
                    "invalidated archive is detached"
                );
                cache.save_metal();
                assert!(!path.exists(), "invalidated cache cannot be resurrected");
            }
            println!(
                "Metal archive {mode}: hits={hits} misses={misses} additions={}",
                archive.additions()
            );
            return;
        }
        if archive_device().is_none() {
            return;
        }
        let directory = super::super::tests::directory();
        let path = directory.join("archive.bin");
        let run = |mode: &str| {
            let result = command("pipeline_cache::metal::tests::metal_archives_reuse_pipelines_across_processes_and_reject_stale_envelopes", &directory)
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
        run("late");
        run("seed");
        run("late-seed");
        run("shader");
        let marker = marker(&path);
        assert!(
            !marker.exists(),
            "an addition to a restored archive leaves no marker"
        );
        // A launch that stopped inside an addition: the next ones do not restore that
        // archive, and its marker stays until one of them writes a new archive.
        std::fs::write(&marker, b"").unwrap();
        run("stopped-idle");
        run("stopped");
        assert!(!marker.exists());
        run("warm");
        run("refused");
        run("cold");
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
