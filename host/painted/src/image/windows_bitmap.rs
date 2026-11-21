//! Bounded metadata for the Windows bitmap and icon codecs.
use super::png_decode::{inspect_png, Header};
use exact_raster::{Metadata, PixelSize, Refusal, MAX_HEADER_BYTES, MAX_SOURCE_PIXELS};
use std::io::{BufReader, Read, Seek, SeekFrom};

#[derive(Clone, Copy, Debug)]
pub(super) struct IconEntry {
    pub(super) index: u32,
    pub(super) offset: u32,
    pub(super) length: u32,
}

pub(super) fn inspect(
    head: &[u8],
    input: &mut (impl Read + Seek),
    encoded_bytes: u64,
) -> Result<Option<Header>, Refusal> {
    let (natural, header_bytes, icon) = if head.starts_with(b"BM") {
        let (size, bytes) = bitmap_size(head, encoded_bytes)?;
        (size, bytes, None)
    } else if head.starts_with(&[0, 0, 1, 0]) {
        let (natural, bytes, icon) = icon_entry(head, input, encoded_bytes)?;
        (natural, bytes, Some(icon))
    } else {
        return Ok(None);
    };
    if natural.width == 0 || natural.height == 0 {
        return Err(Refusal::InvalidDimensions);
    }
    if u64::from(natural.width) * u64::from(natural.height) > MAX_SOURCE_PIXELS {
        return Err(Refusal::SourcePixels);
    }
    let mut header = Header::platform(Metadata {
        natural,
        encoded_bytes,
        header_bytes,
    });
    if let Some(entry) = icon {
        // Keep the PNG decoder's sampling and alpha rounding when the icon wraps a PNG.
        let mut prefix = [0; 33];
        let len = (entry.length as usize).min(prefix.len());
        input
            .seek(SeekFrom::Start(u64::from(entry.offset)))
            .map_err(|_| Refusal::DecodeFailed)?;
        input
            .read_exact(&mut prefix[..len])
            .map_err(|_| Refusal::DecodeFailed)?;
        if prefix[..len].starts_with(b"\x89PNG\r\n\x1a\n") {
            header = inspect_png(
                &prefix[..len],
                input,
                u64::from(entry.length),
                u64::from(entry.offset),
            )?;
            if header.metadata.natural != natural {
                return Err(Refusal::ConflictingMetadata);
            }
            header.metadata.header_bytes += header_bytes;
            if header.metadata.header_bytes > MAX_HEADER_BYTES {
                return Err(Refusal::HeaderLimit);
            }
            header.metadata.encoded_bytes = encoded_bytes;
        }
        header.icon = Some(entry);
    }
    Ok(Some(header))
}

fn bitmap_size(head: &[u8], encoded: u64) -> Result<(PixelSize, u64), Refusal> {
    let header: &[u8; 26] = head.first_chunk().ok_or(Refusal::DecodeFailed)?;
    let dib = u32::from_le_bytes(header[14..18].try_into().unwrap());
    let pixels = u32::from_le_bytes(header[10..14].try_into().unwrap());
    if u64::from(pixels) < 14 + u64::from(dib) || u64::from(pixels) >= encoded {
        return Err(Refusal::DecodeFailed);
    }
    let (width, height) = match dib {
        12 => (
            u32::from(u16::from_le_bytes([header[18], header[19]])),
            u32::from(u16::from_le_bytes([header[20], header[21]])),
        ),
        40.. => (
            u32::try_from(i32::from_le_bytes(header[18..22].try_into().unwrap()))
                .map_err(|_| Refusal::InvalidDimensions)?,
            i32::from_le_bytes(header[22..26].try_into().unwrap()).unsigned_abs(),
        ),
        _ => return Err(Refusal::DecodeFailed),
    };
    Ok((PixelSize { width, height }, 26))
}

fn icon_entry(
    head: &[u8],
    input: &mut (impl Read + Seek),
    encoded: u64,
) -> Result<(PixelSize, u64, IconEntry), Refusal> {
    let header: &[u8; 6] = head.first_chunk().ok_or(Refusal::DecodeFailed)?;
    let count = u32::from(u16::from_le_bytes([header[4], header[5]]));
    let end = 6 + u64::from(count) * 16;
    if end > MAX_HEADER_BYTES {
        return Err(Refusal::HeaderLimit);
    }
    if end > encoded {
        return Err(Refusal::DecodeFailed);
    }
    input
        .seek(SeekFrom::Start(6))
        .map_err(|_| Refusal::DecodeFailed)?;
    let mut directory = BufReader::new(input.take(end - 6));
    let mut best = None;
    for index in 0..count {
        let mut row = [0; 16];
        directory
            .read_exact(&mut row)
            .map_err(|_| Refusal::DecodeFailed)?;
        let length = u32::from_le_bytes(row[8..12].try_into().unwrap());
        let offset = u32::from_le_bytes(row[12..16].try_into().unwrap());
        if length == 0 || u64::from(offset) < end || u64::from(offset) + u64::from(length) > encoded
        {
            return Err(Refusal::DecodeFailed);
        }
        // Zero encodes 256. The largest representation sets the image's natural size.
        let extent = |n: u8| if n == 0 { 256 } else { u32::from(n) };
        let size = PixelSize {
            width: extent(row[0]),
            height: extent(row[1]),
        };
        let rank = (
            size.width * size.height,
            u16::from_le_bytes([row[6], row[7]]),
        );
        if best
            .as_ref()
            .is_none_or(|(previous, _, _)| rank > *previous)
        {
            best = Some((
                rank,
                size,
                IconEntry {
                    index,
                    offset,
                    length,
                },
            ));
        }
    }
    let (_, size, entry) = best.ok_or(Refusal::DecodeFailed)?;
    Ok((size, end, entry))
}
