import { expect, test } from 'bun:test';
import { proveTerrain, sectionsDrawn } from './proof.mjs';

function drive({
  presentAt = 37,
  initial = 2312,
  jobs = false,
  extraJobs = 0,
  validating = false,
  validationError = false,
  changeLookAt = 0,
  itemsDrawn = { count: 3, max: sectionsDrawn, mean: sectionsDrawn },
} = {}) {
  let advances = 0, looks = 0, resetAt, redraws = 0, initialReads = 0;
  const playClock = 100;
  const session = {
    async target() { return { id: 1 }; },
    async find() { return advances < presentAt; },
    async clock(step) {
      if (step === '+0') {
        expect(advances).toBe(0);
        // The pipeline validates at the third redraw of the play timestamp.
        if (++redraws === 3) validating = false;
        return { clock: playClock };
      }
      expect(step).toBe('+16');
      advances++;
    },
    async op(request) {
      expect(request.world).toBe(true);
      if (request.form === 'look') {
        looks++;
        return { data: looks === changeLookAt ? 'YQ==' : 'Yg==', width: 320, height: 180, tick: advances };
      }
      if (request.perf_reset) resetAt = advances;
      const pending = advances >= 37 ? 0 : initial - 64 * advances;
      if (validating && ++initialReads > 3) throw new Error('The drive polled validation without redrawing');
      return { world: {
        readyReasons: [
          `${pending} sections pending${validating ? ', terrain pipeline validating' : ''}`,
          ...(validationError ? ['render hook: rejected pipeline'] : []),
        ],
        renderHooks: {
          frame: { items: 'sections', itemsDrawn },
          jobs: jobs ? { lane: 'inline', done: initial - pending + (looks > 1 ? extraJobs : 0),
            stale: 0, failed: 0, running: 0 } : null,
        },
      } };
    },
  };
  const failures = [];
  return {
    async run(extraLooks = true) {
      const pictures = await proveTerrain(session, (label, ok) => {
        if (!ok) failures.push(label);
      }, extraLooks, jobs);
      return { advances, looks, resetAt, redraws, failures, pictures };
    },
  };
}

test('the drive redraws at the play timestamp until the pipeline validates', async () => {
  const { redraws, failures } = await drive({ validating: true }).run();
  expect(redraws).toBe(3);
  expect(failures).toEqual([]);
});

test('the drive rejects a failed pipeline at the play timestamp', async () => {
  await expect(drive({ validationError: true }).run()).rejects.toThrow('rejected pipeline');
});

test('a terrain drive rejects work before the first advance', async () => {
  await expect(drive({ initial: 2248 }).run()).rejects.toThrow('Expected 2312 sections pending: 2248 sections pending');
});

test('the terrain must present on advance 37', async () => {
  await expect(drive({ presentAt: 36 }).run()).rejects.toThrow('Terrain presented after only 36 advances');
  await expect(drive({ presentAt: 38 }).run()).rejects.toThrow('Terrain did not present after 37 advances: 0 sections pending');
  expect(await drive().run()).toMatchObject({ advances: 38, resetAt: 37, failures: [] });
});

test('the drive catches any picture change during the 20 looks', async () => {
  expect((await drive({ changeLookAt: 2 }).run()).failures).toEqual([
    'all 20 extra looks match the first at the same time',
  ]);
});

test('the second drive takes only the matching checkpoint pictures', async () => {
  const first = await drive().run(true);
  const second = await drive().run(false);
  expect(second.looks).toBe(2);
  expect(first.looks).toBe(second.looks + 20);
  expect(second.pictures).toEqual(first.pictures);
});

test('all sampled frames must draw the pinned sections', async () => {
  const run = async (itemsDrawn) => (await drive({ itemsDrawn }).run()).failures;
  const pinned = { max: sectionsDrawn, mean: sectionsDrawn };
  expect(await run({ count: 1, ...pinned })).toEqual([]);
  expect(await run({ count: 3, ...pinned })).toEqual([]);
  expect(await run({ count: 0, max: 0, mean: 0 })).toContain('at least one itemsDrawn sample after perf reset');
  expect(await run({ count: 2, ...pinned, max: sectionsDrawn + 1 })).toEqual([`itemsDrawn maximum is ${sectionsDrawn} sections`]);
  expect(await run({ count: 2, ...pinned, mean: sectionsDrawn - 0.5 })).toEqual([`itemsDrawn mean is ${sectionsDrawn} sections`]);
});

test('the jobs leg requires inline progress and held looks leave its counters unchanged', async () => {
  expect(await drive({ jobs: true }).run()).toMatchObject({ advances: 38, resetAt: 37, failures: [] });
  await expect(drive({ jobs: true, extraJobs: 1 }).run()).rejects.toThrow('1152 terrain jobs taken');
});
