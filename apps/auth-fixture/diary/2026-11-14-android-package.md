surfaces: android, web
agent: Codex, GPT-6.1 Sol
task: use the selected Android package in the public passkey association

## Rough

- The public origin wrote the top-level web manifest ID into `assetlinks.json`,
  bypassing the Android package override and default hyphen folding.

## Lean in

- The origin now uses the same Android association builder as the web build.

## Needed

- package identity: provided by `host.android.id`, with `app.id` as its default.

## Checkpoints

- The unit regression failed with the web ID as the Android package before the
  fix. It checks the override, hyphen folding, certificates and Apple identity.
- No public deployment or Android device ceremony was run in this revision.
