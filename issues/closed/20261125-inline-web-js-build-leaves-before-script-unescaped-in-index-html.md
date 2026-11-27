# --inline web JS build leaves '<!--' before '<script' unescaped in index.html, breaking inline module parsing (and coupling CSP hash to on-disk app.js)

**Status:** Closed
**Resolution:** Escape HTML script-data tags in --inline entry chunks after minification so disk and inline bytes match, and hash the shell's inline entry script in exact-render CSP headers.
**Systems:** host/web-js bundler (--inline) and host/render CSP hashing
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Numeric narrowing, Unicode slicing and escaping / Trust boundaries)

Verification: verified by running.

1. **Exact location**
At `9c0431fd5`, `host/web-js/build.mjs:1318-1322` inlines `app-[hash].js` into `index.html` using only `.replaceAll('</script', '<\\/script')`, leaving HTML script-data escape triggers (`<!--` followed by `<script`) unescaped, while `host/render/src/csp.rs:56-63` hashes `dist/app-[hash].js` on disk to authorize that inline `<script type="module" data-entry="...">` under `Content-Security-Policy`.

2. **Reachable trigger**
Build any web app with `bun host/web-js/build.mjs <app> --inline` where any Contract string literal, `app.ts` string, or build-time baked resource answer contains `<!--` followed later by `<script` (followed by whitespace, `/`, or `>`, in any ASCII case) without a subsequent `-->` — for example `text "<!-- <script>hello"`, `text "<!-- <script></script>"`, or two separate strings `"<!--"` and `"<script>"`. Load the built `index.html` (or a `client` route served by `host/render/src/serve.rs:875-886`) in a browser.

3. **Actual vs. expected result**
- **Actual**:
  - **Confirmed (`<!--` + `<script`)**: Rolldown (`format: 'esm', minify: true, comments: false` at `host/web-js/build.mjs:1241,1247`) preserves `<!--` and `<script` verbatim in string and template literals (`const e=\`<!-- <script>\``, `const i=\`<!-- <SCRIPT>x\``), and `build.mjs:1321` writes them verbatim inside `<script type="module" data-entry="./app-[hash].js">...</script>`. Under HTML Living Standard §13.2.5.17–32 ("Script data escape start state" → "Script data escaped state" → "Script data double escape start state" → "Script data double escaped state" → "Script data double escape end state"), `<!--` followed by `<script>` enters script data double escaped state (even for `"<!-- <script></script>"`, because Rolldown rewrites `</script>` to `<\/script>`, which does not exit double-escaped state). When the HTML tokenizer reaches the real closing `</script>` tag at `build.mjs:1321`, it appends `</script>` as script text and only transitions back to escaped state rather than closing the element. At EOF, the module script ends with literal `</script>\n`, throwing a JS `SyntaxError` and preventing the app from booting. Both `host/render/src/page.rs:709-710` and `host/web-js/render.mjs:55-56` (`blanked`) also explicitly rely on script text holding no `<!--`.
  - **Disproved claims from original report (reducing severity from P1 XSS to P2 broken boot)**: Running Rolldown with `build.mjs`'s exact options (`format: 'esm', minify: true, comments: false`) shows OXC codegen already escapes `</` inside JS strings and template literals to `<\/` (`"</SCRIPT>"` → `` `<\/SCRIPT>` ``, `"</Script >"` → `` `<\/Script >` ``, `String.raw\`</SCRIPT>\`` → `` `<\/SCRIPT>` ``, `JSON.parse("{\"x\":\"</SCRIPT>\"}")` → `` JSON.parse(`{"x":"<\/SCRIPT>"}`) ``), strips comments, and emits `a< /script/i` with a space between `<` and `/`. Consequently, `</SCRIPT>` cannot break out of `<script>` for XSS, `a</script/i` syntax corruption does not occur, and `.replaceAll('</script', '<\\/script')` currently changes zero bytes relative to `app-[hash].js` on disk. However, `host/render/src/csp.rs:56-63` hashes `dist/app-[hash].js` on disk, so fixing `<!--` solely by mutating the string at `build.mjs:1321` would introduce a CSP SHA-256 mismatch and block execution under `host/render`.
- **Expected**: An `--inline` build must neutralize `<!--` (and not rely implicitly on minifier internals for `</script`) in a way that stays valid JS and matches the SHA-256 hash emitted by `host/render/src/csp.rs`. Established by HTML Living Standard §13.2.5 and Exact's own `scripts/web-head.mjs:49-56` (`SCRIPT_DATA_TAG = /<(?=!--|\/script[\t\n\f\r />])/gi`).

4. **Verification and sibling check**
Verified by running Rolldown via `bun -e` with `format: "esm", minify: true, comments: false` against `"</SCRIPT>"`, `"</Script >"`, `` `</SCRIPT>` ``, `String.raw\`</SCRIPT>\``, `a < /script/i`, `/*! </SCRIPT> */`, `"<!-- <script>"`, `"<!-- <SCRIPT>x"`, and separate `"<!--"` / `"<script>"` literals: all `</script` variants were emitted as `<\/...`, `a < /script/i` stayed `a< /script/i`, and `<!--` + `<script` were emitted verbatim (`const e=\`<!-- <script>\``).
- Siblings checked: `scripts/web-head.mjs:56,101` (clean: checks/escapes both `<!--` and case-insensitive `</script[\t\n\f\r />]`), `host/web-js/build.mjs:1444` (`application/vnd.exact.markdown`, clean: `.replace(/</g, '\\u003c')`), `host/web-js/render.mjs:450,459` (clean: `.replace(/</g, '\\u003c')`), `scripts/install-page.mjs:388-389` (clean: `.replaceAll('<', '\\u003c')`), `host/web/build.mjs` (wasm target has no `--inline` JS bundle).

5. **Suggested fix and regression test**
- Escape `<` in `<!--` (and `</script` defensively) inside emitted JS string/template literals in `app-[hash].js` itself (e.g., via a Rolldown transform/renderChunk AST pass or escaping `<!--` to `\u003C!--` inside strings/templates before or after minification, or checking/verifying the final inlined chunk matches `SCRIPT_DATA_TAG` with zero matches), AND/OR have `host/render/src/csp.rs` hash the actual inlined `<script type="module" data-entry="...">` body extracted from `shell` instead of reading `app-[hash].js` separately from disk.
- Add a test in `host/web-js/build.test.mjs` (beside the `--inline` test at `:972`) that builds an app with `text "<!-- <script></script>"` using `--inline`, checks that the inlined `<script>` contains no `<!--` / `</script` hazard (`SCRIPT_DATA_TAG`), verifies the page boots cleanly in HTML5 parsing, and asserts `csp::headers` admits the exact inlined script bytes.

### Siblings checked

scripts/web-head.mjs:56,101 (clean), host/web-js/build.mjs:1444 (clean), host/web-js/render.mjs:450,459 (clean), scripts/install-page.mjs:388-389 (clean), host/web/build.mjs (no --inline JS bundle)
