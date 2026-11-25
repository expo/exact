# @exact/translation on devices, by hand

LLP 1133.027 §12 stage 3's hand checks: the system's own download sheet and
a language pack removed in Settings, which the agent scripts but cannot
show. Each step says what to see; a step that shows something else is a
failure to report with a screenshot.

Excord's fixture world has a Japanese message to translate: in #general,
open the fixture menu ("Fixture traffic", the ⋯ button in the channel's
header, shown only against the fixture) and choose "Receive a
message in Japanese". Its text is "明日の会議は十時からです。".
Run the fixture server as `packages/local-auth/test/device-checklist.md`
says (on this Mac, with `--host <LAN address>` and
`EXACT_BACKEND_ORIGIN` for an iPhone, `adb reverse tcp:4790 tcp:4790` for
Android).

## Mac

Build: `bun host/apple/build.mjs excord-apple --run`.

1. System Settings → General → Language & Region → Translation Languages:
   remove Japanese if it is downloaded.
2. In Excord, right-click the Japanese message: the menu has Translate.
   Choose it. macOS shows its download sheet for Japanese, over Excord's
   window. Cancel: the line under the message reads "Japanese isn't
   downloaded", and no translation shows.
3. Translate again and choose Download: the sheet shows the download, then
   closes; the message shows "Tomorrow's meeting starts at ten o'clock." (or
   close to it) and "Translated from Japanese · Show Original".
4. Show Original: the translation goes.
5. An English message: its menu has no Translate.
6. Remove Japanese again (step 1) with Excord open, and translate the message
   once more: the sheet asks again (macOS 26) or the line reads "Japanese
   isn't downloaded" (macOS 15 to 25).

## iPhone, iOS 26 or later

Build: `bun host/apple/build.mjs excord-apple --ios --device --run`.

1. Settings → Apps → Translate → Downloaded Languages: delete Japanese.
2. Long-press the Japanese message: Translate. Choose it: iOS's download
   sheet. Cancel: "Japanese isn't downloaded". Translate, Download: the
   translation, as on the Mac.
3. With the translation showing, go to Settings and delete Japanese, return
   and translate another Japanese message: the line reads "Japanese isn't
   downloaded" or the sheet asks again; Excord does not crash or hang.

## iPhone or iPad, iOS 18 to 25

As iOS 26. The download sheet comes from a view Excord hosts out of sight;
check that it shows over the transcript and that cancelling it returns to
the transcript with the line.

## Chrome on a desktop

Chrome 138 or later has `Translator` and `LanguageDetector`.
`bun host/web/dev.mjs --app excord`, opened in Chrome.

1. chrome://on-device-translation-internals: remove the ja–en pack if it is
   installed.
2. Right-click the Japanese message → Translate: "Downloading Japanese…" with
   a percentage, then the translation once Chrome has the pack.
3. Firefox and Safari have no `Translator`: the menu has no Translate.

## Android phone

The Android package uses ML Kit translate 17.0.3 and bundled language ID
17.0.6. Creating a translator downloads the pair's missing models. English
ships with the SDK. Concurrent creates for one pair share the transfer and
the download conditions of the create that starts it. A failed download answers `NetworkError`, as in the
[web model creation algorithm](https://webmachinelearning.github.io/writing-assistance-apis/#create-an-ai-model-object).

1. In Excord on a Pixel, with Wi-Fi on, translate the Japanese message: the
   button should read "Translate with Google". Check "Downloading Japanese…"
   while the pack arrives, then the translation and its adjacent Google
   Translate attribution graphic (D10). The app owns these labels.
2. Kill Excord and open it again: the pair reads `available` at once, and a
   translate needs no network (turn Wi-Fi and data off and translate again).
3. Clear Excord's storage in Settings → Apps → Excord → Storage, open it
   again: the pair reads `downloadable` (ML Kit keeps its models in the app's
   own storage), and translating downloads again. Android has no setting
   that removes one model while the app runs, so the `not-installed` failure
   (a model gone under a live translator) has no hand check here; the
   package reads the installed models before using the translator.

The package drive is `bun packages/translation/test/agent.mjs android`. Set
`ANDROID_SERIAL` to select the emulator. To exercise ML Kit in the built
fixture, run `bun exact.mjs agent android --bind translation` with its tap,
clock and state operations.

The contributor tested on 2026-11-24 with a POCO X6 Pro (Android 14, HyperOS 1.0), not rooted, the fixture's
minified release build, ML Kit `translate` 17.0.3 and `language-id` 17.0.6:

| Step | Result |
| --- | --- |
| ja→en before any download | `downloadable` |
| Create | `downloading` (no progress), then `available` after 2.9 s; `translator-1 ja en google-translate` |
| Translate 明日の会議は十時からです。 | "Tomorrow's meeting is from ten o'clock." |
| Relaunch | ja→en `available` at once |
| Detect ja, ru, ko, empty | `ja 0.993`, `ru 0.9998`, `ko 0.9996`, each with `und` holding the rest; empty `und 1` |
| zh-TW→en, en→en-GB, ja→xx | `unavailable`, `unsupported-options`; a create fails `NotSupportedError` (ML Kit's Chinese is Simplified) |
| Translate after Destroy | `InvalidStateError` |
| `agent.mjs android` | all passed |

ML Kit 17.0.3 writes the source message and the full translation to Android
logcat. The emulator reproduced both under the `native` and `exact` tags.
A bug report or log capture can include both. The contributor also saw
translation output on the POCO over wireless adb.

On the Android 16 arm64 emulator, an ordinary Wi-Fi connection downloaded
`en_ja.zip` from Google's model server in 9.2 s. Android recorded
48,033,822 bytes transferred. The 31 installed files occupied 63,391,975
bytes of file content. The model list changed from `["en"]` to
`["en","ja"]`. The [Japanese pack](https://redirector.gvt1.com/edgedl/translate/offline/v5/high/r29/en_ja.zip) was fetched by ML Kit.

- The sentence above and 1,262 characters in eight paragraphs translated
  online and in airplane mode. The long translation kept all eight paragraphs.
  The detector returned `ja` at 0.9931 for the sentence.
- In airplane mode with Wi-Fi and mobile data off, creating fr→en returned
  `NetworkError`. The pair stayed `downloadable` with no create pending.
  Moving the Japanese pack out of the model directory under a live translator
  made its next translation fail `InvalidStateError`, `not-installed`.
- With both download conditions set, Android queued the request on mobile
  data while unplugged. It had transferred zero bytes on Wi-Fi while
  unplugged and completed after charging resumed.
- With Play services disabled, the installed pair still translated. The
  missing pair answered `unavailable`, `device-not-eligible`.
- Only an app importing `@exact/translation` pays the APK cost. The final
  signed arm64 fixture grew from 76,497,832 to 95,077,691 bytes
  (+18,579,859). The first qualified build measured +18,563,543 bytes
  on the same fixture. Both figures include the bundled language detector
  and the 16,361,032-byte translation JNI library. Without the import, all
  688 APK entry contents matched the parent and the APK had zero translation
  classes.

The shared fixture keeps its Android lock and verification metadata in
`packages/barcode-detection/test/fixture/`. Both Android agent suites build
against these files in strict mode. To update the graph, run
`bun exact.mjs android --write-locks` in the generated fixture app and
copy its `gradle.lockfile` and `gradle-verification.xml` back together.

ML Kit’s default Android provider starts a model-directory migration at
process launch. An APK binding translation uses Google’s manual initialization
at the first bound ML Kit client. App-owned Kotlin that calls ML Kit directly
uses `exact.app.ExactMlKit.initialize(context)` before that call. All clients
share this process-wide initializer.
[Google’s initialization API](https://developers.google.com/android/reference/com/google/mlkit/common/MlKit)

Excord’s attribution UI and the Pixel hand check remain open.

The bundled detector checks that it supports every expected language before
creating a handle. ML Kit has no hints argument, so detection still considers
all its languages. If the SDK cannot read its translation model inventory,
availability reports `model-not-ready` and create returns `OperationError`.
