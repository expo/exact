import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

/** A refusal: printed as one line, exit 1. Anything else is a bug and keeps its stack. */
export class Refusal extends Error {}
export const refuse = (message) => { throw new Refusal(message); };

// ---------------------------------------------------------------- the signer

/** One JSON value in the signed representation. Write sorted entries
 * directly: rebuilding an object and then calling `JSON.stringify` is not
 * canonical for integer-like keys because JavaScript enumerates those in
 * numeric order regardless of insertion order. */
function requireUnicodeScalars(text) {
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const low = text.charCodeAt(i + 1);
      if (!(low >= 0xdc00 && low <= 0xdfff)) refuse('the head carries an unpaired UTF-16 surrogate, not a Unicode scalar value');
      i++;
    } else if (unit >= 0xdc00 && unit <= 0xdfff) refuse('the head carries an unpaired UTF-16 surrogate, not a Unicode scalar value');
  }
  return text;
}

export function canonicalJson(value, dropSignature = false) {
  if (value === null) return 'null';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'string') return JSON.stringify(requireUnicodeScalars(value));
  if (typeof value === 'number') {
    if (!Number.isSafeInteger(value) || Object.is(value, -0)) refuse(`the head carries the unsafe or non-canonical integer ${value}; canonical bytes require exact integers (update/src/envelope.rs)`);
    return String(value);
  }
  if (Array.isArray(value)) return `[${value.map((item) => canonicalJson(item)).join(',')}]`;
  if (!value || typeof value !== 'object') refuse(`the head carries a ${typeof value}; canonical bytes require JSON values`);
  const keys = Object.keys(value).filter((key) => !dropSignature || key !== 'signature').map(requireUnicodeScalars)
    .sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  return `{${keys.map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
}

/** Refuse a numeric token whose spelling Rust's canonical parser cannot
 * admit. `JSON.parse` alone loses this evidence by normalizing `7.0`, `7e0`,
 * and `-0` to the number 7 or 0 before the signature check sees it. */
export function validateRawIntegers(text) {
  for (let i = 0; i < text.length;) {
    if (text[i] === '"') {
      i++;
      while (i < text.length && text[i] !== '"') {
        if (text[i] !== '\\') { i++; continue; }
        if (text[i + 1] !== 'u') { i += 2; continue; }
        const first = Number.parseInt(text.slice(i + 2, i + 6), 16);
        if (first >= 0xd800 && first <= 0xdbff) {
          const second = text.slice(i + 6, i + 8) === '\\u' ? Number.parseInt(text.slice(i + 8, i + 12), 16) : NaN;
          if (!(second >= 0xdc00 && second <= 0xdfff)) throw new Error('the envelope carries an escaped lone surrogate, not a Unicode scalar value');
          i += 12;
          continue;
        }
        if (first >= 0xdc00 && first <= 0xdfff) throw new Error('the envelope carries an escaped lone surrogate, not a Unicode scalar value');
        i += 6;
      }
      if (text[i] === '"') i++;
      continue;
    }
    if (text[i] !== '-' && (text[i] < '0' || text[i] > '9')) { i++; continue; }
    const token = /^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/.exec(text.slice(i))?.[0];
    if (!token) { i++; continue; } // JSON.parse below reports malformed JSON.
    if (!/^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/.test(token) || !Number.isSafeInteger(Number(token))) {
      throw new Error(`the envelope carries the non-canonical integer ${token}; canonical bytes require shortest exact integers`);
    }
    i += token.length;
  }
}

/** The bytes the signature covers: the head without its top-level
 * `signature`, keys sorted by UTF-8 bytes and emitted directly. */
export function canonicalBytes(head) {
  if (!head || typeof head !== 'object' || Array.isArray(head)) refuse('the envelope is not a JSON object');
  return Buffer.from(canonicalJson(head, true), 'utf8');
}

// ------------------------------------------------------- the envelope's files

/** An envelope's bound and its asset index's, in bytes: `update/src/lib.rs`
 * `MAX_ENVELOPE_BYTES` and `MAX_ASSET_INDEX_BYTES`, which every client
 * applies. The asset cards live in the index so that the envelope stays a
 * pointer card whatever the asset count. */
export const ENVELOPE_BYTES = 64 * 1024, ASSET_INDEX_BYTES = 4 * 1024 * 1024;
const kib = (n) => `${(n / 1024).toFixed(1)} KiB`;

/** Named snapshot plans share the asset roster's authenticated cards (LLP
 * 1126 D2): the index's `plans`, by entry kind. */
export function entryPlans(assets) {
  const plans = Object.fromEntries(assets.flatMap(card => {
    const kind = /^plans\/([A-Za-z][A-Za-z0-9_-]*)\.plan$/.exec(card.name)?.[1];
    return kind ? [[kind, card]] : [];
  }));
  return Object.keys(plans).length ? { plans } : {};
}

/** `cards`' asset index at `url` (or at `url(sha256)`, for a blob named by its digest): its bytes, the canonical JSON of
 * `{"assets":[…],"plans":{…}}`, and the envelope's card for it
 * (`update/src/envelope.rs`). A card's URL resolves against the index's own.
 * Null for no assets, whose envelope omits `assets`. Refused over its bound. */
export function assetIndex(cards, url) {
  if (!cards.length) return null;
  const bytes = canonicalBytes({ assets: cards, ...entryPlans(cards) });
  if (bytes.length > ASSET_INDEX_BYTES) refuse(`the asset index is ${kib(bytes.length)} for ${cards.length} assets; the most is ${kib(ASSET_INDEX_BYTES)}`);
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  return { bytes, card: { url: typeof url === 'function' ? url(sha256) : url, sha256, bytes: bytes.length, count: cards.length } };
}

/** An envelope's bytes as served, refused over its bound with what it holds. */
export function envelopeBytes(envelope) {
  const bytes = Buffer.from(JSON.stringify(envelope) + '\n', 'utf8');
  if (bytes.length > ENVELOPE_BYTES) refuse(`the envelope is ${kib(bytes.length)}; the most is ${kib(ENVELOPE_BYTES)} (its asset index names ${envelope.assets?.count ?? 0} assets)`);
  return bytes;
}

/** `envelope` as served: its asset cards moved to the index at `indexUrl`,
 * the bytes of both (`index` null for no assets), and a line stating each
 * size against its bound with the card count, so an author sees it at build. */
export function sealEnvelope({ assets = [], ...envelope }, indexUrl = './assets.json') {
  const index = assetIndex(assets, indexUrl);
  if (index) envelope.assets = index.card;
  const bytes = envelopeBytes(envelope);
  return { envelope, bytes, index: index?.bytes ?? null, report: envelopeReport(bytes.length, index?.bytes.length ?? null, assets.length) };
}

/** Both files' sizes against their bounds and the card count, as a line; a
 * file over its bound is said to be one every client refuses. */
export function envelopeReport(envelope, index, count) {
  const over = (n, most) => n > most ? ' (over its bound: every client refuses it)' : '';
  return `exact.json ${kib(envelope)} of ${kib(ENVELOPE_BYTES)}${over(envelope, ENVELOPE_BYTES)}, ${index === null ? 'no asset index:' : `assets.json ${kib(index)} of ${kib(ASSET_INDEX_BYTES)}${over(index, ASSET_INDEX_BYTES)} for`} ${count} asset${count === 1 ? '' : 's'}`;
}

export function fileCard(object, name) {
  if (!object || typeof object !== 'object' || Array.isArray(object)) throw new Error(`the envelope names no ${name}`);
  if (typeof object.url !== 'string') throw new Error(`${name} has no url`);
  if (typeof object.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(object.sha256)) throw new Error(`${name}'s sha256 is not 64 lowercase hex digits`);
  if (!Number.isSafeInteger(object.bytes) || object.bytes < 0) throw new Error(`${name} has no exact nonnegative byte count`);
}

/** The envelope's card for its asset index, refused over the index's bound
 * unless `bounded` is false (a web root, which a browser loads at any size). */
export function indexCard(card, bounded = true) {
  fileCard(card, 'assets.json');
  if (!Number.isSafeInteger(card.count) || card.count < 0) refuse('assets.json has no exact nonnegative card count');
  if (bounded && card.bytes > ASSET_INDEX_BYTES) refuse(`the asset index is ${kib(card.bytes)} for ${card.count} assets; the most is ${kib(ASSET_INDEX_BYTES)}`);
}

/** exact-update's `safe_name`: a relative path whose segments use only the
 * POSIX portable filename characters and `@` (`photo@2x.png`), so no
 * filesystem can fold or normalize two names into one file. */
function safeAssetName(name) {
  if (typeof name !== 'string' || !name) throw new Error('an asset has no nonempty name');
  for (const part of name.split('/')) {
    if (!part || part === '.' || part === '..') throw new Error(`the asset name ${name} is not a relative path`);
    if (!/^[A-Za-z0-9._@-]+$/.test(part)) throw new Error(`the asset name ${name} is not portable: segments use A-Z, a-z, 0-9, '.', '_', '-' and '@'`);
  }
}

/** exact-update's `check_asset_names`: a roster stages as distinct files on a
 * case-insensitive filesystem (APFS's default), so no two names differ only
 * by case and none is a file where another needs a directory. The bake and
 * every client refuse the same rosters, and name every refused name at once. */
export function portableAssetNames(names) {
  const files = new Map(), directories = new Map(), problems = [];
  for (const name of names) {
    try { safeAssetName(name); } catch (error) { problems.push(error.message); continue; }
    const folded = name.toLowerCase(), other = files.get(folded); // ASCII after safeAssetName
    if (other !== undefined) { problems.push(other === name ? `the envelope names the asset ${name} twice` : `the assets ${other} and ${name} differ only by case, one file on a case-insensitive filesystem`); continue; }
    if (directories.has(folded)) { problems.push(`the asset ${name} is a file where ${directories.get(folded)} needs a directory`); continue; }
    for (let end = folded.indexOf('/'); end !== -1; end = folded.indexOf('/', end + 1)) {
      const directory = folded.slice(0, end);
      if (files.has(directory)) problems.push(`the asset ${files.get(directory)} is a file where ${name} needs a directory`);
      if (!directories.has(directory)) directories.set(directory, name);
    }
    files.set(folded, name);
  }
  if (problems.length) throw new Error(problems.join('\n'));
}

/** The asset cards of an envelope whose index `card` is these `raw` bytes:
 * refused unless they are its exact bytes and hold `count` cards with
 * portable, distinct names (`update/src/envelope.rs` `read_index`). */
export function readAssetIndex(card, raw, { bounded = true } = {}) {
  indexCard(card, bounded);
  if (!raw || raw.length !== card.bytes || createHash('sha256').update(raw).digest('hex') !== card.sha256) refuse('the asset index is not the bytes its envelope names');
  let index;
  try { index = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(raw)); } catch (error) { refuse(`the asset index is not JSON: ${error.message}`); }
  const assets = index?.assets;
  if (!Array.isArray(assets) || assets.length !== card.count) refuse(`the asset index holds ${Array.isArray(assets) ? assets.length : 'no list of'} assets; its envelope declared ${card.count}`);
  for (const asset of assets) fileCard(asset, typeof asset?.name === 'string' ? asset.name : 'an asset');
  portableAssetNames(assets.map((asset) => asset.name));
  if (index.plans !== undefined && canonicalJson(index.plans) !== canonicalJson(entryPlans(assets).plans ?? {})) refuse("the asset index's plans must index its authenticated plan assets");
  return assets;
}

/** An Ed25519 public key from its 32 raw bytes: the SubjectPublicKeyInfo prefix the crypto API wants, then the bytes. */
export function publicKeyFromRaw(raw) {
  if (raw.length !== 32) refuse(`an Ed25519 public key is 32 bytes; this one is ${raw.length}`);
  return createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), raw]), format: 'der', type: 'spki' });
}

/** The 32 raw bytes of an Ed25519 public key object. */
const rawPublic = (key) => key.export({ type: 'spki', format: 'der' }).subarray(-32);

/** The signer for the app's `deploy.signing.key`: the PEM at `<keys>/<id>.pem`, refused by path when missing, and refused when the manifest's public key for that id is not this private key's. A dry run never calls this. */
export function loadSigner(app, keysDir) {
  const signing = app.manifest.deploy?.signing ?? {};
  const keyId = signing.key;
  if (!keyId) refuse(`${app.dir}/app.json names no deploy.signing.key: a publish signs every head (LLP 1026 D11); bun scripts/deploy.mjs keygen <id> makes one`);
  const path = resolve(keysDir, `${keyId}.pem`);
  if (!existsSync(path)) refuse(`no signing key at ${path} — bun scripts/deploy.mjs keygen ${keyId} --keys ${keysDir} writes one (and prints the public half for app.json's deploy.signing.keys["${keyId}"])`);
  let privateKey;
  try { privateKey = createPrivateKey(readFileSync(path)); } catch (e) { refuse(`${path} is not a PEM private key: ${e.message}`); }
  if (privateKey.asymmetricKeyType !== 'ed25519') refuse(`${path} is a ${privateKey.asymmetricKeyType} key; the head is signed with Ed25519`);
  const declared = signing.keys?.[keyId];
  if (!declared) refuse(`${app.dir}/app.json's deploy.signing.keys has no "${keyId}": the binary verifies with that key (LLP 1030 D3a); paste the public half keygen printed`);
  const manifestKey = publicKeyFromRaw(Buffer.from(declared, 'base64'));
  const probe = Buffer.from('exact deploy: is this key mine?');
  if (!verify(null, probe, manifestKey, sign(null, probe, privateKey))) refuse(`the manifest's public key for ${keyId} is not this private key's (${path}): a head signed here would be refused by every installed binary`);
  return {
    keyId,
    path,
    publicKey: rawPublic(createPublicKey(privateKey)).toString('base64'),
    /** An Ed25519 signature over `bytes`: how an AppDrop origin authenticates a write. */
    signBytes(bytes) { return sign(null, bytes, privateKey); },
    /** The `signature` member for `head` (which must not carry one yet). */
    sign(head) { return { keyId, ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }; },
  };
}

/** `keygen <id>`: a fresh Ed25519 key at `<keys>/<id>.pem` (mode 0600, never overwritten) and its public half printed for the manifest. */
export function keygen(opts, usage) {
  const id = opts._[1];
  if (!id || !/^[A-Za-z0-9._-]+$/.test(id)) refuse(`keygen <id>: letters, digits, . _ - (it names the PEM and the manifest entry)\n${usage}`);
  const path = resolve(opts.keys, `${id}.pem`);
  if (existsSync(path)) refuse(`${path} exists: a signing key is never overwritten — remove it yourself, or pick another id (a rotation is a new cohort, LLP 1030 D3a)`);
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  mkdirSync(opts.keys, { recursive: true, mode: 0o700 });
  try {
    writeFileSync(path, privateKey.export({ type: 'pkcs8', format: 'pem' }), { mode: 0o600, flag: 'wx' });
  } catch (error) {
    if (error.code === 'EEXIST') refuse(`${path} exists: a signing key is never overwritten — remove it yourself, or pick another id (a rotation is a new cohort, LLP 1030 D3a)`);
    throw error;
  }
  const raw = rawPublic(publicKey).toString('base64');
  if (opts.json) console.log(JSON.stringify({ id, path, publicKey: raw }));
  else console.log(`wrote ${path}\npaste into app.json → deploy.signing.keys["${id}"]: ${JSON.stringify(raw)}`);
  return 0;
}
