# Heavy list on Android

The heavy list (`../SPEC.md`) on an Android phone, three ways from the same data:

| App | Package | Source | Build |
|---|---|---|---|
| Exact | `dev.exact.heavybench.exact` | `../exact-heavylist/` (its `android/` crate) on the Canvas reader, `host/android` | `EXACT_APP_DIR=$PWD/bench/heavy-list/exact-heavylist bun scripts/agent-android.mjs build exact-heavylist --apk --release` |
| Views | `dev.exact.heavybench.views` | `views/`: a RecyclerView | `host/android/gradlew -p bench/heavy-list/android :views:assembleRelease` |
| Compose | `dev.exact.heavybench.compose` | `compose/`: a LazyColumn | `host/android/gradlew -p bench/heavy-list/android :compose:assembleRelease` |

`../prepare.sh` copies the data into `common/src/main/assets/` (not committed). `common/` is what the two native
apps share: the JSON reader, live mode's step and the image loader. Both native apps are R8-minified release builds
signed with the debug key. The launch extra `live 1` turns live mode on in all three.

Views is written as an Android engineer writes a performance-sensitive feed (the bar `uikit/` follows on iOS);
Compose is the ordinary way (SwiftUI's bar).

## How each native app expresses the spec

| Spec item | Views | Compose |
|---|---|---|
| list | `RecyclerView` + `LinearLayoutManager`, no item animator; RecyclerView's own prefetch | `LazyColumn` with `items(key = id)` over a `SnapshotStateList` |
| row | one `Row` (a `LinearLayout`) whose views are made once and reused; paragraph views and chips grow on demand and hide when unused | composables per item |
| paragraphs | one `TextView` per paragraph, a `SpannableStringBuilder` of the runs, built at bind | one `BasicText` per paragraph, an `AnnotatedString` remembered per paragraph |
| photos | a `ViewGroup` placing 1–4 `ImageView`s, rounded by its outline | `Row`/`Column` of fixed-size boxes, clipped |
| images | `common/Images.kt` for both: `ImageDecoder` at the box's aspect-fill pixel size on two worker threads, hardware bitmaps, an `LruCache` of 300; #E5E5EA until decoded | the same loader through a `LaunchedEffect` |
| reactions | a wrapping `ViewGroup` of chips; a tap replaces the message and `notifyItemChanged` | `FlowRow`, with taps finding the current list item by message ID |
| separator | an `ItemDecoration`, 68 → width − 16 | `drawBehind` on the row |
| live insert | `notifyItemInserted(0)`; the layout manager's anchor keeps the viewport | the list keeps its first visible key |
| live times | the visible rows' time labels set once a second | `seconds` state read by every row |
| scrollbar | `android:scrollbars="vertical"` (`res/layout/feed.xml`): the theme's thumb and fade | `Modifier.scrollbar`, as LazyColumn draws none: the same thumb, delay and fade |

All three show a scrollbar while the list scrolls, as the iOS lists do. Exact's is the platform's, so the native
apps show one too; its fade is about 20 frames after every fling.

Fonts are the platform's (Roboto on most phones) at the spec's sizes in sp; the device's font scale is recorded.

## Measuring

```sh
export ANDROID_SERIAL=<adb serial>
adb install -r <each apk>
adb shell cmd package compile -f -m speed <each package>   # AOT, so no app is measured while still in the JIT
bun bench/heavy-list/android/measure.mjs target/bench/heavy-list/android/<series> 3
bun bench/heavy-list/android/summarize.mjs target/bench/heavy-list/android/<series>
```

Every app is driven and read from outside, the same way:

- **Touches**: `adb shell input swipe` up the middle of the screen at constant speed, then a lift, so each app
  flings with its own physics (OverScroller in Views and in Exact's reader, Compose's spline decay). `fling` is six
  swipes at each of 3k, 6k, 12k and 24k px/s, 1.5 s apart. This is not the iOS probe's constant-speed scroll:
  distances follow each app's fling, and the platform caps the velocity.
- **Frames**: `dumpsys gfxinfo`, reset before each stretch: frames HWUI rendered, janky frames, frame-time
  percentiles, missed deadlines, GPU percentiles. It sees the frames the app's renderer drew, not whether their
  content was complete; `blank` covers that.
- **CPU**: `/proc/<pid>/stat` and the main thread's `/proc/<pid>/task/<pid>/stat`, utime + stime per second.
- **Memory**: `dumpsys meminfo` TOTAL PSS at the end of a stretch.
- **Cold start**: `am start -W` TotalTime after `am force-stop`, five launches: process start to the activity's
  first frame. For the native apps that frame has rows; for Exact it is the first frame the reader draws.
- **Blank**: eight 24k px/s flings filmed with `screenrecord`. A frame counts when the measured list viewport,
  excluding the scrollbar gutter, has a run of white at least a quarter of the screen tall. Each round keeps its video if it found a blank.

Rounds rotate the apps' order. The script records the device, its resolution, refresh rate and font scale.

## Last standings

Galaxy A57 5G (SM-A576B), Android 17, 1080×2340 at 120 Hz, font scale 1.0; exact2 at `13c75e3b13` with this
bench's data source; all three AOT-compiled; medians of 3 rounds, blank frames summed over them (2026-11-23).

| | Exact | Views | Compose |
|---|---:|---:|---:|
| cold ms (`am start -W`) | 204 | 240 | 258 |
| fling frames drawn | 5094 | 5078 | 4341 |
| fling janky % | 0.20 | 0.65 | 0.62 |
| fling frame deadline missed | 10 | 33 | 27 |
| fling frame p50 / p90 / p99 ms | 5 / 5 / 7 | 5 / 6 / 10 | 5 / 6 / 10 |
| fling CPU ms/s, process / main thread | 662 / 148 | 507 / 146 | 513 / 193 |
| fling PSS MB | 227 | 262 | 273 |
| live janky %, deadline missed | 0.27, 14 | 0.57, 29 | 1.0, 46 |
| live CPU ms/s, process / main thread | 700 / 148 | 511 / 149 | 538 / 212 |
| rest CPU ms/s | 10 | 0 | 1 |
| rest PSS MB | 111 | 88 | 94 |
| blank frames / filmed (worst band) | 2 / 4342 (0.35) | 0 / 4334 (0.04) | 0 / 4329 (0.05) |

- Exact misses the fewest frame deadlines, scrolling and in live mode, at about a third more process CPU: its
  rows are recorded on threads beside the main one. Deadlines count the frames HWUI drew, not their content: a
  row Exact makes late is an on-time frame with a gap (the blank row), where a native app's is a late frame.
- Exact's main thread replays what its `exact` thread recorded; the native apps' main thread binds, lays out and
  records. The main-thread column compares those, not equal work.
- Exact's blank frames are a row coming into view a frame late where a fling slows. On a moto g57 power the same
  build had 123 in 2619 filmed, the worst 0.84 of the screen; the native apps had none.
- Cold start is not like for like either: the native apps' first frame has rows, Exact's is the reader's first
  frame, before them.
- Compose draws a seventh fewer frames for the same swipes: its flings stop sooner, so it also scrolls less.
  Per-second CPU is therefore not per-distance CPU.
- At rest Exact keeps 10 ms/s of CPU and about 20 MB more memory than the native apps. The bench app's two
  timers (every 250 ms and every second) run with live mode off as well, where the native apps run none.
