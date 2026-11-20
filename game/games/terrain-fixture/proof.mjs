#!/usr/bin/env bun
import { resolve } from 'node:path';
import { proof } from '../../proof.mjs';
import logic from './logic/src/lib.rs' with { type: 'text' };

const [, width, height] = logic.match(/^pub const VIEWPORT: \[u32; 2\] = \[(\d+), (\d+)\];$/m);
export const sectionsDrawn = Number(logic.match(/^pub const SECTIONS_DRAWN: usize = (\d+);$/m)[1]);
const sections = 2312;
const allowance = 64;
const advances = Math.ceil(sections / allowance);
const lookAt = 18;

function sectionCount(world, expected) {
  const reasons = world?.readyReasons ?? [];
  const pending = reasons.find(reason => /^\d+ sections pending\b/.test(reason));
  const count = pending ? Number(pending.split(' ')[0]) : undefined;
  if (count !== expected) {
    throw new Error(`Expected ${expected} sections pending: ${reasons.join(', ') || 'section backlog unavailable'}`);
  }
  return pending;
}

export async function proveTerrain(session, check, extraLooks = true, jobs = false) {
  const { clock } = await session.clock('+0');
  const target = await session.target('world');
  const request = { op: 'state', ...target, world: true };
  const state = async () => (await session.op(request)).world;
  const look = async () => {
    const { data, width, height, tick, error, unavailable } = await session.op({
      op: 'screenshot', ...target, world: true, form: 'look', resolution: 0.25,
    });
    if (!data) throw new Error(`Terrain look: ${error || unavailable || 'no pixels'}`);
    return { data, width, height, tick };
  };

  // Redraws at the play timestamp let the pipeline validate: no advance and no wall-clock wait.
  let initial = await state();
  for (let redraws = 0; redraws < 20 && initial?.readyReasons?.some(reason => reason.includes('terrain pipeline validating')); redraws++) {
    if ((await session.clock('+0')).clock !== clock) throw new Error('A redraw at the play timestamp moved the clock');
    initial = await state();
  }
  const failure = initial?.readyReasons?.find(reason => reason.startsWith('render hook:'));
  if (failure) throw new Error(`Terrain pipeline validation failed: ${failure}`);
  if (sectionCount(initial, sections) !== `${sections} sections pending`) {
    throw new Error('Terrain pipeline did not validate at the play timestamp');
  }
  check('pipeline validated with all 2,312 sections pending before the first advance', true);

  function jobsAt(world, done) {
    const report = world?.renderHooks?.jobs;
    if (report?.lane !== 'inline' || report.done !== done || report.running || report.stale || report.failed) {
      throw new Error(`Expected the inline lane with ${done} terrain jobs taken: ${JSON.stringify(report)}`);
    }
  }

  if (jobs) jobsAt(initial, 0);
  let partial;
  for (let advance = 1; advance <= advances; advance++) {
    await session.clock('+16');
    if (advance < advances) {
      if (!await session.find('terrain-pending', false, false)) {
        throw new Error(`Terrain presented after only ${advance} advances`);
      }
      const current = await state();
      sectionCount(current, sections - advance * allowance);
      if (jobs) jobsAt(current, advance * allowance);
    }
    if (advance === lookAt) {
      partial = await look();
      if (extraLooks) {
        let matches = true;
        for (let i = 0; i < 20; i++) {
          if (!sameLook(partial, await look())) matches = false;
        }
        check('all 20 extra looks match the first at the same time', matches);
      }
      const current = await state();
      sectionCount(current, sections - advance * allowance);
      if (jobs) jobsAt(current, advance * allowance);
    }
  }
  if (await session.find('terrain-pending', false, false)) {
    const reasons = (await state())?.readyReasons?.join(', ') || 'section backlog unavailable';
    throw new Error(`Terrain did not present after ${advances} advances: ${reasons}`);
  }
  if (jobs) jobsAt(await state(), sections);
  check('terrain presents after exactly 37 advances', true);
  const complete = await look();

  await session.op({ ...request, perf_reset: true });
  await session.clock('+16');
  const frame = (await state())?.renderHooks?.frame;
  const samples = frame?.itemsDrawn;
  check('the hook frame report names sections', frame?.items === 'sections', frame?.items);
  check('at least one itemsDrawn sample after perf reset', samples?.count > 0, samples);
  check(`itemsDrawn maximum is ${sectionsDrawn} sections`, samples?.max === sectionsDrawn, samples?.max);
  check(`itemsDrawn mean is ${sectionsDrawn} sections`, samples?.mean === sectionsDrawn, samples?.mean);
  return { clock, partial, complete };
}

function sameLook(a, b) {
  return a.tick === b.tick && a.width === b.width && a.height === b.height && a.data === b.data;
}

if (import.meta.main) await proof(import.meta, async ({ pin, pinSave, open, check, host, out }) => {
  const env = { EXACT_SCALE: '1' };
  if (host === 'linux') {
    env.EXACT_GPU_RENDER = '1';
    // The bake joins shader preludes under .shells/shaders, where the host can verify the packaged bytes.
    env.EXACT_ASSETS = resolve(process.env.EXACT_APP_DIR, '.shells');
  }
  for (const jobs of [false, true]) {
    let first;
    for (let run = 0; run < 2; run++) {
      const session = await open({ size: [Number(width), Number(height)], env });
      try {
        if (jobs) await session.type('jobs', 'on');
        await session.tap('play');
        const leg = jobs ? 'jobs=on' : 'jobs=off';
        const verify = (name, passed, details) => check(`${leg} ${name}`, passed, details);
        const pictures = await proveTerrain(session, verify, run === 0, jobs);
        const { clock } = pictures;
        if (first) {
          verify('drives with and without extra looks produce byte-identical checkpoint pictures',
            clock === first.clock && sameLook(pictures.partial, first.partial) && sameLook(pictures.complete, first.complete));
        } else {
          first = pictures;
        }
        if (!jobs) await session.clock(clock + 1000);
        if (!jobs && run === 0) {
          const world = session.world('world');
          pin(60, await world.snapshot());
          const save = resolve(out, 'terrain-60.world');
          await world.save(save);
          pinSave('continuation', save);
        }
      } finally {
        await session.close();
      }
    }
  }
});
