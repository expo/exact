surfaces: web, macos, ios
agent: Claude
task: move the scan screen's detection and lookup out of the root (LLP 1035.005.004 piece 7, #476)

### Rough
The scan screen's `skipped`, `scan`, `lookup` and `isbn` lived in the root,
so the barcode detection and the ISBN lookup outlived the screen: Cancel
cleared `scanTrack`, and the root's requests idled on it rather than ending.

### Lean in
`Scan` declares `skipped`, `scan`, `isbn`, `lookup` and `tryAgain` itself.
The camera (`opened`, `scanTrack`, `stopped`) stays the root's and reaches
`Scan` as `track`; Cancel is the root's `endScan`, passed as `cancel`, and
Use This Book calls the root's `useBook(found)` with the lookup's answer.
A new scan screen starts with no ISBNs passed over, so `openScan` no longer
clears them.

### Needed
- a child's own requests: provided by exact2 (LLP 1035.005.004)

### Checkpoints
- `expect perf query lookup` counts 1 when the scan screen opens and 0
  across Cancel.
