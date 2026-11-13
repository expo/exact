import { expect, test } from 'bun:test';
import { cpSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Whether an answer in flight was let go (ts-fetch.js `letGo`), which its later sends, timers and fetches ask.
// The call and its ticket are made by hand, as rt.js ties them; ts-fetch.js loads beside a stand-in admission, as
// answer-timers loads it.
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

test('an answer is let go when its owner no longer holds its ticket: one ticket, or one of a parallel mutation\'s', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-let-go-'));
  cpSync(resolve(ROOT, 'host/web-js/ts-fetch.js'), resolve(dir, 'ts-fetch.js'));
  writeFileSync(resolve(dir, 'admission.js'), `export { ownWithout, redirectOf, signalDeadline, socketProtocols } from ${JSON.stringify(pathToFileURL(resolve(ROOT, 'host/web-js/admission.js')).href)};
export class FetchError extends Error {} export const overlaying = { on: false }; export const inOverlay = what => new Error(what); export const fetchWith = () => Promise.resolve();
`);
  writeFileSync(resolve(dir, 'admission-data.js'), 'export const tsGrantSet = {};\n');
  try {
    const { answers, letGo } = await import(pathToFileURL(resolve(dir, 'ts-fetch.js')).href);
    // A resource, and a mutation that keeps its newest send or queues: the owner's one ticket.
    const one = { ticket: null }, call = { ticket: { owner: one } };
    one.ticket = call.ticket;
    expect([!!letGo(call), answers(call)]).toEqual([false, true]);
    one.ticket = {}; // a newer ask
    expect([!!letGo(call), answers(call)]).toEqual([true, false]);
    one.ticket = call.ticket; one.gone = true; // its view ended
    expect(letGo(call)).toBe(true);
    // A `parallel` mutation (LLP 1161.000) leaves `ticket` null and holds a ticket for each send in flight.
    const sends = { parallel: true, ticket: null, tickets: [] }, sent = { ticket: { owner: sends } };
    sends.tickets = [{}, sent.ticket];
    expect([!!letGo(sent), answers(sent)]).toEqual([false, true]);
    sends.tickets = [sends.tickets[0]]; // it concluded
    expect([!!letGo(sent), answers(sent)]).toEqual([true, false]);
    // No ticket yet (its source is still running): not let go; and one that has answered answers no more.
    expect([!!letGo({}), answers({ settled: true })]).toEqual([false, false]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
