# Adding tvOS writes a manifest that Exact rejects

**Status:** Closed
**Resolution:** `tvos` is not a platform `--with` accepts: `exact new --with tvos` and `update --with tvos` are refused before anything is written, saying an Apple TV build takes `host.ios`.
**Systems:** app scaffolding, generated native workspaces, app manifest validation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, both `exact new <path> --with tvos` and an existing app's
`exact.mjs update --with tvos` succeed, but write `host.tvos: {}` into
`app.json`. Exact's own manifest reader then rejects the app with
`host.tvos: not a known key`. Adding a platform makes the app's manifest
unusable by subsequent build commands, including its existing platforms.

`game/new.mjs:179` and `:1245` accept every key in `PLATFORMS`.
`scripts/app-shells.mjs:22` includes `tvos`, and `namePlatform` at lines
51–61 writes that key directly under `host`. The schema's `host` object
at `scripts/app.schema.json:737` forbids additional properties and
declares no `tvos` property. Other Apple build paths use iOS configuration
for tvOS; the scaffold's accepted platform set and manifest representation
do not agree.

Reproduced through the actual CLI with pinned Bun 1.4.2:

```sh
bun scripts/exact.mjs new /tmp/exact-review-20261112/tvos-scaffold --with tvos
bun scripts/exact.mjs new /tmp/exact-review-20261112/tvos-update
cd /tmp/exact-review-20261112/tvos-update
bun exact.mjs update --with tvos
```

Both platform additions exited zero. The update printed `with tvos
(added host.tvos)`. Calling the actual `readManifest(dir, name)` exported
by `scripts/app.mjs` on either resulting app failed:

```text
host.tvos: not a known key
```

No native build or GUI was needed to reproduce the failure. The two
temporary app directories above retain the generated manifests.

Make tvOS enrollment agree with the supported manifest representation.
If `--with tvos` is unsupported, reject it before writing anything; if it
is supported, write configuration accepted by the schema and consumed by
the tvOS build. Validate both new-app and update paths by passing their
result through the production manifest reader, and check that a refused
update leaves an existing manifest intact.

Checked all open and closed filesystem issues and the 275-issue GitHub
snapshot for tvOS scaffolding, `host.tvos`, `--with tvos`, and invalid
platform manifests. No matching issue was found.
