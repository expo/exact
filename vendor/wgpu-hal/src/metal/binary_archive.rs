//! EXACT: persistent Metal pipelines (EXACT-PATCHES.md, 15).
use super::Device;
use alloc::{string::String, sync::Arc, vec::Vec};
use core::sync::atomic::{AtomicU64, Ordering};
use objc2::{
    available,
    rc::{autoreleasepool, Retained},
    runtime::ProtocolObject,
};
use objc2_foundation::{NSArray, NSError, NSProcessInfo, NSString, NSURL};
use objc2_metal::{
    MTLBinaryArchive, MTLBinaryArchiveDescriptor, MTLComputePipelineDescriptor,
    MTLComputePipelineState, MTLDevice, MTLPipelineOption, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState,
};
use parking_lot::Mutex;
use std::{io, path::PathBuf};

/// An optional archive shared by this device's render and compute pipelines.
/// All archive use is serialized, including snapshots and compilation workers.
#[derive(Debug)]
pub struct BinaryArchive {
    raw: Mutex<Retained<ProtocolObject<dyn MTLBinaryArchive>>>,
    // Drop the archive before its device, including when a caller retains this handle.
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    restored: bool,
    _source: Option<Temporary>,
    hits: AtomicU64,
    misses: AtomicU64,
}
// Metal archives have no thread affinity. The mutex serializes every operation,
// including pipeline creation and serialization; the retained object never escapes.
unsafe impl Send for BinaryArchive {}
unsafe impl Sync for BinaryArchive {}
impl Device {
    /// Device, OS build and backend encoding identity for an archive envelope.
    pub fn binary_archive_identity(&self) -> String {
        format!(
            "exact-metal-archive-1:wgpu-hal-30:{}:{}:{}",
            self.shared.device.registryID(),
            self.shared.device.name(),
            NSProcessInfo::processInfo().operatingSystemVersionString()
        )
    }

    /// Install the device's optional pipeline archive. Invalid data falls back
    /// to an empty archive; unsupported systems return an error without changing it.
    pub fn install_binary_archive(
        &self,
        data: Option<&[u8]>,
    ) -> Result<Arc<BinaryArchive>, String> {
        if !available!(macos = 11.0, ios = 14.0, tvos = 14.0, visionos = 1.0) {
            return Err("Metal binary archives require macOS 11 or iOS/tvOS 14".into());
        }
        let mut installed = self.binary_archive.lock();
        let archive = autoreleasepool(|_| {
            let descriptor = MTLBinaryArchiveDescriptor::new();
            let loaded = data.and_then(|bytes| {
                let temporary = Temporary::new().ok()?;
                std::fs::write(&temporary.path, bytes).ok()?;
                descriptor.setUrl(Some(&temporary.url()));
                let result = self
                    .shared
                    .device
                    .newBinaryArchiveWithDescriptor_error(&descriptor)
                    .ok();
                descriptor.setUrl(None);
                result.map(|raw| (raw, temporary))
            });
            let restored = loaded.is_some();
            let (raw, source) = match loaded {
                Some((raw, source)) => (raw, Some(source)),
                None => (
                    self.shared
                        .device
                        .newBinaryArchiveWithDescriptor_error(&descriptor)
                        .map_err(|e| format!("Metal binary archive: {e}"))?,
                    None,
                ),
            };
            Ok::<_, String>(Arc::new(BinaryArchive {
                raw: Mutex::new(raw),
                _device: self.shared.device.clone(),
                restored,
                _source: source,
                hits: AtomicU64::new(0),
                misses: AtomicU64::new(0),
            }))
        })?;
        *installed = Some(Arc::clone(&archive));
        Ok(archive)
    }

    /// Remove this owner's archive without detaching a newer installation.
    pub fn clear_binary_archive(&self, archive: &Arc<BinaryArchive>) {
        let mut installed = self.binary_archive.lock();
        if installed
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, archive))
        {
            *installed = None;
        }
    }
}
impl BinaryArchive {
    /// Whether initialization loaded a serialized archive successfully.
    pub fn restored(&self) -> bool {
        self.restored
    }
    /// Successful archive lookups and misses, across render and compute pipelines.
    pub fn counts(&self) -> (u64, u64) {
        (
            self.hits.load(Ordering::Relaxed),
            self.misses.load(Ordering::Relaxed),
        )
    }
    /// Snapshot bytes while excluding pipeline additions. Disk ownership and
    /// atomic replacement of the persistent envelope belong to the caller.
    pub fn data(&self) -> Option<Vec<u8>> {
        autoreleasepool(|_| {
            let temporary = Temporary::new().ok()?;
            self.raw
                .lock()
                .serializeToURL_error(&temporary.url())
                .ok()?;
            std::fs::read(&temporary.path).ok()
        })
    }
    pub(super) fn render(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        descriptor: &MTLRenderPipelineDescriptor,
    ) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>, Retained<NSError>> {
        let archive = self.raw.lock();
        let archives = NSArray::from_slice(&[&**archive]);
        descriptor.setBinaryArchives(Some(&archives));
        let cached = device.newRenderPipelineStateWithDescriptor_options_reflection_error(
            descriptor,
            MTLPipelineOption::FailOnBinaryArchiveMiss,
            None,
        );
        if cached.is_ok() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return cached;
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = archive.addRenderPipelineFunctionsWithDescriptor_error(descriptor) {
            log::warn!("Metal render archive addition failed: {error}");
            descriptor.setBinaryArchives(None);
        }
        device.newRenderPipelineStateWithDescriptor_error(descriptor)
    }
    pub(super) fn compute(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        descriptor: &MTLComputePipelineDescriptor,
    ) -> Result<Retained<ProtocolObject<dyn MTLComputePipelineState>>, Retained<NSError>> {
        let archive = self.raw.lock();
        let archives = NSArray::from_slice(&[&**archive]);
        descriptor.setBinaryArchives(Some(&archives));
        let cached = device.newComputePipelineStateWithDescriptor_options_reflection_error(
            descriptor,
            MTLPipelineOption::FailOnBinaryArchiveMiss,
            None,
        );
        if cached.is_ok() {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return cached;
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        if let Err(error) = archive.addComputePipelineFunctionsWithDescriptor_error(descriptor) {
            log::warn!("Metal compute archive addition failed: {error}");
            descriptor.setBinaryArchives(None);
        }
        device.newComputePipelineStateWithDescriptor_options_reflection_error(
            descriptor,
            MTLPipelineOption::empty(),
            None,
        )
    }
}

// Metal accepts URLs rather than in-memory archive bytes. An exclusively created
// directory keeps each transfer private to this process and is removed on every exit.
#[derive(Debug)]
struct Temporary {
    directory: PathBuf,
    path: PathBuf,
}
impl Temporary {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..16 {
            let directory = std::env::temp_dir().join(format!(
                "exact-metal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    return Ok(Self {
                        path: directory.join("archive.metallib"),
                        directory,
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::other("Metal archive temporary paths occupied"))
    }
    fn url(&self) -> Retained<NSURL> {
        NSURL::fileURLWithPath(&NSString::from_str(&self.path.to_string_lossy()))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let _ = std::fs::remove_dir(&self.directory);
    }
}
