# An installed app shim treats a leading-dash filename as a flag

**Status:** Closed
**Resolution:** The installed shim runs `open -a <bundle> -- "$@"`, so `--` ends open's options and a name starting with `-` is opened as a document.
**Systems:** scripts
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`exact install` writes a shim `exec /usr/bin/open -a <bundle> "$@"`. macOS `open` parses a first operand that starts with `-` as its own switch and exits with `invalid option`, so the document is never delivered. The comment above the shim says `--args` after the files prevents that. The script writes neither `--args` nor `--`.

Document apps are opened through this shim so a running copy receives the file. `mdview -notes.md` fails before Launch Services. Putting the names after `--args` would also be wrong: those words become argv on a cold launch only, and are not opened as documents in the already-running app.

## Where

`scripts/exact.mjs`, `install`.

## How to reproduce

`exact install` a document app, then run `<command> -notes.md`. `open` exits with `invalid option`. The file does not open.

## Direction

Emit `exec /usr/bin/open -a <bundle> -- "$@"` so `--` ends `open`'s options and the words stay documents.
