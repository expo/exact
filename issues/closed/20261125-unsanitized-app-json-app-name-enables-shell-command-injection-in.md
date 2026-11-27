# Unsanitized `app.json` `app.name` enables shell command injection in `exact install`'s `0755` PATH shim and path traversal in `exact release` and `exact appstore`

**Status:** Closed
**Resolution:** Reject control characters in manifest app/variant names in app.schema.json, sanitize display names in shim and generated comments, and use appleExecutableName(app) for release and appstore artifacts.
**Systems:** macOS install / release / appstore / shell generation
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Trust boundaries)

Verification: verified by running.

At `9c0431fd5`, `scripts/app.schema.json:759` (`app.name`) and `:1282` (`variants.<v>.name`) allow any non-empty string (`{"type": "string", "minLength": 1}`), including newlines, control characters, `/`, and `..`, and `readManifest` (`scripts/app.mjs:555-582`) performs no additional character validation on `app.name` (exposed as `app.displayName` at `:529`). Several build/install/release paths interpolate this unsanitized value into executable scripts, filesystem paths, and generated source comments:
1. **Shell injection in `exact install` PATH shim (`scripts/exact-install.mjs:37-41, 97-99`):** `shimText` carefully single-quotes `target` (`:36, 40`) against shell metacharacters, but interpolates raw `app.displayName` into `# ${app.displayName} — ...` (`:38`) and writes the resulting `#!/bin/sh` script with mode `0o755` into `binDirectory()` on `PATH` (`:97-99`). A newline in `app.name` breaks out of the comment and injects arbitrary shell commands that execute whenever the installed CLI shim is run.
2. **Path traversal and `/` breakage in `exact release` (`scripts/exact.mjs:252, 265, 288`) and sibling `exact appstore` (`host/apple/appstore.mjs:125`):** `release()` sanitizes the `.app` bundle path with `appleExecutableName(app)` at `:249, 289`, yet constructs `.dSYM` (`:252`), `.zip` (`:265`), and `.dmg` (`:288`) with raw `app.displayName` inside `resolve(out, ...)`. Likewise, `host/apple/appstore.mjs:125` builds `resolve(out, \`${app.displayName}.pkg\`)` with raw `app.displayName` (while `:110` uses the sanitized `.app` basename). Names containing `../` write release/store artifacts outside `out`, and even benign names containing `/` (e.g. `"A/B Tester"`) fail `ditto` / `hdiutil` / `productbuild`.
3. **Comment breakout in generated workspaces and hatch stubs:** `scripts/app-shells.mjs:649` (`.exact/shells/Cargo.toml`) and `:210, 363, 410, 439, 472` (generated `src/lib.rs` / `src/main.rs`), plus `scripts/exact.mjs:949, 1040, 1085, 1172` (Swift/JS/Rust/Kotlin hatch stubs) interpolate raw `title` (`manifest.app?.name ?? manifest.name`) into `#`, `//!`, `//`, or `/** ... */` comments without stripping `\r`/`\n` (or `*/` at `exact.mjs:1172`), even though `app-shells.mjs:483` escapes `title` via `JSON.stringify(title)`.

### Reachable trigger
Set `app.name` in `app.json` to `"Foo\necho SHIM_INJECTED_OK >&2; exit 0 #"` or `"../../escape"` (or `"A/B"`) and run `bun scripts/exact.mjs install <app>`, `bun scripts/exact.mjs release <app>`, `bun scripts/exact.mjs appstore <app>`, `bun scripts/exact.mjs hatch ...`, or any native build (`ensureShells`). Because `executableName` (`scripts/app.mjs:881`) replaces control characters and `/` with `-`, the `.app` build itself succeeds.

### Actual vs. expected
- **Actual:** `readManifest` accepts newlines and `../` in `app.name`; running the `0755` shim written by `shimText` executes the injected shell command; `resolve(out, \`${app.displayName}.zip\`)` escapes `out` (e.g. `/tmp/out/probe` + `../../escape.zip` → `/tmp/escape.zip`); generated TOML/Rust/Swift/JS/Kotlin comments allow newline/`*/` breakout.
- **Expected:** `scripts/app.schema.json` already rejects control characters in other manifest display fields (`live_activities.*.title`/`.description` at `:364, 370` and `controls.*.description` at `:603, 621`), `scripts/exact-install.mjs:36` explicitly defends the shim against shell injection, `host/apple/tests/it/development.rs:63, 95` treats `app.displayName` as untrusted, and `scripts/exact.mjs:249, 289` uses `appleExecutableName(app)` to keep bundle filenames inside `out`.

### Verification and siblings
Verified by running a Bun probe against `readManifest`, `appleExecutableName`, and `shimText`:
- `readManifest` accepted `app.name = "Foo\necho SHIM_INJECTED_OK >&2; exit 0 #"`; executing the resulting `shimText` script via `/bin/sh` printed `SHIM_INJECTED_OK` and exited `0`.
- `readManifest` accepted `app.name = "../../escape"`, for which `appleExecutableName` returned `"-..-escape"` while `resolve("/tmp/out/probe", `${app.displayName}.zip`)` resolved outside `out` to `"/tmp/escape.zip"`.
- **Siblings affected:** `host/apple/appstore.mjs:125` (`resolve(out, \`${app.displayName}.pkg\`)`), and minor comment interpolation in `game/new.mjs:782` (`applyPattern` `.contract` header comment).
- **Siblings safe:** `host/web/serve.mjs:59-60` (HTML-escapes `app.displayName`), `scripts/exact-install.mjs:14, 16` (`commandOf` uses schema-validated `app.command` or normalized `app.name`; `installedAt` uses `appleExecutableName`), `scripts/app-shells.mjs:389` (no `title` interpolation) and `:483` (`JSON.stringify(title)`), `scripts/agent-android.mjs:1166` and `scripts/app-cli.mjs:836` (argv without shell, though schema validation also protects `AndroidManifest.xml`).

### Suggested fix and regression test
- Add the single-line no-control-characters pattern (matching `live_activities.*.title` at `scripts/app.schema.json:364`) to `app.name`, top-level `name`, and `variants.*.name` in `scripts/app.schema.json`.
- Strip `[\r\n]` in `shimText` (`scripts/exact-install.mjs:38`) and generated single-line comments in `scripts/app-shells.mjs` and `scripts/exact.mjs` (plus `*/` in `exact.mjs:1172`).
- Use `appleExecutableName(app)` for `.dSYM`, `.zip`, and `.dmg` in `scripts/exact.mjs:252, 265, 288` and for `.pkg` in `host/apple/appstore.mjs:125`.
- Add tests in `scripts/exact-install.test.mjs` and manifest validation tests verifying control characters in `app.name` / `variants.*.name` are rejected, `shimText` never emits extra command lines, and release/appstore artifact paths stay within `out` even when `app.name` contains `/` or `..`.

### Siblings checked

host/apple/appstore.mjs:125 (affected: raw app.displayName in .pkg path); game/new.mjs:782 (minor comment interpolation); host/web/serve.mjs:59-60, scripts/exact-install.mjs:14, 16, scripts/app-shells.mjs:389, 483, scripts/agent-android.mjs:1166, scripts/app-cli.mjs:836 (checked/safe)
