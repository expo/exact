/** Locate each new failure independently; an obstructed probe never counts as a pass. */
export async function bisectFailures(commits, failures, probe) {
  if (commits.length < 2) return Object.fromEntries(failures.map(f => [f, { unresolved: 'no earlier commit to compare' }]));
  const cache = new Map();
  const at = async sha => {
    if (!cache.has(sha)) cache.set(sha, await probe(sha, failures));
    return cache.get(sha);
  };
  const out = {};
  for (const failure of failures) {
    let good = 0, bad = commits.length - 1;
    if ((await at(commits[bad]))[failure] !== 'fail') {
      out[failure] = { unresolved: 'tip failure did not reproduce', good: commits[good], bad: commits[bad] };
      continue;
    }
    let obstruction;
    // Failures can be repaired and reintroduced. Every commit after the boundary must still fail.
    for (let i = bad - 1; i >= 0; i--) {
      const status = (await at(commits[i]))[failure];
      if (status === 'pass') { good = i; break; }
      if (status !== 'fail') { obstruction = commits[i]; break; }
      bad = i;
    }
    out[failure] = obstruction
      ? { unresolved: `probe did not finish at ${obstruction}`, good: commits[good], bad: commits[bad] }
      : bad === 0 ? { preexisting: commits[0] } : { commit: commits[bad], parent: commits[good] };
  }
  return out;
}

const OVER = /^(.* VIOLATION) by (\d+(?:\.\d+)?) MB$/;
/** A byte-budget row's step: metrics.mjs rounds how far over its ceiling it is up to the next 0.1 MB. */
const STEP = 0.1;
const over = f => { const m = OVER.exec(f); return m && { row: m[1], steps: Math.round(Number(m[2]) / STEP) }; };

/**
 * Whether `failures` holds `f`. A byte-budget row's failure names how far over its ceiling it is
 * (`... VIOLATION by 0.3 MB`); the same row at that distance or further holds it, so a step down is not new.
 */
export function holds(failures, f) {
  const o = over(f);
  if (!o) return failures.includes(f);
  return failures.some(g => { const p = over(g); return p?.row === o.row && p.steps >= o.steps; });
}

/**
 * The failures in `now` that `before` does not hold. A byte-budget row names each step it rose since `before`, so
 * each step is attributed to the commit that took it.
 */
export function newFailures(before, now) {
  return now.flatMap(f => {
    const o = over(f);
    if (!o) return before.includes(f) ? [] : [f];
    const from = Math.max(0, ...before.map(over).filter(p => p?.row === o.row).map(p => p.steps));
    return Array.from({ length: Math.max(0, o.steps - from) }, (_, i) => `${o.row} by ${((from + i + 1) * STEP).toFixed(1)} MB`);
  });
}

/** The same failure names at a probe: other compile failures, timeouts and omitted steps obstruct attribution. */
export function probeOutcomes(result, wanted) {
  return Object.fromEntries(wanted.map(f => {
    const check = result.checks[f.split(': ', 1)[0]], o = over(f);
    if (holds(result.failures, f)) return [f, 'fail'];
    // The row over its ceiling by a distance it did not name says nothing of this step.
    if (o && result.failures.includes(o.row)) return [f, 'unknown'];
    return [f, check && !check.skipped && check.complete ? 'pass' : 'unknown'];
  }));
}

/** Confirm the actual parent after pruning unrelated commits; a missed input cannot assign blame. */
export async function bisectAffected(base, tip, failures, candidates, probe, parentOf) {
  const cache = new Map(), out = {};
  const at = async sha => {
    if (!cache.has(sha)) cache.set(sha, await probe(sha, failures));
    return cache.get(sha);
  };
  for (const failure of failures) {
    const relevant = candidates.get(failure) ?? [];
    if (!relevant.length) {
      out[failure] = { unresolved: 'no commit in the range changes a known input of this failure', good: base, bad: tip };
      continue;
    }
    const commits = [...new Set([base, ...relevant, tip])];
    let found = (await bisectFailures(commits, [failure], at))[failure];
    if (found.commit) {
      const parent = await parentOf(found.commit);
      if (!relevant.includes(found.commit) || (await at(parent))[failure] !== 'pass') {
        found = { unresolved: 'the actual parent did not confirm the affected-commit boundary', good: base, bad: found.commit };
      } else found.parent = parent;
    }
    out[failure] = found;
  }
  return out;
}
