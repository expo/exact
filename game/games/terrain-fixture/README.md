# Terrain fixture

This fixture uses a seeded heightfield and a fixed camera (LLP 1182.000 D4a).
The hook meshes up to 64 of the window's 2,312 section addresses per advancing
frame and submits one indexed draw per visible section with exposed terrain.
The logic test counts those sections independently by testing each section's
eight corners.

Check the recorded tick and save baseline at 1280 × 720 from the repository root.

```sh
bun game/games/terrain-fixture/proof.mjs web
bun game/games/terrain-fixture/proof.mjs linux
```

Each leg runs the drive twice. Settling at the play timestamp validates the
pipeline with all 2,312 sections still pending. The proof takes 20 extra looks
partway through meshing and requires presentation after exactly 37 advances.
It compares pixels at the same partial and complete frames across both runs.
After a perf reset and one advance, every sample must report the pinned 823
sections.

The Linux leg enables the GPU-backed agent and reads the bake's shader pack.
On a Mac with Metal and Chrome with WebGPU, record the tick and save baseline
again with `bun game/prove.mjs terrain-fixture --hosts linux,web --repin`.

The continuation save includes the `jobs` canvas argument. Older saves lack
that field, so their continuation bytes differ. The smoke tick hash is unchanged, and the save at 1280 × 720 is
pinned from the device-free simulation. A restore to the same or a later tick
with the same seed and settings keeps the window and jobs in flight. Both
derive from the tick, seed and settings. `jobs=off` keeps its window the
same way.

The proof also runs `jobs=on` on each host using the same radius-8 smoke
window. Both runs require 37 advances and the `inline` lane. The 20 held
looks preserve the job counts and pixels, and the completed frame draws 823
sections.

## Decision workload

Run `bun game/games/terrain-fixture/bench.mjs --help` for the web, macOS and
iOS commands. Display runs require the device, OS and refresh rate; an iOS run
also takes `--phone=<name or identifier>` of a paired device and draws at the
device's size. The web build uses the `web` profile and the native ones
`host-dev`. `BENCH_HEADLESS=1` runs the web transport without a display and
marks the row headless.

The web row's browser is the one the drivers launch, or the one `CHROME` names,
started without Chromium's testing configuration of experiments
(`--disable-field-trial-config`) and recorded in the row. With that
configuration, Chrome for Testing 159 gave this page a main frame on one
display frame in two to four while it uploaded sections. A Mac row records the
machine's load before and after, and marks itself when there was more load
than cores.

- The default `budget=deadline` stops after a completed operation when the live
  deadline has passed. `budget=count:N` respects the engine's allowance and
  checks the deadline with `jobs=on`. Section work takes priority over blended
  uploads.
- A unit of the allowance is one mesh, one install, one dropped staged result,
  one edit or one blended upload, or 64 reuse compares or retirements, which
  are far cheaper (a compare measured about 0.01 ms). A run of fewer than 64
  before a larger unit, or at the end of a call, costs nothing. With no
  deadline (the agent's clock, or `budget=count:N` with `jobs=off`) a call can do 64
  times its allowance of them: 4,096 compares at the declared 64. The row's
  `operationsCompleted` counts each compare, retirement, mesh, install and edit
  as one, so it is not the allowance spent.
- `light=N` visits N distinct cells per section, capped at 97,336 cells
  (46³ for one 16³ section with a 15-cell halo, LLP 1182 §2).
  The report includes the effective cell count.
- `compare=on` checks 20³ padded cells on section reuse. The hook meshes missing
  sections nearest the camera first and queues one compare per installed
  section awaiting reuse. A changed repair mesh stages until an advancing
  frame can install it. A late start begins the edit schedule at the current tick.
- `jobs=off|on` defaults to `off`. The jobs lane moves section lighting and
  meshing to one job per section and batches up to 64 reuse compares per job.
  Compares share the previous padded inputs. Installation, retirement and
  blended sorting and uploads stay in `prepare`. The frame reserves its
  last unit for section work when sections are pending and none has run.
  Each result costs one unit, even when a compare batch has fewer than 64
  sections. After the frame's units, the hook refills the pool's queue up
  to 32 KiB, past the deadline if needed. That step selects and submits work.
  Lighting and meshing run on the workers.
- With `inflight=keep` on either lane, a kept stale mesh finishes its crossing
  and its repair is separate work.
- On a canvas that rests, pending jobs can keep the canvas drawing and ticking
  after a canvas with finished jobs would rest, so later input can land on a
  different tick.
- The lane uses up to four workers on a live native canvas and runs inline
  under the agent's clock or on WebAssembly. A hook that uses jobs declares
  a positive allowance. This fixture takes at most 64 results per frame
  within its current allowance and takes the first result of a late frame
  past the deadline. It caps queued inputs at 32
  KiB and holds completed outputs at a 32 MiB threshold. The queue budget
  covers metadata and references to already resident padded inputs. The
  completion threshold prevents a radius-40 window of 64-byte vertices
  from accumulating in CPU memory while uploads wait. Up to four running
  meshes can finish after the threshold is reached.
- `itemsDrawn` counts submitted sections once across the opaque and blended
  draws, using the sections in the blended buffer's last upload.

Mount times are observations at `pollMs` intervals. `draw=off` reports a null
first picture. A refill starts with its crossing frame's full interval and
ends after its sections and blended buffer are current. Each window reports
the count of unfinished crossings, the oldest age and up to four details.
A crossing with discarded sections stays unfinished. The hook retains 128
finished durations for window percentiles. A window that exceeds that limit
reports the completion count and marks its distribution inconclusive.

The discard counters distinguish stale staged results, staged results that left
the window, and queued work dropped after it left the window. With
`jobs=on`, `operationsCompleted` counts job operations whose results reached
the hook, along with operations performed in `prepare`. Work a worker
finished for a section that left the window contributes to `queuedLeft`
and `renderHooks.jobs.stale`. A compare batch contributes one stale result
and counts each of its sections in `queuedLeft`.

`gpuBufferBytes` covers installed vertex, index and blended buffers.
`cpuBytes` is sampled in `prepare` and covers retained padded inputs, staged
meshes, blended quads, queued jobs and completed results. It excludes the
submitted batch's section list (about 2 KB per batch) while its result waits.
It also excludes previous padded inputs a running compare still shares
after their sections retire, up to 64 × 8,000 bytes per running batch.
`peakCpuBytes` also counts
a result as it is taken, before installation. Each benchmark window records
its own jobs counters in `jobs`. The row records `lane` when enabled.

The JSON row records separate CPU timing rings with `paired: false`. The trace
export lacks per-frame prepare and total encode samples joined to tick CPU time
and the display interval on the same frame. `--pair draw` runs `draw=on` and
`draw=off` in order. It subtracts the corresponding encode statistics for each
window if completed operations match and uploaded bytes differ by at most 2%.
