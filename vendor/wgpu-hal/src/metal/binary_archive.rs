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
use std::{io, path::PathBuf, sync::OnceLock};

/// An optional archive shared by this device's render and compute pipelines.
/// All archive use is serialized, including snapshots and compilation workers.
#[derive(Debug)]
pub struct BinaryArchive {
    raw: Mutex<Retained<ProtocolObject<dyn MTLBinaryArchive>>>,
    // Drop the archive before its device, including when a caller retains this handle.
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    restored: bool,
    // The file of the last serialization or, before one, the file restored from,
    // with the count of additions it holds.
    snapshot: Mutex<Option<(u64, Temporary)>>,
    hits: AtomicU64,
    misses: AtomicU64,
    additions: AtomicU64,
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
        // The simulator's Metal aborts the process serializing an archive
        // (`MTLReportFailure` under `-[_MTLBinaryArchive legacySerializeToURL:options:error:]`,
        // iOS 27 simulator, 2026-11-12).
        if cfg!(target_abi = "sim") {
            return Err("the simulator has no Metal binary archives".into());
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
                snapshot: Mutex::new(source.map(|source| (0, source))),
                hits: AtomicU64::new(0),
                misses: AtomicU64::new(0),
                additions: AtomicU64::new(0),
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
    /// Successful additions, across render and compute pipelines.
    pub fn additions(&self) -> u64 {
        self.additions.load(Ordering::Relaxed)
    }
    /// Snapshot bytes while excluding pipeline additions. Disk ownership and
    /// atomic replacement of the persistent envelope belong to the caller.
    pub fn data(&self) -> Option<Vec<u8>> {
        autoreleasepool(|_| {
            let raw = self.raw.lock();
            // Counted under the archive's lock, held here.
            let additions = self.additions.load(Ordering::Relaxed);
            let mut snapshot = self.snapshot.lock();
            // Metal serializes an archive that gained nothing since its last
            // serialization by moving that file, and then fails every later
            // attempt until an addition (macOS 27.2). An unchanged archive is
            // read from the kept file instead, which for a restored archive is
            // the file it was restored from.
            if snapshot.as_ref().is_none_or(|(at, _)| *at != additions) {
                let temporary = Temporary::new().ok()?;
                raw.serializeToURL_error(&temporary.url()).ok()?;
                *snapshot = Some((additions, temporary));
            }
            std::fs::read(&snapshot.as_ref()?.1.path).ok()
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
        match archive.addRenderPipelineFunctionsWithDescriptor_error(descriptor) {
            Ok(()) => {
                self.additions.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                log::warn!("Metal render archive addition failed: {error}");
                descriptor.setBinaryArchives(None);
            }
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
        match archive.addComputePipelineFunctionsWithDescriptor_error(descriptor) {
            Ok(()) => {
                self.additions.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => {
                log::warn!("Metal compute archive addition failed: {error}");
                descriptor.setBinaryArchives(None);
            }
        }
        device.newComputePipelineStateWithDescriptor_options_reflection_error(
            descriptor,
            MTLPipelineOption::empty(),
            None,
        )
    }
}

// Metal accepts URLs rather than in-memory archive bytes. A snapshot's file lives
// as long as its archive, and a killed process, the usual end of an iOS app,
// removes nothing. So each process keeps its files in a directory of its own
// beside a lock it holds for life, and the first use in a process removes the
// directories whose lock is free.
#[derive(Debug)]
struct Temporary {
    path: PathBuf,
}
impl Temporary {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let directory =
            directory().ok_or_else(|| io::Error::other("no Metal archive directory"))?;
        Ok(Self {
            path: directory.join(format!(
                "{}.metallib",
                NEXT.fetch_add(1, Ordering::Relaxed)
            )),
        })
    }
    fn url(&self) -> Retained<NSURL> {
        NSURL::fileURLWithPath(&NSString::from_str(&self.path.to_string_lossy()))
    }
}
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn directory() -> Option<&'static PathBuf> {
    static DIRECTORY: OnceLock<Option<(PathBuf, std::fs::File)>> = OnceLock::new();
    DIRECTORY
        .get_or_init(|| {
            let root = std::env::temp_dir().join("exact-metal");
            let own = root.join(format!("{}", std::process::id()));
            // An earlier process of this id is gone.
            let _ = std::fs::remove_dir_all(&own);
            std::fs::create_dir_all(&own).ok()?;
            let lock = std::fs::File::create(own.join("lock")).ok()?;
            lock.try_lock().ok()?;
            for entry in std::fs::read_dir(&root).ok()?.flatten() {
                let other = entry.path();
                let free = other != own
                    && std::fs::File::open(other.join("lock"))
                        .is_ok_and(|lock| lock.try_lock().is_ok());
                if free {
                    let _ = std::fs::remove_dir_all(&other);
                }
            }
            Some((own, lock))
        })
        .as_ref()
        .map(|(own, _)| own)
}
