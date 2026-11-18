#if os(macOS) || os(iOS)
import XCTest
#if os(macOS)
import AppKit
#else
import UIKit
import UIKit.UIGestureRecognizerSubclass
#endif
@testable import ExactKit

/// LLP 1029.000.002 D7 and D9: `ExactApp.apply` waits while a field
/// composes, a pointer is down or a gesture is in flight, as the platform
/// says and not as Exact's own gestures do. The app keeps the newest candidate as
/// bytes and prepares it when the hold clears, so it carries the state of
/// that moment; its lifecycle hears once how each held candidate ended. The
/// fixture's two plans differ in one label, "before" and "after".
final class ReplacementHoldTests: XCTestCase {
    private final class Life: ExactAppLifecycle {
        var started: [UInt64] = []
        var ended: [ExactReplacementOutcome] = []
        var onEnd: ((UInt64, ExactReplacementOutcome) -> Void)?
        func generationStarted(_ app: ExactApp, token: UInt64) { started.append(token) }
        func firstPixel(_ app: ExactApp, token: UInt64) {}
        func initialGenerationRefused(_ app: ExactApp, token: UInt64, reason: String) {}
        func replacementEnded(_ app: ExactApp, token: UInt64, outcome: ExactReplacementOutcome, reason: String) { ended.append(outcome); onEnd?(token, outcome) }
        func handleCommand(_ name: String, app: ExactApp) -> Bool { false }
    }

    private let app = ExactApp.shared
    private let life = Life()
    private var session: ExactSession!
    private var saved: ExactAppLifecycle?
    #if os(macOS)
    private var window: NSWindow?
    private var hardware: (() -> Int)?
    private var restore: [() -> Void] = []
    #else
    private var window: UIWindow?
    private var touch: UITouch?
    #endif

    override func setUpWithError() throws {
        #if os(macOS)
        _ = NSApplication.shared
        // No button is down, whatever the machine's mouse or an earlier test left.
        hardware = app.replacements.hardwareButtons
        hold(false)
        _ = app.replacements.interactionHold
        #endif
        let before = try fixturePlan("REPLACEMENT_HOLD")
        saved = app.lifecycle
        app.lifecycle = life
        session = app.makeSession(label: "replacement-hold")
        XCTAssertNil(session.boot(plan: before, size: CGSize(width: 320, height: 240)).error)
        #if os(macOS)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 240), styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = ExactView(session: session)
        #else
        let window = UIWindow(frame: CGRect(x: 0, y: 0, width: 320, height: 240))
        window.addSubview(ExactView(session: session))
        window.subviews[0].frame = window.bounds
        window.makeKeyAndVisible()
        window.layoutIfNeeded() // a candidate prepares at the viewport's size
        #endif
        self.window = window
        life.started.removeAll()
    }

    override func tearDown() {
        hold(false)
        session?.destroy()
        app.lifecycle = saved
        #if os(macOS)
        hardware.map { app.replacements.hardwareButtons = $0 }
        restore.forEach { $0() }
        restore = []
        window?.makeFirstResponder(nil)
        window?.close()
        #else
        window?.isHidden = true
        #endif
        window = nil
    }

    private func after() throws -> Data { try fixturePlan("REPLACEMENT_HOLD_AFTER") }
    /// Which plan runs: its label's text, as the runner's tree has it.
    private func label() -> String {
        let tree = session.agent(#"{"op":"tree"}"#)
        return ["before", "after"].first { tree.contains("\"\($0)\"") } ?? tree
    }
    private func draft() throws -> String? {
        let state = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any])
        return (state["slots"] as? [String: Any])?["draft"] as? String
    }

    #if os(macOS)
    /// An input method composing `text`, until the test ends: asked to let
    /// go (`NSTextInputContext.discardMarkedText`), a live one inserts what
    /// it composes into its client, whatever the client has made of its
    /// marked range by then.
    private func liveInputMethod(composing text: String) {
        var composing: String? = text
        let method = class_getInstanceMethod(NSTextInputContext.self, #selector(NSTextInputContext.discardMarkedText))!
        let discard: @convention(block) (NSTextInputContext) -> Void = { context in
            guard let text = composing else { return }
            composing = nil
            context.client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
        }
        let original = method_setImplementation(method, imp_implementationWithBlock(discard))
        restore.append { method_setImplementation(method, original) }
    }
    #endif

    /// A contact down: a mouse button the hardware reports on macOS, a touch UIKit has not ended on iOS.
    private func hold(_ down: Bool) {
        #if os(macOS)
        if down, let press = NSEvent.mouseEvent(with: .leftMouseDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1) {
            app.replacements.hear(press)
        }
        app.replacements.hardwareButtons = { down ? 1 : 0 }
        #else
        if down {
            let touch = UITouch()
            precondition(touch.phase == .began, "a new UITouch is a touch beginning")
            app.replacements.touches.add(touch)
            self.touch = touch
        } else {
            touch.map(app.replacements.touches.remove)
            touch = nil
        }
        #endif
    }

    func testAnApplyWhileAFieldComposesIsHeldAndCommitsWhenTheCompositionEnds() throws {
        let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "draft" })
        #if os(macOS)
        let window = try XCTUnwrap(self.window)
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(node.field)))
        let editor = try XCTUnwrap(window.firstResponder as? NSTextView)
        let none = NSRange(location: NSNotFound, length: 0)
        editor.setMarkedText("にほん", selectedRange: NSRange(location: 3, length: 0), replacementRange: none)
        let commit = { editor.insertText("日本", replacementRange: none) }
        #else
        let field = try XCTUnwrap(node.field)
        XCTAssertTrue(field.becomeFirstResponder())
        field.setMarkedText("にほん", selectedRange: NSRange(location: 3, length: 0))
        node.fieldChanged()
        let commit = { field.insertText("日本"); node.fieldChanged() }
        #endif
        XCTAssertTrue(node.fieldComposing)
        var commits = 0
        let result = app.applyGeneration(ExactGeneration(plan: try after(), assets: app.resolver)) { commits += 1; return true }
        XCTAssertEqual(result, .held("composition"))
        XCTAssertEqual(app.replacements.reason, "composition")
        waitForMainQueue()
        XCTAssertEqual(label(), "before")
        XCTAssertEqual(commits, 0, "nothing is prepared or committed during the hold")
        commit()
        XCTAssertFalse(node.fieldComposing)
        waitFor("the replacement after the composition") { label() == "after" }
        XCTAssertEqual(commits, 1)
        XCTAssertEqual(try draft(), "日本", "the composed text, carried")
        XCTAssertEqual(life.started.count, 1)
        XCTAssertEqual(life.ended, [])
        XCTAssertNil(app.replacements.reason)
    }

    func testAnApplyWhileAPointerIsDownIsHeldUntilItIsUp() throws {
        hold(true)
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        waitForMainQueue()
        XCTAssertEqual(label(), "before")
        hold(false)
        waitFor("the replacement after the release") { label() == "after" }
        XCTAssertEqual(life.started.count, 1)
        XCTAssertEqual(life.ended, [])
    }

    func testAnExplicitReloadEndsALateCompositionBeforeCommit() throws {
        app.endCompositions() // the click, before the connected generation is fetched
        let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "draft" })
        #if os(macOS)
        let window = try XCTUnwrap(self.window)
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(node.field)))
        let editor = try XCTUnwrap(window.firstResponder as? NSTextView)
        editor.setMarkedText("にほん", selectedRange: NSRange(location: 3, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))
        liveInputMethod(composing: "にほん")
        #else
        let field = try XCTUnwrap(node.field)
        XCTAssertTrue(field.becomeFirstResponder())
        field.setMarkedText("にほん", selectedRange: NSRange(location: 3, length: 0))
        node.fieldChanged()
        #endif
        XCTAssertTrue(node.fieldComposing)
        let result = app.applyTogether(try after(), label: "reload", resolver: app.resolver, token: 0, now: true) {
            XCTAssertFalse(node.fieldComposing, "Reload ends late compositions before committing")
            return true
        }
        XCTAssertEqual(result, .applied)
        XCTAssertEqual(try draft(), "にほん")
    }

    func testANewerCandidateSupersedesAHeldOneAndAnInvalidOneIsRefusedOnce() throws {
        hold(true)
        XCTAssertEqual(app.apply(try fixturePlan("REPLACEMENT_HOLD")), .held("pointer"))
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        XCTAssertEqual(life.ended, [.superseded])
        hold(false)
        waitFor("the newest candidate") { label() == "after" }
        XCTAssertEqual(life.started.count, 1, "the superseded one never applies")

        life.started.removeAll()
        hold(true)
        XCTAssertEqual(app.apply(Data("not a plan".utf8)), .held("pointer"))
        hold(false)
        waitFor("the refusal") { life.ended.count == 2 }
        XCTAssertEqual(life.ended, [.superseded, .refused])
        waitForMainQueue()
        waitForMainQueue()
        XCTAssertEqual(life.ended.count, 2, "a refused candidate is not tried again")
        XCTAssertEqual(life.started, [])
        XCTAssertEqual(label(), "after")
    }

    /// One queue keeps the candidate that waits, whichever producer brought
    /// it: a generation with a durable commit, as an update's, and a plain
    /// apply supersede each other.
    func testTheNewestCandidateWinsWhicheverProducerBroughtIt() throws {
        var commits = 0
        let before = try fixturePlan("REPLACEMENT_HOLD")
        let update = { self.app.applyGeneration(ExactGeneration(plan: before, assets: self.app.resolver)) { commits += 1; return true } }
        hold(true)
        XCTAssertEqual(update(), .held("pointer"))
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        XCTAssertEqual(life.ended, [.superseded])
        XCTAssertEqual(update(), .held("pointer"))
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        XCTAssertEqual(life.ended, [.superseded, .superseded, .superseded])
        hold(false)
        waitFor("the newest candidate") { label() == "after" }
        waitForMainQueue()
        XCTAssertEqual(commits, 0, "a superseded generation never commits")
        XCTAssertEqual(life.started.count, 1)
    }

    /// One attempt asks the interaction and the candidate's own holds
    /// together: a request that began while a press was waited out holds
    /// the candidate, which applies once a change ends that hold too.
    func testARequestThatBeganDuringAPressHoldsTheCandidateThatWaitedThePressOut() throws {
        var requests = false, attempts = 0, completion: ReplacementCompletion?
        hold(true)
        let result = app.replacements.submit(token: 0, attempt: {
            attempts += 1
            return requests ? .held("requests") : .applied
        }, finished: { completion = $0 })
        XCTAssertEqual(result, .held("pointer"))
        requests = true // the press sends one
        hold(false)
        waitFor("the attempt after the press") { attempts == 1 }
        waitForMainQueue()
        XCTAssertEqual(app.replacements.reason, "requests")
        XCTAssertNil(completion, "kept, not refused")
        requests = false
        app.replacements.changed() // its reply
        waitFor("the candidate applies") { completion == .applied }
        XCTAssertEqual(attempts, 2)
    }

    func testLifecycleCallbacksCanSubmitANewerCandidate() throws {
        let bytes = try after()
        var commits: [UInt64] = [], ended: [UInt64] = []
        let submit: (UInt64) -> ExactApplyResult = { number in
            self.app.applyGeneration(ExactGeneration(plan: bytes, assets: self.app.resolver)) {
                commits.append(number)
                return true
            }
        }
        hold(true)
        life.onEnd = { _, outcome in
            ended.append(UInt64(ended.count + 1))
            XCTAssertEqual(outcome, .superseded)
            if ended.count == 1 { XCTAssertEqual(submit(3), .held("pointer")) }
        }
        XCTAssertEqual(submit(1), .held("pointer"))
        _ = submit(2)
        XCTAssertEqual(ended, [1, 2], "the callback's newer candidate supersedes the outer submission")
        hold(false)
        waitFor("the reentrant candidate") { !commits.isEmpty }
        XCTAssertEqual(commits, [3])
        XCTAssertEqual(life.started, [0])

        life.onEnd = nil
        var finished: [ReplacementCompletion] = []
        hold(true)
        // An attempt may call back before returning; its completion cannot
        // remove the candidate the callback submitted.
        XCTAssertEqual(app.replacements.submit(token: 0, attempt: {
            self.hold(true)
            XCTAssertEqual(submit(5), .held("pointer"))
            return .applied
        }, finished: { finished.append($0) }), .held("pointer"))
        hold(false)
        waitFor("the callback after applying") { !finished.isEmpty }
        XCTAssertEqual(finished, [.applied])
        XCTAssertEqual(app.replacements.reason, "pointer", "completing the older attempt must keep the newer one")
        hold(false)
        waitFor("the candidate submitted after applying") { commits.contains(5) }
        XCTAssertEqual(commits, [3, 5])
        XCTAssertEqual(life.ended, [.superseded, .superseded])
    }

    #if os(macOS)
    func testAnExternalApplySupersedesAHeldURLGenerationWithoutARefusal() throws {
        let server = try DevServerStub()
        defer { server.stop() }
        let program = String(repeating: "a", count: 64)
        try server.start(program: program, plan: fixturePlan("REPLACEMENT_HOLD"))
        app.connect(server.page)
        defer { app.disconnect() }
        waitFor("the connected plan") { !self.life.started.isEmpty }
        hold(true)
        _ = server.publish(program: program, plan: try after())
        waitFor("the URL candidate held") { self.app.replacements.reason == "pointer" }
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        XCTAssertEqual(life.ended, [.superseded])
        XCTAssertNil(app.connection?.refused, "supersession is not a failed preparation")
        hold(false)
        waitFor("the external replacement") { label() == "after" }
        XCTAssertNil(app.connection?.refused)

        // Supersession may call the producer before its submit returns.
        hold(true)
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        let bytes = try after()
        var inserted = false
        life.onEnd = { _, outcome in
            guard !inserted else { return }
            inserted = true
            XCTAssertEqual(outcome, .superseded)
            XCTAssertEqual(self.app.apply(bytes), .held("pointer"))
        }
        defer { life.onEnd = nil }
        _ = server.publish(program: program, plan: try fixturePlan("REPLACEMENT_HOLD"))
        waitFor("the reentrant supersession") { inserted }
        XCTAssertNil(app.connection?.refused)
        hold(false)
        waitFor("the newest candidate") { self.app.replacements.reason == nil }
        XCTAssertEqual(label(), "after")
    }

    /// A held URL generation goes with the connection that brought it, when
    /// another `connect` replaces it or `disconnect` closes it; a candidate
    /// another producer brought stays.
    func testAHeldURLGenerationGoesWithItsConnection() throws {
        let server = try DevServerStub()
        defer { server.stop() }
        let program = String(repeating: "a", count: 64)
        try server.start(program: program, plan: fixturePlan("REPLACEMENT_HOLD"))
        app.connect(server.page)
        defer { app.disconnect() }
        waitFor("the connected plan") { !self.life.started.isEmpty }

        hold(true)
        _ = server.publish(program: program, plan: try after())
        waitFor("the URL candidate held") { self.app.replacements.reason == "pointer" }
        app.connect(server.page)
        XCTAssertEqual(life.ended, [.dropped], "replaced")
        hold(false)
        waitFor("the new connection's generation") { self.label() == "after" }

        hold(true)
        XCTAssertEqual(app.apply(try fixturePlan("REPLACEMENT_HOLD")), .held("pointer"))
        app.disconnect()
        XCTAssertEqual(app.replacements.reason, "pointer", "another producer's candidate stays")
        XCTAssertEqual(life.ended, [.dropped])
        hold(false)
        waitFor("the other producer's candidate") { self.label() == "before" }

        let applied = life.started.count
        app.connect(server.page)
        waitFor("the reconnected plan") { self.life.started.count > applied && self.label() == "after" }
        hold(true)
        _ = server.publish(program: program, plan: try fixturePlan("REPLACEMENT_HOLD"))
        waitFor("the URL candidate held") { self.app.replacements.reason == "pointer" }
        app.disconnect()
        XCTAssertEqual(life.ended, [.dropped, .dropped], "closed")
        XCTAssertNil(app.replacements.reason)
        hold(false)
        waitForMainQueue()
        XCTAssertEqual(label(), "after")

        // The lifecycle that hears the drop may connect again, and keeps that connection.
        app.connect(server.page)
        waitFor("the reconnected plan") { self.label() == "before" }
        hold(true)
        _ = server.publish(program: program, plan: try after())
        waitFor("the URL candidate held") { self.app.replacements.reason == "pointer" }
        life.onEnd = { _, _ in self.app.connect(server.page) }
        defer { life.onEnd = nil }
        app.disconnect()
        XCTAssertEqual(life.ended.last, .dropped)
        XCTAssertNotNil(app.connectedPage, "the connection the callback made")
    }

    func testTheDevMenusConnectedReloadCommitsWhileAPointerIsDown() throws {
        let server = try DevServerStub()
        defer { server.stop() }
        try server.start(program: String(repeating: "a", count: 64), plan: fixturePlan("REPLACEMENT_HOLD"))
        app.connect(server.page)
        defer { app.disconnect() }
        waitFor("the connected plan") { !self.life.started.isEmpty }
        hold(true)
        _ = server.publish(program: String(repeating: "a", count: 64), plan: try after())
        waitFor("the background save held by the pointer") { self.app.replacements.reason == "pointer" }
        let savedSession = DevMenu.session
        defer { DevMenu.session = savedSession }
        DevMenu.session = session
        DevMenu.reload()
        waitFor("the menu reload while the pointer remains down") { label() == "after" }
        XCTAssertNil(app.replacements.reason)
        XCTAssertEqual(life.ended, [.superseded])

        hold(true)
        _ = server.publish(program: String(repeating: "a", count: 64), plan: try fixturePlan("REPLACEMENT_HOLD"))
        waitFor("the next pushed generation still waits") { self.app.replacements.reason == "pointer" }
        XCTAssertEqual(label(), "after")
        hold(false)
        waitFor("the pushed generation after release") { label() == "before" }
    }

    /// The dev menu's Reload is the person's own act: it ends the composition
    /// as a click elsewhere does, keeping the marked text, and commits now.
    func testTheDevMenusReloadEndsACompositionAndCommitsAtOnce() throws {
        let node = try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == "draft" })
        let window = try XCTUnwrap(self.window)
        XCTAssertTrue(window.makeFirstResponder(try XCTUnwrap(node.field)))
        let editor = try XCTUnwrap(window.firstResponder as? NSTextView)
        editor.setMarkedText("にほん", selectedRange: NSRange(location: 3, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))
        liveInputMethod(composing: "にほん")
        XCTAssertEqual(app.apply(try after()), .held("composition"), "a save in the background waits")

        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("replacement-hold-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let plan = dir.appendingPathComponent("app.plan")
        try after().write(to: plan)
        let (savedSession, savedPath) = (DevMenu.session, DevMenu.planPath)
        defer { DevMenu.session = savedSession; DevMenu.planPath = savedPath }
        DevMenu.session = session
        DevMenu.planPath = plan.path
        DevMenu.reload()

        XCTAssertFalse(editor.hasMarkedText())
        XCTAssertEqual(label(), "after", "committed in the reload's own turn")
        XCTAssertEqual(try draft(), "にほん", "the marked text, kept once and carried")
        XCTAssertEqual(life.ended, [.superseded])
        XCTAssertEqual(life.started.count, 1)
        XCTAssertNil(app.replacements.reason)
    }
    #endif

    /// A recognizer or a scroll view in the session's window that says the
    /// person still holds it, whatever became of the touch or the press.
    #if os(macOS)
    private final class Recognizing: NSGestureRecognizer {
        var held = true
        override var state: NSGestureRecognizer.State { get { held ? .changed : .possible } set {} }
    }
    #else
    private final class Recognizing: UIGestureRecognizer {
        var held = true
        override var state: UIGestureRecognizer.State { get { held ? .changed : .possible } set {} }
    }
    private final class Tracking: UIScrollView {
        var held = true
        override var isTracking: Bool { held }
    }
    #endif

    func testARecognizedGestureInTheWindowHoldsAReplacement() throws {
        let window = try XCTUnwrap(self.window)
        let gesture = Recognizing()
        #if os(macOS)
        try XCTUnwrap(window.contentView).addGestureRecognizer(gesture)
        #else
        window.addGestureRecognizer(gesture)
        #endif
        XCTAssertEqual(app.apply(try after()), .held("gesture"))
        waitForMainQueue()
        XCTAssertEqual(label(), "before")
        gesture.held = false
        waitFor("the replacement after the gesture") { label() == "after" }
        XCTAssertEqual(life.ended, [])
    }

    #if os(iOS)
    /// UIKit cancels a view's touch when a scroll view's pan takes it, with
    /// the finger still down and the scroll view tracking it.
    func testAScrollViewStillTrackingHoldsAReplacementAfterItsTouchIsCancelled() throws {
        let scroll = Tracking(frame: CGRect(x: 0, y: 0, width: 40, height: 40))
        try XCTUnwrap(window).addSubview(scroll)
        defer { scroll.removeFromSuperview() }
        XCTAssertEqual(app.apply(try after()), .held("gesture"))
        waitForMainQueue()
        XCTAssertEqual(label(), "before")
        scroll.held = false
        waitFor("the replacement after the scroll view lets go") { label() == "after" }
    }

    private final class Press: UIPress {
        var now = UIPress.Phase.began
        override var phase: UIPress.Phase { now }
    }
    private final class Presses: UIPressesEvent {
        var all = Set<UIPress>()
        override var allPresses: Set<UIPress> { all }
    }

    /// A remote's or a controller's button is a press with no touch
    /// (`UIPressesEvent`; tvOS's Select). It holds a replacement until it
    /// ends, heard at the app's window and, in an embedder's window, by the
    /// session view's recognizer.
    func testAButtonPressWithNoTouchHoldsAReplacement() throws {
        let recognizers = session.presenter.viewport.gestureRecognizers ?? []
        for (embedded, plan, shows) in [(false, try after(), "after"), (true, try fixturePlan("REPLACEMENT_HOLD"), "before")] {
            let press = Press(), event = Presses()
            event.all = [press]
            let deliver = { (began: Bool) in
                guard embedded else { return self.app.replacements.receive(event) }
                for recognizer in recognizers { began ? recognizer.pressesBegan([press], with: event) : recognizer.pressesEnded([press], with: event) }
            }
            deliver(true)
            XCTAssertEqual(app.apply(plan), .held("pointer"), "embedded: \(embedded)")
            waitForMainQueue()
            XCTAssertNotEqual(label(), shows)
            press.now = .ended
            deliver(false)
            waitFor("the replacement after the press") { self.label() == shows }
        }
    }
    #endif

    #if os(macOS)
    /// A button whose press this app's events never brought went down in another app.
    func testAnotherAppsPressHoldsNothing() throws {
        app.replacements.hardwareButtons = { 1 }
        XCTAssertEqual(app.apply(try after()), .applied)
    }

    /// A control's tracking loop takes its press's release. Once the run
    /// loop is the app's again the press is over, and a button down after
    /// that went down in another app: its release would turn no loop here.
    func testAPressWhoseReleaseALoopTookIsOver() throws {
        let press = try XCTUnwrap(NSEvent.mouseEvent(with: .leftMouseDown, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        app.replacements.hear(press)
        waitForMainQueue()
        app.replacements.hardwareButtons = { 1 }
        XCTAssertEqual(app.apply(try after()), .applied)
    }

    /// A trackpad's fingers, as AppKit phases their scroll events (CGScrollPhase:
    /// began 1, changed 2, ended 4), through the application as a hand's come.
    private func fingers(_ phase: Int64) throws {
        let cg = try XCTUnwrap(CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2, wheel1: -4, wheel2: 0, wheel3: 0))
        cg.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
        cg.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase)
        NSApp.sendEvent(try XCTUnwrap(NSEvent(cgEvent: cg)))
    }

    /// A pan or a pinch on the trackpad presses no button and makes no Exact
    /// press: the fingers hold the replacement until they lift.
    func testATrackpadGestureHoldsAReplacementUntilTheFingersLift() throws {
        try fingers(1)
        XCTAssertEqual(app.apply(try after()), .held("gesture"))
        try fingers(2)
        waitForMainQueue()
        XCTAssertEqual(label(), "before")
        try fingers(4)
        waitFor("the replacement after the lift") { label() == "after" }
        XCTAssertEqual(life.ended, [])
    }

    /// Leaving the app does not lift the fingers: a gesture down as the app
    /// resigned active holds until the app hears the next gesture, which
    /// begins after they lifted.
    func testAGestureDownAsTheAppResignsHoldsUntilTheNextGesture() throws {
        try fingers(1)
        XCTAssertEqual(app.apply(try after()), .held("gesture"))
        NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)
        XCTAssertEqual(app.replacements.interactionHold, "gesture")
        RunLoop.main.run(until: Date().addingTimeInterval(0.1)) // turns enough to try the candidate
        XCTAssertEqual(label(), "before")
        try fingers(128) // the next gesture's first event: resting fingers (`mayBegin`)
        XCTAssertEqual(app.replacements.reason, "gesture", "the new gesture's fingers")
        try fingers(4)
        waitFor("the replacement after the new gesture") { label() == "after" }
    }

    /// AppKit runs a menu, a control's press and a scroller's drag in a loop
    /// of its own, whose events no view of the session sees.
    func testATrackingLoopHoldsAReplacement() throws {
        let bytes = try after()
        var result: ExactApplyResult?
        RunLoop.main.perform(inModes: [.eventTracking]) { result = self.app.apply(bytes) }
        let until = Date().addingTimeInterval(10)
        while result == nil, Date() < until { RunLoop.main.run(mode: .eventTracking, before: Date().addingTimeInterval(0.005)) }
        XCTAssertEqual(result, .held("gesture"))
        XCTAssertEqual(label(), "before")
        waitFor("the replacement after the loop") { label() == "after" }
    }
    #endif

    func testDestroyingItsSessionDropsAHeldCandidate() throws {
        hold(true)
        XCTAssertEqual(app.apply(try after()), .held("pointer"))
        session.destroy()
        XCTAssertEqual(life.ended, [.dropped])
        XCTAssertNil(app.replacements.reason)
    }
}
#endif
