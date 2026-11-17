# @exact/text-detection on devices, by hand

What the simulator and the agent's fixture pictures cannot show: a photo the
camera took of a real page, with its tilt, shadow and curl. The package's
drives (`packages/barcode-detection/test/agent.mjs`, which reads the text
fixtures too) cover the fixture pictures on macOS, an iOS simulator and the
web. Each step says what to see; a step that shows something else is a
failure to report with a screenshot.

Have a printed book open to a page of prose.

## iPhone

Build: `bun host/apple/build.mjs shelf-apple --ios --device --run`.

1. Open a book in Shelf. Tap Add Quote from Photo, then take a photo of the
   page (or choose one you took). The quote sheet lists the page's lines in
   reading order, each with a switch, all on.
2. Turn off the lines outside the passage you want and tap Add: the book's
   notes end with the lines left on, joined by spaces in curly quotes.
3. A photo held sideways (the phone turned 90°): the lines read upright, in
   order, as in step 1.
4. A photo of something with no text (a wall): "No text found in this
   photo."
5. A page in another script (Japanese, Chinese, Korean, Russian): the lines
   in that script when the device's language list holds it (Settings →
   General → Language & Region); otherwise fewer or garbled lines, which is
   expected.

## Mac

Build: `bun host/apple/build.mjs shelf-apple --run`.

1. As steps 1 to 4, choosing a photo of a page from Finder (AirDrop one from
   the phone). The first read after installing a new build can take up to a
   minute while macOS compiles the recognizer for this Mac's Neural Engine;
   later reads take under a second.

## Android (stage 4, device run pending)

The implementation bundles ML Kit's Latin recognizer. It does not download a
model or read non-Latin scripts. The JVM suite stubs the recognizer; it proves
mapping and lifecycle, not recognition. Run the minified APK on an emulator
and a photo of a real page on a phone before claiming either works.

For the smallest test app, from the checkout root on a machine with a rootable
AOSP emulator already running (`ANDROID_SERIAL` names it):

```sh
bun scripts/exact.mjs new /tmp/text254 --bundle-id dev.exact.text254 --with android
cd /tmp/text254
bun exact.mjs add @exact/text-detection
```

Replace `app.ts` with:

```ts
export const appId = 'dev.exact.text254';
export const grants = 'fs.read app:/data';
export const answer = (source: string) => { throw new Error(`no source ${source}`); };
```

Replace `app.contract` with:

```contract
use textDetectorAvailability, detectText, TextDetectorAvailability, DetectedTextList from "@exact/text-detection"

component TextFixture
  state image = "app:/data/quote.png"
  state language = "sr-RS"
  resource available = textDetectorAvailability() as shape TextDetectorAvailability else empty()
  resource english = textDetectorAvailability(languages=["en-US", "fr"]) as shape TextDetectorAvailability else empty()
  resource japanese = textDetectorAvailability(languages=["ja"]) as shape TextDetectorAvailability else empty()
  resource selected = textDetectorAvailability(languages=[language, "en"]) as shape TextDetectorAvailability else empty()
  mutation found as shape DetectedTextList
  action choose(value: string)
    image = value
  action setLanguage(value: string)
    language = value
  action readLanguage
    send found = detectText(image=image, languages=[language])
  action read
    send found = detectText(image=image)
  action unsupported
    send found = detectText(image=image, languages=["ja"])
  view
    column padding=16 gap=8
      input value=image input=choose testId="image"
      input value=language input=setLanguage testId="language"
      text selected.availability testId="selected-availability"
      text selected.languages.join(",") testId="selected-languages"
      text available.availability testId="available"
      text english.languages.join(",") testId="languages"
      text japanese.availability testId="japanese"
      button press=read testId="read"
        text "Read"
      button press=readLanguage testId="read-language"
        text "Read selected language"
      button press=unsupported testId="unsupported"
        text "Read Japanese"
      text (match failure(found) { case some(f) => f.code, case none => "" }) testId="error"
      match found
        case some(result)
          text `${result.width}x${result.height}` font-variant-numeric="tabular-nums" testId="size"
          for line, i in result.texts key=i
            text line.rawValue
        case none
          text "No result"
```

Remove the generated starter's `app.test.contract`, then build with
`mkdir -p target/android/app/icons` (the no-icon workaround in #181),
`bun exact.mjs android --write-locks`, followed by `bun exact.mjs android`.
The second build must pass strict dependency verification. Both use Gradle's
minified release variant. The fixture has no camera or picker dependency.
Set `exact_checkout` to this checkout's absolute path and use the SDK's `adb`:

```sh
adb root
adb wait-for-device
bun exact.mjs agent android --storage text254 --keep-storage "clock data" tree state
adb shell 'mkdir -p /data/user/0/dev.exact.text254/cache/exact/dev.exact.text254/agent/text254/data'
adb push "$exact_checkout/packages/text-detection/fixture/quote.png" /data/user/0/dev.exact.text254/cache/exact/dev.exact.text254/agent/text254/data/quote.png
adb push "$exact_checkout/packages/text-detection/fixture/rotated-quote.jpg" /data/user/0/dev.exact.text254/cache/exact/dev.exact.text254/agent/text254/data/rotated.jpg
adb push "$exact_checkout/packages/barcode-detection/fixture/none.png" /data/user/0/dev.exact.text254/cache/exact/dev.exact.text254/agent/text254/data/none.png
adb shell 'chmod -R a+rX /data/user/0/dev.exact.text254/cache/exact/dev.exact.text254/agent/text254/data'
bun exact.mjs agent android --storage text254 --keep-storage "tap read" "clock data" tree state logs
```

Expect `available`, `en,fr`, and `unavailable` for Japanese. The read must
succeed with upright size 1200×360 and these four lines, allowing differences
in case, whitespace and punctuation:

- It is a truth universally acknowledged,
- that a single man in possession
- of a good fortune, must be
- in want of a wife.

In `state`, each line must have a positive pixel box, four clockwise corners
from top left, and `android.recognizedLanguage` (`en` or `und`, as ML Kit
answers). Repeat the drive with `type image app:/data/rotated.jpg` before
`tap read`: the same upright dimensions and lines. `none.png` must succeed
with an empty `texts`. An absent filename must fail `NotFoundError`;
`tap unsupported` must fail `NotSupportedError`. Add a plain text file to
the same directory and read it: `InvalidStateError`. A path outside
`app:/data` must fail `NotAllowedError` before reaching the package.

Check Serbian-locale availability with the fixture's language field:

```sh
bun exact.mjs agent android --storage text254 --keep-storage "type image app:/data/quote.png" "type language sr-RS" "clock data" "tap read-language" "clock data" tree state
```

`selected` must answer `unavailable`, `unsupported-options`, and an empty
language list; the read must fail `NotSupportedError`. Repeat with `sr`,
`sr-BA`, `sr-ME` and `ms-zsm-Arab`, with the same answers. With `sr-Latn-RS`,
`selected` must instead answer `available`, languages `["sr-Latn", "en"]`,
and the read must succeed on the Latin fixture. Also set an emulator's system
language to Serbian (Serbia), relaunch Shelf, and verify `time.locale` is
`sr-RS` (or an explicit Cyrillic tag). `reader` must be unavailable while
`readerAny` stays available. Add Quote from Photo must remain offered and
read the Latin fixture through Shelf's call with languages omitted. This
does not promise recognition of a Cyrillic page.

Finally run the same minified APK without the agent: the Read button must
work after copying the fixtures to
`files/data/exact/dev.exact.text254/data/` under the app's private directory.
Repeat offline to check the bundled model. Capture the answers, Android
version, APK digest and logcat; a registrar-loading exception is a failure.
This procedure is a handoff, not a recorded device run.

## The web

No browser reads text (D18): Shelf shows no Add Quote from Photo in Chrome,
Safari or Firefox.
