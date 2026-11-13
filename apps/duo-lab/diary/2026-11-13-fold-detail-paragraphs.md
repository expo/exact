surfaces: web, ios
agent: Claude Code (Claude Opus 5.5)
task: Apply the long-text recipe to the rest of Duo Lab.

### Rough

Fold's detail drew an item's body, three paragraphs joined by blank lines, in one plain `text`. A plain text collapses line breaks, as CSS's `white-space: normal` does, so the three paragraphs showed as one on the web and on iOS. The body is now a list of paragraphs from `app.ts`, drawn one `text` each, as `docs/recipes/long-text.md` says for plain text. Duo Lab's other texts hold one paragraph each, and Reflow's document already follows the recipe.

The Duo smoke's `lab` leg fails four checks at origin/main `32c22e0a`, with this change and without it. They are in `QUEUE.md`.

### Lean in

`agent web … "tap item-2" "screenshot"` showed the lost paragraphs in one step.

### Needed

none

### Checkpoints

- web build: smooth
- iOS build: smooth
