# Chat replay benchmark (iOS)

A WhatsApp-style conversation of 10,000 messages (text, photos, voice notes), scrolled from the newest message up
to message 1 with the same touches on any app that draws it. The apps measured are the pair at
https://github.com/magrinj/rn-vs-exact-chat: `exact-app/` (Exact) and `rn/` (React Native with Expo). The touches are
played through XCTest's event synthesizer, so they reach each app through UIKit's gesture recognizers and scroll
physics, and a probe inserted into the app records what the screen shows on each frame. It needs an Apple silicon Mac
and an iOS 17 or later simulator.

That repository's README builds for a phone. For a simulator, in Release:

```sh
# in a clone of that repository
(cd exact-app && bun exact.mjs ios --release --sim <udid>)          # builds and installs
(cd rn && npm install && node scripts/build-rows.mjs && npx expo prebuild -p ios &&
  xcodebuild -workspace ios/Chat.xcworkspace -scheme Chat -configuration Release -sdk iphonesimulator \
    -destination 'generic/platform=iOS Simulator' -derivedDataPath ios/build CODE_SIGNING_ALLOWED=NO build &&
  xcrun simctl install <udid> ios/build/Build/Products/Release-iphonesimulator/Chat.app)
```

Then per run, from this checkout:

```sh
export REPLAY_TOP_TEXT="J'ai hâte|Vas-y fonce|Il fait super beau aujourd"   # message 1 and its neighbours
bun bench/chat-replay/run.mjs --sim <udid> --app <bundle id> --out r/exact-s1.json              # sampled
bun bench/chat-replay/run.mjs --sim <udid> --app <bundle id> --out r/exact-p1.json --sample 0   # pacing
python3 bench/chat-replay/summarize.py exact=r/exact-s1.json,r/exact-p1.json rn=r/rn-s1.json,r/rn-p1.json
```

`REPLAY_TOP_TEXT` names texts that show together only at message 1, so that a list that pages older rows in is not
taken to be at message 1 at each page's top; a run needs it, or `REPLAY_UNVERIFIED=1` to take the first top. Run the
apps of a comparison in alternation, several rounds, on a host doing nothing else: frame pacing and the time to
message 1 move with the host's load and across simulator restarts.

`run.mjs` builds the probe (`probe/chatprobe.m`) and the XCTest runner (`replay/replay.swift`) into
`target/chat-replay/` on first use, launches the app with the probe inserted, waits 5 s (`--settle`) and plays the
replay. The runner's first accessibility query, which makes the app load the accessibility bundles (a stall of most of
a second), comes 2 s before the first flick: after the frames the launch figures use and before the scroll. A run
counts only when the test passed and the probe saw every phase; otherwise it prints `no result` and keeps the file as
`<out>.failed.json`. `--dump <dir>` also writes into that directory the sampled image of up to 40 frames with
something missing, named by process, phase, time and counts, and of the first with a drawn bubble and nothing missing,
to check the classification by eye.

## The replay

1. **Launch**: from process start until the first touch.
2. **Up to message 1**: a flick toward older messages every 0.125 s, eight a second as the original benchmark's
   robot (400 pt in 50 ms, lifted at once; UIKit compounds the velocity of flicks that follow each other), played in
   records of eight on one cadence, until message 1 is on screen: the list has rested at its visual top for 500 ms
   with its content size unchanged and the texts of `REPLAY_TOP_TEXT` are in the app's accessibility tree. The time
   to message 1 runs to the start of that rest, where this phase ends; the rest until the next touch counts for no
   metric.
3. **Flings**: 1 s after message 1 is confirmed, back toward newer messages, single flings at 1,500, 3,000, 6,000 and
   12,000 pt/s, twice, 2 s apart start to start, then 2 s still.

`REPLAY_PERIOD` and `REPLAY_FLINGS` change steps 2 and 3.

## Metrics

On sampled frames (every 2nd by default) the probe renders the window's layer tree, over the transcript's visible
rect, at 0.5 pixel per point, and sorts pixels into wallpaper, my-bubble colour, their-bubble colour and everything
else (ink). A *bubble* is a connected region of one bubble colour at least 20 × 16 pt with wallpaper beside it. The
colours are WhatsApp's light theme, as the apps above draw them: bubbles `#d9fdd3` (mine) and `#ffffff` (theirs, and
day pills) on a flat `#efeae2` wallpaper or a two-tone doodle between `#efe7dd` and `#e2d7c9`. Another app must use
them for the classification to hold.

| Metric | Meaning |
|---|---|
| first content ms | process start to the first frame with a bubble that has ink |
| ready ms | process start to the first frame with three drawn bubbles or more and nothing missing |
| to message 1 s | first touch to message 1 on screen (from runs without sampling) |
| empty bubble % | sampled frames with a bubble under 0.6 % ink: its text, time or waveform not drawn (a bubble cut by the list's edge is judged only when 60 pt of it shows) |
| unpainted photo % | sampled frames with a photo-sized bubble (≥ 200 × 140 pt) whose middle is bubble colour for ≥ 60 pt: the photo not drawn |
| blank band % | sampled frames with ≥ 60 pt of rows holding no bubble (only wallpaper, or one flat colour): rows not drawn at all |
| mostly blank % | sampled frames with half the visible area or more missing |
| incomplete % | sampled frames with any of the three |
| missing area % | mean over sampled frames of the missing area (empty and unpainted bubbles, blank bands) over the visible area |
| incomplete % at a–b pt/s | incomplete % of the sampled frames whose list moved at that speed (over the 3 frames before, leaving out windows in which the content grew or shrank by over 1,000 pt, as when a page lands, and those faster than 150,000 pt/s): apps that scroll at different speeds compare here |
| late/s, late % | frames whose interval exceeded 1.5 × the display link's expected interval (from runs without sampling) |
| worst ms | the longest frame interval |
| main cpu % | main-thread CPU over the phase's wall time |

Rendering and classifying a frame costs the main thread about 5 ms on an M-series simulator, so pacing and
time to message 1 come from runs with `--sample 0`. Launch frames are classified in every run, each frame until
a second after ready, so launch figures carry that cost, and the probe's insertion by `DYLD_INSERT_LIBRARIES`, which
disables the app's prebuilt launch closure. A sample is the layer tree's model at the display-link callback,
rendered with `renderInContext:`, which leaves out animations running in the render server; it is dated at the
frame's intended presentation time (the display link's target time), not a measured one.

## Limits

The simulator runs the display at 60 Hz and its GPU and decode paths are the Mac's; its numbers rank causes and
check fixes, and a device run decides. XCTest turns the simulator's accessibility on while it plays touches, for
both apps alike.
