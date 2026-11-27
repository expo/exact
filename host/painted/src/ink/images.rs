use super::*;
use exact_ink::device::TextureDesc;
use std::collections::{HashMap, VecDeque};
use std::sync::Weak;

const IDLE_BYTES: u64 = 8 << 20;
const IDLE_BITMAPS: usize = 64;

#[derive(Default)]
pub(super) struct Images {
    bitmaps: HashMap<(usize, [u32; 2]), (Arc<Bitmap>, TextureId)>,
    idle: IdleBitmaps,
    pixmaps: HashMap<usize, Uploaded>,
    /// A recorded revision keeps its texture without holding the CPU pixels.
    frames: HashMap<(u64, bool), TextureId>,
    /// Textures whose pixmap no one holds any more, kept for one frame to
    /// take a new pixmap of the same size in place: a canvas redrawn at an
    /// unchanged size keeps its one texture instead of making another.
    spare: Vec<Spare>,
    pins: HashMap<TextureId, usize>,
    used: std::collections::HashSet<TextureId>,
}

#[derive(Default)]
struct IdleBitmaps {
    /// Full-size bitmap textures kept in the order they left the frame.
    entries: VecDeque<(Weak<Bitmap>, TextureId, u64)>,
    bytes: u64,
}

impl IdleBitmaps {
    fn mip_bytes(width: u32, height: u32) -> u64 {
        let desc = TextureDesc {
            mipmapped: true,
            ..TextureDesc::rgba(width, height)
        };
        (0..desc.levels())
            .map(|level| {
                u64::from((desc.width >> level).max(1))
                    * u64::from((desc.height >> level).max(1))
                    * u64::from(desc.format.channels())
            })
            .sum()
    }

    fn remove(&mut self, index: usize) -> TextureId {
        let (_, texture, bytes) = self.entries.remove(index).unwrap();
        self.bytes -= bytes;
        texture
    }

    fn take(&mut self, image: &Arc<Bitmap>, size: [u32; 2]) -> Option<TextureId> {
        if size != [image.width(), image.height()] {
            return None;
        }
        // A Weak keeps its allocation address reserved even after the decoded pixels drop.
        let i = self
            .entries
            .iter()
            .position(|(owner, _, _)| owner.as_ptr() == Arc::as_ptr(image))?;
        Some(self.remove(i))
    }

    fn take_texture(&mut self, texture: TextureId) -> Option<Arc<Bitmap>> {
        let i = self.entries.iter().position(|(_, t, _)| *t == texture)?;
        let image = self.entries[i].0.upgrade()?;
        self.remove(i);
        Some(image)
    }

    fn sweep(&mut self, device: &mut impl Device, all: bool) -> Result<()> {
        for i in (0..self.entries.len()).rev() {
            if all || self.entries[i].0.strong_count() == 0 {
                device.retire_texture(self.remove(i))?;
            }
        }
        Ok(())
    }

    fn keep(
        &mut self,
        device: &mut impl Device,
        image: Arc<Bitmap>,
        texture: TextureId,
    ) -> Result<()> {
        let (w, h) = (image.width(), image.height());
        if Arc::strong_count(&image) == 1 || w == 0 || h == 0 {
            return device.retire_texture(texture);
        }
        let bytes = Self::mip_bytes(w, h);
        if bytes > IDLE_BYTES {
            return device.retire_texture(texture);
        }
        self.entries
            .push_back((Arc::downgrade(&image), texture, bytes));
        self.bytes += bytes;
        while self.entries.len() > IDLE_BITMAPS || self.bytes > IDLE_BYTES {
            device.retire_texture(self.remove(0))?;
        }
        Ok(())
    }
}

struct Uploaded {
    image: Arc<Pixmap>,
    texture: TextureId,
    mipmapped: bool,
}

struct Spare {
    size: [u32; 2],
    texture: TextureId,
    mipmapped: bool,
}

impl Images {
    pub fn pin(&mut self, texture: TextureId) {
        if self.bitmaps.values().any(|(_, t)| *t == texture)
            || self.pixmaps.values().any(|u| u.texture == texture)
            || self.frames.values().any(|&t| t == texture)
        {
            *self.pins.entry(texture).or_default() += 1;
        } else if let Some(image) = self.idle.take_texture(texture) {
            let key = (
                Arc::as_ptr(&image) as usize,
                [image.width(), image.height()],
            );
            self.bitmaps.insert(key, (image, texture));
            *self.pins.entry(texture).or_default() += 1;
        }
    }

    pub fn unpin(&mut self, texture: TextureId) {
        if let Some(n) = self.pins.get_mut(&texture) {
            *n -= 1;
            if *n == 0 {
                self.pins.remove(&texture);
            }
        }
    }

    pub fn begin(&mut self, device: &mut impl Device) -> Result<()> {
        self.used.clear();
        let mut counts = HashMap::new();
        for &(address, _) in self.bitmaps.keys() {
            *counts.entry(address).or_insert(0) += 1;
        }
        let dead: std::collections::HashSet<_> = self
            .bitmaps
            .iter()
            .filter_map(|(&(address, _), (p, _))| {
                (Arc::strong_count(p) == counts[&address]).then_some(address)
            })
            .collect();
        for (_, (_, texture)) in self
            .bitmaps
            .extract_if(|(address, _), (_, t)| dead.contains(address) && !self.pins.contains_key(t))
        {
            device.retire_texture(texture)?;
        }
        self.idle.sweep(device, false)?;
        for unused in self.spare.drain(..) {
            device.retire_texture(unused.texture)?;
        }
        self.spare.extend(
            self.pixmaps
                .extract_if(|_, u| {
                    Arc::strong_count(&u.image) == 1 && !self.pins.contains_key(&u.texture)
                })
                .map(|(_, u)| Spare {
                    size: [u.image.width(), u.image.height()],
                    texture: u.texture,
                    mipmapped: u.mipmapped,
                }),
        );
        Ok(())
    }

    pub(super) fn end(&mut self, device: &mut impl Device) -> Result<()> {
        self.release_unused(device, true)
    }

    pub(super) fn trim(&mut self, device: &mut impl Device) -> Result<()> {
        self.used.clear();
        self.idle.sweep(device, true)?;
        self.release_unused(device, false)
    }

    fn release_unused(&mut self, device: &mut impl Device, keep_idle: bool) -> Result<()> {
        for (_, texture) in self
            .frames
            .extract_if(|_, t| !self.used.contains(t) && !self.pins.contains_key(t))
        {
            device.retire_texture(texture)?;
        }
        let unused =
            |texture: &TextureId| !self.used.contains(texture) && !self.pins.contains_key(texture);
        if keep_idle {
            // Release unused reduced variants first so their Arcs do not keep a full-size bitmap eligible.
            for (_, (_, texture)) in self.bitmaps.extract_if(|(_, size), (image, t)| {
                unused(t) && *size != [image.width(), image.height()]
            }) {
                device.retire_texture(texture)?;
            }
            self.idle.sweep(device, false)?;
        }
        for (_, (image, texture)) in self.bitmaps.extract_if(|_, (_, t)| unused(t)) {
            if keep_idle {
                self.idle.keep(device, image, texture)?;
            } else {
                device.retire_texture(texture)?;
            }
        }
        for (_, u) in self.pixmaps.extract_if(|_, u| {
            !self.used.contains(&u.texture) && !self.pins.contains_key(&u.texture)
        }) {
            device.retire_texture(u.texture)?;
        }
        for unused in self.spare.drain(..) {
            device.retire_texture(unused.texture)?;
        }
        Ok(())
    }

    pub fn bitmap(
        &mut self,
        device: &mut impl Device,
        image: &Arc<Bitmap>,
        size: [u32; 2],
    ) -> Result<TextureId> {
        let key = (Arc::as_ptr(image) as usize, size);
        if let Some((_, texture)) = self.bitmaps.get(&key) {
            self.used.insert(*texture);
            return Ok(*texture);
        }
        let texture = match self.idle.take(image, size) {
            Some(texture) => texture,
            None => upload_bitmap(device, image, size)?,
        };
        self.used.insert(texture);
        self.bitmaps.insert(key, (image.clone(), texture));
        Ok(texture)
    }

    pub fn canvas(
        &mut self,
        device: &mut impl Device,
        image: &Pixmap,
        update: CanvasUpdate,
        mipmapped: bool,
    ) -> Result<TextureId> {
        let key = (update.revision, mipmapped);
        if let Some(&texture) = self.frames.get(&key) {
            self.used.insert(texture);
            return Ok(texture);
        }
        let previous = (update.previous, mipmapped);
        // A retained row or an earlier draw in this frame still needs its own pixels.
        let reuse = self
            .frames
            .get(&previous)
            .is_some_and(|t| !self.used.contains(t) && !self.pins.contains_key(t));
        let texture = if reuse {
            let texture = self.frames.remove(&previous).unwrap();
            if let Some(area) = update.area {
                if let Err(error) = fill_area(device, texture, image, area, mipmapped) {
                    device.retire_texture(texture)?;
                    return Err(error);
                }
            }
            texture
        } else {
            upload(device, image.as_ref(), mipmapped)?
        };
        self.frames.insert(key, texture);
        self.used.insert(texture);
        Ok(texture)
    }

    /// The texture showing `image`. One not seen before is uploaded, with a
    /// mip chain when `mipmapped`, into a spare texture of its size if there
    /// is one, else a new texture.
    pub fn pixmap(
        &mut self,
        device: &mut impl Device,
        image: &Arc<Pixmap>,
        mipmapped: bool,
    ) -> Result<TextureId> {
        let key = Arc::as_ptr(image) as usize;
        if let Some(u) = self.pixmaps.get(&key) {
            self.used.insert(u.texture);
            return Ok(u.texture);
        }
        let pixels = image.as_ref().as_ref();
        let size = [image.width(), image.height()];
        let fits = |s: &Spare| s.size == size && s.mipmapped == mipmapped;
        let texture = match self.spare.iter().position(fits) {
            Some(i) => {
                let texture = self.spare.swap_remove(i).texture;
                if let Err(error) = fill(device, texture, pixels, mipmapped) {
                    return Err(retire_after_error(device, texture, error));
                }
                texture
            }
            None => upload(device, pixels, mipmapped)?,
        };
        self.used.insert(texture);
        self.pixmaps.insert(
            key,
            Uploaded {
                image: image.clone(),
                texture,
                mipmapped,
            },
        );
        Ok(texture)
    }
}

fn upload_bitmap(device: &mut impl Device, image: &Bitmap, size: [u32; 2]) -> Result<TextureId> {
    #[cfg(target_os = "macos")]
    let _trace =
        exact_ink::device::apple::Trace::with_value(c"picture upload", image.trace_source());
    if size == [image.width(), image.height()] {
        upload(device, image.pixels(), true)
    } else {
        let pixels =
            exact_ink::image::reduce(image.pixels().data(), [image.width(), image.height()], size);
        let pixels = tiny_skia::PixmapRef::from_bytes(&pixels, size[0], size[1]).unwrap();
        upload(device, pixels, true)
    }
}

fn upload(
    device: &mut impl Device,
    pixels: tiny_skia::PixmapRef<'_>,
    mipmapped: bool,
) -> Result<TextureId> {
    #[cfg(target_os = "macos")]
    let _trace = exact_ink::device::apple::Trace::new(c"image upload");
    let texture = device.create_texture(TextureDesc {
        mipmapped,
        ..TextureDesc::rgba(pixels.width(), pixels.height())
    })?;
    if let Err(error) = fill(device, texture, pixels, mipmapped) {
        return Err(retire_after_error(device, texture, error));
    }
    Ok(texture)
}

fn retire_after_error(device: &mut impl Device, texture: TextureId, error: Error) -> Error {
    match (error, device.retire_texture(texture)) {
        (error @ Error::DeviceLost(_), _) => error,
        (_, Err(lost @ Error::DeviceLost(_))) => lost,
        (error, _) => error,
    }
}

fn fill_area(
    device: &mut impl Device,
    texture: TextureId,
    image: &Pixmap,
    area: tiny_skia::IntRect,
    mipmapped: bool,
) -> Result<()> {
    let width = image.width();
    let packed;
    let bytes = if area.width() == width {
        let stride = width as usize * 4;
        &image.data()[area.top() as usize * stride..area.bottom() as usize * stride]
    } else {
        packed = image.clone_rect(area).expect("damage is inside the canvas");
        packed.data()
    };
    device.upload_texture(
        texture,
        [area.left() as u32, area.top() as u32],
        [area.width(), area.height()],
        bytes,
    )?;
    if mipmapped {
        device.generate_mips(texture)?;
    }
    Ok(())
}

fn fill(
    device: &mut impl Device,
    texture: TextureId,
    pixels: tiny_skia::PixmapRef<'_>,
    mipmapped: bool,
) -> Result<()> {
    device.upload_texture(
        texture,
        [0, 0],
        [pixels.width(), pixels.height()],
        pixels.data(),
    )?;
    if mipmapped {
        device.generate_mips(texture)?;
    }
    Ok(())
}

impl Ink {
    pub(super) fn picture(
        &mut self,
        texture: TextureId,
        dst: Rect4,
        clips: &[Shape],
        ts: Transform,
        tint: Option<[u8; 4]>,
        sampling: (exact_kernel::ImageRendering, [u32; 2]),
    ) {
        if dst.2 <= 0. || dst.3 <= 0. {
            return;
        }
        for shape in clips {
            self.push_clip(shape, ts);
        }
        let mut instance = Instance::new(Kind::RoundedRect, Brush::Image, rect(dst));
        instance.xform = affine(self.device_transform(ts));
        let (rendering, size) = sampling;
        match rendering {
            exact_kernel::ImageRendering::Auto => {}
            exact_kernel::ImageRendering::CrispEdges => instance.flags |= flags::NEAREST,
            exact_kernel::ImageRendering::Pixelated => {
                instance.flags |= flags::PIXELATED;
                // Virtual nearest-neighbour grid; the shader samples it without allocating it.
                let dev = self.device_transform(ts);
                instance.param[0] = (dst.2 * dev.sx.hypot(dev.ky) / size[0] as f32)
                    .round()
                    .max(1.);
                instance.param[1] = (dst.3 * dev.kx.hypot(dev.sy) / size[1] as f32)
                    .round()
                    .max(1.);
            }
        }
        if let Some(tint) = tint {
            instance.flags |= flags::TINT;
            instance.color = color(tint);
        }
        let result = self.emit(
            instance,
            Bindings {
                image: texture,
                ..Default::default()
            },
        );
        self.accept(result);
        for _ in clips {
            self.pop_clip();
        }
    }
}

/// A mip chain cannot reduce one axis while retaining detail in the other.
pub(super) fn sampling_size(size: [u32; 2], dst: Rect4, dev: Transform) -> [u32; 2] {
    let pixels = [
        dst.2.abs() * dev.sx.hypot(dev.ky),
        dst.3.abs() * dev.kx.hypot(dev.sy),
    ];
    let ratios = [pixels[0] / size[0] as f32, pixels[1] / size[1] as f32];
    if ratios[0].max(ratios[1]) > 2. * ratios[0].min(ratios[1]) {
        std::array::from_fn(|i| {
            if ratios[i] < 0.5 {
                (pixels[i].round() as u32).clamp(1, size[i])
            } else {
                size[i]
            }
        })
    } else {
        size
    }
}

/// Whether a picture of `size` drawn into `dst` under `dev` is shown at less
/// than half its size on either axis, where sampling it needs a mip chain.
pub(super) fn minified(size: [u32; 2], dst: Rect4, dev: Transform) -> bool {
    dst.2.abs() * dev.sx.hypot(dev.ky) < size[0] as f32 / 2.
        || dst.3.abs() * dev.kx.hypot(dev.sy) < size[1] as f32 / 2.
}
