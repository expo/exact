surfaces: linux, android
agent: Claude Code (Claude Opus 5.5)
task: the Player and Flight screens' `portal` video on the painted hosts (LLP 1013.002 piece 5, #698)

### Rough

- A tap on a button inside the moved player ran the shown card's `pointerdown` and `press`,
  not the declaration's: the painted listener walks climbed the shown tree. Fixed in exact2
  (`Presenter::listener_parent`).
- A focused button in a player that hid kept the focus. Fixed in exact2: it blurs once.
- On Android the hidden player is released and made again when shown: `loads` goes 1 → 2
  (QUEUE).

### Lean in

- Moves keep the same node and, on Android, the same player: nothing in the app changed.

### Needed

- Android holding a hidden mover's player; Linux draws no shared-element flights (by design).

### Checkpoints

- Linux (headless, this Mac): Count, Open, Back 60 ms in, Open, Count gives Count 2 with one
  player node; a tab without the card leaves the player out of the tree. No video decoder,
  and the Flight screen's flier appears in place: Linux draws no flights.
- Android (emulator, API 36.1, the reader APK): the same round trip with the system Back
  60 ms in gives Count 2, `loads` 1, one video (generation 1), `currentTime` 0.43, 0.76, 0.90,
  1.31 s. Another tab and back: `loads` 2, a new player.
