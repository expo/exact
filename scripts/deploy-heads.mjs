// A stream's head admitted as exact-update admits it, and the sequence the next release takes above
// everything the stream has authenticated (LLP 1030.000 D3 item 5): scripts/deploy.mjs's classifier and
// publisher read both.
import { verify } from 'node:crypto';
import { mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { refuse, canonicalBytes, ENVELOPE_BYTES, fileCard, publicKeyFromRaw, indexCard, readAssetIndex, validateRawIntegers } from './deploy-signing.mjs';
import { channelOrigin, MAX_FILE_BYTES, OriginUnavailable, safeRelative, sha256, streamPath } from './origin.mjs';

/** Binary source evidence travels as a blob: a full compiler input roster
 * can exceed the envelope's size limit. Its digest is signed in the cohort. */
export function binaryInputFile(build) {
  if (!build?.binary?.sourceInputs) return null;
  const bytes = canonicalBytes(build.binary.sourceInputs);
  return { name: 'binary-inputs.json', bytes, sha256: sha256(bytes) };
}

/** Frozen capabilities and binary evidence. Absolute source paths stay in
 * the private build receipt. */
export function cohortReceipt(build) {
  if (build?.version !== 1 || build.graph?.version !== 1 || !build.binary?.sha256) throw new Error('missing completed bake graph and binary receipt');
  const evidence = binaryInputFile(build);
  return {version:1,compat:{id:build.compat.id,inputs:build.compat.inputs,target:build.compat.target},sources:build.graph.sources,surfaceCalls:build.graph.surfaceCalls??{},binary:build.binary.sha256,
    ...(evidence ? {binaryInputs:{sha256:evidence.sha256,bytes:evidence.bytes.length}} : {})};
}

function cohortBlob(name, bytes, prefix) {
  const digest = sha256(bytes);
  const card = { url: `${prefix}${digest}`, sha256: digest, bytes: bytes.length };
  return { name, bytes, sha256: digest, card };
}

export function cohortFiles(receipt) {
  const bytes = canonicalBytes(receipt);
  if (bytes.length <= MAX_FILE_BYTES) return [cohortBlob('cohort receipt', bytes, '../../blobs/')];
  const chunks = [];
  for (let offset = 0; offset < bytes.length; offset += MAX_FILE_BYTES) {
    chunks.push(cohortBlob('cohort receipt chunk', bytes.subarray(offset, offset + MAX_FILE_BYTES), './'));
  }
  const index = canonicalBytes({ chunks: chunks.map(chunk => chunk.card) });
  const manifest = cohortBlob('cohort receipt manifest', index, '../../blobs/');
  manifest.card.chunked = true;
  return [manifest, ...chunks];
}

export function sameCohortPayload(a, b) {
  return a.sha256 === b.sha256 && a.bytes === b.bytes && a.chunked === b.chunked;
}

function cohortForStream(receipt, stream) {
  return receipt?.version === 1 && receipt.compat?.id === stream.compatibilityId
    && receipt.compat.inputs && receipt.sources && receipt.binary ? receipt : null;
}

function headFiles(origin, app, stream) {
  const configured = origin.url ?? channelOrigin(app.manifest, stream.channel);
  const root = new URL(/^https?:\/\//.test(configured) ? `${configured.replace(/\/+$/, '')}/` : 'https://origin.invalid/');
  const headUrl = new URL(`${streamPath(stream)}/exact.json`, root);
  return { headUrl, async read(card, base = headUrl) {
    const url = resolveCardURL(base, card.url);
    if (origin.getURL) return { url, raw: await origin.getURL(url.href) };
    if (!url.pathname.startsWith(root.pathname)) refuse('the file card is outside the directory origin');
    const path = decodeURIComponent(url.pathname.slice(root.pathname.length));
    return { url, raw: await origin.get(safeRelative(path)) };
  } };
}

// Same-origin HTTP file cards use the same URL rules as exact-update.
function resolveCardURL(base, reference) {
  const absolute = reference.includes('://') || reference.startsWith('//');
  const authority = /^(?:https?:)?\/\/([^/?#]+)/.exec(reference);
  if (reference.includes('\\') || (absolute ? !authority || authority[1].includes('@') : reference.includes(':') && !reference.startsWith('/'))) {
    refuse('the file card must name an HTTP file on the stream origin');
  }
  const rawPath = (absolute ? reference.slice(authority[0].length) : reference).split(/[?#]/, 1)[0];
  let depth = absolute || rawPath.startsWith('/') ? 0 : base.pathname.split('/').length - 2;
  for (const segment of rawPath.replace(/^\//, '').split('/')) {
    if (segment === '.') continue;
    if (segment === '..') {
      if (depth === 0) refuse('the file card climbs above the stream origin');
      depth--;
    } else depth++;
  }
  const url = new URL(reference, base);
  if (url.origin !== base.origin || url.username || url.password) refuse('the file card must name an HTTP file on the stream origin');
  url.hash = '';
  return url;
}

function cohortCard(card, name) {
  fileCard(card, name);
  if (card.bytes > MAX_FILE_BYTES) refuse(`${name} is ${card.bytes} bytes; the most is ${MAX_FILE_BYTES}`);
}

function checkedCohortBytes(card, raw, label) {
  if (!raw || raw.length !== card.bytes || sha256(raw) !== card.sha256) refuse(`${label} is not the bytes its envelope names`);
  return raw;
}

function cohortJson(raw, label, { exactIntegers = false } = {}) {
  let text;
  // Preserve a leading BOM so JSON.parse rejects it as the native parser does.
  try { text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(raw); }
  catch (error) { refuse(`${label} is not UTF-8: ${error.message}`); }
  if (exactIntegers) validateRawIntegers(text, label);
  try { return JSON.parse(text); }
  catch (error) { refuse(`${label} is not JSON: ${error.message}`); }
}

/** Read the cohort from a head whose signature has already been checked. */
async function readHeadCohort(files, head, stream) {
  const card = head.cohortReceipt;
  // The additive envelope format (LLP 1030 D9) also reads signed inline receipts.
  if (!card) return cohortForStream(head.cohort, stream);
  const { url, raw } = await files.read(card);
  let bytes = checkedCohortBytes(card, raw, 'the cohort receipt');
  if (card.chunked) {
    const chunks = cohortJson(bytes, 'the cohort receipt manifest', { exactIntegers: true })?.chunks;
    if (!Array.isArray(chunks) || !chunks.length) refuse('the cohort receipt manifest has no chunks');
    for (const chunk of chunks) cohortCard(chunk, 'a cohort receipt chunk');
    const bodies = [];
    for (const chunk of chunks) {
      const { raw } = await files.read(chunk, url);
      bodies.push(checkedCohortBytes(chunk, raw, 'a cohort receipt chunk'));
    }
    bytes = Buffer.concat(bodies);
  }
  const receipt = cohortJson(bytes, 'the cohort receipt');
  const cohort = cohortForStream(receipt, stream);
  if (!cohort) refuse(`the cohort receipt does not describe cohort ${stream.compatibilityId} in version 1`);
  return cohort;
}

function strictBase64(text, length, label) {
  if (typeof text !== 'string') throw new Error(`${label} is not base64 text`);
  const decoded = Buffer.from(text, 'base64');
  if (decoded.length !== length || decoded.toString('base64') !== text) throw new Error(`${label} is not canonical base64 of ${length} bytes`);
  return decoded;
}

/** Authenticate the head, then verify its asset index and cohort receipt. */
export async function admitHead(origin, found, app, stream) {
  const admission = inspectHead(found, app, stream);
  if (!admission.usable) return admission;
  try {
    const files = headFiles(origin, app, stream);
    admission.cohort = await readHeadCohort(files, admission.head, stream);
    const card = admission.head.assets;
    const assets = card ? readAssetIndex(card, (await files.read(card)).raw) : [];
    return { ...admission, head: { ...admission.head, assets } };
  } catch (error) {
    if (error instanceof OriginUnavailable) throw error;
    return { ...admission, usable: false, seq: admission.authenticated ? admission.seq : null, problem: error.message || String(error) };
  }
}

/** Parse and authenticate a head before `admitHead` reads its payloads. */
export function inspectHead(found, app, stream) {
  try {
    if (!Buffer.isBuffer(found?.bytes)) throw new Error('the origin returned no head bytes');
    if (found.bytes.length > ENVELOPE_BYTES) throw new Error(`the envelope is ${found.bytes.length} bytes; the most is ${ENVELOPE_BYTES}`);
    const text = new TextDecoder('utf-8', { fatal: true }).decode(found.bytes);
    validateRawIntegers(text);
    const head = JSON.parse(text);
    if (!head || typeof head !== 'object' || Array.isArray(head)) throw new Error('the envelope is not a JSON object');
    canonicalBytes(head); // recursively rejects every inexact JSON number
    if (head.exact !== 1) throw new Error(`the envelope is exact ${head.exact ?? '(missing)'}; this binary reads exact 1`);
    if (!head.app || typeof head.app !== 'object' || Array.isArray(head.app)) throw new Error('the envelope names no app');
    if (typeof head.app.id !== 'string' || !head.app.id) throw new Error('the envelope names no nonempty app id');
    fileCard(head.plan, 'app.plan');
    if (!head.stream || typeof head.stream !== 'object' || Array.isArray(head.stream)) throw new Error('the envelope names no stream');
    if (typeof head.stream.channel !== 'string') throw new Error('the stream names no channel');
    if (typeof head.stream.compatibilityId !== 'string') throw new Error('the stream names no compatibility id');
    if (!Number.isSafeInteger(head.stream.seq) || head.stream.seq < 0) throw new Error('the stream names no exact nonnegative seq');
    if (head.assets !== undefined) indexCard(head.assets);
    if (head.cohortReceipt !== undefined) {
      cohortCard(head.cohortReceipt, 'the cohort receipt');
      if (head.cohortReceipt.chunked !== undefined && head.cohortReceipt.chunked !== true) refuse('the cohort receipt has an invalid chunked flag');
    }
    if (head.sunset !== undefined) {
      if (!head.sunset || typeof head.sunset !== 'object' || Array.isArray(head.sunset)) throw new Error('the sunset card is not an object');
      if (typeof head.sunset.message !== 'string') throw new Error('the sunset card has no message');
    }
    let signature = null;
    if (head.signature !== undefined) {
      if (!head.signature || typeof head.signature !== 'object' || Array.isArray(head.signature)) throw new Error('the signature is not an object');
      if (typeof head.signature.keyId !== 'string') throw new Error('the signature names no key id');
      signature = strictBase64(head.signature.ed25519, 64, 'the signature');
    }
    if (head.app.id !== app.id) throw new Error(`the head is for ${head.app.id}; this binary is ${app.id}`);
    if (head.app.name !== app.displayName) throw new Error(`the head calls ${app.id} ${JSON.stringify(head.app.name)}, not ${JSON.stringify(app.displayName)}`);
    if (typeof head.stream.app === 'string' && head.stream.app !== app.id) throw new Error(`the head's stream is for ${head.stream.app}; this binary is ${app.id}`);
    if (head.stream.channel !== stream.channel) throw new Error(`the head is for channel ${head.stream.channel}; this binary is ${stream.channel}`);
    if (head.stream.compatibilityId !== stream.compatibilityId) throw new Error(`the head is for cohort ${head.stream.compatibilityId}; this binary is ${stream.compatibilityId}`);
    const keys = app.manifest.deploy?.signing?.keys ?? {};
    let authenticated = false;
    if (Object.keys(keys).length) {
      if (!signature) throw new Error('the head is unsigned and this binary carries keys');
      if (!Object.hasOwn(keys, head.signature.keyId)) throw new Error(`the head is signed by ${head.signature.keyId}, which this binary does not carry`);
      const key = strictBase64(keys[head.signature.keyId], 32, `the embedded key ${head.signature.keyId}`);
      if (!verify(null, canonicalBytes(head), publicKeyFromRaw(key), signature)) throw new Error(`the head's signature by ${head.signature.keyId} does not verify`);
      authenticated = true;
    }
    return { usable: true, authenticated, head, seq: head.stream.seq, problem: null };
  } catch (error) {
    return { usable: false, authenticated: false, head: found?.json ?? null, seq: null, problem: error.message || String(error) };
  }
}

/** The largest sequence authenticated inside an immutable release record.
 * The record wrapper is audit metadata; only its embedded signed envelope is
 * authority for a client's rollback floor. Malformed, foreign, and unsigned
 * records do not contribute a number. */
export async function authenticatedReleaseFloor(origin, app, stream) {
  // Hidden names were accepted by older publishers. Include them while
  // recovering the rollback floor even though new release ids cannot begin
  // with a dot.
  const names = await origin.list(`${streamPath(stream)}/releases`, { includeHidden: true });
  // A missing directory on the writable filesystem origin is authoritative
  // emptiness. Other adapters use null when they cannot enumerate history;
  // that is not evidence that no client has observed a higher sequence.
  if (names === null && origin.kind !== 'directory') return { known: false, empty: false, floor: null };
  if (names === null || names.length === 0) return { known: true, empty: true, floor: null };
  let floor = null;
  const kept = keptRecords(origin, app, stream), added = {};
  for (const name of names.filter((entry) => entry.endsWith('.json'))) {
    if (Object.hasOwn(kept.seqs, name)) { floor = Math.max(floor ?? 0, kept.seqs[name]); continue; }
    const bytes = await origin.get(`${streamPath(stream)}/releases/${name}`);
    if (!bytes) throw new OriginUnavailable(`the listed release record ${origin.describe()}/${streamPath(stream)}/releases/${name} disappeared while establishing the authenticated sequence floor`);
    const record = authenticatedRecord(bytes, app, stream);
    if (record) floor = Math.max(floor ?? 0, added[name] = record.envelope.stream.seq);
  }
  kept.keep(added);
  return { known: true, empty: false, floor };
}

/** A release record whose embedded envelope authenticates for `stream`, else null: the wrapper is audit metadata, and
 *  an unauthenticated record has no say in the floor nor any capability authority. */
function authenticatedRecord(bytes, app, stream) {
  try {
    const record = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    if (!record.envelope || typeof record.envelope !== 'object' || Array.isArray(record.envelope)) return null;
    const recordApp = { ...app, displayName: record.envelope.app?.name };
    const envelopeBytes = Buffer.from(JSON.stringify(record.envelope), 'utf8');
    const admission = inspectHead({ bytes: envelopeBytes, json: record.envelope, sha256: sha256(envelopeBytes) }, recordApp, stream);
    return admission.usable && admission.authenticated ? record : null;
  } catch { return null; }
}

/** Keep authenticated AppDrop sequences for the app and keys that verified
 * them. Release records are immutable. Only names still listed count toward
 * the floor, and records that could not be authenticated are read again. */
function keptRecords(origin, app, stream) {
  if (origin.kind !== 'appdrop') return { seqs: {}, keep() {} };
  const cache = process.env.XDG_CACHE_HOME?.startsWith('/') ? process.env.XDG_CACHE_HOME : resolve(homedir(), '.cache');
  const scope = { origin: origin.describe(), stream: streamPath(stream), app: app.id, keys: app.manifest.deploy?.signing?.keys ?? {} };
  const file = resolve(cache, 'exact/release-seqs', `${sha256(canonicalBytes(scope))}.json`);
  const seqs = {};
  try {
    for (const [name, seq] of Object.entries(JSON.parse(readFileSync(file, 'utf8')))) if (Number.isSafeInteger(seq) && seq >= 0) seqs[name] = seq;
  } catch { /* none kept, or unreadable: every record is read */ }
  return { seqs, keep(added) {
    if (!Object.keys(added).length) return;
    try {
      mkdirSync(dirname(file), { recursive: true });
      const temporary = `${file}.${process.pid}`;
      writeFileSync(temporary, JSON.stringify({ ...seqs, ...added }));
      renameSync(temporary, file);
    } catch { /* the next publication reads these records again */ }
  } };
}

/** Keep the record a publication wrote as authenticated (`keptRecords`): it holds the seq its own signed envelope names. */
export const keepRecord = (origin, app, stream, name, seq) => keptRecords(origin, app, stream).keep({ [name]: seq });

/** Allocate above the verified head and signed history. A missing payload
 * can make a signed head unusable while its sequence still binds clients. */
export async function nextSeq(origin, app, stream, admission, at, above = 0, { startingCohort = false } = {}) {
  return Math.max(above + 1, await nextOriginSeq(origin, app, stream, admission, at, startingCohort));
}

async function nextOriginSeq(origin, app, stream, admission, at, startingCohort) {
  const history = await authenticatedReleaseFloor(origin, app, stream);
  if (startingCohort && history.floor !== null) refuse(`${at} has authenticated release history but no authenticated capability receipt for this publication; run deploy to recover it`);
  let floor;
  if (admission?.seq != null) floor = Math.max(admission.seq, history.floor ?? 0);
  else {
    if (!history.known) throw new OriginUnavailable(`${at} ${admission ? `is unusable (${admission.problem})` : 'is missing'}, but ${origin.describe()} cannot enumerate its authenticated release history; a rollback-safe sequence cannot be allocated`);
    floor = history.floor;
    if (floor === null && !admission && history.empty) return 1;
    if (floor === null) refuse(`${at} ${admission ? `is unusable (${admission.problem}) and its` : 'is missing, and its nonempty'} release history has no authenticated sequence floor; restore a signed release record before repairing it`);
  }
  if (floor === Number.MAX_SAFE_INTEGER) refuse(`${at} is at the largest exact JavaScript seq; a higher repair seq cannot be allocated safely`);
  return floor + 1;
}

/** The latest signed release record and a verified cohort from its history.
 * A signed head's receipt card fixes which payload can be recovered. */
export async function latestRecord(origin, app, stream, admission = null) {
  const names = await origin.list(`${streamPath(stream)}/releases`, { includeHidden: true });
  if (!names?.length) return null;
  // Each record not kept is read; then the one with the largest seq, unless it was among them (`keptRecords`).
  const kept = keptRecords(origin, app, stream), added = {}, read = new Map(), seqs = {};
  const recordAt = async (name) => {
    const bytes = await origin.get(`${streamPath(stream)}/releases/${name}`);
    return bytes ? authenticatedRecord(bytes, app, stream) : null;
  };
  for (const name of names.filter((n) => n.endsWith('.json'))) {
    if (Object.hasOwn(kept.seqs, name)) { seqs[name] = kept.seqs[name]; continue; }
    const record = await recordAt(name);
    if (record) { read.set(name, record); seqs[name] = added[name] = record.envelope.stream.seq; }
  }
  kept.keep(added);
  const files = headFiles(origin, app, stream);
  let latest = null;
  let required = admission?.authenticated ? admission.head.cohortReceipt : null;
  let cohortProblem = required ? admission.problem : null;
  const failed = new Set();
  for (const name of Object.keys(seqs).sort((a, b) => seqs[b] - seqs[a])) {
    const record = read.get(name) ?? await recordAt(name);
    if (!record) continue;
    latest ??= record;
    const card = record.envelope.cohortReceipt;
    if (card && required && !sameCohortPayload(card, required)) continue;
    const key = card ? JSON.stringify([card.url, card.sha256, card.bytes, card.chunked]) : null;
    if (failed.has(key)) continue;
    try {
      const cohort = await readHeadCohort(files, record.envelope, stream);
      if (!required || (cohort && sameCohortPayload(required, card ?? cohortFiles(cohort)[0].card))) {
        return { ...latest, cohort, cohortCard: card };
      }
    }
    catch (error) {
      if (error instanceof OriginUnavailable) throw error;
      failed.add(key);
      required ??= card;
      cohortProblem ??= error.message || String(error);
    }
  }
  return latest ? { ...latest, cohort: null, cohortCard: required, cohortProblem } : null;
}

function matchingBuildCohort(card, build, stream) {
  if (!card || build?.trust !== 'production' || build.compat.id !== stream.compatibilityId) return null;
  const cohort = cohortReceipt(build);
  return sameCohortPayload(card, cohortFiles(cohort)[0].card) ? cohort : null;
}

/** Recover a frozen cohort from the signed head, a matching production
 * receipt, or signed history. Keep any verified card and its original URL. */
export async function recoverCohort(origin, app, stream, admission, build) {
  const card = admission?.authenticated ? admission.head.cohortReceipt : null;
  if (admission?.authenticated && admission.cohort) return { cohort: admission.cohort, cohortCard: card };
  const fromHead = matchingBuildCohort(card, build, stream);
  if (fromHead) return { cohort: fromHead };
  const record = await latestRecord(origin, app, stream, admission);
  if (record?.cohort) return record;
  const fromHistory = matchingBuildCohort(record?.cohortCard, build, stream);
  return fromHistory ? { ...record, cohort: fromHistory, cohortCard: null } : record;
}
