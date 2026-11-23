// The object-store origin the tools publisher writes (LLP 1174 D4.2), against an S3 stand-in that checks each
// request's SigV4 signature itself, and the pruning pass on it and on a directory origin.
import { afterAll, expect, test } from 'bun:test';
import { createHash, createHmac, generateKeyPairSync } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { lockClosure, digest, workspaceManifest } from './kept.mjs';
import { DirectoryOrigin, HttpsOrigin, ObjectStoreOrigin, blobPath, openOrigin, signS3 } from './origin.mjs';
import { ROOT, fetchToolSet, fromSets, hostTriple, pruneOrigin, publishToolSet, withPublication } from './tools.mjs';

const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const hmac = (key, text) => createHmac('sha256', key).update(text).digest();
const CREDENTIALS = { EXACT_S3_ACCESS_KEY_ID: 'stand-in-key', EXACT_S3_SECRET_ACCESS_KEY: 'stand-in/secret+value' };

// Percent-encodes every byte but RFC 3986's unreserved characters, as SigV4 specifies, written apart from the adapter's.
const encode = (text) => [...Buffer.from(text)].map((b) => /[A-Za-z0-9\-._~]/.test(String.fromCharCode(b)) ? String.fromCharCode(b) : `%${b.toString(16).toUpperCase().padStart(2, '0')}`).join('');
const xml = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;');

/** An S3-compatible bucket: signed GET, PUT (If-Match, If-None-Match), DELETE and ListObjectsV2 under `/<bucket>`, and
 * unsigned public reads under `/public/`, as a public R2 bucket serves them. A list page holds two keys. */
function standIn(bucket = 'tools') {
  const objects = new Map(), requests = [];
  let writes = 0;
  const refuse = (status, code) => new Response(`<Error><Code>${code}</Code></Error>`, { status });
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    const url = new URL(request.url), body = Buffer.from(await request.arrayBuffer());
    requests.push({ method: request.method, path: url.pathname, signed: request.headers.has('authorization') });
    if (url.pathname.startsWith('/public/')) {
      if (request.method !== 'GET') return refuse(405, 'MethodNotAllowed');
      const object = objects.get(decodeURIComponent(url.pathname.slice('/public/'.length)));
      return object ? new Response(object.body) : refuse(404, 'NoSuchKey');
    }
    const auth = /^AWS4-HMAC-SHA256 Credential=([^/]+)\/(\d{8})\/(\w+)\/s3\/aws4_request, SignedHeaders=([a-z0-9;-]+), Signature=([0-9a-f]{64})$/.exec(request.headers.get('authorization') ?? '');
    if (!auth || auth[1] !== CREDENTIALS.EXACT_S3_ACCESS_KEY_ID || auth[3] !== 'auto') return refuse(403, 'InvalidAccessKeyId');
    const [, , day, region, signedList, signature] = auth, signed = signedList.split(';');
    const date = request.headers.get('x-amz-date') ?? '';
    const required = ['host', 'x-amz-date', 'x-amz-content-sha256', ...[...request.headers.keys()].filter((name) => name.startsWith('if-'))];
    if (!date.startsWith(day) || required.some((name) => !signed.includes(name))) return refuse(403, 'AccessDenied');
    if (request.headers.get('x-amz-content-sha256') !== sha(body)) return refuse(400, 'XAmzContentSHA256Mismatch');
    const query = [...url.searchParams].map(([k, v]) => `${encode(k)}=${encode(v)}`).sort().join('&');
    const canonical = [request.method, url.pathname, query, ...signed.map((name) => `${name}:${request.headers.get(name).trim()}`), '', signedList, sha(body)].join('\n');
    const key = [day, region, 's3', 'aws4_request'].reduce(hmac, `AWS4${CREDENTIALS.EXACT_S3_SECRET_ACCESS_KEY}`);
    if (hmac(key, ['AWS4-HMAC-SHA256', date, `${day}/${region}/s3/aws4_request`, sha(canonical)].join('\n')).toString('hex') !== signature) return refuse(403, 'SignatureDoesNotMatch');

    if (url.pathname === `/${bucket}` && request.method === 'GET' && url.searchParams.get('list-type') === '2') {
      const prefix = url.searchParams.get('prefix') ?? '', after = url.searchParams.get('continuation-token');
      const keys = [...objects.keys()].filter((k) => k.startsWith(prefix) && (!after || k > Buffer.from(after, 'base64').toString())).sort();
      const page = keys.slice(0, 2), more = keys.length > page.length;
      return new Response(`<?xml version="1.0" encoding="UTF-8"?><ListBucketResult><IsTruncated>${more}</IsTruncated>${page.map((k) =>
        `<Contents><Key>${xml(k)}</Key><LastModified>${new Date(objects.get(k).modified).toISOString()}</LastModified><ETag>${xml(objects.get(k).etag)}</ETag></Contents>`).join('')}${
        more ? `<NextContinuationToken>${Buffer.from(page.at(-1)).toString('base64')}</NextContinuationToken>` : ''}</ListBucketResult>`);
    }
    if (!url.pathname.startsWith(`/${bucket}/`)) return refuse(404, 'NoSuchBucket');
    const name = url.pathname.slice(bucket.length + 2).split('/').map(decodeURIComponent).join('/'), object = objects.get(name);
    if (request.method === 'GET') return object ? new Response(object.body, { headers: { etag: object.etag } }) : refuse(404, 'NoSuchKey');
    if (request.method === 'DELETE') { objects.delete(name); return new Response(null, { status: 204 }); }
    if (request.method !== 'PUT') return refuse(405, 'MethodNotAllowed');
    // R2 allows one write a second to a key.
    if (name.endsWith('/busy') && !objects.has(`${name}.throttled`)) { objects.set(`${name}.throttled`, { body: Buffer.alloc(0), modified: 0 }); return refuse(429, 'TooManyRequests'); }
    const match = request.headers.get('if-match'), noneMatch = request.headers.get('if-none-match');
    if ((noneMatch === '*' && object) || (match !== null && object?.etag !== match)) return refuse(412, 'PreconditionFailed');
    const etag = `"${sha(body).slice(0, 32)}-${++writes}"`;
    objects.set(name, { body, etag, modified: Date.now(), cacheControl: request.headers.get('cache-control') });
    return new Response(null, { headers: { etag } });
  } });
  return { objects, requests, server, url: `http://127.0.0.1:${server.port}` };
}

const store = standIn();
afterAll(() => store.server.stop(true));
const bucketOrigin = (env = CREDENTIALS, prefix = '') => new ObjectStoreOrigin(`s3+${store.url}/tools${prefix && `/${prefix}`}`, env);

test('a request is signed as AWS documents for GET Object', () => {
  // https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-header-based-auth.html, "Example: GET Object".
  const headers = signS3({ method: 'GET', host: 'examplebucket.s3.amazonaws.com', path: '/test.txt', headers: { Range: 'bytes=0-9' },
    accessKeyId: 'AKIAIOSFODNN7EXAMPLE', secretAccessKey: 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY', region: 'us-east-1', date: new Date('2013-05-24T00:00:00Z') });
  expect(headers.authorization).toBe('AWS4-HMAC-SHA256 Credential=AKIAIOSFODNN7EXAMPLE/20130524/us-east-1/s3/aws4_request, '
    + 'SignedHeaders=host;range;x-amz-content-sha256;x-amz-date, Signature=f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41');
});

test('the spelling names an endpoint and a bucket, never a secret, and credentials come from the environment', async () => {
  expect(() => new ObjectStoreOrigin('s3+https://key:hunter2@example.r2.cloudflarestorage.com/tools')).toThrow('EXACT_S3_ACCESS_KEY_ID');
  expect(() => new ObjectStoreOrigin('s3+https://key:hunter2@example.r2.cloudflarestorage.com/tools')).not.toThrow('hunter2');
  expect(() => new ObjectStoreOrigin('s3+http://example.r2.cloudflarestorage.com/tools')).toThrow('over https');
  expect(new ObjectStoreOrigin('s3+https://Example.r2.cloudflarestorage.com:443/tools/').describe()).toBe('s3+https://example.r2.cloudflarestorage.com/tools');
  expect(() => new ObjectStoreOrigin('s3+https://example.r2.cloudflarestorage.com')).toThrow('name the bucket');
  expect(() => openOrigin('s3+https://example.r2.cloudflarestorage.com/tools')).toThrow('holds tool sets');
  await expect(bucketOrigin({}).get('a')).rejects.toThrow('set EXACT_S3_ACCESS_KEY_ID and EXACT_S3_SECRET_ACCESS_KEY');
  const wrong = bucketOrigin({ ...CREDENTIALS, EXACT_S3_SECRET_ACCESS_KEY: 'not the secret' });
  const failure = await wrong.put('a', Buffer.from('x')).catch((error) => error);
  expect(failure.message).toBe(`PUT s3+${store.url}/tools/a: HTTP 403 SignatureDoesNotMatch`);
  expect(failure.message).not.toContain('not the secret');
});

test('an object store gets, puts immutable files once, replaces only the version read, deletes and lists by page', async () => {
  const origin = bucketOrigin(CREDENTIALS, 'under a prefix');
  expect(await origin.get('.exact/none')).toBe(null);
  const blob = Buffer.from('a blob');
  expect(await origin.put(blobPath(sha(blob)), blob, { immutable: true })).toBe('written');
  expect(await origin.put(blobPath(sha(blob)), Buffer.from('other bytes'), { immutable: true })).toBe('present');
  expect(await origin.get(blobPath(sha(blob)))).toEqual(blob);
  expect(store.objects.get(`under a prefix/${blobPath(sha(blob))}`).cacheControl).toBe('public, max-age=31536000, immutable');

  const recent = '.exact/tools/t/recent.json';
  expect(await origin.replace(recent, Buffer.from('[1]'), null)).toBe(true);
  expect(await origin.replace(recent, Buffer.from('[2]'), null)).toBe(false);
  const read = await origin.getVersion(recent);
  // Two publishers that read the same version: one replaces it, the other learns it lost.
  const race = await Promise.all([origin.replace(recent, Buffer.from('[3]'), read.version), origin.replace(recent, Buffer.from('[4]'), read.version)]);
  expect(race.sort()).toEqual([false, true]);
  expect(['[3]', '[4]']).toContain((await origin.get(recent)).toString());
  expect(await origin.replace(recent, Buffer.from('[5]'), read.version)).toBe(false);
  expect(store.objects.get(`under a prefix/${recent}`).cacheControl).toBe('no-cache');

  expect(await origin.put('.exact/busy', Buffer.from('later'))).toBe('written');
  expect((await origin.get('.exact/busy')).toString()).toBe('later');

  for (const name of ['a', 'b', 'c/d', 'c/e']) await origin.put(`.exact/list/${name}`, Buffer.from(name));
  expect((await origin.files('.exact/list')).map(({ path }) => path)).toEqual(['.exact/list/a', '.exact/list/b', '.exact/list/c/d', '.exact/list/c/e']);
  expect((await origin.files('.exact/list'))[0]).toMatchObject({ modified: expect.any(Number), version: expect.stringMatching(/^"/) });
  await origin.delete('.exact/list/b');
  await origin.delete('.exact/list/b');
  expect((await origin.files('.exact/list')).length).toBe(3);
  expect(store.requests.every(({ path, signed }) => signed && /^\/tools(\/|$)/.test(path))).toBe(true);
});

test('a set published to the object store is fetched from its public URL into an empty cache, and racing publishers both land', async () => {
  const home = mkdtempSync(resolve(tmpdir(), 'exact-tools-s3-'));
  try {
    const origin = bucketOrigin(CREDENTIALS, 'fetch'), { publicKey, privateKey } = generateKeyPairSync('ed25519');
    const triple = hostTriple(), base = `.exact/tools/${triple}`;
    const head = Bun.spawnSync(['git', '-C', ROOT, 'rev-parse', 'HEAD']).stdout.toString().trim();
    const inputs = ['rust-toolchain.toml', 'contract/syntax/src'].map((file) => [file, digest(resolve(ROOT, file))]);
    const tool = Buffer.from('contract executable');
    writeFileSync(resolve(home, 'contract'), tool);
    const product = { file: 'contract', inputs, crate: 'contract', closure: lockClosure(ROOT, 'contract'),
      workspace: sha(JSON.stringify(workspaceManifest(ROOT))), size: tool.length, sha256: sha(tool) };
    const set = (commit) => ({ dir: home, manifest: { version: 1, triple, source: 'built', commit, products: { contract: product } } });
    const [first, second] = await Promise.all([publishToolSet(origin, set('e'.repeat(40)), privateKey), publishToolSet(origin, set('f'.repeat(40)), privateKey)]);
    expect(JSON.parse(await origin.get(`${base}/recent.json`)).sort()).toEqual([first, second].sort());
    expect(JSON.parse(await origin.get(`${base}/commits/${'e'.repeat(40)}.json`))).toBe(first);
    expect(JSON.parse(await origin.get(`${base}/commits/${'f'.repeat(40)}.json`))).toBe(second);
    const id = await publishToolSet(origin, set(head), privateKey);

    const env = { HOME: home, PATH: process.env.PATH, EXACT_TOOLS_ORIGIN: `${store.url}/fetch` };
    store.requests.length = 0;
    expect(await fetchToolSet(new HttpsOrigin(`${store.url}/public/fetch`), 'contract', [publicKey], env)).toBe(true);
    expect(store.requests.some(({ signed }) => signed)).toBe(false);
    expect(store.requests.map(({ path }) => path)).toContain(`/public/fetch/${base}/manifests/${id}.json`);
    expect(readFileSync(fromSets('contract', env).path)).toEqual(tool);
  } finally { rmSync(home, { recursive: true, force: true }); }
}, 60000);

const DAY = 86_400_000;

/** The same pass on a directory and on the object store: files written, then each given its age. */
for (const [kind, open, age] of [
  ['a directory', (dir) => new DirectoryOrigin(dir), (origin, rel, time) => utimesSync(origin.path(rel), time / 1000, time / 1000)],
  ['an object store', (dir) => bucketOrigin(CREDENTIALS, `prune/${dir.split('/').at(-1)}`), (origin, rel, time) => { store.objects.get(`${origin.prefix}${rel}`).modified = time; }],
]) {
  test(`pruning ${kind} deletes what two passes a day apart found unnamed, lookups before manifests before blobs`, async () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-tools-prune-'));
    try {
      const origin = open(dir), now = Date.UTC(2026, 10, 22), old = now - 100 * DAY, fresh = now - DAY;
      const t = '.exact/tools/aarch64-apple-darwin', id = (c) => c.repeat(64), blob = (c) => blobPath(c.repeat(64));
      const manifestAt = (c) => `${t}/manifests/${id(c)}.json`, lookup = (c) => `${t}/commits/${c.repeat(40)}.json`;
      const write = async (rel, value, time) => { await origin.put(rel, Buffer.from(JSON.stringify(value))); age(origin, rel, time); };
      const manifest = (...blobs) => ({ products: Object.fromEntries(blobs.map((b, i) => [`p${i}`, { sha256: b.repeat(64) }])) });
      // A: in recent.json. B: named by a fresh lookup. C: named only by an expired lookup. D: named by nothing. E: new.
      await write(manifestAt('a'), manifest('1', '5'), old);
      await write(manifestAt('b'), manifest('2'), old);
      await write(manifestAt('c'), manifest('3'), old);
      await write(manifestAt('d'), manifest('4', '5'), old);
      await write(manifestAt('e'), manifest('6'), fresh);
      await write(`${t}/recent.json`, [id('a')], old);
      await write(lookup('1'), id('b'), fresh);
      await write(lookup('2'), id('c'), old);
      for (const c of '1234567') await write(blob(c), c, old);
      const present = async () => (await origin.files('.exact')).map(({ path }) => path).filter((p) => !p.endsWith('/.lock') && !p.endsWith('/pruned.json')).sort();
      const all = await present();
      const gone = (...paths) => all.filter((p) => !paths.includes(p));
      const pass = (day) => pruneOrigin(origin, { now: now + day * DAY });

      // The first pass only marks: the expired lookup, D and the blob nothing names.
      expect(await pass(0)).toEqual({ lookups: 0, manifests: 0, blobs: 0, condemned: 3 });
      expect(await present()).toEqual(all);
      expect(await pass(0.5)).toBe(null);
      // Marked a day ago and still unnamed: deleted. C stays, since its lookup was there when the pass began, so a
      // reader that had just read the lookup can still read C and its blob.
      expect(await pass(1)).toEqual({ lookups: 1, manifests: 1, blobs: 1, condemned: 0 });
      expect(await present()).toEqual(gone(lookup('2'), manifestAt('d'), blob('7')));
      // C is unnamed now, and so is D's blob 4; blob 5 is still A's. Each goes a pass after it is marked.
      expect(await pass(2)).toEqual({ lookups: 0, manifests: 0, blobs: 0, condemned: 2 });
      expect(await pass(3)).toEqual({ lookups: 0, manifests: 1, blobs: 1, condemned: 0 });
      expect(await pass(4)).toEqual({ lookups: 0, manifests: 0, blobs: 0, condemned: 1 });
      expect(await pass(5)).toEqual({ lookups: 0, manifests: 0, blobs: 1, condemned: 0 });
      expect(await present()).toEqual(gone(lookup('2'), manifestAt('c'), manifestAt('d'), ...'347'.split('').map(blob)));

      // A manifest marked once and named again before the next pass is kept.
      await write(`${t}/recent.json`, [], old);
      expect(await pass(6)).toEqual({ lookups: 0, manifests: 0, blobs: 0, condemned: 1 });
      await write(lookup('3'), id('a'), now + 6.5 * DAY);
      expect(await pass(7)).toEqual({ lookups: 0, manifests: 0, blobs: 0, condemned: 0 });
      expect(await origin.get(manifestAt('a'))).not.toBe(null);

      await write(`${t}/manifests/${id('f')}.json`, { products: {} }, old);
      await expect(pass(8)).rejects.toThrow('does not name its blobs');
      await origin.delete(`${t}/manifests/${id('f')}.json`);
      // A deploy channel named tools is not a tool set.
      await origin.put('.exact/tools/web/releases/r.json', Buffer.from('{}'));
      await expect(pass(8)).rejects.toThrow('need an origin of their own');
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 60000);
}

test('publishers and the pruner of one origin take turns on this machine', async () => {
  const home = mkdtempSync(resolve(tmpdir(), 'exact-tools-turns-')), origin = bucketOrigin(), order = [];
  try {
    const turn = (name) => withPublication(origin, async () => { order.push(`${name} in`); await Bun.sleep(200); order.push(`${name} out`); }, { HOME: home });
    await Promise.all([turn('a'), turn('b')]);
    expect(order[1]).toEndWith(' out');
    expect(order.length).toBe(4);
  } finally { rmSync(home, { recursive: true, force: true }); }
}, 30000);

test('a pass that fails a delete starts no other and returns only when every delete it started has finished', async () => {
  const files = new Map(), log = [];
  const origin = {
    describe: () => 'memory',
    get: async (rel) => files.get(rel)?.body ?? null,
    put: async (rel, body) => { files.set(rel, { body: Buffer.from(body), modified: 0 }); },
    files: async () => [...files].filter(([path]) => path !== '.exact/tools/pruned.json').map(([path, { modified }]) => ({ path, modified, version: null })),
    delete: async (rel) => {
      await Bun.sleep(rel.endsWith('1'.repeat(64)) ? 5 : 30);
      if (rel.endsWith('1'.repeat(64))) throw new Error('refused');
      log.push(rel);
    },
  };
  for (let i = 0; i < 40; i++) await origin.put(blobPath(i.toString(16).padStart(64, '1')), 'x');
  await pruneOrigin(origin, { now: DAY });
  const failure = await pruneOrigin(origin, { now: 2 * DAY }).catch((error) => error);
  expect(failure.message).toBe('refused');
  const settled = log.length;
  await Bun.sleep(100);
  expect(log.length).toBe(settled);
  expect(settled).toBeLessThan(16);
});
