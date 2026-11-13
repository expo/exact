// The request headers that choose a representation (Accept-Encoding, RFC 9110 §12.5.3;
// If-None-Match, §13.1.2) and the encoded variants `serve` keeps of a response body.
import { brotliCompress, constants as zlib, gzip } from 'node:zlib';
import { createHash } from 'node:crypto';

// Compressed representations for the production server (`serve.mjs` itself;
// the dev server and drivers send identity bytes). Brotli at quality 11 takes
// seconds for an app's wasm, so a variant is made once per body digest, off
// the request path — the server warms its tree at startup — and a request that
// arrives first gets the identity bytes. A variant no smaller is not kept.

export const COMPRESSIBLE = new Set(['application/wasm', 'text/javascript', 'text/css', 'text/html', 'application/json',
  'application/manifest+json', 'application/vnd.exact.envelope+json', 'image/svg+xml', 'text/wgsl',
  'application/vnd.exact.model', 'application/vnd.exact.texture']);
export function compressionCache(limit = 256 * 1024 * 1024) {
  const variants = new Map(), pending = new Map();
  let held = 0;
  const make = (digest, body, encoding) => {
    const key = `${digest}:${encoding}`;
    if (variants.has(key)) return Promise.resolve();
    if (!pending.has(key)) pending.set(key, new Promise(done => {
      const finish = (error, out) => {
        pending.delete(key);
        if (!error && out.length < body.length && held + out.length <= limit) { variants.set(key, out); held += out.length; }
        done();
      };
      if (encoding === 'br') brotliCompress(body, { params: { [zlib.BROTLI_PARAM_QUALITY]: 11, [zlib.BROTLI_PARAM_SIZE_HINT]: body.length } }, finish);
      else gzip(body, { level: 9 }, finish);
    }));
    return pending.get(key);
  };
  return {
    get: (digest, encoding) => variants.get(`${digest}:${encoding}`) ?? null,
    warm: (digest, body) => Promise.all(['br', 'gzip'].map(encoding => make(digest, body, encoding))),
  };
}
export const bodyDigest = body => createHash('sha256').update(body).digest('base64url').slice(0, 32);
/** The encodings a request accepts, per RFC 9110 §12.5.3 (a q of 0 refuses). */
export function acceptedEncodings(header = '') {
  const accepted = new Set();
  for (const part of String(header).split(',')) {
    const [name, ...params] = part.trim().toLowerCase().split(';');
    const q = params.map(p => /^\s*q=([0-9.]+)\s*$/.exec(p)?.[1]).find(Boolean);
    if (name && (q === undefined || Number(q) > 0)) accepted.add(name);
  }
  return accepted;
}
export const matchesETag = (header, etag) => typeof header === 'string'
  && header.split(',').some(tag => { const t = tag.trim(); return t === '*' || t.replace(/^W\//, '') === etag; });
