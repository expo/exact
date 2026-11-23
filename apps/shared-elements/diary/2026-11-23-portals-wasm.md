surfaces: web
agent: Claude Code (Claude Opus 5.5)
task: the Player screen's video plays through the move on the wasm web host (`portal`, LLP 1013.002 D6, #698)

### Rough

- On the wasm host the moved player existed but never played: a mover hidden at boot was never
  in the document, and the media glue installs an element only once it is connected, so its
  `muted` and `autoplay` never applied. Fixed in exact2: the host names each mover whose shown
  parent changes (`hold`), and the page keeps it in a connected, `display: none`, inert holder
  until a list shows it, as the JS target does.
- The agent drives whichever web build is in `host/web/dist`; `bun host/web/build.mjs
  shared-elements-web --wasm` makes the wasm one.

### Lean in

- Chrome's own off-screen rule pauses the held video and resumes it; nothing in the app changed.

### Needed

- Nothing for this.

### Checkpoints

- Google Chrome, wasm host: Count, Open, Back 60 ms in, Open, Count keeps one element with no
  `loadstart` and no seek, `currentTime` 0.41, 0.82, 1.12, 1.49 s, Count 2. On the Grid tab it is
  in the holder, paused at 1.76 s, not focusable, not visible and outside `#exact-root`; back on
  Player it plays on from 1.85 s (2.86 s a second later), `loads` 1. Remove, then New: a new
  element, Count 0.
- The same with `moveBefore` removed (`insertBefore`): 0.30, 0.71, 1.02, 1.38 s, paused at
  1.66 s while hidden, resumed, `loads` 1.
