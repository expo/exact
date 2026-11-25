# Mixed::start_delivery sets set.busy without set.running, permanently deadlocking the ordered set on JS retirement, discard, or route error

**Status:** Closed
**Resolution:** Track active kept-reply deliveries in Mixed Set::running as Running::Delivery and clear busy/running and delivery state on JS retirement, discard, route error, or missing child dispatch.
**Systems:** exact-data (Mixed composer ordered sets and kept-reply module delivery)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Back doors)

Verification: verified by reading.

At `9c0431fd5`, `Mixed::start_delivery` (`data/src/mixed.rs:539-559`) reserves the JavaScript child's ordered set by setting `self.sets[set].busy = true` (`data/src/mixed.rs:549`) without recording anything in `self.sets[set].running` (leaving `running == None`).

### Reachable trigger
In any mixed TypeScript + Rust app where the JavaScript child belongs to an ordered set (`set_of[0] == Some(set)` — either JavaScript is worker-placed, or Rust is worker-placed and shares a `secret.keep` grant with JavaScript; `data/src/mixed.rs:215-242`):
1. A JavaScript request with effects is let go by a commit and kept via `Runner::keep_sent` (`runner/src/runner/module_work.rs:296-330`, carrying the JS child's `ModuleInstance` captured in `runner/src/runner/commit.rs:1077-1080` and `module_work.rs:239`).
2. When its reply lands, `Runner::module_work_landed` (`runner/src/runner/module_work.rs:334-379`) calls `Mixed::module_delivery` (`data/src/mixed.rs:1256-1302`), which queues an `OrderedTurn` continuation (`delivery_phases.insert(token, false)`).
3. The host dispatches that continuation into `Mixed::dispatch` -> `Mixed::start_delivery` (`data/src/mixed.rs:826-828, 539-559`), which sets `self.sets[set].busy = true` while leaving `self.sets[set].running == None`.
4. If the JavaScript module retires on OOM before delivery finishes — either while a worker-placed JS child executes `Job::KeptReply` (after phase `Some(false)` routes the worker continuation at `data/src/mixed.rs:1293-1296`), or inline during `start_delivery` on a main-placed JS child before `fulfill` reaches `module_delivery` — `Runner::collect_aborts` calls `Mixed::take_aborted` -> `Mixed::sync_generations` (`data/src/mixed/turns.rs:5-35`) and `Runner::match_failed_requests` (`runner/src/runner/retirement.rs:48`) flags the `module_work` ticket as failed, after which `Runner::executor_failed` (`runner/src/runner/retirement.rs:86`) drops it via `drop_module_work` without calling `Mixed::module_delivery` again.

Two sibling paths in `Mixed` leak `set.busy = true` the same way:
- `Mixed::module_delivery` (`data/src/mixed.rs:1292-1296`): if `self.route_delivery(Some(request))` returns `Err(_)` (missing continuation at line 567 or token exhaustion at line 568), `self.sets[set].busy` is left `true` (and `delivery_phases` keeps `(token, true)`) while `module_work_landed` (`runner/src/runner/module_work.rs:372`) logs the error and drops the ticket.
- `Mixed::discard` (`data/src/mixed.rs:1018-1065`): checks only `set.running`, `recorded`, and `continuations`, ignoring `deliveries`, `delivery_waiters`, `delivery_routes`, and in-flight delivery state.

### Actual vs. expected
- **Actual:** `Mixed::sync_generations` (`data/src/mixed/turns.rs:19-31`) resets `set.busy = false` only when `set.running.as_ref().is_some_and(|r| r.key.0 == rust)`, which is `false` because `set.running` is `None`. It then clears `deliveries`, `delivery_waiters`, `delivery_phases`, and `delivery_routes`, leaving `self.sets[set].busy == true` with no path able to clear it. Every subsequent turn in that ordered set (both JS and any Rust child sharing `sets[0]`) hits `if self.sets[set].busy` in `Mixed::dispatch` (`data/src/mixed.rs:842`) and returns `Dispatch::Held` forever, while `Mixed::release` (`data/src/mixed.rs:871, 876`) skips the set forever.
- **Expected:** Generation retirement, delivery discard, or delivery routing failure must clear `self.sets[set].busy = false` so the ordered set unblocks for the next generation and for sibling members of the set, matching `Placed`'s ordered-set invariant (`data/src/placed/order.rs:9-12, 75-84, 198-202` and `data/src/placed/order_tests.rs:112-168`).

### Verification & sibling sweep
Verified by reading the code path from `Runner::keep_sent` / `module_work_landed` / `retirement.rs` through `Mixed::module_delivery`, `start_delivery`, `sync_generations`, `discard`, `dispatch`, and `release`.
- **Siblings checked:**
  - `Placed` (`data/src/placed/order.rs:9-12, 75-84, 198-202`, `data/src/placed/worker.rs:16-33`, `data/src/placed.rs:1050-1058, 1180-1217`): already models `Running::Delivery { kept }`, clears it in `clear_order()` on `sync_generation()`, handles delivery discard in `Placed::discard`, and releases `running` before `envelope::apply` in `module_delivery`.
  - `Storage` (`data/host/src/lib.rs:425-540`), `Swappable` (`logic/src/lib.rs:533`), `configured!` (`logic/src/configured.rs:209`), and `host/render/src/source.rs:295`: pass-through wrappers with no ordered-set `busy` lock.

### Suggested fix & regression test
Track an active delivery explicitly in `Set::running` (e.g., an enum `Running::Turn { ... } | Running::Delivery { kept, outer, child }`, mirroring `data/src/placed/order.rs:9-12`), clear it on JS retirement in `sync_generations` (`data/src/mixed/turns.rs:19-31`), clear `set.busy` and `set.running` (and clean up `deliveries`/`delivery_waiters`/`delivery_routes`/`delivery_phases`, forwarding `discard` to `self.javascript`) in `Mixed::discard` (`data/src/mixed.rs:1018-1065`), and reset `set.busy`/`set.running`/`delivery_phases` when `route_delivery` returns `Err(_)` (`data/src/mixed.rs:1293-1296`). Add unit tests in `data/src/mixed/turns.rs` (or `data/src/mixed_tests/`) covering: (1) JS retirement during an in-flight ordered `module_delivery` unblocking subsequent JS and Rust turns in the same set, (2) discarding an in-flight/queued delivery token, and (3) a `route_delivery` error clearing `set.busy`.

### Siblings checked

Placed (data/src/placed/order.rs, data/src/placed/worker.rs, data/src/placed.rs — already tracks Running::Delivery and clears on sync_generation and discard); Storage (data/host/src/lib.rs — pass-through, no ordered-set lock); Swappable (logic/src/lib.rs), configured! (logic/src/configured.rs), host/render/src/source.rs (pass-throughs).
