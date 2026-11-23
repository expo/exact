```prompt Show dates and days in my app
Make the Exact app in this folder read the date, and keep each day as a key in the time zone of the person who uses the app. Use https://github.com/expo/exact/blob/main/docs/recipes/dates-and-days.md as the source for each step. The Exact checkout has the same page at docs/recipes/dates-and-days.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say what the app dates and which days it keeps (a habit log, a due date, entries by day), propose it from what the app does and ask me before you write code.
3. In app.contract, read `exactTime()` as a resource with a `shape Time` that has `epochAtZero` and `utcOffset`. The date is `time.epochAtZero + performanceNow()`.
4. Keep the time in a state that a `task` writes, as the recipe does with `every(60000, step)`. A derive of the clock does not update by itself, and `performanceNow()` is 0 at each launch.
5. Keep a day as the key `formatDate(shown, time.utcOffset, "iso")`, and compare keys as text. An action that writes a day reads the clock itself: it takes `let at = time.epochAtZero + performanceNow()` and keys `formatDate(at, time.utcOffset, "iso")`. Do not key the entry by the ticked `now`: it can lag a minute, and an entry at midnight lands on yesterday.
6. Write a day's label with `formatDay`, never with a comparison such as `day == today`. Its modes are `"day"`, `"day-time"`, `"list"` and `"section"`. Label an `input type="date"` value with `formatDate(due, 0, "medium")`.
7. Add the recipe's two tests to app.test.contract, each with `epoch` and `time-zone` pinned. One moves the clock past local midnight (`clock +3600000`) and expects the new day. One writes an entry 30 s after midnight, before the tick, and expects the new day's key.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me which dates and days the app keeps, what the tests and checks reported, and the launch command for each target platform. `utcOffset` is the zone's offset now: tell me each label that can miss an hour across a DST change.
```
