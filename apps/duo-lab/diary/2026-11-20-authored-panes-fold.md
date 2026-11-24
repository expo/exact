surfaces: ios, web
agent: Claude Code (Claude Opus 5.5)
task: Keep the authored panes clear of a book fold on an iPhone Duo.

### Rough

On a partially folded Duo the Panes tab's authored panes place themselves by the viewport segments, inside the route's content, which the route's clearance insets by the 84-point side strip and the 34-point home indicator. So the right-hand pane, aligned to the container's end, began at 411.67 and crossed the fold at 455.5 to 495.5. Finding that took about twenty minutes, mostly reading the smoke's check and the layout; a log line printing the boxes found it in one run (Lean in). On a book fold with one row of segments the route now turns its side clearance off (`-exact-safe-area="left right"`), so the container spans the viewport's width as the segments do.

In that pose the panes pad their sides by the route's insets (`var(--exact-safe-area-inset-left)` and `-right`), as an agent's explicit size zeros the page's (`env()`) while UIKit still insets the route: under `951x669`, `env()` would end the right-hand pane's draft field 68 points under the strip. Finding that took about twenty minutes with a drive at both sizes; a line in `--size`'s help saying that it zeros the page's insets and not the routes' would have saved it (`QUEUE.md`).

A long unbroken word in the draft's echo overflowed its pane, so the echo now wraps it anywhere (`overflow-wrap="anywhere"`). That took about an hour: `break-word` can draw a dictionary hyphen on iOS, `hyphens="none"` with it drops soft hyphens, clipping the echo hides the rest of the word, and wrapping only on a book fold loses the pane's horizontal position across a fold. `docs/agent-pitfalls.md` gives the fix ("A long unbroken word runs off its box"); searching it for the symptom first would have saved the tries.

The new binding made the web runtime journal `unset safe-area: "left right" is not a value it takes` (a few minutes to trace): the web emitter wrote the row as a CSS property, which the browser refuses. The emitter now writes nothing for it, and the browser test reads the journal, as it had passed listening only for page errors. A test helper that fails on refused styles would have caught it at once.

### Lean in

One log line in the smoke's fold check, with each pane's box, the container's and the segments, showed the cause in one run.

### Needed

- foldable segments: provided, viewport-relative only
- owner-relative segments: missing (neither iOS nor the web publishes them; the panes would size themselves by them in the route's cleared content)
- safe-area insets: provided (the viewport's, and the route's with `var(--exact-safe-area-inset-*)`)
- a box's own safe area: missing (both panes pad both sides by the route's insets, so the one away from the strip keeps 84 points it does not need; `QUEUE.md`)

### Checkpoints

- editing app.contract / app.ts: rough (see Rough: the echo's wrap)
- iOS build: smooth
- driving the app (`exact.mjs agent`, `exact.mjs test`): rough (see Rough: the fold's cause and the explicit size)
