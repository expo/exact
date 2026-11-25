```prompt Draw a voice note's waveform and a bubble's tail
Make the Exact app in this folder draw a voice note's waveform and a chat bubble's tail, each as one `svg`. Use https://github.com/expo/exact/blob/main/docs/recipes/waveform-and-tail.md as the source for each step. The Exact checkout has the same page at docs/recipes/waveform-and-tail.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say which rows get a waveform, a tail or both, and where a note's levels come from, propose it from the app and ask me before you write code. Do not invent levels the app does not have.
3. In app.ts, shape each waveform once, when the note loads: one path string with one bar for each level (0 to 1), as the recipe's `wave` function writes it. Return the string in the note's data. The view does no arithmetic.
4. In app.contract, draw the waveform as one `svg` with a `viewBox`, a fixed `width` and `height`, `aria-hidden="true"`, and one `path` whose `d` is the note's string. Never draw a box for each bar: one `path` is one node to lay out and paint, where 40 boxes are 40.
5. Draw the tail as a second `svg` beside the bubble, not over it. Make the bubble's corner on that side nearly square (`4px` in its `border-radius`), so the two meet without an overlap: `-exact-fill` can be translucent, and an overlap draws darker.
6. For the other person's bubbles, write the `svg` before the bubble, make the small corner the bottom-left one, and put `transform="translate(8, 0) scale(-1, 1)"` on the `path`. Give each `fill` a role (`-exact-fill`, `-exact-secondary-label`), not a hex colour, so it follows dark mode.
7. Add a test in app.test.contract like the recipe's: after `clock data`, `expect tree has` for a note's waveform and for its tail, and `expect text` for its duration.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me which rows draw a waveform or a tail, where the levels come from, what the tests and checks reported, and the launch command for each target platform.
```
