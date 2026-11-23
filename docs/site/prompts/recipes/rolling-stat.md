```prompt Rotate a stat and pause it for accessibility
Add a stat that rotates on a timer to the Exact app in this folder, and keep it still for a screen reader, large text, reduced motion or the person's own setting. Use https://github.com/expo/exact/blob/main/docs/recipes/rolling-stat.md as the source for each step. The Exact checkout has the same page at docs/recipes/rolling-stat.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say which stats rotate and on which screen, propose them from what the app does and ask me before you write code.
3. In app.contract, read `exactViewport()` as a resource with a `shape Viewport` that has `screenReader`, `accessibilityTextSize` and `prefersReducedMotion`. Keep the person's gentler-motion setting in a state (`gentlerMotion`) with a button that shows it with `aria-pressed`.
4. Derive `rotates`: true only while `viewport.screenReader == "off"` and the three other conditions are false. `"unknown"` is possibly on, so require `"off"`; do not test for "on". Do not compute the large-text condition from a font size the app sets.
5. Gate the timer itself: `task rotate when rotates` with `every(3000, next)`. Do not let the timer run and hide the change.
6. Add a Next button that runs the same action (`next`), so every stat stays available while the rotation is still. Give the stat `font-variant-numeric="tabular-nums"`.
7. Add a test in app.test.contract that presses the Next button and expects the next stat, then presses it until the first stat returns. The recipe has no test to copy.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me which stats rotate and what stops the rotation, what the tests and checks reported, and the launch command for each target platform. Browsers cannot detect screen readers, so tell me that the stat does not rotate on the web and that the Next button is the way to advance there.
```
