// The TypeScript seam (ts-data.js and what it brings), loaded at a page's first ask (LLP 1047.003 D6). Until it is
// installed an ask is not ready: the resource keeps what its page showed and waits through `data.ready`, as one a
// Rust module answers does (rt.js `data`), and `install` asks it again. Counted in flight, so `clock settle` waits for
// it. A seam that does not load is said once in the journal, and each source then fails when asked, keeping its value
// (LLP 1071 §7).
export function seam(data, inflight, journal, clock) {
  let loading = null;
  const start = () => loading ??= (inflight.n++, import('./ts-data.js').then(m => m.install(data)).catch(e => {
    const why = `the TypeScript module did not load: ${e?.message ?? e}`;
    journal.push(`t=${clock.now} ${why}`);
    data.answer = () => { throw new Error(why); };
    for (const f of data.q.splice(0)) f();
  }).finally(() => inflight.n--));
  data.answer = () => { start(); return null; };
  return start;
}
