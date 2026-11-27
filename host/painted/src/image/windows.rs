//! WIC's frame decoder, scaler and premultiplied RGBA conversion. No COM
//! object or borrowed stream buffer leaves the raster worker.
#![allow(unsafe_code)]
use super::{read_all, DecodePlan, Pixmap, Read, Refusal, Seek};
use ::windows::core::Interface;
use ::windows::Win32::{Graphics::Imaging::*, System::Com::*};
use std::cell::RefCell;

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: paired with this thread's successful CoInitializeEx.
        unsafe { CoUninitialize() };
    }
}

/// A thread's multithreaded apartment and the imaging factory made in it.
/// Every WIC object of a decode is made, used and released on that thread.
struct Wic {
    // Declared first: the factory is released before the apartment closes.
    factory: IWICImagingFactory,
    _apartment: Apartment,
}
impl Wic {
    fn open() -> Result<Self, Refusal> {
        // SAFETY: the guard balances this thread's successful CoInitializeEx.
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|_| Refusal::DecodeFailed)?;
            let apartment = Apartment;
            let factory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|_| Refusal::DecodeFailed)?;
            #[cfg(test)]
            OPENED.set(OPENED.get() + 1);
            Ok(Self {
                factory,
                _apartment: apartment,
            })
        }
    }
}

thread_local! {
    /// On a decode worker, its apartment and factory once a decode has
    /// opened them.
    static KEPT: RefCell<Option<Option<Wic>>> = const { RefCell::new(None) };
    /// The apartments this thread has opened.
    #[cfg(test)]
    static OPENED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// The size the last reduced decode on this thread asked of the decoder.
    #[cfg(test)]
    static REDUCED: std::cell::Cell<Option<(u32, u32)>> = const { std::cell::Cell::new(None) };
}

/// Marks this thread a decode worker while it lives: its decodes share one
/// apartment and one imaging factory, opened by the first of them. Any
/// other thread opens and closes its own around each decode.
///
/// COM is initialized once for each thread that uses it, and
/// `CoUninitialize` must not be called from `DllMain`. A thread-local's
/// destructor runs as its thread detaches, under the same loader lock, so
/// the apartment closes when this is dropped, in the thread's own code:
/// https://learn.microsoft.com/windows/win32/api/combaseapi/nf-combaseapi-coinitializeex
pub(in crate::image) struct Worker(std::marker::PhantomData<*const ()>);
impl Worker {
    pub(in crate::image) fn enter() -> Self {
        KEPT.with_borrow_mut(|kept| *kept = Some(None));
        Self(std::marker::PhantomData)
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        KEPT.with_borrow_mut(|kept| *kept = None);
    }
}

pub(in crate::image) fn decode<R: Read + Seek>(
    input: R,
    plan: &DecodePlan,
    cancelled: impl Fn() -> bool,
) -> Result<Pixmap, Refusal> {
    let bytes = read_all(input, plan, &cancelled)?;
    KEPT.with_borrow_mut(|kept| match kept {
        Some(kept) => {
            let wic = match kept {
                Some(wic) => wic,
                None => kept.insert(Wic::open()?),
            };
            decode_in(&wic.factory, &bytes, plan, cancelled)
        }
        // Any other thread opens an apartment for this decode alone.
        None => decode_in(&Wic::open()?.factory, &bytes, plan, cancelled),
    })
}

fn decode_in(
    factory: &IWICImagingFactory,
    bytes: &[u8],
    plan: &DecodePlan,
    cancelled: impl Fn() -> bool,
) -> Result<Pixmap, Refusal> {
    // SAFETY: every interface made here is released before the caller's
    // apartment closes; the stream's borrowed bytes outlive the entire
    // decoder/transform chain.
    unsafe {
        let stream = factory.CreateStream().map_err(|_| Refusal::DecodeFailed)?;
        stream
            .InitializeFromMemory(bytes)
            .map_err(|_| Refusal::DecodeFailed)?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .map_err(|_| Refusal::DecodeFailed)?;
        let index = plan.header.icon.map_or(0, |entry| entry.index);
        let frame = decoder.GetFrame(index).map_err(|_| Refusal::DecodeFailed)?;
        let (mut width, mut height) = (0, 0);
        frame
            .GetSize(&mut width, &mut height)
            .map_err(|_| Refusal::DecodeFailed)?;
        let swaps_axes = plan.header.orientation >= 5;
        let natural = if swaps_axes {
            (height, width)
        } else {
            (width, height)
        };
        if natural != plan.natural() {
            return Err(Refusal::ConflictingMetadata);
        }
        let full = plan.full();
        // The plan's size in the stored grid, before the turn.
        let stored = if swaps_axes {
            (full.height, full.width)
        } else {
            (full.width, full.height)
        };
        let whole: IWICBitmapSource = frame.cast().map_err(|_| Refusal::DecodeFailed)?;
        let colour_jpeg = decoder.GetContainerFormat() == Ok(GUID_ContainerFormatJpeg)
            && frame.GetPixelFormat() == Ok(GUID_WICPixelFormat24bppBGR);
        let scaled = if colour_jpeg {
            // No alpha to weigh the colours by: scaling the three stored
            // channels and converting the result gives the same pixels as
            // converting every source pixel first.
            let source = match reduced(factory, &frame, (width, height), stored) {
                Some(bitmap) => bitmap.cast().map_err(|_| Refusal::DecodeFailed)?,
                None => whole,
            };
            if cancelled() {
                return Err(Refusal::Stale);
            }
            premultiplied(factory, &scale(factory, &source, stored)?)?
        } else {
            scale(factory, &premultiplied(factory, &whole)?, stored)?
        };
        let transform = match plan.header.orientation {
            2 => WICBitmapTransformFlipHorizontal,
            3 => WICBitmapTransformRotate180,
            4 => WICBitmapTransformFlipVertical,
            5 => WICBitmapTransformOptions(
                WICBitmapTransformRotate270.0 | WICBitmapTransformFlipHorizontal.0,
            ),
            6 => WICBitmapTransformRotate90,
            7 => WICBitmapTransformOptions(
                WICBitmapTransformRotate90.0 | WICBitmapTransformFlipHorizontal.0,
            ),
            8 => WICBitmapTransformRotate270,
            _ => WICBitmapTransformRotate0,
        };
        let source = if transform == WICBitmapTransformRotate0 {
            scaled
        } else {
            let oriented = factory
                .CreateBitmapFlipRotator()
                .map_err(|_| Refusal::DecodeFailed)?;
            oriented
                .Initialize(&scaled, transform)
                .map_err(|_| Refusal::DecodeFailed)?;
            oriented.cast().map_err(|_| Refusal::DecodeFailed)?
        };
        if cancelled() {
            return Err(Refusal::Stale);
        }
        let mut pixels =
            Pixmap::new(plan.pixels.width, plan.pixels.height).ok_or(Refusal::InvalidDimensions)?;
        let rect = WICRect {
            X: plan.crop.x as i32,
            Y: plan.crop.y as i32,
            Width: plan.pixels.width as i32,
            Height: plan.pixels.height as i32,
        };
        source
            .CopyPixels(&rect, plan.pixels.width * 4, pixels.data_mut())
            .map_err(|_| Refusal::DecodeFailed)?;
        if cancelled() {
            return Err(Refusal::Stale);
        }
        for pixel in pixels.data_mut().as_chunks_mut::<4>().0 {
            let alpha = pixel[3];
            for channel in &mut pixel[..3] {
                *channel = (*channel).min(alpha);
            }
        }
        Ok(pixels)
    }
}

/// `source` as premultiplied RGBA.
///
/// # Safety
/// The caller is in the apartment `factory` was made in.
unsafe fn premultiplied(
    factory: &IWICImagingFactory,
    source: &IWICBitmapSource,
) -> Result<IWICBitmapSource, Refusal> {
    unsafe {
        let converted = factory
            .CreateFormatConverter()
            .map_err(|_| Refusal::DecodeFailed)?;
        converted
            .Initialize(
                source,
                &GUID_WICPixelFormat32bppPRGBA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|_| Refusal::DecodeFailed)?;
        converted.cast().map_err(|_| Refusal::DecodeFailed)
    }
}

/// `source` at `size`.
///
/// # Safety
/// The caller is in the apartment `factory` was made in.
unsafe fn scale(
    factory: &IWICImagingFactory,
    source: &IWICBitmapSource,
    size: (u32, u32),
) -> Result<IWICBitmapSource, Refusal> {
    unsafe {
        let scaled = factory
            .CreateBitmapScaler()
            .map_err(|_| Refusal::DecodeFailed)?;
        // HighQualityCubic widens the filter for reductions beyond 2x:
        // https://learn.microsoft.com/windows/win32/api/wincodec/ne-wincodec-wicbitmapinterpolationmode
        scaled
            .Initialize(
                source,
                size.0,
                size.1,
                WICBitmapInterpolationModeHighQualityCubic,
            )
            .map_err(|_| Refusal::DecodeFailed)?;
        scaled.cast().map_err(|_| Refusal::DecodeFailed)
    }
}

/// A colour JPEG's `frame` decoded at the smallest size its decoder reduces
/// to while decoding (an eighth, a quarter or a half) that is still at least
/// twice `wanted`, for the scaler to take the rest of the way; `None` when
/// no reduced size is.
///
/// Twice, because the decoder's reduction and the scaler's filter differ:
/// reduced to just cover `wanted`, a detailed photograph came out with a
/// tenth of its pixels more than 8/255 from the whole frame scaled, and at
/// twice with under half a percent. And only a size that divides the frame
/// exactly: the decoder rounds any other up, 540 rows to 68 at an eighth,
/// and scaling those as if they were the whole picture moves it by up to
/// half a pixel.
///
/// `IWICBitmapSourceTransform` is how a decoder is asked for a size:
/// https://learn.microsoft.com/windows/win32/api/wincodec/nn-wincodec-iwicbitmapsourcetransform
///
/// The bitmap holds at most a quarter of the frame's pixels, within the
/// whole frame the plan's scratch allows a WIC decoder.
///
/// # Safety
/// The caller is in the apartment `factory` was made in.
unsafe fn reduced(
    factory: &IWICImagingFactory,
    frame: &IWICBitmapFrameDecode,
    natural: (u32, u32),
    wanted: (u32, u32),
) -> Option<IWICBitmap> {
    unsafe {
        let least = (wanted.0.checked_mul(2)?, wanted.1.checked_mul(2)?);
        let (width, height) = [8, 4, 2]
            .into_iter()
            .filter(|by| natural.0 % by == 0 && natural.1 % by == 0)
            .map(|by| (natural.0 / by, natural.1 / by))
            .find(|size| size.0 >= least.0 && size.1 >= least.1)?;
        let transform: IWICBitmapSourceTransform = frame.cast().ok()?;
        let mut closest = (width, height);
        transform
            .GetClosestSize(&mut closest.0, &mut closest.1)
            .ok()?;
        if closest != (width, height) {
            return None;
        }
        #[cfg(test)]
        REDUCED.set(Some((width, height)));
        let format = GUID_WICPixelFormat24bppBGR;
        let bitmap = factory
            .CreateBitmap(width, height, &format, WICBitmapCacheOnLoad)
            .ok()?;
        let all = WICRect {
            X: 0,
            Y: 0,
            Width: width as i32,
            Height: height as i32,
        };
        let lock = bitmap.Lock(&all, WICBitmapLockWrite.0 as u32).ok()?;
        let stride = lock.GetStride().ok()?;
        let (mut len, mut data) = (0, std::ptr::null_mut());
        lock.GetDataPointer(&mut len, &mut data).ok()?;
        // SAFETY: the lock holds `len` writable bytes at `data` until it is
        // released, after the decoder has filled them.
        let rows = std::slice::from_raw_parts_mut(data, len as usize);
        transform
            .CopyPixels(
                std::ptr::null(),
                width,
                height,
                &format,
                WICBitmapTransformRotate0,
                stride,
                rows,
            )
            .ok()?;
        drop(lock);
        Some(bitmap)
    }
}

/// The apartments this thread has opened, and the size its last reduced
/// decode asked of the decoder.
#[cfg(test)]
pub(in crate::image) fn observed() -> (u32, Option<(u32, u32)>) {
    (OPENED.get(), REDUCED.take())
}
