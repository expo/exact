---
name: exact-performance
description: Make an Exact (expo/exact) app fast and keep it fast — launch time, long lists, taps that lag, scroll hitches, work at rest, slow data sources. Use when an Exact app feels slow or janky, when adding a list of more than a few hundred rows, a timer, polling, a large JSON or catalog, or a costly computed value, and before claiming an Exact app performs well. Covers how to measure with the agent's `perf` operations and which pattern removes each cost.
---

# exact-performance

Measure, change one thing, measure again. Each rule below removes a cost that was
measured in a real app; `<exact>/docs/agent-pitfalls.md` has each one's evidence.
Commands run from the app's directory; `<exact>` is the Exact checkout the app uses.

## Measure

```sh
bun exact.mjs agent web 'perf explorer during "type search dra"'   # work one operation caused in a subtree
bun exact.mjs agent web "perf frames live 3000"                    # frames presented over 3 s of wall time
bun exact.mjs agent ios logs                                       # names an app.ts top level over 50 ms, a big answer
bun exact.mjs verify ios                                           # scrolls a long first-screen list by real touches
```

- `perf <target> during "<op>"` reports, per plan site, evaluations, unchanged results,
  and row instances created and retired. Many evaluations with unchanged results mean a
  binding depends on more than it reads.
- Under the driver's clock no frame is presented. Frame timing needs `perf frames live`.
- A missing measurement is not a zero, and a web number is not a phone number. Hermes on
  an iPhone ran a catalog map in 314 ms that Bun ran in 60 ms. Measure on the host you
  ship.
- The app's `.exact/commands.jsonl` records each `exact.mjs` command's exit and elapsed
  milliseconds.
- `bun exact.mjs docs inspection and testing` prints the frame-health and `perf frames`
  reference.

## Lists

- **Virtualize a long list.** `list virtualized=true`, one direct keyed `for`, one flow
  root per row, and a bound (`flex=1 min-height=0`, `height` or `max-height`). Add
  `estimated-item-height` when rows vary.
- **Window a very long one.** A mutation or resource that answers the whole list is
  copied and checked row by row: a tap on one row of 10,000 took about 80 ms on the web.
  Answer at most about 200 rows from a cursor (`resource feed = feed(cursor)`), move the
  cursor from the list's `reachstart` and `reachend`, and give writes `refreshes feed`:
  16 to 24 ms. A development build logs `big answer` past 2,000 rows.
- **Join in the data module.** `users.filter(u => u.id == p.userId)` inside
  `for p in posts` walks posts × users on every update (`lower-row-lookup`). Have the
  source give each row the field it shows. The same holds for a row that walks its own
  list (`lower-row-own-list`).
- **Pass a helper the fields it reads.** `weight(run)` depends on every field of the row;
  `weight(run.bold, run.dim)` lets unrelated changes skip the binding.
- **A hidden list still builds its rows.** An ordinary `for` inside a closed dialog
  instantiates every child. Gate the rows with `if`, or virtualize.
- **Open a feed at its end** with `scroll-start="end"` (and `scrollFollowEnd=true` for a
  transcript), not a `scrollTop` write.
- **Page from `reachend`, not `scroll`.** A `scroll=` handler runs an action per scroll
  frame. Remove ones left from experiments.

## Work at rest

An app that commits visible state on a timer makes the host run its whole post-apply pass
every tick.

- Write state only when it changes: a clock shown to the minute ticks by the minute.
- Gate a task on the state that needs it, so an idle app commits nothing:

  ```text
  task poll when inFlight
    every(200, tick)
  task hide when toast != "" key=toastUntil
    after(5000, clear)
  ```

- Do not run an always-on `every` that checks whether something expired; use a gated
  `after`. Its action clears unconditionally and does not re-test the time.
- Motion is the platform's: `transition`, `keyframes`, `-exact-layout-transition`
  (`vocab motion`). Reserve `every(frame, …)` for what cannot be expressed that way, gated
  (`task fly when flying`).
- Measure time with `performanceNow()`, never by counting ticks: a suspended app fires a
  missed repeat once.

## Data sources

- **Keep `app.ts`'s top level cheap.** It runs after first pixel and every answer waits
  for it. Shape data inside the source that answers it, or once on its first call, and
  check that first call's cost too. If it cannot be made cheap,
  `"typescript": {"placement": "worker"}` in `app.json` moves it to its own thread;
  answers then arrive later, outside the action's transaction.
- **Each turn of an answer has 100 ms on native hosts.** A turn runs from its call, or
  from the `fetch` or storage step it awaited, to its next await. A longer one is refused
  as `Unavailable`, so a parse that passes on small data fails on large data. Yield:

  ```ts
  for (let i = 0; i < nodes.length; i++) {
    if (i % 2000 === 1999) await scheduler.yield();
    out.push(toRow(nodes[i]));
  }
  ```

- **A root `resource` is asked at launch and again whenever an argument changes.** One
  that only a later screen shows (a QR code, a share card) should take state that screen
  sets when it opens (`state sharing: option<Pattern> = none`), with the source answering
  empty for `none`.
- **Poll cheaply.** Send the last revision and let the service answer `unchanged`; return
  the previous answer object so nothing rebinds. Poll fast only while something is in
  flight.
- **Reads queue one at a time per source.** A screen that reads many feeds waits for each
  in turn. An order-independent read opts out with
  `fetch(url, { exactIndependentHttp: { maxResponseBytes: 4194304 } })`; a native app runs
  up to six at once. Leave writes in the queue.
- **Write no cache of your own.** `fetch` honours the API's `Cache-Control`
  (`vocab cache`).
- **Bound a slow server in the source**, with an abort signal, not in the view.
- **Rust sources:** parse or build data in `DataSource::activate`, which runs after first
  pixel, not on first use mid-scroll.

## Launch and build

- A native view or module that loads at launch slows it. Drawing (rings, glows, charts,
  step strips) is boxes, SVG and keyframes in Contract; keep native code for platform APIs,
  created on first use.
- `else empty()` on a resource gives the first frame a value, so launch does not wait for
  data. Show a spinner only while nothing is loaded; a refresh keeps the last answer.
- A `.ts` file's imports are staged for the type check. Importing a large package's types
  into the data module's graph can exceed the 64 MiB bound: declare the two fields you
  read instead.
- Machine caches make later builds seconds; a first native build is minutes. Run
  `bun exact.mjs ios --prebuild` in the background while writing the app.

## Before saying it is fast

State the host, the build kind (development or release), the data size and the number.
A development build's timing does not establish a release build's, and callback timing
does not prove what the compositor presented.
