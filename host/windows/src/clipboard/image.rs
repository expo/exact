//! PNG sanitization and the packed DIB, independent of Win32 (LLP 1133.006 D1–D2).
use sha2::{Digest, Sha256};
use std::io::Cursor;

const MOST_PIXELS: u64 = 64_000_000;
const MOST_FILE: usize = 64 * 1024 * 1024;

/// A sanitized PNG and, when its colours permit it, a Windows bitmap.
#[derive(Debug)]
pub struct Image {
    pub(super) png: Vec<u8>,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) dib: Option<Vec<u8>>,
    width: u32,
    height: u32,
}

impl Image {
    /// Decode frame zero of a PNG's animation (skipping a separate default
    /// image), bounded before decoding, and retain its colour interpretation.
    pub fn decode(source: &[u8]) -> Result<Self, String> {
        if source.len() > MOST_FILE {
            return Err("image/png is over 64 MiB".into());
        }
        if source.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
            || source.get(12..16) != Some(b"IHDR")
            || source.len() < 24
        {
            return Err("not a PNG".into());
        }
        let width = u32::from_be_bytes(source[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(source[20..24].try_into().unwrap());
        let size = pixel_bytes(width, height)?;
        let mut decoder = png::Decoder::new_with_limits(
            Cursor::new(source),
            png::Limits {
                bytes: size + MOST_FILE,
            },
        );
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        decoder.set_ignore_text_chunk(true);
        let bad = |e| format!("not a PNG: {e}");
        let mut reader = decoder.read_info().map_err(bad)?;
        let skip_default =
            reader.info().animation_control.is_some() && reader.info().frame_control.is_none();
        if skip_default {
            reader.next_frame_info().map_err(bad)?;
        }
        let mut frame = vec![0; reader.output_buffer_size().ok_or("PNG is too large")?];
        let info = reader.next_frame(&mut frame).map_err(bad)?;
        let control = reader.info().frame_control;
        // png 0.18.1's Reader::next_frame expands Adam7 into IHDR-width rows,
        // although OutputInfo::line_size describes the narrower APNG subframe.
        let stride = if reader.info().interlaced {
            width as usize * info.color_type.samples()
        } else {
            info.line_size
        };
        reader.finish().map_err(bad)?;
        let (x, y) = control.map_or((0, 0), |f| (f.x_offset, f.y_offset));
        if u64::from(x) + u64::from(info.width) > u64::from(width)
            || u64::from(y) + u64::from(info.height) > u64::from(height)
        {
            return Err("not a PNG: frame outside the canvas".into());
        }
        // A first animation frame composites over transparent black. Later
        // frames and disposal do not affect the clipboard's still image.
        let mut rgba = vec![0; size];
        let samples = info.color_type.samples();
        for row in 0..info.height as usize {
            for col in 0..info.width as usize {
                let src = &frame[row * stride + col * samples..][..samples];
                let pixel = match info.color_type {
                    png::ColorType::Rgba => [src[0], src[1], src[2], src[3]],
                    png::ColorType::Rgb => [src[0], src[1], src[2], 255],
                    png::ColorType::GrayscaleAlpha => [src[0], src[0], src[0], src[1]],
                    png::ColorType::Grayscale => [src[0], src[0], src[0], 255],
                    png::ColorType::Indexed => return Err("PNG palette was not expanded".into()),
                };
                let at = ((row + y as usize) * width as usize + col + x as usize) * 4;
                rgba[at..at + 4].copy_from_slice(&pixel);
            }
        }
        // Preserve colour interpretation, but no text, EXIF or animation. No
        // ICC conversion crate is locked here: tagged non-sRGB pixels are
        // offered as PNG only, never mislabeled by our sRGB DIB (LLP 1133.006 D5).
        let source = reader.info();
        let mut output = png::Info::with_size(width, height);
        output.bit_depth = png::BitDepth::Eight;
        output.color_type = if matches!(
            info.color_type,
            png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha
        ) {
            png::ColorType::GrayscaleAlpha
        } else {
            png::ColorType::Rgba
        };
        // png 0.18.1 decodes into *_chunk; its encoder reads source_*.
        output.source_gamma = source.gama_chunk;
        output.source_chromaticities = source.chrm_chunk;
        output.icc_profile = source.icc_profile.clone();
        // PNG third edition gives iCCP precedence over sRGB. The encoder
        // otherwise drops iCCP when both are set.
        // https://www.w3.org/TR/png-3/#color-chunk-precendence
        output.srgb = source.srgb.filter(|_| output.icc_profile.is_none());
        if source.mastering_display_color_volume.is_some() || source.content_light_level.is_some() {
            return Err("PNG HDR colour information cannot be preserved by this encoder".into());
        }
        if let Some(cicp) = source.coding_independent_code_points {
            if cicp.color_primaries != 1
                || cicp.transfer_function != 13
                || cicp.matrix_coefficients != 0
                || !cicp.is_video_full_range_image
            {
                return Err(
                    "PNG cICP colour information cannot be preserved by this encoder".into(),
                );
            }
            // H.273 (1, 13, 0, 1) is full-range sRGB. cICP takes precedence
            // over ICC and the other colour tags (PNG third edition, Table 1).
            // Normalize to the sRGB chunk this encoder can write.
            output.icc_profile = None;
            output.source_gamma = None;
            output.source_chromaticities = None;
            output.srgb = Some(source.srgb.unwrap_or(png::SrgbRenderingIntent::Perceptual));
        }
        let dib = if output.icc_profile.is_none()
            && (output.srgb.is_some()
                || (output.source_gamma.is_none() && output.source_chromaticities.is_none()))
        {
            Some(dib_v5(width, height, &rgba)?)
        } else {
            None
        };
        // A grayscale ICC profile must still describe grayscale samples.
        let gray;
        let pixels = if output.color_type == png::ColorType::GrayscaleAlpha {
            gray = rgba
                .chunks_exact(4)
                .flat_map(|p| [p[0], p[3]])
                .collect::<Vec<_>>();
            &gray
        } else {
            &rgba
        };
        let mut png = Vec::new();
        {
            let encoder = png::Encoder::with_info(&mut png, output).map_err(|e| e.to_string())?;
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(pixels).map_err(|e| e.to_string())?;
            writer.finish().map_err(|e| e.to_string())?;
        }
        Ok(Self {
            png,
            dib,
            width,
            height,
        })
    }

    pub(super) fn description(&self) -> serde_json::Value {
        serde_json::json!({
            "width": self.width, "height": self.height, "bytes": self.png.len(),
            "sha256": format!("{:x}", Sha256::digest(&self.png)),
        })
    }

    /// The still image's canvas size, unchanged by sanitization.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

fn pixel_bytes(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
        return Err("not a PNG".into());
    }
    let pixels = u64::from(width) * u64::from(height);
    if pixels > MOST_PIXELS {
        return Err(format!(
            "too large: {width}×{height} is over 64,000,000 pixels"
        ));
    }
    Ok(pixels as usize * 4)
}

/// Packed BITMAPV5HEADER, then bottom-up BGRA rows with premultiplied alpha,
/// PNG retains straight alpha. Chromium's ClipboardWin::WriteBitmap calls
/// skia_utils_win.cc's CreateDIBV5ImageDataFromN32SkBitmap: premultiplied
/// sRGB pixels, BI_RGB, LCS_WINDOWS_COLOR_SPACE. clipboard_util_win.cc does
/// not construct that header. Firefox's former nsImageClipboard.cpp delegates
/// to nsBMPEncoder, which writes premultiplied pixels, BI_BITFIELDS and
/// LCS_sRGB. We use the latter header convention with BGRA masks. Microsoft's
/// CSType documentation specifies colour interpretation, not alpha synthesis;
/// legacy consumers' transparency still needs a Windows paste check.
/// At 32 bits/pixel each width*4 row is already DWORD-aligned; no file header,
/// palette or embedded profile is present.
/// https://learn.microsoft.com/windows/win32/api/wingdi/ns-wingdi-bitmapv5header
/// https://chromium.googlesource.com/chromium/src/+/main/ui/base/clipboard/clipboard_win.cc
/// https://chromium.googlesource.com/chromium/src/+/main/skia/ext/skia_utils_win.cc
/// https://github.com/mozilla/gecko-dev/blob/2afd829d0f1aba056c4322c1d3d579ede7d0b287/widget/windows/nsImageClipboard.cpp
/// https://github.com/mozilla/gecko-dev/blob/master/image/encoders/bmp/nsBMPEncoder.cpp
fn dib_v5(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let size = pixel_bytes(width, height)?;
    if rgba.len() != size {
        return Err("invalid RGBA buffer length".into());
    }
    let mut out = vec![0; 124 + size];
    for (at, value) in [
        (0, 124u32),
        (4, width),
        (8, height),
        (16, 3), // BI_BITFIELDS: the four masks below describe the channels.
        (20, size as u32),
        (40, 0x00ff0000),
        (44, 0x0000ff00),
        (48, 0x000000ff),
        (52, 0xff000000),
        (56, 0x73524742), // LCS_sRGB
        (108, 4),         // LCS_GM_IMAGES
    ] {
        out[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    out[12..14].copy_from_slice(&1u16.to_le_bytes());
    out[14..16].copy_from_slice(&32u16.to_le_bytes());
    let stride = width as usize * 4;
    for (source, dest) in rgba
        .chunks_exact(stride)
        .rev()
        .zip(out[124..].chunks_exact_mut(stride))
    {
        for (s, d) in source.chunks_exact(4).zip(dest.chunks_exact_mut(4)) {
            let premul = |v: u8| ((u16::from(v) * u16::from(s[3]) + 127) / 255) as u8;
            d.copy_from_slice(&[premul(s[2]), premul(s[1]), premul(s[0]), s[3]]);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
