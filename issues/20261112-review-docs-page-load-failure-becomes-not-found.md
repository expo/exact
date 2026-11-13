# A failed docs page download becomes a successful “Page Not Found” result

**Status:** Open
**Systems:** docs/site/app.ts, docs/site/app.contract
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `loadPage` catches every rejected page-module import and returns
the same `missing` object used for an unknown route (`app.ts:111–115`). A
temporary connection failure while navigating to a known page therefore
settles the resource successfully with the title “Page Not Found” and no
article blocks. It loses the failure reason and offers no retry.

The route itself still exists. `App` supplies `missing` to `Article` only
when the route name is `notfound` (`app.contract:304`), so this result also
bypasses the normal not-found recovery links. The reader gets an empty
article headed “Page Not Found” at a valid address.

## Verification

Copied the unchanged data module into a temporary fixture, replacing only
its generated content imports with controlled adapters. The `recipeWebsite`
loader rejected with `TypeError("Failed to fetch dynamically imported module")`.
Calling `answer('loadPage', ['recipeWebsite'], ...)` resolved, rather than
rejected, with:

```json
{"slug":"notfound","title":"Page Not Found","blocks":[]}
```

The probe ran under Bun 1.4.2 and used no network. It verifies the source's
failure conversion; the rendering consequence follows the inspected
`App` and `Article` branches. A browser check should hold or fail a page
chunk request during navigation, then restore the connection.

Distinguish an absent route from a failed import. Preserve the load error,
show a recoverable failure state, and let the reader retry or reload the
page. Cover successful loading, an unknown route, and a transient failure
of a known page.

Searched open and closed filesystem issues and all 275 GitHub issues in
the review snapshot. No matching report was found.
