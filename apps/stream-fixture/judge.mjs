// The throughput gate's verdict on its runs (gate.mjs, LLP 1133.003 §9).
// A run's latency, busy and frame numbers say something only when the
// streams it declares were there: each of its resources live, one stream
// per resource receiving and decoding, and the server sending each at its
// pace for the time measured. A run without them fails as unestablished
// rather than passing on empty numbers.

const RESOURCES = ['a', 'b', 'c'];
const FPS = 60;

// Why `run` did not establish its declared streams, or null when it did.
export function unestablished(run) {
  const why = [];
  const want = RESOURCES.slice(0, run.streams);
  const dead = want.filter((k) => !run.live.includes(k));
  if (dead.length) why.push(`${dead.join(', ')} not live`);
  const names = run.perStream.map((x) => x.name).sort();
  if (names.join() !== want.join()) why.push(`streams received for [${names}], not [${want}]`);
  // Half of what the server's pace sends in the time measured: a stream that
  // opened late or stalled falls under it.
  const least = (FPS * run.seconds) / 2;
  if (run.connections.length !== run.streams) why.push(`${run.connections.length} connections, not ${run.streams}`);
  const short = run.connections.filter((frames) => frames < least);
  if (short.length) why.push(`sent ${short.join(', ')} frames, under ${Math.ceil(least)}`);
  for (const x of run.perStream) {
    if (x.decodes < least || !x.toDecode.n) why.push(`${x.name}: ${x.decodes} decodes, ${x.toDecode.n} timed`);
  }
  return why.length ? why.join('; ') : null;
}

// Pass or fail for each criterion (§16 question 8) over the gate's runs:
// the first is the run with no stream, and two have three streams.
export function judge(results) {
  const base = results[0].frame, three = results.filter((r) => r.streams === 3);
  if (three.length !== 2) throw new Error(`gate: ${three.length} three-stream runs, not 2`);
  const established = results.every((r) => r.streams === 0 || !unestablished(r));
  const each = (test) => established && three.every(test);
  return {
    established,
    noOverflowOrDrop: each((r) => !r.failed.length && !r.refused && !r.dropped),
    decodeP95: each((r) => r.toDecode.p95 < 8),
    decodeP99: each((r) => r.toDecode.p99 < 16),
    busy: each((r) => r.busy < 0.25),
    // Within the noise: the p95 frame interval moves by under a millisecond, and
    // missed vsyncs by under 1% of the frames presented.
    // A run that presented nothing (the app left the foreground, and the
    // macOS sampler with it) measured no frame time, and fails.
    frames: base.presented > 0 && each((r) => r.frame.presented > 0 && r.frame.p95 - base.p95 < 1 && (r.frame.missed / r.frame.presented) - (base.missed / base.presented) < 0.01),
  };
}
