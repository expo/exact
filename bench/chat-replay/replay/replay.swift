// The finger replay: one XCTest UI test that plays the same touches on REPLAY_APP through XCTest's event synthesizer
// (XCSynthesizedEventRecord and XCPointerEventPath, as host/apple/touches.swift uses them), so the touches reach the
// app through the HID system and UIKit's own gesture recognizers and scroll physics. It never launches the app:
// run.mjs launches it with the probe, then starts this test, which brings it to the front.
//
//   up      flicks toward older messages (finger moving down the screen), one every REPLAY_PERIOD seconds (0.125,
//           eight a second, as the original benchmark's robot), played in records of eight on one cadence, until
//           message 1 is on screen or 150 s. When the probe posts
//           dev.exact.chatreplay.edge (the list resting at its visual top, again each second it stays there), the
//           runner looks for every text of REPLAY_TOP_TEXT ("|"-separated: message 1 and its neighbours) in the
//           app's accessibility tree and, if all are there, posts dev.exact.chatreplay.top and waits for the
//           probe's dev.exact.chatreplay.topok (or notop, when the list has left that edge, and it flicks on);
//           with REPLAY_UNVERIFIED=1 instead, the first edge is the top.
//   flings  one second later, flings back toward newer messages (finger moving up) at the speeds of REPLAY_FLINGS,
//           2 s apart, start to start, then 2 s still
// and then it posts dev.exact.chatreplay.done, on which the probe writes its result.
import XCTest
import UIKit
import ObjectiveC

// XCTest waits for the app to go idle before it answers a query; a chat whose list is moving, or that receives a
// message every second, is never idle, so the wait would hold the accessibility check (and the replay) for seconds.
private let noQuiescence: Void = {
    guard let cls = NSClassFromString("XCUIApplicationProcess") else { return }
    for name in ["waitForQuiescenceIncludingAnimationsIdle:", "waitForQuiescenceIncludingAnimationsIdle:isPreEvent:"] {
        guard let m = class_getInstanceMethod(cls, NSSelectorFromString(name)) else { continue }
        let block: @convention(block) (AnyObject, Bool, Bool) -> Void = { _, _, _ in }
        method_setImplementation(m, imp_implementationWithBlock(block))
    }
}()

final class ChatReplay: XCTestCase {
    func testReplay() throws {
        let env = ProcessInfo.processInfo.environment
        let period = Double(env["REPLAY_PERIOD"] ?? "0.125") ?? .nan
        let flingText = (env["REPLAY_FLINGS"] ?? "1500,3000,6000,12000,1500,3000,6000,12000").split(separator: ",")
        let flings = flingText.compactMap { Double($0) }
        // A flick takes 50 ms; a fling at under 150 pt/s would take over the 2 s between flings.
        guard period.isFinite, period >= 0.06, !flings.isEmpty, flings.count == flingText.count, flings.allSatisfy({ $0.isFinite && $0 >= 150 }) else {
            XCTFail("REPLAY_PERIOD must be at least 0.06 s and REPLAY_FLINGS a list of speeds of at least 150 pt/s"); return
        }
        let texts = (env["REPLAY_TOP_TEXT"] ?? "").split(separator: "|").map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty }
        guard !texts.isEmpty || env["REPLAY_UNVERIFIED"] == "1" else {
            XCTFail("REPLAY_TOP_TEXT names the texts of message 1 (REPLAY_UNVERIFIED=1 takes the first edge instead)"); return
        }
        let size = XCUIScreen.main.screenshot().image.size // points, portrait
        let x = size.width / 2
        let app = env["REPLAY_APP"].map { XCUIApplication(bundleIdentifier: $0) }
        _ = noQuiescence
        app?.activate() // the runner launches in front; the touches are the app's
        // XCTest turns the app's accessibility on at its first query, and the app then loads the accessibility
        // bundles, stalling for most of a second: done here, and let settle, so the stall is not in the scroll.
        _ = app?.descendants(matching: .any).count
        spin(2)
        var top = false
        let center = CFNotificationCenterGetDarwinNotifyCenter()
        let observer = Unmanaged.passUnretained(self).toOpaque()
        // Darwin notifications arrive on another thread; the flags are the main thread's.
        CFNotificationCenterAddObserver(center, observer, { _, _, _, _, _ in DispatchQueue.main.async { ChatReplay.atEdge = true } },
                                        "dev.exact.chatreplay.edge" as CFString, nil, .deliverImmediately)
        CFNotificationCenterAddObserver(center, observer, { _, _, _, _, _ in DispatchQueue.main.async { ChatReplay.answer = false } },
                                        "dev.exact.chatreplay.notop" as CFString, nil, .deliverImmediately)
        CFNotificationCenterAddObserver(center, observer, { _, _, _, _, _ in DispatchQueue.main.async { ChatReplay.answer = true } },
                                        "dev.exact.chatreplay.topok" as CFString, nil, .deliverImmediately)
        defer { CFNotificationCenterRemoveEveryObserver(center, observer) }
        ChatReplay.atEdge = false
        let messageOne = { () -> Bool in
            guard let app, !texts.isEmpty else { return true }
            return texts.allSatisfy { app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", $0)).firstMatch.exists }
        }
        let deadline = Date().addingTimeInterval(150)
        // One flick: down at 30 % of the height, 400 pt toward the bottom in 50 ms, lifted at once (8,000 pt/s).
        let flickUp = { (at: Double) -> Stroke in Stroke(from: CGPoint(x: x, y: size.height * 0.3), to: CGPoint(x: x, y: size.height * 0.3 + 400), start: at, travel: 0.05) }
        var checks: [String] = []
        // Records follow one cadence: each starts one period after the last flick of the one before, unless an
        // edge check (between records, so that no flick follows a confirmed top) ran past that time.
        var next = Date()
        while !top && Date() < deadline {
            if next > Date() { spin(next.timeIntervalSinceNow) }
            if ChatReplay.atEdge {
                ChatReplay.atEdge = false
                let t0 = Date(); top = messageOne()
                checks.append(String(format: "%.0f ms %@", Date().timeIntervalSince(t0) * 1000, top ? "top" : "not top"))
                if top {
                    ChatReplay.answer = nil
                    CFNotificationCenterPostNotification(center, CFNotificationName("dev.exact.chatreplay.top" as CFString), nil, nil, true)
                    // The probe accepts, or refuses an edge the list has left since.
                    let asked = Date()
                    while ChatReplay.answer == nil, Date().timeIntervalSince(asked) < 3 { spin(0.01) }
                    guard let accepted = ChatReplay.answer else { XCTFail("the probe did not answer the top"); return }
                    if accepted { break }
                    top = false; checks.append("refused")
                }
            }
            next = max(next, Date()).addingTimeInterval(8 * period)
            XCTAssertTrue(Self.play((0..<8).map { flickUp(Double($0) * period) }), "the synthesizer did not play the up flicks")
            spin(0.01)
        }
        print("REPLAY edge checks: \(checks.joined(separator: ", "))")
        XCTAssertTrue(top, "message 1 was not reached in 150 s")
        // The flings start 1 s after the confirmation and 2 s apart, start to start.
        let flingStart = Date().addingTimeInterval(1)
        for (i, v) in flings.enumerated() {
            let at = flingStart.addingTimeInterval(Double(i) * 2)
            if at > Date() { spin(at.timeIntervalSinceNow) }
            let distance = 300.0
            let stroke = Stroke(from: CGPoint(x: x, y: size.height * 0.7), to: CGPoint(x: x, y: size.height * 0.7 - distance), start: 0, travel: distance / v)
            XCTAssertTrue(Self.play([stroke]), "the synthesizer did not play a fling")
        }
        spin(2)
        CFNotificationCenterPostNotification(center, CFNotificationName("dev.exact.chatreplay.done" as CFString), nil, nil, true)
        spin(1)
    }

    static var atEdge = false
    static var answer: Bool?

    private func spin(_ seconds: Double) {
        let end = Date().addingTimeInterval(seconds)
        while Date() < end { RunLoop.current.run(mode: .default, before: min(end, Date().addingTimeInterval(0.01))) }
    }

    struct Stroke { let from: CGPoint; let to: CGPoint; let start: Double; let travel: Double }

    private typealias Alloc = @convention(c) (AnyClass, Selector) -> AnyObject
    private typealias InitPath = @convention(c) (AnyObject, Selector, CGPoint, Double) -> AnyObject
    private typealias Lift = @convention(c) (AnyObject, Selector, Double) -> Void
    private typealias Move = @convention(c) (AnyObject, Selector, CGPoint, Double) -> Void
    private typealias InitRecord = @convention(c) (AnyObject, Selector, NSString, Int) -> AnyObject
    private typealias Add = @convention(c) (AnyObject, Selector, AnyObject) -> Void
    private typealias Get = @convention(c) (AnyObject, Selector) -> AnyObject?
    private typealias Synthesize = @convention(c) (AnyObject, Selector, AnyObject, @escaping @convention(block) (Bool, NSError?) -> Void) -> Void

    private static func imp<T>(_ cls: AnyClass?, _ name: String, _ type: T.Type, classMethod: Bool = false) -> (Selector, T)? {
        let sel = NSSelectorFromString(name)
        guard let cls, let method = classMethod ? class_getClassMethod(cls, sel) : class_getInstanceMethod(cls, sel) else { return nil }
        return (sel, unsafeBitCast(method_getImplementation(method), to: type))
    }

    /// Plays every stroke in one event record (each its own pointer path, at its own start offset) and waits for it.
    static func play(_ strokes: [Stroke]) -> Bool {
        let pathClass: AnyClass? = NSClassFromString("XCPointerEventPath"), recordClass: AnyClass? = NSClassFromString("XCSynthesizedEventRecord")
        guard let alloc = imp(pathClass, "alloc", Alloc.self, classMethod: true), let recordAlloc = imp(recordClass, "alloc", Alloc.self, classMethod: true),
              let initPath = imp(pathClass, "initForTouchAtPoint:offset:", InitPath.self), let moveTo = imp(pathClass, "moveToPoint:atOffset:", Move.self),
              let up = imp(pathClass, "liftUpAtOffset:", Lift.self), let initRecord = imp(recordClass, "initWithName:interfaceOrientation:", InitRecord.self),
              let add = imp(recordClass, "addPointerEventPath:", Add.self), let synthesizer = imp(XCUIDevice.self, "eventSynthesizer", Get.self),
              let device = synthesizer.1(XCUIDevice.shared, synthesizer.0) else { return false }
        let syn = NSSelectorFromString("synthesizeEvent:completion:")
        guard let method = class_getInstanceMethod(object_getClass(device), syn) else { return false }
        let ms = { (s: Double) in (s * 1000).rounded() / 1000 }
        let record = initRecord.1(recordAlloc.1(recordClass!, recordAlloc.0), initRecord.0, "chat-replay" as NSString, UIInterfaceOrientation.portrait.rawValue)
        for s in strokes {
            let path = initPath.1(alloc.1(pathClass!, alloc.0), initPath.0, s.from, ms(s.start))
            moveTo.1(path, moveTo.0, s.to, ms(s.start + s.travel))
            up.1(path, up.0, ms(s.start + s.travel + 0.001))
            add.1(record, add.0, path)
        }
        var result: Bool?
        unsafeBitCast(method_getImplementation(method), to: Synthesize.self)(device, syn, record) { ok, _ in DispatchQueue.main.async { result = ok } }
        let end = Date().addingTimeInterval(5 + (strokes.last.map { $0.start + $0.travel } ?? 0))
        while result == nil, Date() < end { RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.005)) }
        return result == true
    }
}
