//! The codecs behind the data module's `DecompressionStream` and
//! `CompressionStream` (the WHATWG Compression standard,
//! <https://compression.spec.whatwg.org/>; issue
//! `20261023-a-stream-mapper-cannot-yield`, Charlie, 2026-11-10). `streams.js`
//! hands each chunk through the bytes door (`crypto.rs`, ops 12 to 15) and
//! the work runs here, at native speed, inside the turn that wrote the chunk.
//!
//! One instance is one stream: a `deflate` (zlib, RFC 1950), `deflate-raw`
//! (RFC 1951) or `gzip` (RFC 1952, one member) context that keeps its
//! window across chunks, so a `zlib-stream` whose messages each end in a
//! sync flush inflates message by message. As the standard says, data after
//! the end of the compressed input, corrupt data, and input that ends before
//! its end are errors (a `TypeError` in JavaScript).

use miniz_oxide::deflate::core::{create_comp_flags_from_zip_params, CompressorOxide};
use miniz_oxide::inflate::stream::InflateState;
use miniz_oxide::{DataFormat, MZError, MZFlush, MZStatus};

/// Output grows by at least this much per step.
const STEP: usize = 64 << 10;
/// Maximum gzip member header size (RFC 1952's 64 KiB `FEXTRA` plus name/comment headroom).
const MAX_GZIP_HEADER: usize = 128 << 10;
/// Maximum open codec contexts per module.
pub(crate) const MAX_CODECS: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Format {
    Deflate,
    DeflateRaw,
    Gzip,
}

impl Format {
    /// The standard's `CompressionFormat` this executor supports (`brotli` is not).
    pub(crate) fn parse(name: &str) -> Option<Format> {
        match name {
            "deflate" => Some(Format::Deflate),
            "deflate-raw" => Some(Format::DeflateRaw),
            "gzip" => Some(Format::Gzip),
            _ => None,
        }
    }
}

/// Native memory bounds for one module's codecs, derived from its heap ceiling.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub(crate) max_output: usize,
    pub(crate) max_codecs: usize,
    pub(crate) max_codec_bytes: usize,
}

impl Limits {
    pub(crate) fn for_heap(max_heap: usize) -> Self {
        let min_codec = std::mem::size_of::<CompressorOxide>();
        Self {
            max_output: (max_heap / 2).max(1),
            max_codecs: MAX_CODECS,
            max_codec_bytes: (max_heap / 4).max(min_codec),
        }
    }
}

/// Release large transient output capacity while keeping a warm [`STEP`] buffer.
pub(crate) fn release_output(out: &mut Vec<u8>) {
    if out.capacity() > STEP {
        *out = Vec::new();
    } else {
        out.clear();
    }
}

/// Streaming RFC 1952 §2.3 gzip member header parser (O(1) memory, O(N) scan).
enum HeaderState {
    Fixed { buf: [u8; 10], len: u8 },
    ExtraLen { buf: [u8; 2], len: u8 },
    Extra { remaining: usize },
    Name,
    Comment,
    Crc { buf: [u8; 2], len: u8 },
    Done,
}

impl Default for HeaderState {
    fn default() -> Self {
        Self::Fixed {
            buf: [0; 10],
            len: 0,
        }
    }
}

impl HeaderState {
    fn after_fixed(flags: u8) -> Self {
        if flags & 4 != 0 {
            Self::ExtraLen {
                buf: [0; 2],
                len: 0,
            }
        } else {
            Self::after_extra(flags)
        }
    }

    fn after_extra(flags: u8) -> Self {
        if flags & 8 != 0 {
            Self::Name
        } else {
            Self::after_name(flags)
        }
    }

    fn after_name(flags: u8) -> Self {
        if flags & 16 != 0 {
            Self::Comment
        } else {
            Self::after_comment(flags)
        }
    }

    fn after_comment(flags: u8) -> Self {
        if flags & 2 != 0 {
            Self::Crc {
                buf: [0; 2],
                len: 0,
            }
        } else {
            Self::Done
        }
    }
}

/// A gzip member's header and trailer around its deflate data.
#[derive(Default)]
struct Gzip {
    header: HeaderState,
    flags: u8,
    header_bytes: usize,
    header_crc: crc32fast::Hasher,
    crc: crc32fast::Hasher,
    size: u32,
    trailer: [u8; 8],
    trailer_len: u8,
}

impl Gzip {
    fn header_done(&self) -> bool {
        matches!(self.header, HeaderState::Done)
    }

    /// Consume header bytes from `input`, returning the remaining deflate bytes
    /// once the header is complete (or empty while more header bytes are needed).
    fn feed_header<'a>(&mut self, mut input: &'a [u8]) -> Result<&'a [u8], String> {
        let bad = || Err::<&'a [u8], String>("the gzip header is not valid".into());
        while !self.header_done() {
            if input.is_empty() {
                return Ok(input);
            }
            match &mut self.header {
                HeaderState::Fixed { buf, len } => {
                    let take = input.len().min(10 - *len as usize);
                    buf[*len as usize..*len as usize + take].copy_from_slice(&input[..take]);
                    *len += take as u8;
                    input = &input[take..];
                    if (*len >= 1 && buf[0] != 0x1f)
                        || (*len >= 2 && buf[1] != 0x8b)
                        || (*len >= 3 && buf[2] != 8)
                    {
                        return bad();
                    }
                    if *len < 10 {
                        return Ok(input);
                    }
                    let flags = buf[3];
                    if flags & 0xe0 != 0 {
                        return bad();
                    }
                    self.flags = flags;
                    self.header_bytes = 10;
                    if flags & 2 != 0 {
                        self.header_crc.update(buf);
                    }
                    self.header = HeaderState::after_fixed(flags);
                }
                HeaderState::ExtraLen { buf, len } => {
                    let take = input.len().min(2 - *len as usize);
                    buf[*len as usize..*len as usize + take].copy_from_slice(&input[..take]);
                    *len += take as u8;
                    input = &input[take..];
                    if *len < 2 {
                        return Ok(input);
                    }
                    let xlen = u16::from_le_bytes(*buf) as usize;
                    self.header_bytes += 2;
                    if self.flags & 2 != 0 {
                        self.header_crc.update(buf);
                    }
                    self.header = if xlen == 0 {
                        HeaderState::after_extra(self.flags)
                    } else {
                        HeaderState::Extra { remaining: xlen }
                    };
                }
                HeaderState::Extra { remaining } => {
                    let take = input.len().min(*remaining);
                    self.header_bytes += take;
                    if self.flags & 2 != 0 {
                        self.header_crc.update(&input[..take]);
                    }
                    input = &input[take..];
                    *remaining -= take;
                    if *remaining == 0 {
                        self.header = HeaderState::after_extra(self.flags);
                    }
                }
                HeaderState::Name | HeaderState::Comment => {
                    let is_name = matches!(self.header, HeaderState::Name);
                    let allow = MAX_GZIP_HEADER.saturating_sub(self.header_bytes);
                    let scan = &input[..input.len().min(allow)];
                    if let Some(zero) = scan.iter().position(|&b| b == 0) {
                        let take = zero + 1;
                        self.header_bytes += take;
                        if self.flags & 2 != 0 {
                            self.header_crc.update(&input[..take]);
                        }
                        input = &input[take..];
                        self.header = if is_name {
                            HeaderState::after_name(self.flags)
                        } else {
                            HeaderState::after_comment(self.flags)
                        };
                    } else if input.len() >= allow {
                        return Err("the gzip header is too large".into());
                    } else {
                        self.header_bytes += input.len();
                        if self.flags & 2 != 0 {
                            self.header_crc.update(input);
                        }
                        return Ok(&[]);
                    }
                }
                HeaderState::Crc { buf, len } => {
                    let take = input.len().min(2 - *len as usize);
                    buf[*len as usize..*len as usize + take].copy_from_slice(&input[..take]);
                    *len += take as u8;
                    input = &input[take..];
                    if *len < 2 {
                        return Ok(input);
                    }
                    let expected = std::mem::take(&mut self.header_crc).finalize() as u16;
                    if u16::from_le_bytes(*buf) != expected {
                        return bad();
                    }
                    self.header = HeaderState::Done;
                }
                HeaderState::Done => break,
            }
        }
        Ok(input)
    }
}

/// One decompression context.
pub(crate) struct Decompressor {
    format: Format,
    inflate: Box<InflateState>,
    gzip: Gzip,
    /// The deflate data reached its last block.
    inflated: bool,
    /// The end of the compressed input: anything after it is an error.
    ended: bool,
}

const CORRUPT: &str = "the compressed data is corrupt";
const TRAILING: &str = "there is data after the end of the compressed input";
const TOO_LARGE: &str = "the output exceeds the module's memory limit";
const INTERRUPTED: &str = "the codec was interrupted";
const TOO_MANY_CODECS: &str = "too many open compression streams";

impl Decompressor {
    pub(crate) fn new(format: Format) -> Decompressor {
        let data = match format {
            Format::Deflate => DataFormat::Zlib,
            Format::DeflateRaw | Format::Gzip => DataFormat::Raw,
        };
        Decompressor {
            format,
            inflate: InflateState::new_boxed(data),
            gzip: Gzip::default(),
            inflated: false,
            ended: false,
        }
    }

    /// Decompress `input`, appending what it yields to `out` up to `limit` bytes:
    /// all of it, so a chunk that ends in a sync flush yields everything it encodes.
    pub(crate) fn write(
        &mut self,
        mut input: &[u8],
        out: &mut Vec<u8>,
        limit: usize,
        interrupted: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        if self.ended {
            return if input.is_empty() {
                Ok(())
            } else {
                Err(TRAILING.into())
            };
        }
        if self.format == Format::Gzip && !self.gzip.header_done() {
            input = self.gzip.feed_header(input)?;
            if !self.gzip.header_done() {
                return Ok(());
            }
        }
        if !self.inflated {
            let start = out.len();
            input = self.inflate_into(input, out, limit, interrupted)?;
            if self.format == Format::Gzip {
                self.gzip.crc.update(&out[start..]);
                let n = (out.len() - start) as u64;
                self.gzip.size = self.gzip.size.wrapping_add(n as u32);
            }
        }
        if !self.inflated {
            return Ok(());
        }
        if self.format != Format::Gzip {
            self.ended = true;
            return if input.is_empty() {
                Ok(())
            } else {
                Err(TRAILING.into())
            };
        }
        let len = self.gzip.trailer_len as usize;
        let take = input.len().min(8 - len);
        self.gzip.trailer[len..len + take].copy_from_slice(&input[..take]);
        self.gzip.trailer_len += take as u8;
        if self.gzip.trailer_len < 8 {
            return Ok(());
        }
        let word = |at: usize| {
            let b = &self.gzip.trailer[at..at + 4];
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        };
        let crc = std::mem::take(&mut self.gzip.crc).finalize();
        if word(0) != crc || word(4) != self.gzip.size {
            return Err(CORRUPT.into());
        }
        self.ended = true;
        if take < input.len() {
            return Err(TRAILING.into());
        }
        Ok(())
    }

    /// The input is over: an error unless it reached its end.
    pub(crate) fn finish(&mut self) -> Result<(), String> {
        if self.ended {
            Ok(())
        } else {
            Err("the compressed input ended before its end".into())
        }
    }

    /// Inflate until more input is needed or the deflate data ends,
    /// returning the unconsumed input.
    fn inflate_into<'a>(
        &mut self,
        mut input: &'a [u8],
        out: &mut Vec<u8>,
        limit: usize,
        interrupted: &dyn Fn() -> bool,
    ) -> Result<&'a [u8], String> {
        let mut at = out.len();
        loop {
            if interrupted() {
                out.truncate(at);
                return Err(INTERRUPTED.into());
            }
            if at == out.len() {
                if at >= limit {
                    // Check whether the stream finishes or consumes trailer bytes without writing more output.
                    let mut extra = [0u8; 1];
                    let r = miniz_oxide::inflate::stream::inflate(
                        &mut self.inflate,
                        input,
                        &mut extra,
                        MZFlush::None,
                    );
                    if r.bytes_written > 0 {
                        out.truncate(at);
                        return Err(TOO_LARGE.into());
                    }
                    input = &input[r.bytes_consumed..];
                    match r.status {
                        Ok(MZStatus::StreamEnd) => {
                            self.inflated = true;
                            break;
                        }
                        Ok(_) if input.is_empty() => break,
                        Ok(_) if r.bytes_consumed == 0 => {
                            out.truncate(at);
                            return Err(CORRUPT.into());
                        }
                        Ok(_) => continue,
                        Err(MZError::Buf) if input.is_empty() => break,
                        Err(_) => {
                            out.truncate(at);
                            return Err(CORRUPT.into());
                        }
                    }
                }
                let grow = at.max(STEP).max(input.len().saturating_mul(4));
                let next = at.saturating_add(grow).min(limit);
                out.try_reserve_exact(next - out.len())
                    .map_err(|_| TOO_LARGE.to_string())?;
                out.resize(next, 0);
            }
            let r = miniz_oxide::inflate::stream::inflate(
                &mut self.inflate,
                input,
                &mut out[at..],
                MZFlush::None,
            );
            input = &input[r.bytes_consumed..];
            at += r.bytes_written;
            let full = at == out.len();
            match r.status {
                Ok(MZStatus::StreamEnd) => {
                    self.inflated = true;
                    break;
                }
                // Output to come still: go on into more room.
                Ok(_) if full => {}
                // The inflater can hold output after consuming the last input byte.
                Ok(_) if input.is_empty() && r.bytes_written == 0 => break,
                // Neither output room nor input was the reason it stopped.
                Ok(_) if r.bytes_consumed == 0 && r.bytes_written == 0 => {
                    out.truncate(at);
                    return Err(CORRUPT.into());
                }
                Ok(_) => {}
                // No progress: it needs more input than this chunk had.
                Err(MZError::Buf) if input.is_empty() => break,
                Err(MZError::Buf) if full => {}
                Err(_) => {
                    out.truncate(at);
                    return Err(CORRUPT.into());
                }
            }
        }
        out.truncate(at);
        Ok(input)
    }
}

/// One compression context, at zlib's default level (6).
pub(crate) struct Compressor {
    format: Format,
    deflate: Box<CompressorOxide>,
    started: bool,
    crc: crc32fast::Hasher,
    size: u32,
}

impl Compressor {
    pub(crate) fn new(format: Format) -> Compressor {
        let bits = if format == Format::Deflate { 15 } else { -15 };
        let flags = create_comp_flags_from_zip_params(6, bits, 0);
        Compressor {
            format,
            deflate: Box::new(CompressorOxide::new(flags)),
            started: false,
            crc: crc32fast::Hasher::new(),
            size: 0,
        }
    }

    /// Compress `input`, appending what the compressor has ready to `out`.
    pub(crate) fn write(
        &mut self,
        input: &[u8],
        out: &mut Vec<u8>,
        limit: usize,
        interrupted: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        self.start(out, limit)?;
        if self.format == Format::Gzip {
            self.crc.update(input);
            self.size = self.size.wrapping_add(input.len() as u32);
        }
        self.run(input, MZFlush::None, out, limit, interrupted)
    }

    /// End the compressed data, appending the rest of it (and gzip's trailer).
    pub(crate) fn finish(
        &mut self,
        out: &mut Vec<u8>,
        limit: usize,
        interrupted: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        self.start(out, limit)?;
        self.run(&[], MZFlush::Finish, out, limit, interrupted)?;
        if self.format == Format::Gzip {
            if out.len().saturating_add(8) > limit {
                return Err(TOO_LARGE.into());
            }
            out.try_reserve_exact(8)
                .map_err(|_| TOO_LARGE.to_string())?;
            let crc = std::mem::take(&mut self.crc).finalize();
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&self.size.to_le_bytes());
        }
        Ok(())
    }

    fn start(&mut self, out: &mut Vec<u8>, limit: usize) -> Result<(), String> {
        if !self.started && self.format == Format::Gzip {
            if out.len().saturating_add(10) > limit {
                return Err(TOO_LARGE.into());
            }
            out.try_reserve_exact(10)
                .map_err(|_| TOO_LARGE.to_string())?;
            // No name, no time; "unknown" OS (255), as Chrome writes it.
            out.extend_from_slice(&[0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255]);
        }
        self.started = true;
        Ok(())
    }

    fn run(
        &mut self,
        mut input: &[u8],
        flush: MZFlush,
        out: &mut Vec<u8>,
        limit: usize,
        interrupted: &dyn Fn() -> bool,
    ) -> Result<(), String> {
        let mut at = out.len();
        loop {
            if interrupted() {
                out.truncate(at);
                return Err(INTERRUPTED.into());
            }
            if at == out.len() {
                if at >= limit {
                    let mut extra = [0u8; 16];
                    let r = miniz_oxide::deflate::stream::deflate(
                        &mut self.deflate,
                        input,
                        &mut extra,
                        flush,
                    );
                    if r.bytes_written > 0 {
                        out.truncate(at);
                        return Err(TOO_LARGE.into());
                    }
                    input = &input[r.bytes_consumed..];
                    match r.status {
                        Ok(MZStatus::StreamEnd) => break,
                        Ok(_) | Err(MZError::Buf)
                            if input.is_empty() && flush != MZFlush::Finish =>
                        {
                            break;
                        }
                        Ok(_) | Err(MZError::Buf) if r.bytes_consumed == 0 => {
                            out.truncate(at);
                            return Err(TOO_LARGE.into());
                        }
                        Ok(_) | Err(MZError::Buf) => continue,
                        Err(e) => {
                            out.truncate(at);
                            return Err(format!("compression failed: {e:?}"));
                        }
                    }
                }
                let grow = at.max(STEP).max(input.len() / 2);
                let next = at.saturating_add(grow).min(limit);
                out.try_reserve_exact(next - out.len())
                    .map_err(|_| TOO_LARGE.to_string())?;
                out.resize(next, 0);
            }
            let r = miniz_oxide::deflate::stream::deflate(
                &mut self.deflate,
                input,
                &mut out[at..],
                flush,
            );
            input = &input[r.bytes_consumed..];
            at += r.bytes_written;
            let full = at == out.len();
            match r.status {
                Ok(MZStatus::StreamEnd) => break,
                Ok(_) | Err(MZError::Buf) if full => {}
                Ok(_) | Err(MZError::Buf) if input.is_empty() && flush != MZFlush::Finish => break,
                Ok(_) => {}
                Err(e) => {
                    out.truncate(at);
                    return Err(format!("compression failed: {e:?}"));
                }
            }
        }
        out.truncate(at);
        Ok(())
    }
}

/// One stream's context.
pub(crate) enum Codec {
    Decompress(Decompressor),
    Compress(Compressor),
}

impl Codec {
    fn native_bytes(&self) -> usize {
        match self {
            Codec::Decompress(_) => std::mem::size_of::<InflateState>(),
            Codec::Compress(_) => std::mem::size_of::<CompressorOxide>(),
        }
    }
}

/// The bytes door's codec ops (`crypto.rs` `bytes_door`), by handle into
/// `codecs`, whose handles share the key handles' counter so op 10 releases
/// either: 12 `"d <format>"` or `"c <format>"` makes a context and answers
/// its handle; 13 `"<handle> <offset> <length>"` writes that range of the
/// buffer and 14 `"<handle>"` ends the input, each answering the length of
/// what it yielded, which waits in `out` until op 15 copies it into a
/// buffer of that length. A context that fails is dropped: its stream is
/// errored, and the standard never writes to one again.
#[allow(clippy::too_many_arguments)]
pub(crate) fn door(
    codecs: &mut std::collections::HashMap<u64, Codec>,
    next: &mut u64,
    out: &mut Vec<u8>,
    limits: Limits,
    interrupted: &dyn Fn() -> bool,
    op: u32,
    a: &str,
    data: &[u8],
) -> Result<String, String> {
    let handle = |text: &str| {
        text.parse::<u64>()
            .map_err(|_| "a codec handle that is not a number".to_string())
    };
    match op {
        12 => {
            let (kind, name) = a.split_once(' ').unwrap_or((a, ""));
            let format =
                Format::parse(name).ok_or_else(|| format!("unsupported format \"{name}\""))?;
            let needed = match kind {
                "d" => std::mem::size_of::<InflateState>(),
                _ => std::mem::size_of::<CompressorOxide>(),
            };
            let used: usize = codecs.values().map(Codec::native_bytes).sum();
            if codecs.len() >= limits.max_codecs
                || used.saturating_add(needed) > limits.max_codec_bytes
            {
                return Err(TOO_MANY_CODECS.into());
            }
            let codec = match kind {
                "d" => Codec::Decompress(Decompressor::new(format)),
                _ => Codec::Compress(Compressor::new(format)),
            };
            let id = *next;
            *next += 1;
            codecs.insert(id, codec);
            Ok(id.to_string())
        }
        13 | 14 => {
            let mut words = a.split(' ');
            let id = handle(words.next().unwrap_or(""))?;
            let range = if op == 13 {
                let at = words.next().and_then(|w| w.parse::<usize>().ok());
                let n = words.next().and_then(|w| w.parse::<usize>().ok());
                match (at, n) {
                    (Some(at), Some(n))
                        if at.checked_add(n).is_some_and(|end| end <= data.len()) =>
                    {
                        Some(&data[at..at + n])
                    }
                    _ => return Err("a range outside the chunk's buffer".into()),
                }
            } else {
                None
            };
            let codec = codecs.get_mut(&id).ok_or("the stream's context is gone")?;
            release_output(out);
            let done = match (codec, range) {
                (Codec::Decompress(d), Some(input)) => {
                    d.write(input, out, limits.max_output, interrupted)
                }
                (Codec::Decompress(d), None) => d.finish(),
                (Codec::Compress(c), Some(input)) => {
                    c.write(input, out, limits.max_output, interrupted)
                }
                (Codec::Compress(c), None) => c.finish(out, limits.max_output, interrupted),
            };
            if done.is_err() || op == 14 {
                codecs.remove(&id);
            }
            match done {
                Ok(()) => {
                    let n = out.len();
                    if n == 0 {
                        release_output(out);
                    }
                    Ok(n.to_string())
                }
                Err(e) => {
                    release_output(out);
                    Err(e)
                }
            }
        }
        _ => Err(format!("__exact_bytes: no op {op}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miniz_oxide::deflate::stream::deflate;

    fn json(n: usize) -> Vec<u8> {
        let mut s = String::from("[");
        for i in 0..n {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"id\":\"{}\",\"name\":\"guild {i}\",\"members\":{}}}",
                1_000_000_007u64.wrapping_mul(i as u64 + 1),
                i % 977
            ));
        }
        s.push(']');
        s.into_bytes()
    }

    const TEST_LIMIT: usize = 32 << 20;
    fn never() -> bool {
        false
    }

    fn inflate_all(format: Format, chunks: &[&[u8]]) -> Result<Vec<u8>, String> {
        let mut d = Decompressor::new(format);
        let mut out = Vec::new();
        for chunk in chunks {
            d.write(chunk, &mut out, TEST_LIMIT, &never)?;
        }
        d.finish()?;
        Ok(out)
    }

    fn compress_all(format: Format, input: &[u8]) -> Vec<u8> {
        let mut c = Compressor::new(format);
        let mut out = Vec::new();
        c.write(input, &mut out, TEST_LIMIT, &never).unwrap();
        c.finish(&mut out, TEST_LIMIT, &never).unwrap();
        out
    }

    #[test]
    fn deflate_inflates_whole_and_byte_by_byte() {
        let text = json(200);
        let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&text, 6);
        assert_eq!(inflate_all(Format::Deflate, &[&zlib]).unwrap(), text);
        let bytes: Vec<&[u8]> = zlib.chunks(1).collect();
        assert_eq!(inflate_all(Format::Deflate, &bytes).unwrap(), text);
        let raw = miniz_oxide::deflate::compress_to_vec(&text, 6);
        assert_eq!(inflate_all(Format::DeflateRaw, &[&raw]).unwrap(), text);
    }

    #[test]
    fn a_final_match_crossing_a_window_inflates_whole() {
        for len in [32768, 32769, 65537, 98305] {
            let input = vec![0; len];
            for format in [Format::DeflateRaw, Format::Deflate, Format::Gzip] {
                let packed = compress_all(format, &input);
                for chunk_len in [packed.len(), 1] {
                    let mut d = Decompressor::new(format);
                    let mut out = Vec::new();
                    for chunk in packed.chunks(chunk_len) {
                        d.write(chunk, &mut out, TEST_LIMIT, &never).unwrap();
                    }
                    assert_eq!(out.len(), len, "{format:?}, chunk size {chunk_len}");
                    assert_eq!(out, input);
                    d.finish().unwrap();
                }
            }
        }
    }

    #[test]
    fn every_format_round_trips_through_the_compressor() {
        let text = json(500);
        for format in [Format::Deflate, Format::DeflateRaw, Format::Gzip] {
            let packed = compress_all(format, &text);
            assert!(packed.len() < text.len() / 3, "{format:?}");
            assert_eq!(inflate_all(format, &[&packed]).unwrap(), text, "{format:?}");
        }
        // Empty input is a whole, empty stream.
        for format in [Format::Deflate, Format::DeflateRaw, Format::Gzip] {
            let packed = compress_all(format, b"");
            assert_eq!(inflate_all(format, &[&packed]).unwrap(), b"", "{format:?}");
        }
    }

    #[test]
    fn a_gzip_header_with_every_field_split_across_chunks() {
        let text = b"hello, header".to_vec();
        let raw = miniz_oxide::deflate::compress_to_vec(&text, 6);
        let mut gz = vec![
            0x1f,
            0x8b,
            8,
            4 | 8 | 16 | 2,
            0,
            0,
            0,
            0,
            0,
            3,
            3,
            0,
            b'a',
            b'b',
            b'c',
        ];
        gz.extend_from_slice(b"name\0comment\0");
        // FHCRC: the low 16 bits of the CRC32 of the header before it.
        let fhcrc = crc32fast::hash(&gz) as u16;
        assert_eq!(fhcrc, 0x7179);
        gz.extend_from_slice(&fhcrc.to_le_bytes());
        gz.extend_from_slice(&raw);
        let mut crc = crc32fast::Hasher::new();
        crc.update(&text);
        gz.extend_from_slice(&crc.finalize().to_le_bytes());
        gz.extend_from_slice(&(text.len() as u32).to_le_bytes());
        let bytes: Vec<&[u8]> = gz.chunks(1).collect();
        assert_eq!(inflate_all(Format::Gzip, &bytes).unwrap(), text);
        // A wrong header checksum is refused, whole or split.
        let at = gz.len() - raw.len() - 10;
        gz[at] ^= 1;
        let bytes: Vec<&[u8]> = gz.chunks(1).collect();
        for chunks in [&bytes[..], &[&gz[..]]] {
            assert_eq!(
                inflate_all(Format::Gzip, chunks).unwrap_err(),
                "the gzip header is not valid"
            );
        }
    }

    /// Discord's `zlib-stream`: one zlib stream across many messages, each
    /// ending in a sync flush (`00 00 ff ff`), each inflated on its own.
    #[test]
    fn a_sync_flushed_stream_inflates_message_by_message() {
        let messages: Vec<Vec<u8>> = (0..20).map(|i| json(i * 7 + 1)).collect();
        let mut c = CompressorOxide::new(create_comp_flags_from_zip_params(6, 15, 0));
        let mut d = Decompressor::new(Format::Deflate);
        for message in &messages {
            let mut packet = vec![0; message.len() + 1024];
            let r = deflate(&mut c, message, &mut packet, MZFlush::Sync);
            assert_eq!(r.bytes_consumed, message.len());
            packet.truncate(r.bytes_written);
            assert!(packet.ends_with(&[0, 0, 0xff, 0xff]));
            let mut out = Vec::new();
            d.write(&packet, &mut out, TEST_LIMIT, &never).unwrap();
            assert_eq!(&out, message, "each packet yields its whole message");
        }
        // The stream never ended: closing it is the standard's error.
        assert_eq!(
            d.finish().unwrap_err(),
            "the compressed input ended before its end"
        );
    }

    #[test]
    fn corrupt_truncated_and_trailing_input_are_errors() {
        let text = json(50);
        let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&text, 6);
        // A bad header.
        assert_eq!(
            inflate_all(Format::Deflate, &[b"\x00\x01garbage"]).unwrap_err(),
            CORRUPT
        );
        // A wrong Adler-32.
        let mut bad = zlib.clone();
        let n = bad.len();
        bad[n - 1] ^= 1;
        assert_eq!(inflate_all(Format::Deflate, &[&bad]).unwrap_err(), CORRUPT);
        // Cut short: an error only once the input is over.
        let mut d = Decompressor::new(Format::Deflate);
        let mut out = Vec::new();
        d.write(&zlib[..zlib.len() / 2], &mut out, TEST_LIMIT, &never)
            .unwrap();
        assert!(d.finish().is_err());
        // Data after the end, in the same chunk or a later one.
        let mut extra = zlib.clone();
        extra.push(0);
        assert_eq!(
            inflate_all(Format::Deflate, &[&extra]).unwrap_err(),
            TRAILING
        );
        assert_eq!(
            inflate_all(Format::Deflate, &[&zlib, b"x"]).unwrap_err(),
            TRAILING
        );
        // gzip: a bad magic, a wrong CRC, a second member.
        assert!(inflate_all(Format::Gzip, &[b"\x1f\x8c\x08"]).is_err());
        let mut gz = compress_all(Format::Gzip, &text);
        let two = [gz.clone(), gz.clone()].concat();
        assert_eq!(inflate_all(Format::Gzip, &[&two]).unwrap_err(), TRAILING);
        let n = gz.len();
        gz[n - 6] ^= 1;
        assert_eq!(inflate_all(Format::Gzip, &[&gz]).unwrap_err(), CORRUPT);
    }

    /// A large input: 8 MB of JSON, as a large account's READY.
    #[test]
    fn a_large_input_inflates_whole() {
        let mut text = Vec::new();
        while text.len() < 8 << 20 {
            text.extend_from_slice(&json(2000));
        }
        let zlib = miniz_oxide::deflate::compress_to_vec_zlib(&text, 6);
        let started = std::time::Instant::now();
        let out = inflate_all(Format::Deflate, &[&zlib]).unwrap();
        eprintln!(
            "inflated {} MB from {} KB in {:.1} ms",
            text.len() >> 20,
            zlib.len() >> 10,
            started.elapsed().as_secs_f64() * 1e3
        );
        assert!(out == text);
    }

    #[test]
    fn deflate_bomb_respects_output_limit_and_exact_limit_succeeds() {
        let limit = 64 << 10;
        let bomb = vec![0u8; 4 << 20];
        let exact = vec![0x42u8; limit];
        let over = vec![0x42u8; limit + 1];
        for format in [Format::Deflate, Format::DeflateRaw, Format::Gzip] {
            let packed_bomb = compress_all(format, &bomb);
            assert!(packed_bomb.len() < 8 << 10, "high ratio bomb");
            let mut d = Decompressor::new(format);
            let mut out = Vec::new();
            assert_eq!(
                d.write(&packed_bomb, &mut out, limit, &never).unwrap_err(),
                TOO_LARGE,
                "{format:?}"
            );
            assert!(out.capacity() <= limit, "{format:?} exceeded limit");

            // Exact limit succeeds, one byte past limit fails.
            let packed_exact = compress_all(format, &exact);
            let mut d = Decompressor::new(format);
            let mut out = Vec::new();
            d.write(&packed_exact, &mut out, limit, &never).unwrap();
            d.finish().unwrap();
            assert_eq!(out, exact, "{format:?} exact boundary");

            let packed_over = compress_all(format, &over);
            let mut d = Decompressor::new(format);
            let mut out = Vec::new();
            assert_eq!(
                d.write(&packed_over, &mut out, limit, &never).unwrap_err(),
                TOO_LARGE,
                "{format:?} one byte over"
            );
            assert!(out.capacity() <= limit);
        }
    }

    #[test]
    fn compressor_respects_output_limit() {
        // Pseudo-random incompressible bytes.
        let mut x = 0x1234_5678_9abc_def0u64;
        let input: Vec<u8> = (0..(128 << 10))
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect();
        let limit = 32 << 10;
        for format in [Format::Deflate, Format::DeflateRaw, Format::Gzip] {
            let mut c = Compressor::new(format);
            let mut out = Vec::new();
            let err = c
                .write(&input, &mut out, limit, &never)
                .and_then(|()| c.finish(&mut out, limit, &never))
                .unwrap_err();
            assert_eq!(err, TOO_LARGE, "{format:?}");
            assert!(out.capacity() <= limit, "{format:?} exceeded limit");
        }
    }

    #[test]
    fn unterminated_gzip_header_is_bounded_without_quadratic_scan() {
        for flag in [8u8, 16u8] {
            let mut d = Decompressor::new(Format::Gzip);
            let mut out = Vec::new();
            let header = [0x1f, 0x8b, 8, flag, 0, 0, 0, 0, 0, 3];
            d.write(&header, &mut out, TEST_LIMIT, &never).unwrap();
            let chunk = [b'a'; 1024];
            let mut failed = None;
            for _ in 0..(MAX_GZIP_HEADER / chunk.len() + 4) {
                if let Err(e) = d.write(&chunk, &mut out, TEST_LIMIT, &never) {
                    failed = Some(e);
                    break;
                }
            }
            assert_eq!(
                failed.as_deref(),
                Some("the gzip header is too large"),
                "flag {flag}"
            );
        }
    }

    #[test]
    fn door_bounds_codecs_releases_peak_capacity_and_checks_interrupts() {
        let mut codecs = std::collections::HashMap::new();
        let mut next = 1u64;
        let mut out = Vec::new();
        let limits = Limits {
            max_output: 512 << 10,
            max_codecs: 4,
            max_codec_bytes: 4 * std::mem::size_of::<InflateState>(),
        };

        // Peak output capacity releases after copy or failure.
        let h = door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &never,
            12,
            "d deflate",
            &[],
        )
        .unwrap();
        let payload = vec![b'z'; 256 << 10];
        let packed = compress_all(Format::Deflate, &payload);
        let n: usize = door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &never,
            13,
            &format!("{h} 0 {}", packed.len()),
            &packed,
        )
        .unwrap()
        .parse()
        .unwrap();
        assert_eq!(n, payload.len());
        assert!(out.capacity() >= payload.len());
        release_output(&mut out);
        assert!(out.capacity() <= STEP);
        door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &never,
            14,
            &h,
            &[],
        )
        .unwrap();
        assert!(codecs.is_empty());

        // A deflate bomb is rejected, its codec dropped, and output capacity released.
        let h = door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &never,
            12,
            "d deflate",
            &[],
        )
        .unwrap();
        let bomb = compress_all(Format::Deflate, &vec![0u8; 2 << 20]);
        let err = door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &never,
            13,
            &format!("{h} 0 {}", bomb.len()),
            &bomb,
        )
        .unwrap_err();
        assert_eq!(err, TOO_LARGE);
        assert!(codecs.is_empty());
        assert!(out.capacity() <= STEP);

        // Interrupt stops inflation immediately.
        let h = door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &never,
            12,
            "d deflate",
            &[],
        )
        .unwrap();
        let err = door(
            &mut codecs,
            &mut next,
            &mut out,
            limits,
            &|| true,
            13,
            &format!("{h} 0 {}", packed.len()),
            &packed,
        )
        .unwrap_err();
        assert_eq!(err, INTERRUPTED);
        assert!(codecs.is_empty());

        // Concurrent codec cap enforces both count and native-byte budgets.
        for _ in 0..4 {
            door(
                &mut codecs,
                &mut next,
                &mut out,
                limits,
                &never,
                12,
                "d deflate",
                &[],
            )
            .unwrap();
        }
        assert_eq!(
            door(
                &mut codecs,
                &mut next,
                &mut out,
                limits,
                &never,
                12,
                "d deflate",
                &[]
            )
            .unwrap_err(),
            TOO_MANY_CODECS
        );
        // Byte cap refuses a larger CompressorOxide even when count has room.
        codecs.clear();
        let small_bytes = Limits {
            max_output: 512 << 10,
            max_codecs: 64,
            max_codec_bytes: std::mem::size_of::<CompressorOxide>(),
        };
        door(
            &mut codecs,
            &mut next,
            &mut out,
            small_bytes,
            &never,
            12,
            "c deflate",
            &[],
        )
        .unwrap();
        assert_eq!(
            door(
                &mut codecs,
                &mut next,
                &mut out,
                small_bytes,
                &never,
                12,
                "c deflate",
                &[]
            )
            .unwrap_err(),
            TOO_MANY_CODECS
        );
    }
}
