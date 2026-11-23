//! A package's decoded file, charged to the raster cache, and read while the
//! file is the one the package wrote.
use super::{png_decode, workers::Backend, Bitmap};
use exact_raster::{Crop, DecodeCost, Demand, Metadata, Priority, RasterKey, ViewKey};
use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::{Arc, Mutex, Weak};
use tiny_skia::Pixmap;

/// The decoded, premultiplied sRGB pixels of an encoded image file.
pub struct PublishedImage {
    pixels: Pixmap,
    metadata: Metadata,
}

impl PublishedImage {
    /// Pair pixels with the bytes the package wrote to its file. For a lossy
    /// file supply what decoding it gives.
    pub fn new(encoded: &[u8], pixels: Pixmap) -> Option<Self> {
        let metadata = png_decode::inspect(&mut Cursor::new(encoded), encoded.len() as u64)
            .ok()?
            .metadata;
        if (pixels.width(), pixels.height()) != (metadata.natural.width, metadata.natural.height) {
            return None;
        }
        Some(Self { pixels, metadata })
    }
}

#[derive(Clone)]
pub(super) struct Published {
    demand: Demand,
    stamp: Stamp,
    pixels: Weak<Bitmap>,
}

pub(super) type Files = Mutex<BTreeMap<u64, BTreeMap<String, Published>>>;

/// A file's length and modification time.
type Stamp = (u64, std::time::SystemTime);

/// Read without opening the file: an open of a file just written waits on a
/// virus scan on Windows (9 ms a capture's pair on the NucBox), and a link
/// put in its place has stamps of its own.
fn stamp(name: &str) -> Option<Stamp> {
    let metadata = std::fs::symlink_metadata(crate::picker::resolve(name)?).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

/// The cached pixels published for `name` in image `generation`, while its
/// file still has the length and modification time it had when the package
/// published it: a file replaced or deleted since is read as it is. One
/// `stat`, so a reader takes them as it asks, with no worker's turn; a
/// rewrite that keeps both (a restored modification time, or one within a
/// FAT volume's two-second step) is not told apart.
pub(super) fn current(files: &Files, generation: u64, name: &str) -> Option<Demand> {
    if !name.starts_with("app:/") {
        return None;
    }
    let published = (files.lock().unwrap().get(&generation))
        .and_then(|files| files.get(name))
        .cloned()?;
    (published.pixels.strong_count() > 0 && stamp(name)? == published.stamp)
        .then_some(published.demand)
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
        let PublishedImage { pixels, metadata } = image;
        // Taken as the file is published, before any reader knows its path.
        let Some(stamp) = stamp(name) else {
            return;
        };
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
                    stamp,
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
