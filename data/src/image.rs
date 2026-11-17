//! `storage.fs.compressImage` (LLP 1069.002 Amendment A1): decode an image,
//! scale it to a pixel budget and encode it as a JPEG, or a PNG that keeps
//! its alpha (A1.11), within a byte budget; and `storage.fs.transformImage`
//! (LLP 1133.019 D14), the same codec's edit: turned, flipped, cropped and
//! sized, keeping the source's colour, HDR and metadata (D15). Both take
//! [`edit`]'s `rotate`, `flip` and `crop`.
//!
//! The JPEG search is Bluesky's (`social-app` `compress.ts`), restated in
//! A1.2 and kept as a pure function over an encoder ([`search`]) so every
//! host's trials can be checked against one recorded sequence; a PNG has no
//! quality, so it only shrinks ([`search_lossless`]). The codec is the
//! platform's: ImageIO on Apple ([`compress`]), else one a host installs
//! ([`platform`]: Android's, LLP 1133.019 D16); with neither the call is
//! refused (`unsupported`, A1.3).
use std::time::Instant;

#[cfg(target_vendor = "apple")]
mod apple;
pub mod edit;
pub mod platform;
pub use edit::{Edit, Metadata, Transform};

/// Whether this process has an image codec: Apple's, or one a host installed.
pub fn available() -> bool {
    cfg!(target_vendor = "apple") || platform::installed().is_some()
}

/// The largest `maxDimension` (A1.1).
pub const MAX_DIMENSION: u32 = 8192;
/// The largest `maxBytes` (A1.1), and the largest source file (A1.5):
/// `exact_raster::MAX_ENCODED_BYTES`.
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// The most source pixels the header may declare (A1.5):
/// `exact_raster::MAX_SOURCE_PIXELS`.
pub const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
/// The most dimensions the search shrinks through (`attempts >= 4`).
pub const MAX_SHRINKS: u32 = 4;

/// What `compressImage` writes (`type`, A1.11): a JPEG unless the app asks
/// for a PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Jpeg,
    Png,
}

impl Format {
    /// The `type` option's value, or `None` for a type it does not take.
    pub fn parse(media_type: &str) -> Option<Self> {
        match media_type {
            "image/jpeg" => Some(Self::Jpeg),
            "image/png" => Some(Self::Png),
            _ => None,
        }
    }
    /// The media type the result reports.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
        }
    }
}

/// What the codec made: encoded bytes and their pixel size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compressed {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A failure, as `compressImage: <code>: <detail>` (A1.4): the prelude reads
/// the code from the message natively.
pub fn failure(code: &str, detail: impl std::fmt::Display) -> String {
    format!("compressImage: {code}: {detail}")
}

/// What a compression aims at (A1.1, A1.12): the longer side's limit, and a
/// byte target, a quality ceiling or both.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub max_dimension: u32,
    /// The encoded file's limit. `None`, with a `quality`: one encode at it,
    /// within [`MAX_BYTES`].
    pub max_bytes: Option<u64>,
    /// The quality to try first and never exceed, 0 to 1. `None`: Bluesky's
    /// search alone.
    pub quality: Option<f64>,
}

impl Limits {
    /// The most bytes the result may hold.
    pub fn bytes(&self) -> u64 {
        self.max_bytes.unwrap_or(MAX_BYTES)
    }
}

/// Check the options as A1.1 and A1.12 bound them, in A1.12's order.
pub fn check_limits(
    max_dimension: f64,
    max_bytes: Option<f64>,
    quality: Option<f64>,
) -> Result<Limits, String> {
    let integer = |n: f64, max: f64| n.fract() == 0.0 && (1.0..=max).contains(&n);
    if !integer(max_dimension, MAX_DIMENSION as f64) {
        return Err(failure(
            "invalid",
            format!("maxDimension must be an integer from 1 to {MAX_DIMENSION}"),
        ));
    }
    if max_bytes.is_some_and(|n| !integer(n, MAX_BYTES as f64)) {
        return Err(failure(
            "invalid",
            format!("maxBytes must be an integer from 1 to {MAX_BYTES}"),
        ));
    }
    if quality.is_some_and(|q| !(0.0..=1.0).contains(&q)) {
        return Err(failure("invalid", "quality must be a number from 0 to 1"));
    }
    if max_bytes.is_none() && quality.is_none() {
        return Err(failure("invalid", "needs maxBytes, quality or both"));
    }
    Ok(Limits {
        max_dimension: max_dimension as u32,
        max_bytes: max_bytes.map(|n| n as u64),
        quality,
    })
}

/// `containImageRes`: the size `width × height` takes inside a square of
/// `max`, never larger than itself, each side floored (and at least 1).
pub fn contain(width: u32, height: u32, max: u32) -> (u32, u32) {
    if width <= max && height <= max {
        return (width, height);
    }
    let scale = if width > height {
        f64::from(max) / f64::from(width)
    } else {
        f64::from(max) / f64::from(height)
    };
    let side = |n: u32| ((f64::from(n) * scale).floor() as u32).max(1);
    (side(width), side(height))
}

/// The bisection's exclusive top in whole percents (A1.12): 101, or under a
/// ceiling the least `n` with `n / 100 >= quality`, so that every trial is
/// below it. `quality * 100` can round across a whole percent either way.
fn top(quality: Option<f64>) -> u32 {
    let Some(quality) = quality else {
        return 101;
    };
    let mut n = (quality * 100.0).ceil() as u32;
    if f64::from(n) / 100.0 < quality {
        n += 1;
    }
    if n > 0 && f64::from(n - 1) / 100.0 >= quality {
        n -= 1;
    }
    n
}

/// The JPEG search over an oriented `width × height` source: Bluesky's
/// (A1.2), under `limits.quality` when there is one (A1.12).
/// `encode(w, h, quality)` returns the JPEG at `quality`, 0 to 1; an `Err`
/// ends the search with it. Returns the best fit, or `None`: nothing fits.
pub fn search<E>(
    width: u32,
    height: u32,
    limits: Limits,
    mut encode: E,
) -> Result<Option<Compressed>, String>
where
    E: FnMut(u32, u32, f64) -> Result<Vec<u8>, String>,
{
    let fits = |bytes: Vec<u8>, (width, height)| {
        (bytes.len() as u64 <= limits.bytes()).then_some(Compressed {
            bytes,
            width,
            height,
        })
    };
    let max_bytes = match (limits.max_bytes, limits.quality) {
        (Some(max_bytes), _) => max_bytes,
        // No byte target: one encode at the quality asked.
        (None, Some(quality)) => {
            let (w, h) = contain(width, height, limits.max_dimension);
            return Ok(fits(encode(w, h, quality)?, (w, h)));
        }
        (None, None) => return Err(failure("invalid", "needs maxBytes, quality or both")),
    };
    let top = top(limits.quality);
    let mut dimension = limits.max_dimension;
    let (mut lo, mut hi) = (0u32, top);
    let mut shrinks = 0;
    let mut best = None;
    // Whether this size has yet to try the ceiling.
    let mut fresh = limits.quality.is_some();
    // Until a quality fits and the bisection closes on it. Unsaid, `lo` is 0
    // only while `hi` is at least 14, so this is Bluesky's `hi - lo > 1`.
    while lo == 0 || hi - lo > 1 {
        if shrinks >= MAX_SHRINKS {
            break;
        }
        let (w, h) = contain(width, height, dimension);
        if let (Some(ceiling), true) = (limits.quality, fresh) {
            fresh = false;
            if let Some(fit) = fits(encode(w, h, ceiling)?, (w, h)) {
                return Ok(Some(fit));
            }
        }
        // `Math.round((hi + lo) / 2)`: a half rounds up.
        let quality = (hi + lo).div_ceil(2);
        if quality <= 13 {
            lo = 0;
            hi = top;
            shrinks += 1;
            fresh = limits.quality.is_some();
            dimension = (f64::from(dimension) * 0.8).floor() as u32;
            continue;
        }
        let bytes = encode(w, h, f64::from(quality) / 100.0)?;
        if bytes.len() as u64 <= max_bytes {
            lo = quality;
            best = Some(Compressed {
                bytes,
                width: w,
                height: h,
            });
        } else {
            hi = quality;
        }
    }
    Ok(best)
}

/// A PNG's search (A1.11): the fitted size, then each 0.8 shrink, at most
/// [`MAX_SHRINKS`] of them, until `encode(w, h)` fits `limits.max_bytes`;
/// without one, the fitted size alone (A1.12). A PNG has no quality, so
/// `limits.quality` is not read. `None`: nothing fits.
pub fn search_lossless<E>(
    width: u32,
    height: u32,
    limits: Limits,
    mut encode: E,
) -> Result<Option<Compressed>, String>
where
    E: FnMut(u32, u32) -> Result<Vec<u8>, String>,
{
    let mut dimension = limits.max_dimension;
    for _ in 0..=MAX_SHRINKS {
        let (w, h) = contain(width, height, dimension);
        let bytes = encode(w, h)?;
        if bytes.len() as u64 <= limits.bytes() {
            return Ok(Some(Compressed {
                bytes,
                width: w,
                height: h,
            }));
        }
        if dimension == 1 || limits.max_bytes.is_none() {
            break;
        }
        dimension = ((f64::from(dimension) * 0.8).floor() as u32).max(1);
    }
    Ok(None)
}

/// When the codec must stop starting work: checked before the decode and
/// before each trial (a trial already running finishes).
pub struct Deadline<'a> {
    pub at: Instant,
    pub check: &'a dyn Fn() -> Result<(), String>,
    pub max_pixels: u64,
    pub max_scratch_bytes: u64,
}

impl Deadline<'_> {
    pub fn check(&self) -> Result<(), String> {
        (self.check)()?;
        if Instant::now() >= self.at {
            return Err(failure("timeout", "the search ran past its time"));
        }
        Ok(())
    }
}

/// Decode `source`, then search with the platform's encoder for `format`,
/// starting no decode or trial at or after `deadline` (`timeout`).
/// `limits` are as [`check_limits`] admits them.
pub fn compress(
    source: &[u8],
    format: Format,
    limits: Limits,
    deadline: Instant,
) -> Result<Compressed, String> {
    compress_controlled(
        source,
        format,
        limits,
        deadline,
        &|| Ok(()),
        MAX_SOURCE_PIXELS,
        512 * 1024 * 1024,
    )
}

/// The native storage provider's bounded, cooperatively cancellable entry.
/// `check` is consulted before each decode/encode trial; a running native call
/// finishes before that worker's reservation is released.
/// `max_scratch_bytes` is a conservative estimate used for native-memory admission,
/// not an allocator cap on ImageIO or a bound on process RSS.
pub fn compress_controlled(
    source: &[u8],
    format: Format,
    limits: Limits,
    deadline: Instant,
    check: &dyn Fn() -> Result<(), String>,
    max_pixels: u64,
    max_scratch_bytes: u64,
) -> Result<Compressed, String> {
    compress_edited(
        source,
        format,
        limits,
        &Edit::default(),
        Deadline {
            at: deadline,
            check,
            max_pixels: max_pixels.min(MAX_SOURCE_PIXELS),
            max_scratch_bytes,
        },
    )
}

/// [`compress_controlled`] of `edit`'s picture (LLP 1133.019 D15): turned,
/// flipped and cropped before the search, which runs on the crop's size.
pub fn compress_edited(
    source: &[u8],
    format: Format,
    limits: Limits,
    edit: &Edit,
    deadline: Deadline<'_>,
) -> Result<Compressed, String> {
    check_limits(
        f64::from(limits.max_dimension),
        limits.max_bytes.map(|n| n as f64),
        limits.quality,
    )?;
    (deadline.check)()?;
    if source.len() as u64 > MAX_BYTES {
        return Err(failure(
            "too-large",
            format!("{} bytes is over {MAX_BYTES}", source.len()),
        ));
    }
    platform(source, format, limits, edit, deadline)
}

#[cfg(target_vendor = "apple")]
fn platform(
    source: &[u8],
    format: Format,
    limits: Limits,
    edit: &Edit,
    deadline: Deadline<'_>,
) -> Result<Compressed, String> {
    apple::compress(source, format, limits, edit, deadline)
}

#[cfg(not(target_vendor = "apple"))]
fn platform(
    source: &[u8],
    format: Format,
    limits: Limits,
    edit: &Edit,
    deadline: Deadline<'_>,
) -> Result<Compressed, String> {
    let codec = platform::installed().ok_or_else(unsupported)?;
    platform::compress(codec, source, format, limits, edit, deadline)
}

/// What `transformImage` wrote, and what of the source it kept: D14's
/// `TransformedImage` less its path and size, which the writer knows.
#[derive(Debug, Clone, PartialEq)]
pub struct Transformed {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub media_type: &'static str,
    /// `srgb`, `display-p3` or `other`.
    pub color_space: &'static str,
    pub hdr: bool,
    pub metadata: Metadata,
}

impl Transformed {
    /// The facts the result record carries beside its path, size and pixels.
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({
            "type": self.media_type,
            "colorSpace": self.color_space,
            "hdr": self.hdr,
            "metadata": self.metadata.word(),
        })
    }
}

/// `transformImage` (LLP 1133.019 D14): one decode, the edit, one encode,
/// keeping what `options` says (D15), in at most `max_bytes`.
pub fn transform(
    source: &[u8],
    options: &Transform,
    max_bytes: u64,
    deadline: Deadline<'_>,
) -> Result<Transformed, String> {
    (deadline.check)()?;
    if source.len() as u64 > MAX_BYTES {
        return Err(edit::failure_named(
            "transformImage",
            "too-large",
            format!("{} bytes is over {MAX_BYTES}", source.len()),
        ));
    }
    transform_platform(source, options, max_bytes, deadline)
}

#[cfg(target_vendor = "apple")]
fn transform_platform(
    source: &[u8],
    options: &Transform,
    max_bytes: u64,
    deadline: Deadline<'_>,
) -> Result<Transformed, String> {
    apple::transform(source, options, max_bytes, deadline)
}

#[cfg(not(target_vendor = "apple"))]
fn transform_platform(
    source: &[u8],
    options: &Transform,
    max_bytes: u64,
    deadline: Deadline<'_>,
) -> Result<Transformed, String> {
    let codec = platform::installed().ok_or_else(|| {
        edit::failure_named(
            "transformImage",
            "unsupported",
            "no image encoder on this host",
        )
    })?;
    platform::transform(codec, source, options, max_bytes, deadline)
}

/// What an embedder's codec returns for either call: the bytes and their
/// size, and for a transform its report and the extension a `to` without
/// one gains.
#[derive(Debug, Clone, PartialEq)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub report: Option<String>,
    pub extension: Option<&'static str>,
}

/// One request to the codec as an embedder receives it (Ibex's
/// `ImageRequest`): a compression in `format` within its limits, whose
/// `options` may carry an edit and a `quality` (A1.12), and whose
/// `max_bytes` is `None` when the call names none; or a transform, whose
/// `options` are [`edit::parse_transform`]'s and whose output is at most
/// `max_bytes`.
pub fn encode_request(
    source: &[u8],
    transform_call: bool,
    options: Option<&str>,
    format: Format,
    (max_dimension, max_bytes): (u32, Option<u64>),
    deadline: Deadline<'_>,
) -> Result<Encoded, String> {
    let call = if transform_call {
        "transformImage"
    } else {
        "compressImage"
    };
    let options: serde_json::Value = match options {
        Some(text) => serde_json::from_str(text)
            .map_err(|e| edit::failure_named(call, "invalid", format!("options: {e}")))?,
        None => serde_json::Value::Null,
    };
    if transform_call {
        let t = edit::parse_transform(&options)?;
        let out = transform(source, &t, max_bytes.unwrap_or(MAX_BYTES), deadline)?;
        return Ok(Encoded {
            report: Some(out.report().to_string()),
            extension: edit::extension(out.media_type),
            bytes: out.bytes,
            width: out.width,
            height: out.height,
        });
    }
    let edit = edit::parse_edit(call, &options)?;
    let quality = match &options["quality"] {
        serde_json::Value::Null => None,
        q => Some(
            q.as_f64()
                .ok_or_else(|| failure("invalid", "quality must be a number from 0 to 1"))?,
        ),
    };
    let limits = check_limits(
        f64::from(max_dimension),
        max_bytes.map(|n| n as f64),
        quality,
    )?;
    let out = compress_edited(source, format, limits, &edit, deadline)?;
    Ok(Encoded {
        bytes: out.bytes,
        width: out.width,
        height: out.height,
        report: None,
        extension: None,
    })
}

/// The refusal where there is no codec (A1.3).
pub fn unsupported() -> String {
    failure("unsupported", "no image encoder on this host")
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
