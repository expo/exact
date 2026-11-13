# The translation web conformance suite does not parse

**Status:** Open
**Systems:** packages/translation/test/web.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At audited commit `f6be99af`, line 68 puts unescaped backticks around
`availability()` inside a template literal. Bun rejects the entire module
before any conformance check can run, including the Firefox and WebKit
cases that do not enter that branch.

Reproduced with the pinned runtime:

```text
$ bun packages/translation/test/web.mjs
error: Expected ")" but found "availability"
    at packages/translation/test/web.mjs:68:123
error: Expected ";" but found ")"
    at packages/translation/test/web.mjs:68:219
Bun v1.4.2 (macOS arm64)
```

The command exited 1 during parsing; it built no fixture and launched no
browser. Escape or remove the inner backticks, then run the conformance
suite. Ensure routine validation parses this suite so a syntax error
cannot leave the package's web acceptance checks unavailable.

Checked all open and closed filesystem issues and all 275 cached GitHub
issue bodies; no existing report covers this parse failure.
