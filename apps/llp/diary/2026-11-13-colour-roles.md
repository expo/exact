surfaces: web, linux
agent: Codex
task: replace reader chrome colours with platform roles

## Rough

- The outline's unselected rows kept light-only ink in dark mode. Shallow
  headings now use the reader's scheme-aware `ink`; headings at level 3 or
  deeper use the secondary label role. The reading palette's `softInk` has
  only 3.19:1 contrast on the light outline surface.
- Replacing that whole palette with roles lost the shared readers' warm
  night surface and distinct code ink. The document palette stays identical
  to Markdown and Markdown Stress. Chrome metadata and controls use the
  secondary label role where the palette's muted ink was hard to read.
- There is no checked-in graphical Linux shell. A temporary carrier under
  `target/q1` links the shared Linux host, reader data and app storage;
  it drives the current compiled Contract without adding a shipping target.
- The web reader cannot open a local document. Its appearance regression
  compiles the actual outline component, heading shape and theme into a
  fixture so deeper rows are checked instead of only the welcome heading.

## Lean in

- A theme can keep the document's intentional reading colours while chrome
  text names a platform role. Inline code stays distinct from prose.

## Needed

- Appearance and colour roles: provided.

## Checkpoints

- `bun host/web/build.mjs llp` reports no colour warnings, down from one.
- `bun test apps/llp/app.test.mjs` checks the welcome document and chrome,
  all six outline depths selected and unselected, accent contrast, and
  palette parity with the other readers. The outline regression fails with
  `softInk` before the repair and passes with the secondary label role.
- Browser and headless Linux drives capture both appearances.
