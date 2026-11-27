// Streams and Compression Streams, run in Hermes (js/src/streams.js) and in
// Chrome by the same bundle (tests/it/pure.rs): the order a write, a read and
// a transform settle in, piping, teeing, iteration, and the codecs' bytes and
// errors. Output chunking is the implementation's, so only joined bytes are
// compared.

const bytes = (b64: string) => Uint8Array.from(atob(b64), c => c.charCodeAt(0));
const joined = (chunks: Uint8Array[]) => {
  const all = new Uint8Array(chunks.reduce((n, c) => n + c.byteLength, 0));
  let at = 0;
  for (const c of chunks) { all.set(c, at); at += c.byteLength; }
  return all;
};
const text = (chunks: Uint8Array[]) => new TextDecoder().decode(joined(chunks));
const errorName = (e: unknown) => (e instanceof TypeError ? 'TypeError' : e instanceof RangeError ? 'RangeError' : String((e as Error)?.name ?? e));
const settles = (p: Promise<unknown>) => p.then(v => 'ok ' + JSON.stringify(v ?? null), errorName);

// Discord's zlib-stream: one zlib stream across messages, each ending in a sync flush.
const PACKETS = [
  'eJyqVsovULIyNNBRSlGyqlbKSE0sKklKTSyJz8wrSS0qS8xRsjIxNDI1qK0FAAAA//8=',
  'ZNM9akMxFEThrQTVKTwz/u8M8QbSBeMiEGMMSZo4lXl7tzAGDXc6oXNV6EO6PSb74LVt2/t+9/bRnofO/5fvr7+2PdzapW+0WQ+/nz+nvnykl1mbXp8NtWE01sbRVJtGm9c2H21R22K0ZW3L0Va1rUZb17YebVPbxu4eMHCZpDEbBA5MB8ED80EAwYQQRDAjBBJMCcEEc0JAwaQQVDArhhXNimFFf0f5kMyKYUWzYljRrBhWNCuGFc2KYUWzYljRrBhWNCuFlcxKYSWzUljJf11+O7NSWMmsFFYyK4WVzEphJbNSWMmsFFZ9ZzpO0x0AAP//',
  'ghRghoa1AAAAAP//',
];
const DEFLATE = 'eJzLSM3JyVdISU3LSSxJBQAjDAUK', RAW = 'y0jNyclXKEosBwA=', GZIP = 'H4sIAAAAAAAAE8tIzcnJV0ivyiwAABlq0t8KAAAA';

// What one packet yields: write it, then read what is queued, keeping one
// read pending for the next packet. After a write settles, its transform has
// run and its output is queued (or fulfilled the pending read), so a read
// that does not settle at once means there is no more.
function inflater() {
  const ds = new DecompressionStream('deflate');
  const writer = ds.writable.getWriter(), reader = ds.readable.getReader();
  let pending = reader.read();
  const NOTHING = {};
  return async (packet: Uint8Array) => {
    await writer.write(packet);
    const out: Uint8Array[] = [];
    for (;;) {
      const got = await Promise.race([pending, Promise.resolve(NOTHING)]);
      if (got === NOTHING) return text(out);
      const { value, done } = got as ReadableStreamReadResult<Uint8Array>;
      if (done) return text(out);
      out.push(value);
      pending = reader.read();
    }
  };
}

async function decompress(format: CompressionFormat, input: Uint8Array) {
  const stream = new ReadableStream({ start(c) { c.enqueue(input); c.close(); } }).pipeThrough(new DecompressionStream(format));
  const out: Uint8Array[] = [];
  for await (const chunk of stream) out.push(chunk);
  return text(out);
}

async function roundTrip(format: CompressionFormat, value: string) {
  const packed: Uint8Array[] = [];
  const cs = new CompressionStream(format), w = cs.writable.getWriter();
  w.write(new TextEncoder().encode(value)); w.close();
  const r = cs.readable.getReader();
  for (let x = await r.read(); !x.done; x = await r.read()) packed.push(x.value);
  return decompress(format, joined(packed));
}

async function order() {
  const log: string[] = [];
  const ts = new TransformStream<number, number>({ transform(c, ctl) { log.push('transform ' + c); ctl.enqueue(c * 2); } });
  const w = ts.writable.getWriter(), r = ts.readable.getReader();
  log.push('desired ' + w.desiredSize);
  const w1 = w.write(1).then(() => log.push('wrote 1'));
  const w2 = w.write(2).then(() => log.push('wrote 2'));
  log.push('desired ' + w.desiredSize);
  w.ready.then(() => log.push('ready'));
  const r1 = r.read().then(x => log.push('read ' + x.value));
  await Promise.all([w1, r1]);
  const r2 = r.read().then(x => log.push('read ' + x.value));
  await Promise.all([w2, r2]);
  const closing = w.close().then(() => log.push('closed'));
  const end = r.read().then(x => log.push('done ' + x.done));
  await Promise.all([closing, end]);
  return log;
}

async function pipes() {
  const seen: string[] = [];
  const upper = new TransformStream<string, string>({ transform(c, ctl) { ctl.enqueue(c.toUpperCase()); }, flush(ctl) { ctl.enqueue('!'); } });
  const source = new ReadableStream<string>({ start(c) { ['a', 'b', 'c'].forEach(x => c.enqueue(x)); c.close(); } });
  await source.pipeThrough(upper).pipeTo(new WritableStream({ write(c) { seen.push(c); }, close() { seen.push('closed'); } }));
  const [one, two] = new ReadableStream<number>({ start(c) { c.enqueue(1); c.enqueue(2); c.close(); } }).tee();
  const all = async (s: ReadableStream<number>) => { const out: number[] = []; for await (const x of s) out.push(x); return out; };
  const teed = await Promise.all([all(one), all(two)]);
  // A sink that fails: the pipe rejects, and the source is cancelled with that reason.
  let cancelled = '';
  const failing = new ReadableStream({ pull(c) { c.enqueue(1); }, cancel(r) { cancelled = String(r); } })
    .pipeTo(new WritableStream({ write() { throw 'sink broke'; } }));
  const failed = await failing.then(() => 'resolved', e => String(e));
  // Pulled with backpressure: a high-water mark of 2 pulls twice before any read.
  let pulls = 0;
  const pulled = new ReadableStream({ pull(c) { pulls++; c.enqueue(pulls); } }, new CountQueuingStrategy({ highWaterMark: 2 }));
  await Promise.resolve(); await Promise.resolve(); await Promise.resolve();
  const before = pulls;
  const reader = pulled.getReader();
  const first = (await reader.read()).value;
  await reader.cancel();
  return { seen, teed, failed, cancelled, before, first, locked: pulled.locked, size: new ByteLengthQueuingStrategy({ highWaterMark: 8 }).size(new Uint8Array(3)) };
}

async function codecs() {
  const inflate = inflater();
  const packets: string[] = [];
  for (const p of PACKETS) packets.push(await inflate(bytes(p)));
  const formats = [await decompress('deflate', bytes(DEFLATE)), await decompress('deflate-raw', bytes(RAW)), await decompress('gzip', bytes(GZIP))];
  const trips = [];
  for (const f of ['deflate', 'deflate-raw', 'gzip'] as CompressionFormat[]) trips.push(await roundTrip(f, 'round ' + f + ' '.repeat(40) + 'trip'));
  // The standard's errors: corrupt input, data after the end, input that ends early, a chunk that is not bytes.
  const corrupt = await settles(decompress('deflate', new Uint8Array([1, 2, 3, 4, 5])));
  const trailing = await settles(decompress('deflate', new Uint8Array([...bytes(DEFLATE), 0])));
  const short = bytes(DEFLATE).subarray(0, 8);
  const ds = new DecompressionStream('deflate'), w = ds.writable.getWriter(), r = ds.readable.getReader();
  const reading = (async () => { const out: Uint8Array[] = []; for (let x = await r.read(); !x.done; x = await r.read()) out.push(x.value); return text(out); })();
  const wrote = await settles(w.write(short));
  const closed = await settles(w.close());
  const truncated = [wrote, closed, await settles(reading)];
  // (a write waits for a read: the transform runs only once the readable side wants a chunk)
  const gz = new DecompressionStream('gzip'), gzRead = settles(gz.readable.getReader().read());
  const notBytes = [await settles(gz.writable.getWriter().write('text' as never)), await gzRead];
  let format = '';
  try { new DecompressionStream('lzma' as CompressionFormat); } catch (e) { format = errorName(e); }
  return { packets, formats, trips, corrupt, trailing, truncated, notBytes, format };
}

export async function streams(): Promise<string> {
  return JSON.stringify({ order: await order(), pipes: await pipes(), codecs: await codecs(), bodies: await responseBodies() });
}

async function responseBodies() {
  const source = (...chunks: unknown[]) => new ReadableStream({ start(c) { chunks.forEach(chunk => c.enqueue(chunk)); c.close(); } });
  const said = (f: () => unknown) => { try { f(); return 'ok'; } catch (e) { return errorName(e); } };
  const encoded = new TextEncoder().encode('\ufeff{"text":"hé😀"}');
  const stream = source(...Array.from(encoded, b => new Uint8Array([0, b, 0]).subarray(1, 2)));
  const response = new Response(stream), clone = response.clone();
  const before = [response.bodyUsed, clone.bodyUsed, stream.locked, response.headers.get('content-type')];
  const reading = response.json();
  const during = [response.bodyUsed, clone.bodyUsed, said(() => response.clone())];
  const value = await reading, cloneText = await clone.text();
  const repeated = await settles(response.arrayBuffer());
  const empty = await new Response(source(new Uint8Array(0))).arrayBuffer();
  const invalid = [];
  for (const method of ['text', 'json', 'arrayBuffer'] as const) {
    for (const chunk of ['text', new ArrayBuffer(1), new Uint16Array([1]), new DataView(new ArrayBuffer(1))]) {
      const r = new Response(source(chunk));
      invalid.push([await settles(r[method]()), r.bodyUsed]);
    }
  }
  const failure = new Error('source failed'), errors = [];
  for (const method of ['text', 'json', 'arrayBuffer'] as const) {
    const r = new Response(new ReadableStream({ pull(c) { c.error(failure); } }));
    // Chrome replaces the source's error with a TypeError; both readers must reject.
    errors.push(await r[method]().then(() => false, () => true));
  }
  const locked = source(encoded), holder = locked.getReader();
  const lockedConstructor = said(() => new Response(locked));
  holder.releaseLock();
  const heldResponse = new Response(locked), heldReader = locked.getReader();
  const held = [await settles(heldResponse.text()), heldResponse.bodyUsed, said(() => heldResponse.clone())];
  heldReader.releaseLock();
  const afterRelease = await heldResponse.text();
  const disturbed = source(encoded), prior = new Response(disturbed), reader = disturbed.getReader();
  await reader.read(); reader.releaseLock();
  const used = [prior.bodyUsed, said(() => new Response(disturbed)), await settles(prior.text()), said(() => prior.clone())];
  const canceled = source(encoded), canceledResponse = new Response(canceled);
  await canceled.cancel();
  const cancelled = [canceledResponse.bodyUsed, said(() => new Response(canceled)), await settles(canceledResponse.text())];
  const chunk = new Uint8Array([1, 2, 3]), original = new Response(source(chunk)), copy = original.clone();
  const originalBytes = Array.from(new Uint8Array(await original.arrayBuffer()));
  chunk[0] = 9;
  const copyBytes = Array.from(new Uint8Array(await copy.arrayBuffer()));
  let number = 0;
  const buffer = new Uint8Array(1);
  const reused = new ReadableStream({ pull(c) {
    if (number === 3) { c.close(); return; }
    buffer[0] = ++number; c.enqueue(buffer);
  } }, { highWaterMark: 1 });
  const snapshots = Array.from(new Uint8Array(await new Response(reused).arrayBuffer()));
  return { collected: JSON.parse(await streamBody()), brands: JSON.parse(await streamBodyBrands()), before, during, value, cloneText, repeated, empty: empty.byteLength,
    invalid, errors, lockedConstructor, held, afterRelease, used, cancelled, originalBytes, copyBytes, snapshots };
}

export async function streamBodyBrands(): Promise<string> {
  const results = [];
  const chunks = [
    Object.setPrototypeOf(new Uint16Array([257]), Uint8Array.prototype),
    Object.defineProperty(new Uint16Array([257]), Symbol.toStringTag, { value: 'Uint8Array' }),
    Object.setPrototypeOf(new Uint8Array([1, 2]), Uint16Array.prototype),
    Object.defineProperty(new Uint8Array([3, 4]), Symbol.toStringTag, { value: 'Uint16Array' }),
  ];
  for (const chunk of chunks) {
    const response = new Response(new ReadableStream({ start(c) { c.enqueue(chunk); c.close(); } }));
    results.push(await response.arrayBuffer().then(b => Array.from(new Uint8Array(b)), errorName));
  }
  const bodies = [];
  const bytes = Object.setPrototypeOf(new Uint8Array([65]), ReadableStream.prototype);
  bodies.push(await new Response(bytes).text().then(v => v, errorName));
  for (const prototype of [null, Uint8Array.prototype]) {
    const stream = new ReadableStream({ start(c) { c.enqueue(new Uint8Array([66])); c.close(); } });
    Object.setPrototypeOf(stream, prototype);
    stream.toString = () => { throw new Error('the stream was stringified'); };
    try {
      const reader = ReadableStream.prototype.getReader.call(stream);
      reader.releaseLock();
      const response = new Response(stream), clone = response.clone();
      bodies.push([await response.text(), await clone.text()]);
    } catch (e) { bodies.push(errorName(e)); }
  }
  return JSON.stringify({ chunks: results, bodies });
}

export async function streamBody(): Promise<string> {
  const encoded = new TextEncoder().encode('hello');
  const source = () => new ReadableStream({ start(c) { c.enqueue(encoded); c.close(); } });
  const viaResponse = await new Response(source()).text();
  if (viaResponse !== 'hello') throw new Error(`Response read ${viaResponse}`);
  const cs = new CompressionStream('gzip'), writer = cs.writable.getWriter();
  const writes = Promise.all([writer.write(encoded), writer.close()]);
  const packed = new Uint8Array(await new Response(cs.readable).arrayBuffer());
  await writes;
  const unpacked = await new Response(new ReadableStream({ start(c) { c.enqueue(packed); c.close(); } })
    .pipeThrough(new DecompressionStream('gzip'))).text();
  return JSON.stringify({ viaResponse, magic: Array.from(packed.subarray(0, 2)), unpacked });
}

export async function streamUpload(): Promise<string> {
  const results = [];
  const locked = Object.getOwnPropertyDescriptor(ReadableStream.prototype, 'locked')!.get!;
  for (const prototype of [ReadableStream.prototype, null, Uint8Array.prototype]) {
    for (const duplex of ['half', undefined]) {
      const stream = new ReadableStream({ start(c) { c.enqueue(new Uint8Array([1])); c.close(); } });
      Object.setPrototypeOf(stream, prototype);
      let stringified = false;
      stream.toString = () => { stringified = true; return 'wrong'; };
      try {
        await fetch('https://fixture.exact.test/up', { method: 'POST', body: stream, duplex } as RequestInit);
        results.push('sent');
      } catch (e) {
        results.push(`${errorName(e)}: ${(e as Error).message}; locked=${locked.call(stream)}; stringified=${stringified}`);
      }
    }
  }
  return JSON.stringify(results);
}

export async function codecLimits(): Promise<string> {
  // 1. A small deflate payload that expands past the module's memory ceiling rejects with TypeError.
  const cs = new CompressionStream('deflate'), cw = cs.writable.getWriter();
  const writeZeros = (async () => {
    const chunk = new Uint8Array(64 * 1024);
    for (let i = 0; i < 32; i++) await cw.write(chunk); // 2 MiB of zeros -> ~2 KiB compressed
    await cw.close();
  })();
  const bomb = new Uint8Array(await new Response(cs.readable).arrayBuffer());
  await writeZeros;
  let bombError = '';
  try {
    await decompress('deflate', bomb);
    bombError = 'ok';
  } catch (e: any) {
    bombError = `${errorName(e)}: ${e.message}`;
  }

  // 2. An unterminated gzip FNAME header is bounded and rejects with TypeError.
  let headerError = '';
  try {
    const gz = new DecompressionStream('gzip'), w = gz.writable.getWriter();
    const reading = new Response(gz.readable).arrayBuffer();
    reading.catch(() => {});
    await w.write(new Uint8Array([0x1f, 0x8b, 8, 8, 0, 0, 0, 0, 0, 3]));
    const fill = new Uint8Array(8192).fill(97);
    for (let i = 0; i < 20; i++) await w.write(fill);
    headerError = 'ok';
  } catch (e: any) {
    headerError = `${errorName(e)}: ${e.message}`;
  }

  // 3. Opening too many concurrent codecs rejects with TypeError.
  const held: DecompressionStream[] = [];
  let capError = '';
  try {
    for (let i = 0; i < 100; i++) held.push(new DecompressionStream('deflate'));
    capError = 'ok';
  } catch (e: any) {
    capError = `${errorName(e)}: ${e.message}`;
  }
  return JSON.stringify({ bombBytes: bomb.byteLength, bombError, headerError, opened: held.length, capError });
}
