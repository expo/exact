# The Android reader

The Kotlin side of Android's Canvas host (LLP 1076 §3.3), in the repository by LLP 1115.001 D2 (Charlie, 2026-10-11).

An app on Android is its Rust, built for `aarch64-linux-android` (`host/linux`: the runner, the kernel, layout, text
and the paint walk), and this. The Rust records what it paints as an op stream (`host/linux/src/canvas.rs`). The
reader replays that stream onto Android's own Canvas: a `RenderNode` for each row, scroller and animated layer,
hardware bitmaps for pictures, `drawGlyphs` for text the Rust has shaped (below Android 12, the glyphs' outlines:
"Android 10 and 11", below). The two meet at `dev.exact.canvas.Native`,
whose methods `exact_linux::canvas_jni!(Data)` defines for an app (`host/linux/src/canvas/jni.rs`). Nothing versions
the stream or those calls, so both sides are built from one commit. The build holds that as far as it can see
(LLP 1159.002): Gradle runs without file watching, so an edit it was not told of is still read, and the APK is refused
when the library it packaged does not load what the one the build staged loads. A pair that slips past both shows as
one line in the log, `exact canvas: unknown op …; the frame stops here`.

| | |
|---|---|
| `canvas/` | the reader, an Android library, package `dev.exact.canvas`: `ExactActivity`, `ExactView`, `Native`, `Player` (the replay), `Owner` (exact2 on a thread of its own), `Layers`, `Natives` (hosted views), `Fields` (the app's text fields, each an `EditText`), `Surfaces` (the top layer's windows: a `Dialog`, a `PopupWindow`), `Bars` (the top app bar and the navigation bar, each a view over the page), `Keys`, `Scrolling`, `Device` and `Palette` (what the device says), `Requests` (what exact2 asks of it), `ExactTamber` (`Tamber.kt`: a native view's design tokens by name, LLP 1121 D12) |
| `app/` | the shell: one activity over the reader, packaging the app that `-Papp=<name>` names |
| `gradlew`, `gradle/` | the Gradle wrapper (9.8.0), committed. Its jar is Gradle's own (sha256 `238e777f…`, as services.gradle.org publishes it) and the distribution's sha256 is pinned in `gradle-wrapper.properties` |

minSdk the app's manifest's `host.android.minimumOS` gives: 31 (Android 12) without one, 29 at `"10"` ("Android 10
and 11", below). targetSdk 36, compileSdk 37.2, AGP 9.4.1, Kotlin 2.4.10, `arm64-v8a` only. Views, with no Compose,
AppCompat or Material library.

## Where it came from

The benchmark lane wrote it outside this repository. These are the files of
`expo-build-1000:~/bench/lists/exact-v11/android/canvas` at commit `c30c683` (2026-10-11). That repository has no
remote, and its history (55 commits) is not imported: for why a line is as it is, `git log -- android/canvas` there.

What changed on the way in, one commit each:

- `ExactCanvas.kt` (2,299 lines) is `Native.kt`, `Player.kt`, `ExactView.kt` and `ExactActivity.kt`: code moved by
  line range, not rewritten. `Player` is `internal` where it was file-private.
- The benchmark's driver is cut out (below). Its module (`:bench`: `Driver.kt`, `Feed.kt`) and its five apps are not here.
- The package is `dev.exact.canvas` (it was `dev.exact.bench.exactcanvas`), in Kotlin and in the JNI's function names.

## Building an app

    bun scripts/agent-android.mjs build caltrain --apk

It needs the Android SDK (`ANDROID_HOME`, else `~/Library/Android/sdk` or `~/Android/Sdk`) with platform 37.2 and an
NDK (`ANDROID_NDK_HOME`, else the newest under the SDK), a JDK (17 or newer) and the Rust target
(`exact setup` adds it to the pinned toolchain). It:

1. writes the app's launcher icon and TV banner, from its manifest's `icons` (LLP 1163 D6), as resources under
   `app/build/<app>/icons/res/`: from a square `maskable` PNG an adaptive icon, with a `monochrome` glyph as its
   themed layer; without one, a legacy icon from a square `any` PNG; and a banner from a 16:9 `any` PNG. A file its
   slot refuses (smaller than the slot, or a glyph with no transparent pixels) fails the build here, before anything
   compiles; and the launch theme in its `background_color`, where it names one, under `app/build/<app>/launch/res/`
   ([the launch window](#the-devices-colours-llp-1115001-d4));
2. builds the app's `android/` crate for `aarch64-linux-android` at `host-dev` (`--release`: at `release` with fat
   LTO, as a production build links, for measuring). The crate is a cdylib whose source is `exact_linux::canvas_jni!(<the data source>, hatches =
   crate::exact_hatches())`, with a build script that bakes the plan and calls `contract::native::android_entry`,
   which writes the app's hatches' install (`hatches.rs`) and the plan's uses (`uses.txt`) beside it; `apps/caltrain/android` is the one to copy for an app whose data is Rust and
   `apps/shelf/android` for one whose data is TypeScript (three files each, and a line in the root `Cargo.toml`'s
   `members`). A game's is generated in its `.shells/` (`game/app/shells.mjs`). An app with GPU modules (LLP 1009;
   every game) is built as a Linux bake is (`buildBake`): the modules first, then the crate, whose bake records each
   module's product digest. The shared Linux/Android loader checks baked cards against that digest: on
   macOS, signed 64-bit Mach-O code omits the signature and normalizes its three size fields, matching
   the definition in `update/src/gpu.rs` through a parity-tested host copy; other files hash whole. Completed development receipts always
   bind the whole file. Both the process cache and Android's persistent cache distinguish these digests;
3. copies the library to `app/build/<app>/rust/jniLibs/arm64-v8a/libexactcanvas.so`, the GPU module and each of
   `gpu.modules` beside it under its own name, the app's `assets/` to `app/build/<app>/rust/assets/app/assets/`, and a GPU module's shaders
   (`gpu/shaders/*.wgsl`, as a Linux bake packs them) to `app/build/<app>/rust/assets/app/shaders/`, where the host
   registers them from before a canvas is made;
4. runs `./gradlew :app:assembleRelease -Papp=<app> -PappDir=<the app's directory>` here (a game's `.shells/`, whose
   `app.json` is resolved), with `-PappIcon` and `-PappBanner` naming the icon and banner it wrote, with
   `-PgpuModules=<file>,<file>…` naming the GPU modules: the APK then extracts its libraries and leaves
   the modules unstripped, since the host opens each by path and checks its digest; with `-Pcompose=1` when the plan's
   uses name `dates` (a date or time control), which links Compose Material 3 for its pickers; and with the Kotlin
   in the app's `modules/android`, its hatches and module roster, compiled in,

and prints the APK's path. Each build has its own Gradle output and locks: the
shell's build directory (`-PappBuildDir`), the reader's (`-PcanvasBuildDir`), Gradle's project cache
(`--project-cache-dir`) and Kotlin's session files, under `app/build/<app>`, `canvas/build/<app>`, `.gradle/apps/<app>`
and `.kotlin/apps/<app>` for an app of this repository and under the app's own `target/android/` for one outside it, so
builds of two apps at once do not wait on each other or mix outputs. The library is stripped with the NDK it was linked
with (`-PndkPath`). Beside the APK, `exact-build.json` records when the build began, the library and the Kotlin it
compiled: a drive refuses an APK older than its sources by it. `--install` then installs the APK on the device a drive
would use (`--run`: and launches it). The package id and the label are the app's
`app.json` (`app.id`, with `-` as `_`, which a package name cannot hold; `app.name`). `ORG_GRADLE_PROJECT_idSuffix=.dev` in the environment builds a side-by-side package
(`<app id>.dev`). `./gradlew :canvas:assembleRelease` builds the reader alone.

A component fixture may compile the real app's Kotlin with
`ORG_GRADLE_PROJECT_modulesDir=<absolute path to the app's modules/android>`. The default
is the built app's own `modules/android`; this override avoids copying a provider into fixtures.

Then `adb install -r <apk>` and `adb shell am start -n <app id>/dev.exact.app.MainActivity`, or drive it:
`bun scripts/agent.mjs android --reader --app <app> …` (below, "Driving the reader").

**An `exact new` app**'s `bun exact.mjs android` names Android in its `app.json` the first time and builds in the
workspace generated under `.exact/shells/` (`scripts/app-shells.mjs`), whose `android/` crate is Shelf's shape with
`exact_js_bake::build_android` for its build script; an app with its own workspace gains the crate from `bun exact.mjs
update` once its `app.json` names Android. That verb is `build <app> --apk --install`; its
`test android` and `agent android` drive the APK (`--headless`: the Linux host under `adb shell`), and `verify android`
runs the tests, the first screen and the relaunch with screenshots light and dark, without `check`, which this host
does not observe for yet.

**In a dev loop** (LLP 1122 D4): give the launch the loop's page URL as `x_dev_plan` (exact2's `EXACT_DEV_PLAN`),
through `adb reverse` since the loop serves this Mac's loopback only: `adb reverse tcp:<port> tcp:<port>`, then
`adb shell am start -S -n <app id>/dev.exact.app.MainActivity --es x_dev_plan http://127.0.0.1:<port>/` (`-S`:
the environment is read once per process, so a running reader is stopped first). The reader
polls it as the Linux display does (`CanvasHost`'s turn and wakes): a plan the loop compiles replaces the running
one with its state carried, and the build channel brings the design server's live batches, pins and `prefer`s, and
registers the build, so the design tool sees it. A generation whose Rust pair changed is refused with the URL loop's
message and the running app kept: the APK's Rust is baked in its library, so a Rust edit needs a new APK. A drive
takes the same URL as `--url` (`agent.mjs android --reader --url http://127.0.0.1:<port>/ …`), which reverses the
port itself. Drives on the same adb server and device share mappings they create until the last drive releases
them. Mappings set up outside a drive are kept; a conflicting destination refuses the drive. A production bake
opens no build channel.

It builds on a Mac as on the Linux build box (tried 2026-10-12 with an SDK under `~/android`: platform-tools, NDK
27.1.12297006, a JDK, and `ANDROID_HOME`, `ANDROID_NDK_HOME` and `JAVA_HOME` naming them). Gradle fetches what the
SDK lacks when its licences are accepted there: it installed build-tools 36.0.0 and platform 37.2 on that run. The
library goes into that APK unstripped (Shelf's is 69 MB there against 20.5 MB from the build box), which changes
nothing a person sees; a cold start is measured with the stripped one. Shelf also needs the Android Hermes bundle,
which `exact setup` installs.

## A TypeScript data module (LLP 1115.001 §5.2)

Shelf (`apps/shelf`) is the app that defines "Android works", and its data is `app.ts`. Its library
(`apps/shelf/android`) is Caltrain's with two differences: the build script is the Linux host's JavaScript bake
(`exact_js_bake::build(.., "linux")`: the plan, the module as Hermes bytecode, its identity and grants), and the
library links `exact-js` and with it the lean Hermes VM, from the pinned Ibex release's Android Hermes bundle, which
`exact setup` installs and verifies as it does the other targets' (LLP 1107 §4.1). One built from an Ibex checkout
with `bun scripts/hermes-android.mjs build` takes its place (`agent-android.mjs` hands it to Cargo).
The local builder selects API 29, the oldest supported Android, for both Hermes and ICU.
An API-30 bundle can import `pthread_cond_clockwait`, which Android 10 lacks;
changing only the app's linker cannot remove that dependency. The headless linker
continues to read the selected bundle's API from `hermes-input-receipt.json`.
The published v5 pin still names the API-30 archive. Replacing it requires a new
`expo/ibex` release and its verified archive digests; a local build does not publish
or change that pin.

**The Canvas host needed nothing for it.** It boots through the Linux host's own door
(`app::boot_presenter_with_painter`), so the module is made, activated after the first frame is recorded
(`first_pixel`), asked and answered as on desktop Linux and under the headless Android carrier; its replies wake the
reader by the descriptor it already waits on. With the reader's default (`Owner`) the host boots and runs on one
thread, so the VM never crosses one (`owner=0`, the benchmark's compare switch, boots on one thread and runs on
another after a first frame that may have activated the module: not tried with one). What the reader lacked was
outside exact2:

- **The network permission.** The shell's manifest asked for none, so a `fetch` failed at the name lookup
  ("failed to lookup address information"). It now asks for `android.permission.INTERNET` for every app it packages;
  what an app may reach is still its grants' to say. TLS is rustls with the compiled-in roots: the journal says
  "no system trust store" on Android.
- Storage needed nothing: `app:/data` is under the `XDG_DATA_HOME` the reader sets (the app's own files
  directory), and Shelf's library survives a force-stop and a cold start.
- The clock needed nothing: a book added on the phone sorts above the seeded ones.

**What it costs**, on a Pixel 10 Pro XL (Android 16) against Caltrain in the same session, 2026-10-11:

| | Shelf (TypeScript) | Caltrain (Rust) |
|---|---|---|
| APK | 20,531,755 bytes | 12,181,871 bytes |
| `libexactcanvas.so` in it (stripped, stored) | 20,232,952 bytes | 11,882,224 bytes |
| `classes.dex` | 267,980 bytes | the same |
| Cold start, `am start -W` after a force-stop, twenty each, interleaved | median 92.0 ms (quartiles 86 to 96; 79 to 101) | median 90.5 ms (86 to 94; 84 to 96) |
| Boot (`CanvasHost::boot`) | 15.7 ms | 16.6 ms |
| The host's first turn ends, after boot began | 26.2 ms | 24.0 ms |
| Resident memory with the first screen up (`VmRSS`) | 171.3 MB | 172.0 MB |
| Native heap (PSS) | 8.1 MB | 6.3 MB |

The VM, ICU and the JavaScript executor are 8.4 MB of the library. Cold start did not move past its spread: three
earlier sessions of twenty each, on earlier trees, gave Shelf 92, 89.5 and 91.5 ms and Caltrain 93, 94.5 and 92.
Shelf's first turn is 2.2 ms longer than Caltrain's (2.5, 4.5 and 4.0 in those sessions); it holds the first
frame's paint and then making the VM, loading the module and asking it for `library` and `subjectWorks`, and the two
apps' first frames are not the same work, so that is a bound on what Hermes adds and not its measure. The module's
first answer (`library`, after a read of its file) is committed 27 ms after boot began, **61 ms after the process
started** (median of ten; 56 to 65), a few milliseconds after the first frame is displayed (55 ms). Memory is one
reading each, in each of four sessions, never more than 1 MB apart.

`x_log_journal=1` at launch (exact2's `EXACT_LOG_JOURNAL`, `host/linux/src/canvas/journal.rs`) writes the app's
journal to logcat as the host comes to hold each line, with the milliseconds since boot began: that is how the times
above were read (`adb logcat -v time`, from ActivityManager's "Start proc" line). Without it nothing is read or
written.

The published v5 Hermes bundle has ICU Unicode support but no `Intl` (LLP 1107 §4.1).
An Intl-enabled bundle with receipt-bound English ICU data enables Ibex's
`NumberFormat` and `DateTimeFormat` and Hermes's ICU `Collator`. Build one with
`IBEX_DIR=/path/to/patched/ibex bun scripts/hermes-android.mjs build --intl`;
the Ibex builder must support `IBEX_ANDROID_INTL=true` and the ICU data tiers.
The local build targets API 29. Before `build` or `install` changes the current
bundle, the Rust resolver checks the receipt schema, all listed archives and
headers, the bundled compiler and ICU filters against their digests, even when
Intl is disabled. JSI, Boost and base ICU archives must be listed. The candidate’s bytecode
version must match the cached host compiler.
Run `exact setup` first to install that compiler. The published bundle continues
to build without Intl; the bake does not warn about a top-level Intl use.

Not there: a picture from a URL (below, "Not done").

## Driving the reader (LLP 1107 D1)

    bun scripts/agent.mjs android --reader --app shelf --storage s1 "clock data" tree "tap add-book" \
        "type add-title Emma" state "screenshot out.png"
    bun scripts/agent.mjs android --reader --app shelf --test apps/shelf/app.test.contract

drives the app the reader shows, on a phone over adb, by the Linux host's own protocol: no second one. `--reader`
takes the APK `agent-android.mjs build <app> --apk` made (`EXACT_ANDROID_BIN=<apk>` names another), installs it,
launches it under a driver and speaks the agent API's JSON lines to it. `ANDROID_SERIAL` picks the phone, and
`ORG_GRADLE_PROJECT_idSuffix` says the package's suffix, as it did to the build.

**How a line crosses.** The shell's activity attaches an `AgentDrive` at the reader's driver seam (below) when its
launch says `agent=<name>`: a socket of that name in the abstract namespace, served only to adb's own user (shell
or root: any app on the phone can reach an abstract socket). adb forwards it; the carrier's process
(`agent-android.mjs relay`) carries lines between `agent.mjs`'s stdio carrier and the socket. Each request is one
call, `Native.agent(line)`, on exact2's thread, into `host/linux/src/canvas/agent.rs`, which hands it to the handler
the headless carrier uses (`exact_linux::agent::handle`). The drive's `EXACT_AGENT` and `EXACT_AGENT_*` (its store,
epoch, locale, seed and fetch faults), and a `--url`'s `EXACT_DEV_PLAN`, reach the app as `x_<name>` launch extras; a launch whose environment does
not say `EXACT_AGENT=1` answers no request, and a production bake drops that variable (LLP 1069.007 D2).

**What the Canvas host adds to that handler**, which assumes a host with no display:

- *Its paints reach the reader.* The handler paints as it goes, each paint a stream that defines faces, pictures
  and rows the next ones draw with, and the recorder kept one finished stream. A stream finished over one the
  reader has not taken is now queued while a driver holds the host, and `Native.frame` hands the reader the oldest
  first.
- *What it painted was displayed.* A display admits a touch only on what its last displayed frame showed, and the
  handler's paints are not displayed frames: after each request the host takes one display turn, so the next
  request can tap what this one made.
- *The clock is the driver's.* From the first request the host's turns run at the runner's clock, which only a
  `clock` request moves: no timer fires and no motion advances between requests.

**Frame timing** (LLP 1079.000 §6.2). In a driver's launch the reader measures two sources, which `perf frames`
reads as `exact.frame-health.metrics/2` (`health`) and `exact.frame-health/2` (`report`):

- `app` (`exact-app-choreographer`), Exact's frames: each Choreographer turn on exact2's thread is a frame at its
  vsync, whose work runs until the main thread has ingested the stream it published. Its deadline and period are
  Choreographer's (`FrameData`, Android 13 and later); before 13, the next vsync and the display's refresh. A turn whose
  stream waits behind eight others has its work lost and counted (`work.lost`).
- `window` (`android-frame-metrics`), the window's drawn frames from `FrameMetrics`: each from its intended vsync for
  its total duration, against its `DEADLINE` (Android 12 and later; before it lateness is unknown). A window draws
  only when something changed, so this source has work and no frame times. The reader's own scrolling and the layers
  it plays are here.

Neither observes presentation, and a GPU canvas's `SurfaceView` is drawn outside the window. Under the driver's clock
a frame is virtual and nothing is recorded: `perf frames capture start [ms]` lends the display the clock for the
lease, refusing a `clock` request meanwhile, and `capture stop` (or the lease's end) gives the driver its clock back
where the display left it. `perf frames live <ms>` is a lease of its own, which reads both sources from its start to
its end. A production bake collects nothing.

**The operations**, each tried on a Pixel 10 Pro XL with Shelf:

| Operation | On the reader |
|---|---|
| `tree`, `state`, `logs`, `layout` | answered by the handler as on Linux; `layout` is in points, in the page the reader placed (the viewport the system's bars and the keyboard leave) |
| `tap <target>` | the presenter's addressed press, as on Linux: it is not a touch through Android's input, so the reader's own gesture code (its slop, its long-press timer, a field's touch) is not what is tested. A row of an open alert or menu is chosen by its authored node, and so is what a bar stands for: a header's button, a tab, the Back control the Up arrow stands for ("The bars", below) |
| `tap <target> contextmenu` | opens the menu the node names, as a finger held on it does (`Presenter::long_press`) |
| `type <target> <text>` | sets the field's value through the presenter; the hosted `EditText` then shows it, focused, with its keyboard up. Not typed through the keyboard: nothing composes |
| `clock data`, `clock settle`, `clock +<ms>` | as on Linux. What the reader plays itself is outside the clock: a fling, the keyboard's slide, a `Dialog`'s own fade. A drive launches with `lower=0`, so no layer plays on the reader's clock |
| `screenshot <png>` | the device's own picture (`screencap`), taken 400 ms after the operation before it so the reader has drawn: the whole screen, bars and keyboard included, in device pixels. `window` changes nothing; `over … every …` is refused |
| `prefer` | the handler's: it changes what the app is told (`prefers-color-scheme light` turned Shelf light on a phone in night mode), not the phone's settings |
| `perf` | the runner's table by plan site. `perf frames health`, `report`, `capture` and `live` read the reader's own frames in the Apple host's formats (above, "Frame timing"); `perf frames track` answers unavailable |
| `fail fetch`, `pass fetch`, a test's `reload` | work (Shelf's tests use them) |
| `check` | refused by `agent.mjs` for every Android carrier ("the web and iOS hosts carry it") |
| `--plan`, `--world`, a launch URL | refused: the reader boots the plan its APK bakes. `--size` is the phone's. `resize`, a drag and a wheel were not tried |
| `tree --ax` | the reader's own answer ("Accessibility", below): what Android has for the window in front, joined to the views |

Shelf's own tests (`apps/shelf/app.test.contract`, eleven) pass on the reader. They found three things on the way,
each fixed where it was: a value the app sets that a field's keyboard would not admit ("12.5" in a number field)
crashed the reader (below, "Text fields"); the driver's `reveal` before a tap refused a row of an open alert as not
on screen; and `tap … contextmenu` was refused as a popover though this host presents one.

**What it costs.** The handler was dead code in an app's library and is now reachable from `Native.agent`:
0.52 MB more in `libexactcanvas.so` for every app (Shelf's 19,717,432 bytes before it, 20,241,592 after). Nothing of
it runs unless a driver asks, and cold start with it is the table's above.

A drive refuses an APK older than its sources, naming the build (the record beside the APK, above), and installs it
only when the device does not already hold the same bytes. With several devices attached and no `ANDROID_SERIAL` it
refuses to choose. Not built: a drive leaves the phone's night mode, font scale and locale as they are
(`prefer` tells the app, not the phone); and a touch through Android's own input (`adb shell input`, or a
`UiAutomation`) is still the only way to test what the reader does with a finger.

## Attaching a driver

The reader drives nothing by itself. A benchmark, or a test, that scrolls and taps an app from outside subclasses the
activity:

```kotlin
// In the benchmark's own module, which depends on this project's `:canvas`
// (settings.gradle.kts: include(":canvas"); project(":canvas").projectDir = file("<exact2>/host/android/canvas")).
open class BenchActivity(private val key: String) : ExactActivity() {
    override fun drive(scroll: (Float) -> Unit): Drive {
        val driver = Driver(this, key, scroll)
        return Drive { driver.contentReady() }
    }
}
```

`drive` is called once in `onCreate`, where the reader used to construct the benchmark's `Driver` (the host has
begun to boot, the view is not made yet), with the call that scrolls the feed by a distance in pixels
(`ExactView.scrollBy`, `Native.scroll`: the scroller under the viewport's centre, or the one a finger last moved).
The `Drive` it returns is told once, after the first frame following successful startup, even when the page is empty
or only bars draw. The default returns null.

Without a driver the activity no longer does what `Driver` did: show over the keyguard, turn the screen on, keep the
screen on, ask for a 120 Hz display mode, and call `reportFullyDrawn()`. The shell's manifest asks for none of them
either, so a benchmark that runs on a locked phone declares them in its own.

An app's files (the copied assets, what the app stores, the fonts it writes) are in the app's own files directory,
which Android keeps encrypted until the phone is first unlocked. The benchmark kept them in device-protected storage
so that it could run before that; a benchmark that still must run on a phone nobody unlocked needs a reader that
does so, which this one no longer is.

## Scrolling: who owns what (LLP 1115.001 D7.1 to D7.3)

A finger scrolls. The gesture and its physics are the reader's (`Scrolling.kt`), made of the platform's own classes
and nothing imitated: the system's touch slop and fling velocities (`ViewConfiguration`), `VelocityTracker`,
`OverScroller` for the fling, the stretch `EdgeEffect` at the ends, and for the indicator the scrollbar a `View` draws
and fades by itself. The reader has no dependency for it. exact2 recognizes no scroll: its side is
`host/linux/src/presenter/scrollers.rs` and `canvas/scroll.rs`.

**The reader moves the scroller, at once, and exact2 catches up.** A scroller whose rows the stream keeps in a node of
their own (a group: ops 20 and 21) is translated on the UI thread in the frame the finger moved. exact2 is told how
far, mounts and retires rows behind it, and its next stream draws the rows where it then has them; the reader places
the node by the difference. This is what a driver's `scroll` always did, now for the scroller under the finger.

Six calls cross, all in the view's pixels:

| Call | Direction | What it says |
|---|---|---|
| `Native.under(x, y)` (`CanvasHost::under`, `Presenter::reader_under`) | reader asks, at a touch's down | Which scrollers are under the point, innermost first, then the page itself where the document is larger than the viewport (id 0). For each: its id, the axes it takes a drag from here on, its revision, its offset, its farthest offset and the box it shows in. Also the axes exact2's own gestures claim there: a swipe row sideways, a reorder grip or a height handle up and down, a transform drag both, a `pan` whatever its `touch-action` does not give to scrolling. And, from D8 on, the hosted text field the touch is for (below) |
| `Native.scrolled(id, dx, dy, rev)` (`CanvasHost::scrolled`, `Presenter::reader_scrolled`) | reader tells | It moved scroller `id` by that much, knowing revision `rev` of it. exact2 clamps the move to the scroller's travel |
| `Native.scrollDrag(id, ended, vx, vy, rest, restX, restY)` (`CanvasHost::drag_began`, `drag_ended`; `presenter/scroll_drag.rs`) | reader tells | A finger began dragging scroller `id`, at the drag's first move of it (each scroller the drag moves, an outer one the hand-off reaches too); or let go of it, before the fling's first frame, with `VelocityTracker`'s velocity (px/s, positive as the offset grows) and where the scroller will rest (`OverScroller`'s end, or where it is when nothing is thrown). The app hears `scrolldragstart` and `scrolldragend`. From the start the scroller's moves owe no `scrollend` until the lift with nothing thrown, or until the fling stops |
| `Native.refresh(id)` (`CanvasHost::refresh`, `Presenter::reader_refresh`) | reader tells | A pull to refresh of scroller `id` was let go of past its threshold: the app hears `refresh` |
| `Native.flingEnded(id)` (`CanvasHost::fling_ended`, `Presenter::reader_fling_ended`) | reader tells | The fling a drag of scroller `id` threw stopped: at rest, caught by a finger, or ended by the app's scroll. Its `scrollend` is due now |
| `Native.scrollers()` (`CanvasHost::scrollers`, `Presenter::reader_scrollers`) | exact2 tells, with each stream | For every scroller it has told the reader of: its id, its axes, its revision, where it is, the offset its rows were last painted at, and its farthest offset. Null when nothing of it changed; a scroller it leaves out is gone, and the reader forgets it |

**Who is the authority.** While a drag or a fling runs, the reader: exact2 takes each move as told. A scroll the app
makes goes the other way (a `scrollTop` or `scrollLeft` write, `scrollIntoView` of an element, `scrollBy`, a field kept
above the keyboard, a list a reorder drags along): exact2 sees the offset is not where the reader's moves left it and
moves the scroller's **revision** on. The reader then takes the offset told and ends the fling that was moving that
scroller; a move it had sent under the old revision is dropped in exact2. A virtualized list's own correction of its
offset (rows that turned out taller than their estimate) is not such a scroll: what shows stays where it is, the
revision stands and a fling goes on.

**Whose a touch is.** A touch's down, and its moves until it is known to be a scroll, go to exact2 as they always did:
it may be a tap, a press or a gesture of exact2's own. Once the finger has moved past the system's slop:

- along an axis exact2 claims there, the touch is exact2's until it lifts, by the rules it has (swipe rows, reorder,
  `pan`, a transform drag);
- else, along an axis some scroller under it scrolls on, the scroll wins. The reader cancels the touch in exact2
  (`Native.touch` with a cancel): its press, its press feedback and whatever drag it might have begun end there, and no
  press fires. No more of that touch reaches exact2;
- else it stays exact2's.

The drag takes the way the finger went past the slop, as RecyclerView's does. Whose a touch is waits for exact2's
answer for its down. A frame that answers late loses nothing: what came of the touch meanwhile (its moves, a second
finger, a finger that lifted, the lift itself) is kept and taken in order when the answer comes, as it would have been
had it come at once (`Scrolling.next`, `caughtUp`). A hosted field that held the touch gets each held event after the
scroll has seen it, so a field that asks to keep its touch keeps it and one the scroll takes is cancelled where it is.
A touch that ends before its answer while the next one comes down is taken when its answer comes, from its down to its
end, the later touch set aside meanwhile, and a fling it throws stops where the later touch came down. A touch whose
activity stops, or whose view leaves its window, before the answer is forgotten: nothing scrolls or
flings for it after.

A finger that lands on a list while it flings stops it and has it, as on the platform's lists, and exact2 hears
nothing of that touch.

**Nested hand-off.** The drag belongs to every scroller under the finger that scrolls on its axis, innermost first.
Each move goes to the innermost; what it cannot take, because it is at its end, goes to the next one out, and so on
to the page. A fling hands off the same way, frame by frame. An `overscroll-behavior` other than `auto` ends the
chain at its scroller. What no scroller can take stretches the edge of the innermost one that has a node.

**Pull to refresh** (`Refresh.kt`, LLP 1115.001 D12). A scroller with a `refresh` handler is Material 3's pull to
refresh, Compose's `PullToRefreshBox` with its default indicator, short or empty too (exact2 gives it to the reader as
scrolling up and down whatever its range, as UIKit's scroll view with a refresh control always bounces): a downward
drag at the top that no scroller of the drag takes pulls, instead of stretching the edge, a 40 dp disc
(`surfaceContainerHigh`, elevation 3 dp) down from the scroller's top at half the finger's way, with tension past the
80 dp threshold, and its arc and arrow (`onSurfaceVariant`) grow with the pull. The list stays where it is. The
finger's way back takes the pull back first, and on past it the drag scrolls and flings as any other.
Let go past the threshold, the reader sends `refresh` (`Native.refresh`, `Presenter::reader_refresh`) and the disc
settles at the threshold and turns as Material's indeterminate indicator does while the app's `refreshing` is true
(the statement's bit 5); it goes back up when `refreshing` ends, or 350 ms after the release when the app never set
it, as the iOS host ends UIKit's control. Short of the threshold, it goes back up. A pull while `refreshing` is true
stretches the edge, and `refreshing` with no pull shows nothing, as on iOS. Under "remove animations" the disc moves
at once and its arc stands still.

**The keyboard** (`Dismissal.kt`, LLP 1115.001 D12). A drag does to the software keyboard what the first scroller
it moves (or, while it moves none, whose edge it stretches) says in `keyboardDismissMode` (bits 6 and 7 of its flags). Unsaid, nothing, as the platform's lists leave the
keyboard up. `on-drag`: as the drag starts, the focused field lets go of the focus (the app hears a blur) and the
keyboard goes. `interactive`, up and down: once the finger reaches the keyboard, the keyboard follows it, moved by
the platform's control of the IME's inset animation (`controlWindowInsetsAnimation`, Android 11 and later); at the
lift it goes, and the field lets go of the focus, when the finger was moving down or had pulled it more than half
away, and comes back otherwise. The page is laid out for where the keyboard ends, not each step of the follow. Before
Android 11 the finger reaching the keyboard puts it away.

**The ends and the indicator.** The stretch is `EdgeEffect`'s own: it stretches the `RenderNode` it is drawn into, so
while an edge is stretched the scroller's rows are drawn through a node the size of its box (`Scrolling.record`).
`scrollbar-width="none"` suppresses the indicator, including after a live style change. The page follows the root's setting.
The indicator is a `View` over the scroller's box that draws nothing but the scrollbar `View` gives a view that
scrolls, with the theme's thumb, shown as the scroller moves and faded by `View`.

**Scroll snap** (LLP 1008 §9): with what is under a touch, exact2 tells the reader each scroller's snap areas as the
offsets they rest at (`presenter/snap.rs`, `reader_snaps`). When the finger lifts, the drag's innermost scroller rests
where `snapRest` picks from where the platform's fling would have left it (a fling, at the platform's minimum fling
velocity or more, reaches at least the next position its way from where the drag began), and settles there in
ViewPager's time with its quintic ease. At its end and flung on past it, the fling goes on to the scrollers outside. A virtualized list
tells the rows it has not mounted up to 64 either way of its offset; at the release the reader asks for those around the
projection and the first unmounted `always` row on the way (`Native.snapsAround`) and aims the settle again; a `proximity`
list that the rows it told leave free flings as the platform does, and the answer may still snap it. An answer for a touch
that ended, or for a list the app has scrolled since, is dropped. A commit that moves the area a scroller rests on
does not move the scroller: its offset is the reader's.

**The feed** is no longer one scroller chosen once at the viewport's centre. It is the scroller a finger last moved
up or down, or the one a driver's `scroll` found under the centre (found again when its node is gone); it is what
exact2 times its collection passes for and what the reader's row heuristics follow (`Native.feed`).

Not built of D7:

- **A scroller with no node of its own does not move at once** (D7.2). The recorder gives a node only to a scroller
  that scrolls up and down, has rows, and is not itself inside another scroller's row. A list that scrolls sideways,
  a scroller nested in a list's row, and the page itself are drawn inside their surroundings, so they move when
  exact2's paint arrives; the frame waits up to 2 ms for it, as it does for a drag's moves. They have no stretch at
  their ends either. Giving each a node needs the recorder to open a group inside a row's recording.
- A sheet's part in the hand-off is a later stage. A hosted `EditText` takes part as a child of a ScrollView
  does (below).
- A mouse's wheel and a trackpad do not scroll (`onGenericMotionEvent`), and nothing scrolls for TalkBack.
- A pressable under the finger shows its press at the down, not after the platform's tap delay, so a row may flash
  as a scroll starts.

## The device's colours (LLP 1115.001 D4)

An app's colour roles (`Canvas`, `-exact-secondary-label`, `AccentColor` and the rest, LLP 1095) are the device's
Material 3 colours, light and dark, from its dynamic palette. Nothing is tabled: the reader resolves them on the
device and tells exact2, which reports them through the door the Apple host reports its own through
(`roles::set_reported`).

**What the reader resolves** (`Palette.kt`): twelve Material roles under each scheme, with the platform's own
resources and no library. From Android 14 the platform has each as a colour of its own
(`android.R.color.system_primary_light`, `system_surface_container_high_dark`, …) and nothing is derived. Android 12
and 13 have only the tonal palettes, so each role is the tone Material's scheme gives it:

| Role | Palette | Tone, light | Tone, dark |
|---|---|---|---|
| `primary` | `system_accent1` | 40 | 80 |
| `onPrimary` | `system_accent1` | 100 | 20 |
| `secondaryContainer` | `system_accent2` | 90 | 30 |
| `surface` | `system_neutral1` | 98 | 6 |
| `onSurface` | `system_neutral1` | 10 | 90 |
| `onSurfaceVariant` | `system_neutral2` | 30 | 80 |
| `surfaceContainerLow` | `system_neutral1` | 96 | 10 |
| `surfaceContainer` | `system_neutral1` | 94 | 12 |
| `surfaceContainerHigh` | `system_neutral1` | 92 | 17 |
| `surfaceContainerHighest` | `system_neutral1` | 90 | 22 |
| `outlineVariant` | `system_neutral2` | 80 | 30 |
| `error` | none: Material's baseline error | `#B3261E` | `#F2B8B5` |

A palette has a swatch for tones 100, 99, 95, 90, 80 … 10 and 0 (`system_neutral1_100` is tone 90). A tone between
them (98, 96, 94, 92, 6, 12, 17, 22) is the palette's colour at that lightness, which the platform computes
(`ColorStateList.withLStar`).

**The call.** `Native.colors(IntArray)`: the twelve roles light and then the same twelve dark, ARGB, in the order of
the table (`presenter::Material`). Told before the host boots, so its first frame is in them, and again whenever what
the reader reads is not what it last told: each time a reader is made (the activity made, or started after a stop) and
on every configuration change. A new wallpaper changes the palette; the activity does not declare that change
(`assetsPaths`), so Android makes it again and its reader, adopting the host that runs, tells the new palette. Both
schemes are always told: night mode chooses between them in exact2.

**Which of Exact's roles is which** is exact2's to say, in `host/linux/src/presenter/palette.rs`, which has D4's
table and the four rows it adds. A report paints the page again; a role D4 leaves out (the system hues but red and
blue) keeps its table pair.

**One accent.** A control, a focus ring and a filled button paint in the node's `accent-color` where it sets one, else
in `AccentColor` as reported, with `AccentColorText` drawn on it (a check, a switch's thumb, a filled button's title):
the device's `primary` and `onPrimary`. The painter's own blue is desktop Linux's alone, where nothing is reported.
The controls' shapes are still the desktop painter's (D9).

**A press is Material's ripple** (D9; `host/linux/src/paint/ripple.rs`): on a button, a segment, a field that opens
something, a checkbox and a radio (in the 40 dp circle round the mark, from its middle), a list's row and any other
node that presses (a `press` or a link's `href`) with no pressed look of its own (`:active`, `-exact-press-scale`,
`pointerdown`). In what scrolls it waits the platform's tap timeout, 100 ms, so a touch a scroller takes sooner shows
none. It is the framework `RippleDrawable`'s wave, which Compose
Material 3 draws a press with: a soft circle from the touch, in the content's colour at the pressed state's 10%,
bounded by the pressed shape, spreading to rest in 450 ms while the finger is down, its centre moving to the shape's
middle, and fading out over 375 ms once the finger has lifted and those 450 ms are over. No state layer shows beside
it, as Compose shows none for a press; a hover's 8% does. The wave's sparkles are not drawn. With animations removed,
and on the agent's clock, a press is the wave at rest and a release takes it away. The bars' targets ripple too, in
the shape their state layer had (below); the top layer's rows and buttons still show the 10% state layer alone.

**The launch window.** From Android 14 the launch theme names the platform's `system_surface_light` and
`system_surface_dark` (`res/values-v34`, `values-night-v34`), the colour the first frame's unsaid background has, with
the bars' icons set for it. On Android 12 and 13 the platform has no `surface` to name in a theme, and the launch
window is the device theme's own day or night background (`?android:attr/colorBackground`). An app whose manifest
names a `background_color` launches on it instead, and on `color_scheme_dark.background_color` in night mode, as its
iOS launch screen and its web page's first paint do: the build writes a theme over `ExactLaunch` with that colour
behind the window and on Android 12's starting window, and the bars' icons dark over a light colour
(`agent-android.mjs` `androidLaunch`).

## The channel out (LLP 1115.001 D5)

Every other call here goes from the reader into exact2. What an app asks of the platform goes the other way, and all
of it takes one channel: exact2's presenter **queues a request and wakes the reader**, the reader **takes the queue
and acts on the main thread**, and a request that has an answer is **answered or cancelled later by its id**. No
capability has a call of its own.

- **The wake** is the one the reader already waits on: the first descriptor of `Native.fds()` (the executor's, which
  a data source's replies also wake). A request does not wait for a paint: a haptic changes no pixel, and a frame may
  carry no stream. Whoever holds the host (`Owner` on exact2's thread, else `ExactView` on the main thread) calls
  `Native.requests()` after every turn and whenever that descriptor wakes it, and hands the words to the main thread
  (`Owner.asked`, `ExactView.asked`), where `Requests.perform` acts on them.
- **The words** (`Native.requests(): IntArray?`, null when nothing is asked). For each request: its kind, its id, the
  count of words that follow, then those words. A string is its count of bytes and then its UTF-8 padded to whole
  words, low byte first, as the op stream writes one. A reader skips a kind it does not know by its count. Ids start
  at 1 and are never reused.
- **An answer** is `Native.answer(id, String?)` on exact2's thread (`Exact.answer`): the answer's text, or null when
  the request was cancelled. An id that waits for no answer changes nothing. When a reader is made again, exact2
  cancels what the last one was asked and never answered, and tells the new one the state again.

| Kind | Request | Words after the count | What the reader does | Answer |
|---|---|---|---|---|
| 1 | open a link that leaves the app | a string: an absolute `http`, `https`, `mailto` or `tel` URL (exact2 lets no other scheme through) | `ACTION_VIEW` for the web, `ACTION_SENDTO` for `mailto:`, `ACTION_DIAL` for `tel:`. No Custom Tab, which is a library's | none |
| 2 | a haptic (`haptic()`, a pressable's `-exact-press-haptic`) | one word: 1 `selection`, 2 `impact-light`, 3 `impact-medium` (and `impact`), 4 `impact-heavy`, 5 `impact-soft`, 6 `impact-rigid`, 7 `success`, 8 `warning`, 9 `error` | `View.performHapticFeedback` | none |
| 3 | `copyText` | a string | the clipboard (`ClipboardManager`) | none |
| 4 | `share` | one word saying which of the three the app gave (bit 0 the title, 1 the text, 2 the URL), then three strings: title, text, URL | the Sharesheet (`ACTION_SEND` in a chooser) | `shared`, `dismissed`, or why it could not be shown; exact2 journals `share: shared`, `share: dismissed` or `share: refused: …` |
| 5 | whether Back has somewhere to go in the app (D6) | one word: 1 or 0 | registers its `OnBackInvokedCallback` while it has, and unregisters it while it has not, so Back at the first screen is the system's own and leaves with its animation | none |
| 6 | the software keyboard | two words: 0 hidden, 1 shown for an `input`, 3 for a `textarea`; then the hosted field it is for (D8), or 0 for a field exact2 paints | for a hosted field, that `EditText` takes the focus and shows its own keyboard (`Fields.focus`); else `Keys.show` | none |
| 7 | the top layer's surface (D10, below) | its serial; what it is (0 it closed, 1 an alert, 2 a menu); flags (bit 0 Back and Escape dismiss it, bit 1 a tap outside does, bits 2 and 3 the corner a menu grows from: its right side, its bottom); where its window goes in the activity's window and its size (x, y, width, height, pixels; an alert's place is the platform's); its container's corner radius and elevation in pixels, as float bits; a count, then that many words: its picture, an op stream of its own | makes, repaints, moves or dismisses that surface's window (`Surfaces.told`) | none by id: what happens in the window is told by `Native.surface` |
| 8 | whether a displayed element says `wakelock="screen"` (LLP 1125 D11) | one word: 1 or 0; a reader's view starts at 0, so a reader made again is told only a 1 | `View.setKeepScreenOn` on the reader's view, so the screen stays on while its window is shown | none |
| 9 | a date or time control's picker (LLP 1115.001.000 D1) | the control's node; then five strings: its kind (`date`, `time`, `datetime-local`), what it shows now, its `min`, `max` and `step`, each as HTML writes it; then its `accent-color`, opaque ARGB, or 0 for none | asks the node's Kotlin hatch (D3), then shows the activity's `pickers`: Compose Material 3's where the app links it, else the framework's dialogs ("Pickers and hatches", below) | the value chosen in HTML's form, which exact2 sets as a person's choice (`input`, then `change`), or null when dismissed; exact2 journals `picker: dismissed for <node>` |
| 10 | the moments the app's Kotlin hatches hear, and what their clock and input call back (LLP 1115.001.000 D4) | for each, led by its scope: 1 the app or 3 a node, then its moment (0 built, 1 changed, 2 ended), the node, its box (four floats as bits, the document's points), its word and `id` as strings, the count of its `data-*` words and each name and value; 5 a tick, its ticket, the clock (an `f64`'s bits, high word first) and the commit; 6 an `after`, its ticket; 7 input, its ticket, the node, then 1 and a pointer's phase, points and whether it was handled, or 2 and a key's `key`, `code`, down, repeat and handled (`hatches/reader.rs`) | calls the app's `exact.app.Hatches` (`HatchReader.told`) | none: what a hatch asks goes by `Native.hatchAct` |
| 11 | a call of a module a package implements here (LLP 1133.001 D5) | one string: the runner's body, `{"extension", "source"\|"send", "args"}` or `{"extension", "release"}`; or a package picture the runner admitted (LLP 1133.019 D11 item 1), `{"extension", "picture", "width", "height", "fit", "owned", "agent"}` | calls the extension's `later` or `release`, or `picture` (`Extensions.call`, "Module implementations", below) | the reply's JSON, `{"error": code}` for a failure; null fails the call. A picture is answered by `Native.picture` with its `Bitmap`, or with an error code |
| 13 | the session's orientation lock (LLP 1133.029 D6) | 0 for none; or 1, the orientations its types allow as bits (portrait-primary 1, portrait-secondary 2, landscape-primary 4, landscape-secondary 8), whether a holder says `natural`, whether one says `current`, a count, and for each holder its node and its word as a string | sets the activity's `requestedOrientation` (`Orientation.told`): the manifest's mapping for the set, `locked` for `current`, the manifest's value for none; on a display 600 dp wide or wider only `current` | none by id: `Native.orientationApplied` reports what was asked, for `state.orientationlock.applied` |
| 14 | whether a canvas receives pad input (LLP 1133.022 D4) | one word: 1 or 0 | observes controller buttons and joystick motion, consuming them only while 1; Android's fallback keys always pass through for navigation | none |
| 15 | a GPU surface's haptic message (LLP 1009 D10) | the canvas's view id (a surface id is its module's, and two modules count from 1), then a string: the message's JSON as `exact_gpu::haptics` writes it (a texture's `intensity` and `sharpness`, its stop, or a pattern); exact2 sends a stop when the surface goes | `SurfaceHaptics`, on the vibrator ("Haptics", below); nothing while the activity is paused | none |

Kinds 5, 6, 7, 8, 13 and 14 are statements of the app's state, told when it changes and anew to a reader made again; exact2
reads them as the queue is taken (`Presenter::tell_shell`), Back, the wake lock and the orientation lock only after a
commit or (Back) when a surface opens or closes, and none of them during a scroll's own frames.

**Haptics.** Android names occasions, not strengths. Under its constants are a texture tick, a tick, a click, a
heavy click and a double click, and Exact's names choose by those: `selection` is `SEGMENT_TICK` (Android 14;
`CLOCK_TICK` below), `impact-light` `CONTEXT_CLICK`, `impact-medium` `VIRTUAL_KEY`, `impact-heavy` `LONG_PRESS`,
`impact-soft` `TEXT_HANDLE_MOVE`, `success` `CONFIRM`, `error` `REJECT`. Android has nothing for two of them:
`impact-rigid` is the click `impact-medium` is, and `warning` is `REJECT`, as `error` is. The system plays a haptic or
not by the person's touch feedback setting.

A GPU surface's haptics (kind 15) need the vibrator, so the shell's manifest asks for `VIBRATE`, a normal permission.
A texture is a repeating waveform at an amplitude of its `intensity`, played again when the amplitude changes, at
most every 16 ms; of several surfaces' textures the strongest plays, its `sharpness` is not played, and a vibrator
without amplitude control plays no texture. A pattern is a composition of click and tick primitives when every
event is a transient and the device has both (Android 12, which says how long each lasts): from Android 16 each
delay counts from the previous primitive's start, before that from its end, so its duration comes off. Otherwise it
is a waveform on a 10 ms grid. It interrupts the texture, which plays again when its last primitive or slot ends. Both use `VibrationAttributes.USAGE_TOUCH` from Android 13, so the touch feedback setting
applies; before 13 the reader reads that setting and plays nothing when it is off.

**The Sharesheet's answer** is as much as Android tells. The sheet says what was chosen to a sender it is handed
(a target; from Android 15 also its own actions, such as Copy): that is `shared`, though nothing says the share was
finished in the app chosen. A dismissal is not told: when the activity is in front again and the sheet has said
nothing for half a second, the answer is `dismissed`.

**Adding a kind.** In exact2: a variant of `presenter::Request` with the next number, its words in `encode`, and
where it is asked (`Presenter::request`, which wakes the reader); with an answer, a `Waiting` variant and its arm in
`answer_request`. Here: its number and its arm in `Requests.perform`, and `Exact.answer` when it has one. Write the
row above. What layout needs while it runs (a field's metrics, D8) is not a request: a queue taken after the frame is
too late for it.

**Not on it yet:** the file pickers (`showPicker`, `saveFile`, the document pickers) are still refused, and the journal
then says the channel does not carry them yet. `@exact/notifications` has no Android implementation until LLP 1133.004
stage 3. Accessibility events
(D13) will take it too.

`x_log_requests=1` at launch (exact2's `EXACT_LOG_REQUESTS`) logs each request as exact2 queues it and as the reader
acts on it, with what `performHapticFeedback` returned: a haptic and a clipboard write show nowhere else.

## Pickers and hatches (LLP 1115.001.000)

**A date or time control's picker** (D1, D2). A press on an `input type="date"`, `"time"` or `"datetime-local"` is
request kind 9 (above). The reader asks the node's hatch first, then shows `ExactActivity.pickers`: the shell's
`MainActivity` gives Compose Material 3's `DatePickerDialog` and `TimePickerDialog` (`app/src/compose`, on the
device's dynamic colours where available, else Material's baseline scheme, in a `ComposeView` added for as long as
the dialog is up) when the APK links Compose, and the framework's `DatePickerDialog` and `TimePickerDialog`
(`FrameworkPickers`) when it does not. `datetime-local` asks
for the day, then the time. The APK links Compose only when the plan has such a control: `agent-android.mjs` reads
the plan's uses and passes `-Pcompose=1`. The activity stays a plain `Activity`; the Compose picker brings the
lifecycle and saved-state owners a `ComposeView` looks for. Both pickers offer Clear while the control holds a value
(the string is the reader's, `res/values*/strings.xml`), and Material's is in the control's `accent-color` where it
has one, its primary roles that colour's tones, including on Android 10 and 11. Under the agent a press holds the
picker instead (`type @date 2026-10-23`, `tap @date clear`, `tap @date cancel`). `x_log_requests=1` logs each picker's
first frame, from the request and from the touch (`Exact.touched`).

**The field** (D6) is painted by exact2 as Material's filled text field, for a text field, a select and a date or
time control alike. A date control shows its value as the device writes it: `Device.read` sends, with the other
settings, the patterns `DateFormat.getBestDateTimePattern` picks for the locale and the person's clock, ICU's month
and weekday names, eras and day periods, and the zero digit (`Device.dates`), and exact2 fills them in. A field's
`placeholder` is its floating label, painted by exact2; a hosted `EditText` keeps it as its hint, in no colour, for
TalkBack.

**Hatches** (D4, D5). The app's Rust hatches (`modules/linux`, or Rust in `modules/android`) run on this host as on
Linux: `canvas_jni!`'s `hatches` hands the host their install, they connect as the reader takes its first requests
after a displayed frame, and an overlay is painted as a canvas is, as a picture. Beside them, Kotlin in the app's
`modules/android` defines `class exact.app.Hatches : ExactHatches()` (`Hatches.kt`), which hears the same moments
for the same words (`app.json`'s for `android`) as request kind 10: `app`, `element`, `elementEnded`, on the main
thread. Its handle, `ExactElement`, has the node's word, `id`, box and `data-*` words; `click`, `focus`, `blur` and
`input(text)`, which cross by the node's number (`Native.hatchAct`) and run between exact2's turns; and `view`, a
text field's `EditText` where the reader hosts it; `host { context -> view }`, a view of the app's own in the node's
box, made for each activity and shown by `Natives` as element kind 5 wherever the node is painted; and `diagnostics`. The module has exact2's clock and
input: `frames`, `after` and `observe`, each a ticket that `stop()` ends, told back as scopes 5, 6 and 7 of kind 10.
`picker(element, picker)` is asked as a hatched date or time
control's picker opens: it may set the title, the text-entry mode, the mode toggle, the days allowed, the 24-hour
clock, or show its own (`ExactPicker.show`). `apps/native-fixture/modules/android` is one: it logs every moment
under `ExactHatch`, gives its `due` pickers titles, no weekends and a 24-hour clock, and hosts a dial at `spin` that
turns on exact2's clock and stops at a tap. `bun scripts/exact.mjs hatch <word> <app>` writes the class for an app
with an `android/` crate.

An app's optional native module views (LLP 1024) live in the same `modules/android` source directory. Their Maven
libraries are named in `app.json` under `host.android.maven` ("Maven libraries" below). Define
`class exact.app.Modules : dev.exact.canvas.NativeModules` with a `modules: Map<String, NativeModule>` roster;
the APK resolves it only at the first module view. Each module creates and updates its real Android `View`.
`events(view, emit)` binds before its first update; call `emit("change", value)` or `emit("input", value)` with
string values, `emit("message", payload)` with the guest message, `emit("load", "")`, or `emit("press", "")`.
Retired bindings and events for an old node allocation are ignored. A module button receives `performClick()`
for an unscrolled tap that no long press took, at its painted position so a platform popup can anchor to it. Native drags, arbitrary
pointer forwarding and zero-size service modules are not implemented by this button seam.

## Module implementations (LLP 1133.001 D5)

A package that implements a module on Android (`"platform": "android"` in its `exact.json`, with its `class` and its
Kotlin under `sources.kotlin`) is compiled into the APK when the app binds it (LLP 1133.001 D4):
`agent-android.mjs` adds its Kotlin to the build (`-PextensionDirs`) with a roster it writes,
`exact.app.ExactExtensions`, and the permissions the app's grant lines derive (the bake's `reach.android`, from
`runner/src/device.rs`) to the release manifest (`-Ppermissions`). The class subclasses `ExactExtension`
(`Extensions.kt`): `later(name, args, reply)` for each source read and send (`later(name, args, preferred, reply)`
under the agent; a list is answered as `{"exact:answer": [...]}`, since a reply is an object), `release(name)` when a live source's last reader goes and `release(name, args)` when one argument
group's does, `dispose(ids)` for the handles of a reply the runner dropped, `foreground(shown)` when the app goes to
the foreground or leaves it, `preferred(values)` for the agent's values, and `resumed()` when the activity is in front
again, which is where it reads again what the person changed in Settings, since Android sends no event for it. Its
`ExtensionContext` gives the application, the activity in front, `agent`, `now()` (the session's clock, the agent's
under the agent), `label` (`main`, the same at every launch), `data`, `cache` and `temporary` (the directories behind
`app:/data`, `app:/cache` and `app:/tmp`, the drive's store under the agent), `foreground`, `changed(topic)`, which asks the topic's readers again, `wake(event, args, id,
ended)`, which raises one of the interface's wake events and hears its outcome (LLP 1133.017 D8; `woke(id, outcome)`
too), and
`requestPermissions(permissions, done)`: one platform request, answered by `onRequestPermissionsResult`, or by the
next resume when a recreated activity lost the result (LLP 1133.002 D9). Overlapping requests queue in order;
identical permission sets share a result. `done` receives the set of permission names the platform answered, empty
when nothing was asked or the result was lost or cancelled. Record a permanent denial only for a name in that set.
`invoke(handle, handler, args, reply)` runs an app handler on the host's owner thread and delivers its answer on the
main thread. The activity binds the roster's names as it
is made (`Native.bindExtensions`). Outside the agent the host binds them as in production; under it, those a drive
names (`--bind`) as in production and the rest told they run under the agent, which answer an interface that is its
own substitute behind the runner's tickets; the substitute answers every other. Android runs one session a process,
which ends with the process: `destroy(done)` is not called.
`openURL("app-settings:")` opens the app's page in Settings (`ACTION_APPLICATION_DETAILS_SETTINGS`), journaled only
under the agent. `packages/geolocation/android/Geolocation.kt` is one. `permission(name)` and `askPermission(name,
done)` read and ask a runtime permission with the one history every module shares: `denied` once a request was
refused and the platform owes no rationale after it (LLP 1133.008 D8 item 7; the camera's uses it, geolocation's and
the microphone's still keep their own). `tracks` is the session's track table: a package that opens an input adds its
tracks (`TrackEntry`, with `state` and, for a camera, `video`, the running capture), and one that consumes a track
subscribes (`TrackConsumer.live` for audio, `liveVideo` for a camera, whose `VideoSource` takes CameraX use cases
typed `Any`, so the reader links no CameraX).

### Maven libraries: one locked, verified Gradle graph (LLP 1133.008 D8 item 5)

An implementation may name Maven coordinates on Google's Maven and Maven Central in its `exact.json`,
`"maven": ["androidx.camera:camera-camera2:1.6.2", …]` (`packages/camera`, `packages/media-capture`). An app names
libraries its own Kotlin in `modules/android` needs in `app.json`:

```json
{ "host": { "android": { "maven": ["org.apache.commons:commons-lang3:3.17.0"] } } }
```

Both arrays take `group:artifact:version` with a pinned version: no ranges, dynamic selectors, SNAPSHOTs,
classifiers or repository URLs. Neither adds a repository or plugin. A conflict between direct declarations
of the same artifact is refused with both coordinates and their declaration files named. They resolve in the app's
one Gradle graph, with the reader's own AndroidX (Compose, for an app with a date or time control), so a camera
beside a date picker builds; two closures of AndroidX would not. `agent-android.mjs` passes the union of the app's
and bound implementations' coordinates (`-Pmaven`), and the graph is held by Gradle's own means:

- **Locked:** `dependencyLocking` in `STRICT` mode, to the app's `gradle.lockfile` beside its other locks
  (`-PmavenLock`). A module, or a version, the lock does not name is refused by Gradle, naming it.
- **Verified:** Gradle's dependency verification (SHA-256 of every artifact and its metadata, plugins included),
  from the app's `gradle-verification.xml`. Gradle reads that file only from the reader's `gradle/`, so the build
  copies it to `gradle/verification-metadata.xml` for the run and takes it away after (ignored by Git), and passes
  `--dependency-verification=strict`. `gradle.properties` leaves verification off for every other build.
- **Written** once, and again when the app's or a module's coordinates change:
  in the repository directory, `bun scripts/agent-android.mjs build <app> --apk --write-locks` (Gradle's `--write-locks` and
  `--write-verification-metadata sha256`); the app keeps both files with its source. They are written in a Gradle
  user home of their own with nothing resolved in it, as a fresh machine's is: from a warm cache Gradle fetches fewer
  files than a fresh machine verifies (a BOM's `.module` beside its `.pom`), so checksums written there fail a fresh
  build. That home keeps the user home's `gradle.properties` and init scripts and links its wrapper distributions; the
  write downloads the graph once, without a daemon, and the home is removed after. Without the two files, an app whose
  modules or own Kotlin name libraries is refused before Gradle runs. The refusal and `exact add` name the directory and
  command: `in <app dir>: bun exact.mjs android --write-locks` when the app has its own launcher, otherwise
  the repository directory and command above, with `EXACT_APP_DIR` set to the app's directory unless it is
  `apps/<app>`.

An app with no Maven libraries resolves as before, unlocked and unverified. Two such builds at once on one checkout
would share the placed verification file. A library that is not on those repositories stays LLP 1133.001 D9's pinned
`artifacts`. `packages/camera/test/android.mjs` checks each step on a fixture app with CameraX and Compose.

## Text fields (LLP 1115.001 D8)

Each `input` of a text type and each `textarea` is the framework's own `android.widget.EditText` (`Fields.kt`), with
no background of its own, over the container exact2 paints for it. No library: the one subclass (`FieldText`) only
says when its selection moves, which no listener of the class tells. The IME and its composition, autofill's hints,
the selection handles, the magnifier, the floating toolbar and the spell checker come with the class. A field is an
`EditText` whether or not it is focused, so nothing shifts when it takes the focus. exact2's side is
`host/linux/src/fields.rs`, `presenter/hosted.rs` and `canvas/field.rs`.

**Desktop Linux hosts nothing.** A shell that hosts fields attaches when it boots the host: Android's reader does at
`Native.start`, where the JNI finds `Fields.measure` (`canvas::field::attach`). No other host attaches, and with no
shell a field is measured, painted and typed into as it always was. A Markdown editor (`markup="markdown"`) is not
hosted on Android either: it keeps the field exact2 paints, typed into one key at a time through this view's own
input connection (`Keys`, `Native.key`), because an `EditText` shows plain text.

**Measured during layout.** The kernel lays the field out. The text in it is the `EditText`'s to set, so its size
is the `EditText`'s to say: when the kernel measures a text control's text it asks its measurer
(`TextMeasurer::field_text`, the control-measurement seam's new hook; a host that paints its fields answers
nothing), and on Android that calls up into the reader, from the thread layout runs on, and has the answer before
it returns:

    Fields.measure(size px, weight, flags, tracking em, line px, font file, axes, features, text, width px): Long

An `EditText` of that thread's own, never attached, is set as the field is (no background, no padding, no font
padding, the field's size, weight, slant, tracking, line height, face, axes and OpenType features) and measured;
the answer is its width, height and first baseline in pixels. A single line is as long as its text; a `textarea`
wraps at the width offered. Each distinct question is asked once (a form of twenty fields in one font asks two).
It is no request (D5): that queue is taken after the frame. The container's own insets are still the painter's
(its border and padding, `field_chrome`); the `EditText` has none.

**What exact2 tells** (`Native.fields(): IntArray?`, null while nothing changed; taken with each stream, before the
stream is drawn). A count, then for each live field, in tree order:

- its id; its **generation**, moved on when its node is renewed for another row's item (LLP 1078): the view then
  starts over, with no text, selection or undo of the last item's; flags (bit 0 it was painted and the box that
  follows is where it goes, bit 1 it takes no touch and no focus: disabled, or under a route that covers it); and
  the scroller whose rows it was painted among (0: none);
- its content box and the clip it shows through (x, y, width, height each, as float bits, in the view's pixels; a
  negative clip width: none), where the last paint put them;
- the count of the reader's edits that reached exact2, and the value the app shows as of it (a string as the op
  stream writes one);
- its look, eighteen words: what it is (bit 0 a `textarea`, 1 a password, 2 disabled, 3 read-only, 4 no
  autocorrect, 5 no suggestions, 6 italic, 7 Enter submits, 8 another field follows it); its keyboard (0 text, 1
  email, 2 URL, 3 telephone, 4 digits, 5 a decimal number, 6 search, 7 none), from `inputmode`, else `type`; its
  capitalization (0 none, 1 sentences, 2 words, 3 characters); its enter key (0 unsaid, 1 enter, 2 done, 3 go, 4
  next, 5 previous, 6 search, 7 send); its `maxlength` (-1 none); its alignment; the colours of its text, its
  placeholder, its caret, its handles and its selection (ARGB, as the roles resolve in its scheme: `FieldText`,
  `-exact-placeholder`, the accent for the caret, the handles and, thinned, the selection, with an authored `color`,
  `caret-color` or `accent-color` winning); its font's size in pixels, weight, tracking and line height (the
  computed font, whose own starting size is 16 by the device's font scale); and the revision, start and end of
  the selection a command last set (`setSelectionRange`, `select()`);
- five strings: its placeholder, its `autocomplete`, its font's file, its axes (`'wght' 650, 'wdth' 75`;
  empty for none), and its effective OpenType features (`'tnum' 1`). The features combine
  `font-variant-numeric` and `font-variant-ligatures` with `font-feature-settings`, whose values win for tags
  they name. Measurement and displayed fields use the same features; an empty features string clears
  `fontFeatureSettings`, restoring the font's defaults.

A field a statement leaves out is gone, and its view with it. A field that was not painted (its row is out of the
page's view) keeps its view, its focus and its keyboard, out of sight.

**The keyboard's shape.** `type`, `inputmode`, `autocapitalize`, `autocorrect` and `spellcheck` are the `EditText`'s
input type: an email, a URL, a telephone, a number pad with or without a decimal point, a password (masked; of
digits with `inputmode="numeric"`), capitalization as said and none unsaid (the platform's), autocorrect on for
prose unless `autocorrect="off"`, and no suggestions with `spellcheck="false"` (Android has one switch for the spell
checker and the suggestions). `enterkeyhint` is the IME action. Unsaid, a search field's is Search, a field with a
`submit` handler's Go, and any other's the platform's own default for a field: Next where another field follows it,
Done where none does. `autocomplete`'s last token is autofill's hint (`email`, `username`, `current-password`,
`new-password`, `one-time-code`, `name`, `tel`, `postal-code` and the address and card tokens); `off` takes the
field out of autofill. `maxlength` is an `InputFilter.LengthFilter`.

**Where a field is, and how it is drawn.** The view is put where its box was painted, for real: its handles, its
magnifier and the keyboard's cursor anchor are placed by the view's own position. It sits in a `Port` the size of
its clip (the platform hides a handle outside the field's ancestors, so it hides one the clip cuts), and it moves
with its scroller's rows as the reader moves them between exact2's paints (`Fields.place`). The view system draws
none of it: as for `Natives`, the field's own drawing is recorded into a `RenderNode` that the stream draws in the
field's place (op 28, kind 4: `NATIVE view 4 rect radii ""`, in the content box, after the container's fill and
border and under the clips around it). So a field stacks in tree order with what exact2 paints. A field that
invalidates (its caret blinks, a character is typed) has only its node recorded again; Android frees a node's
drawing when a paint leaves it out of the tree, so a field a stream draws again is recorded again
(`Fields.drawn`). A transform over a field moves its box's corner and does not scale or turn the view (as for
every hosted view).

**Whose a touch is.** The reader's view still takes every touch. A down inside a field's box is held, with what
follows it, until exact2 says what is under the point (`Native.under`'s third word: the hosted field the node hit
is, by exact2's own hit test). If it is the field, the touch is the field's from its down on, in the field's own
coordinates, and exact2 hears none of it: the caret lands under the finger, and a long press, the handles, the
magnifier and the toolbar are the platform's. Otherwise (the field is under a bar or a sheet) it goes to exact2 as
any touch does. A drag that begins on a field still scrolls the list around it: past the slop along an axis a
scroller takes, the reader cancels the touch in the field and scrolls (`Scrolling`), unless the field asked its
parents to leave it its touch (`requestDisallowInterceptTouchEvent`: a selection or a caret being dragged), as a
ScrollView would.

**What the reader tells back**, on exact2's thread and in order:

| Call | When | What exact2 does |
|---|---|---|
| `Native.fieldFocused(id, generation, focused)` | the field took the focus (a finger tapped it), or lost it with no other field taking it | its focus follows, with the `blur`, `change` and `focus` that makes |
| `Native.fieldEdited(id, generation, seq, value, start, end)` | the person's edit number `seq` left the field holding `value`, the selection at `start` to `end` (UTF-16 units) | the field's text is that; `input` fires, with the selection |
| `Native.fieldComposing(id, generation, composing, value)` | the field's text began to compose, holding `value`, or its composition ended, committed or cancelled: before the edit that began or ended it | while it composes a statement says the field's own text, and the app's value is held; once it ends, the value the app then shows |
| `Native.fieldSelected(id, generation, start, end)` | the selection moved with no edit | kept for the next `input`; `select` fires for a range |
| `Native.fieldAction(id, generation, action)` | the action key, or Enter or Tab on a hardware keyboard: 0 Enter with no action, 2 done, 3 go, 4 next, 5 previous, 6 search, 7 send, 16 Tab, 17 Shift+Tab | Enter's `key` handlers, then `change` and `submit` for a single line; then `next` and `previous` move to the field after or before (the focus leaves where there is none), and `done`, `go`, `search` and `send` leave the field, which puts the keyboard away. Tab is the page's own Tab |
| `Native.fieldInput(id, generation)` | a finger came down in the field (it is the field's), or a key went down in it | real input on the field: a state held on it (LLP 1122 D8, `hold`, `holdState`) lets go |

What a call says of a generation exact2 has moved on from is dropped.

**Who owns the text.** The field does, while the person edits it. Each edit is counted; a statement says the count
exact2 has reached beside the value the app then shows. The reader takes a told value only when it answers the
field's newest edit and differs from what the field holds. So a commit that sets the value the field already has
changes nothing in it (its composition and its selection stand), and one that sets another replaces the text where
it stands (the field keeps its undo), puts the caret where it was among the text after it, and restarts the
keyboard. **The app's value never ends a composition** (LLP 1115.000 §5, ruling 6): Contract gives an action no way
to know text is composing, and Android's keyboards compose ordinary words, so an action that rewrites what it hears
would end every word at its first letter. The field says when its text begins to compose and when that ends
(`Native.fieldComposing`). While it composes exact2 holds a value the app writes and tells the field its own text,
which changes nothing in it; when the composition ends, committed or cancelled, exact2 tells the value the app then
shows, the last it wrote, after the app has heard the text. The action key commits what is composing first. The rule
is held at exact2's side (`presenter/hosted_tests.rs`) and has not been seen on a phone. **Composing text is
reported as it composes**: `input` fires for every change
of the field's text, the composing region included, as the web reports it and as an Android `TextWatcher` hears it.
Android's keyboards compose ordinary words, so reporting on commit (AppKit's way) would hold each word back until
its space.

**The focus.** exact2 owns it. A focus the app moves (`autofocus`, `focus()`, the action key's Next) reaches the
field through the keyboard request (kind 6, with the field's id): the `EditText` takes the focus and the keyboard
shows. A focus that leaves the fields (a press on a button, `blur()`, Done) hides the keyboard and gives the focus
back to the reader's view, so keys go on to exact2 (`Native.key`). Back closes the keyboard first, which the system
does itself for an `EditText`: the IME takes Back before the reader's callback. A hardware keyboard types into the
`EditText`; its Enter is the action key and its Tab the page's.

**A field that exact2 takes the focus from gives it up itself.** Two roads led the platform to move the focus on its
own, both seen with Shelf on a Pixel and both closed in `Fields.kt`. A field that is covered while it holds the focus
(a route pushed over its own: search, then a tap on a result) was only made unfocusable, which dropped its focus and
left its keyboard up and connected to it: the page on top sat under a keyboard that typed into the field beneath,
and Back left that page before it closed the keyboard. And a field that went while it held the focus (Save in the
add form) left the platform to focus the first field still in the window, which told exact2 as a person's tap would:
the library came back with its search field focused and the keyboard up. In both the field now hands the focus to the
reader's own view first, and the keyboard goes.

**A value the app sets is not the keyboard's to refuse.** A field's filters are the person's limits: its
`maxlength`, and the characters its keyboard's kind admits (a number field's key listener). An app's value went
through the second, so "12.5" set in `inputmode="numeric"` became "125" and the caret, placed by the told value's
length, was past the end: `IndexOutOfBoundsException` in `EditText.setSelection`, the process gone. Shelf's own
test of its pages field found it through the driver. An app's value now replaces the text with no filter at all,
and the caret is kept within what the field holds.

`x_log_fields=1` at launch (exact2's `EXACT_LOG_FIELDS`) logs each edit told to exact2 with its composing region,
and what the keyboard asks of the field's input connection (`setComposingText`, `commitText`, …).

Not built of D8:

- **`autofocus` on a page that opens does not take.** Shelf's add form names its title field and opens with no
  field focused and no keyboard; a tap on the field works. The cause is exact2's, not the reader's:
  `Presenter::after_commit` focuses an `autofocus` node only once it has a box, a display admits a box only after the
  frame that showed it (`display.allows`), and nothing commits after that frame, so the node is never tried again.
  Desktop Linux's display has the same hole.
- **The keyboard comes back by itself.** A person who puts the keyboard away with Back leaves the field focused, as
  the platform does. When exact2 later states that field's focus again (an alert or a menu closed and the focus went
  back to it; the activity came back from the background), the reader shows the keyboard, where a platform field
  keeps it down until it is tapped. exact2 cannot say "focused, keyboard down" on the channel (kind 6).
- **Seen once and not explained:** the action key moved exact2's focus to the next field, whose ring showed, while
  the platform's stayed in the first, so what was typed next went on into the first field. One run in eight of
  Shelf's add form, with Enter sent by `adb shell input keyevent`. `Fields.focus` asks the field for the focus and
  does not check that it took it.

- **A field's views are all made as its page appears**, on the main thread: about 0.6 to 1 ms a field on a Pixel
  10 Pro XL (the `EditText`'s constructor is a third of it, then its input type, its first layout and its first
  drawing). The frame that showed `apps/unsaid`'s form of twenty-one fields took 26 ms, where the same page with
  painted fields took 5 to 8: one slow frame as the page opens. Cold start with a field on the first screen did
  not move (92 ms, as before). Making a view only once its field is near its port, or ahead of need while the
  thread is idle, would bound it.
- **A swipe row, a reorder grip or a `pan` under a field's touch does not get that touch**: exact2 never heard its
  down. A drag there scrolls or stays the field's.
- **A field inside a layer the reader animates** (a transition) is drawn moving, and its view is where the last
  paint put it until the next.
- **A font the app declares by its bytes** has no file to name: the field is set in the device's default family.
- **A password's own width** is measured unmasked, and a `textarea`'s min-content width is its max-content.
- **The Material container** (D9), the floating label, a field's place in TalkBack's order (D13: the `EditText`
  reports itself, with its text, its hint and its bounds, and nothing orders it among the painted content yet), a
  mouse's I-beam, and autofill's save UI.
- **`inputmode="none"`** keeps the keyboard down on focus; nothing else of an app's own keys is built.

## The top layer (LLP 1115.001 D10)

A confirmation and a menu are **windows of the platform's own**, not a layer painted in the page: an alert-shaped
`alertdialog` is a `Dialog` and a menu popover a `PopupWindow` (`Surfaces.kt`), the framework's classes and no
library. What is in each window is exact2's picture of Material 3's component. exact2's side is
`host/linux/src/presenter/top.rs` (what is open, what closes it), `top/shape.rs` (what an authored subtree is) and
`top/look.rs` (Material's metrics, measured and painted), with `canvas/surface.rs` for this host.

**Which host has one.** A shell that makes windows says so (`Presenter::serve_windows`, which the Canvas host does as
it boots). With no such shell (desktop Linux, a headless run, the agent) nothing changes: a popover's invoker and a
dialog's button are refused by the words they always were, and `showModal` is an unknown command.

**What is presented.** The shape rule is the Apple hosts' (`ConfirmationShape.swift`), read here from the kernel's
tree, so it is written twice; it belongs in shared Rust.

- *An alert*: an `alertdialog` (a `dialog`, or a popover) whose rows are text, actions (a `button` with a `press`
  that closes it) and one cancel (a closing `button` with no `press`), at most three buttons; text and the cancel
  alone are the one-button alert. Opened by
  `showModal(id)` or a button's `commandfor` with `command="show-modal"`. Material's basic dialog: `surfaceContainerHigh`,
  28 dp corners, 24 dp padding, 280 to 560 dp wide and 24 dp from the window's sides; the `aria-label` (else the first
  line) as its headline, the rest as supporting text; text buttons at the trailing edge, the cancel first and then
  the actions as written, on a second line where they do not fit one; a `destructive` action in the error role.
- *A menu*: a popover that is no confirmation, each `button` row with a `press` an item. Opened by a button's
  `popovertarget`, or by holding a node that names it with `contextPopover` (its `contextmenu` runs first). Material's
  menu: `surfaceContainer`, 4 dp corners, 3 dp of elevation (the window's own shadow), 112 to 280 dp wide, 48 dp
  rows, a leading symbol where the row's button has one (a check where it is `aria-checked`), a `destructive` row in
  the error role, a line where the author wrote `hr`. It hangs under its invoker from the invoker's start, ends at
  the invoker's end where that does not fit, and opens upward where there is no room below (Compose's
  `DropdownMenuPositionProvider`); a context menu opens where the finger is.
- Anything else (a chooser with no text, a sheet, a `dialog` its author lays out, a popover that is no menu) is
  refused in the journal, and nothing opens.

**How a window is painted: a second stream, in a second view.** A surface is no part of the page's stream. exact2
measures and paints it through a recorder of its own, and the words go to the reader inside the statement of the
surface (request kind 7, above), where the window's view (`Pane`) replays them with a `Player` of its own. The
stream is whole each time: it defines every face it draws with, shares no row, group, layer or picture with the
page, and a reader made again needs nothing the last one was sent. The kernel does not lay a surface out: its size
is Material's, computed from the window exact2 was told (`Native.window`) and the person's font scale, and exact2
tells the reader the size the window is to be.

**What crosses.** One request kind out, one call in, and one more action on a call there was.

| Call | Direction | What it says |
|---|---|---|
| request kind 7 (`Presenter::tell_surface`, `Surfaces.told`) | exact2 tells | The surface that is open, by its serial: opened, painted again (a row lit, another scheme or font scale), moved or sized again, or closed. One surface is open at a time |
| `Native.surface(serial, what, x, y)` (`CanvasHost::surface`, `Presenter::surface_event`) | reader tells, on exact2's thread | What happened in that surface's window: a touch (`what` 0 down, 1 up, 2 move, 3 cancel) at the window's own pixels; the system dismissed the window (8: Back, Escape, a tap outside a menu); the reader could not make it (9); a key (16 Enter, 17 Tab, 18 Shift+Tab, 19 to 22 the arrows, 23 Space). What is said of a serial that is no longer open is dropped |
| `Native.touch` with action 4 | reader tells | The touch that is down has been held still for the system's long-press time (`ViewConfiguration.getLongPressTimeout`), which the reader measures. What names a context menu there opens it, the long-press haptic is asked for, and the touch presses nothing when it lifts |

exact2 does the hit testing: it knows each row's box in the surface, lights the row a finger holds (Material's
state layer, not its ripple), and chooses the row the finger lifts in. A driver reads the open surface from the
agent's `state.surface` (each row's authored node and its box in the window) and `tap`s a row by that node; `tree`
rows are not marked.

**What a window gives, and what closes it.** Seen on a Pixel 10 Pro XL (Android 16):

- The alert is its own window above the activity's (`dumpsys window`: `ty=APPLICATION fmt=TRANSPARENT`,
  `fl=DIM_BEHIND`), with the window manager's dim behind it at Material's 32%, which also darkens the page under the
  system bars. The menu is `ty=APPLICATION_PANEL`, `fl=WATCH_OUTSIDE_TOUCH`. Each takes the window focus.
- Back and Escape go to the window by the system's own routing, and the reader says it was dismissed. A tap outside
  dismisses a menu and does nothing to an alert; a tap on a page button under either presses nothing. An alert's
  cancel, Back and Escape close it and run nothing (unless its `closedby` is `none`); a row's tap closes the
  surface and then runs the row's `press`.
- While a surface is open the page is inert in exact2 as well: nothing of it is hit, keys go to the surface, and
  `Presenter::system_back` closes the surface before any route is asked, with the reader's back callback enabled
  (kind 5). The focus moves in: the page's node loses it (a hosted field's keyboard goes) and gets it back when the
  surface closes.
- **A hosted field cannot take the focus under a surface.** A hardware key ends touch mode, and the platform then
  focuses the activity window's first focusable view, a hosted `EditText`: before this was guarded, Tab in an alert
  put a caret in the field under the scrim and the keyboard rose when the alert closed. The fields' container blocks
  focus while a window is up (`Fields.covered`), and exact2 refuses such a focus (`Presenter::reader_field_focused`).
- Night mode changed with an alert or a menu open paints the same window again; a font scale changed with an alert
  open does too, larger where the text is. The app sent to the background and brought back keeps the same window,
  an alert's or a menu's, and the page is live once it closes. An activity made again is told the open surface anew
  and makes a new window for it.

A surface the reader cannot make (no window to attach to, a kind it does not know) is answered with 9: exact2
closes it, says so in the journal, and the page is live again. exact2's half of that is held by a test; no phone
has refused one.

**Motion** is the platform's. The dialog has what the platform gives its window, with nothing set here. Filmed: it
fades and settles into place over about 180 ms as its scrim fades in, complete in every frame, and leaves in about
140 ms with its scrim gone in about 50.

**The menu is given its motion**, since the platform gives none to a popup it did not place (Charlie's ruling of
2026-10-12: the window keeps Exact's placement, and Android animates it). Checked on a Pixel 10 Pro XL first:

| The same menu | `dumpsys window`, its window's attributes | Filmed at 120 frames a second |
|---|---|---|
| placed by `showAtLocation`, nothing set (as it was) | no `wanim` | whole in one frame, gone in one |
| dropped from an anchor view (`showAsDropDown`), nothing set | `wanim=0x1030304`, the platform's drop-down style | grows over about 150 ms, leaves in about 100 |
| placed by `showAtLocation` with a window animation of its own (as it is) | `wanim=0x7f020005`, this library's | grows over about 130 ms, fades out in about 70 |

So `PopupWindow`'s default animation is the drop-down's, chosen only for a popup shown from an anchor. The menu's
window gets one of four styles (`PopupWindow.setAnimationStyle`; `res/values/motion.xml`, `res/anim/exact_menu_*`),
by the corner it grows from, which exact2 says with the surface (the flags' bits 2 and 3: the corner at its anchor,
as `look::place` put the menu): a scale from 80% at that corner over 120 ms with a 30 ms fade in, and a 75 ms fade
out at its size, as Compose's `DropdownMenu` moved it (those four numbers from memory of its source, not read that
day). The window manager runs it as it runs any window's, so the system's animation scale and "remove animations"
apply. In the film every frame of both has the menu's rows painted: none shows the page through an empty window.

**What it costs** (one Pixel, one run of each). Nothing is made until a surface opens: one window root before any,
and cold start within its spread (median 90 ms before, 93 after, twenty launches each). A surface's first frame is
committed about 35 ms after the touch lifts (alert 30 to 41, menu 29 to 37, five each). A closed window is garbage
until the next collection and is then gone (one root again); resident memory after the first use stays about 8 MB
above what it was before any, which has not been attributed.

**A select's menu** is painted in the page by the shared presenter, for both default appearance and
`appearance="none"`. Its rows show the selected option, skip disabled options and commit a choice on a tap.
A tap outside, Back or Escape dismisses it. Keyboard navigation and a mouse wheel can reach a long list,
but there is no touch scrolling. The menu is clipped to the page's viewport and stays below hosted views
such as `EditText` and WebView and below the system bars. Its rows are not exposed as a separate accessible
menu, and it has no popup-window enter or exit transition. D10's anchored native popup remains unbuilt.

Not built of D10:

- **Accessibility.** `uiautomator dump` shows an open alert as a window of its own holding one focused
  `android.view.View` with no text, description or children: TalkBack lands on it and has nothing to read (D13).
- **A text field inside a dialog.** The shape rule admits text and buttons only, so a dialog with an `input` is no
  alert and is refused; nothing hosts a field in a surface's window.
- A bottom sheet for a chooser or a `dialog` its author lays out (a `modal` route is one: [Sheets](#sheets-and-route-motion-llp-1115001-d10-d12)), a `select`'s anchored native popup, submenus as cascades (a row that opens another
  menu replaces the first), a menu or a dialog taller than the window (it is not scrolled), a raster image in a
  menu row, right-to-left placement.
- The ripple, Material's own type scaling above 100% (sizes are sp taken linearly), and the device's own face.
- An outside tap that dismisses an alert whose author wrote `closedby="any"`: lowering implies `any` for every
  `alertdialog` today, so the two cannot be told apart (LLP 1115.000 D13.4).
- The arrow keys reach a surface; nothing else of D7.6's hardware keys is built for the page.

Not checked on a phone in this menu/dialog qualification: Shelf itself (its Android crate is
`apps/shelf/android`; the fixture uses its shapes); an `alertdialog`
written as a popover, and an `aria-checked` row (neither is in a test either); a font scale changed with a menu
open; rotation with a surface open; a second display; TalkBack. The dialog's width between
its limits is the window less 24 dp a side, which was not compared with a Compose dialog on the same phone, and no
screenshot here was compared with a reference app's.

## Sheets and route motion (LLP 1115.001 D10, D12)

The presenter places these routes itself (`host/linux/src/presenter/stage.rs`, drawn by `paint/stage.rs`), on a
shell that shows bars; desktop Linux and the headless carrier show a `modal` route as written, with no motion.

**A `modal` route is Material 3's modal bottom sheet** (Compose's `ModalBottomSheet` and `BottomSheetDefaults`): as
wide as the window up to 640 dp and centred, its top corners 28 dp, `surfaceContainerLow` under a route that writes no
`background-color`, a 32 by 4 dp drag handle 22 dp below its top (`onSurfaceVariant` at 40 %), over a scrim of 32 %
black that covers the window, the page's bars and the status bar included. It is drawn in the page's surface, not a
window of its own (LLP 1115.001 D10, amended): the page's own walk paints it, so its fields, its touches and
TalkBack are the page's, and the bars of the page beneath are drawn in the page under the scrim while it is up (the
reader is told none). Its heights are `navigationDetent`'s, shortest first: `large` (the default) the window below
the status bar, `medium` half that, a point height or a percentage of the tallest, each with the gesture bar's room
added below; `fit-content` rests at `medium` here. It opens at `navigationInitialDetent`, else the first, and rests
at its tallest while the keyboard is up. Its route is laid out once at its tallest, as Compose lays out a sheet's
content, inset by the handle's 48 dp at its top and the gesture bar at its foot: at a lower detent the rest is below
the window's edge. A `modal` route with no header bar therefore starts below the status bar (expo/exact#234).

It slides up as it opens (350 ms, emphasized decelerate) and down as it leaves (200 ms, emphasized accelerate) from
where it was, its last drawing kept for that (`paint/tape.rs`), so a sheet the app closes slides out too. A drag
anywhere on it that no scroller of its own takes (its handle, a header, a form's rows) moves it once past the
system's 8 dp slop, letting go of what the finger pressed; let go, it rests on the detent its speed carries it to,
or leaves the window. A tap on the scrim dismisses it. Either dismissal closes the sheet whole, with any route
pushed inside it: a root that hears `navigate` is sent to the location beneath the sheet, else Back goes once for
each route it holds. `closedby="none"` keeps it: the scrim takes the tap and the sheet does not go below its
lowest detent. System Back is the route's own (D6), so Back from a route pushed inside a sheet goes to the route
before it in the sheet. While it is up the page beneath is inert, the navigation bar too, and TalkBack's pane is
the sheet, titled by its first heading.

**A route pushed while a sheet is on top is shown in it** (expo/exact#326): laid out at the sheet's width and
height, the sheet keeping the detents of the route that opened it, the page beneath still under the scrim. A
`fullscreen` route ends the sheet's stack: a route pushed over it is a page. A `fullscreen` route presented over a
sheet covers it: the sheet takes no touch and is not TalkBack's pane, the bars are the shell's again, and the sheet
shows beneath the route only where that route's author wrote a background.

**A push and a pop cross on Material's shared X axis**, in the page or inside a sheet: the route that leaves moves
30 dp toward where it goes and fades over the first 35 % of 300 ms, the one that arrives moves in 30 dp from the
other side and fades in over the rest (`FastOutSlowIn`). The top app bar crosses with its route: the bars are drawn
in the page for the crossing. A popped route is drawn from its kept drawing. Under the agent a sheet and a crossing
move on its clock: `clock settle` ends them.

**Predictive Back** (Android 14, `OnBackAnimationCallback`, `Native.backProgress`): the reader passes the gesture's
start, progress and edge, and its cancel. Where Back would pop a route, the route shrinks toward 90 %, moves toward
the side the swipe came from by a twentieth of the width less 8 dp and rounds its corners to 28 dp, the route it
returns to drawn beneath; where Back would close a sheet, the sheet shrinks toward 90 % about its foot. Only what
Back's order would take is previewed: nothing while a menu or a surface of the top layer is open, nor where the
route's Back control is disabled. A cancel puts it back over 150 ms, and so does a commit whose Back handler keeps
the route. Otherwise the commit is Back itself, in its order, and the pop goes on from where the preview left the
route. Nothing commits before the gesture does. On Android 13 Back is heard only when it is done.

With "remove animations" nothing moves: a sheet is at rest at once, a push is a cut, and there is no preview.

Seen on an Android 16 emulator (API 36.1, 2.625 pixels a dp) with an app made for it: a sheet at `medium` and
`large`, dragged up by its handle and flung out, a headerless `modal` route below the status bar, a route pushed
inside a sheet and popped with Back, the keyboard raising a sheet, and a push and a pop crossing, filmed on the
agent's clock. The callback registers as an animation callback (`CoreBackPreview`), but a swipe `adb` injects is not
the system's Back gesture, so the preview itself was seen only in the presenter's tests.

Not built: the hand-off between a sheet and a scroller in it (a list at its top dragged down moves the list's edge,
not the sheet); a tab switch's fade-through; a `fullscreen` route's own entrance; a sheet's window of its own
(the IME's insets move it by ending the page, as they move any page).

## The bars (LLP 1115.001 D11)

A route's `header` is Material 3's **top app bar** and the navigation root's `tablist` is its **navigation bar**
(`Bars.kt`). Each is a picture exact2 paints, replayed in a `Pane` of its own over the page, as a surface of the top
layer is replayed in a window of its own: no library, and none of the framework's widgets. exact2's side is
`kernel/src/bars.rs` (what counts as either), `host/linux/src/bars.rs` (what leaves the page for them),
`presenter/bars.rs` (what each says, where it is, what a touch does) and `presenter/bars/look.rs` (Material's
metrics, measured and painted), with `canvas/surface.rs` for this host.

**One rule, two hosts.** What is header-shaped, and which tablist is the root's, is decided in the kernel
(`exact_kernel::bars`), not here: a route whose first child is a `header` holding exactly one heading; the heading
its title; its buttons before the heading leading and after it trailing; the stack's Back control the platform's
own Back on a pushed route. iOS makes UIKit's navigation bar from the same rule (`HeaderRule.swift`). What each
bar becomes is the platform's: nothing below is UIKit's.

**Which host has them.** A shell that shows bars says so (`Presenter::serve_bars`, which the Canvas host does as it
boots, beside `serve_windows`). With no such shell (desktop Linux, a headless run, the headless Android carrier)
nothing is projected: the header and the tablist are laid out and painted in the page as written.

**Out of the page, as iOS takes them.** After each commit the host reads the first navigation root's routes and
tabs by the rule and tells the kernel what the bars cover (`HostCover`, the Apple hosts' own):

- the header a top bar replaces takes no room and is neither painted nor hit;
- the tablist the navigation bar replaces, likewise;
- each route is inset by the room its bars take: 64 dp at its top where it has a top bar, the navigation bar's
  height at its bottom.

So the page's content starts under the top bar and ends at the navigation bar, and neither bar is in the page's
stream. Both are over it: the top bar at the window's top, reaching under the status bar, whose area is the bar's
own colour; the navigation bar at the window's bottom, reaching under the gesture bar. **A keyboard does not move
the navigation bar.** It ends the page above itself and covers the bar, as it covers a Material app's, and the room
the bar took of the routes goes to nothing while it is covered (`Presenter::fit_bars`).

**The top app bar** (Material's small one; Compose's `TopAppBar`):

| | Here | Material 3 |
|---|---|---|
| Height | 64 dp, under the status bar | 64 dp |
| Colour | `surface`; `surfaceContainer` once the route's content has scrolled; the status bar's area the same | the same two (`OnScrollContainerColor`) |
| Title | the heading, title-large (22/28 sp, regular), `onSurface`, one line cut with an ellipsis; 16 dp from the side, or 4 dp after what leads it (56 dp after the Up arrow) | the same (`TopAppBarLayout`: the title's box at `TopAppBarTitleInset` or after the navigation icon, its own 4 dp padding) |
| Up arrow | on a pushed route: Material Symbols' `arrow_back`, 24 dp in a 48 dp target 4 dp from the side, `onSurface` | the same |
| Buttons | the header's, at the trailing side, 4 dp from it: a 24 dp symbol in a 48 dp target (`onSurfaceVariant`) where the button has a symbol, else a text button (label-large, `primary`, 12 dp padding, at least 58 dp wide) | icon buttons and text buttons, the same metrics |
| Held | the ripple (above), in a 40 dp circle on an icon, a pill on a text button | the ripple, bounded by the icon button's 40 dp circle |

A level-1 heading is the same small bar: Material's medium and large bars, which collapse as the content scrolls,
are S3's, and so is the change of colour as a transition (it switches in one frame here; Compose animates it).
The Up arrow goes Back in Back's own order (D6, `Presenter::system_back`), so it presses the route's authored Back
control where there is one, which is then no item of the bar. A full-screen route (`navigationPresentation="fullscreen"`)
has no Up arrow, and its Back control is the bar's leading item, where Material's full-screen dialog has a close
icon. A `modal` route is a sheet and has no bar: its header stays in the sheet as written (below). A pushed route with no header
of the bar's has no bar at all: Android's Back is the system's, not a bar's button. A route that says
`navigationBar="none"` keeps its header in the page.

**Search in a header** stays in the page, under the bar: a header that holds an `input type="search"` (or a
`tablist`) is not taken out whole. What the bar shows of it (the heading, the buttons, and whatever else holds
neither) is, and the field keeps its place in what is left of the header. Material's own answer is a search bar
that opens a search view, or a search action in the bar; neither is built.

**The navigation bar** (Compose's `NavigationBar`):

| | Here | Material 3 |
|---|---|---|
| Height | 80 dp, over the gesture bar | 80 dp |
| Colour | `surfaceContainer`, the gesture bar's area the same | `surfaceContainer` |
| Destinations | the root tablist's tabs that name a panel, sharing the width 8 dp apart | 3 to 5, the same (`NavigationBarItemHorizontalPadding`) |
| Symbol and label | a 24 dp symbol 16 dp from the top, its label under it (label-medium, 12/16 sp), 16 dp from the bottom | the same |
| Active indicator | a 56 by 32 dp pill in `secondaryContainer` behind the selected symbol | the same |
| Colours | the selected destination's symbol `onSecondaryContainer` and its label `secondary`; the others `onSurfaceVariant`, at 38 % when disabled | the same (`NavigationBarItemDefaults.colors`) |

"Here" in both tables is what was seen on a Pixel 10 Pro XL (Android 16, 2.4375 pixels a dp), from `state.bars`
and its screenshots: the top bar's view is 317 pixels tall (a 66.05 dp status bar and 64 dp), the navigation bar's
254 (80 dp and a 24.2 dp gesture bar); read off the pictures, the indicator is about 156 by 77 pixels and the
title's first glyph about 40 pixels from the side, about 129 after the arrow. "Material 3" is the Compose Material 3 library's `TopAppBar` and `NavigationBar`
and their tokens (`TopAppBarSmallTokens`, `NavigationBarTokens`) as remembered, not read against the library that
day. Both bars' are now read from it at the pinned version: their numbers are the schema's `top-` and `nav-` rows
(`reference/rows.json`), and the Material oracle compares each with Compose's.

A destination tapped again while it is selected and its stack is at its root scrolls that route's content to its
top, as iOS does (in one step: nothing animates it). Above its root the tap is the app's own. At the largest font
scale (2.0) the label is twice as tall and the 80 dp still hold it; past that the bar would grow, as Compose's does.
The bar shows on every route but a sheet's, which comes up over it and dims it with the page.

**What crosses.** One statement out, one call in.

| Call | Direction | What it says |
|---|---|---|
| `Native.bars()` (`CanvasHost::bars`, `Presenter::reader_bars`) | exact2 tells, with each stream | Null while nothing changed. Else a count, then for each bar: which it is (1 the top app bar, 2 the navigation bar), its frame in the window (x, y, width, height, pixels), a count of words and that many: its picture, an op stream of its own, whole (it defines every face it draws with). A bar a statement leaves out is gone |
| `Native.bar(which, what, x, y)` (`CanvasHost::bar`, `Presenter::bar_event`) | reader tells, on exact2's thread | A touch in that bar (`what` 0 down, 1 up, 2 move, 3 cancel) at the bar's own pixels. exact2 does the hit testing: it lights the target a finger holds and chooses the one the finger lifts in |

The statement is taken with each stream (`Owner.Pub.bars`) and not on the channel, so a bar and the page under it
change in one frame. The reader's two views are made with the reader and laid out by hand when a statement says
where they are: a statement often arrives while the page's view is being drawn, where a layout asked of the parent
would come a frame late. They sit in a layer of their own over the page (the window's size, drawing nothing itself
and taking no touch), which the frame's drawing reaches after the page's view and records again in that same pass.
Filmed at 120 frames a second, a tab switch changes the page, the title and the indicator in one frame. A cold
launch was not read frame by frame: it is under the launcher's own opening animation.

**Addressing what is in a bar.** A driver reads the bars from the agent's `state.bars`: each one's frame in the
window, what it says, and each target's authored node and box (`kind` is `up`, `item` or `tab`). `tap` names a node
as it always did: a header's button or a tab is chosen in its bar (the answer says `"bar":"top"` or `"bottom"`),
and on a pushed route the authored Back control is the Up arrow. `tree` still shows the authored header and
tablist, which the page no longer paints. To an accessibility service each bar is one `android.view.View` with its
own bounds and no text, description or children (`uiautomator dump`: `[0,0][1080,317]` and `[0,2150][1080,2404]`
on a Pixel 10 Pro XL), as a surface's window is: TalkBack has nothing to read in either until D13's provider
serves them.

**What it costs** (one Pixel 10 Pro XL, twenty cold starts each, `am start -W` after a force-stop, the two builds
in alternating blocks of ten, each build installed fresh). Against the same main built without the bars
(`db7a98a02`): Shelf's median 89.5 ms before and 89.5 after (quartiles 88 to 93 and 87 to 94); Caltrain's, which
has no bars, 90.0 and 90.0. A session an hour earlier against the older base gave Shelf 94.5 and 95.5 and Caltrain
90.5 and 95.5: sessions differ by more than the bars do. The first frame now carries two more pictures and the
page is laid out once more as it boots; neither shows. Shelf's APK is 70,644 bytes larger (20,506,607 to
20,577,251), Caltrain's 10,420.

Not built of D11:

- **The medium and large top app bars**, which collapse as the content scrolls (S3); the colour change as a
  transition; a centred title; an overflow menu for buttons that do not fit (they are all placed, and the title
  is cut first).
- **The segmented buttons** for a `tablist` in the content, and for one in a header (it stays in the page, as the
  search field does). A `radiogroup` of radios is built (`kernel/src/segmented.rs`, `paint/segment.rs`): Compose's
  `SingleChoiceSegmentedButtonRow`, its numbers the schema's `segmented-button-` rows, seen through the presenter on
  a Mac and not yet on a phone or beside the oracle's `segmented-buttons` specimen. Owed there: Compose's motion
  (the label slides and the check grows as the choice moves; here both change at once).
  **List items** and their dividers (rows as cells land on every host at once).
- **A header's rich title**: the avatar, the subtitle and the tap on the title that iOS shows are not read here.
- **`onSecondaryContainer`** for the selected symbol, badges, a navigation rail on a wide window, and hiding the
  navigation bar for a route pushed while the tablist is hidden (the bar follows the tablist's own `display`).
- **A window whose page does not start at its top.** The top bar is at the window's top and its room is taken
  from each route's own top edge: a route that begins lower in the page (under a banner of the app's own) would
  have the bar over what is above it. A page under `viewport-fit="cover"` that pads itself by
  `env(safe-area-inset-*)` is inset by the bar as well as by its own padding.
- **Right to left**: the Up arrow is not mirrored and the bar's sides are not swapped (the host has no door for
  the direction yet).
- **The content scroller rule** (`navigationScroll`, else the first node under the route that scrolls up and
  down) is written a second time here, in `presenter/bars.rs`: it is the iOS host's `contentScroller`, in Swift.
- A bar's symbols are still the symbol table's paths (D14 is S2's), but for the Up arrow.

## Accessibility (LLP 1115.001 D13)

What the reader shows is painted: the page in one view, each bar and each surface of the top layer in a `Pane`. None
of it said anything to an accessibility service, so TalkBack read an Exact app as one unlabelled view. The reader's
root (`Root`, `Access.kt`) now answers through an `AccessibilityNodeProvider` whose virtual nodes are exact2's own
statement of the tree (`host/linux/src/presenter/access.rs`), which reads the kernel's tree as the Apple hosts do.

**Why the root, and not the page's view.** The bars are over the page's view and reach outside it (the top app bar
fills the status bar's area), and the hosted text fields are its siblings, under it. The root holds all three, fills
the window, and is where a finger's hover arrives first. The page's view, the bars' layer and its panes are marked
not important for accessibility: they are pictures.

**What a node is**, in the order a person reads: the top app bar's nodes (the Up arrow, leading buttons, the title,
trailing buttons), then the page's, then the navigation bar's.

| exact2's node | Android's node | Said by |
|---|---|---|
| text | `android.widget.TextView` | its text |
| a heading | `TextView`, `isHeading` | the flag; Android has no heading level |
| a button; a row of a menu or an alert | `android.widget.Button` | its text; click, and long click where it names a context menu |
| a link (`href`, `role="link"`) | `TextView`, role description `link` | the role description |
| a checkbox, a switch, a radio | `CheckBox`, `Switch`, `RadioButton` | checkable and checked |
| a button with `aria-pressed` | `android.widget.ToggleButton` | checkable and checked |
| a range | `android.widget.SeekBar` | its range; scroll forward and backward step it, set progress sets it |
| progress | `android.widget.ProgressBar` | its range where it has one |
| a named image | `android.widget.ImageView` | its content description |
| a `select` | `android.widget.Spinner` | its name as content description, its choice as state description |
| a tab; a navigation bar's destination | `android.view.View`, role description `Tab` | selected, its place among the tabs; no click while selected, as Material's own |
| a `tablist`; the navigation bar | `android.view.ViewGroup` | a collection of one row, single selection |
| a scroll container that scrolls | `ScrollView`, `HorizontalScrollView` | scrollable; scroll forward and backward; a list's rows give it a collection |
| the Up arrow | `android.widget.ImageButton` | the framework's own words for it (`action_bar_up_description`: "Navigate up"), in the device's language |
| a hosted text field | none: the field's own `EditText` | below |

Hidden, inert and `aria-hidden` subtrees, and what a scroller does not show, are no nodes. A node's description
(`aria-description`, `aria-describedby`) is its tooltip text. A live region (`aria-live`, `role="status"`,
`role="alert"`) is a node with Android's live-region mode.

**Identity.** A node's virtual id is exact2's identity for it: the view's own id, which the runner keeps while the
view lives; under a list's row, a hash of the list's identity, the row's item key and the node's place in the row,
since a row's views may be given to another item. So it is the same node after a commit, a scroll, and a row that
leaves and returns (`presenter/access_tests.rs`). The Up arrow, an alert's headline and its text, and a page that
scrolls as a whole have fixed ids of their own.

**What reports itself is not reported twice.**

- *A hosted text field* is a real `EditText` (`Fields.kt`). Where exact2's tree has one, the provider adds the
  field's own view as the child, in its place in the order, and the field's node says it stands under that parent
  and takes its accessible name as its hint (`Access.Stand`). Android's own node for it says the rest: its text,
  that it is an edit box, its selection, its actions.
- *An alert and a menu are windows.* While one is open exact2's tree is that surface alone, in the window's own
  pixels: its pane answers with it, and the root answers with no node. The alert's window is titled by its headline,
  as the platform's own alert is. Android itself leaves the activity's window out of what a service is given while
  either is up (seen with `uiautomator dump --windows`), so the page is unreachable twice over.
- *Hosted elements* (a web view, a video) report themselves; their layer is added after exact2's nodes. Not tried:
  no app on the reader has one.

**The calls.**

| Call | Direction | What it says |
|---|---|---|
| `Native.access(again)` (`CanvasHost::access`, `Presenter::reader_access`) | exact2 tells, with each stream, only while a service is on | Null while nothing changed. Else the statement's number; the surface whose window the nodes are in (0: the page's); 1 when the tree is told whole; the route that shows and its title; a count of changes, four words each (a node, 0 for the view; what changed: bit 0 its subtree, bit 1 its text, bit 2 its state, bit 3 it scrolled, bit 4 it is a live region that speaks; how far a scroller moved, x and y as float bits); a count of nodes and for each, in reading order: its id, its parent's, its view, its role, its flags, its box in pixels, its range as float bits, its place in its set and the set's size, and three strings (name, value, description). `again`: the reader holds nothing and is told the tree whole |
| `Native.accessAct(id, action, value)` (`CanvasHost::access_act`, `Presenter::access_act`) | reader asks, on exact2's thread, and waits | A service acts on a node: 1 press (the presenter's own press at the node, the path a driver's `tap` takes), 2 hold (its context menu, as a held finger opens it), 3 scroll a scroller on by what it shows less a line, or step a range up, 4 back or down, 5 set a range to `value`. Whether anything took it |
| `Native.accessOff()` (`CanvasHost::access_off`, `Presenter::access_reset`) | reader tells | No service is on any more: exact2 forgets what it told |

**Actions.** TalkBack's double tap is `ACTION_CLICK` on the node, which is `accessAct(id, 1)`; the Up arrow's is the
system's Back. Accessibility focus is kept in `Access` and is not exact2's keyboard focus: neither moves the other. A
hosted field takes its own click as any `EditText` does, which gives it the input focus and the keyboard.

**Events.** A statement lists what changed and only that is sent: a content change for the node whose subtree,
text or state changed (a checked or selected state as a state-description change; a tab that became selected also
sends `TYPE_VIEW_SELECTED`), `TYPE_VIEW_SCROLLED` from a scroller that moved, and a content change from a live
region, which is what TalkBack speaks one by. More than twelve changes, or another window's tree, is told whole: one
content change of the view. A live region that came to say something is still listed then, also when it changed
while a menu or an alert was open over it. The route is the root's pane: its title (the top app bar's) is the root's
pane title, which Android announces when it changes.

**With no service on, nothing is built.** `Owner` asks for the tree only while `AccessibilityManager` says a
service is enabled; with none, `Native.access` is never called, the presenter makes no tree, and the root is a plain
frame with no provider. Cold start, twenty launches each in alternating blocks on a Pixel 10 Pro XL, the build
before this work (an older main, `a0600e637`) against the build with it: Shelf's median 92.0 ms before and 94.5
after (quartiles 86 to 97 and 89 to 98), Caltrain's 88.0 and 88.0. With a service on, a scroll is a statement of
the whole tree every frame; that is not coalesced.

**`tree --ax`** (`AgentDrive`, `ExactActivity.ax`, `Access.ax`). The Linux handler has no window and answered "no
AT-SPI tree". The reader answers a driver as it answers a service: exact2 is asked for the tree, and each node's own
`AccessibilityNodeInfo`, the object Android hands a service, is read back on the main thread in the order a service
walks it (a hosted field's from the field's own view; the open surface's window where one is open). An in-app read
and not `uiautomator dump`, because the dump cannot say a heading, a role description, a range or a live region and
cannot be joined to a view. It follows the provider's own child lists, so that a client outside the app is served
the same nodes was checked apart, with `uiautomator`. `scripts/agent-ax.mjs` has Android's row of the per-platform
table (LLP 1115.000 D14.4).

`x_log_access=1` (a launch extra) logs each statement, each node a service focuses or acts on and each event sent.

**Seen on a Pixel 10 Pro XL (Android 16, TalkBack 17.0.1), 2026-10-12.**

- *With TalkBack off, from outside the app* (`uiautomator dump`, `uiautomator events`): Shelf's library, its add
  form, a book's page, a row's menu and the alert each serve the nodes above in reading order, the fields in their
  places with their names as hints, the Up arrow as "Navigate up", the tabs with their selection; `apps/unsaid`'s
  checkbox and switch change their checked state on a tap and send one state change each from their own node; a
  finger's scroll sends a scroll and a content change from the list; a route sends its pane title. `tree --ax`
  through the carrier returns the same, joined to the views.
- *With TalkBack on*: TalkBack found the tree and put its focus on the top app bar's title, "Library", with its
  rectangle on the title's box; when the add form opened it moved its focus to "Cancel", the first node of the new
  screen.
- *Not seen.* TalkBack reading a screen from top to bottom, its double tap, its scroll action, its focus in an
  alert or a menu, and anything it says. A touch or a key sent by `adb shell input` does not reach TalkBack on this
  phone: it goes to the app as a plain touch (a single injected tap pressed a button with touch exploration on), so
  TalkBack's own gestures could not be made from a shell. Its speech is not in the log without its developer
  setting. A person with the phone, or a virtual input device (`uinput`), is what it takes.

**Short of D13.**

- The two role descriptions, `Tab` and `link`, are English words: the reader has no strings by language, and
  Material ships its own "Tab" in each.
- A swipe row's trailing action (Shelf's Delete) is no node and no custom action; the row's menu has the same
  action.
- A polite live region that arrives with a new route is spoken once where the app has no top app bar, since the
  route is known by the bar.
- `tabindex` and other authored order: the kernel's tree order is the reading order, and nothing else is read.
- Braille, Switch Access and Voice Access get what the nodes give any service (names, roles, click, scroll);
  none was tried.
- The reader's `performAction` (click, long click, scroll, set progress) was reached on the phone only for
  accessibility focus; the presenter's side of each is tested (`presenter/access_tests.rs`).

## Android 10 and 11

An app runs on Android 10 or 11 when its manifest says so: `host.android.minimumOS` `"10"` or `"11"` (absent, `"12"`)
is the shell's minSdk. The reader is built for Android 10 (its minSdk 29, the app's library at the NDK's API 29). A
call of a later release is behind a `Build.VERSION.SDK_INT` check (lint's `NewApi` finds none unguarded), and a class
that names a later release's types is a nested class an older release never loads (`ExactActivity.Back`, `Splash`).
What Android 10 and 11 do in its place is in `Legacy.kt` and `LegacyGlyphs.kt`, and on the Rust side in
`host/linux/src/canvas/legacy.rs`; a newer release never enters them. Only an app below Android 12 keeps the Kotlin of
it (`app/proguard-legacy.pro`); for any other, R8 folds each `SDK_INT` check against the app's minSdk and leaves none.
The Rust of it (12 KB) is in every app's library: the host's Rust is the same for every app.

| | Android 12 and later | Android 10 and 11 |
|---|---|---|
| A glyph run | `Canvas.drawGlyphs` (API 31) | each glyph's outline, which the Rust reads with swash from the file the reader's `Font` maps (`LegacyGlyphs.glyph`, a JNI call once per face and glyph), kept as a `Path` in ems; a run's outlines are filled as one path with the run's paint. A colour glyph is its image: a bitmap strike's (Noto Color Emoji's) at its own size, a COLR version 0 glyph's at 128 pixels an em; the 128 most recently drawn are kept. A COLRv1 glyph is its plain outline |
| JPEG, GIF and WebP | `AImageDecoder` (API 30), looked up at run time | `ImageDecoder` (API 28), the same decoder's Java face, asked for the same size and crop (`Legacy.decode`, from the Rust's decode threads) |
| Edge to edge, the bars' icons | `setDecorFitsSystemWindows`, `WindowInsetsController` (API 30) | `systemUiVisibility` flags and the cutout's short edges |
| Insets, the window's size | `WindowInsets.Type`, `currentWindowMetrics` (API 30) | the root's system window, stable, cutout and gesture insets; `Display.getRealSize` |
| The keyboard, accessibility's nodes and events, a node's value | API 30's | `InputMethodManager`; `obtain()`; the value in its description |
| Haptics `CONFIRM`, `REJECT` | API 30's | the nearest older constants, as AndroidX's compat falls back |
| The device's colours | dynamic colour (API 31) | Material 3's baseline scheme |
| A radial gradient with two circles | API 31's constructor | the end circle, the start circle's radius folded into the stops when they share a centre |
| Back | `OnBackInvokedCallback` (API 33) | `onBackPressed` (also on Android 12) |

Not there below Android 12: a backdrop's blur (`RenderEffect`; what is behind shows unfiltered, as a hand-built
Android 10 app has no blur either), the overscroll stretch and its distance (`EdgeEffect`'s, API 31; Android 10's
glow is not drawn, so an end shows nothing), Bold Text (the setting is Android 12's) and the starting window's quick
exit. A glyph's outline is filled as a path, unhinted and at its exact position, where `drawGlyphs` draws it from
HWUI's glyph atlas.

Tried on 2026-10-17 on a Galaxy Note 9 (Android 10, Adreno 630) against a Pixel 5 (Android 14) showing the same
screen with `drawGlyphs` and `AImageDecoder`: Noto Serif's fi, fl, ffi and ffl ligatures, Arabic's joined forms,
Devanagari's conjuncts and reph, a synthesized oblique, and emoji with a skin tone, a flag and a family sequence draw
the glyphs `drawGlyphs` draws, and a JPEG, a WebP and two animated GIFs decode, whole and cropped. With both
fallbacks forced on the Pixel, the same screen's text differs from `drawGlyphs` only along glyph edges (0.5 to 3 %
of a line's pixels), the outlines carrying 1 to 4 % more ink by coverage; its emoji are not drawn, since Android 13
and later ship Noto Color Emoji as COLRv1 (Android 10 and 11 ship its CBDT build).

What it costs, in Caltrain's release APK against the same tree without this work (10,555,651 bytes): at the default
`minimumOS`, `classes.dex` +1,440 (`ExactActivity.Back` and `Splash`) and `libexactcanvas.so` +11,776 (5,070
compressed; `canvas/legacy.rs` and the run-time lookup of `AImageDecoder`). At `"10"`, `classes.dex` is +17,604 (the
checks, R8's outlines of the newer calls, `Legacy`, `LegacyGlyphs`) and `resources.arsc` +256 (+22,932 in all, 0.2 %),
in the one APK every release installs; at run time a newer release pays the `SDK_INT` checks alone.

Not tried: Android 11 (no device), and a TypeScript data module: the Android Hermes bundle is built at the NDK's API
30 (LLP 1107 §4.1), and whether its archives link and load at API 29 is unknown.

## Android TV (LLP 1115.001.003)

The phone's APK also runs on an Android TV, and its manifest puts the app in the TV launcher with the app's banner: a
16:9 `any` PNG of its manifest's `icons` (LLP 1163 D6), else a plain one (`app/src/main/res/drawable/banner.xml`).
Build it as for a phone, then install it on a TV emulator:

```sh
bun scripts/agent-android.mjs build caltrain --apk
adb -s <serial> install -r host/android/app/build/caltrain/outputs/apk/release/app-release.apk
adb -s <serial> shell am start -a android.intent.action.MAIN -c android.intent.category.LEANBACK_LAUNCHER \
  -n com.exact.caltrain/dev.exact.app.MainActivity
```

On a TV:

- **Device facts.** The reader tells the host that the device is a TV (`Device.tv`, from the UI mode's type), and
  the app reads `pointer: none` and `hover: none`, as on tvOS. The page is 1080 CSS pixels tall, as Apple TV's is
  1080 points (`Device.scale`): a 1080p TV draws one pixel per CSS pixel, not Android's two per dp. Its safe area
  is 5% in from each edge, Android TV's overscan margin (`presenter/window.rs`, `tv_margin`). A TV image has no dynamic colour, so the colour
  roles are Material's baseline scheme (`Palette.read`).
- **D-pad.** The arrows reach `key` handlers as `ArrowUp`, `ArrowDown`, `ArrowLeft` and `ArrowRight`
  (`Keys.remote`, `Native.namedKey`). Their default action moves the focus by Android's `FocusFinder` rules
  (`host/linux/src/presenter/remote.rs`).
- **Select.** DPAD_CENTER is heard as `Enter`. It presses the focus, toggles a checkbox or switch, and checks a
  radio.
- **Back.** Back keeps the phone's order. With no route to go back from, it presses the shown button with
  `aria-keyshortcuts="Escape"`, as tvOS's Menu does.

A drive with `--reader` restarts the app, and its `type … key ArrowDown` reaches the presenter, not Android's input.
To test the real keys, send them with `adb shell input keyevent KEYCODE_DPAD_DOWN` while the drive waits on real time
(`"clock +15000 real"`). `focusGuide`, the media keys and text entry with the TV's keyboard are not built.

## Still the benchmark's

The cut was the smallest one. Left in the reader:

- its 57 launch extras, and the `mi_*` and `x_*` families: compare switches (`owner=0`, `async=0`, `retain=0`, …)
  and `scenario`, `freeze`, `start`, `kinds` and `live`, with the `BENCH_*` variables they set (`Native.start` takes
  `live`);
- `startPitchDp`, the tag view (`tag=1`) and the coverage counters (`cover=1`);
- the heuristics that follow one scroller, the feed (which rows are recorded on the main thread, which are made
  first, where a row's platform view exists): a second scroller that scrolls has none of them;
- comments that speak of the benchmark's apps and its measurements.

## Not done

- **`mix-blend-mode`.** The stream has no blend, so a blended SVG element draws over what is below it; the recorder
  says so once on the log and the schema's row records it (LLP 1159.000). The pictures the paint walk rasterises itself
  (SVG filters, `text-shadow`, `-webkit-text-stroke`, `background-clip: text`) are placed as a canvas's is: seen on an
  emulator, and what an animated one costs is not measured (`QUEUE.md`, "Android's island pictures").
- **Most of LLP 1115.001.** The controls have the desktop painter's shapes, and a sheet or a chooser is refused.
  What is built of it:
  - *The reader answers accessibility services* (D13): the section above, with what was and was not seen of
    TalkBack and what is short of it.
  - *A header is the top app bar and the root tablist the navigation bar* (D11): the section above, with what is
    not built of it.
  - *An alert and a menu are windows* (D10): the section above, with what is not built of it.
  - *A text field is a real `EditText`* (D8): the section above, with what is not built of it.
  - *The device's colours* (D4, colours only) and *the channel out* (D5): the two sections above. The type ramp
    is not reported yet.
  - *A finger scrolls* (D7.1 to D7.3): the section above, with what is not built of it.
  - *An authored swipe yields the system's edges* (D7.5): a swipe row's own gesture does not start from the sides
    the system's Back gesture starts from, by the system-gesture insets `Native.window` tells, in place of the fixed
    20 points the other hosts yield. With a button for Back there is no such edge, and nothing is yielded.
  - *Edge to edge* (D3.3, D3.4, D3.6). The reader tells exact2 the window's size, the display's density and the
    system's raw insets (bars, cutout, keyboard, gestures; `Native.window`) and pads nothing. exact2 says where the
    page is, what colour is behind it and each bar's icon appearance (`Native.chrome`); the reader puts the view
    there and makes that colour the window's background. `status-bar-style="light-content"` requests light status
    bar icons, `"dark-content"` dark icons, and `"auto"` the scheme where the winning declaration sits (LLP 1105 D7).
    An invalid bound value counts as unset; without a declaration the status bar follows the page's scheme.
    Navigation bar icons always follow the page's scheme. The sixth chrome word packs the dark scheme in bit 0
    and light status bar icons in bit 1, without changing the JNI array's size. Both API 30's appearance flags and
    API 29's system UI flags apply the two independently.

    Resolution follows iOS's rule: among declarations in the shown scope, a descendant beats its ancestor;
    between branches, paint rank and then sibling order decide. Hidden routes, unselected tabs, `display: none`,
    computed `visibility: hidden`, viewless inline text and projected dialogs/menus do not compete. A lifted
    route header counts at its route's place. The selected `modal` or `fullscreen` page excludes declarations
    painted behind it; declarations painted above it still compete. A `display: none` navigation subtree or one inside
    a dialog/menu projection cannot capture the bar. Without a covering page, the shown route and the root's
    own nodes compete. `layout.statusBar`
    reports the resolved style (`light-content`, `dark-content` or `default`) and source node, as a request,
    not an observation of the system's icons, through both Android agent carriers.

    Differences from iOS: a `fullscreen` route covers the bar while selected; a `modal` one is a sheet below it,
    so the bar stays the page's, though the sheet's own declarations still compete with the page's. A sibling overlay
    painted above an Android modal can win; iOS excludes surrounding root nodes from its presented navigation
    view's scope. There is no native sheet detent, compact-width trait or zoom presentation to consult:
    iOS leaves the bar to the screen behind a medium sheet or an iPad sheet.
    There is also no native navigation/presentation transition completion to wait for, so the request changes
    with committed state. `status-bar-animation` is not applied on Android.

    `system-bars-hidden` resolves by the same walk, apart from the style: `false` is a declaration, so a
    descendant's `false` shows the bars its ancestor hides, and a covering page that declares nothing shows them.
    Bit 2 of the sixth chrome word carries it. `ExactActivity` hides the status and navigation bars with
    `WindowInsetsController.hide(systemBars())` and `BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE`, as a hand-built
    immersive screen does (API 29: `Legacy`'s sticky immersive flags), and shows them again when no declaration
    hides them; it acts on a change only, so a chrome that moves the page does not hide bars a swipe has just
    shown. The insets are the system's: hidden bars report none, so the safe area keeps only the display cutout's
    and the page grows into the space the bars left (API 29's stable insets keep the bars'). `layout.statusBar`
    reports `hidden` and `hiddenSource` beside the style.

    The software keyboard ends the page at its top, and the field being edited stays in view (once the page has been painted in the
    viewport the keyboard left: the boxes a reveal goes by are the last paint's). The keyboard's own slide is not followed
    frame by frame (`WindowInsetsAnimation`).
  - *The device's settings* (D3.1, `Device.kt`, `Native.configure`): night mode, the font scale (the root font size
    is 16 times it; Material's type ramp is a later stage), the locale, "remove animations", high contrast, and that
    the screen is a finger's. The 12 or 24-hour clock is `exactTime().formatLocale`'s hour cycle: where it differs
    from the locale's own, the locale with `-u-hc-h12` or `-u-hc-h23`, as Apple tells it. The layout direction and
    bold text cross too and change nothing yet: exact2 has no door for them.
  - *Back, in order* (D6, `Native.back`): from the navigation root's selected route, the Back control the root names,
    else the root's `navigate`, else the runner's own `back`. The reader takes Back only while exact2 says it has
    somewhere to go (a request, above), so at the first screen Back is the system's own and leaves the app with
    its animation. An open alert or menu closes first (D10): its window takes Back by the system's own routing.
    From Android 14 Back is previewed as the gesture goes ([Sheets and route motion](#sheets-and-route-motion-llp-1115001-d10-d12)).
  - *A finger is a finger* (D7.4, D7.6). Each touch says what touched (`Native.touch`'s tool type), so a finger is a
    `touch` pointer to the app, hovers nothing, and leaves no focus ring on the button it presses (a text field keeps
    its own focus ring). The system's touch slop, told with the settings, is how far a touch moves before it is a
    drag. A GPU canvas that holds the touch (its first finger landed on the canvas) takes every other finger that
    lands on it (`Native.finger`, `Fingers.kt`, `surfaces/fingers.rs`): each is a pointer of its own there, numbered
    from 2 and never reused, as on iOS, and stays the canvas's after the first finger lifts; a finger landing
    anywhere else is nobody's. A cancelled touch, one a scroll took among them, cancels each finger the canvas took.
    Unlike iOS, a finger on another canvas or on a control over the canvas is not that one's. Any other touch is
    one pointer still: a second finger cancels it, unless it is a scroll, which the second finger then takes.
  - *The lifecycle's first two parts* (D15): the manifest declares the configuration changes the host takes in
    place, an activity made again adopts the host that runs (`Native.start` on a thread that has one), and the host
    does no frame work while the activity is stopped. Android discards a stopped window's display lists, the reader's
    kept rows among them, so a start after a stop makes the reader again (`ExactActivity.read`) and the host paints
    everything for it; hosted views are made again with it. Nothing is saved for a process the system kills (D15.3).
- **Only the async lane's second tier builds it** (LLP 1133.002 stage 4): Weatherlight's APK, hourly, which
  compiles `exact-linux` for the Android target and the reader's Kotlin with a module's. Gradle is in none of the
  six checks.
- **The shell is a development build**: signed with the debug key. Its one activity turns keys on for
  every app, and gives none an app's own native module (LLP 1024) or a video player, which the benchmark's apps
  passed from an activity of their own. A package's implementation of a module is the exception (below, "Module
  implementations").
- **A GPU canvas over children**: a `Composite` surface (Caltrain's sky, `glass`, which refracts the content inside
  it) is given its children's picture (`gpu_texture`) and the page no longer paints them over it; built and unit
  tested on a Mac, not seen on a device.
- **No update store** and no replaced Rust module: the Canvas host has neither, though an app's compatibility
  record is computed as its Linux crate's is, and Caltrain's names both (`store.L` is `A`, `rustMode` is `native`).
- **No picture from a URL.** The Linux host's pictures are files under the asset root (`host/linux/src/image`), on
  every shell it has: Shelf's covers, which Open Library serves, are the grey box behind them in Discover and a gap on
  a book's page. Nothing of this host's is missing for it; the loader has no door for a remote source.
- **The benchmark still builds its own copy** of the reader, which no longer matches this repository's JNI names.

## The Material reference

`reference/` is Compose Material 3 at the pinned version (`gradle/libs.versions.toml`), drawn on Robolectric's native
graphics, and the oracle that compares it with Exact's drawing of the same specimens (LLP 1115.001.001). It is in no
app. On a machine with the SDK, Java 21 (`JAVA_HOME`) and Cargo:

    bun host/android/reference/oracle.mjs            # draw both sides, compare, report
    bun host/android/reference/oracle.mjs --compare  # compare what is already drawn
    bun host/android/reference/tokens.mjs            # hold the schema's Material column to the pinned source
    bun host/android/reference/tokens.mjs --diff 1.3.2   # what a version changed, for a bump

The report is `reference/build/oracle/report.html`. `specimens.json` lists the specimens, their probes and the planted
defects; each specimen's Exact half is `apps/material-specimens/specimens/<id>.contract`.

On a phone (D4's calibration, LLP 1115.001.001 §3), after the build box's run:

    bun host/android/reference/oracle.mjs --phone <adb serial>

builds and installs the catalogue (`:reference`'s debug APK, which draws the specimen `--es id <id> --es scheme
light|dark|dynamic-light|dynamic-dark` names over the whole screen and logs `drawn` once settled) and the specimen app
(`agent-android.mjs build material-specimens --apk`), draws each specimen on the phone both ways (the reader driven by
`agent.mjs android --reader --plan`, which pushes the plan), and writes `phone/floors.json`: each layer's largest
difference between two renders of one side, and between the two sides on the phone. It moves the phone's night mode
and puts it back.
