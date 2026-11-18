/** Read presentation size and duration with the browser's demuxer, including rotation and edit lists. */
export async function videoMetadata(bytes, type) {
  const blob = new Blob([bytes], { type }), url = URL.createObjectURL(blob);
  const video = document.createElement('video');
  video.preload = 'auto';
  video.muted = true;
  try {
    await new Promise((resolve, reject) => {
      video.onerror = () => reject(new Error('the recorded video did not decode'));
      video.onloadedmetadata = () => {
        if (Number.isFinite(video.duration)) resolve();
        // MediaRecorder's WebM often omits Duration. Seeking past the end makes the demuxer discover it.
        else { video.onseeked = resolve; video.currentTime = Number.MAX_SAFE_INTEGER; }
      };
      video.src = url;
    });
    if (!(video.duration > 0 && Number.isFinite(video.duration) && video.videoWidth && video.videoHeight)) {
      throw new Error('the recorded video has no finite duration or pixel size');
    }
    const frameRate = videoFrameRate(bytes, type);
    return { duration: video.duration * 1000, width: video.videoWidth, height: video.videoHeight,
      ...(frameRate > 0 && Number.isFinite(frameRate) ? { frameRate } : {}) };
  } finally {
    video.removeAttribute('src'); video.load(); URL.revokeObjectURL(url);
  }
}

/** Average encoded video rate. The browser exposes no file frame-rate API; count the video samples and
 * their media time in ISO BMFF or WebM, never the source track's requested or negotiated capture rate.
 * https://www.w3.org/TR/mse-byte-stream-format-isobmff/ and https://www.webmproject.org/docs/container/ */
export function videoFrameRate(bytes, type) {
  if (bytes.byteLength < 8) throw new Error('the recorded file is truncated');
  const view = new DataView(bytes);
  return type.split(';')[0] === 'video/mp4' ? mp4Rate(view) : webmRate(view);
}

function mp4Rate(v) {
  const u32 = p => v.getUint32(p), u64 = p => Number(v.getBigUint64(p));
  const text = p => String.fromCharCode(...new Uint8Array(v.buffer, p, 4));
  function boxes(start, end) {
    const found = [];
    for (let p = start; p + 8 <= end;) {
      let size = u32(p), header = 8;
      if (size === 1) { size = u64(p + 8); header = 16; }
      else if (!size) size = end - p;
      if (size < header || p + size > end) throw new Error('invalid MP4 box');
      found.push({ type: text(p + 4), p: p + header, end: p + size });
      p += size;
    }
    return found;
  }
  const children = box => box ? boxes(box.p, box.end) : [];
  const child = (box, type) => children(box).find(b => b.type === type);
  const root = { p: 0, end: v.byteLength }, moov = child(root, 'moov');
  const track = children(moov).filter(b => b.type === 'trak').find(t => {
    const handler = child(child(t, 'mdia'), 'hdlr');
    return handler && text(handler.p + 8) === 'vide';
  });
  if (!track) return;
  const tkhd = child(track, 'tkhd'), mdia = child(track, 'mdia'), mdhd = child(mdia, 'mdhd');
  const id = u32(tkhd.p + (v.getUint8(tkhd.p) ? 20 : 12));
  const scale = u32(mdhd.p + (v.getUint8(mdhd.p) ? 20 : 12));
  let samples = 0, ticks = 0;
  const stts = child(child(child(mdia, 'minf'), 'stbl'), 'stts');
  if (stts) for (let p = stts.p + 8; p + 8 <= stts.end; p += 8) {
    samples += u32(p); ticks += u32(p) * u32(p + 4);
  }
  const trex = children(child(moov, 'mvex')).find(b => b.type === 'trex' && u32(b.p + 4) === id);
  const defaultDuration = trex ? u32(trex.p + 12) : 0;
  for (const moof of children(root).filter(b => b.type === 'moof')) {
    for (const traf of children(moof).filter(b => b.type === 'traf')) {
      const tfhd = child(traf, 'tfhd');
      if (!tfhd || u32(tfhd.p + 4) !== id) continue;
      const flags = u32(tfhd.p) & 0xffffff;
      const at = tfhd.p + 8 + (flags & 1 ? 8 : 0) + (flags & 2 ? 4 : 0);
      const duration = flags & 8 ? u32(at) : defaultDuration;
      for (const run of children(traf).filter(b => b.type === 'trun')) {
        const flags = u32(run.p) & 0xffffff, count = u32(run.p + 4);
        let p = run.p + 8 + (flags & 1 ? 4 : 0) + (flags & 4 ? 4 : 0);
        const stride = [0x100, 0x200, 0x400, 0x800].filter(f => flags & f).length * 4;
        if (p + count * stride > run.end) throw new Error('invalid MP4 samples');
        samples += count;
        if (!(flags & 0x100)) ticks += count * duration;
        else for (let i = 0; i < count; i++, p += stride) ticks += u32(p);
      }
    }
  }
  return ticks > 0 ? samples * scale / ticks : undefined;
}

function webmRate(v) {
  function vint(p, id = false) {
    const first = v.getUint8(p);
    let n = 1, mask = 128;
    while (mask && !(first & mask)) { n++; mask >>= 1; }
    if (!mask || p + n > v.byteLength) throw new Error('invalid WebM integer');
    let value = id ? first : first & (mask - 1), unknown = (first & (mask - 1)) === mask - 1;
    for (let i = 1; i < n; i++) { const b = v.getUint8(p + i); value = value * 256 + b; unknown &&= b === 255; }
    return { value, n, unknown: !id && unknown };
  }
  function elements(start, end) {
    const found = [];
    for (let p = start; p < end;) {
      const id = vint(p, true), size = vint(p + id.n), data = p + id.n + size.n;
      const next = size.unknown ? end : data + size.value;
      if (next > end) throw new Error('invalid WebM element');
      found.push({ id: id.value, p: data, end: next });
      p = next;
    }
    return found;
  }
  const children = box => box ? elements(box.p, box.end) : [];
  const child = (box, id) => children(box).find(b => b.id === id);
  const uint = box => {
    let value = 0;
    if (box) for (let p = box.p; p < box.end; p++) value = value * 256 + v.getUint8(p);
    return value;
  };
  const segment = child({ p: 0, end: v.byteLength }, 0x18538067);
  const scale = uint(child(child(segment, 0x1549a966), 0x2ad7b1)) || 1000000;
  const track = children(child(segment, 0x1654ae6b)).find(t => t.id === 0xae && uint(child(t, 0x83)) === 1);
  if (!track) return;
  const id = uint(child(track, 0xd7)), defaultDuration = uint(child(track, 0x23e383));
  let count = 0, first = Infinity, last = -Infinity, finalDuration = 0, finalFrames = 0;
  for (const cluster of children(segment).filter(b => b.id === 0x1f43b675)) {
    // An unknown-sized Cluster extends to the next Cluster at the same level.
    let time = 0;
    const blocks = children(cluster);
    for (let i = 0; i < blocks.length; i++) {
      const b = blocks[i];
      if (b.id === 0x1f43b675) { blocks.splice(i, 1, ...children(b)); i--; continue; }
      if (b.id === 0xe7) { time = uint(b); continue; }
      const block = b.id === 0xa3 ? b : b.id === 0xa0 ? child(b, 0xa1) : null;
      if (!block) continue;
      const number = vint(block.p);
      if (number.value !== id) continue;
      const p = block.p + number.n, timestamp = (time + v.getInt16(p)) * scale;
      const frames = v.getUint8(p + 2) & 6 ? v.getUint8(p + 3) + 1 : 1;
      count += frames;
      first = Math.min(first, timestamp);
      if (timestamp >= last) {
        last = timestamp; finalFrames = frames;
        finalDuration = b.id === 0xa0 ? uint(child(b, 0x9b)) * scale : 0;
        finalDuration ||= defaultDuration * frames;
      }
    }
  }
  // SimpleBlock has no duration. Its timestamps still give the observed rate, even if DefaultDuration
  // retained the initial capture rate after applyConstraints changed it during the take.
  return count > 1 && last > first ? (count - finalFrames) * 1e9 / (last - first)
    : finalDuration > 0 ? count * 1e9 / finalDuration : undefined;
}
