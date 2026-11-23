```prompt Add a timer to my app
Add a timer to the Exact app in this folder: work that repeats, fires once, or counts down to an end. Use https://github.com/expo/exact/blob/main/docs/recipes/timers.md as the source for each step. The Exact checkout has the same page at docs/recipes/timers.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say what the timer does, what starts and stops it, and which screen shows it, propose it and ask me before you write code. If the timer must keep its time when the app quits, tell me and use docs/recipes/timer-that-survives-relaunch.md.
3. In app.contract, declare a `task`: it is the only timer. Put it in the root, or in a child at a bounded use as docs/recipes/timer-in-a-component.md describes. Inside it, `every` repeats (the recipe's `every(250, step)`) and `after(ms, action)` fires once.
4. Make start and stop a state: `when cond` arms the task while the condition holds. `key=x` restarts a task when `x` changes.
5. The task's action writes `performanceNow()` into a state: a derive does not update as time passes. Compute the time left from the end (`endsAt - now`). Do not count ticks: the count drifts.
6. Show a readout that changes in place with `font-variant-numeric="tabular-nums"`. For work that can wait, add the recipe's options: `wait=scrollend` with `max=1000`, `wait=idle`, `wait=frame`, `wait=update`, or `tolerance=500` to coalesce wakes.
7. Add a test in app.test.contract like the recipe's. A test's `clock +N` fires the timers it passes: start the timer, move the clock, expect the readout, move the clock past the end, and expect the end state.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me what the timer does and which state starts and stops it, what the tests and checks reported, and the launch command for each target platform. `performanceNow()` restarts at launch, so tell me that this timer does not survive a quit.
```
