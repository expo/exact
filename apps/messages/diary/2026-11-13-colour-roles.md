surfaces: web, linux
agent: Codex
task: replace application chrome colours with platform roles

## Rough

- Fixed control and avatar ink did not all read on their fills. Controls and
  bubbles now pair roles, including `AccentColorText` on the accent. Content
  avatars keep their identifying hues, with black initials except on the
  darkest model swatch.
- The Linux driver refuses `contextmenu` on the message box, so two existing
  authored tests cannot reach their menus there. Browser tests cover those
  menus; Linux screenshots cover the conversation and composer.
- Messages conformance omits a scratch store. JS and Linux then show a refused
  save notice absent from its wasm session, causing tree, layout and pixel
  differences. The original Contract reproduces that refusal without a store.

## Lean in

- The same bubble and tail fill role works on the browser and Linux. Reply
  bars, typing dots, separators and menu controls share the platform palette.

## Needed

- Appearance and colour roles: provided.

## Checkpoints

- Colour warnings fall from 33 sites to two: the avatar's fixed content ink
  and the neutral conversation avatar fill. Action icons use the app accent.
- `bun test apps/messages/appearance.test.mjs` checks contrast in the inbox,
  conversation, composer, context menu, reply and action panel in both schemes.
- Browser and Linux conversation screenshots show readable incoming and
  outgoing bubbles. Linux's existing context-menu input refusal is unchanged.
