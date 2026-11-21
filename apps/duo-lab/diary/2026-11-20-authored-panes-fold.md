surfaces: ios, web
agent: Claude Code (Claude Opus 5.5)
task: Keep the authored panes clear of a book fold on an iPhone Duo.

### Rough

On a partially folded Duo the Panes tab's authored layout places its two panes by the viewport segments, inside a container (`panes-container`) in the route's content. The route's clearance (main's model, which LLP 1123 replaces at activation) insets that content by the open Duo's 84-point side strip and the 34-point home indicator, so the container was 867 by 635 points, and the right-hand pane, aligned to the container's end, began at 411.67 and crossed the fold, which lies from 455.5 to 495.5. Finding that took about twenty minutes, most of it reading the smoke's check and the layout; a log line printing the boxes found it in one run (Lean in). In Authored mode on a book fold with one row of segments the route now turns its side clearance off (`-exact-safe-area="left right"`), so the container spans the viewport's width as the segments do, and the right-hand pane begins at 495.67. Its top and bottom stay cleared.

In that pose the panes pad their sides by the route's insets (`var(--exact-safe-area-inset-left)` and `-right`), which keep their content clear of the strip; in the other anchored layouts they keep main's `env()` padding. `env(safe-area-inset-*)` reads the same at the device's size, but an agent's explicit size zeros it while UIKit still insets the route: under `951x669` the right-hand pane's draft field ends at 851, 16 points short of the strip, where with `env()` it would end 68 points under it. Finding that took about twenty minutes with a drive at both sizes; a line in `--size`'s help saying that it zeros the page's insets and not the routes' would have saved it (`QUEUE.md`).

On a book fold only the pane's padding keeps a long unbroken word in the draft's echo off the strip, so the echo now wraps such a word anywhere (`overflow-wrap="anywhere"`), in every layout, so that a long word stays inside its pane in any pose. That took about an hour over several tries: `break-word` can draw a dictionary hyphen on iOS, `hyphens="none"` with it drops soft hyphens, clipping the echo hides the rest of the word, and wrapping only on a book fold loses the pane's horizontal position across a fold. `docs/agent-pitfalls.md` gives the fix ("A long unbroken word runs off its box"); searching the pitfalls for the symptom first would have saved the tries.

At the device's size both panes pad their right edge by 16 points and the strip's 84, as before, so the pane away from the strip keeps 84 points it does not need. In a right-to-left document the panes swap sides through flexbox alignment and their padding does not; on iOS Duo Lab's document stays left to right, as the app has no strings table. To pad only where the strip overlaps it, a pane would need its own safe area, and a box can read only its route's (`QUEUE.md`).

On a book fold the new binding made the web runtime journal `unset safe-area: "left right" is not a value it takes` (a few minutes to trace): the web emitter wrote the row as a CSS `safe-area` property, which the browser refuses. The web clears no route for the row to turn off, so the emitter now writes nothing for it, and the browser test checks the journal for refused styles, which the runtime journals without throwing. The browser test had passed with the refusal, as it listened only for page errors; a test helper that fails on refused styles would have caught it at once.

### Lean in

One log line in the smoke's fold check, with each pane's box, the container's and the segments, showed the cause in one run.

### Needed

- foldable segments: provided, viewport-relative only
- owner-relative segments: missing (iOS publishes the session's viewport segments; its sampler of an owner's segments is called only by tests, so nothing publishes them yet; the web has none; the panes would size themselves by them in the route's cleared content)
- safe-area insets: provided (the viewport's, and the route's with `var(--exact-safe-area-inset-*)`)
- a box's own safe area: missing (both panes pad by the route's side insets, see Rough)

### Checkpoints

- editing app.contract / app.ts: rough (see Rough: the echo's wrap)
- iOS build: smooth
- driving the app (`exact.mjs agent`, `exact.mjs test`): rough (see Rough: the fold's cause and the explicit size)
