// One trial of this fixture (app.contract) with real input and VoiceOver on macOS, and what
// VoiceOver said. Build it with `swiftc -O vo-trial.swift -o vo-trial`; run it in a logged-in
// session whose SSH or terminal process holds the Accessibility, Screen Recording and
// Automation (VoiceOver) grants, with VoiceOver running, its AppleScript allowed (VoiceOver
// Utility › General) and its caption panel shown, and the app open and frontmost.
//
//   vo-trial <app name> <item title> [seconds]   e.g. vo-trial "Menu Pick" "Open Box, Field Focused"
//   vo-trial read <film directory>                the caption panel's text, from a trial's film
//
// A trial pushes Compose if it is not shown, clicks the draft (the text area "Compose message"),
// stops speech (Control), right-clicks the row "draft", waits for VoiceOver to read the menu,
// and clicks the item with the pointer: HID events, as a mouse's. It records, on one clock
// (ms from the start of recording, just before the right click; `clock:` gives its epoch
// milliseconds mod 1e6, the reference's log's clock):
//   ax:      each accessibility notification the app posts (an AXObserver on its own thread)
//   said:    VoiceOver's last phrase, polled, each time it changes
//   cursor:  the text under VoiceOver's cursor, each time it changes
//   caption: VoiceOver's caption panel, filmed by `screencapture -R` (about 11 frames a second)
//            and read by Vision's text recognition, each time its text changes
// Each channel misses phrases the other has: an AppleScript read blocks while VoiceOver is busy
// (up to half a second), so a phrase its hint replaces in that time is lost, and the film misses
// a short caption. Count a trial by both. `before:` says whether the draft had the focus as the
// trial began; a trial where it had not, or after which VoiceOver said nothing, does not count. The film is kept, and its directory printed: where text recognition is unavailable (a
// virtual machine), `vo-trial read` it on another Mac.
//
// How often a pick re-reads changes from one VoiceOver session to the next on the same build,
// so compare two builds, or Exact and the reference, only interleaved in one session, each trial
// a fresh launch. Alternating two apps, open each as an app bundle and quit it by Apple event:
// a bare executable started over SSH and killed between trials left VoiceOver silent for the
// next app.
//
// A hand-built AppKit window re-reads the draft after these picks as often as Exact does
// (issues/closed/20261018-macos-menu-pick-*.md, measured with `appkit-reference.swift`).
import AppKit
import ApplicationServices
import ScreenCaptureKit
import Vision

setvbuf(stdout, nil, _IOLBF, 0)
let args = CommandLine.arguments

/// The film's captions, each time the text changes, and how many frames could not be read.
func readFilm(_ dir: String) -> (captions: [(ms: Int, text: String)], unread: Int) {
    let frames = ((try? FileManager.default.contentsOfDirectory(atPath: dir)) ?? [])
        .compactMap { Int($0.replacingOccurrences(of: ".png", with: "")) }.sorted()
    var read: [(Int, String)] = [], last = "", lastBytes = Data(), unread = 0
    for at in frames {
        guard let bytes = FileManager.default.contents(atPath: "\(dir)/\(at).png") else { unread += 1; continue }
        guard bytes != lastBytes else { continue }
        lastBytes = bytes
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = false
        guard let image = NSImage(data: bytes)?.cgImage(forProposedRect: nil, context: nil, hints: nil),
              (try? VNImageRequestHandler(cgImage: image).perform([request])) != nil else { unread += 1; continue }
        let text = (request.results ?? []).sorted { $0.boundingBox.minY > $1.boundingBox.minY }
            .compactMap { $0.topCandidates(1).first?.string }.joined(separator: " ")
        if !text.isEmpty, text != last { read.append((at, text)); last = text }
    }
    return (read, unread + (frames.isEmpty ? 1 : 0))
}
if args.count == 3, args[1] == "read" {
    let (captions, unread) = readFilm(args[2])
    for (at, text) in captions { print(String(format: "%6d", at), "caption:", text) }
    if unread > 0 { print("error: \(unread) frames unread"); exit(1) }
    exit(0)
}
guard args.count > 2 else { print("usage: vo-trial <app name> <item title> [seconds] | vo-trial read <film directory>"); exit(2) }
let appName = args[1], pickTitle = args[2]
let seconds = args.count > 3 ? Double(args[3]) ?? 4 : 4
guard let running = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == appName }) else { print("error: \(appName) is not running"); exit(1) }
let pid = running.processIdentifier
let app = AXUIElementCreateApplication(pid)
AXUIElementSetMessagingTimeout(app, 2)
nonisolated(unsafe) var t0 = Date()
func now() -> Int { Int(Date().timeIntervalSince(t0) * 1000) }
func stamp(_ ms: Int) -> String { String(format: "%6d", ms) }
let lock = NSLock()
func out(_ s: String) { lock.lock(); print(s); lock.unlock() }

// MARK: Accessibility
func value(_ e: AXUIElement, _ name: String) -> CFTypeRef? {
    var v: CFTypeRef?
    return AXUIElementCopyAttributeValue(e, name as CFString, &v) == .success ? v : nil
}
func string(_ e: AXUIElement, _ name: String) -> String? { value(e, name) as? String }
func describe(_ e: AXUIElement) -> String {
    var s = string(e, kAXRoleAttribute) ?? "?"
    if let sub = string(e, kAXSubroleAttribute) { s += "/" + sub }
    for n in [kAXTitleAttribute, kAXDescriptionAttribute, kAXIdentifierAttribute] {
        if let v = string(e, n), !v.isEmpty { s += " \(n.dropFirst(2).lowercased())=\"\(v.prefix(32))\"" }
    }
    return s
}
func find(_ root: AXUIElement, depth: Int = 40, _ match: (AXUIElement) -> Bool) -> AXUIElement? {
    var queue = [(root, 0)]
    while !queue.isEmpty {
        let (e, d) = queue.removeFirst()
        if match(e) { return e }
        if d < depth { queue += ((value(e, kAXChildrenAttribute) as? [AXUIElement]) ?? []).map { ($0, d + 1) } }
    }
    return nil
}
func center(_ e: AXUIElement) -> CGPoint? {
    guard let p = value(e, kAXPositionAttribute), let s = value(e, kAXSizeAttribute) else { return nil }
    var point = CGPoint.zero, size = CGSize.zero
    AXValueGetValue(p as! AXValue, .cgPoint, &point); AXValueGetValue(s as! AXValue, .cgSize, &size)
    return CGPoint(x: point.x + size.width / 2, y: point.y + min(size.height / 2, 20))
}
func named(_ e: AXUIElement, _ names: [String]) -> Bool {
    [kAXIdentifierAttribute, kAXDescriptionAttribute, kAXTitleAttribute].contains { n in string(e, n).map(names.contains) ?? false }
}

// MARK: Input, posted at the HID tap as a mouse's and a keyboard's are
let source = CGEventSource(stateID: .hidSystemState)
func nap(_ ms: UInt32) { usleep(ms * 1000) }
func mouse(_ type: CGEventType, _ p: CGPoint, _ button: CGMouseButton = .left) {
    CGEvent(mouseEventSource: source, mouseType: type, mouseCursorPosition: p, mouseButton: button)!.post(tap: .cghidEventTap)
}
func click(_ p: CGPoint) { mouse(.mouseMoved, p); nap(150); mouse(.leftMouseDown, p); nap(70); mouse(.leftMouseUp, p) }
func key(_ code: CGKeyCode) {
    for down in [true, false] { CGEvent(keyboardEventSource: source, virtualKey: code, keyDown: down)!.post(tap: .cghidEventTap); nap(40) }
}

// MARK: VoiceOver
let phrase = NSAppleScript(source: "tell application \"VoiceOver\" to get content of last phrase")!
let cursor = NSAppleScript(source: "tell application \"VoiceOver\" to get text under cursor of vo cursor")!
func run(_ s: NSAppleScript) -> String? {
    var error: NSDictionary?
    return s.executeAndReturnError(&error).stringValue?.replacingOccurrences(of: "\n", with: " / ")
}
func captionPanel() -> CGRect? {
    var frame: CGRect?
    let done = DispatchSemaphore(value: 0)
    SCShareableContent.getWithCompletionHandler { content, _ in
        frame = content?.windows.filter { $0.owningApplication?.bundleIdentifier == "com.apple.VoiceOver" && $0.frame.width > 200 && $0.frame.height < 300 && $0.isOnScreen }
            .max { $0.frame.width < $1.frame.width }?.frame
        done.signal()
    }
    done.wait()
    return frame
}
let film = NSTemporaryDirectory() + "vo-trial-film-\(pid)-\(Int(Date().timeIntervalSince1970))"
let filming = NSCondition()
nonisolated(unsafe) var stopFilming = false, filmStopped = false
// `screencapture`: a ScreenCaptureKit capture of the panel, of its window or of the display
// cropped to it, comes out blank.
func shoot(_ rect: CGRect) {
    try? FileManager.default.createDirectory(atPath: film, withIntermediateDirectories: true)
    Thread.detachNewThread {
        while true {
            filming.lock(); let stop = stopFilming; filming.unlock()
            if stop { break }
            let shot = Process()
            shot.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
            shot.arguments = ["-x", "-R\(Int(rect.minX)),\(Int(rect.minY)),\(Int(rect.width)),\(Int(rect.height))", "\(film)/\(now()).png"]
            try? shot.run(); shot.waitUntilExit()
        }
        filming.lock(); filmStopped = true; filming.signal(); filming.unlock()
    }
}
/// Ends the film once its last frame is written.
func stopFilm() {
    filming.lock()
    stopFilming = true
    while !filmStopped { filming.wait() }
    filming.unlock()
}

// MARK: The trial
guard let window = find(app, depth: 1, { string($0, kAXRoleAttribute) == kAXWindowRole }) else { print("error: no window"); exit(1) }
func draft() -> AXUIElement? { find(window) { named($0, ["draft", "Compose message"]) && string($0, kAXRoleAttribute) == kAXTextAreaRole } }
if draft() == nil, let compose = find(window, { named($0, ["compose", "Compose"]) && string($0, kAXRoleAttribute) == kAXButtonRole }), let at = center(compose) {
    click(at); nap(1500)
}
guard let field = draft(), let fieldAt = center(field) else { print("error: no draft"); exit(1) }
click(fieldAt); nap(900)
key(59); nap(700) // Control: VoiceOver stops speaking
let focused = value(app, kAXFocusedUIElementAttribute).map { describe($0 as! AXUIElement) } ?? "none"
out("before: frontmost=\(running.isActive) focused=[\(focused)] cursor=\"\(run(cursor) ?? "")\"")
guard let row = find(window, { named($0, ["row"]) || (string($0, kAXRoleAttribute) == kAXStaticTextRole && string($0, kAXValueAttribute) == "draft") }),
      let rowAt = center(row) else { print("error: no row"); exit(1) }

let notifications = [kAXFocusedUIElementChangedNotification, kAXFocusedWindowChangedNotification, kAXMainWindowChangedNotification,
                     kAXWindowCreatedNotification, kAXSheetCreatedNotification, kAXMenuOpenedNotification, kAXMenuClosedNotification,
                     kAXLayoutChangedNotification, kAXAnnouncementRequestedNotification, kAXUIElementDestroyedNotification,
                     kAXCreatedNotification, kAXTitleChangedNotification]
t0 = Date()
out("clock: \(Int(t0.timeIntervalSince1970 * 1000) % 1_000_000)")
let observing = DispatchSemaphore(value: 0)
Thread.detachNewThread {
    var observer: AXObserver?
    let made = AXObserverCreateWithInfoCallback(pid, { _, element, note, info, _ in
        let at = now() // before `describe`, whose reads of the app can wait on it
        var extra = ""
        if let elements = (info as? [String: Any])?[kAXUIElementsKey] as? [AXUIElement] {
            extra = " naming [\(elements.map(describe).joined(separator: ", "))]"
        }
        out("\(stamp(at)) ax: \(note as String) [\(describe(element))]\(extra)")
    }, &observer)
    guard made == .success, let observer else { out("error: no AXObserver (\(made.rawValue))"); exit(1) }
    let refused = notifications.filter { AXObserverAddNotification(observer, app, $0 as CFString, nil) != .success }
    if !refused.isEmpty { out("error: not observed: \(refused.joined(separator: ", "))") }
    CFRunLoopAddSource(CFRunLoopGetCurrent(), AXObserverGetRunLoopSource(observer), .defaultMode)
    observing.signal()
    CFRunLoopRun()
}
observing.wait()
var lastSaid = run(phrase), lastCursor = run(cursor)
func poll() {
    if let p = run(phrase), p != lastSaid { out("\(stamp(now())) said: \(p)"); lastSaid = p }
    if let c = run(cursor), c != lastCursor { out("\(stamp(now())) cursor: \(c)"); lastCursor = c }
}
func listen(_ seconds: Double) { let end = Date().addingTimeInterval(seconds); while Date() < end { poll(); nap(20) } }

let panel = captionPanel()
if let panel { shoot(panel) } else { out("error: no caption panel") }
out("\(stamp(now())) right-click the row")
mouse(.mouseMoved, rowAt); nap(150); mouse(.rightMouseDown, rowAt, .right); nap(70); mouse(.rightMouseUp, rowAt, .right)
var item: AXUIElement?
let deadline = Date().addingTimeInterval(3)
while item == nil, Date() < deadline {
    poll()
    item = find(app, depth: 6) { string($0, kAXRoleAttribute) == kAXMenuItemRole && string($0, kAXTitleAttribute) == pickTitle }
}
guard let item, let itemAt = center(item) else { out("error: no menu item \(pickTitle)"); key(53); exit(1) }
listen(0.9) // VoiceOver reads the menu, as a person waits to hear it
mouse(.mouseMoved, itemAt)
listen(0.25)
mouse(.leftMouseDown, itemAt); nap(70); mouse(.leftMouseUp, itemAt)
let clicked = now()
out("\(stamp(clicked)) clicked \"\(pickTitle)\"")
listen(seconds)
if panel != nil {
    stopFilm()
    let (captions, unread) = readFilm(film)
    for (at, text) in captions { out("\(stamp(at)) caption: \(text)") }
    out("film: \(film)\(unread > 0 ? " (\(unread) frames unread: vo-trial read it on another Mac)" : "")")
}
out("end: cursor=\"\(run(cursor) ?? "")\"")
