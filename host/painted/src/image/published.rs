//! A package's decoded file, charged to the raster cache and verified by a reader.
use super::{png_decode, workers::Backend, workers::SourceOwner, Bitmap};
use exact_raster::{Crop, DecodeCost, Demand, Metadata, Priority, RasterKey, ViewKey};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, Weak};
use tiny_skia::Pixmap;

/// The decoded, premultiplied sRGB pixels of an encoded image file.
pub struct PublishedImage {
    pixels: Pixmap,
    digest: [u8; 32],
    metadata: Metadata,
}

impl PublishedImage {
    /// Pair pixels with the exact bytes the package wrote. A reader checks the
    /// file again before using them. For lossy files supply the decoded output.
    pub fn new(encoded: &[u8], pixels: Pixmap) -> Option<Self> {
        let metadata = png_decode::inspect(&mut Cursor::new(encoded), encoded.len() as u64)
            .ok()?
            .metadata;
        if (pixels.width(), pixels.height()) != (metadata.natural.width, metadata.natural.height) {
            return None;
        }
        Some(Self {
            pixels,
            digest: Sha256::digest(encoded).into(),
            metadata,
        })
    }
}

#[derive(Clone)]
pub(super) struct Published {
    demand: Demand,
    digest: [u8; 32],
    pixels: Weak<Bitmap>,
}

pub(super) type Files = Mutex<BTreeMap<u64, BTreeMap<String, Published>>>;

/// A file's length and modification time.
pub(super) type Stamp = (u64, std::time::SystemTime);

fn stamp(source: &SourceOwner) -> Option<Stamp> {
    let metadata = std::fs::metadata(source.file()?).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

/// Published pixels a reader matched to the file it opened.
pub(super) struct Verified {
    demand: Demand,
    stamp: Stamp,
}

impl Verified {
    /// The pixels' cache demand, unless the file changed since it was hashed.
    pub(super) fn current(&self, source: &SourceOwner) -> Option<Demand> {
        (stamp(source)? == self.stamp).then_some(self.demand)
    }
}

/// Match `source`'s file, just opened as `input` with its stamp, to the
/// pixels published for it: the same bytes, by digest. A worker reads the
/// whole file once.
pub(super) fn verify(
    files: &Files,
    source: &SourceOwner,
    (mut input, opened): (impl Read + Seek, Option<Stamp>),
    metadata: Metadata,
) -> Option<Verified> {
    if !source.name().starts_with("app:/") {
        return None;
    }
    let published = (files.lock().unwrap().get(&source.generation()))
        .and_then(|files| files.get(source.name()))
        .cloned()?;
    if published.demand.metadata != metadata || published.pixels.strong_count() == 0 {
        return None;
    }
    // The opened file's: a file renamed onto the path since has another.
    let stamp = opened?;
    input.seek(SeekFrom::Start(0)).ok()?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut file = input.take(metadata.encoded_bytes.checked_add(1)?);
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        total += n as u64;
        hash.update(&buffer[..n]);
    }
    (total == metadata.encoded_bytes && <[u8; 32]>::from(hash.finalize()) == published.digest)
        .then_some(Verified {
            demand: published.demand,
            stamp,
        })
}

/// The receiver for one generation's file publications. Package workers hold it weakly.
pub(crate) struct Publisher {
    backend: Weak<Backend>,
    generation: u64,
    cache_generation: u64,
}

impl Publisher {
    pub(super) fn new(backend: &Arc<Backend>, generation: u64) -> Arc<Self> {
        Arc::new(Self {
            backend: Arc::downgrade(backend),
            generation,
            cache_generation: backend.generation(),
        })
    }

    pub(crate) fn offer(&self, name: &str, image: PublishedImage) {
        if !name.starts_with("app:/") || name.len() > super::source_limit(name) {
            return;
        }
        let Some(backend) = self.backend.upgrade() else {
            return;
        };
        let PublishedImage {
            pixels,
            digest,
            metadata,
        } = image;
        let natural = metadata.natural;
        let Ok(cost) = DecodeCost::checked(u64::from(natural.width) * 4, natural.height, 0, 0)
        else {
            return;
        };
        let demand = Demand {
            view: ViewKey {
                view: 0,
                generation: self.cache_generation,
            },
            key: RasterKey {
                generation: self.cache_generation,
                source: backend.generation(),
                pixels: natural,
                crop: Crop::default(),
                variant: exact_raster::variant::SRGB8,
            },
            metadata,
            cost,
            priority: Priority::Visible,
        };
        let mut weak = Weak::new();
        let stored = backend.session.publish(demand, |charge| {
            let opaque = pixels.pixels().iter().all(|p| p.is_opaque());
            let mut bitmap = Bitmap::decoded(pixels, (natural.width, natural.height), charge);
            bitmap.opaque_for_canvas = opaque;
            let bitmap = Arc::new(bitmap);
            weak = Arc::downgrade(&bitmap);
            bitmap
        });
        if stored.is_ok() {
            let mut all = backend.published.lock().unwrap();
            let files = all.entry(self.generation).or_default();
            files.retain(|_, entry| entry.pixels.strong_count() > 0);
            files.insert(
                name.to_owned(),
                Published {
                    demand,
                    digest,
                    pixels: weak,
                },
            );
        }
    }
}
impl Drop for Publisher {
    fn drop(&mut self) {
        if let Some(backend) = self.backend.upgrade() {
            backend.published.lock().unwrap().remove(&self.generation);
            backend.session.retire_generation(self.cache_generation);
        }
    }
}
