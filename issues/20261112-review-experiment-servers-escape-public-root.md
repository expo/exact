# Two game comparison servers can serve files outside their public directory

**Status:** Open
**Systems:** experiments/game-engine-comparison/full/three/server.mjs, experiments/game-engine-comparison/full/godot/serve.mjs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, both development servers decode the URL path before opening
a file, but do not correctly confine the result to `dist`:

- Godot's `serve.mjs:6` prepends `dist` without a containment check.
  `/%2e%2e%2foutside.txt` reads `outside.txt` in the game's directory.
  Additional parent segments can reach other readable files.
- Three's `server.mjs:13` checks `path.startsWith(root)`, accepting sibling
  names with the same prefix. `/%2e%2e%2fdist-private/outside.txt` reads
  `dist-private/outside.txt` outside `dist`.

Both print a loopback URL but leave `hostname` unset. Bun's documented
[default is `0.0.0.0`](https://bun.com/docs/runtime/http/server), so starting
either server also exposes it on available network interfaces. These are
experiment servers, not Exact's production serving implementation.

Reproduced on Bun 1.4.2 using unchanged copies of the two server modules in
an isolated temporary directory. A stub captured each `Bun.serve` handler;
the actual handler read only synthetic files created for the probe:

```text
three /%2e%2e%2foutside.txt                 403 Forbidden
three /%2e%2e%2fdist-private/outside.txt    200 synthetic-prefix-sibling
godot /%2e%2e%2foutside.txt                 200 synthetic-outside-root
godot /%2e%2e%2fdist-private/outside.txt    200 synthetic-prefix-sibling
```

The executable probe is
`/tmp/exact-review-20261112/experiment-serve-paths.mjs`. It opened no
listening socket and read no personal files.

Use a shared canonical containment resolver that refuses escapes and
symlinks leaving the public directory, serves regular files only, and
handles malformed percent escapes as a client error. Bind explicitly to
loopback for these local previews. Cover encoded separators and sibling
prefixes in the path checks.

Checked open and closed filesystem issues and all 275 cached GitHub issues.
The closed `20260901-metrics-path-containment.md` fixes the same class in
the metrics server; it does not cover these two experiment servers.
