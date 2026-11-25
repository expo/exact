# @exact/global-shortcuts by hand (LLP 1133.025 D3)

The drives and the person's steps are the desktop fixture's, shared by the
three desktop packages: `packages/dock/test/desktop.mjs` (drives 4 and 7)
and `packages/dock/test/checklist.md` (Mac steps 7 and 8). The runner's
tests are `contract/cli/tests/it/desktop.rs`.

A press needs a key pressed at the keyboard, which the agent does not make:
the substitute's `tap @ref` is the drive's press.

`bun test packages/global-shortcuts/test/pairs.test.mjs` checks pair delivery
with synthetic macOS modifier events, including missed presses and releases.
For the keyboard check, mount a reader of `Meta+Meta` while left Command is
held, release it, and press right Command alone: no shortcut. Then hold both:
one press. Releasing either raises one `globalshortcutup`.
