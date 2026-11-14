// The next seq of a long-lived AppDrop stream (LLP 1030.000 D3 item 5): above every release record it has
// authenticated, reading over the network only the records this machine has not authenticated before.
import { afterAll, expect, test } from 'bun:test';
import { generateKeyPairSync, sign } from 'node:crypto';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { keepRecord, latestRecord, nextSeq } from './deploy-heads.mjs';
import { canonicalBytes, streamHead } from './deploy.mjs';
import { sha256 } from './origin.mjs';

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
const record = (seq) => {
  const envelope = streamHead({ app, bundle, stream, seq, release: `r${seq}` });
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
  keepRecord(appdrop, stream, 'r102.json', 102);
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
