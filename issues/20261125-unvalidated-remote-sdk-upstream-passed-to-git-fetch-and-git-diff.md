# Unvalidated remote `sdk.upstream` passed to `git fetch` and `git diff` enables argument/command injection (and unvalidated `accepted.id` in authenticated URLs)

**Status:** Open
**Systems:** Exact Build client (share)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Trust boundaries)

Verification: verified by running.

At `9c0431fd5`, `scripts/exact-build.mjs:130-136, 145` passes `sdk.upstream` from the remote build service's `GET /sdk?exact=<commit>` JSON response directly to `git cat-file`, `git fetch`, and `git diff` without validating that it is a commit hash or terminating option parsing with `--`. In the same module, `scripts/exact-build.mjs:209, 219` interpolates unvalidated `accepted.id` from `POST /build` directly into authenticated `request()` paths (`/${accepted.id}/` and `/${accepted.id}.ipa`) that carry the user's `Authorization: Bearer` header.

### Reachable trigger
1. From an Exact checkout with local commits ahead of `main` or uncommitted changes outside the app directory, run `EXACT_EXPERIMENTAL=1 EXACT_BUILD=1 bun exact.mjs share` (or without local Xcode / Apple cross SDK, including against a custom `EXACT_BUILD_URL`).
2. `scripts/share.mjs:198-203` calls `exactBuild.buildIpa(...)` (`scripts/exact-build.mjs:178-226`), which calls `sdkSource(checkout, app.dir, lookup)` (`:185-188`).
3. Because `changed` (`:129`) is `true`, `sdkSource` calls `lookup(commit)` (`GET /sdk?exact=<commit>`) and passes raw `sdk.upstream` to `git cat-file -e` (`:133`), `git fetch -q <remote> <sdk.upstream>` (`:134`), and `git diff ... <sdk.upstream> -- .` (`:145`).
4. On `POST /build`, the server returns `{ id: "../other-service/secret?k=1#", missing: [] }`, which `:209` and `:219` pass to `request(...)` with `Authorization: Bearer <token>`.

### Actual vs. expected
- **Actual:**
  - When a malicious or compromised build service returns `sdk.upstream = "--upload-pack=sh -c '<cmd>'"`, `git cat-file -e` (`:133`) fails and `:134` runs `git fetch -q <remote> --upload-pack=sh -c '<cmd>'`. Because `git fetch` parses options after `<repository>`, `<cmd>` executes locally via `sh -c` on local/file remotes or is passed as the remote command over `ssh` on SSH remotes (such as `git@github.com:expo/exact.git`, matched at `:124`).
  - Returning a refspec such as `"+HEAD:refs/heads/<branch>"` overwrites or creates arbitrary local branches in the developer's Exact checkout on any remote protocol (HTTPS, SSH, or local).
  - Returning an `accepted.id` containing `../`, `?`, or `#` redirects authenticated `Bearer` requests to arbitrary paths on the host (including outside `EXACT_BUILD_URL`'s path prefix, which `serviceUrl` preserves at `:33`).
- **Expected:** `scripts/exact-build.mjs:19-34, 56-64` explicitly treats the remote build service as a trust boundary (per-origin tokens, HTTPS-only URLs, `redirect: 'error'` to prevent token leakage, and `encodeURIComponent(id)` at `:103`), and sibling `scripts/tools.mjs:518, 573, 602, 667` validates Git commits as lowercase 40/64-char hex before passing them to Git. `sdk.upstream` must be validated as a full hex commit hash and `accepted.id` as a safe single path segment.

### Verification and siblings
Verified by running a Bun probe directly against `sdkSource` and `buildIpa` from `scripts/exact-build.mjs` in temporary directories:
- `sdkSource` with `upstream: '--upload-pack=sh -c "touch <file>; exit 1"'` created `<file>` (`C146_UPLOAD_PACK_RAN: true`).
- `sdkSource` with `upstream: '+HEAD:refs/heads/injected-branch'` created local branch `injected-branch` (`C146_BRANCH_INJECTED: injected-branch`).
- `buildIpa` against `http://127.0.0.1:<port>/subpath` with `POST /subpath/build` returning `id: "../other-service/secret?k=1#"` sent `GET /other-service/secret?k=1` with `Authorization: Bearer <token>`.
- **Siblings checked:** `scripts/exact-build.mjs:103` (`finishLogin` encodes `id` with `encodeURIComponent`, safe); `scripts/share.mjs:66-70, 180` (`parseLink` allowlists `^\/d\/([A-Za-z0-9]+)\/?$`, safe); `scripts/tools.mjs:518, 573, 602, 667` (strictly validates commits with `/^[0-9a-f]{40}(?:[0-9a-f]{24})?$/`, safe); `scripts/async.mjs:707, 744` and `host/android/reference/watch.mjs:42` (`git fetch` takes only literal `'origin'`, safe).

### Suggested fix and regression test
- In `sdkSource` (`scripts/exact-build.mjs:132`), validate `typeof sdk.upstream === 'string' && /^[0-9a-f]{40}(?:[0-9a-f]{24})?$/.test(sdk.upstream)` before any `git` call (and reject otherwise).
- In `buildIpa` (`scripts/exact-build.mjs:196`), validate `accepted.id` against a strict single-segment allowlist (e.g. `/^[A-Za-z0-9._-]+$/` rejecting `.` and `..`) before interpolating it into `/${accepted.id}/`, `/${accepted.id}.ipa`, and `.exact/exact-build.json`.
- Add unit tests in `scripts/exact-build.test.mjs` asserting that `sdkSource` rejects option-like (`--upload-pack=...`) and refspec (`+HEAD:refs/heads/...`) `sdk.upstream` values without invoking `git fetch`, and that `buildIpa` rejects traversal/query `accepted.id` values before polling.

### Siblings checked

scripts/exact-build.mjs:103 (encodes request_id); scripts/share.mjs:66-70, 180 (strictly allowlists drop id); scripts/tools.mjs:518, 573, 602, 667 (validates hex commits before git); scripts/async.mjs:707, 744 and host/android/reference/watch.mjs:42 (literal origin only)
