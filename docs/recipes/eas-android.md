# Recipe: build Android with EAS

Use [native CI preparation](../reference.md#native-ci-preparation) for the
separate Expo config, EAS profile and `.ci/prepare-native.sh`. Declare
`host.android` in the Exact manifest. The image needs JDK 17+ and Android
command-line tools. Set `JAVA_HOME` to the JDK (or put both `java` and
`javac` on PATH), and `ANDROID_HOME` to the SDK. `ANDROID_NDK_HOME` can
select an NDK explicitly; otherwise Exact finds one under the SDK.

In the EAS production environment, create `EXACT_KEYSTORE` as a secret
file variable containing your upload keystore. Create secret strings
`EXACT_KEYSTORE_PASSWORD`, `EXACT_KEY_ALIAS` and `EXACT_KEY_PASSWORD`.
The [reference](../reference.md#import-an-ios-signing-identity-on-a-ci-worker)
also names the fields for EAS-managed credentials.

Create `.eas/build/eas-android.yml`:

```yaml
build:
  name: Exact Android
  steps:
    - eas/checkout
    - run:
        name: Prepare Exact
        shell: /bin/sh
        command: |
          set -eu
          sh .ci/prepare-native.sh android
          yes | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --licenses
          "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" \
            'platforms;android-37.2' 'build-tools;37.0.0' 'ndk;27.1.12297006'
    - eas/install_node_modules
    - eas/resolve_build_config
    - run:
        name: Bundle
        command: |
          set -eu
          bun "$EXACT/scripts/exact.mjs" setup --check --platform android
          export EXACT_BUILD_NUMBER="${ eas.job.version.versionCode }"
          bun exact.mjs android --bundle --json > artifact.json
    - eas/upload_artifact:
        inputs:
          type: application-archive
          path: target/android/app/outputs/bundle/release/app-release.aab
```

Accept SDK licenses only after reviewing them. The setup check names
missing packages and compiler paths.
Gradle uses Exact's settings repositories even if a worker init script adds
project repositories.

`EXACT_VERSION` becomes `versionName`; the resolved number becomes
`versionCode`. A bundle requires all four signing inputs and bakes
production update trust (LLP 1030.003 D6, D8). Android uses the Linux bake:
set `deploy.store.linux` to `"0"` for a binary without Exact updates;
`deploy.store.android` does not disable the bake's update checks. Otherwise,
supply the update signing keys and `EXACT_UPDATE_RECEIPT` (or genesis for a new stream);
the build checks this first.
For an APK, set `EXACT_UPDATE_TRUST=production` and use
`android --release --no-install --json`; its path is
`target/android/app/outputs/apk/release/app-release.apk`. Neither command
contacts adb. JSON reports the artifact path; logs go to stderr.

Run `bunx eas build --platform android --profile exact --non-interactive`
after the account, credentials and build counter are ready. EAS workers
have built the APK and a bundle signed with an EAS-managed upload key this
way; an upload, Play submission and device start have not been run. The
publisher owns the Play record, upload key and submission credentials.
