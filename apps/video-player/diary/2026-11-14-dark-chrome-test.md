surfaces: web
agent: Codex (GPT-6)
task: correct the dark-chrome test's scheme assertion

## Rough

The test treated `check.scheme` as the requested device preference. It is the scheme the
page draws; `check.asked` records the requested preference. Video Player declares
`color-scheme="dark"`, so a light preference correctly produces a dark report.

## Lean in

The check report already distinguishes the requested preference from the rendered scheme.

## Needed

none

## Checkpoints

- Web build: passed offline.
- Web drive: both preference cases pass, including note editing and the media-error state.
  The browser's media query confirms each requested preference, and both check reports
  retain the authored dark scheme.
