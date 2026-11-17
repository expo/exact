surfaces: none (the data source, by its Bun test)
agent: Claude Code, Claude Opus 5.5
task: answer the recents strip's empty list in `PhotoAssetList`'s new shape

### Rough

`photoAssets` now answers a `cursor` (expo/exact#714). `recents` writes its
empty answer by hand, and the bake refuses an answer that is missing the field,
so it now writes `{ assets: [], cursor: null }`. The strip reads only the first
page.

### Lean in

none

### Needed

none

### Checkpoints

- `bun test save.test.ts` passes with the mock's list in the new shape.
