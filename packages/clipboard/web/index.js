// @exact/clipboard on the web (LLP 1133.006): `navigator.clipboard.write`
// with a `ClipboardItem` (https://www.w3.org/TR/clipboard-apis/).
//
// - The image is sanitized as Apple's is (D1): decoded, frame 0 of an
//   animation chosen with `ImageDecoder` where the browser has it, and drawn
//   to a canvas whose PNG is what the browser is given, so both hosts hold
//   the same pixels to the same bounds before the browser sanitizes again.
// - When a write may happen is the browser's (D3): Chrome writes while the
//   document has focus; Safari and Firefox within a person's activation. A
//   refusal is the browser's `NotAllowedError`.
// - Under the agent it is its own substitute (D8 item 4, §7): the item is
//   kept in memory, never given to the browser.
// - A later write supersedes one still reading its file or decoding, which
//   fails `AbortError` (D2), and one waiting its turn at the browser: the
//   browser is given one write at a time, in order, since the clipboard
//   holds whichever of two overlapping writes settles last.

const MOST_FILE = 64 * 1024 * 1024, MOST_BYTES = 2 * 1024 * 1024, MOST_PIXELS = 64_000_000, MOST_TEXT = 1024 * 1024;
const READ_TYPES = ['text/plain', 'image/png'];
const WRITE_TYPES = [...READ_TYPES, 'text/html'];
const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/** `{base64}` as bytes. */
const unwired = b64 => Uint8Array.from(atob(b64), c => c.charCodeAt(0));

/** A PNG's size from its `IHDR`, or null for what is not a PNG: a side past the specification's 2³¹ − 1, as on Apple. */
export function pngSize(bytes) {
  if (bytes.length < 24 || SIGNATURE.some((b, i) => bytes[i] !== b)) return null;
  if (String.fromCharCode(...bytes.subarray(12, 16)) !== 'IHDR') return null;
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength), width = v.getUint32(16), height = v.getUint32(20);
  return width <= 0x7fffffff && height <= 0x7fffffff ? { width, height } : null;
}

/** Frame 0 of `bytes` drawn: the first frame of its animation, as Chromium's clipboard writer decodes it. */
async function firstFrame(bytes) {
  if (typeof globalThis.ImageDecoder === 'function') {
    const decoder = new ImageDecoder({ data: bytes, type: 'image/png' });
    try { return (await decoder.decode({ frameIndex: 0 })).image; } finally { decoder.close(); }
  }
  return createImageBitmap(new Blob([bytes], { type: 'image/png' }));
}

const formatting = new Set("a b blockquote br caption code dd del div dl dt em h1 h2 h3 h4 h5 h6 hr i ins kbd li mark ol p pre s small span strong sub sup table tbody td th thead tfoot tr u ul".split(" "));
const containers = new Set("html body main section article header footer".split(" "));

/** Parse a document so a hidden html or body element keeps its children hidden. */
function sanitizedHTML(html) {
  const parsed = Document.parseHTML ? Document.parseHTML(html, { sanitizer: {} }) : new DOMParser().parseFromString(html, 'text/html');
  const clean = document.createElement('template');
  function copy(node, parent) {
    if (node.nodeType === Node.TEXT_NODE) { parent.append(node.data); return; }
    if (node.nodeType !== Node.ELEMENT_NODE || node.namespaceURI !== 'http://www.w3.org/1999/xhtml' || node.hasAttribute('hidden')) return;
    const tag = node.localName;
    if (formatting.has(tag)) {
      const element = clean.content.ownerDocument.createElement(tag);
      const href = tag === 'a' && node.getAttribute('href');
      if (href && /^(?:https?:\/\/|mailto:)/i.test(href)) {
        try {
          const url = new URL(href);
          if (url.protocol === 'mailto:' || url.hostname) element.setAttribute('href', href);
        } catch { /* Keep the text of a malformed link. */ }
      }
      parent.append(element);
      parent = element;
    } else if (!containers.has(tag)) return;
    for (const child of node.childNodes) copy(child, parent);
  }
  if (parsed.documentElement) copy(parsed.documentElement, clean.content);
  return clean.innerHTML;
}

const hex = buffer => [...new Uint8Array(buffer)].map(b => b.toString(16).padStart(2, '0')).join('');

export function implementation({ agent, log, files, clipboard = { item: null } }) {
  const say = line => log?.(`clipboard: ${line}`);
  let latest = 0, inFlight = null, writes = 0, last = null, shownNumber = 0, browser = Promise.resolve();
  const refused = (error, message) => { say(`write refused: ${error} (${message})`); return { error, message }; };

  /** A new PNG of `bytes`' pixels, or a refusal. */
  async function sanitized(bytes) {
    const size = pngSize(bytes);
    if (!size) return refused('DataError', 'not a PNG');
    if (size.width * size.height > MOST_PIXELS) return refused('DataError', `too large: ${size.width}×${size.height} is over 64,000,000 pixels`);
    let frame;
    try { frame = await firstFrame(bytes); } catch { return refused('DataError', 'not a PNG: it does not decode'); }
    try {
      const width = frame.displayWidth ?? frame.width, height = frame.displayHeight ?? frame.height;
      const canvas = new OffscreenCanvas(width, height);
      canvas.getContext('2d').drawImage(frame, 0, 0);
      const blob = await canvas.convertToBlob({ type: 'image/png' });
      const png = new Uint8Array(await blob.arrayBuffer());
      return { blob, png, width, height };
    } finally { frame.close?.(); }
  }

  async function write(items, told = {}) {
    // Chrome resolves an empty write and leaves the clipboard as it was.
    if (!Array.isArray(items)) return refused('TypeError', 'write takes a list of ClipboardItem');
    if (!items.length) return null;
    if (items.length > 1) return refused('NotAllowedError', 'Support for multiple ClipboardItems is not implemented.');
    const record = items[0];
    if (!record || typeof record !== 'object' || !Object.keys(record).length) return refused('TypeError', 'a ClipboardItem has at least one type');
    const other = Object.keys(record).sort().find(t => !WRITE_TYPES.includes(t));
    if (other) return refused('NotAllowedError', `Type ${other} not supported on write.`);
    const text = record['text/plain'];
    if (text != null && typeof text !== 'string') return refused('TypeError', 'text/plain is a string');
    if (text != null && new TextEncoder().encode(text).length > MOST_TEXT) return refused('DataError', 'text/plain is over 1 MiB');
    let html = record['text/html'];
    if ('text/html' in record) {
      if (typeof html !== 'string') return refused('TypeError', 'text/html is a string');
      const encoder = new TextEncoder();
      if (encoder.encode(html).length > MOST_TEXT) return refused('DataError', 'text/html is over 1 MiB');
      try { html = sanitizedHTML(html); } catch { return refused('NotAllowedError', 'could not sanitize text/html'); }
      if (encoder.encode(html).length > MOST_TEXT) return refused('DataError', 'sanitized text/html is over 1 MiB');
    }
    let source = null;
    const image = record['image/png'];
    if (image && typeof image.base64 === 'string') {
      source = unwired(image.base64);
      if (source.length > MOST_BYTES) return refused('DataError', 'image/png is over 2 MiB as bytes: pass an app:/ path');
    } else if (image != null && typeof image !== 'string') return refused('TypeError', 'image/png is an app:/ path or bytes');

    // This write supersedes any still reading its file or decoding (D2), and
    // is superseded from here, before its own first wait.
    const number = ++latest;
    const beforeCopy = clipboard.item;
    inFlight?.supersede();
    let superseded = null;
    const mine = { request: told.request, supersede: () => { superseded ??= refused('AbortError', 'a later write superseded it'); } };
    inFlight = mine;
    const ended = () => superseded || number !== latest || (agent && clipboard.item !== beforeCopy) ? superseded ?? refused('AbortError', 'a later write superseded it') : null;
    let png = null;
    try {
      if (typeof image === 'string') {
        let failure = null;
        try { source = new Uint8Array(await files.read(image)); } catch (e) { failure = e; }
        const stop = ended();
        if (stop) return stop;
        if (failure) return refused(/ENOENT|not found|no such/i.test(failure?.message ?? '') ? 'NotFoundError' : 'NotReadableError', `${image}: ${failure?.message ?? failure}`);
        if (source.length > MOST_FILE) return refused('DataError', `too large: ${image} is over 64 MiB`);
      }
      png = source ? await sanitized(source) : null;
      const stop = ended();
      if (stop) return stop;
      if (png?.error) return png;
      if (!agent) {
        const data = {};
        if (png) data['image/png'] = png.blob;
        if (html != null) data['text/html'] = new Blob([html], { type: 'text/html' });
        if (text != null) data['text/plain'] = new Blob([text], { type: 'text/plain' });
        // After the browser write before it settles; a write superseded while it waits is never given to the browser.
        const before = browser;
        let settled;
        browser = new Promise(ok => { settled = ok; });
        try {
          await before;
          const stop = ended();
          if (stop) return stop;
          await navigator.clipboard.write([new ClipboardItem(data)]);
        } catch (e) { return refused(e?.name ?? 'NotAllowedError', e?.message ?? String(e)); } finally { settled(); }
      }
    } finally { if (inFlight === mine) inFlight = null; }
    if (agent) clipboard.item = { ...(html != null && { 'text/html': html }), ...(text != null && { 'text/plain': text }), ...(png && { 'image/png': png.blob }) };
    writes++;
    const shown = { types: [...(text != null ? ['text/plain'] : []), ...(html != null ? ['text/html'] : []), ...(png ? ['image/png'] : [])], pasteboard: agent ? 'memory' : 'navigator.clipboard' };
    if (text != null) shown.text = text.slice(0, 200);
    if (png) {
      shown.image = { width: png.width, height: png.height, bytes: png.png.length, sha256: hex(await crypto.subtle.digest('SHA-256', png.png)) };
      say(`wrote image/png ${png.width}×${png.height} (${png.png.length.toLocaleString('en-US')} bytes)${shown.types.filter(t => t !== 'image/png').map(t => ' and ' + t).join('')}`);
    } else if (html == null) say(`wrote text/plain (${new TextEncoder().encode(text).length} bytes)`);
    else say(`wrote ${shown.types.join(' and ')}`);
    // What `state()` shows is the newest write's, though an older one's hash, awaited after its write, lands later.
    if (number > shownNumber) { shownNumber = number; last = shown; }
    return null;
  }
  const reads = new Map();
  async function read(textOnly, request) {
    const token = {};
    reads.set(request, token);
    const cancelled = () => reads.get(request) !== token ? new DOMException('clipboard read cancelled', 'AbortError') : undefined;
    try {
      if (textOnly) {
        if (!agent && !navigator.clipboard?.readText) return { error: 'NotSupportedError' };
        const text = agent ? clipboard.item?.['text/plain'] ?? '' : await navigator.clipboard.readText();
        if (cancelled()) return { error: 'AbortError' };
        return text.length <= MOST_TEXT && new TextEncoder().encode(text).length <= MOST_TEXT ? text : { error: 'DataError', message: 'text/plain is over 1 MiB' };
      }
      if (!agent && !navigator.clipboard?.read) return { error: 'NotSupportedError' };
      const own = clipboard.item;
      const items = agent ? (own ? [{ types: Object.keys(own), getType: type => own[type] }] : []) : await navigator.clipboard.read();
      const records = [], copies = [];
      let textBytes = 0, imageBytes = 0;
      for (const item of items) {
        if (cancelled()) return { error: 'AbortError' };
        const record = {};
        for (const type of item.types.filter(type => READ_TYPES.includes(type))) {
          const data = await item.getType(type);
          if (type === 'text/plain') {
            const tooLarge = { error: 'DataError', message: 'text/plain is over 1 MiB' };
            const remaining = MOST_TEXT - textBytes;
            if ((agent ? data.length : data.size) > remaining) return tooLarge;
            const text = agent ? data : new TextDecoder("utf-8", { ignoreBOM: true }).decode(await data.arrayBuffer());
            textBytes += new TextEncoder().encode(text).length;
            if (textBytes > MOST_TEXT) return tooLarge;
            record[type] = text;
          } else {
            const blob = data;
            const tooLarge = { error: 'DataError', message: 'image/png is over 64 MiB' };
            if (blob.size > MOST_FILE) return tooLarge;
            const image = await sanitized(new Uint8Array(await blob.arrayBuffer()));
            if (image.error) return image;
            imageBytes += image.png.length;
            if (imageBytes > MOST_FILE) return tooLarge;
            const path = `app:/tmp/clipboard-${crypto.randomUUID()}.png`;
            copies.push({ path, blob: image.blob });
            record[type] = path;
          }
        }
        if (Object.keys(record).length) records.push(record);
      }
      if (cancelled()) return { error: 'AbortError' };
      if (copies.length) {
        await files.putAll(copies, cancelled);
        if (cancelled()) {
          await Promise.all(copies.map(({ path }) => files.remove(path)));
          return { error: 'AbortError' };
        }
      }
      return records;
    } catch (error) { return { error: error.name ?? 'NotReadableError', message: error.message }; }
    finally { if (reads.get(request) === token) reads.delete(request); }
  }

  const disclosed = () => ({ writes, ...(last ? { lastWrite: last } : {}) });

  return {
    read: (_, { request = Symbol() } = {}) => read(false, request),
    readText: (_, { request = Symbol() } = {}) => read(true, request),
    write: ({ data }, told) => write(data, told),
    writeText: ({ data }, told) => typeof data === 'string' ? write([{ 'text/plain': data }], told) : refused('TypeError', 'writeText takes a string'),
    cancel(request) { reads.delete(request); if (inFlight && inFlight.request === request) { latest++; inFlight.supersede(); } },
    destroy() { reads.clear(); inFlight?.supersede(); },
    /** `state.extensions.clipboard.implementation` (§7 item 3). */
    state: disclosed,
  };
}
