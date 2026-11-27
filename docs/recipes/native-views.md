# Recipe: native views

Use `exact-camera` from `@exact/camera`. See [calibration](camera-calibration.md).

Declare a hyphenated tag in `app.json` under `modules`. Implement it with public APIs in `modules/apple/*.swift`.

```text
component Clip
  state last = ""
  action seeked(at: string)
    last = at
  view
    column
      video-view source="intro.mp4" message=seeked aria-label="Intro" height=180
      text last testId="last"
```

```swift
import UIKit // AppKit under #if os(macOS)

final class Clip: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        ["video-view": ExactNativeFactory(for: Clip.self) { clip, props, events in Video(clip, props, events) }]
    }
}
let exactModule: ExactModule.Type = Clip.self

final class Video: ExactNativeInstance {
    private let strip = VideoView() // an AVPlayerLayer and a tap recognizer
    private let pad: Clip
    private var ticket: ExactTicket?
    init(_ pad: Clip, _ props: [String: String], _ events: ExactNativeEvents) {
        self.pad = pad
        super.init(events: events)
        try? setProps(props)
        strip.tapped = { [weak self] at in events.message(String(at)); self?.start() }
        events.intrinsicSize(CGSize(width: 320, height: 180))
    }
    // A live ticket costs CPU.
    func start() {
        guard ticket == nil else { return }
        ticket = pad.context.frames { [weak self] f in
            guard let self, !self.strip.advance(to: f.now) else { return }
            self.ticket?.stop(); self.ticket = nil
        }
    }
    override var view: ExactNativeView { strip }
    override func setProps(_ props: [String: String]) throws { strip.src = props["source"] ?? "" }
    override func destroy() { ticket?.stop() }
}
```

- **Props are strings**, sent whole on change. Contract names (`value`, `src`) and events are reserved.
- **Web:** `modules/web/index.js` exports `abi = 1`, `roster`, `create(tag, el, propsJson, event, reply)`,
  `setProps(handle, propsJson)`, and `destroy(handle)`.
- **Events:** `events.message(text)` runs `message=`; `press()`, `change(v)`, `load()` likewise.
- **Time:** use `context.frames` / `context.after(ms)`, never `CADisplayLink` or timers, so `clock +N` advances it.
- **Touches:** add recognizers to the view; points `point(inside:)` declines fall through unless `press=` is set. Drive with `agent ios --touch platform`.
- **Delegates:** `ExactNativeInstance` is not an `NSObject`; hold an `NSObject` delegate.
- **Size:** CSS reads `events.intrinsicSize` on auto axes; Android calls [wrapContent()](../../host/android/README.md#compose-module-views).
- **Accessibility:** [native views](../contract-for-agents.md#native-views-accessibility).
- **Data:** `app.ts` calls `native!.watch('state'); return native!.call({op: 'state'});`; the `else` row shows until launch.
- **Crashes:** `agent ios logs`.
- **First frame:** views attach after first paint. Only cheap views set `ExactNativeFactory(firstFrame: true)` (runs on main at launch; defer `AVAudioEngine`).
