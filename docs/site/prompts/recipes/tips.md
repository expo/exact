```prompt Add a tip that teaches a control
Add a tip to the Exact app in this folder: it names a control's use the first time the person sees it, and goes away for good once they have used it. Use https://github.com/expo/exact/blob/main/docs/recipes/tips.md as the source for each step. The Exact checkout has the same page at docs/recipes/tips.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say which control each tip teaches, its words, and what the person does to end its lesson, propose them and ask me before you write code. Tips show on iOS, iPadOS and macOS only (TipKit).
3. In app.contract, give each tipped element an `id` that stays the same, and `tip` with the title. The build refuses a `tip` without an `id`. TipKit remembers the tip by the id across launches and languages: do not change the id later.
4. Add `tipMessage` for the line under the title, and `tipImage` for a symbol beside it (`symbol:<role>` or `symbol:sf/<name>`), where they help. Write the words with `t()` to translate them.
5. Bind `tipDone` to a value that turns true when the person has done what the tip teaches. Once true, the tip is retired for good, even when the value turns false again or the app relaunches. Without `tipDone`, the tip stays until the person closes it.
6. Do not decide when a tip shows: the platform does. To hold a tip back (after the third launch, once a list has rows), bind `tip` to `""` until then. The web, Android, Linux and Windows show no tip, so put anything essential in the screen itself.
7. Add a test in app.test.contract that uses each tipped control and expects the state its `tipDone` reads.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me each tip, its element's `id` and what ends its lesson, what the tests and checks reported, and how to see the tips: the driver's `state` lists each tip under `tips`. Soon after launch a tip may still be `pending`. A drive without `--storage` starts having seen no tip, and `--reset-storage` forgets what a named store remembers.
```
