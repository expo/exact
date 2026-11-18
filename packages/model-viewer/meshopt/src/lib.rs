//! Port of meshoptimizer v0.25's reference decoder by Jasper St. Pierre.
//! https://github.com/zeux/meshoptimizer/blob/v0.25/js/meshopt_decoder_reference.js
//! Copyright (c) 2016-2025 Arseny Kapoulkine. MIT license: ../LICENSE.md.
mod filters;

type Result<T> = std::result::Result<T, String>;

struct Input<'a>(&'a [u8]);
impl<'a> Input<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let bytes = self.0.get(..count).ok_or("truncated meshopt stream")?;
        self.0 = &self.0[count..];
        Ok(bytes)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn varint(&mut self) -> Result<u32> {
        let mut value = 0;
        for shift in (0..35).step_by(7) {
            let b = self.byte()?;
            if shift == 28 && b > 15 {
                return Err("invalid meshopt varint".into());
            }
            value |= u32::from(b & 127) << shift;
            if b < 128 {
                return Ok(value);
            }
        }
        Err("invalid meshopt varint".into())
    }
    fn finish(&self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err("unexpected meshopt trailing data".into())
        }
    }
}
fn dezig(v: u32) -> u32 {
    (v >> 1) ^ (0u32.wrapping_sub(v & 1))
}

/// Decode an `EXT_meshopt_compression` buffer view.
///
/// `count` and `stride` are the view's element count and byte stride. `mode`
/// and `filter` take the extension's uppercase names, including `NONE`.
/// Invalid streams and decoded views above 256 MiB return an error.
pub fn decode(
    count: usize,
    stride: usize,
    source: &[u8],
    mode: &str,
    filter: &str,
) -> Result<Vec<u8>> {
    let valid = match mode {
        "ATTRIBUTES" => (4..=256).contains(&stride) && stride.is_multiple_of(4),
        "TRIANGLES" => matches!(stride, 2 | 4) && count.is_multiple_of(3),
        "INDICES" => matches!(stride, 2 | 4),
        _ => return Err(format!("unsupported meshopt mode {mode}")),
    };
    if !valid || count == 0 {
        return Err(format!("invalid meshopt {mode} count/byteStride"));
    }
    filters::validate(mode, stride, filter)?;
    let size = count
        .checked_mul(stride)
        .filter(|&n| n <= 256 * 1024 * 1024)
        .ok_or("meshopt decoded buffer exceeds 256 MiB")?;
    let mut target = vec![0; size];
    match mode {
        "ATTRIBUTES" => vertices(&mut target, count, stride, source)?,
        "TRIANGLES" => triangles(&mut target, count, stride, source)?,
        "INDICES" => sequence(&mut target, stride, source)?,
        _ => unreachable!(),
    }
    filters::apply(&mut target, stride, filter);
    Ok(target)
}

fn vertices(target: &mut [u8], count: usize, stride: usize, source: &[u8]) -> Result<()> {
    let version = match source.first() {
        Some(0xa0) => 0,
        Some(0xa1) => 1,
        _ => return Err("unsupported meshopt vertex codec version".into()),
    };
    let tail_size = stride + if version == 1 { stride / 4 } else { 0 };
    let padded = tail_size.max(if version == 0 { 32 } else { 24 });
    if source.len() < 1 + padded {
        return Err("truncated meshopt vertex tail".into());
    }
    let tail = &source[source.len() - tail_size..];
    let mut previous = tail[..stride].to_vec();
    let channels = if version == 1 {
        tail[stride..].to_vec()
    } else {
        vec![0; stride / 4]
    };
    if channels.iter().any(|c| c & 3 == 3) {
        return Err("invalid meshopt vertex channel".into());
    }
    let mut input = Input(&source[1..source.len() - padded]);
    let block_size = ((8192 / stride) & !15).min(256);
    for base in (0..count).step_by(block_size) {
        let n = (count - base).min(block_size);
        let groups = n.div_ceil(16);
        // The last group is padded to 16 even when the vertex count is not.
        let pitch = groups * 16;
        let mut deltas = vec![0u8; stride * pitch];
        let controls = if version == 1 {
            input.take(stride / 4)?
        } else {
            &[]
        };
        for byte in 0..stride {
            let control = if version == 1 {
                (controls[byte / 4] >> ((byte % 4) * 2)) & 3
            } else {
                0
            };
            let out = &mut deltas[byte * pitch..(byte + 1) * pitch];
            if control == 2 {
                continue;
            }
            if control == 3 {
                out[..n].copy_from_slice(input.take(n)?);
                continue;
            }
            let headers = input.take(groups.div_ceil(4))?;
            let modes = if version == 0 {
                [0, 2, 4, 8]
            } else if control == 0 {
                [0, 1, 2, 4]
            } else {
                [1, 2, 4, 8]
            };
            for group in 0..groups {
                let bits = modes[((headers[group / 4] >> ((group % 4) * 2)) & 3) as usize];
                let out = &mut out[group * 16..group * 16 + 16];
                if bits == 0 {
                    continue;
                }
                if bits == 8 {
                    out.copy_from_slice(input.take(16)?);
                    continue;
                }
                let packed = input.take(bits * 2)?;
                let mask = (1 << bits) - 1;
                for (i, value) in out.iter_mut().enumerate() {
                    let shift = if bits == 1 {
                        i % 8
                    } else {
                        8 - bits - (i % (8 / bits)) * bits
                    };
                    let v = (packed[i * bits / 8] >> shift) & mask as u8;
                    *value = if v == mask as u8 { input.byte()? } else { v };
                }
            }
        }
        for i in 0..n {
            for (group, &channel) in channels.iter().enumerate() {
                let width = 1 << (channel & 3);
                for byte in (group * 4..group * 4 + 4).step_by(width) {
                    let mut delta = 0u32;
                    let mut old = 0u32;
                    for k in 0..width {
                        delta |= u32::from(deltas[(byte + k) * pitch + i]) << (k * 8);
                        old |= u32::from(previous[byte + k]) << (k * 8);
                    }
                    let value = if width == 4 {
                        old ^ delta.rotate_right(u32::from(channel >> 4))
                    } else {
                        old.wrapping_add(dezig(delta))
                    };
                    let bytes = value.to_le_bytes();
                    previous[byte..byte + width].copy_from_slice(&bytes[..width]);
                }
            }
            target[(base + i) * stride..(base + i + 1) * stride].copy_from_slice(&previous);
        }
    }
    input.finish()
}

fn push<const N: usize>(fifo: &mut [u32; N], value: u32) {
    fifo.copy_within(..N - 1, 1);
    fifo[0] = value;
}
fn next_index(next: &mut u32) -> u32 {
    let value = *next;
    *next = next.wrapping_add(1);
    value
}
fn delta_index(input: &mut Input<'_>, last: &mut u32) -> Result<u32> {
    *last = last.wrapping_add(dezig(input.varint()?));
    Ok(*last)
}
fn triangles(target: &mut [u8], count: usize, stride: usize, source: &[u8]) -> Result<()> {
    let version = match source.first() {
        Some(0xe0) => 0,
        Some(0xe1) => 1,
        _ => return Err("unsupported meshopt triangle codec version".into()),
    };
    let tris = count / 3;
    if source.len() < 1 + tris + 16 {
        return Err("truncated meshopt triangle stream".into());
    }
    let table = &source[source.len() - 16..];
    let mut input = Input(&source[1 + tris..source.len() - 16]);
    let mut edge = [u32::MAX; 32];
    let mut vertex = [u32::MAX; 16];
    let (mut next, mut last) = (0u32, 0u32);
    for (tri, &code) in source[1..1 + tris].iter().enumerate() {
        let (b0, b1) = ((code >> 4) as usize, (code & 15) as usize);
        let (a, b, c);
        if b0 < 15 {
            a = edge[b0 * 2];
            b = edge[b0 * 2 + 1];
            c = if b1 == 0 {
                next_index(&mut next)
            } else if b1 < if version == 0 { 15 } else { 13 } {
                vertex[b1]
            } else if b1 == 13 {
                last = last.wrapping_sub(1);
                last
            } else if b1 == 14 {
                last = last.wrapping_add(1);
                last
            } else {
                delta_index(&mut input, &mut last)?
            };
            if b1 == 0 || b1 >= if version == 0 { 15 } else { 13 } {
                push(&mut vertex, c);
            }
            for value in [b, c, c, a] {
                push(&mut edge, value);
            }
        } else {
            let aux = if b1 < 14 { table[b1] } else { input.byte()? };
            let (z, w) = ((aux >> 4) as usize, (aux & 15) as usize);
            if b1 >= 14 && aux == 0 {
                next = 0;
            }
            a = if b1 == 15 {
                delta_index(&mut input, &mut last)?
            } else {
                next_index(&mut next)
            };
            b = if z == 0 {
                next_index(&mut next)
            } else if z == 15 && b1 >= 14 {
                delta_index(&mut input, &mut last)?
            } else {
                vertex[z - 1]
            };
            c = if w == 0 {
                next_index(&mut next)
            } else if w == 15 && b1 >= 14 {
                delta_index(&mut input, &mut last)?
            } else {
                vertex[w - 1]
            };
            push(&mut vertex, a);
            if z == 0 || (z == 15 && b1 >= 14) {
                push(&mut vertex, b);
            }
            if w == 0 || (w == 15 && b1 >= 14) {
                push(&mut vertex, c);
            }
            for value in [a, b, b, c, c, a] {
                push(&mut edge, value);
            }
        }
        for (i, value) in [a, b, c].into_iter().enumerate() {
            let offset = (tri * 3 + i) * stride;
            target[offset..offset + stride].copy_from_slice(&value.to_le_bytes()[..stride]);
        }
    }
    input.finish()
}
fn sequence(target: &mut [u8], stride: usize, source: &[u8]) -> Result<()> {
    if !matches!(source.first(), Some(0xd0 | 0xd1)) {
        return Err("unsupported meshopt index sequence codec version".into());
    }
    if source.len() < 5 {
        return Err("truncated meshopt index sequence tail".into());
    }
    let mut input = Input(&source[1..source.len() - 4]);
    let mut last = [0u32; 2];
    for out in target.chunks_exact_mut(stride) {
        let v = input.varint()?;
        let index = (v & 1) as usize;
        last[index] = last[index].wrapping_add(dezig(v >> 1));
        out.copy_from_slice(&last[index].to_le_bytes()[..stride]);
    }
    input.finish()
}
