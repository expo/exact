# A changed topic asks every resource of any source that ever watched it, on the JS target

**Status:** Closed
**Resolution:** An answer's watched topics are kept on its call and taken by the resource it asked for, replaced at each fresh ask (a failed ask's added), so `announce` asks only the resources whose current answers watch the topic.
**Systems:** host/web-js/ts-data.js, runner/src/runner/settlement.rs, runner/src/runner/time.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A TypeScript answer watches a topic with `native.watch(topic)` or `storage.fs.watch(path)`. LLP 1016.002 says a
changed topic asks again "exactly the resources whose current answers watch it", and that "a fresh answer replaces
the resource's topics" (D2).

At `dd6f5dcd` the JS target records the answer's *source* under the topic (`host/web-js/ts-data.js:182, 195`:
`watched.get(topic).add(watching)`, where `watching` is the source's name), and nothing removes an entry. When the
topic changes, `announce` asks again every resource whose source is in that set (`ts-data.js:194`). So it asks
(a) every other resource of the same source, whatever its own answer watched, and (b) a resource whose later
answers stopped watching the topic. The `watched` map only grows: a reader that has opened a hundred documents is
asked again when any of the hundred changes.

The runner keeps topics by resource and replaces them at each fresh answer
(`runner/src/runner/settlement.rs:864-866`), and `changed` asks only the resources whose answer watches the topic
(`runner/src/runner/time.rs:260-266`).

## The failure

```
component App
  state path = "a"
  resource open = read(path) as shape string
  resource pinned = read("p") as shape string
  action go(p: string)
    path = p
  view
    column
      text open testId="open"
      text pinned testId="pinned"
      button press=go("b") testId="b"
        text "b"
```

`read(x)` watches the topic `x` and answers. Boot, press `b` (so `open` reads and watches `b`), announce topic `a`,
then topic `p`.

| | topic `a` changed | topic `p` changed |
|---|---|---|
| Runner | no commit, nothing asked | `read(p)` asked |
| JS target | `open` and `pinned` both asked | `open` and `pinned` both asked |

Each extra ask runs the source again. One that fetches sends a request, and a resource that answers later shows
`pending` meanwhile.

## Verification

- The plan on the runner, its `DataSource` calling `store.observe_topic(args[0])`, then `Runner::changed`: `topic a
  changed: a commit false; asked again []`, `topic p changed: a commit true; asked again ["read(p)"]`.
- The plan built with `bun host/web-js/build.mjs` beside an `app.ts` whose `read` calls `native.watch(args[0])` and
  numbers its answers, and a page module (`modules/web/index.js`) whose `connect({ changed })` hands the test
  `changed`; opened in Chrome:

  ```
  booted: read a #1 | read p #2
  after go("b"): read b #3 | read p #2
  after topic a changed: read b #4 | read p #5
  after topic p changed: read b #6 | read p #7
  ```

`storage.fs.watch` was not run.

## Direction

Keep the topics on the resource and replace them at each fresh answer: set `r.watches` from the answer that settles
it (`rt.js` `res` knows the resource; `ts-data.js` knows the call), and have `announce` test the resource's own
topics. `host/web-js/modules.js:1007` already reads `r.watches`, which nothing sets.

## Related

`issues/closed/20260927-watching-not-rolled-back.md` fixed the runner's topics across a refused commit. Nothing in
`QUEUE.md` or `docs/agent-pitfalls.md` covers the JS side.
