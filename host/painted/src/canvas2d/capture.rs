//! A 2D canvas's kept bitmap as an image file (LLP 1056 D9, as amended for
//! expo/exact#188 and #504; `exact_runner::canvas_capture`). The bitmap is
//! the replayer's own pixmap, every list applied (the painters share it, and
//! a list drawn while one holds it draws into a copy), so an accumulating
//! drawing, its images and its text are what the capture holds. PNG stores straight-alpha sRGB pixels. A JPEG is the bitmap
//! over opaque black, as `toBlob` writes one: by WIC on Windows
//! ([JPEG encoder options](https://learn.microsoft.com/windows/win32/wic/jpeg-format-overview)),
//! by `Bitmap.compress` on Android, which also writes WebP, and elsewhere
//! by `jpeg-encoder`, since Linux has no system encoder this host links.
//! Another type is written as PNG. The file is written beside its target
//! and renamed over it, under `fs.write`, as `exactBodyTo` writes.

use super::Canvases;
use exact_runner::canvas_capture::{refused, CanvasCapture, Encoding};
use exact_runner::Outcome;

impl Canvases {
    /// Encode only the bitmap held for the requested lifetime and generation,
    /// as `(bytes, type)`.
    fn capture_image(&self, c: &CanvasCapture) -> Result<(Vec<u8>, &'static str), String> {
        if c.sequence == 0
            || self
                .completed
                .get(&c.view)
                .is_none_or(|seq| *seq < c.sequence)
        {
            return Err("the requested drawing has not been replayed".into());
        }
        let pixmap = match self.replayers.get(&c.view) {
            Some(r) if r.lifetime == c.lifetime && r.generation == c.generation => &r.pixmap,
            _ => return Err("the requested canvas generation has not been drawn".into()),
        };
        encode(pixmap, &c.encoding)
    }
}

/// `pixmap` as `encoding` asks where this host writes that type, else as PNG.
pub(crate) fn encode(
    pixmap: &tiny_skia::Pixmap,
    encoding: &Encoding,
) -> Result<(Vec<u8>, &'static str), String> {
    let jpeg = encoding.media_type == "image/jpeg";
    #[cfg(target_os = "android")]
    if let Some(named) = ["image/jpeg", "image/webp"]
        .into_iter()
        .find(|t| *t == encoding.media_type)
    {
        if let Some(codec) = exact_data::image::platform::installed().filter(|c| c.encodes(named)) {
            let source = if jpeg { opaque(pixmap) } else { pixmap.clone() };
            let png = canvas_png(&source).map_err(|e| e.to_string())?;
            let failed = |(code, detail): (String, String)| format!("{code}: {detail}");
            let decoded = codec
                .decode(&png, true, exact_runner::canvas_capture::MAX_PIXELS)
                .map_err(failed)?;
            let size = (decoded.width, decoded.height);
            let encoded =
                codec.encode(decoded.handle, size, named, encoding.lossy_quality(), false);
            codec.close(decoded.handle);
            return encoded.map(|bytes| (bytes, named)).map_err(failed);
        }
    }
    #[cfg(not(target_os = "android"))]
    if jpeg {
        if pixmap.width() > u32::from(u16::MAX) || pixmap.height() > u32::from(u16::MAX) {
            return Err("a JPEG holds at most 65,535 pixels a side".into());
        }
        return jpeg_bytes(&opaque(pixmap), encoding.lossy_quality()).map(|b| (b, "image/jpeg"));
    }
    canvas_png(pixmap)
        .map(|b| (b, "image/png"))
        .map_err(|e| e.to_string())
}

fn canvas_png(pixels: &tiny_skia::Pixmap) -> Result<Vec<u8>, png::EncodingError> {
    let straight;
    let rgba = if pixels.pixels().iter().all(|p| p.is_opaque()) {
        pixels.data()
    } else {
        straight = pixels.clone().take_demultiplied();
        &straight
    };
    png(pixels.width(), pixels.height(), rgba)
}

/// The PNG representation of a captured canvas or display. Rows are straight-alpha sRGB.
pub(crate) fn png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    writer.finish()?;
    Ok(out)
}

/// `pixmap` over opaque black: its premultiplied colour with full alpha.
pub(super) fn opaque(pixmap: &tiny_skia::Pixmap) -> tiny_skia::Pixmap {
    let mut out = pixmap.clone();
    for p in out.pixels_mut() {
        *p = tiny_skia::PremultipliedColorU8::from_rgba(p.red(), p.green(), p.blue(), 255)
            .expect("full alpha admits every colour");
    }
    out
}

/// An opaque `pixmap` as a baseline JPEG at `quality` (0 to 1).
#[cfg(not(any(windows, target_os = "android")))]
fn jpeg_bytes(pixmap: &tiny_skia::Pixmap, quality: f64) -> Result<Vec<u8>, String> {
    let rgb: Vec<u8> = (pixmap.pixels().iter())
        .flat_map(|p| [p.red(), p.green(), p.blue()])
        .collect();
    let mut out = Vec::new();
    jpeg_encoder::Encoder::new(&mut out, jpeg_quality(quality))
        .encode(
            &rgb,
            pixmap.width() as u16,
            pixmap.height() as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// `toBlob`'s quality as libjpeg's 1–100, as Chrome maps it.
#[cfg(not(any(windows, target_os = "android")))]
fn jpeg_quality(quality: f64) -> u8 {
    ((quality * 100.0).round() as u8).max(1)
}

/// An opaque `pixmap` as a JPEG at `quality` by WIC's encoder, whose
/// `ImageQuality` option is `toBlob`'s 0 to 1.
#[cfg(windows)]
#[allow(unsafe_code)] // WIC through COM.
fn jpeg_bytes(pixmap: &tiny_skia::Pixmap, quality: f64) -> Result<Vec<u8>, String> {
    use ::windows::Win32::Foundation::HGLOBAL;
    use ::windows::Win32::Graphics::Imaging::*;
    use ::windows::Win32::System::Com::StructuredStorage::{CreateStreamOnHGlobal, PROPBAG2};
    use ::windows::Win32::System::Com::*;
    use ::windows::Win32::System::Variant::{VARIANT, VT_R4};
    struct Apartment(bool);
    impl Drop for Apartment {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: paired with this thread's successful CoInitializeEx.
                unsafe { CoUninitialize() };
            }
        }
    }
    let failed = |e: ::windows::core::Error| format!("WIC: {e}");
    let (width, height) = (pixmap.width(), pixmap.height());
    let bgr: Vec<u8> = (pixmap.pixels().iter())
        .flat_map(|p| [p.blue(), p.green(), p.red()])
        .collect();
    // SAFETY: every interface is released before the apartment guard, and
    // the property name and pixels outlive the calls that read them.
    unsafe {
        // A thread already in another apartment keeps it; COM is usable.
        let _apartment = Apartment(CoInitializeEx(None, COINIT_MULTITHREADED).is_ok());
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(failed)?;
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true).map_err(failed)?;
        let encoder = factory
            .CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())
            .map_err(failed)?;
        encoder
            .Initialize(&stream, WICBitmapEncoderNoCache)
            .map_err(failed)?;
        let (mut frame, mut options) = (None, None);
        encoder
            .CreateNewFrame(&mut frame, &mut options)
            .map_err(failed)?;
        let (Some(frame), Some(options)) = (frame, options) else {
            return Err("WIC made no JPEG frame".into());
        };
        let mut name: Vec<u16> = "ImageQuality".encode_utf16().chain(Some(0)).collect();
        let property = PROPBAG2 {
            pstrName: ::windows::core::PWSTR(name.as_mut_ptr()),
            ..Default::default()
        };
        let mut value = VARIANT::default();
        (*value.Anonymous.Anonymous).vt = VT_R4;
        (*value.Anonymous.Anonymous).Anonymous.fltVal = quality as f32;
        options.Write(1, &property, &value).map_err(failed)?;
        frame.Initialize(&options).map_err(failed)?;
        frame.SetSize(width, height).map_err(failed)?;
        let mut format = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut format).map_err(failed)?;
        if format != GUID_WICPixelFormat24bppBGR {
            return Err("WIC's JPEG encoder takes no 24-bit BGR".into());
        }
        frame.WritePixels(height, width * 3, &bgr).map_err(failed)?;
        frame.Commit().map_err(failed)?;
        encoder.Commit().map_err(failed)?;
        let mut stat = STATSTG::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).map_err(failed)?;
        let mut out = vec![0u8; usize::try_from(stat.cbSize).map_err(|e| e.to_string())?];
        stream.Seek(0, STREAM_SEEK_SET, None).map_err(failed)?;
        let mut read = 0u32;
        stream
            .Read(out.as_mut_ptr().cast(), out.len() as u32, Some(&mut read))
            .ok()
            .map_err(failed)?;
        out.truncate(read as usize);
        Ok(out)
    }
}

/// The capture `found` (the runner's resolution of the request) carried
/// out: the PNG written at its path under the app's `grants`, narrowed by
/// the request's `scope`, below the app's `roots`.
pub(crate) fn capture(
    canvases: &Canvases,
    found: Result<CanvasCapture, Outcome>,
    roots: Option<[std::path::PathBuf; 3]>,
    grants: &str,
    scope: Option<&str>,
) -> Outcome {
    let c = match found {
        Ok(c) => c,
        Err(refusal) => return refusal,
    };
    let Some(roots) = roots else {
        return refused(
            "NotSupportedError",
            "this host has no app files (no storage here)",
        );
    };
    let (image, media_type) = match canvases.capture_image(&c) {
        Ok(encoded) => encoded,
        Err(why) => return refused("EncodingError", &why),
    };
    if let Err(refusal) = c.snapshotted() {
        return refusal;
    }
    match write(&roots, grants, scope, &c.path, &image) {
        Ok(()) => c.captured(media_type),
        Err((name, why)) => refused(name, &format!("{}: {why}", c.path)),
    }
}

fn write(
    roots: &[std::path::PathBuf; 3],
    grants: &str,
    scope: Option<&str>,
    path: &str,
    image: &[u8],
) -> Result<(), (&'static str, String)> {
    use ibex2::boundary::HostError;
    let denied = |why: String| ("NotAllowedError", why);
    let failed = |e: HostError| match e {
        HostError::Denied { capability } => {
            denied(format!("no grant covers it; grant `{capability} {path}`"))
        }
        e => ("EncodingError", e.to_string()),
    };
    let admitted = exact_data::storage::scope(grants, scope).map_err(denied)?;
    let grants = ibex2::grant::GrantSet::parse(&exact_runner::io_grants(admitted))
        .map_err(|e| denied(format!("the app's grants did not parse: {e}")))?;
    for root in roots {
        std::fs::create_dir_all(root).map_err(|e| ("EncodingError", e.to_string()))?;
    }
    let directories = ibex2::stdlib::app_fs::AppDirectories::new(&roots[0], &roots[1], &roots[2])
        .map_err(failed)?;
    let mut staged = directories.stage(&grants, path).map_err(failed)?;
    staged.write_all(image).map_err(failed)?;
    staged.publish().map_err(failed)
}
