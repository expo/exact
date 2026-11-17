# The brand asset's README tells maintainers to keep a mirror file in step that does not exist

**Status:** Closed
**Resolution:** The README names the mark's readers in this repository, the install page and the docs site, and the docs site's filled adaptation as the copy to keep identical, in place of the absent `js/src/guide/lib/icons.ts`.
**Systems:** assets/brand/README.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`assets/` holds two files at `dd6f5dcd`: `assets/brand/exact-mark.svg` and `assets/brand/README.md`. The README's last paragraph (line 7) says:

> The Exact Guide keeps an inline adaptation in `js/src/guide/lib/icons.ts` because Contract's `svgSource` API currently consumes SVG markup. That mirror should remain geometrically identical to this asset.

There is no `js/src/guide/` in the repository, and no tracked path contains `guide`. The mark's geometry (`161.803`, `M100 4v92`) appears in no file but the SVG and this README, so there is no second copy to keep identical. Line 5's "Consumers may tint the monochrome mark for their context, as the Exact Guide does" is about the same absent consumer.

The consumer that does exist is not mentioned: `scripts/install-page.mjs:112` reads `assets/brand/exact-mark.svg` for the standard install page.

## The failure

```sh
ls js/src/guide/lib/icons.ts
# ls: js/src/guide/lib/icons.ts: No such file or directory
grep -rIl "161.803\|M100 4v92" . --exclude-dir=target --exclude-dir=node_modules
# ./assets/brand/exact-mark.svg
# ./assets/brand/README.md
grep -rIl "exact-mark\.svg" . --exclude-dir=target --exclude-dir=node_modules
# ./scripts/install-page.mjs
# ./assets/brand/README.md
```

Expected: the file the README tells a maintainer to keep in step exists, or the README does not name it.

## Verification

Ran the three commands. Searched the tracked file list for `guide`: no match.

## Direction

If the Exact Guide lives in another repository, say so instead of giving a path that reads as this repository's; otherwise delete the two sentences. Name the install page as the mark's reader.

## Related

None found in `issues/`, `QUEUE.md` or `docs/agent-pitfalls.md`.
