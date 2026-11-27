// Bun test preload installing the deterministic virtual clock and seeded
// Math.random() stream matching Exact's Hermes and web JS runtimes
// (LLP 1195.000 D2.6).
//
// Usage:
//   bun test --preload ./scripts/test-clock-preload.mjs <tests…>
//
// Environment variables:
//   EXACT_TEST_EPOCH     Unix ms or ISO-8601 date (default 2026-01-01T00:00:00.000Z)
//   EXACT_TEST_SEED      Unsigned 32-bit integer seed for Math.random() (default 1)
//   EXACT_TEST_TIME_ZONE IANA time zone (default UTC; sets process.env.TZ)
import { agentStream } from '../host/web/storage-environment.js';

const DEFAULT_EPOCH = 1767225600000;
const tz = process.env.EXACT_TEST_TIME_ZONE ?? 'UTC';
process.env.TZ = tz;

function parseEpoch(raw) {
  if (raw === undefined || raw === '') return DEFAULT_EPOCH;
  const n = /^\d+$/.test(raw) ? Number(raw) : Date.parse(raw);
  if (!Number.isSafeInteger(n) || n < 0) {
    throw new Error(`EXACT_TEST_EPOCH must be Unix ms or an ISO date, got ${JSON.stringify(raw)}`);
  }
  return n;
}

function parseSeed(raw) {
  if (raw === undefined || raw === '') return 1;
  const n = Number(raw);
  if (!Number.isSafeInteger(n) || n < 0 || n >= 2 ** 32) {
    throw new Error(`EXACT_TEST_SEED must be an integer in 0..2^32-1, got ${JSON.stringify(raw)}`);
  }
  return n;
}

const epochAtZero = parseEpoch(process.env.EXACT_TEST_EPOCH);
const seed = parseSeed(process.env.EXACT_TEST_SEED);
let elapsedMs = 0;

const stream = agentStream(seed, 'ts-random');
function randomFloat(b) {
  const hi = ((b[0] << 16) | (b[1] << 8) | b[2]) & 0x1fffff;
  const lo = ((b[3] << 24) | (b[4] << 16) | (b[5] << 8) | b[6]) >>> 0;
  return (hi * 4294967296 + lo) / 9007199254740992;
}

const NativeDate = globalThis.Date;
function TestDate(...args) {
  if (!new.target) return new NativeDate(Math.floor(epochAtZero + elapsedMs)).toString();
  if (!args.length) return Reflect.construct(NativeDate, [Math.floor(epochAtZero + elapsedMs)], new.target);
  return Reflect.construct(NativeDate, args, new.target);
}
TestDate.prototype = NativeDate.prototype;
Object.defineProperties(TestDate, {
  now: { value: () => Math.floor(epochAtZero + elapsedMs), writable: true, configurable: true },
  UTC: { value: NativeDate.UTC, writable: true, configurable: true },
  parse: { value: NativeDate.parse, writable: true, configurable: true },
});
globalThis.Date = TestDate;

Math.random = () => randomFloat(stream(7));

const nativePerf = globalThis.performance;
Object.defineProperty(globalThis, 'performance', {
  configurable: true,
  writable: true,
  value: Object.create(nativePerf, {
    now: { value: () => elapsedMs, configurable: true, writable: true },
  }),
});

globalThis.__exactTestClock = Object.freeze({
  epochAtZero,
  seed,
  timeZone: tz,
  now: () => Math.floor(epochAtZero + elapsedMs),
  elapsed: () => elapsedMs,
  advance(ms) {
    elapsedMs += Number(ms);
  },
  setElapsed(ms) {
    elapsedMs = Number(ms);
  },
});
