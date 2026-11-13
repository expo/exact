surfaces: web, linux
agent: Codex
task: replace workspace chrome colours with platform roles

## Rough

- The photo viewer kept a fixed dark background. Its surface now follows a
  role. The workspace supplies its own role palette instead of asking the
  interaction fixture for colours.
- System green was too pale for this app's text on light surfaces. The root
  keeps the existing dark-green/light-green accent pair; filled controls use
  explicit white/black ink paired with that accent. `AccentColorText` stays
  white on Apple and made the dark-mode mint fill unreadable.

## Lean in

- Injected palette values can name roles, keeping the workspace's shared
  components and the interaction gallery's fixture independent.

## Needed

- Appearance and colour roles: provided.

## Checkpoints

- `bun host/web/build.mjs exact-live` reports no colour warnings, down from one.
- `bun test apps/exact-live/app.test.mjs` checks the viewer's changing surface
  and text contrast in both appearances. Its native-colour regression checks
  Go live, Post and selected workload labels against both schema columns;
  the previous dark-mode Apple labels measured 1.31:1.
- Browser and headless Linux drives open a photo and capture both appearances.
