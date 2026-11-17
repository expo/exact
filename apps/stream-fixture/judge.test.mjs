// The throughput gate's verdict over runs as gate.mjs records them.
import { expect, test } from 'bun:test';
import { judge, unestablished } from './judge.mjs';

const SECONDS = 60, FRAMES = 60 * SECONDS;
const frame = { presented: 3600, p95: 16.8, missed: 2 };
const stream = (name) => ({ name, messages: FRAMES, decodes: FRAMES, busy: 0.01, toDecode: { n: FRAMES, p95: 2, p99: 4, max: 6 } });

// The gate's six runs, each with `make(streams)` of what it saw.
function runs(make) {
  return [0, 1, 1, 1, 3, 3].map((streams) => ({
    streams, seconds: SECONDS, frame, failed: [], refused: 0, dropped: 0, busy: 0.03,
    toDecode: { p95: 2, p99: 4 }, live: [], perStream: [], connections: [], ...(streams ? make(streams) : {}),
  }));
}
const names = (n) => ['a', 'b', 'c'].slice(0, n);
const working = (n) => ({ live: names(n), perStream: names(n).map(stream), connections: names(n).map(() => FRAMES) });

test('a complete run of every declared stream passes', () => {
  const pass = judge(runs(working));
  expect(Object.values(pass).every(Boolean)).toBe(true);
});

test('a page that draws but opens no stream passes nothing', () => {
  const results = runs(() => ({}));
  expect(unestablished(results[4])).toBe('a, b, c not live; streams received for [], not [a,b,c]; 0 connections, not 3');
  expect(judge(results)).toEqual({ established: false, noOverflowOrDrop: false, decodeP95: false, decodeP99: false, busy: false, frames: false });
});

test('one working stream of three does not establish the run', () => {
  const results = runs((n) => (n === 3 ? { ...working(1), live: names(3) } : working(n)));
  expect(unestablished(results[4])).toBe('streams received for [a], not [a,b,c]; 1 connections, not 3');
  expect(judge(results).established).toBe(false);
});

test('streams received without decode timing, or sent too little, do not establish the run', () => {
  const blind = runs((n) => ({ ...working(n), perStream: names(n).map((k) => ({ ...stream(k), decodes: 0, toDecode: { n: 0, p95: 0, p99: 0, max: 0 } })) }));
  expect(unestablished(blind[1])).toBe('a: 0 decodes, 0 timed');
  expect(judge(blind).established).toBe(false);
  const stalled = runs((n) => ({ ...working(n), connections: names(n).map(() => 40) }));
  expect(unestablished(stalled[5])).toBe('sent 40, 40, 40 frames, under 1800');
  expect(judge(stalled).decodeP95).toBe(false);
});
