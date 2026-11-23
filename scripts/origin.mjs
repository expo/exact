// The origin adapter `exact deploy` publishes through (LLP 1030.000 D3 item
// 5, D7; LLP 1030 D3a): the static host that serves the web app at its root,
// content-addressed blobs under `.exact/blobs/<sha256>`, and one directory
// per stream — `.exact/<channel>/<compatibilityId>/` — holding the signed
// head `exact.json` and `releases/<release>.json` (the immutable prepared
// record of each publish). A client resolves the head's relative `url`s to
// the immutable blob tree; no mutable payload copy lives under a stream.
//
// Four adapters. An **AppDrop** origin (`https://appdrop.app/u/<key id>`,
// which app.json spells `"appdrop"`) is the one that needs no account and no
// server: every PUT is signed with the app's update key, and AppDrop keeps
// blobs content-addressed, release records write-once and each head's seq
// rising, with `If-Match` on the ETag as the conditional put. A **directory** is v1's writable origin: any directory a
// supported serve.mjs origin handler serves. Every file lands whole or absent (a temp file beside
// it, then a rename), blobs are immutable and skipped when present by
// digest, and the head is a **conditional put**: one writer per stream at a
// time through an OS lock on a permanent `.lock` inode (never stolen on
// elapsed time or a host's interpretation of another host's pid), and the current head's digest compared with the one the publisher
// read before the new head is renamed into place. An **https** origin is
// read-only here — enough for the classifier to read live heads and root
// files; `--yes` against https refuses by name. An **object store**
// (`s3+https://<endpoint>/<bucket>`, S3's API signed with SigV4) is where
// `tools.mjs publish` writes tool sets (LLP 1174 D4.2); its readers fetch the
// bucket's public https URL as an https origin. Deploy does not take it yet.
import { createHash, createHmac } from 'node:crypto';
import { extname, resolve } from 'node:path';
import { within } from './paths.mjs';
import { canonical, matchPattern } from '../host/web-js/route-pattern.js';
import { filesystem, filesystemLock } from './filesystem.mjs';

/** SHA-256, lowercase hex — the digest every card and blob name carries (LLP 1023 D2). */
export const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

/** A transport or filesystem availability failure. Classifiers turn only
 * this typed failure into an `unavailable` row; malformed heads and broken
 * invariants still fail closed. */
export class OriginUnavailable extends Error {}

/** A publisher never replays a signed write through a redirect. The timeout covers
 * headers and the streamed body; every chunk is checked before retaining it. */
async function publisherFetch(url, init = {}, cap = 64 << 10, timeout = 30_000) {
  let response;
  try { response = await fetch(url, { ...init, redirect: 'error', signal: AbortSignal.timeout(timeout) }); }
  catch (error) { throw new OriginUnavailable(`${url}: ${error.cause?.code ?? error.message}`); }
  const reader = response.body?.getReader();
  const chunks = [];
  let length = 0;
  try {
    if (Number(response.headers.get('content-length')) > cap) throw new Error(`${url}: response exceeds ${cap} bytes`);
    while (reader) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value.byteLength > cap - length) throw new Error(`${url}: response exceeds ${cap} bytes`);
      length += value.byteLength;
      chunks.push(value);
    }
    return { response, bytes: Buffer.concat(chunks, length) };
  } finally { await reader?.cancel().catch(() => {}); reader?.releaseLock(); }
}

export function validateChannel(channel) {
  if (typeof channel !== 'string' || !/^[A-Za-z0-9._-]+$/.test(channel)
    || ['.', '..', 'blobs', 'root'].includes(channel.toLowerCase())) {
    throw new Error(`the channel ${JSON.stringify(channel)} cannot name a directory under .exact/`);
  }
  return channel;
}

/** The stream's directory on the origin, relative to its root (LLP 1030.000 D7). */
export const streamPath = ({ channel, compatibilityId }) => `.exact/${channel}/${compatibilityId}`;

/** The blob's path on the origin: content-addressed, immutable. */
export const blobPath = (digest) => `.exact/blobs/${digest}`;

// The readback limit also applies to payload files on native update hosts.
export const MAX_FILE_BYTES = 64 * 1024 * 1024;

// The web release has one complete public inventory and one guarded pointer.
// It uses the same OS lock and conditional write as native stream heads.
export const webRootStream = { channel: 'root', compatibilityId: 'web' };
export const webRootPath = `${streamPath(webRootStream)}/exact.json`;
export const webReleasePath = (id) => `.exact/root/web/releases/${id}`;
// @ref LLP 1038 D7; LLP 1023 §7 — the directory origin, dev and static
// server share this document fallback. Try a public file first; absent
// extensionless paths are locations. Reserved/private paths stay files.
// Inspect the raw request target before WHATWG URL removes dot segments.
// Used by static hosting, the dev server and the document fallback.
export function webRequestURL(target) {
  if (typeof target !== 'string' || target.includes('\\') || /%00/i.test(target)) return null;
  try {
    const path = target.split(/[?#]/, 1)[0];
    if (decodeURIComponent(path).includes('\\') || path.includes('\0')) return null;
    for (const segment of path.replace(/%2f/ig, '/').split('/')) {
      const decoded = decodeURIComponent(segment);
      if (decoded === '..' || decoded === '.' && segment !== '.') return null;
    }
    return new URL(target, 'http://exact.invalid');
  } catch { return null; }
}

/** Extensions a web build's files have (serve.mjs `webContentType` and the
 * game's `.hbc`, `.model`, `.tex`): a missing one stays a 404, a page or an
 * iframe's document (`/deck/missing.html`) included, unless a declared
 * route matches the path on a navigation. */
const FILE_EXTENSIONS = new Set(['.html', '.htm', '.css', '.js', '.mjs', '.json', '.wasm', '.plan', '.map', '.txt', '.xml', '.pdf', '.zip',
  '.png', '.jpg', '.jpeg', '.gif', '.webp', '.avif', '.svg', '.ico', '.mp4', '.webm', '.vtt', '.wav', '.mp3', '.m4a', '.aac', '.ogg', '.oga',
  '.ttf', '.otf', '.woff', '.woff2', '.wgsl', '.hbc', '.model', '.tex']);

/** Whether a request's `Accept` names text/markdown at a quality above zero and no lower than text/html's,
 * which is that of the most specific range matching it (RFC 9110 §12.5.1): text/html, then text/*, then *\/*. */
export function prefersMarkdown(accept = '') {
  const ranges = acceptRanges(accept);
  const quality = (...types) => ranges.find(([type]) => type === types.find((t) => ranges.some(([r]) => r === t)))?.[1] ?? 0;
  const markdown = quality('text/markdown');
  return markdown > 0 && markdown >= quality('text/html', 'text/*', '*/*');
}

// Media ranges and numeric qualities shared by both negotiation paths.
function acceptRanges(accept) {
  // Split on commas and semicolons outside a quoted parameter value.
  const ranges = [[[]]];
  let quoted = false;
  for (let i = 0, s = String(accept ?? ''); i < s.length; i++) {
    const c = s[i], range = ranges.at(-1), param = range.at(-1);
    if (quoted && c === '\\') { param.push(c, s[++i] ?? ''); continue; }
    if (c === '"') quoted = !quoted;
    if (!quoted && c === ',') { ranges.push([[]]); continue; }
    if (!quoted && c === ';') { range.push([]); continue; }
    param.push(c);
  }
  return ranges.map(([type, ...params]) => {
    const q = params.map((p) => p.join('').trim().toLowerCase()).find((p) => p.startsWith('q='));
    return [type.join('').trim().toLowerCase(), q === undefined ? 1 : Math.max(0, Math.min(1, Number(q.slice(2)) || 0))];
  });
}

/** Whether a request's `Accept` takes HTML (RFC 9110: the media type, any
 * case, at a quality above zero), as a browser's navigation does. With
 * `wildcard`, a range taking any type or any text type, or no header at
 * all, does too (curl's request) unless it names `text/html` at quality zero. */
export function acceptsHTML(accept = '', { wildcard = false } = {}) {
  const types = acceptRanges(accept).map(([type, q]) => [type, q > 0]);
  const html = types.filter(([type]) => type === 'text/html');
  if (html.length || !wildcard) return html.some(([, taken]) => taken);
  return !String(accept ?? '').trim() || types.some(([type, taken]) => taken && (type === '*/*' || type === 'text/*'));
}
/** Whether a path that names no file is the app's document. A navigation
 * (`acceptsHTML`) to one whose last segment has a dot is too, unless that
 * is a file's extension and no declared route matches it. */
export function appDocumentPath(pathname, navigation = false, routes = []) {
  if (!webRequestURL(pathname)) return false;
  let path;
  try { path = decodeURIComponent(pathname); } catch { return false; }
  const extension = extname(path.replace(/\/+$/, '')).toLowerCase();
  return path.startsWith('/') && !path.includes('\\') && !path.includes('\0')
    && !path.split('/').some(part => part.startsWith('.'))
    && path !== '/__dev' && !path.startsWith('/__dev/')
    && (extension === '' || (navigation && (!FILE_EXTENSIONS.has(extension) || matchPattern(routes, canonical(pathname)) >= 0)));
}
export function parseWebRoot(bytes) {
  const root = JSON.parse(bytes.toString('utf8'));
  if (root.webRoot !== 1 || !/^[0-9a-f]{64}$/.test(root.id ?? '') || !Array.isArray(root.files)) throw new Error('invalid web root pointer');
  const names = new Set();
  for (const file of root.files) {
    safeRelative(file.name);
    if (names.has(file.name) || !/^[0-9a-f]{64}$/.test(file.sha256 ?? '')
      || !/^[0-9a-f]{64}$/.test(file.sourceSha256 ?? '') || !Number.isSafeInteger(file.bytes) || file.bytes < 0) throw new Error('invalid web root inventory');
    names.add(file.name);
  }
  return root;
}

/** A relative origin path is segments with no `..`, no empty segment, no backslash, no drive. */
export function safeRelative(rel) {
  if (typeof rel !== 'string' || rel === '' || rel.startsWith('/') || rel.includes('\\') || rel.includes('\0')) throw new Error(`not a relative origin path: ${JSON.stringify(rel)}`);
  for (const segment of rel.split('/')) if (segment === '' || segment === '.' || segment === '..') throw new Error(`not a relative origin path: ${JSON.stringify(rel)}`);
  return rel;
}

/** The origin `spec` names: an AppDrop origin, a directory (created on first write) or an https URL (read-only). */
export function openOrigin(spec) {
  if (OBJECT_STORE.test(spec)) throw new Error('an s3+ origin is an object store, which holds tool sets (scripts/tools.mjs); deploy publishes to AppDrop or a directory');
  if (APPDROP_PATH.test(spec)) return new AppDropOrigin(spec);
  if (/^https?:\/\//.test(spec)) return new HttpsOrigin(spec);
  return new DirectoryOrigin(spec);
}

const APPDROP_PATH = /^https?:\/\/[^/]+\/u\/[0-9a-f]{32}\/?$/;

/** AppDrop's key id for a raw Ed25519 public key: the first 32 hex digits of its sha256. */
export const appdropKeyId = (rawPublicKey) => sha256(rawPublicKey).slice(0, 32);

/** AppDrop's origin for the app's update key (`deploy.signing.key`, or the
 * only key): `<base>/u/<key id>`, the base `https://appdrop.app` unless
 * `EXACT_APPDROP_ORIGIN` names another. Matches `Manifest::appdrop_origin`
 * in contract/cli, which bakes the same URL into the binary. */
export function appdropOrigin(manifest) {
  const keys = manifest.deploy?.signing?.keys ?? {};
  const id = manifest.deploy?.signing?.key ?? (Object.keys(keys).length === 1 ? Object.keys(keys)[0] : null);
  if (!id || !keys[id]) throw new Error('the origin "appdrop" is named after the update key: set deploy.signing.key and deploy.signing.keys');
  const base = (process.env.EXACT_APPDROP_ORIGIN ?? 'https://appdrop.app').replace(/\/+$/, '');
  return `${base}/u/${appdropKeyId(Buffer.from(keys[id], 'base64'))}`;
}

/** The origin spec a channel names in app.json (`deploy.channels.<channel>`, else `app.origin`), with `"appdrop"` resolved. */
export function channelOrigin(manifest, channel) {
  const spec = manifest.deploy?.channels?.[channel] ?? manifest.app?.origin ?? null;
  return spec === 'appdrop' ? appdropOrigin(manifest) : spec;
}

/** The channel the manifest bakes in: `deploy.channel`, else the only key of `deploy.channels`, else `prod` — the same rule as `Manifest::channel` in contract/cli. */
export function channelOf(manifest) {
  const deploy = manifest.deploy ?? {};
  if (deploy.channel != null) return deploy.channel;
  const names = Object.keys(deploy.channels ?? {});
  return names.length === 1 ? names[0] : 'prod';
}

/** A directory served by host/web/serve.mjs --origin: the one writable origin in v1, one publisher per stream at a time. */
export class DirectoryOrigin {
  constructor(dir) {
    this.dir = resolve(dir);
    this.kind = 'directory';
    this.writable = true;
  }

  /** How the table names it. */
  describe() { return this.dir; }

  /** The absolute path of `rel`, refused when it would leave the origin. */
  path(rel) {
    const abs = resolve(this.dir, safeRelative(rel));
    if (!within(this.dir, abs)) throw new Error(`${rel} leaves the origin`);
    return abs;
  }

  async operation(input) {
    try { return this.locked ? await this.locked(input) : filesystem({ root: this.dir, ...input }); }
    catch (error) {
      if (['EACCES', 'EPERM', 'EIO', 'EMFILE', 'ENFILE'].includes(error.code)) throw new OriginUnavailable(`${this.dir}: ${error.code}`);
      throw error;
    }
  }

  /** Read through owned directory handles; only a genuinely absent file is null. */
  async get(rel) {
    const value = await this.operation({ op: 'get', path: safeRelative(rel) });
    return value === null ? null : Buffer.from(value, 'base64');
  }

  async list(rel, { includeHidden = false } = {}) {
    const value = await this.operation({ op: 'list', path: safeRelative(rel) });
    return value?.filter((name) => includeHidden || !name.startsWith('.')) ?? null;
  }

  /** A complete sibling-temp rename, or an exclusive immutable link. */
  async put(rel, bytes, { immutable = false } = {}) {
    const parts = safeRelative(rel).toLowerCase().split('/');
    if (parts.length === 4 && parts[0] === '.exact' && ['exact.json', '.lock'].includes(parts[3])) throw new Error('stream heads and locks require the held stream lock');
    return this.operation({ op: 'put', path: rel, bytes: Buffer.from(bytes).toString('base64'), immutable });
  }

  /** The stream's current head: `{bytes, sha256, json}`, or null when the stream has none. A head that is not JSON is an error naming the path — a corrupt head is not a missing one. */
  async head(stream) {
    const rel = `${streamPath(stream)}/exact.json`;
    const bytes = await this.get(rel);
    if (!bytes) return null;
    let json;
    try { json = JSON.parse(bytes.toString('utf8')); } catch (e) { throw new Error(`the head at ${this.path(rel)} is not JSON: ${e.message}`); }
    return { bytes, sha256: sha256(bytes), json };
  }

  /** Hold an OS lock on a permanent, no-follow inode. Age, PID and host
   * identity never authorize stealing it; closing the helper releases it. */
  async withLock(stream, fn) {
    if (this.lockPending) throw new Error('this origin already holds or is acquiring a stream lock');
    const path = safeRelative(`${streamPath(stream)}/.lock`);
    this.lockPending = true;
    try {
      return await filesystemLock(this.dir, path, async (send) => {
        this.locked = send;
        try { return await fn(); } finally { this.locked = null; }
      });
    } finally { this.lockPending = false; }
  }

  /** The digest check and head rename run inside the same lock-owning
   * helper. A standalone call acquires that stream lock too. */
  async putHead(stream, bytes, { previousDigest = null } = {}) {
    if (!this.locked) return this.withLock(stream, () => this.putHead(stream, bytes, { previousDigest }));
    return this.operation({ op: 'head', path: safeRelative(`${streamPath(stream)}/exact.json`),
      bytes: Buffer.from(bytes).toString('base64'), previousDigest });
  }

  /** The file and a version that `replace` compares: its digest. */
  async getVersion(rel) {
    const bytes = await this.get(rel);
    return { bytes, version: bytes && sha256(bytes) };
  }

  /** Write `rel` only if it is still at `version` (absent, when null). The
   * stream lock held through `withLock` is what makes the read and write one step. */
  async replace(rel, bytes, version) {
    if (!this.locked) throw new Error(`replacing ${rel} requires the held stream lock`);
    if ((await this.getVersion(rel)).version !== version) return false;
    await this.put(rel, bytes);
    return true;
  }

  /** Every regular file under `rel`, with its modification time; a symbolic link refuses the listing. */
  async files(rel) {
    const value = await this.operation({ op: 'files', path: safeRelative(rel) });
    return Object.entries(value ?? {}).map(([path, modified]) => ({ path, modified, version: null }));
  }

  /** Remove the regular file at `rel`; an absent one is already removed. Stream heads and locks are refused. */
  async delete(rel) {
    await this.operation({ op: 'remove', path: safeRelative(rel) });
  }
}

/** An https origin: what a client sees. Reads heads and root files for the classifier; every write refuses — an object-store adapter is owed. */
export class HttpsOrigin {
  constructor(url) {
    this.url = url.replace(/\/+$/, '');
    this.kind = 'https';
    this.writable = false;
  }

  describe() { return this.url; }

  /** The body at `rel`, or null on 404. Any other failure is an error naming the URL.
   * A stream head is read within its 64 KiB bound; the web root's pointer,
   * which lists every file of the root, is not a head. */
  async get(rel) {
    const url = `${this.url}/${safeRelative(rel)}`;
    return this.getURL(url, /(?:^|\/)exact\.json$/i.test(rel) && rel !== webRootPath ? 64 << 10 : MAX_FILE_BYTES);
  }

  /** Read a resolved file card, including its query. */
  async getURL(url, cap = MAX_FILE_BYTES) {
    const { response, bytes } = await publisherFetch(url, { headers: { 'cache-control': 'no-cache' } }, cap);
    if (response.status === 404) return null;
    if (!response.ok) throw new OriginUnavailable(`${url}: HTTP ${response.status}`);
    return bytes;
  }

  /** An https origin cannot be listed: null, and the caller says so. */
  async list() { return null; }

  async head(stream) {
    const bytes = await this.get(`${streamPath(stream)}/exact.json`);
    if (!bytes) return null;
    let json;
    try { json = JSON.parse(bytes.toString('utf8')); } catch (e) { throw new Error(`the head at ${this.url}/${streamPath(stream)}/exact.json is not JSON: ${e.message}`); }
    return { bytes, sha256: sha256(bytes), json };
  }

  readOnly() { throw new Error(`${this.url} is an https origin, read-only in v1: point --origin at the directory the host serves (an object-store adapter with a conditional put is owed)`); }
  async put() { this.readOnly(); }
  async withLock() { this.readOnly(); }
  async putHead() { this.readOnly(); }
}

/** An AppDrop origin: reads like any https origin, lists its directories,
 * and writes with every request signed by the update key its URL is named
 * after (`useSigner`). There is no lock to hold: AppDrop refuses a head that
 * doesn't raise the stream's seq, and the head is put `If-Match` the ETag of
 * the head whose digest the publisher read. */
export class AppDropOrigin extends HttpsOrigin {
  constructor(url) {
    super(url);
    this.kind = 'appdrop';
    this.writable = true;
    // AppDrop holds update streams, not a web app's root files.
    this.servesWeb = false;
    this.keyId = this.url.split('/').at(-1);
  }

  /** Sign writes with the deploy signer, refused when its key isn't the one this origin is named after. */
  useSigner(signer) {
    const raw = Buffer.from(signer.publicKey, 'base64');
    if (appdropKeyId(raw) !== this.keyId) throw new Error(`${this.url} belongs to another update key; ${signer.keyId} publishes to ${this.url.replace(/[0-9a-f]{32}$/, appdropKeyId(raw))}`);
    this.signer = signer;
    return this;
  }

  async list(rel) {
    const url = `${this.url}/${safeRelative(rel)}/`;
    const { response, bytes: received } = await publisherFetch(url, { headers: { 'cache-control': 'no-cache' } });
    if (!response.ok) throw new OriginUnavailable(`${url}: HTTP ${response.status}`);
    return JSON.parse(received.toString('utf8')).names;
  }

  async send(rel, bytes, headers = {}) {
    if (!this.signer) throw new Error(`${this.url}: writes are signed with the update key; no signer was given`);
    const path = safeRelative(rel);
    const body = Buffer.from(bytes);
    const digest = path.startsWith('.exact/blobs/') ? path.slice('.exact/blobs/'.length) : sha256(body);
    const message = Buffer.from(`appdrop-exact-put\n${this.keyId}\n${path}\n${digest}`);
    const { response, bytes: received } = await publisherFetch(`${this.url}/${path}`, { method: 'PUT', body, headers: {
      'x-exact-public-key': this.signer.publicKey,
      'x-exact-signature': this.signer.signBytes(message).toString('base64'),
      ...headers,
    } });
    let reply;
    try { reply = JSON.parse(received.toString('utf8')); } catch { reply = {}; }
    if (response.status >= 500) throw new OriginUnavailable(`${this.url}/${path}: HTTP ${response.status}`);
    if (!response.ok) throw new Error(`${this.url}/${path}: ${reply.error ?? `HTTP ${response.status}`}`);
    return reply;
  }

  /** A blob or release record: written once, `present` when it already is. */
  async put(rel, bytes) {
    return (await this.send(rel, bytes)).written ? 'written' : 'present';
  }

  async withLock(stream, fn) { return fn(); }

  /** Replace the head only if it is still the one whose digest the publisher read (none, when `previousDigest` is null). */
  async putHead(stream, bytes, { previousDigest = null } = {}) {
    const rel = `${streamPath(stream)}/exact.json`;
    if (previousDigest === null) return this.send(rel, bytes, { 'if-none-match': '*' });
    const url = `${this.url}/${rel}`;
    const { response, bytes: received } = await publisherFetch(url, { headers: { 'cache-control': 'no-cache' } });
    if (!response.ok) throw new Error(`${url}: the head the publisher read is gone (HTTP ${response.status})`);
    const etag = response.headers.get('etag');
    if (sha256(received) !== previousDigest) throw new Error(`${url}: the head changed since it was read`);
    return this.send(rel, bytes, { 'if-match': etag });
  }
}

// `s3+https://<endpoint>/<bucket>[/<prefix>]`: S3's API at the endpoint, the
// bucket in the path. For Cloudflare R2 the endpoint is
// `<account id>.r2.cloudflarestorage.com`. The spelling names no secret.
const OBJECT_STORE = /^s3\+https?:\/\//;

/** RFC 3986 percent-encoding, as SigV4 requires: only unreserved characters stay. */
const uriEncode = (text) => encodeURIComponent(text).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
const hmac = (key, text) => createHmac('sha256', key).update(text).digest();

/** The headers that sign a request with AWS Signature Version 4, in its
 * Authorization header, over every header given and the payload's SHA-256.
 * `path` is the URI-encoded path and `query` the unencoded parameters; S3
 * encodes the path once. https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html */
export function signS3({ method, host, path, query = {}, headers = {}, body = Buffer.alloc(0), accessKeyId, secretAccessKey, region, date = new Date() }) {
  const stamp = date.toISOString().replace(/[-:]/g, '').replace(/\.\d+/, '');
  const day = stamp.slice(0, 8), scope = `${day}/${region}/s3/aws4_request`;
  const signed = { ...Object.fromEntries(Object.entries(headers).map(([k, v]) => [k.toLowerCase(), String(v)])),
    host, 'x-amz-content-sha256': sha256(body), 'x-amz-date': stamp };
  const names = Object.keys(signed).sort();
  const canonicalQuery = Object.entries(query).map(([k, v]) => [uriEncode(k), uriEncode(String(v))])
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)).map(([k, v]) => `${k}=${v}`).join('&');
  const canonical = [method, path, canonicalQuery, ...names.map((n) => `${n}:${signed[n].trim().replace(/\s+/g, ' ')}`), '',
    names.join(';'), signed['x-amz-content-sha256']].join('\n');
  const toSign = ['AWS4-HMAC-SHA256', stamp, scope, sha256(canonical)].join('\n');
  const key = hmac(hmac(hmac(hmac(`AWS4${secretAccessKey}`, day), region), 's3'), 'aws4_request');
  const signature = createHmac('sha256', key).update(toSign).digest('hex');
  return { ...signed, authorization: `AWS4-HMAC-SHA256 Credential=${accessKeyId}/${scope}, SignedHeaders=${names.join(';')}, Signature=${signature}` };
}

const xmlText = (text) => text.replace(/&(lt|gt|quot|apos|amp|#\d+|#x[0-9a-f]+);/gi, (_, e) => ({ lt: '<', gt: '>', quot: '"', apos: "'", amp: '&' })[e.toLowerCase()]
  ?? String.fromCodePoint(e[1].toLowerCase() === 'x' ? parseInt(e.slice(2), 16) : Number(e.slice(1))));
const xmlAll = (xml, tag) => [...xml.matchAll(new RegExp(`<${tag}>([\\s\\S]*?)</${tag}>`, 'g'))].map((m) => m[1]);
const xmlOne = (xml, tag) => { const value = xmlAll(xml, tag)[0]; return value === undefined ? undefined : xmlText(value); };

/** A bucket written through S3's API: the tools publisher's origin (LLP 1174
 * D4.2). Credentials come from the environment of the publisher only:
 * `EXACT_S3_ACCESS_KEY_ID`, `EXACT_S3_SECRET_ACCESS_KEY` and, for a store
 * other than R2, `EXACT_S3_REGION` (R2's region is `auto`). There is no lock
 * to hold: a file that must not lose a concurrent writer's change is written
 * with `replace`, a PUT conditional on the ETag read, which R2 honours
 * (`If-Match`, `If-None-Match`: https://developers.cloudflare.com/r2/api/s3/api/) and refuses with 412 when another
 * writer came first. */
export class ObjectStoreOrigin {
  constructor(spec, env = process.env) {
    // Diagnostics never repeat the spelling given: it might carry a secret.
    if (!OBJECT_STORE.test(spec)) throw new Error('an object-store origin is spelled s3+https://<endpoint>/<bucket>');
    const url = new URL(spec.slice('s3+'.length));
    if (url.username || url.password || url.search || url.hash) throw new Error('an object-store origin names its endpoint and bucket only; credentials come from EXACT_S3_ACCESS_KEY_ID and EXACT_S3_SECRET_ACCESS_KEY');
    // Over plain HTTP nothing authenticates the store's answers, among them the listing a pruning pass deletes by.
    if (url.protocol === 'http:' && !['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname)) throw new Error('an object-store origin is reached over https, unless it is on this machine');
    const [bucket, ...prefix] = url.pathname.split('/').filter(Boolean).map(decodeURIComponent);
    if (!bucket) throw new Error('name the bucket: s3+https://<endpoint>/<bucket>');
    this.endpoint = url.origin;
    this.host = url.host;
    this.bucket = bucket;
    this.prefix = prefix.length ? `${prefix.map(safeRelative).join('/')}/` : '';
    // One spelling for one bucket and prefix, which names the publishers' lock (`withPublication`).
    this.spec = `s3+${this.endpoint}/${[bucket, ...prefix].join('/')}`;
    this.env = env;
    this.kind = 'object-store';
    this.writable = true;
    this.servesWeb = false;
  }

  describe() { return this.spec; }

  credentials() {
    const { EXACT_S3_ACCESS_KEY_ID: accessKeyId, EXACT_S3_SECRET_ACCESS_KEY: secretAccessKey, EXACT_S3_REGION: region = 'auto' } = this.env;
    if (!accessKeyId || !secretAccessKey) throw new Error(`${this.spec}: set EXACT_S3_ACCESS_KEY_ID and EXACT_S3_SECRET_ACCESS_KEY to the bucket's write token`);
    return { accessKeyId, secretAccessKey, region };
  }

  /** A signed request for `key` (the bucket itself when null). */
  async request(method, key, { body, headers = {}, query = {}, cap = 64 << 10 } = {}) {
    const path = `/${[this.bucket, ...(key === null ? [] : key.split('/'))].map(uriEncode).join('/')}`;
    const payload = body === undefined ? Buffer.alloc(0) : Buffer.from(body);
    const signed = signS3({ method, host: this.host, path, query, headers, body: payload, ...this.credentials() });
    const search = Object.entries(query).map(([k, v]) => `${uriEncode(k)}=${uriEncode(String(v))}`).join('&');
    const url = `${this.endpoint}${path}${search ? `?${search}` : ''}`;
    // An upload is given at least 100 KB/s beyond the 30 s every request has. R2 answers 429 to more than one write a
    // second to one key, so a 429 is asked again a second later, twice (https://developers.cloudflare.com/r2/api/error-codes/).
    for (let attempt = 1; ; attempt++) {
      const answer = await publisherFetch(url, { method, headers: signed, body: body === undefined ? undefined : payload }, cap, 30_000 + payload.length / 100);
      if (answer.response.status !== 429 || attempt === 3) return answer;
      await Bun.sleep(1000);
    }
  }

  fail(method, rel, response, bytes) {
    const reason = `${method} ${this.spec}/${rel}: HTTP ${response.status}${bytes.length ? ` ${xmlOne(bytes.toString('utf8'), 'Code') ?? ''}` : ''}`.trim();
    return response.status >= 500 || response.status === 429 ? new OriginUnavailable(reason) : new Error(reason);
  }

  async getVersion(rel) {
    const { response, bytes } = await this.request('GET', this.prefix + safeRelative(rel), { cap: MAX_FILE_BYTES });
    if (response.status === 404) return { bytes: null, version: null };
    if (!response.ok) throw this.fail('GET', rel, response, bytes);
    return { bytes, version: response.headers.get('etag') };
  }

  async get(rel) { return (await this.getVersion(rel)).bytes; }

  /** An immutable file (a content-addressed blob or manifest) is put only where
   * absent, `present` otherwise, and may be cached for a year; any other is revalidated on each read. */
  async put(rel, bytes, { immutable = false } = {}) {
    const headers = { 'cache-control': immutable ? 'public, max-age=31536000, immutable' : 'no-cache' };
    if (immutable) headers['if-none-match'] = '*';
    const { response, bytes: reply } = await this.request('PUT', this.prefix + safeRelative(rel), { body: bytes, headers });
    if (immutable && response.status === 412) return 'present';
    if (!response.ok) throw this.fail('PUT', rel, response, reply);
    return 'written';
  }

  /** Write `rel` only if its ETag is still `version` (absent, when null): false when another writer came first. */
  async replace(rel, bytes, version) {
    const headers = { 'cache-control': 'no-cache', ...(version === null ? { 'if-none-match': '*' } : { 'if-match': version }) };
    const { response, bytes: reply } = await this.request('PUT', this.prefix + safeRelative(rel), { body: bytes, headers });
    if (response.status === 412) return false;
    if (!response.ok) throw this.fail('PUT', rel, response, reply);
    return true;
  }

  /** S3 answers 204 whether or not the key existed. */
  async delete(rel) {
    const { response, bytes } = await this.request('DELETE', this.prefix + safeRelative(rel));
    if (!response.ok && response.status !== 404) throw this.fail('DELETE', rel, response, bytes);
  }

  /** Every object under `rel`, with its modification time and ETag: ListObjectsV2, page by page. */
  async files(rel) {
    const prefix = `${this.prefix}${safeRelative(rel)}/`, found = [];
    for (let token; ;) {
      const query = { 'list-type': '2', prefix, ...(token ? { 'continuation-token': token } : {}) };
      const { response, bytes } = await this.request('GET', null, { query, cap: 16 << 20 });
      if (!response.ok) throw this.fail('LIST', rel, response, bytes);
      const xml = bytes.toString('utf8'), truncated = xmlOne(xml, 'IsTruncated');
      // What a pruning pass does not list it may take for unnamed: a page that is not whole stops the walk.
      if (!/<ListBucketResult[\s>]/.test(xml) || !['true', 'false'].includes(truncated)) throw new Error(`LIST ${this.spec}/${rel}: not a ListObjectsV2 page`);
      for (const item of xmlAll(xml, 'Contents')) {
        const key = xmlOne(item, 'Key'), modified = Date.parse(xmlOne(item, 'LastModified') ?? '');
        if (!key?.startsWith(prefix) || !Number.isFinite(modified)) throw new Error(`LIST ${this.spec}/${rel}: an entry without its key or time`);
        found.push({ path: key.slice(this.prefix.length), modified, version: xmlOne(item, 'ETag') ?? null });
      }
      if (truncated === 'false') return found;
      token = xmlOne(xml, 'NextContinuationToken');
      if (!token) throw new Error(`LIST ${this.spec}/${rel}: a truncated page without a continuation token`);
    }
  }

  async withLock(stream, fn) { return fn(); }
}
