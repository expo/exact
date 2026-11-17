// MP4 sample tables supply presentation times; WebCodecs decodes each sample once.
// Playback only selects a cached image. It never runs a media element's wall clock.
export async function decodeClip(path) {
  const url = new URL(path, document.baseURI);
  if (url.origin !== location.origin || /^(?:\/|[a-z]+:)|(?:^|\/)\.\.(?:\/|$)/i.test(path))
    throw new Error('camera.clip must name an app asset');
  const response = await fetch(url);
  if (!response.ok) throw new Error(`camera.clip: ${response.status} loading ${path}`);
  const bytes = await response.arrayBuffer(), data = new DataView(bytes);
  const u32 = at => data.getUint32(at), u64 = at => Number(data.getBigUint64(at));
  const text = at => String.fromCharCode(...new Uint8Array(bytes, at, 4));
  function boxes(start, end) {
    const out = [];
    for (let at = start; at < end;) {
      if (at + 8 > end) throw new Error('camera.clip: truncated MP4 box');
      const size = u32(at), header = size === 1 ? 16 : 8, length = size === 1 ? u64(at + 8) : size || end - at;
      if (length < header || at + length > end) throw new Error('camera.clip: invalid MP4 box');
      out.push({ type: text(at + 4), start: at + header, end: at + length });
      at += length;
    }
    return out;
  }
  const children = box => boxes(box.start, box.end);
  const child = (box, name) => {
    const found = children(box).find(b => b.type === name);
    if (!found) throw new Error(`camera.clip: MP4 has no ${name}`);
    return found;
  };
  const moov = child({ start: 0, end: bytes.byteLength }, 'moov');
  const track = children(moov).filter(b => b.type === 'trak').find(b => {
    const h = child(child(b, 'mdia'), 'hdlr');
    return text(h.start + 8) === 'vide';
  });
  if (!track) throw new Error('camera.clip has no video track');
  const tkhd = child(track, 'tkhd'), matrix = tkhd.start + (data.getUint8(tkhd.start) ? 52 : 40);
  const fixed16 = offset => data.getInt32(matrix + offset) / 65536;
  const [a, b, c, d, x, y] = [0, 4, 12, 16, 24, 28].map(fixed16);
  if (data.getInt32(matrix + 8) || data.getInt32(matrix + 20) || data.getInt32(matrix + 32) !== 1 << 30)
    throw new Error('camera.clip: unsupported perspective display transform');
  const mdia = child(track, 'mdia'), mdhd = child(mdia, 'mdhd');
  const version = data.getUint8(mdhd.start), scale = u32(mdhd.start + (version ? 20 : 12));
  let duration = (version ? u64(mdhd.start + 24) : u32(mdhd.start + 16)) * 1000 / scale;
  let edits = [{ at: 0, media: 0, duration, rate: 1 }];
  const edts = children(track).find(b => b.type === 'edts');
  if (edts) {
    const elst = child(edts, 'elst'), wide = data.getUint8(elst.start) === 1;
    const mvhd = child(moov, 'mvhd'), movieScale = u32(mvhd.start + (data.getUint8(mvhd.start) ? 20 : 12));
    edits = []; duration = 0;
    // Edit durations use the movie timescale; media starts use the track's.
    for (let i = 0; i < u32(elst.start + 4); i++) {
      const pos = elst.start + 8 + i * (wide ? 20 : 12);
      const length = (wide ? u64(pos) : u32(pos)) * 1000 / movieScale;
      const media = wide ? Number(data.getBigInt64(pos + 8)) : data.getInt32(pos + 4);
      const rate = data.getInt32(pos + (wide ? 16 : 8)) / 65536;
      if (!Number.isFinite(length) || length < 0 || media < -1 || rate < 0)
        throw new Error('camera.clip: unsupported MP4 edit');
      edits.push({ at: duration, media: media === -1 ? null : media * 1000 / scale, duration: length, rate });
      duration += length;
    }
  }
  const stbl = child(child(mdia, 'minf'), 'stbl'), stsd = child(stbl, 'stsd');
  const sample = boxes(stsd.start + 8, stsd.end)[0];
  if (!sample || !['avc1', 'avc3'].includes(sample.type)) throw new Error('camera.clip needs an H.264 MP4 on the web');
  const avcc = boxes(sample.start + 78, sample.end).find(b => b.type === 'avcC');
  if (!avcc) throw new Error('camera.clip: MP4 has no AVC configuration');
  const description = bytes.slice(avcc.start, avcc.end);
  const codec = `avc1.${[1, 2, 3].map(i => new Uint8Array(description)[i].toString(16).padStart(2, '0')).join('')}`;
  const config = { codec, description };
  if (typeof VideoDecoder !== 'function' || !(await VideoDecoder.isConfigSupported(config)).supported)
    throw new Error(`camera.clip: this browser cannot decode ${codec}`);
  const table = name => children(stbl).find(b => b.type === name);
  const rows = (box, columns) => Array.from({ length: u32(box.start + 4) }, (_, i) =>
    Array.from({ length: columns }, (_, j) => u32(box.start + 8 + (i * columns + j) * 4)));
  const stsz = child(stbl, 'stsz'), fixed = u32(stsz.start + 4), count = u32(stsz.start + 8);
  const sizes = Array.from({ length: count }, (_, i) => fixed || u32(stsz.start + 12 + i * 4));
  const times = [], lengths = [];
  let time = 0;
  for (const [n, delta] of rows(child(stbl, 'stts'), 2)) for (let i = 0; i < n; i++) {
    times.push(time); lengths.push(delta); time += delta;
  }
  const ctts = table('ctts');
  if (ctts) {
    let index = 0;
    for (const [n, offset] of rows(ctts, 2)) for (let i = 0; i < n; i++)
      times[index++] += data.getUint8(ctts.start) ? offset | 0 : offset;
  }
  const offsets = table('stco') ?? child(stbl, 'co64'), wide = offsets.type === 'co64';
  const chunks = Array.from({ length: u32(offsets.start + 4) }, (_, i) =>
    (wide ? u64 : u32)(offsets.start + 8 + i * (wide ? 8 : 4)));
  const mapping = rows(child(stbl, 'stsc'), 3), sync = table('stss');
  const keys = sync ? new Set(rows(sync, 1).flat()) : null;
  if (!count || times.length !== count || !Number.isFinite(duration) || duration <= 0)
    throw new Error('camera.clip has no timed video frames');
  const decoded = [], pending = [];
  let failure, blank;
  const decoder = new VideoDecoder({ error: e => { failure = e; }, output(frame) {
    pending.push((async () => {
      try {
        const w = frame.displayWidth, h = frame.displayHeight;
        const xs = [x, a * w + x, c * h + x, a * w + c * h + x];
        const ys = [y, b * w + y, d * h + y, b * w + d * h + y];
        const left = Math.min(...xs), top = Math.min(...ys);
        const canvas = new OffscreenCanvas(Math.ceil(Math.max(...xs) - left), Math.ceil(Math.max(...ys) - top));
        const ctx = canvas.getContext('2d');
        ctx.setTransform(a, b, c, d, x - left, y - top);
        ctx.drawImage(frame, 0, 0, w, h);
        const blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: 0.92 });
        const bitmap = canvas.transferToImageBitmap();
        decoded.push({ at: frame.timestamp / 1000, end: (frame.timestamp + frame.duration) / 1000,
          bitmap, blob, url: URL.createObjectURL(blob) });
      } finally { frame.close(); }
    })());
  } });
  try {
    decoder.configure(config);
    let index = 0, row = 0;
    for (let chunk = 0; chunk < chunks.length; chunk++) {
      while (row + 1 < mapping.length && mapping[row + 1][0] <= chunk + 1) row++;
      let offset = chunks[chunk];
      for (let i = 0; i < mapping[row][1]; i++, index++) {
        decoder.decode(new EncodedVideoChunk({ type: !keys || keys.has(index + 1) ? 'key' : 'delta',
          timestamp: Math.round(times[index] * 1e6 / scale), duration: Math.round(lengths[index] * 1e6 / scale),
          data: new Uint8Array(bytes, offset, sizes[index]) }));
        offset += sizes[index];
      }
    }
    await decoder.flush();
    await Promise.all(pending);
    if (failure) throw failure;
    if (decoded.length !== count) throw new Error('camera.clip: the decoder omitted a frame');
    decoded.sort((a, b) => a.at - b.at);
    const frames = [];
    for (const edit of edits) {
      if (!edit.duration) continue;
      if (edit.media === null) {
        if (!blank) {
          const canvas = new OffscreenCanvas(decoded[0].bitmap.width, decoded[0].bitmap.height);
          canvas.getContext('2d').fillRect(0, 0, canvas.width, canvas.height);
          const blob = await canvas.convertToBlob({ type: 'image/jpeg', quality: 0.92 });
          blank = { bitmap: canvas.transferToImageBitmap(), blob, url: URL.createObjectURL(blob) };
        }
        frames.push({ ...blank, at: edit.at });
        continue;
      }
      for (const f of decoded) {
        if (f.end <= edit.media || (edit.rate !== 0 && f.at >= edit.media + edit.duration * edit.rate)) continue;
        if (edit.rate === 0 && f.at > edit.media) continue;
        frames.push({ ...f, at: edit.at + (edit.rate ? Math.max(0, f.at - edit.media) / edit.rate : 0) });
      }
    }
    if (!frames.length) throw new Error('camera.clip has no frames in its edits');
    frames.sort((a, b) => a.at - b.at);
    const sha256 = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(b => b.toString(16).padStart(2, '0')).join('');
    const used = new Set(frames.map(f => f.bitmap));
    close(decoded.filter(f => !used.has(f.bitmap)));
    const kept = decoded.filter(f => used.has(f.bitmap));
    if (blank) kept.push(blank);
    return { frames, duration, sha256, close: () => close(kept) };
  } catch (e) { await Promise.allSettled(pending); close(decoded); if (blank) close([blank]); throw e; }
  finally { if (decoder.state !== 'closed') decoder.close(); }
}

function close(frames) {
  for (const f of frames) { f.bitmap.close(); URL.revokeObjectURL(f.url); }
}

export function clipFrame(clip, elapsed) {
  const at = Math.max(0, elapsed) % clip.duration;
  let lo = 0, hi = clip.frames.length;
  while (lo + 1 < hi) {
    const mid = (lo + hi) >>> 1;
    if (clip.frames[mid].at <= at) lo = mid; else hi = mid;
  }
  return clip.frames[lo];
}
