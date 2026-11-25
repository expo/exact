# `artifact_graph` and `plan_graph` omit `commands`, `iconFillRules`, `atomFace`, `alternateIcons`, `faces`, and `controls` from `app.plan`'s `requires`, letting `deploy.mjs` (and `push.mjs` for baked even-odd icons) ship incompatible OTA plans to older cohorts

**Status:** Closed
**Resolution:** Emit commands, iconFillRules, atomFace, alternateIcons, faces, and controls in artifact_graph and plan_graph requires and propagate iconFillRules and atomFace onto entry-plan cards.
**Systems:** bake (artifact graph / plan_graph) / deploy.mjs & push.mjs OTA classifier
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Version skew)

Verification: verified by reading.

At `9c0431fd5`, `bake/src/receipt.rs:artifact_graph` (`bake/src/receipt.rs:630-748`) and `bake/src/receipt.rs:plan_graph` (`bake/src/receipt.rs:593-626`) omit six compatibility inputs from `app.plan`'s `requires`:

- `artifact_graph` copies only `["app", "kernelSchema", "formatVersion", "formatDigest"]` (`bake/src/receipt.rs:684-686`), `surfaceCalls`, `sources`, `executors`, `grantCeiling`, `hatches`, `androidUses`, `androidExtensions`, `typescriptRuntime`, `appleRasterIcons`, and `["grantPurposes", "localizations", "watchSync"]` (`bake/src/receipt.rs:744-748`).
- `plan_graph` overrides only `formatVersion`, `formatDigest`, `kernelSchema` (`bake/src/receipt.rs:612-615`), `appleRasterIcons` (lines 601-603), and `appleUses` (lines 620-624).

Neither function includes:
1. **`commands`** (`bake/src/compat.rs:636-656,796-798`) — fixed host-command argument shapes.
2. **`iconFillRules`** (`bake/src/icon_rules.rs:1-6,84-97`) — plans containing `evenodd:` icon paths.
3. **`atomFace`** (`bake/src/atom_faces.rs:1-9,25-45`) — `AtomFace` field layout for plans binding `atomFaces`.
4. **`alternateIcons`** (`bake/src/compat.rs:882-894`), **`faces`** (`bake/src/compat.rs:556-580`), and **`controls`** (`bake/src/controls.rs:1-5,179`) — binary-bound iOS alternate-icon rosters, compiled faced-tool doors, and control intents/tiles.

### Triggering sequences
1. **`deploy.mjs` for all six keys:** Run `bun scripts/deploy.mjs <app> --yes` against an origin with an older cohort after changing any fixed host-command shape (`commands`), adding an even-odd SVG icon (`iconFillRules`), binding `atomFaces` with changed `AtomFace` fields (`atomFace`), or modifying `alternate_icons`, faced-tool signatures (`faces`), or `controls`.
   - `deployCaptured` (`scripts/deploy.mjs:1227`) calls `streamCandidates` (`scripts/publisher.mjs:359-368`), which computes `superseded` and puts it into `pendingInputs` (emitting a separate `binary` row at `scripts/deploy.mjs:940-942`), while `classify` (`scripts/deploy.mjs:907-938`) and `publishStream` (`scripts/publisher.mjs:503-504`) gate `action: 'bundle'` strictly on `classifyArtifacts` (`scripts/app.mjs:1348-1378`), `update.reason`, and `update.nativeData` (`dataCrate` only) — never `update.superseded`.
   - Because none of the six keys appear in `app.plan`'s `requires`, `classifyArtifacts` returns `bundle: true` and `deploy.mjs` publishes the incompatible plan to the older cohort.
2. **`push.mjs` for baked even-odd icons:** On an older cohort with no `iconFillRules`, edit a data source so it bakes an `evenodd:` icon into a resource's initial value (the case tested by `bake/src/icon_rules.rs:472`), without writing any literal even-odd icon in `app.contract`. Run `bun scripts/push.mjs <app>`.
   - `snapshotCompat` (`scripts/publisher.mjs:207-218`) invokes `exact-bake compat` with no `OUT_DIR`, so `icon_rules::shipped` (`bake/src/icon_rules.rs:44-57`) returns `Ok(None)` and `icon_rules::apply` inspects only the unbaked `app.contract` — producing no `iconFillRules` in `compat.inputs` and `superseded = []`.
   - `planGraph` (`scripts/publisher.mjs:178-203`) runs `exact-bake graph` on the baked `built.plan`, where `plan_graph` never runs `icon_rules::even_odd(&plan)`. Both `superseded` and `classifyArtifacts` pass, so `push.mjs` ships the `evenodd:` plan to the older cohort.

### Actual vs. expected
- **Actual:** `deploy.mjs` ships incompatible plans to older cohorts across all six omitted inputs (and `push.mjs` additionally ships baked `evenodd:` plans). At runtime, older binaries misread changed command arguments, parse `"evenodd:"` as path commands, silently clear `atomFaces`, or disagree with compiled Siri/Shortcut/Control intents (while `alternateIcons` falls back to `NotFoundError` despite the stated invariant).
- **Expected:** Under LLP 1030 D3 (`scripts/app.mjs:1346-1347`: compatibility-id equality decides neither binary nor safe bundle; `artifact.requires` gates bundle delivery), LLP 1114.001 D1 & §0 Grok C4 (`bake/src/compat.rs:796-797`), LLP 1121 stage 14 C2 (`bake/src/icon_rules.rs:1-6`), LLP 1045.000 D2 (`bake/src/atom_faces.rs:1-9`), LLP 1163.000 D2 (`bake/src/compat.rs:882-883`), LLP 1133.023 D8 & §15 #2 (`llp/1133.023:845-855,1886-1890`), and LLP 1126.004 D3/D5 (`bake/src/controls.rs:1-5`), `artifact_graph` and `plan_graph` must emit these plan-level requirements in `requires` so `classifyArtifacts` refuses incompatible bundles for older cohorts.

### Verification & siblings checked
Verified by reading the exact paths above at `9c0431fd5` (and confirmed via `scripts/push.test.mjs:847-901` that `deploy.mjs` relies solely on `classifyArtifacts` / `requires` to reject bundles for `grantPurposes` and `localizations`).

Sibling sites checked:
- **Affected sibling — entry plans (`plans/<kind>.plan`):** `bake/src/receipt.rs:143-156` and `scripts/publisher.mjs:192-199` copy only `appleRasterIcons` (and `appleUses` in `publisher.mjs`) into entry-plan artifact `requires`, omitting `iconFillRules` and `atomFace` even though `icon_rules::apply` (`icon_rules.rs:539`) and `atom_faces::apply` (`atom_faces.rs:37-40`) derive those requirements from entry plans too.
- **Related sibling — dev candidate graph:** `bake/src/receipt.rs:919-952` (`write_development_artifacts`) delegates to `artifact_graph`, so fixing `artifact_graph` covers it automatically.
- **Related manifest-bound `compat.inputs` keys:** `bake/src/channels.rs:190,200` (`androidChannels`, `androidActivities`) and `bake/src/compat.rs:930-942` (`widgets`, `liveActivities`) are also absent from `artifact_graph`'s `requires` (worth reviewing if plans targeting newly declared channels/widgets/activities should also be gated in `deploy.mjs`).

### Suggested fix & regression test
1. In `bake/src/receipt.rs:artifact_graph`, include `commands` (when present in `inputs` or always via `compat::commands_digest()`), `iconFillRules` (derived directly via `icon_rules::even_odd(plan)?` so baked resource values are always caught), `atomFace` (derived directly via `atom_faces::binds(plan)`), and copy `alternateIcons`, `faces`, and `controls` when present in `inputs`.
2. In `bake/src/receipt.rs:plan_graph`, stamp `requires["commands"] = json!(crate::compat::commands_digest())` alongside `formatVersion`/`formatDigest`/`kernelSchema` so `exact-bake graph` enforces the current toolchain's command shapes even when given an older cohort's `compat.json`.
3. Propagate `iconFillRules` and `atomFace` onto `plans/<kind>.plan` entries in `bake/src/receipt.rs:143-156` and `scripts/publisher.mjs:192-199`.
4. Add unit tests in `bake/src/receipt/tests.rs` verifying `artifact_graph` and `plan_graph` emit all six keys (including an even-odd icon baked into a resource's initial value) and a `scripts/push.test.mjs` / `scripts/deploy.test.mjs` test asserting `classifyArtifacts` returns `bundle: false` when an older cohort lacks or differs on each key.

### Siblings checked

Checked entry plan requires in bake/src/receipt.rs:143-156 and scripts/publisher.mjs:192-199 (also omit iconFillRules and atomFace for widget/activity plans), dev candidate graph in bake/src/receipt.rs:919-952 (shares artifact_graph), publishStream locked re-check in scripts/publisher.mjs:503-504 (uses classifyArtifacts), and other compat.inputs keys (androidChannels, androidActivities in bake/src/channels.rs:190,200 and widgets/liveActivities in bake/src/compat.rs:930-942 are also absent from requires).
