```prompt Fix a function Contract does not have
Make each expression in app.contract of the Exact app in this folder call only the standard functions Contract has. Use https://github.com/expo/exact/blob/main/docs/recipes/functions.md as the source for each step. The Exact checkout has the same page at docs/recipes/functions.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not name the function or the expression, ask me for it, or for the refusal I saw.
3. Look up each function: `bun exact.mjs contract vocab <name>` has its rules, or what to write for a name Contract lacks. Call the web's methods on their value (`s.trim()`, `xs.length`) and the rest as functions (`first(xs)`, `sum(xs)`, `sortBy(xs, x => key)`). `first(xs)`, `xs.at(i)` and `parseNumber(s)` return an `option`, not a value.
4. Compose what the web has and Contract lacks. With `p` a predicate: `find` is `first(xs.filter(p))`, `some` is `xs.map(p).includes(true)`, `every` is `not xs.map(p).includes(false)`, and `findIndex` is `xs.map(p).indexOf(true)`.
5. Compute grouping, folds and other maths (`reduce`, `pow`, `abs`) in the data module, app.ts, not in an expression.
6. Write a count's noun after the number, `${n} ${n == 1 ? "book" : "books"}`, never "1 books". On screen, write upper case as `text-transform="uppercase"`; `s.toUpperCase()` is for a string sent elsewhere. Write a host command (`focus(id)`, `showModal(id)`, `copyText(s)`, `openURL(url)`) as a statement, not in an expression.
7. Keep the existing tests passing without changing what they check. If a fix changes what a screen shows, add a test in app.test.contract that expects the new text.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me each expression you changed and what it calls now, what you moved to app.ts, what the tests and checks reported, and the launch command for each target platform.
```
