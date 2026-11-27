// The next seq of a long-lived AppDrop stream (LLP 1030.000 D3 item 5): above every release record it has
// authenticated, reading over the network only the records this machine has not authenticated before.
import { afterAll, expect, test } from 'bun:test';
import { generateKeyPairSync, sign } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { admitHead, authenticatedReleaseFloor, binaryInputFile, cohortFiles, cohortReceipt, keepRecord, latestRecord, nextSeq, sameCohortPayload } from './deploy-heads.mjs';
import { binarySourceInputs } from './cargo-dep-info.mjs';
import { canonicalBytes, classify } from './deploy.mjs';
import { envelopeBytes } from './deploy-signing.mjs';
import { publishStream, readBinaryInputFile, streamHead } from './publisher.mjs';
import { blobPath, DirectoryOrigin, HttpsOrigin, sha256, streamPath } from './origin.mjs';

const cache = mkdtempSync(join(tmpdir(), 'exact-release-seqs-'));
const xdg = process.env.XDG_CACHE_HOME;
process.env.XDG_CACHE_HOME = cache;
afterAll(() => {
  if (xdg === undefined) delete process.env.XDG_CACHE_HOME; else process.env.XDG_CACHE_HOME = xdg;
  rmSync(cache, { recursive: true, force: true });
});

const stream = { channel: 'prod', compatibilityId: 'c'.repeat(32) };
const { privateKey, publicKey } = generateKeyPairSync('ed25519');
const app = { id: 'com.exact.longlived', displayName: 'Long Lived',
  manifest: { deploy: { signing: { key: 'test', keys: { test: publicKey.export({ type: 'spki', format: 'der' }).subarray(-32).toString('base64') } } } } };
const plan = Buffer.from('plan');
const bundle = { plan: { bytes: plan, sha256: sha256(plan), formatVersion: 4, kernelSchema: '0'.repeat(16) }, assets: [] };
const record = (seq, fields = {}) => {
  const envelope = streamHead({ app, bundle, stream, seq, release: `r${seq}` });
  Object.assign(envelope, fields);
  envelope.signature = { keyId: 'test', ed25519: sign(null, canonicalBytes(envelope), privateKey).toString('base64') };
  return Buffer.from(JSON.stringify({ release: `r${seq}`, envelope }));
};

/** An origin of `kind` holding `records` by name, counting its reads. */
function origin(kind, records) {
  const o = { kind, gets: 0, describe: () => `https://appdrop.test/${kind}`,
    list: async () => Object.keys(records),
    get: async (path) => { o.gets++; return records[path.split('/').at(-1)] ?? null; } };
  return o;
}

test('an AppDrop stream reads each release record once, and still allocates above a rolled-back or unusable head', async () => {
  const records = Object.fromEntries(Array.from({ length: 100 }, (_, i) => [`r${i + 1}.json`, record(i + 1)]));
  const appdrop = origin('appdrop', records), head = (seq) => ({ usable: true, seq });
  expect(await nextSeq(appdrop, app, stream, head(100), 'the head')).toBe(101);
  expect(appdrop.gets).toBe(100);
  appdrop.gets = 0;
  expect(await nextSeq(appdrop, app, stream, head(100), 'the head')).toBe(101);
  expect(appdrop.gets).toBe(0);
  // Another publisher's release is read; the head rolled back to an earlier one, unusable or missing allocates above it.
  records['r101.json'] = record(101);
  expect(await nextSeq(appdrop, app, stream, head(5), 'the head')).toBe(102);
  expect(await nextSeq(appdrop, app, stream, { usable: false, problem: 'corrupt' }, 'the head')).toBe(102);
  expect(await nextSeq(appdrop, app, stream, null, 'the head')).toBe(102);
  expect(appdrop.gets).toBe(1);
  // One this machine published is kept as it is written; a record kept that the origin no longer lists counts no more.
  records['r102.json'] = record(102);
  keepRecord(appdrop, app, stream, 'r102.json', 102);
  delete records['r101.json'];
  appdrop.gets = 0;
  expect(await nextSeq(appdrop, app, stream, head(100), 'the head')).toBe(103);
  delete records['r102.json'];
  expect(await nextSeq(appdrop, app, stream, head(100), 'the head')).toBe(101);
  expect(appdrop.gets).toBe(0);
  // The latest record is the one with the largest seq, read alone.
  expect((await latestRecord(appdrop, app, stream)).envelope.stream.seq).toBe(100);
  expect(appdrop.gets).toBe(1);
});

test('a record that does not authenticate is read again, and a directory origin keeps nothing', async () => {
  const records = { 'r1.json': record(1), 'bad.json': Buffer.from('{"envelope":{}}') };
  const appdrop = origin('appdrop', records);
  appdrop.describe = () => 'https://appdrop.test/unauthenticated';
  for (let i = 0; i < 2; i++) expect(await nextSeq(appdrop, app, stream, null, 'the head')).toBe(2);
  expect(appdrop.gets).toBe(3);
  const directory = origin('directory', records);
  for (let i = 0; i < 2; i++) expect(await nextSeq(directory, app, stream, null, 'the head')).toBe(2);
  expect(directory.gets).toBe(4);
});

test('AppDrop release sequences are cached only for the app and keys that authenticated them', async () => {
  const appdrop = origin('appdrop', { 'initial.json': record(7) });
  appdrop.describe = () => 'https://appdrop.test/shared';
  const variant = { ...app, id: `${app.id}.beta`, displayName: 'Beta' };
  const differentKeys = { ...app, manifest: { deploy: { signing: { keys: { test: Buffer.alloc(32).toString('base64') } } } } };
  expect((await authenticatedReleaseFloor(appdrop, app, stream)).floor).toBe(7);
  for (const selected of [variant, differentKeys]) {
    expect((await authenticatedReleaseFloor(appdrop, selected, stream)).floor).toBeNull();
  }
  expect((await authenticatedReleaseFloor(appdrop, app, stream)).floor).toBe(7);
  expect(appdrop.gets).toBe(3);
});

test('a signed head with a missing receipt still raises the rollback floor', async () => {
  const records = { 'r5.json': record(5) };
  const directory = origin('directory', records);
  const json = JSON.parse(record(10, { cohortReceipt: { url: './receipt', sha256: '0'.repeat(64), bytes: 1 } })).envelope;
  const admission = await admitHead(directory, { bytes: Buffer.from(JSON.stringify(json)), json }, app, stream);
  expect(admission.usable).toBe(false);
  expect(await nextSeq(directory, app, stream, admission, 'the head')).toBe(11);
});

test('deploy repairs a missing asset index with the signed cohort receipt bytes', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-receipt-bytes-'));
  try {
    const key = app.manifest.deploy.signing.keys.test;
    const signer = { keyId: 'test', sign: head => ({ keyId: 'test', ed25519: sign(null, canonicalBytes(head), privateKey).toString('base64') }) };
    const stream = { channel: 'prod', compatibilityId: 'a'.repeat(32) };
    const asset = Buffer.from('asset');
    const update = { ...bundle, assets: [{ name: 'assets/fixture.txt', sha256: sha256(asset), bytes: asset }] };
    const build = { version: 1, compat: { id: stream.compatibilityId, target: 'fixture',
      inputs: { platform: 'ios', store: { L: 'A' }, keys: { test: key } } },
      graph: { version: 1, sources: {}, artifacts: [] }, binary: { sha256: '0'.repeat(64) } };
    const cohort = cohortReceipt(build);
    const receiptBytes = Buffer.from(JSON.stringify(cohort, null, 2));
    const digest = sha256(receiptBytes);
    const escaped = `%${digest.charCodeAt(0).toString(16)}${digest.slice(1)}`;
    const card = { url: `../../blobs/${escaped}`, sha256: digest, bytes: receiptBytes.length };
    const origin = new DirectoryOrigin(dir);
    const head = { ...streamHead({ app, bundle: update, stream, seq: 7, release: 'initial' }), cohortReceipt: card };
    head.signature = signer.sign(head);
    await origin.put(blobPath(digest), receiptBytes, { immutable: true });
    await origin.put(`${streamPath(stream)}/releases/initial.json`, Buffer.from(JSON.stringify({ envelope: head })), { immutable: true });
    await origin.putHead(stream, envelopeBytes(head), { previousDigest: null });
    const admission = await admitHead(origin, await origin.head(stream), app, stream);
    expect(admission).toMatchObject({ usable: false, authenticated: true });
    expect(await latestRecord(origin, app, stream, admission)).toMatchObject({ cohort });
    const table = await classify({ app, origin, opts: {}, channel: stream.channel, snapshot: {}, release: 'repair',
      bundle: update, compat: { ios: build.compat }, builds: { ios: build }, platforms: ['ios'], wantOrigin: false });
    expect(table.rows[0]).toMatchObject({ action: 'bundle', seq: 8 });
    expect(await publishStream({ origin, app, signer, row: table.rows[0], release: 'repair',
      bundle: update, build, compat: build.compat, snapshot: {}, opts: {}, log() {} })).toMatchObject({ action: 'published', seq: 8 });
    const repaired = await origin.head(stream);
    expect(repaired.json.cohortReceipt).toEqual(card);
    expect(await origin.get(blobPath(digest))).toEqual(receiptBytes);
    expect(await admitHead(origin, repaired, app, stream)).toMatchObject({ usable: true, cohort });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each(['.0', 'e0', 'BOM'])('a signed cohort manifest refuses invalid encodings (%s)', async variant => {
  const receipt = canonicalBytes({
    version: 1, compat: { id: stream.compatibilityId, inputs: {} }, sources: {}, binary: '0'.repeat(64),
  });
  const chunk = { url: './chunk', sha256: sha256(receipt), bytes: receipt.length };
  const jsonManifest = JSON.stringify({ chunks: [chunk] });
  const manifest = Buffer.from(variant === 'BOM' ? '\uFEFF' + jsonManifest : jsonManifest.replace(
    `"bytes":${receipt.length}`, `"bytes":${receipt.length}${variant}`,
  ));
  const directory = origin('directory', { manifest, chunk: receipt });
  const { envelope: json } = JSON.parse(record(10, {
    cohortReceipt: { url: './manifest', sha256: sha256(manifest), bytes: manifest.length, chunked: true },
  }));
  const admission = await admitHead(directory, { bytes: Buffer.from(JSON.stringify(json)), json }, app, stream);
  expect(admission).toMatchObject({ authenticated: true, usable: false, seq: 10 });
  expect(admission.problem).toContain(variant === 'BOM' ? 'the cohort receipt manifest is not JSON'
    : `the cohort receipt manifest carries the non-canonical integer ${receipt.length}${variant}`);
  expect(directory.gets).toBe(1);
});

test('cohort cards resolve on the head origin and preserve their query', async () => {
  const receipt = Buffer.from(JSON.stringify({
    version: 1, compat: { id: stream.compatibilityId, inputs: {} }, sources: {}, binary: '0'.repeat(64),
  }));
  const suffix = `/${streamPath(stream)}/exact.json`;
  let cardUrl;
  const requests = [];
  const server = Bun.serve({ port: 0, fetch(request) {
    const { pathname, search } = new URL(request.url);
    if (pathname === `/updates${suffix}`) {
      const { envelope } = JSON.parse(record(10, {
        cohortReceipt: { url: cardUrl, sha256: sha256(receipt), bytes: receipt.length },
      }));
      return Response.json(envelope);
    }
    requests.push(pathname + search);
    return pathname === '/receipts/current' && search === '?v=1'
      ? new Response(receipt) : new Response(null, { status: 404 });
  } });
  try {
    const base = `http://127.0.0.1:${server.port}`;
    const remote = new HttpsOrigin(`${base}/updates`);
    for (const url of [`${base}/receipts/current?v=1#ignored`, '../../../../receipts/current?v=1',
      `http://127.1:${server.port}/receipts/current?v=1`]) {
      cardUrl = url;
      expect(await admitHead(remote, await remote.head(stream), app, stream))
        .toMatchObject({ usable: true, cohort: JSON.parse(receipt) });
    }
    cardUrl = 'https://another.example/receipts/current?v=1';
    expect(await admitHead(remote, await remote.head(stream), app, stream))
      .toMatchObject({ usable: false, problem: expect.stringContaining('stream origin') });
    expect(requests).toEqual(Array(3).fill('/receipts/current?v=1'));
  } finally { server.stop(true); }
});

test('a signed cohort card can spell the same IPv6 address differently from the head URL', async () => {
  const cohort = { version: 1, compat: { id: stream.compatibilityId, inputs: {} }, sources: {}, binary: '0'.repeat(64) };
  const receipt = canonicalBytes(cohort);
  const remote = { ...origin('https', {}), url: 'https://[::1]',
    getURL: async url => url === 'https://[::1]/receipt' ? receipt : null };
  const { envelope } = JSON.parse(record(7, {
    cohortReceipt: { url: 'https://[0:0:0:0:0:0:0:1]:443/receipt', bytes: receipt.length, sha256: sha256(receipt) },
  }));
  const bytes = Buffer.from(JSON.stringify(envelope));
  expect(await admitHead(remote, { bytes, json: envelope }, app, stream)).toMatchObject({ usable: true, cohort });
});

test('binarySourceInputs produces identical binary-inputs and cohort digests regardless of Cargo build-script arrival order', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-binary-source-order-'));
  try {
    const roots = [{ name: 'exact', path: dir }];
    const kernelDir = join(dir, 'kernel/build');
    const jsDir = join(dir, 'vendor/ibex/crates/ibex2/include');
    const fileA = { path: join(dir, 'kernel/src/lib.rs'), sha256: 'a'.repeat(64) };
    const fileB = { path: join(dir, 'js/src/lib.rs'), sha256: 'b'.repeat(64) };
    const dirKernel = [kernelDir, { path: kernelDir, names: ['tables.rs'] }];
    const dirJs = [jsDir, { path: jsDir, names: ['ibex.h'] }];
    const absentKernel = [join(dir, 'kernel/optional'), join(dir, 'kernel/optional')];
    const absentJs = [join(dir, 'js/optional'), join(dir, 'js/optional')];
    const linkKernel = [join(dir, 'kernel/link'), { path: join(dir, 'kernel/link'), target: 'a' }];
    const linkJs = [join(dir, 'js/link'), { path: join(dir, 'js/link'), target: 'b' }];
    const invA = { path: join(dir, 'modules/a'), kind: 'swift-package', identity: { directory: 'a' } };
    const invB = { path: join(dir, 'modules/b'), kind: 'notification-service', identity: { tree: '1'.repeat(64) } };
    const metadata = { delivery: { channel: 'prod' }, store: { L: 'A' }, keys: {} };

    const makeBuild = reverse => {
      const pair = (first, second) => reverse ? [second, first] : [first, second];
      const sourceInputs = binarySourceInputs({
        files: pair(fileA, fileB),
        directories: new Map(pair(dirKernel, dirJs)),
        absent: new Map(pair(absentKernel, absentJs)),
        links: new Map(pair(linkKernel, linkJs)),
        inventories: pair(invA, invB),
        metadata,
      }, roots, []);
      return {
        version: 1,
        compat: { id: stream.compatibilityId, target: 'aarch64-apple-ios', inputs: { platform: 'ios' } },
        graph: { version: 1, sources: {}, artifacts: [] },
        binary: { sha256: 'c'.repeat(64), sourceInputs },
      };
    };

    const first = makeBuild(false);
    const second = makeBuild(true);
    const firstFile = binaryInputFile(first);
    const secondFile = binaryInputFile(second);
    expect(secondFile.bytes).toEqual(firstFile.bytes);
    expect(secondFile.sha256).toBe(firstFile.sha256);

    const firstCohort = cohortReceipt(first);
    const secondCohort = cohortReceipt(second);
    expect(secondCohort).toEqual(firstCohort);
    expect(sameCohortPayload(cohortFiles(firstCohort)[0].card, cohortFiles(secondCohort)[0].card)).toBe(true);

    // Local build with reversed arrival order still restores a missing origin binary-inputs blob.
    const emptyOrigin = origin('directory', {});
    const restored = await readBinaryInputFile(emptyOrigin, firstCohort, second);
    expect(restored.bytes).toEqual(firstFile.bytes);
    expect(restored.restore.sha256).toBe(firstFile.sha256);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
