# Recipe: phone release builds

Run in the app directory. `--json` reports the artifact's absolute `path`,
`target`, `trust` and (Apple) `signing`; failures report `code` and `error`.
Logs: stderr. Releases omit the agent; no `--run`, `--prebuild` or `--test`.

Production bakes with updates need `deploy.signing.keys` and
`EXACT_UPDATE_RECEIPT` (authenticated receipt) or `EXACT_UPDATE_GENESIS=1`
(new stream). Without updates, set the store to `"0"`: `deploy.store.ios`
for iOS, `deploy.store.linux` for Android's shared Linux bake. An absent
Linux store defaults to updates.

**iOS Simulator**, no installation or signing team:

```sh
bun exact.mjs ios --release --no-install --json
```

`--sim <udid|name>` selects it; omit `--no-install` to install.

**Signed iPhone:** needs a paired phone, Apple Development identity with
private key, and profile covering its UDID, bundle id and entitlements:

```sh
DEVELOPMENT_TEAM=<team-id> bun exact.mjs ios --device --phone <udid> --release --no-install --json
```

The team picks an installed profile and identity. `EXACT_PROFILE=<file.mobileprovision>`
and `EXACT_IDENTITY=<certificate-sha1>` override them. If no profile covers
this app and phone, use Xcode's automatic signing for that bundle id and team.

**No team or phone:** an ad-hoc signed device `.app`, no installation:

```sh
bun exact.mjs ios --device --release --unsigned --json
```

No profile, identity or icon needed. `--unsigned --run` refuses: stock iPhones
cannot launch it. `--archive .exact/app.ipa` adds an IPA for re-signing and
requires a square AppIcon of at least 512 px; its path is relative to the
working directory. iOS optimizes for size (games: speed);
`--optimize speed` overrides.

**Android APK**, no adb contact:

```sh
EXACT_UPDATE_TRUST=production bun exact.mjs android --release --no-install --json
```

Omit `--no-install` to install on a device (`ANDROID_SERIAL` selects
one). Maven modules need `--write-locks` once; keep the generated
lock/verification files. Missing locks name the command before compiling.

**Android App Bundle:** supply all four upload-key variables:
`EXACT_KEYSTORE` (file path), `EXACT_KEYSTORE_PASSWORD`, `EXACT_KEY_ALIAS`
and `EXACT_KEY_PASSWORD`. An APK uses them too.
`EXACT_VERSION` sets `versionName` (default `0.1.0`); `EXACT_BUILD_NUMBER`
sets `versionCode` (default `1`, integer 1–2100000000). Raise it per upload.

```sh
bun exact.mjs android --bundle --json
```

Release and production trust, no adb installation.
Under `target/android/app/outputs/`, the bundle is
`bundle/release/app-release.aab`; the APK is `apk/release/app-release.apk`.

With [Play App Signing](https://support.google.com/googleplay/android-developer/answer/9842756),
you supply the upload key. Google holds the app signing key and signs the
APKs delivered to users. This command uploads nothing to Play; submission
remains unverified.

Before a first build, `bun <exact2>/scripts/exact.mjs setup --check --platform ios`
(or `android`) names what it lacks; CI:
[native CI preparation](../reference.md#native-ci-preparation).
