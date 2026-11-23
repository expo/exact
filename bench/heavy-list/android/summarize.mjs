// Medians over rounds of a measure.mjs series (blank frames: the sum and the worst over all rounds):
//   bun bench/heavy-list/android/summarize.mjs <out-dir>
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const { device, results } = JSON.parse(readFileSync(resolve(process.argv[2], 'results.json'), 'utf8'));
const median = xs => { const s = xs.filter(Number.isFinite).sort((a, b) => a - b); return s.length ? (s.length % 2 ? s[s.length >> 1] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2) : NaN; };
const apps = Object.keys(results);
const pick = (app, s, f) => median((results[app][s] ?? []).map(f));
const sum = (app, s, f) => (results[app][s] ?? []).reduce((n, r) => n + f(r), 0);
const jank = ({ janky, frames }) => Number.isFinite(janky) && Number.isFinite(frames) && frames > 0 ? 100 * janky / frames : NaN;
const blankFailed = a => results[a].blank?.some(r => r.error);
function fmt(v) {
  if (typeof v !== 'number') return v;
  if (Number.isNaN(v)) return 'unavailable';
  return Number.isInteger(v) ? String(v) : v.toFixed(v < 1 ? 2 : 1);
}
function gpuP90(app) {
  const values = (results[app].fling ?? []).map(r => r.gpuP90).filter(v =>
    (Number.isFinite(v) && v !== 4950) || Number.isFinite(v?.atLeast));
  const lower = median(values.map(v => v.atLeast ?? v));
  const exact = values.filter(Number.isFinite).length;
  return values.length && exact <= values.length / 2 ? `≥ ${fmt(lower)}` : lower;
}
const rows = [
  ['cold ms (am start -W)', a => median((results[a].cold ?? []).flatMap(r => r.runs))],
  ['fling frames', a => pick(a, 'fling', r => r.frames)],
  ['fling janky %', a => pick(a, 'fling', jank)],
  ['fling deadline missed', a => pick(a, 'fling', r => r.deadlineMissed)],
  ['fling p50 / p90 / p99 ms', a => ['p50', 'p90', 'p99'].map(k => fmt(pick(a, 'fling', r => r[k]))).join(' / ')],
  ['fling gpu p90 ms', gpuP90],
  ['fling cpu ms/s', a => pick(a, 'fling', r => r.cpuMsPerS)],
  ['fling main ms/s', a => pick(a, 'fling', r => r.mainMsPerS)],
  ['fling PSS MB', a => pick(a, 'fling', r => r.pssMB)],
  ['live janky %', a => pick(a, 'live', jank)],
  ['live deadline missed', a => pick(a, 'live', r => r.deadlineMissed)],
  ['live p90 / p99 ms', a => ['p90', 'p99'].map(k => fmt(pick(a, 'live', r => r[k]))).join(' / ')],
  ['live cpu ms/s', a => pick(a, 'live', r => r.cpuMsPerS)],
  ['live main ms/s', a => pick(a, 'live', r => r.mainMsPerS)],
  ['rest cpu ms/s', a => pick(a, 'rest', r => r.cpuMsPerS)],
  ['rest PSS MB', a => pick(a, 'rest', r => r.pssMB)],
  ['blank frames / filmed', a => blankFailed(a) ? 'unavailable' : `${sum(a, 'blank', r => r.blanks)} / ${sum(a, 'blank', r => r.frames)}`],
  ['blank worst band (screen)', a => blankFailed(a) ? 'unavailable' : Math.max(...(results[a].blank ?? []).map(r => r.worstBand))],
];
console.log(`${device.model}, Android ${device.android}, ${device.size.join('×')}, ${device.refresh}; medians of ${results[apps[0]].fling?.length ?? 0} rounds\n`);
console.log(`| | ${apps.join(' | ')} |\n|---|${apps.map(() => '---:').join('|')}|`);
for (const [name, f] of rows) console.log(`| ${name} | ${apps.map(a => fmt(f(a))).join(' | ')} |`);
