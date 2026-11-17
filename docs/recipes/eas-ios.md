# Recipe: build iOS with EAS

Use the [native CI preparation](../reference.md#native-ci-preparation):
add the separate Expo config, EAS profile and `.ci/prepare-native.sh`.
Add [`.ci/sign-ios.sh`](../reference.md#import-an-ios-signing-identity-on-a-ci-worker)
and its certificate/profile secrets. These are app-owned workflow files;
Exact uses its ordinary archive command (LLP 1030.003 D6, D8).

The app needs `host.ios`, its registered bundle id, and a square icon of at
least 1024 px in `icons` or `host.ios.icon`. Xcode must provide iPhoneOS
27.1 or newer; `DEVELOPER_DIR` selects it when several Xcodes are installed.
No simulator or phone is needed for an app without native-view probes.

Create `.eas/build/eas-ios.yml`:

```yaml
build:
  name: Exact iOS
  steps:
    - eas/checkout
    - run:
        name: Prepare Exact
        command: sh .ci/prepare-native.sh ios
    - eas/install_node_modules
    - eas/resolve_build_config
    - run:
        name: Archive
        command: |
          set -eu
          bun "$EXACT/scripts/exact.mjs" setup --check --platform ios --device
          export EXACT_BUILD_NUMBER="${ eas.job.version.buildNumber }"
          . .ci/sign-ios.sh
          bun exact.mjs ios --device --archive out.ipa --json > artifact.json
    - eas/upload_artifact:
        inputs:
          type: application-archive
          path: out.ipa
```

Set `EXACT_VERSION` in the EAS profile; it becomes
`CFBundleShortVersionString`, while the resolved build number becomes
`CFBundleVersion`. Set `EXACT_NON_EXEMPT_ENCRYPTION` to your app's `true`
or `false` answer for export compliance. If the app uses an Exact update
store, supply its `EXACT_UPDATE_RECEIPT`, or `EXACT_UPDATE_GENESIS=1` for a
new stream. With `deploy.store.ios: "0"`, no receipt is needed.

After linking the EAS project, installing credentials and initializing its
build counter, submit with `bunx eas build --platform ios --profile exact
--non-interactive`. `out.ipa` is relative to the app directory;
`artifact.json` reports its absolute path, production trust and signing
identity. Build logs go to stderr. Missing inputs fail rather than prompt.
Keep credentials out of caches; the [cache guidance](../reference.md#native-ci-preparation)
says when a restored `target/` spares compiling Exact again.

To check packaging locally without Apple credentials, omit the signing
step and add `--unsigned`. That archive has an ad-hoc signature and must
be re-signed before distribution. It is not a store submission.

On an EAS worker these steps have built an `.ipa` signed with EAS-managed
credentials, with the build number from EAS's counter. An upload, a store
submission and a start on a device have not been run with them. Store
records, capabilities, credentials and submission remain the publisher's
work.
