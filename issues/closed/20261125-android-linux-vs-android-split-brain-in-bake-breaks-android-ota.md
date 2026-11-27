# Android "linux" vs "android" split-brain in bake breaks Android OTA push for faced tools, drops reach/color/privacy-policy gates on `--platform android`, emits Apple sign-in audiences on APK builds, and misnames `.entries`

**Status:** Closed
**Resolution:** Normalize Android targets to platform "android" at the start of compatibility_id_pinned, accept "android" in reach and color checks, restrict Apple sign-in audiences to Apple hosts, and emit android-<target> bake stems without Linux displayName.
**Systems:** bake (compatibility id, reach, colors, receipt) / Android OTA push
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Asymmetric pairs)

Verification: verified by reading.

At `9c0431fd5`, Android bakes pass `platform = "linux"` with an Android target triple (`*-linux-android`), while `compatibility_with_trust` locally remaps `platform` to `"android"` (`bake/src/compat.rs:685-686`) so `compat.json` records `inputs.platform = "android"` (`bake/src/compat.rs:855`). Because `compatibility_id_pinned` (`bake/src/compat.rs:365-517`) never normalizes `platform` for its own helper calls, build-time callers (`platform = "linux"`) and `exact-bake compat --platform android` (`bake/src/main.rs:53,125`, called by `scripts/publisher.mjs:209`) disagree on Android across four paths:

1. **`faces()` (`bake/src/compat.rs:497,561-568`)**: Matches `"android" => &["android"]` and `_ => return` without checking `target.contains("android")`. Every real Android APK build calls `compatibility_id*` with `platform = "linux"` (`js/bake/src/android.rs:18-20` -> `js/bake/src/lib.rs:230,285`, `bake/src/host.rs:138-144`, and all 14 `apps/*/android/build.rs` files such as `apps/caltrain/android/build.rs:38-41`), hitting `_ => return` and omitting `inputs.faces` from `compat.json`.
2. **`reach::derive`, `reach::privacy_policy`, and `colors` (`bake/src/compat.rs:412,458`, `bake/src/reach.rs:238-251,347-349,408-422`, `bake/src/colors.rs:10-18,181-187,212-230`)**: Match `platform == "linux"` and not `"android"`. Under `exact-bake compat --platform android --target <arch>-linux-android` (as invoked by `scripts/publisher.mjs:209`), Android device permissions (`reach.android`), `reach.androidFeatures`, `reach.androidServices`, and `CHANGE_WIFI_MULTICAST_STATE` are dropped, and both the Health Connect `privacy_policy` refusal (`reach.rs:408-422`) and the sRGB/profiled-color/8-bit-canvas refusals (`colors.rs:10-18,181-187,212-230`) fail open.
3. **`reach::sign_in` (`bake/src/reach.rs:702-706`)**: Matches `"android" => json!({ "google": google })` and falls through `_ => json!({ "apple": ["<the bundle ID>"], "google": google })`. Every real Android APK build (`platform = "linux"`) hits `_` and emits Apple's `"<the bundle ID>"` audience in `compat.reach.auth.signIn.audiences`.
4. **`receipt::emit` (`bake/src/compat.rs:506-514`, `bake/src/receipt.rs:242-254`)**: Builds `let stem = format!("{platform}-{target}");` from the un-remapped `platform`, writing `EXACT_BAKE_OUTPUT/linux-<arch>-linux-android.{entries,json,plan,artifacts.json}`. `completeBuild` (`scripts/app.mjs:1001-1002`) re-copies `.json`, `.plan`, and `.artifacts.json` under `android-<target>`, leaving orphaned `linux-<target>.*` files and omitting `android-<target>.entries`, which `bakedEntryAssets` (`scripts/app.mjs:785`) looks up by `${build.compat.inputs.platform}-${build.compat.target}.entries` (note: current `deploy.mjs`/`agent-android.mjs` paths do not yet invoke `bakedEntryAssets` on an Android build receipt, but the emitted stem contract is broken).

### Triggering sequence
1. Build a production Android APK (`EXACT_UPDATE_TRUST=production bun exact.mjs android --release`) for any app declaring a non-destructive faced tool in `app.json` (`intents.tools`, which defaults `doors` to `["apple", "android", "web"]` in `contract/cli/src/reach.rs:151,190`) or `auth.google`/`auth.apple` grants. Then run `bun scripts/push.mjs <app>` (or `bun scripts/deploy.mjs <app>`).
   - The APK build's `compat.json` has `inputs.platform = "android"` and no `inputs.faces`, and `reach.auth.signIn.audiences` includes `"apple": ["<the bundle ID>"]`.
   - `scripts/publisher.mjs:209,359` invokes `exact-bake compat <app> --platform android --target <arch>-linux-android --json`, where `faces()` now populates `inputs.faces`.
   - `scripts/publisher.mjs:367-368` flags `superseded = ["faces"]`; `scripts/push.mjs:101-104` freezes and refuses every OTA push to that cohort even with zero edits after the APK build, and `scripts/deploy.mjs:939-942` emits a permanent spurious binary row.
2. Run `exact-bake compat <app> --platform android --target aarch64-linux-android --json` (or trigger `snapshotCompat` in `scripts/publisher.mjs:209`) on an Android app with device grants, `device.health-read` without `privacy_policy`, or wide-gamut/profiled colors / `color-space="display-p3"` canvases.

### Actual vs. expected
- **Actual:** Build scripts (`"linux"`) and `exact-bake compat --platform android` produce divergent `compat.inputs`, `compat.reach`, color checks, and `EXACT_BAKE_OUTPUT` stems, permanently blocking Android OTA pushes for apps with Android-faced tools.
- **Expected:** Both entry points treat Android identically: `inputs.faces` is included in Android APK bakes (LLP 1133.023 D8) so `push.mjs` agrees with the APK's cohort; `reach.android*`, `privacy_policy` (LLP 1133.026 D4), and sRGB/profile/canvas refusals (LLP 1100 D1/D3/D12a, buffered RGBA8 sRGB path) apply consistently for Android; `reach.auth.signIn.audiences` emits `{"google": [...]}` on Android APK builds (LLP 1133.014 D8); and `EXACT_BAKE_OUTPUT` writes under `android-<target>` (LLP 1030.006 D2, `scripts/app.mjs:785,1001`).

### Verification & siblings checked
Verified by reading the exact paths above at `9c0431fd5`. `bake/src/compat_faces_tests.rs:40` passed `("android", "aarch64-linux-android")` rather than the real build-script pair `("linux", "aarch64-linux-android")`, masking breakage #1.

Sibling sites checked:
- **Affected sibling — `bake/src/colors.rs:85` (`target_colors`):** compares `name == platform` on `-exact-platform-color(...)`, so an `android` branch is ignored during APK builds (`"linux"`) and selected during `exact-bake compat --platform android`.
- **Affected sibling — `bake/src/receipt.rs:168-170`:** `if platform == "linux"` writes `compat.embedded["displayName"]` (only read by desktop Linux in `host/linux/src/window.rs:113`) onto Android APK builds under `"linux"`, but not under `"android"`.
- **Unaffected siblings:** `bake/src/controls.rs:87` and `bake/src/compat.rs:440,461,476` already check `target.contains("android")`; `bake/src/compat.rs:735,907,913` (`executors`, `links::claims`, `file_handlers`) run inside `compatibility_with_trust` after line 686's remap; `bake/src/compat.rs:405,459,460` (`preload`, `localizations`, `sandbox`) and `bake/src/receipt.rs:81` (`bundledFamily`) behave identically for `"linux"` and `"android"`.

### Suggested fix & regression test
Normalize `let platform = if target.contains("android") { "android" } else { platform };` once at the top of `compatibility_id_pinned` (`bake/src/compat.rs:365`), and update:
- `bake/src/reach.rs:238,347,409,420` to match `"android"` (and `"linux"` if still called directly by unit tests, or migrate tests to `"android"`), while restricting `reach::sign_in`'s Apple audience arm (`reach.rs:702-706`) to `"ios" | "macos"`;
- `bake/src/colors.rs:12,182,212` to match `"linux" | "android"`;
- `bake/src/receipt.rs:242` automatically receives normalized `"android"` for the `EXACT_BAKE_OUTPUT` stem (and line 168 stops attaching Linux's `displayName` to Android).

Add a unit test in `bake/src/compat_tests.rs` / `compat_faces_tests.rs` asserting that `compatibility_id_pinned` produces byte-identical `compat.to_json()`, `faces`, `reach.android*`, `reach.auth.signIn.audiences`, `privacy_policy` refusal, `colors::check_plan` refusal, and `android-<target>.*` output stems for both `("linux", "aarch64-linux-android")` and `("android", "aarch64-linux-android")`.

### Siblings checked

Checked bake/src/controls.rs:87 (already uses target.contains("android")), bake/src/compat.rs:440,461,476 (already use target.contains("android")), bake/src/compat.rs:735,907,913 (post-remap inside compatibility_with_trust), bake/src/compat.rs:405,459,460 and bake/src/receipt.rs:81 (unaffected). Found two additional affected siblings: bake/src/colors.rs:85 (target_colors platform matching) and bake/src/receipt.rs:168-170 (Linux desktop displayName attached to Android APK receipts).
