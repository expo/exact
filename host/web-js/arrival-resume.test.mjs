import { test, expect } from 'bun:test';
import { arrivals } from './arrival.js';

// Coming back to the foreground resumes each arrival extension's own suspended scans (LLP 1133.035 D6), as the
// runner's `arrival_visibility`, and reads a tag still in its field: not only the first extension's.
test('coming back resumes every arrival extension, each reading the tag in its field', () => {
  const arrival = { field: 'field', from: 'tags', status: 'status', scan: 'scan', write: 'write', lock: 'lock', reading: 'reading', readingerror: 'readingerror' };
  const row = key => ({ key, send: true, sub: { arrival } });
  const rows = [row('nfc.scan'), row('tags.scan')];
  const held = rows.map((r, ticket) => ({ row: r, args: {}, waits: 'value', ticket }));
  const preferred = new Map(), said = [], raised = [];
  for (const extension of ['nfc', 'tags']) {
    preferred.set(`${extension}.tags`, [{ id: 'dune', records: [] }]);
    preferred.set(`${extension}.field`, 'dune');
  }
  const a = arrivals({ rows: () => rows, held, preferred, release() {}, raise: key => raised.push(key), say: line => said.push(line), valueOf: () => null });
  a.visibility(true);
  a.visibility(false);
  expect(said.filter(line => line.endsWith('scans resumed'))).toEqual(['nfc: scans resumed', 'tags: scans resumed']);
  expect(raised).toEqual(['nfc.reading', 'tags.reading']);
});

// An implementation's own arrival names (`write#@pkg/apple`) are its declared names, as the runner's.
test('an implementation-owned write replaces the waiting one', () => {
  const arrival = { field: 'field', from: 'tags', status: 'status', scan: 'scan', write: 'write', lock: 'lock', reading: 'reading', readingerror: 'readingerror' };
  const write = { key: 'nfc.write#@fixture/nfc/apple', send: true, sub: { arrival, owner: '@fixture/nfc/apple' } };
  const message = { records: [{ recordType: 'text', data: 'hi' }] };
  const held = [{ row: write, args: { message }, waits: 'value', ticket: 1 }], released = [];
  const a = arrivals({ rows: () => [write], held, preferred: new Map(), release: (h, json) => released.push([h.ticket, json]), raise() {}, say() {}, valueOf: () => null });
  expect(a.decide(write, true, { message })).toEqual({ hold: 'value' });
  held.push({ row: write, args: { message }, waits: 'value', ticket: 2 });
  a.claimed(write);
  expect(released).toEqual([[1, { error: 'AbortError' }]]);
});
