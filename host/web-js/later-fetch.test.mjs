import { expect, test } from 'bun:test';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// LLP 1016.000 D7, the amendment of 2026-11-12: a mutation whose source sent on a socket may fetch in a later
// turn, and that fetch waits for the commit that asked the answer. A view transition holds such a commit; here the
// call is made by hand, as ts-data.js leaves one whose source sent and whose commit has not stood (answer-timers
// loads ts-fetch.js the same way), beside an admission that records each request it is given.
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
async function load() {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-later-fetch-'));
  cpSync(resolve(ROOT, 'host/web-js/ts-fetch.js'), resolve(dir, 'ts-fetch.js'));
  writeFileSync(resolve(dir, 'admission.js'), `export { ownWithout, redirectOf, socketProtocols } from ${JSON.stringify(pathToFileURL(resolve(ROOT, 'host/web-js/admission.js')).href)};
export class FetchError extends Error { constructor(kind, message) { super(message); this.kind = kind; } }
export const overlaying = { on: false }; export const inOverlay = what => new Error(what); export const signalDeadline = { arm: () => undefined };
export const fetchWith = (grants, input, init) => {
  const request = input instanceof Request ? input : null;
  globalThis.given = init;
  globalThis.requested.push([request ? request.url : String(input), init?.method ?? request?.method, (request ? request.headers : new Headers(init?.headers)).get('x-step'), init?.body]);
  return Promise.resolve('answered');
};
`);
  writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = {};\n');
  writeFileSync(resolve(dir, 'ts-body-from.js'), "export const fromFile = (input, init, redirect) => { globalThis.requested.push([String(input), init.method, init.exactBodyTo, redirect]); return 'written'; };\n");
  return { dir, tsf: await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href) };
}
const turn = () => new Promise(r => setTimeout(r));

test("a later turn's fetch waits for the commit that asked its answer, as the request it was given, and never for an answer let go", async () => {
  const { dir, tsf } = await load();
  globalThis.requested = [];
  try {
    for (const sockets of [true, false]) {
      tsf.answering.sockets = sockets;
      // Its source sent, it is in flight, and the commit that asked it has neither stood nor been refused.
      const waiting = () => { const call = { mutation: true, turn: false, flight: true, socketOps: [{}] }; tsf.answering.call = call; return call; };
      const before = globalThis.requested.length;

      // Released when the commit stands: the request is what \`fetch\` was given, whatever its options became.
      let call = waiting();
      const headers = { 'x-step': 'first' }, options = { method: 'POST', headers, body: 'first' };
      const answered = tsf.fetch('https://example.test/a', options);
      tsf.answering.call = null;
      await turn();
      expect([globalThis.requested.length - before, call.waiting.length]).toEqual([0, 1]);
      headers['x-step'] = 'second'; options.body = 'second'; options.method = 'PUT';
      call.stood = true; call.waiting[0]();
      expect(await answered).toBe('answered');
      expect(globalThis.requested.slice(before)).toEqual([['https://example.test/a', 'POST', 'first', 'first']]);

      // A file's request too: the path, the method and the redirect mode are those of the call.
      call = waiting();
      const file = { method: 'GET', exactBodyTo: 'data/first.bin', redirect: 'manual' };
      const written = tsf.fetch(new URL('https://example.test/file'), file);
      tsf.answering.call = null;
      file.method = 'POST'; file.exactBodyTo = 'data/second.bin'; file.redirect = 'error';
      call.stood = true; call.waiting[0]();
      expect(await written).toBe('written');
      expect(globalThis.requested.at(-1)).toEqual(['https://example.test/file', 'GET', 'data/first.bin', 'manual']);

      // A `Request` too, with the signal it was given.
      call = waiting();
      const request = new Request('https://example.test/r', { method: 'POST', headers: { 'x-step': 'first' }, body: 'first' });
      const asked = tsf.fetch(request);
      tsf.answering.call = null;
      request.headers.set('x-step', 'second');
      call.stood = true; call.waiting[0]();
      expect(await asked).toBe('answered');
      expect(globalThis.requested.at(-1)).toEqual(['https://example.test/r', 'POST', 'first', undefined]);
      expect(globalThis.given.signal).toBe(request.signal);

      // One whose body was read, or is held by a reader, with another body in the options, is a request still.
      for (const hold of [r => r.text(), r => { r.body.getReader(); }]) {
        call = waiting();
        const used = new Request('https://example.test/u', { method: 'POST', headers: { 'x-step': 'first' }, body: 'read' });
        await hold(used);
        tsf.answering.call = call;
        const replaced = tsf.fetch(used, { body: 'replacement' });
        tsf.answering.call = null;
        used.headers.set('x-step', 'second');
        call.stood = true; call.waiting[0]();
        expect(await replaced).toBe('answered');
        expect([globalThis.requested.at(-1)[3], new Headers(globalThis.given.headers).get('x-step')]).toEqual(['replacement', 'first']);
      }
      // With no other body it is refused at the call, as `fetch` refuses it, though its reader lets go while it waits.
      call = waiting();
      const locked = new Request('https://example.test/l', { method: 'POST', body: 'held' }), reader = locked.body.getReader();
      const unusable = tsf.fetch(locked).then(() => 'answered', e => e.message);
      reader.releaseLock();
      expect([await unusable, call.waiting]).toEqual(["fetch(): the Request's body was read or is locked", undefined]);

      // A `parallel` mutation holds a ticket for each send in flight: one of them is not let go.
      call = waiting();
      const sends = { parallel: true, ticket: null, tickets: [] }; call.ticket = { owner: sends }; sends.tickets = [{}, call.ticket];
      call.stood = true;
      expect(await tsf.fetch('https://example.test/p', {})).toBe('answered');
      sends.tickets = [sends.tickets[0]];
      expect([tsf.answers(call), await tsf.fetch('https://example.test/p', {}).then(() => 'answered', e => e.message)]).toEqual([false, 'the answer was let go before this fetch']);

      // A stream is not such a fetch: a mutation that sent opens none, and may fetch after being told so.
      call = waiting();
      call.stood = true;
      expect(await tsf.fetch('wss://example.test/s', { exactStream() {} }).then(() => 'answered', e => e.message)).toBe('a mutation cannot both send on a socket and fetch');
      expect(await tsf.fetch('https://example.test/after', {})).toBe('answered');

      // The commit refused: no request, and the fetch says so.
      call = waiting();
      const refused = tsf.fetch('https://example.test/b', {}).then(() => 'answered', e => e.message);
      call.stood = false; call.waiting[0].refused('deliberate');
      expect(await refused).toBe('fetch(): the commit that asked this answer was refused: deliberate');

      // The answer let go while it waited: released, it fetches nothing.
      call = waiting();
      const owner = {}; call.ticket = { owner }; owner.ticket = call.ticket;
      const gone = tsf.fetch('https://example.test/c', {}).then(() => 'answered', e => e.message);
      owner.ticket = {}; // a newer ask took its place
      call.stood = true; call.waiting[0]();
      expect(await gone).toBe('the answer was let go before this fetch');
      expect(globalThis.requested.length - before).toBe(7);

      // A synchronous source still may not do both.
      tsf.answering.call = { mutation: true, turn: true, socketOps: [{}] };
      expect(await tsf.fetch('https://example.test/d', {}).then(() => 'answered', e => e.message)).toBe('a mutation cannot both send on a socket and fetch');
      tsf.answering.call = null;
    }
  } finally { delete globalThis.requested; delete globalThis.given; rmSync(dir, { recursive: true, force: true }); }
});
